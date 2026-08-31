//! Place randomizer ground groups on dry open terrain inside one coalition's AO.

use geo::{Contains, Point};

use crate::frontlines::{densify, AOI_GAP};
use crate::geo::{MAP_MAX, MAP_MIN};
use crate::mapclip::{
    apply_salients, filter_front_band, influence_minus_salients, point_north_of_front, WorldAabb,
};
use crate::placement::{
    hashed_pick, heading_toward, heading_toward_nearest, subsample_spaced, PlaceOpts,
    UNIT_PLACE_SPACING,
};
use crate::mapnet::{self, NetworkSpot, RouteLayout};
use crate::weapon_range::UNKNOWN_ARTILLERY_M;
use crate::watermap::TerrainMap;

pub const MAX_GROUND: usize = 64;
pub const GROUND_SPACING: f64 = UNIT_PLACE_SPACING;
pub const START_DELAY_S: f64 = 5.0;
pub const GROUP_DELAY_S: f64 = 0.5;
/// Fallback when a template has no known gun script.
pub const ARTY_OBJECTIVE_RADIUS: f64 = UNKNOWN_ARTILLERY_M;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundKind {
    Armor,
    Supply,
    Artillery,
    Train,
}

impl GroundKind {
    pub fn label(self) -> &'static str {
        match self {
            GroundKind::Armor => "Armor",
            GroundKind::Supply => "Supply",
            GroundKind::Artillery => "Artillery",
            GroundKind::Train => "Train",
        }
    }
}

/// One group to park. `range_m` is max distance to that copy's hashed objective.
/// `route` parks trains on rails and perfect columns on roads instead.
#[derive(Clone, Debug)]
pub struct GroundJob {
    pub kind: GroundKind,
    pub range_m: Option<f64>,
    pub route: Option<RouteLayout>,
}

impl GroundJob {
    pub fn new(kind: GroundKind, range_m: Option<f64>) -> Self {
        Self {
            kind,
            range_m,
            route: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroundSpot {
    pub x: f64,
    pub z: f64,
    pub in_ao: bool,
    /// Degrees, 0 = north (+X), 90 = east (+Z). Templates are authored pointing north.
    pub heading_deg: f64,
    pub kind: GroundKind,
    /// Hashed objective this group fires on (AttackArea is moved here).
    pub objective: Option<(f64, f64)>,
    pub network: Option<NetworkSpot>,
    /// Soft placement problem for this group (shown with its preview number).
    pub issue: Option<String>,
}

impl GroundSpot {
    pub fn at(
        x: f64,
        z: f64,
        in_ao: bool,
        heading_deg: f64,
        kind: GroundKind,
        objective: Option<(f64, f64)>,
    ) -> Self {
        Self {
            x,
            z,
            in_ao,
            heading_deg,
            kind,
            objective,
            network: None,
            issue: None,
        }
    }

    pub fn on_network(&self) -> bool {
        self.network.is_some()
    }

    pub fn apply_sampled(&mut self, pose: Option<(f64, f64, f64)>) {
        if let Some((x, z, heading)) = pose {
            self.x = x;
            self.z = z;
            self.heading_deg = heading;
        }
    }
}

/// Preview-numbered soft problems for the status list (1-based, same as map labels).
pub fn numbered_ground_issues(spots: &[GroundSpot], objectives_empty: bool) -> Vec<String> {
    spots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let mut parts = Vec::new();
            if let Some(iss) = &s.issue {
                parts.push(iss.clone());
            }
            if !s.in_ao {
                parts.push("parked outside the AO.".into());
            }
            if objectives_empty && !s.on_network() {
                parts.push("no objective to aim at.".into());
            }
            if parts.is_empty() {
                None
            } else {
                Some(format!("{} {}: {}", i + 1, s.kind.label(), parts.join(" ")))
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct MapGroundLayout {
    pub eastern: bool,
    pub spots: Vec<GroundSpot>,
    /// Soft placement notes (range disk missed; parked on closest open ground or the front).
    pub warnings: Vec<String>,
}

impl MapGroundLayout {
    /// Face each spot at the nearest objective. Headings stay north if `objectives` is empty.
    pub fn aim_at_objectives(&mut self, objectives: &[(f64, f64)]) {
        if objectives.is_empty() {
            return;
        }
        for spot in &mut self.spots {
            if spot.on_network() {
                continue;
            }
            if let Some(h) = heading_toward_nearest((spot.x, spot.z), objectives) {
                spot.heading_deg = h;
                if spot.objective.is_none() {
                    if let Some(&p) = objectives.iter().min_by(|a, b| {
                        let da = (a.0 - spot.x).hypot(a.1 - spot.z);
                        let db = (b.0 - spot.x).hypot(b.1 - spot.z);
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    }) {
                        spot.objective = Some(p);
                    }
                }
            }
        }
    }

    /// Face each spot at a hashed objective (stable per index + seed).
    pub fn aim_at_hashed_objectives(&mut self, objectives: &[(f64, f64)], seed: u64) {
        if objectives.is_empty() {
            return;
        }
        for (i, spot) in self.spots.iter_mut().enumerate() {
            if spot.on_network() {
                continue;
            }
            if let Some(&t) = hashed_pick(objectives, seed, i as u64) {
                spot.heading_deg = heading_toward((spot.x, spot.z), t);
                spot.objective = Some(t);
            }
        }
    }
}

/// Grid ground groups on dry open land inside the coalition AO. Leftovers go to
/// the nearest friendly open ground outside the box. `kind_counts` is
/// Armor / Supply / Artillery. When objectives are marked, artillery is parked
/// within [`ARTY_OBJECTIVE_RADIUS`] of the nearest one instead of the front band.
pub fn place_ground(
    eastern: bool,
    kind_counts: [usize; 3],
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    salients: &[Vec<(f64, f64)>],
    stretch_east: bool,
    terrain: &TerrainMap,
    opts: PlaceOpts<'_>,
) -> Result<MapGroundLayout, String> {
    let constrain_arty = kind_counts[2] > 0 && !opts.favor.is_empty();
    let mut jobs = Vec::new();
    if constrain_arty {
        jobs.extend(std::iter::repeat(GroundJob::new(
            GroundKind::Artillery,
            Some(ARTY_OBJECTIVE_RADIUS),
        )).take(kind_counts[2]));
        jobs.extend(std::iter::repeat(GroundJob::new(
            GroundKind::Armor,
            None,
        )).take(kind_counts[0]));
        jobs.extend(std::iter::repeat(GroundJob::new(
            GroundKind::Supply,
            None,
        )).take(kind_counts[1]));
    } else {
        jobs.extend(std::iter::repeat(GroundJob::new(
            GroundKind::Armor,
            None,
        )).take(kind_counts[0]));
        jobs.extend(std::iter::repeat(GroundJob::new(
            GroundKind::Supply,
            None,
        )).take(kind_counts[1]));
        jobs.extend(std::iter::repeat(GroundJob::new(
            GroundKind::Artillery,
            None,
        )).take(kind_counts[2]));
    }
    place_ground_jobs(
        eastern,
        &jobs,
        front_xz,
        aabb,
        salients,
        stretch_east,
        terrain,
        opts,
    )
}

/// Park each job: within `range_m` of its hashed objective, or on the front band.
pub fn place_ground_jobs(
    eastern: bool,
    jobs: &[GroundJob],
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    salients: &[Vec<(f64, f64)>],
    stretch_east: bool,
    terrain: &TerrainMap,
    opts: PlaceOpts<'_>,
) -> Result<MapGroundLayout, String> {
    let side = coalition_land(eastern, front_xz, aabb, salients, stretch_east);
    let take = jobs.len().min(MAX_GROUND);
    if take == 0 {
        return Err(
            "no open ground on that coalition's side of the front. Enlarge the AO or pick the other coalition."
                .into(),
        );
    }
    let mut spots = Vec::new();
    let mut used: Vec<(f64, f64)> = opts.occupied.to_vec();
    let mut warnings = Vec::new();

    for (i, job) in jobs.iter().take(take).enumerate() {
        let mut pending_issue: Option<String> = None;
        if let Some(route) = &job.route {
            let prefer_side = |x: f64, z: f64| aabb.contains(x, z) && allowed(&side, x, z);
            let placed = mapnet::place_route(route, aabb, opts.seed, i, &used, prefer_side);
            let (placed, issue) = match placed {
                Some(p) => (Some(p), None),
                None => {
                    let in_ao = |x: f64, z: f64| aabb.contains(x, z);
                    match mapnet::place_route(
                        route,
                        aabb,
                        opts.seed.wrapping_add(17),
                        i,
                        &used,
                        in_ao,
                    ) {
                        Some(p) => (
                            Some(p),
                            Some(format!(
                                "no {} on this side of the front — parked on a track in the AO.",
                                if route.rail { "railroad" } else { "road" },
                            )),
                        ),
                        None if route.rail => {
                            warnings.push(format!(
                                "{}: no railroad long enough inside the AO — skipped.",
                                job.kind.label(),
                            ));
                            (None, None)
                        }
                        None => {
                            (
                                None,
                                Some(
                                    "no road long enough inside the AO — parked on open ground."
                                        .into(),
                                ),
                            )
                        }
                    }
                }
            };
            if let Some(((x, z, heading_deg, in_ao), network)) = placed {
                used.push((x, z));
                spots.push(GroundSpot {
                    x,
                    z,
                    in_ao,
                    heading_deg,
                    kind: job.kind,
                    objective: None,
                    network: Some(network),
                    issue,
                });
                continue;
            }
            if route.rail {
                continue;
            }
            pending_issue = issue;
        }
        let assigned = hashed_pick(opts.favor, opts.seed, i as u64).copied();
        let ranged = job.range_m.filter(|r| *r > 0.0).zip(assigned);
        let picked = if let Some((radius, obj)) = ranged {
            let spacing = cluster_spacing(radius);
            let near = pick_near_objectives(
                1,
                &side,
                aabb,
                terrain,
                &[obj],
                radius,
                opts.seed.wrapping_add(i as u64),
                &used,
                spacing,
            );
            if let Some(p) = near.into_iter().next() {
                Some(p)
            } else if let Some(p) = pick_closest_open(obj, &side, aabb, terrain, &used, spacing) {
                let d = (p.0 - obj.0).hypot(p.1 - obj.1);
                pending_issue = Some(format!(
                    "no open ground within {:.1} km of an objective — parked {:.1} km away.",
                    radius / 1000.0,
                    d / 1000.0,
                ));
                Some(p)
            } else if let Some(p) = pick_front_open(
                1,
                &side,
                front_xz,
                aabb,
                terrain,
                opts,
                &used,
                spacing,
            )
            .into_iter()
            .next()
            {
                pending_issue = Some("no open ground near an objective — parked along the front.".into());
                Some(p)
            } else {
                None
            }
        } else {
            pick_front_open(
                1,
                &side,
                front_xz,
                aabb,
                terrain,
                opts,
                &used,
                UNIT_PLACE_SPACING,
            )
            .into_iter()
            .next()
        };
        let Some((x, z, in_ao)) = picked else {
            if spots.is_empty() {
                return Err(
                    "no open ground on that coalition's side of the front. Enlarge the AO or pick the other coalition."
                        .into(),
                );
            }
            break;
        };
        used.push((x, z));
        let heading_deg = assigned
            .map(|t| heading_toward((x, z), t))
            .unwrap_or(0.0);
        let mut spot = GroundSpot::at(
            x,
            z,
            in_ao,
            heading_deg,
            job.kind,
            assigned,
        );
        spot.issue = pending_issue;
        spots.push(spot);
    }

    if spots.is_empty() {
        if jobs.iter().any(|j| j.route.as_ref().is_some_and(|r| r.rail)) {
            return Err(
                "no railroad in the AO for those train groups. Enlarge the box or pick another area."
                    .into(),
            );
        }
        return Err(
            "no open ground on that coalition's side of the front. Enlarge the AO or pick the other coalition."
                .into(),
        );
    }
    warnings.extend(numbered_ground_issues(&spots, false));
    Ok(MapGroundLayout {
        eastern,
        spots,
        warnings,
    })
}

/// Keep short-range groups (tanks, MGs) from demanding a 4.5 km gap inside a 1.5 km disk.
fn cluster_spacing(radius: f64) -> f64 {
    (radius * 0.6).clamp(350.0, UNIT_PLACE_SPACING)
}

fn pick_front_open(
    total: usize,
    side: &LandSide,
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    terrain: &TerrainMap,
    opts: PlaceOpts<'_>,
    used: &[(f64, f64)],
    spacing: f64,
) -> Vec<(f64, f64, bool)> {
    let mut inside = sample_open(terrain, aabb, GROUND_SPACING, side);
    inside = filter_front_band(inside, front_xz, opts.front_band);
    inside = drop_near(inside, used, spacing);
    for step in [2_000.0, 1_000.0, 500.0] {
        if inside.len() >= total {
            break;
        }
        inside = sample_open(terrain, aabb, step, side);
        inside = filter_front_band(inside, front_xz, opts.front_band);
        inside = drop_near(inside, used, spacing);
    }
    let take = total.min(inside.len());
    let mut points: Vec<(f64, f64, bool)> = subsample_spaced(
        inside,
        take,
        opts.favor,
        opts.seed,
        spacing,
        used,
    )
        .into_iter()
        .map(|(x, z)| (x, z, true))
        .collect();

    if points.len() < total {
        let outside_box = WorldAabb::full_map();
        let mut outside = sample_open(terrain, outside_box, GROUND_SPACING, side);
        outside.retain(|(x, z)| !aabb.contains(*x, *z));
        outside = filter_front_band(outside, front_xz, opts.front_band);
        outside = drop_near(outside, used, spacing);
        for step in [2_000.0, 1_000.0, 500.0] {
            if outside.len() >= total - points.len() {
                break;
            }
            outside = sample_open(terrain, outside_box, step, side);
            outside.retain(|(x, z)| !aabb.contains(*x, *z));
            outside = filter_front_band(outside, front_xz, opts.front_band);
            outside = drop_near(outside, used, spacing);
        }
        let mut blocked = used.to_vec();
        blocked.extend(points.iter().map(|(x, z, _)| (*x, *z)));
        let need = total - points.len();
        let extra = subsample_spaced(
            outside,
            need,
            opts.favor,
            opts.seed.wrapping_add(1),
            spacing,
            &blocked,
        );
        for (x, z) in extra {
            points.push((x, z, false));
        }
    }
    points
}

fn pick_near_objectives(
    n: usize,
    side: &LandSide,
    aabb: WorldAabb,
    terrain: &TerrainMap,
    objectives: &[(f64, f64)],
    radius: f64,
    seed: u64,
    occupied: &[(f64, f64)],
    spacing: f64,
) -> Vec<(f64, f64, bool)> {
    if n == 0 || objectives.is_empty() {
        return Vec::new();
    }
    let step0 = (radius / 4.0).clamp(250.0, 1_000.0);
    let mut cand = sample_open_near(terrain, step0, side, objectives, radius);
    cand = drop_near(cand, occupied, spacing);
    for step in [500.0, 250.0] {
        if cand.len() >= n {
            break;
        }
        cand = sample_open_near(terrain, step, side, objectives, radius);
        cand = drop_near(cand, occupied, spacing);
    }
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    for p in cand {
        if aabb.contains(p.0, p.1) {
            inside.push(p);
        } else {
            outside.push(p);
        }
    }
    let mut points: Vec<(f64, f64, bool)> = subsample_spaced(
        inside,
        n,
        objectives,
        seed,
        spacing,
        occupied,
    )
        .into_iter()
        .map(|(x, z)| (x, z, true))
        .collect();
    if points.len() < n {
        let mut blocked = occupied.to_vec();
        blocked.extend(points.iter().map(|(x, z, _)| (*x, *z)));
        let need = n - points.len();
        for (x, z) in subsample_spaced(
            outside,
            need,
            objectives,
            seed.wrapping_add(3),
            spacing,
            &blocked,
        ) {
            points.push((x, z, false));
        }
    }
    points
}

fn pick_closest_open(
    obj: (f64, f64),
    side: &LandSide,
    aabb: WorldAabb,
    terrain: &TerrainMap,
    occupied: &[(f64, f64)],
    spacing: f64,
) -> Option<(f64, f64, bool)> {
    let closest = |pts: Vec<(f64, f64)>| {
        pts.into_iter()
            .min_by(|a, b| {
                dist2(a.0, a.1, obj.0, obj.1)
                    .partial_cmp(&dist2(b.0, b.1, obj.0, obj.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(x, z)| (x, z, aabb.contains(x, z)))
    };
    for radius in [2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0] {
        let mut cand = sample_open_near(terrain, 250.0, side, &[obj], radius);
        cand = drop_near(cand, occupied, spacing);
        if let Some(p) = closest(cand) {
            return Some(p);
        }
    }
    let mut cand = sample_open(terrain, aabb, 500.0, side);
    cand = drop_near(cand, occupied, spacing);
    if let Some(p) = closest(cand) {
        return Some(p);
    }
    let mut cand = sample_open(terrain, WorldAabb::full_map(), 1_000.0, side);
    cand = drop_near(cand, occupied, spacing);
    closest(cand)
}

fn sample_open_near(
    terrain: &TerrainMap,
    step: f64,
    side: &LandSide,
    objectives: &[(f64, f64)],
    radius: f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(ox, oz) in objectives {
        let around = WorldAabb::from_corners(ox - radius, oz - radius, ox + radius, oz + radius);
        out.extend(sample_open(terrain, around, step, side));
    }
    let r2 = radius * radius;
    out.retain(|&(x, z)| {
        objectives
            .iter()
            .any(|&(ox, oz)| dist2(x, z, ox, oz) <= r2)
    });
    out.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    out.dedup();
    out
}

fn drop_near(pts: Vec<(f64, f64)>, used: &[(f64, f64)], min_d: f64) -> Vec<(f64, f64)> {
    if used.is_empty() {
        return pts;
    }
    let m2 = min_d * min_d;
    pts.into_iter()
        .filter(|&(x, z)| used.iter().all(|&(ux, uz)| dist2(x, z, ux, uz) >= m2))
        .collect()
}

enum LandSide {
    Any,
    Polygon(geo::MultiPolygon<f64>),
    Front { eastern: bool, front: Vec<(f64, f64)> },
}

fn coalition_land(
    eastern: bool,
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    salients: &[Vec<(f64, f64)>],
    stretch_east: bool,
) -> LandSide {
    let dense = densify(front_xz, 4_000.0);
    if dense.len() < 2 {
        return LandSide::Any;
    }
    let (_, patches) = apply_salients(dense.clone(), salients);
    if let Some((north, south)) =
        influence_minus_salients(&dense, aabb, AOI_GAP, true, stretch_east, &patches)
    {
        LandSide::Polygon(if eastern { north } else { south })
    } else {
        LandSide::Front {
            eastern,
            front: dense,
        }
    }
}

fn allowed(side: &LandSide, x: f64, z: f64) -> bool {
    match side {
        LandSide::Any => true,
        LandSide::Polygon(mp) => mp.contains(&Point::new(x, z)),
        LandSide::Front { eastern, front } => point_north_of_front(front, x, z) == *eastern,
    }
}

fn sample_open(
    terrain: &TerrainMap,
    aabb: WorldAabb,
    step: f64,
    side: &LandSide,
) -> Vec<(f64, f64)> {
    let step = step.max(250.0);
    let x0 = aabb.x_min.max(MAP_MIN) + step * 0.5;
    let z0 = aabb.z_min.max(MAP_MIN) + step * 0.5;
    let x1 = aabb.x_max.min(MAP_MAX);
    let z1 = aabb.z_max.min(MAP_MAX);
    let mut out = Vec::new();
    let mut x = x0;
    while x < x1 {
        let mut z = z0;
        while z < z1 {
            if terrain.is_open_xz(x, z) && allowed(side, x, z) {
                out.push((x, z));
            }
            z += step;
        }
        x += step;
    }
    out
}

fn dist2(x0: f64, z0: f64, x1: f64, z1: f64) -> f64 {
    let dx = x0 - x1;
    let dz = z0 - z1;
    dx * dx + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_map(width: u32, cells: Vec<u8>) -> TerrainMap {
        let mut bytes = Vec::from(*b"WMAP");
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&cells);
        TerrainMap::from_bytes(&bytes).unwrap()
    }

    fn all_open() -> TerrainMap {
        let w = 8u32;
        pack_map(w, vec![4u8; (w * w) as usize])
    }

    fn west_open_only() -> TerrainMap {
        let w = 8u32;
        let mut cells = vec![0u8; (w * w) as usize];
        for y in 0..w {
            for x in 0..2 {
                cells[(y * w + x) as usize] = 4;
            }
        }
        pack_map(w, cells)
    }

    #[test]
    fn grid_fills_ao_open_ground() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(100_000.0, 100_000.0, 180_000.0, 180_000.0);
        let layout = place_ground(
            true,
            [2, 1, 1],
            &[],
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts::UNCONSTRAINED,
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 4);
        assert!(layout.spots.iter().all(|s| s.in_ao));
        assert_eq!(
            layout
                .spots
                .iter()
                .filter(|s| s.kind == GroundKind::Armor)
                .count(),
            2
        );
        for s in &layout.spots {
            assert!(aabb.contains(s.x, s.z));
            assert!(terrain.is_open_xz(s.x, s.z));
        }
    }

    #[test]
    fn landlocked_ao_overflows_to_open_outside() {
        let terrain = west_open_only();
        let aabb = WorldAabb::from_corners(200_000.0, 300_000.0, 280_000.0, 380_000.0);
        let layout = place_ground(
            false,
            [3, 0, 0],
            &[],
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts::UNCONSTRAINED,
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 3);
        assert!(layout.spots.iter().all(|s| !s.in_ao));
        assert!(layout.spots.iter().all(|s| terrain.is_open_xz(s.x, s.z)));
        let msgs = numbered_ground_issues(&layout.spots, false);
        assert_eq!(msgs.len(), 3);
        assert!(msgs[0].starts_with("1 "));
        assert!(msgs[1].starts_with("2 "));
        assert!(msgs[2].starts_with("3 "));
    }

    #[test]
    fn front_keeps_eastern_ground_north() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 200_000.0, 200_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 200_000.0)];
        let layout = place_ground(
            true,
            [6, 0, 0],
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts::UNCONSTRAINED,
        )
        .unwrap();
        for s in &layout.spots {
            if s.in_ao {
                assert!(
                    s.x > 140_000.0 - 1.0,
                    "eastern ground should sit north of the front, got x={}",
                    s.x
                );
            }
        }
    }

    #[test]
    fn aim_faces_nearest_objective() {
        let mut layout = MapGroundLayout {
            eastern: true,
            spots: vec![GroundSpot::at(
                100_000.0,
                100_000.0,
                true,
                0.0,
                GroundKind::Armor,
                None,
            )],
            warnings: Vec::new(),
        };
        layout.aim_at_objectives(&[(100_000.0, 200_000.0), (400_000.0, 100_000.0)]);
        assert!((layout.spots[0].heading_deg - 90.0).abs() < 0.5);
        assert_eq!(layout.spots[0].objective, Some((100_000.0, 200_000.0)));
        layout.aim_at_hashed_objectives(&[(400_000.0, 100_000.0)], 7);
        assert_eq!(layout.spots[0].objective, Some((400_000.0, 100_000.0)));
    }

    #[test]
    fn front_band_keeps_spots_near_the_front() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 200_000.0, 200_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 200_000.0)];
        let layout = place_ground(
            true,
            [6, 0, 0],
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &[],
                seed: 0,
                occupied: &[],
            },
        )
        .unwrap();
        for s in &layout.spots {
            if s.in_ao {
                assert!(
                    (s.x - 140_000.0).abs() <= 10_000.0 + 1.0,
                    "spot x={} should stay within 10 km of the front",
                    s.x
                );
            }
        }
    }

    #[test]
    fn artillery_stays_within_15km_of_nearest_objective() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let obj = [(200_000.0, 150_000.0)];
        let layout = place_ground(
            true,
            [0, 0, 4],
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &obj,
                seed: 1,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 4);
        for s in &layout.spots {
            assert_eq!(s.kind, GroundKind::Artillery);
            let d = (s.x - obj[0].0).hypot(s.z - obj[0].1);
            assert!(
                d <= ARTY_OBJECTIVE_RADIUS + 1.0,
                "arty at ({}, {}) is {d} m from the objective",
                s.x,
                s.z
            );
            assert!(s.x > 140_000.0 - 1.0);
        }
    }

    #[test]
    fn mixed_keeps_armor_on_front_and_artillery_on_objective() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let obj = [(200_000.0, 150_000.0)];
        let layout = place_ground(
            true,
            [4, 0, 3],
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &obj,
                seed: 2,
                occupied: &[],
            },
        )
        .unwrap();
        let arty: Vec<_> = layout
            .spots
            .iter()
            .filter(|s| s.kind == GroundKind::Artillery)
            .collect();
        let armor: Vec<_> = layout
            .spots
            .iter()
            .filter(|s| s.kind == GroundKind::Armor)
            .collect();
        assert_eq!(arty.len(), 3);
        assert_eq!(armor.len(), 4);
        for s in arty {
            let d = (s.x - obj[0].0).hypot(s.z - obj[0].1);
            assert!(d <= ARTY_OBJECTIVE_RADIUS + 1.0);
        }
        for s in armor {
            if s.in_ao {
                assert!(
                    (s.x - 140_000.0).abs() <= 10_000.0 + 1.0,
                    "armor x={} should stay within 10 km of the front",
                    s.x
                );
            }
        }
        assert_min_spacing(
            &layout.spots.iter().map(|s| (s.x, s.z)).collect::<Vec<_>>(),
            UNIT_PLACE_SPACING,
        );
    }

    #[test]
    fn placed_ground_is_at_least_4500m_apart() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let layout = place_ground(
            true,
            [8, 0, 0],
            &[],
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts::UNCONSTRAINED,
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 8);
        assert_min_spacing(
            &layout.spots.iter().map(|s| (s.x, s.z)).collect::<Vec<_>>(),
            UNIT_PLACE_SPACING,
        );
    }

    #[test]
    fn artillery_cluster_still_respects_spacing() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let obj = [(200_000.0, 150_000.0)];
        let layout = place_ground(
            true,
            [0, 0, 6],
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &obj,
                seed: 3,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 6);
        assert_min_spacing(
            &layout.spots.iter().map(|s| (s.x, s.z)).collect::<Vec<_>>(),
            UNIT_PLACE_SPACING,
        );
    }

    #[test]
    fn katyusha_jobs_stay_within_8470m_of_hashed_objective() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let obj = [(200_000.0, 150_000.0)];
        let jobs = vec![GroundJob::new(GroundKind::Artillery, Some(8_470.0)); 4];
        let layout = place_ground_jobs(
            true,
            &jobs,
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &obj,
                seed: 4,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 4);
        for s in &layout.spots {
            let target = s.objective.expect("assigned objective");
            let d = (s.x - target.0).hypot(s.z - target.1);
            assert!(
                d <= 8_470.0 + 1.0,
                "BM-13 at ({}, {}) is {d} m from its objective",
                s.x,
                s.z
            );
            assert_eq!(s.objective, Some(obj[0]));
        }
    }

    #[test]
    fn short_range_armor_fits_several_groups_in_15km_disk() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let obj = [(200_000.0, 150_000.0)];
        let jobs = vec![GroundJob::new(GroundKind::Armor, Some(1_500.0)); 3];
        let layout = place_ground_jobs(
            true,
            &jobs,
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &obj,
                seed: 9,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 3);
        assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
        for s in &layout.spots {
            let d = (s.x - obj[0].0).hypot(s.z - obj[0].1);
            assert!(
                d <= 1_500.0 + 1.0,
                "armor at ({}, {}) is {d} m from the objective",
                s.x,
                s.z
            );
        }
    }

    #[test]
    fn objective_on_wrong_side_falls_back_with_warning() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let obj = [(100_000.0, 150_000.0)];
        let jobs = [GroundJob::new(GroundKind::Armor, Some(1_500.0))];
        let layout = place_ground_jobs(
            true,
            &jobs,
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &obj,
                seed: 3,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 1);
        assert!(layout.spots[0].issue.is_some());
        let msgs = numbered_ground_issues(&layout.spots, false);
        assert_eq!(msgs.len(), 1);
        assert!(
            msgs[0].starts_with("1 Armor:"),
            "expected numbered unit warning, got {:?}",
            msgs[0]
        );
        assert!(!layout.warnings.is_empty());
        assert!(
            layout.spots[0].x > 140_000.0 - 1.0,
            "eastern fallback should stay north of the front, got x={}",
            layout.spots[0].x
        );
    }

    #[test]
    fn column_job_parks_on_a_road() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 250_000.0, 250_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 250_000.0)];
        let route = crate::mapnet::inspect_route(
            &crate::parser::parse_group_file(include_str!(
                "../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group"
            ))
            .unwrap(),
        )
        .unwrap();
        let jobs = [GroundJob {
            kind: GroundKind::Supply,
            range_m: None,
            route: Some(route),
        }];
        let layout = place_ground_jobs(
            true,
            &jobs,
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &[],
                seed: 2,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 1);
        let s = &layout.spots[0];
        let net = s.network.as_ref().expect("road column");
        assert!(!net.rail);
        assert_eq!(net.unit_xz.len(), 5);
        assert_eq!(net.waypoints.len(), 2);
        assert!(
            s.x > 140_000.0 - 1.0 || s.issue.is_some(),
            "eastern column should sit north of the front unless flagged, x={} issue={:?}",
            s.x,
            s.issue
        );
    }

    fn assert_min_spacing(pts: &[(f64, f64)], min_d: f64) {
        for (i, a) in pts.iter().enumerate() {
            for b in &pts[i + 1..] {
                let d = (a.0 - b.0).hypot(a.1 - b.1);
                assert!(
                    d + 1.0 >= min_d,
                    "units {a:?} and {b:?} are {d:.0} m apart, need {min_d}"
                );
            }
        }
    }
}
