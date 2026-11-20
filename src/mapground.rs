//! mapground.rs — park ground groups on dry terrain (or road/rail)
//!
//! Places Army Generator copies inside one coalition's influence area:
//! armor/supply/artillery on open land (`watermap` open cells, not woods),
//! infantry on any dry land (not water), trains and perfect columns on
//! `mapnet` lines.
//! Hashes each group toward an objective (AttackArea later snaps there, or
//! across the front along heading when none are marked) and records soft
//! placement issues for the status line. Infantry parks in the front band
//! and faces the closest objective, or the front when none are
//! marked. Other front-band groups also face the front when no objective is
//! set. Off-road Goto WP hops are aimed the same way and kept on dry land
//! when a path exists. It does not clone templates (`recon` parks the copies onto these spots).
//!
//! ## Public API
//! * `MAX_GROUND` / `GROUND_SPACING` / `START_DELAY_S` / `GROUP_DELAY_S`
//! * `ARTY_OBJECTIVE_RADIUS` — unknown-artillery fallback (15 km)
//! * `enum GroundKind` / `struct GroundJob` / `struct GroundSpot`
//! * `struct MapGroundLayout`
//! * `fn place_ground` / `fn place_ground_jobs`
//! * `fn numbered_ground_issues` — 1-based warnings for the UI
//! * `fn attack_across_front` — AttackArea past the FLOT when no objective
//! * `fn layout_path_waypoints` — off-road Goto WP hops toward objective/front
//!
//! ## Used by
//! * ui.rs (Map, Army Generator) — preview pins and recon parking spots

use geo::{Contains, Point};

use crate::frontlines::{AOI_GAP, densify};
use crate::geo::{MAP_MAX, MAP_MIN};
use crate::mapclip::{
    WorldAabb, apply_salients, filter_front_band, influence_minus_salients, point_north_of_front,
    snap_to_front,
};
use crate::mapnet::{self, NetworkSpot, RouteLayout};
use crate::placement::{
    PlaceOpts, UNIT_PLACE_SPACING, hashed_pick, heading_toward, heading_toward_nearest,
    subsample_spaced,
};
use crate::watermap::TerrainMap;
use crate::weapon_range::UNKNOWN_ARTILLERY_M;

pub const MAX_GROUND: usize = 64;
pub const GROUND_SPACING: f64 = UNIT_PLACE_SPACING;
pub const START_DELAY_S: f64 = 5.0;
pub const GROUP_DELAY_S: f64 = 0.5;
/// Fallback when a template has no known gun script.
pub const ARTY_OBJECTIVE_RADIUS: f64 = UNKNOWN_ARTILLERY_M;

/// Road columns prefer to park this far inside the AO, including their vehicles
/// and waypoints. No road in that inset falls back to the full box.
const ROAD_AO_MARGIN_M: f64 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundKind {
    Armor,
    Supply,
    Artillery,
    Infantry,
    Train,
    AntiAir,
}

impl GroundKind {
    pub fn label(self) -> &'static str {
        match self {
            GroundKind::Armor => "Armor",
            GroundKind::Supply => "Supply",
            GroundKind::Artillery => "Artillery",
            GroundKind::Infantry => "Infantry",
            GroundKind::Train => "Train",
            GroundKind::AntiAir => "Anti-air",
        }
    }
}

/// One group to park. `range_m` is max distance to that copy's hashed objective.
/// `route` parks trains on rails and perfect columns on roads instead.
/// `wp_ahead` is authored Goto WP distances for off-road groups (not columns).
#[derive(Clone, Debug)]
pub struct GroundJob {
    pub kind: GroundKind,
    pub range_m: Option<f64>,
    pub route: Option<RouteLayout>,
    pub wp_ahead: Vec<f64>,
    /// Park in a clearing around this point instead of on the front band.
    pub anchor: Option<(f64, f64)>,
}

impl GroundJob {
    pub fn new(kind: GroundKind, range_m: Option<f64>) -> Self {
        Self {
            kind,
            range_m,
            route: None,
            wp_ahead: Vec::new(),
            anchor: None,
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
    /// Off-road path waypoint world positions (columns use `network.waypoints`).
    pub waypoints: Vec<(f64, f64)>,
    /// Authored hop distances so heading changes can rebuild `waypoints`.
    pub wp_ahead: Vec<f64>,
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
            waypoints: Vec::new(),
            wp_ahead: Vec::new(),
            issue: None,
        }
    }

    pub fn on_network(&self) -> bool {
        self.network.is_some()
    }

    pub fn path_waypoints(&self) -> &[(f64, f64)] {
        if let Some(net) = &self.network {
            &net.waypoints
        } else {
            &self.waypoints
        }
    }

    pub fn apply_sampled(&mut self, pose: Option<(f64, f64, f64)>) {
        if let Some((x, z, heading)) = pose {
            self.x = x;
            self.z = z;
            self.heading_deg = heading;
        }
    }

    /// Rebuild off-road Goto WP hops along heading, staying on dry land when possible.
    pub fn layout_offroad_waypoints(&mut self, terrain: &TerrainMap) {
        if self.network.is_some() || self.wp_ahead.is_empty() {
            return;
        }
        let (wps, wet) = layout_path_waypoints(
            (self.x, self.z),
            self.heading_deg,
            &self.wp_ahead,
            self.objective,
            terrain,
        );
        self.waypoints = wps;
        set_path_water_issue(self, wet);
    }

    /// Update the water warning after a user-placed off-road hop.
    pub fn refresh_path_water_issue(&mut self, terrain: &TerrainMap) {
        if self.network.is_some() {
            return;
        }
        let mut wet = false;
        let mut prev = (self.x, self.z);
        for &p in &self.waypoints {
            if !terrain.is_land_xz(p.0, p.1) || segment_crosses_water(terrain, prev, p) {
                wet = true;
                break;
            }
            prev = p;
        }
        set_path_water_issue(self, wet);
    }
}

/// Preview-numbered soft problems for the status list (1-based, same as map labels).
pub fn numbered_ground_issues(spots: &[GroundSpot]) -> Vec<String> {
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
    /// Face each spot at the nearest objective. Empty `objectives` leaves headings unchanged.
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

    /// Face each non-network spot at the closest point on `front`.
    pub fn aim_at_front(&mut self, front: &[(f64, f64)]) {
        if front.len() < 2 {
            return;
        }
        for spot in &mut self.spots {
            if spot.on_network() {
                continue;
            }
            if let Some(h) = heading_toward_front((spot.x, spot.z), front) {
                spot.heading_deg = h;
            }
        }
    }

    /// Face objectives when any are marked; otherwise face the front.
    pub fn aim_at_objectives_or_front(&mut self, objectives: &[(f64, f64)], front: &[(f64, f64)]) {
        if objectives.is_empty() {
            self.aim_at_front(front);
        } else {
            self.aim_at_objectives(objectives);
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

    /// Rebuild off-road Goto WP hops after heading or objective changes.
    pub fn layout_offroad_waypoints(&mut self, terrain: &TerrainMap) {
        for spot in &mut self.spots {
            spot.layout_offroad_waypoints(terrain);
        }
    }
}

fn heading_toward_front(from: (f64, f64), front: &[(f64, f64)]) -> Option<f64> {
    snap_to_front(front, from).map(|to| heading_toward(from, to))
}

/// How far past the FLOT an AttackArea sits when range allows.
pub const ATTACK_ACROSS_FRONT_M: f64 = 500.0;

/// Point along `heading_deg` for a ground AttackArea when no objective is marked:
/// on the far side of `front` when weapon `range_m` reaches it, otherwise at
/// max range toward the line.
pub fn attack_across_front(
    from: (f64, f64),
    heading_deg: f64,
    front: &[(f64, f64)],
    range_m: f64,
    _eastern: bool,
) -> Option<(f64, f64)> {
    if range_m <= 1.0 {
        return None;
    }
    let range = range_m;
    let rad = heading_deg.to_radians();
    let dir = (rad.cos(), rad.sin());
    if let Some(t) = ray_hit_polyline(from, dir, front) {
        let dist = (t + ATTACK_ACROSS_FRONT_M).min(range);
        return Some((from.0 + dir.0 * dist, from.1 + dir.1 * dist));
    }
    let q = snap_to_front(front, from)?;
    let dx = q.0 - from.0;
    let dz = q.1 - from.1;
    let d = dx.hypot(dz);
    if d < 1.0 {
        let dist = range.min(ATTACK_ACROSS_FRONT_M);
        return Some((from.0 + dir.0 * dist, from.1 + dir.1 * dist));
    }
    let ux = dx / d;
    let uz = dz / d;
    let dist = (d + ATTACK_ACROSS_FRONT_M).min(range);
    Some((from.0 + ux * dist, from.1 + uz * dist))
}

/// Distance along unit `dir` to the nearest hit on `front`, if the ray hits.
fn ray_hit_polyline(origin: (f64, f64), dir: (f64, f64), front: &[(f64, f64)]) -> Option<f64> {
    if front.len() < 2 {
        return None;
    }
    let mut best: Option<f64> = None;
    for w in front.windows(2) {
        if let Some(t) = ray_seg_t(origin, dir, w[0], w[1]) {
            if t >= 0.0 && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
    }
    best
}

fn ray_seg_t(origin: (f64, f64), dir: (f64, f64), a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let sx = b.0 - a.0;
    let sz = b.1 - a.1;
    let det = dir.0 * sz - dir.1 * sx;
    if det.abs() < 1e-9 {
        return None;
    }
    let ox = a.0 - origin.0;
    let oz = a.1 - origin.1;
    let t = (ox * sz - oz * sx) / det;
    let u = (ox * dir.1 - oz * dir.0) / det;
    if (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

const PATH_WP_WATER_ISSUE: &str = "path waypoint crosses water.";
const PATH_WP_ANGLE_OFFSETS: [f64; 12] = [
    8.0, -8.0, 16.0, -16.0, 28.0, -28.0, 40.0, -40.0, 55.0, -55.0, 75.0, -75.0,
];

fn set_path_water_issue(spot: &mut GroundSpot, wet: bool) {
    let rest = spot.issue.take().and_then(|s| {
        let trimmed = s
            .replace(" Path waypoint crosses water.", "")
            .replace(PATH_WP_WATER_ISSUE, "")
            .trim()
            .to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    spot.issue = if wet {
        Some(match rest {
            Some(s) => format!("{s} Path waypoint crosses water."),
            None => PATH_WP_WATER_ISSUE.to_string(),
        })
    } else {
        rest
    };
}

fn sample_step_m(terrain: &TerrainMap) -> f64 {
    let cell = (MAP_MAX - MAP_MIN) / terrain.width.max(1) as f64;
    cell.clamp(80.0, 400.0)
}

fn segment_crosses_water(terrain: &TerrainMap, a: (f64, f64), b: (f64, f64)) -> bool {
    let d = (a.0 - b.0).hypot(a.1 - b.1);
    if d < 1.0 {
        return terrain.is_water_xz(b.0, b.1) || !terrain.is_land_xz(b.0, b.1);
    }
    let step = sample_step_m(terrain);
    let n = (d / step).ceil().max(1.0) as usize;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let x = a.0 + (b.0 - a.0) * t;
        let z = a.1 + (b.1 - a.1) * t;
        if terrain.is_water_xz(x, z) || !terrain.is_land_xz(x, z) {
            return true;
        }
    }
    false
}

fn hop_along(from: (f64, f64), heading_deg: f64, dist: f64) -> (f64, f64) {
    let rad = heading_deg.to_radians();
    (from.0 + rad.cos() * dist, from.1 + rad.sin() * dist)
}

fn last_dry_along(
    terrain: &TerrainMap,
    from: (f64, f64),
    heading_deg: f64,
    dist: f64,
) -> Option<(f64, f64)> {
    let step = sample_step_m(terrain);
    let n = (dist / step).ceil().max(1.0) as usize;
    let mut last = None;
    for i in 1..=n {
        let t = dist * i as f64 / n as f64;
        let p = hop_along(from, heading_deg, t);
        if terrain.is_land_xz(p.0, p.1) {
            last = Some(p);
        } else if last.is_some() {
            break;
        }
    }
    last
}

fn place_path_hop(
    terrain: &TerrainMap,
    from: (f64, f64),
    prev: (f64, f64),
    heading_deg: f64,
    dist: f64,
) -> ((f64, f64), bool) {
    let straight = hop_along(from, heading_deg, dist);
    if terrain.is_land_xz(straight.0, straight.1) && !segment_crosses_water(terrain, prev, straight)
    {
        return (straight, false);
    }
    for &offset in &PATH_WP_ANGLE_OFFSETS {
        let p = hop_along(from, heading_deg + offset, dist);
        if terrain.is_land_xz(p.0, p.1) && !segment_crosses_water(terrain, prev, p) {
            return (p, false);
        }
    }
    if let Some(p) = last_dry_along(terrain, from, heading_deg, dist) {
        let wet = !terrain.is_land_xz(p.0, p.1) || segment_crosses_water(terrain, prev, p);
        return (p, wet);
    }
    let wet = !terrain.is_land_xz(straight.0, straight.1)
        || segment_crosses_water(terrain, prev, straight);
    (straight, wet)
}

/// Place off-road Goto WP hops along `heading_deg` (0 = north / +X).
/// When `objective` is closer than the last authored hop, distances are scaled
/// so the last hop sits on that marker. Each hop stays on dry land when a
/// detour (or a shorter hop) exists; otherwise the straight line is kept and
/// the second return value is `true` (warn the user).
pub fn layout_path_waypoints(
    from: (f64, f64),
    heading_deg: f64,
    wp_ahead: &[f64],
    objective: Option<(f64, f64)>,
    terrain: &TerrainMap,
) -> (Vec<(f64, f64)>, bool) {
    if wp_ahead.is_empty() {
        return (Vec::new(), false);
    }
    let mut dists = wp_ahead.to_vec();
    if let Some(obj) = objective {
        let reach = (obj.0 - from.0).hypot(obj.1 - from.1);
        if let Some(&last) = dists.last() {
            if last > 1.0 && reach > 50.0 && last > reach {
                let scale = reach / last;
                for d in &mut dists {
                    *d *= scale;
                }
            }
        }
    }
    let mut out = Vec::with_capacity(dists.len());
    let mut wet = false;
    let mut prev = from;
    for dist in dists {
        if dist < 50.0 {
            continue;
        }
        let (p, hop_wet) = place_path_hop(terrain, from, prev, heading_deg, dist);
        wet |= hop_wet;
        out.push(p);
        prev = p;
    }
    (out, wet)
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
        jobs.extend(
            std::iter::repeat(GroundJob::new(
                GroundKind::Artillery,
                Some(ARTY_OBJECTIVE_RADIUS),
            ))
            .take(kind_counts[2]),
        );
        jobs.extend(
            std::iter::repeat(GroundJob::new(GroundKind::Armor, None)).take(kind_counts[0]),
        );
        jobs.extend(
            std::iter::repeat(GroundJob::new(GroundKind::Supply, None)).take(kind_counts[1]),
        );
    } else {
        jobs.extend(
            std::iter::repeat(GroundJob::new(GroundKind::Armor, None)).take(kind_counts[0]),
        );
        jobs.extend(
            std::iter::repeat(GroundJob::new(GroundKind::Supply, None)).take(kind_counts[1]),
        );
        jobs.extend(
            std::iter::repeat(GroundJob::new(GroundKind::Artillery, None)).take(kind_counts[2]),
        );
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
            let placed = if route.rail {
                None
            } else {
                place_road_inside(route, aabb, &side, opts.seed, i, &used)
            };
            let placed = if placed.is_some() {
                placed
            } else {
                mapnet::place_route(route, aabb, opts.seed, i, &used, prefer_side)
            };
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
                        None => (
                            None,
                            Some(
                                "no road long enough inside the AO — parked on open ground.".into(),
                            ),
                        ),
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
                    waypoints: Vec::new(),
                    wp_ahead: Vec::new(),
                    issue,
                });
                continue;
            }
            if route.rail {
                continue;
            }
            pending_issue = issue;
        }
        if let Some(anchor) = job.anchor {
            let spacing = 400.0;
            if let Some((x, z, in_ao)) = pick_clearing_around(
                anchor,
                &side,
                aabb,
                terrain,
                &used,
                spacing,
                opts.seed.wrapping_add(i as u64),
            ) {
                used.push((x, z));
                let d = (x - anchor.0).hypot(z - anchor.1);
                if d < AAA_INNER_M || d > AAA_OUTER_M {
                    pending_issue = Some(format!(
                        "no clearing in the ring around the point — parked {:.1} km away.",
                        d / 1000.0
                    ));
                }
                spots.push(GroundSpot {
                    x,
                    z,
                    in_ao,
                    heading_deg: heading_toward_nearest((x, z), &[anchor]).unwrap_or(0.0),
                    kind: job.kind,
                    objective: Some(anchor),
                    network: None,
                    waypoints: Vec::new(),
                    wp_ahead: Vec::new(),
                    issue: pending_issue,
                });
            } else {
                warnings.push(format!(
                    "{} {}: no clearing around {:.0}, {:.0}.",
                    i + 1,
                    job.kind.label(),
                    anchor.0,
                    anchor.1
                ));
            }
            continue;
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
                TerrainFilter::Open,
                false,
            )
            .into_iter()
            .next()
            {
                pending_issue =
                    Some("no open ground near an objective — parked along the front.".into());
                Some(p)
            } else {
                None
            }
        } else {
            let infantry = job.kind == GroundKind::Infantry;
            pick_front_open(
                1,
                &side,
                front_xz,
                aabb,
                terrain,
                opts,
                &used,
                UNIT_PLACE_SPACING,
                if infantry {
                    TerrainFilter::Dry
                } else {
                    TerrainFilter::Open
                },
                infantry && opts.favor.is_empty(),
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
        let nearest = opts.favor.iter().copied().min_by(|a, b| {
            let da = (a.0 - x).hypot(a.1 - z);
            let db = (b.0 - x).hypot(b.1 - z);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        let aim = if job.kind == GroundKind::Infantry {
            nearest
        } else {
            assigned
        };
        let heading_deg = aim
            .map(|t| heading_toward((x, z), t))
            .or_else(|| heading_toward_front((x, z), front_xz))
            .unwrap_or(0.0);
        let objective = if job.kind == GroundKind::Infantry {
            nearest.or(assigned)
        } else {
            assigned
        };
        let mut spot = GroundSpot::at(x, z, in_ao, heading_deg, job.kind, objective);
        spot.issue = pending_issue;
        spot.wp_ahead = job.wp_ahead.clone();
        spot.layout_offroad_waypoints(terrain);
        spots.push(spot);
    }

    if spots.is_empty() {
        if jobs
            .iter()
            .any(|j| j.route.as_ref().is_some_and(|r| r.rail))
        {
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
    warnings.extend(numbered_ground_issues(&spots));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainFilter {
    /// Dry open cells (armor, supply, artillery).
    Open,
    /// Any non-water cell (infantry).
    Dry,
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
    filter: TerrainFilter,
    prefer_cover: bool,
) -> Vec<(f64, f64, bool)> {
    let mut inside = sample_terrain(terrain, aabb, GROUND_SPACING, side, filter);
    inside = filter_front_band(inside, front_xz, opts.front_band);
    inside = drop_near(inside, used, spacing);
    for step in [2_000.0, 1_000.0, 500.0] {
        if inside.len() >= total {
            break;
        }
        inside = sample_terrain(terrain, aabb, step, side, filter);
        inside = filter_front_band(inside, front_xz, opts.front_band);
        inside = drop_near(inside, used, spacing);
    }
    inside = apply_cover_preference(terrain, inside, prefer_cover, total);
    let take = total.min(inside.len());
    let mut points: Vec<(f64, f64, bool)> =
        subsample_spaced(inside, take, opts.favor, opts.seed, spacing, used)
            .into_iter()
            .map(|(x, z)| (x, z, true))
            .collect();

    if points.len() < total {
        let outside_box = WorldAabb::full_map();
        let mut outside = sample_terrain(terrain, outside_box, GROUND_SPACING, side, filter);
        outside.retain(|(x, z)| !aabb.contains(*x, *z));
        outside = filter_front_band(outside, front_xz, opts.front_band);
        outside = drop_near(outside, used, spacing);
        for step in [2_000.0, 1_000.0, 500.0] {
            if outside.len() >= total - points.len() {
                break;
            }
            outside = sample_terrain(terrain, outside_box, step, side, filter);
            outside.retain(|(x, z)| !aabb.contains(*x, *z));
            outside = filter_front_band(outside, front_xz, opts.front_band);
            outside = drop_near(outside, used, spacing);
        }
        outside = apply_cover_preference(terrain, outside, prefer_cover, total - points.len());
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

const AAA_INNER_M: f64 = 600.0;
const AAA_OUTER_M: f64 = 2_200.0;

/// Open ground in a ring around `anchor`: close enough to defend it, not on top of it.
fn pick_clearing_around(
    anchor: (f64, f64),
    side: &LandSide,
    aabb: WorldAabb,
    terrain: &TerrainMap,
    occupied: &[(f64, f64)],
    spacing: f64,
    seed: u64,
) -> Option<(f64, f64, bool)> {
    let pool = clearing_ring(
        anchor,
        side,
        aabb,
        terrain,
        occupied,
        spacing,
        AAA_INNER_M,
        AAA_OUTER_M,
    );
    let pool = if pool.is_empty() {
        clearing_ring(
            anchor, side, aabb, terrain, occupied, spacing, 300.0, 4_000.0,
        )
    } else {
        pool
    };
    if pool.is_empty() {
        return pick_closest_open(anchor, side, aabb, terrain, occupied, spacing);
    }
    let (x, z) = pool[(seed as usize) % pool.len()];
    Some((x, z, aabb.contains(x, z)))
}

fn clearing_ring(
    anchor: (f64, f64),
    side: &LandSide,
    aabb: WorldAabb,
    terrain: &TerrainMap,
    occupied: &[(f64, f64)],
    spacing: f64,
    inner: f64,
    outer: f64,
) -> Vec<(f64, f64)> {
    let mut cand = sample_open_near(terrain, 250.0, side, &[anchor], outer);
    cand.retain(|&(x, z)| {
        let d = (x - anchor.0).hypot(z - anchor.1);
        d >= inner && d <= outer
    });
    cand = drop_near(cand, occupied, spacing);
    let inside: Vec<(f64, f64)> = cand
        .iter()
        .copied()
        .filter(|&(x, z)| aabb.contains(x, z))
        .collect();
    if inside.is_empty() { cand } else { inside }
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
    let mut points: Vec<(f64, f64, bool)> =
        subsample_spaced(inside, n, objectives, seed, spacing, occupied)
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
    out.retain(|&(x, z)| objectives.iter().any(|&(ox, oz)| dist2(x, z, ox, oz) <= r2));
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

fn ao_inset(aabb: WorldAabb, margin: f64) -> Option<WorldAabb> {
    let shrunk = aabb.expanded(-margin);
    (shrunk.x_min < shrunk.x_max && shrunk.z_min < shrunk.z_max).then_some(shrunk)
}

/// Lead, vehicles, and waypoints all inside `aabb`.
fn column_inside(x: f64, z: f64, net: &NetworkSpot, aabb: WorldAabb) -> bool {
    aabb.contains(x, z)
        && net.unit_xz.iter().all(|&(px, pz)| aabb.contains(px, pz))
        && net.waypoints.iter().all(|&(px, pz)| aabb.contains(px, pz))
}

/// A road column at least [`ROAD_AO_MARGIN_M`] inside the AO, on this side.
/// Several seeds are tried so a long column can find a stretch that fits.
fn place_road_inside(
    route: &RouteLayout,
    aabb: WorldAabb,
    side: &LandSide,
    seed: u64,
    index: usize,
    used: &[(f64, f64)],
) -> Option<((f64, f64, f64, bool), NetworkSpot)> {
    let inset = ao_inset(aabb, ROAD_AO_MARGIN_M)?;
    let prefer = |x: f64, z: f64| inset.contains(x, z) && allowed(side, x, z);
    for k in 0..8u64 {
        let Some(placed) = mapnet::place_route(
            route,
            aabb,
            seed.wrapping_add(k.wrapping_mul(97)),
            index,
            used,
            &prefer,
        ) else {
            continue;
        };
        let ((x, z, _, _), net) = &placed;
        if column_inside(*x, *z, net, inset) {
            return Some(placed);
        }
    }
    None
}

enum LandSide {
    Any,
    Polygon(geo::MultiPolygon<f64>),
    Front {
        eastern: bool,
        front: Vec<(f64, f64)>,
    },
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
    sample_terrain(terrain, aabb, step, side, TerrainFilter::Open)
}

fn sample_terrain(
    terrain: &TerrainMap,
    aabb: WorldAabb,
    step: f64,
    side: &LandSide,
    filter: TerrainFilter,
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
            let ok = match filter {
                TerrainFilter::Open => open_ground(terrain, x, z),
                TerrainFilter::Dry => terrain.is_land_xz(x, z),
            };
            if ok && allowed(side, x, z) {
                out.push((x, z));
            }
            z += step;
        }
        x += step;
    }
    out
}

/// Dry open ground from the packed mask. Woods and water are closed.
/// The check is one byte. It does not open an editor photograph.
fn open_ground(terrain: &TerrainMap, x: f64, z: f64) -> bool {
    terrain.is_open_xz(x, z)
}

/// When `prefer_cover` is set, keep photographed woods if there are enough of
/// them. Otherwise keep dry cells the mask does not call open.
fn apply_cover_preference(
    terrain: &TerrainMap,
    pts: Vec<(f64, f64)>,
    prefer_cover: bool,
    need: usize,
) -> Vec<(f64, f64)> {
    if !prefer_cover || pts.is_empty() {
        return pts;
    }
    let woods: Vec<(f64, f64)> = pts
        .iter()
        .copied()
        .filter(|&(x, z)| terrain.is_woods_xz(x, z))
        .collect();
    if woods.len() >= need {
        return woods;
    }
    let cover: Vec<(f64, f64)> = pts
        .iter()
        .copied()
        .filter(|&(x, z)| !open_ground(terrain, x, z))
        .collect();
    if cover.len() >= need { cover } else { pts }
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

    fn all_cover() -> TerrainMap {
        let w = 8u32;
        pack_map(w, vec![0u8; (w * w) as usize])
    }

    fn all_water() -> TerrainMap {
        let w = 8u32;
        pack_map(w, vec![1u8; (w * w) as usize])
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
        let msgs = numbered_ground_issues(&layout.spots);
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
        let front = vec![(90_000.0, 80_000.0), (90_000.0, 200_000.0)];
        layout.aim_at_front(&front);
        let expected = heading_toward((100_000.0, 100_000.0), (90_000.0, 100_000.0));
        assert!((layout.spots[0].heading_deg - expected).abs() < 0.5);
    }

    #[test]
    fn attack_across_front_sits_past_the_flot_when_range_allows() {
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let from = (150_000.0, 150_000.0);
        let heading = heading_toward_front(from, &front).unwrap();
        let p = attack_across_front(from, heading, &front, 15_000.0, true).unwrap();
        assert!(
            p.0 < 140_000.0,
            "15 km from 10 km north of the front should land south of it, got x={}",
            p.0
        );
        let d = (p.0 - from.0).hypot(p.1 - from.1);
        assert!(d <= 15_000.0 + 1.0);
        assert!((p.0 - (140_000.0 - ATTACK_ACROSS_FRONT_M)).abs() < 1.0);
        assert!((p.1 - 150_000.0).abs() < 1.0);
    }

    #[test]
    fn attack_across_front_stays_in_range_when_flot_is_too_far() {
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let from = (150_000.0, 150_000.0);
        let heading = heading_toward_front(from, &front).unwrap();
        let p = attack_across_front(from, heading, &front, 5_000.0, true).unwrap();
        let d = (p.0 - from.0).hypot(p.1 - from.1);
        assert!((d - 5_000.0).abs() < 1.0);
        assert!(
            p.0 > 140_000.0,
            "5 km cannot reach 10 km to the front, should stay north, x={}",
            p.0
        );
    }

    #[test]
    fn infantry_parks_on_front_facing_closest_objective() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let near = (200_000.0, 150_000.0);
        let far = (400_000.0, 400_000.0);
        let jobs = vec![GroundJob::new(GroundKind::Infantry, None); 3];
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
                favor: &[near, far],
                seed: 4,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 3);
        for s in &layout.spots {
            assert_eq!(s.kind, GroundKind::Infantry);
            if s.in_ao {
                assert!(
                    (s.x - 140_000.0).abs() <= 10_000.0 + 1.0,
                    "infantry x={} should stay within 10 km of the front",
                    s.x
                );
            }
            assert_eq!(s.objective, Some(near));
            let expected = heading_toward((s.x, s.z), near);
            assert!(
                (s.heading_deg - expected).abs() < 0.5,
                "heading {} should face closest objective ({expected})",
                s.heading_deg
            );
        }
    }

    fn woods_east_open_west() -> TerrainMap {
        let w = 8u32;
        let mut cells = vec![4u8; (w * w) as usize];
        for y in 0..w {
            for x in 4..w {
                cells[(y * w + x) as usize] = 8;
            }
        }
        pack_map(w, cells)
    }

    #[test]
    fn infantry_prefers_woods_and_armor_stays_on_open() {
        let terrain = woods_east_open_west();
        let aabb = WorldAabb::from_corners(100_000.0, 40_000.0, 200_000.0, 460_000.0);
        let jobs = vec![GroundJob::new(GroundKind::Infantry, None); 3];
        let layout = place_ground_jobs(
            true,
            &jobs,
            &[],
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts::UNCONSTRAINED,
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 3);
        for s in &layout.spots {
            assert!(
                terrain.is_woods_xz(s.x, s.z),
                "infantry at ({}, {}) should prefer woods",
                s.x,
                s.z
            );
            assert!(!terrain.is_open_xz(s.x, s.z));
        }
        let armor = place_ground(
            true,
            [3, 0, 0],
            &[],
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts::UNCONSTRAINED,
        )
        .unwrap();
        assert_eq!(armor.spots.len(), 3);
        for s in &armor.spots {
            assert!(terrain.is_open_xz(s.x, s.z));
            assert!(!terrain.is_woods_xz(s.x, s.z));
        }
    }

    #[test]
    fn infantry_parks_on_cover_while_armor_needs_open() {
        let terrain = all_cover();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let jobs = vec![GroundJob::new(GroundKind::Infantry, None); 3];
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
                seed: 5,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 3);
        for s in &layout.spots {
            assert!(terrain.is_land_xz(s.x, s.z));
            assert!(!terrain.is_water_xz(s.x, s.z));
            assert!(
                !terrain.is_open_xz(s.x, s.z),
                "cover-only map should not look like a clearing"
            );
        }
        let armor = place_ground(
            true,
            [3, 0, 0],
            &front,
            aabb,
            &[],
            true,
            &terrain,
            PlaceOpts {
                front_band: Some(10_000.0),
                favor: &[],
                seed: 5,
                occupied: &[],
            },
        );
        assert!(armor.is_err(), "armor still needs open ground");
    }

    #[test]
    fn infantry_rejects_water() {
        let terrain = all_water();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let jobs = vec![GroundJob::new(GroundKind::Infantry, None); 2];
        let err = place_ground_jobs(
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
                seed: 1,
                occupied: &[],
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn front_band_units_face_the_front_without_objectives() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 220_000.0)];
        let jobs = vec![
            GroundJob::new(GroundKind::Infantry, None),
            GroundJob::new(GroundKind::Armor, None),
            GroundJob::new(GroundKind::Supply, None),
        ];
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
                seed: 6,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 3);
        for s in &layout.spots {
            let expected = heading_toward_front((s.x, s.z), &front).unwrap();
            assert!(
                (s.heading_deg - expected).abs() < 0.5,
                "{} heading {} should face the front ({expected})",
                s.kind.label(),
                s.heading_deg
            );
            assert!(
                (s.heading_deg - 180.0).abs() < 15.0,
                "eastern unit at ({}, {}) should look south toward the front, got {}",
                s.x,
                s.z,
                s.heading_deg
            );
        }
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
        let msgs = numbered_ground_issues(&layout.spots);
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
            wp_ahead: Vec::new(),
            anchor: None,
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
        let inset = aabb.expanded(-ROAD_AO_MARGIN_M);
        assert!(
            column_inside(s.x, s.z, net, inset),
            "truck route should keep a 10 km margin, lead=({}, {})",
            s.x,
            s.z
        );
    }

    #[test]
    fn path_waypoints_follow_heading_on_open_ground() {
        let terrain = all_open();
        let from = (150_000.0, 150_000.0);
        let (wps, wet) = layout_path_waypoints(from, 180.0, &[4_000.0, 8_000.0], None, &terrain);
        assert!(!wet);
        assert_eq!(wps.len(), 2);
        assert!((wps[0].0 - 146_000.0).abs() < 1.0);
        assert!((wps[1].0 - 142_000.0).abs() < 1.0);
        assert!((wps[0].1 - 150_000.0).abs() < 1.0);
        assert!((wps[1].1 - 150_000.0).abs() < 1.0);
    }

    #[test]
    fn path_waypoints_scale_to_a_closer_objective() {
        let terrain = all_open();
        let from = (150_000.0, 150_000.0);
        let obj = (147_000.0, 150_000.0);
        let (wps, wet) =
            layout_path_waypoints(from, 180.0, &[4_000.0, 8_000.0], Some(obj), &terrain);
        assert!(!wet);
        assert_eq!(wps.len(), 2);
        assert!((wps[0].0 - 148_500.0).abs() < 1.0);
        assert!((wps[1].0 - obj.0).abs() < 1.0);
        assert!((wps[1].1 - obj.1).abs() < 1.0);
    }

    #[test]
    fn path_waypoints_warn_when_the_path_is_all_water() {
        let terrain = all_water();
        let from = (150_000.0, 150_000.0);
        let (wps, wet) = layout_path_waypoints(from, 0.0, &[4_000.0], None, &terrain);
        assert!(wet);
        assert_eq!(wps.len(), 1);
        let mut spot = GroundSpot::at(from.0, from.1, true, 0.0, GroundKind::Armor, None);
        spot.wp_ahead = vec![4_000.0];
        spot.layout_offroad_waypoints(&terrain);
        assert_eq!(spot.issue.as_deref(), Some("path waypoint crosses water."));
    }

    #[test]
    fn offroad_job_lays_out_waypoints_toward_the_front() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 250_000.0, 250_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 250_000.0)];
        let jobs = [GroundJob {
            kind: GroundKind::Armor,
            range_m: None,
            route: None,
            wp_ahead: vec![4_000.0],
            anchor: None,
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
                seed: 3,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 1);
        let s = &layout.spots[0];
        assert_eq!(s.waypoints.len(), 1);
        let expected = hop_along((s.x, s.z), s.heading_deg, 4_000.0);
        assert!((s.waypoints[0].0 - expected.0).abs() < 1.0);
        assert!((s.waypoints[0].1 - expected.1).abs() < 1.0);
        let aim = heading_toward_front((s.x, s.z), &front).unwrap();
        assert!((s.heading_deg - aim).abs() < 0.5);
    }

    #[test]
    fn anti_air_parks_in_a_clearing_around_the_anchor() {
        let terrain = all_open();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 250_000.0, 250_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 250_000.0)];
        let anchor = (180_000.0, 160_000.0);
        let jobs = [GroundJob {
            kind: GroundKind::AntiAir,
            range_m: None,
            route: None,
            wp_ahead: Vec::new(),
            anchor: Some(anchor),
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
                seed: 7,
                occupied: &[],
            },
        )
        .unwrap();
        assert_eq!(layout.spots.len(), 1);
        let spot = &layout.spots[0];
        assert_eq!(spot.kind, GroundKind::AntiAir);
        let d = (spot.x - anchor.0).hypot(spot.z - anchor.1);
        assert!(
            (300.0..=4_000.0).contains(&d),
            "parked {d:.0} m from the anchor, want a clearing in the ring"
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
