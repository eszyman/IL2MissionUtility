//! # mapnet.rs — Korea road & railroad network
//!
//! Korea road and railroad polylines (editor `highways.ini` /
//! `railroads.ini`) in world X/Z, plus everything needed to place, drag,
//! and export ground groups parked on them.
//!
//! Image X increases east (game Z); image Y increases south (game X
//! decreases). The square is 9984 units across the 499_200 m map, so 50 m
//! per image unit (`svg_to_world`). `Highways_LaneOffset` is a render
//! offset and is not applied.
//!
//! ## What this owns
//! * Parsing `assets/gamedata/highways.ini` / `railroads.ini` into `Network`
//!   (`PolyLine`s with cumulative arc length), lazily via `OnceLock`:
//!   `roads()`, `railroads()`, `network_for()`. Bridge tags (`B`, `T`) stay
//!   on the centerline. `assets/roads.svg` and `assets/railroads.svg` are
//!   not the network.
//! * `inspect_route` — detects an authored group that is a single road
//!   column (perfect-column heuristic) or a train, and extracts a
//!   `RouteLayout` (behind-unit gaps, ahead MCU_Waypoint hops, Zone IN
//!   radius; defaults 19 km rail / 10 km road).
//! * `place_route` — seeded, stochastic parking onto a line long enough and
//!   overlapping the AO; keeps `NETWORK_SPACING` (2.5 km) from occupied
//!   points. Road columns bias `lead_dist` toward vertices the `prefer`
//!   closure likes (e.g. near the front). A truck lead on one of those
//!   vertices bisects the two road segments that meet there, along its
//!   direction of travel. A train's waypoints stay on the segment midpoint.
//!   The locomotive is then set on a rail node of that segment, and its
//!   heading bisects the two segments that meet there the same way.
//! * Interactive editing: `snap_lead_to_pointer` (left-drag a group),
//!   `snap_waypoint_to_pointer` (place one WP on any branch, including
//!   behind the column), `align_heading_to_path` (right-drag heading; picks
//!   forward/reverse to best match).
//! * `sample_network` — recomputes unit + waypoint world positions from a
//!   `NetworkSpot`; `column_waypoint_dists` — a two-WP column straddles the
//!   Zone IN circle (one hop near each rim) when the path is long enough.
//!   When one side is a dead end, that hop stops at the end of the spur and
//!   the other hop runs to the Zone OUT rim along any connected road.
//! * `park_route_copy` — writes final world positions back into the group:
//!   vehicles, their LinkTrId targets, and MCU_Waypoints (ascending Index),
//!   preserving the original XPos/ZPos/YOri decimal precision. A train's
//!   YPos is the ground placement height at that X/Z (same sample export
//!   uses); road columns are left at their template Y.
//! * `inspect_path_waypoints` / `park_path_waypoints` — off-road Goto WP hops
//!   (not RTB) so `mapground` can aim them at an objective or the front.
//! * `inspect_visual_heading` / `inspect_waypoint_xz` / `inspect_parked_network`
//!   — restore a already-exported column (including a curved road) so Map
//!   reimport can keep vehicle and waypoint world positions.
//!
//! ## Conventions
//! * Trains always use rail; non-train groups ride roads only when
//!   `inspect_route` accepts them as a column — otherwise `mapground`
//!   places them on open ground.
//! * `NetworkSpot.unit_xz[0]` is the lead. `reverse` means the column faces
//!   against the polyline direction; it is re-chosen on every drag to keep
//!   the user's heading.
//! * Once a waypoint has an explicit position, `sample_network` leaves it
//!   alone — WPs may sit on another branch or behind the column.
//!
//! ## Used by
//! * ui.rs (Map mode) — network line drawing, unit/WP dragging, heading drag.
//! * mapground.rs / recon.rs — `RouteLayout` on ground jobs; parking on export.
//! * placement.rs — supplies `heading_toward`, `mix_index`, `move_anchor_to`.
//!
//! Unit-tested against the `TemplateExamples/` `.Group` fixtures.

mod ini;

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::ast::Il2Entity;
use crate::geo::{MAP_MAX, MAP_MIN};
use crate::mapclip::WorldAabb;
use crate::placement::{heading_delta, heading_toward, mix_index, move_anchor_to};

const SVG_SIZE: f64 = 9984.0;
const M_PER_SVG: f64 = (MAP_MAX - MAP_MIN) / SVG_SIZE;
/// Keep network groups this far apart (metres).
pub const NETWORK_SPACING: f64 = 2_500.0;
const COLUMN_MIN_ALONG: f64 = 40.0;
const COLUMN_ACROSS_ABS: f64 = 25.0;
const COLUMN_ACROSS_FRAC: f64 = 0.025;
/// Max metres a parked column vehicle may sit off its road/rail polyline.
const PARKED_ON_LINE_M: f64 = 80.0;

#[derive(Clone, Debug)]
pub struct PolyLine {
    pub pts: Vec<(f64, f64)>,
    pub cum: Vec<f64>,
    pub x_min: f64,
    pub x_max: f64,
    pub z_min: f64,
    pub z_max: f64,
}

impl PolyLine {
    fn from_pts(pts: Vec<(f64, f64)>) -> Option<Self> {
        if pts.len() < 2 {
            return None;
        }
        let mut cum = Vec::with_capacity(pts.len());
        cum.push(0.0);
        let mut len = 0.0;
        let mut x_min = f64::MAX;
        let mut x_max = f64::MIN;
        let mut z_min = f64::MAX;
        let mut z_max = f64::MIN;
        for w in pts.windows(2) {
            len += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            cum.push(len);
        }
        for &(x, z) in &pts {
            x_min = x_min.min(x);
            x_max = x_max.max(x);
            z_min = z_min.min(z);
            z_max = z_max.max(z);
        }
        Some(Self {
            pts,
            cum,
            x_min,
            x_max,
            z_min,
            z_max,
        })
    }

    pub fn len(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }

    pub fn overlaps(&self, aabb: WorldAabb) -> bool {
        self.x_max >= aabb.x_min
            && self.x_min <= aabb.x_max
            && self.z_max >= aabb.z_min
            && self.z_min <= aabb.z_max
    }

    /// World pose at distance `dist` along the polyline. Heading follows the segment.
    pub fn at(&self, dist: f64) -> Option<(f64, f64, f64)> {
        let total = self.len();
        if total < 1.0 {
            return None;
        }
        let d = dist.clamp(0.0, total);
        let mut i = 0usize;
        while i + 1 < self.cum.len() && self.cum[i + 1] < d {
            i += 1;
        }
        let i = i.min(self.pts.len().saturating_sub(2));
        let span = (self.cum[i + 1] - self.cum[i]).max(1e-6);
        let t = ((d - self.cum[i]) / span).clamp(0.0, 1.0);
        let a = self.pts[i];
        let b = self.pts[i + 1];
        let x = a.0 + (b.0 - a.0) * t;
        let z = a.1 + (b.1 - a.1) * t;
        Some((x, z, heading_toward(a, b)))
    }
}

#[derive(Clone, Debug)]
pub struct Network {
    pub lines: Vec<PolyLine>,
}

impl Network {
    fn from_editor_ini(text: &str) -> Self {
        let lines = ini::parse_polylines(text)
            .into_iter()
            .filter_map(|image_pts| {
                let world = image_pts
                    .into_iter()
                    .map(|(sx, sy)| svg_to_world(sx, sy))
                    .collect();
                PolyLine::from_pts(world)
            })
            .collect();
        Self { lines }
    }

    pub fn nearest(&self, x: f64, z: f64) -> Option<Snap> {
        let mut best: Option<Snap> = None;
        let mut best_d2 = f64::MAX;
        for (li, line) in self.lines.iter().enumerate() {
            if line.pts.len() < 2 {
                continue;
            }
            for i in 0..line.pts.len() - 1 {
                let a = line.pts[i];
                let b = line.pts[i + 1];
                let (px, pz, t) = closest_on_segment(x, z, a, b);
                let d2 = (px - x) * (px - x) + (pz - z) * (pz - z);
                if d2 < best_d2 {
                    best_d2 = d2;
                    let seglen = (b.0 - a.0).hypot(b.1 - a.1);
                    best = Some(Snap {
                        line: li,
                        dist: line.cum[i] + t * seglen,
                        x: px,
                        z: pz,
                        heading: heading_toward(a, b),
                    });
                }
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Snap {
    pub line: usize,
    pub dist: f64,
    pub x: f64,
    pub z: f64,
    pub heading: f64,
}

/// Template layout: trains always use rail; a perfect column uses road.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteLayout {
    pub rail: bool,
    pub behind: Vec<f64>,
    pub wp_ahead: Vec<f64>,
    /// Zone IN radius (metres). Two-WP columns try to straddle this circle.
    pub zone_in_m: f64,
    /// Zone OUT radius (metres). A dead-end column's long hop stays inside it.
    pub zone_out_m: f64,
}

/// Live pose of a group parked on a polyline.
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkSpot {
    pub rail: bool,
    pub line: usize,
    pub lead_dist: f64,
    pub reverse: bool,
    pub behind: Vec<f64>,
    pub wp_ahead: Vec<f64>,
    pub zone_in_m: f64,
    pub zone_out_m: f64,
    pub unit_xz: Vec<(f64, f64)>,
    pub waypoints: Vec<(f64, f64)>,
}

pub fn roads() -> &'static Network {
    static N: OnceLock<Network> = OnceLock::new();
    N.get_or_init(|| Network::from_editor_ini(include_str!("../assets/gamedata/highways.ini")))
}

pub fn railroads() -> &'static Network {
    static N: OnceLock<Network> = OnceLock::new();
    N.get_or_init(|| Network::from_editor_ini(include_str!("../assets/gamedata/railroads.ini")))
}

/// True after [`warm_networks`] has finished. The map draws roads only then,
/// so the first map frame does not parse the ini files.
pub fn networks_ready() -> bool {
    NETWORKS_READY.load(std::sync::atomic::Ordering::Acquire)
}

/// Parse the road and rail files off the UI thread. Later `roads()` /
/// `railroads()` calls reuse that result.
pub fn warm_networks() {
    let _ = roads();
    let _ = railroads();
    NETWORKS_READY.store(true, std::sync::atomic::Ordering::Release);
}

static NETWORKS_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn network_for(rail: bool) -> &'static Network {
    if rail { railroads() } else { roads() }
}

/// Image pixel on the 9984 square to world X (north) / Z (east).
/// Same mapping the mission-editor SVG used: 50 m per image unit,
/// origin at the top-left.
pub fn svg_to_world(sx: f64, sy: f64) -> (f64, f64) {
    let z = MAP_MIN + sx * M_PER_SVG;
    let x = MAP_MAX - sy * M_PER_SVG;
    (x, z)
}

fn closest_on_segment(x: f64, z: f64, a: (f64, f64), b: (f64, f64)) -> (f64, f64, f64) {
    let dx = b.0 - a.0;
    let dz = b.1 - a.1;
    let len2 = dx * dx + dz * dz;
    if len2 < 1e-9 {
        return (a.0, a.1, 0.0);
    }
    let t = ((x - a.0) * dx + (z - a.1) * dz) / len2;
    let t = t.clamp(0.0, 1.0);
    (a.0 + dx * t, a.1 + dz * t, t)
}

fn heading_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

/// Average the unit vectors of two headings, then `atan2`. Degree averages
/// break across 0/360. A cancelled pair (a hairpin) keeps `a_deg`.
fn bisect_headings(a_deg: f64, b_deg: f64) -> f64 {
    let a = a_deg.to_radians();
    let b = b_deg.to_radians();
    let dx = a.cos() + b.cos();
    let dz = a.sin() + b.sin();
    if dx * dx + dz * dz < 1e-18 {
        return a_deg.rem_euclid(360.0);
    }
    dz.atan2(dx).to_degrees().rem_euclid(360.0)
}

/// Index of the polyline vertex within `tol` metres of `dist`, if any.
fn node_within(line: &PolyLine, dist: f64, tol: f64) -> Option<usize> {
    let mut best_i = None;
    let mut best_gap = tol;
    for (i, &c) in line.cum.iter().enumerate() {
        let gap = (dist - c).abs();
        if gap <= best_gap {
            best_gap = gap;
            best_i = Some(i);
        }
    }
    best_i
}

/// Heading the lead faces at `dist`.
///
/// A lead within 1 m of an interior node — a road vertex or a rail corner —
/// bisects the incoming and outgoing travel headings (`bisect_headings`).
/// Mid-segment, and an endpoint with only one segment, follow that one
/// segment (`segment_heading` from `PolyLine::at` / `Network::nearest`).
/// Reverse bisects the reversed pair, so the column still points along its
/// direction of travel, halfway through the bend.
///
/// `_rail` is unused. Roads and trains share this rule; callers still pass
/// the flag so each path says which network the lead came from.
fn lead_heading(
    line: &PolyLine,
    dist: f64,
    _rail: bool,
    reverse: bool,
    segment_heading: f64,
) -> f64 {
    if let Some(i) = node_within(line, dist, 1.0) {
        if i > 0 && i + 1 < line.pts.len() {
            let (into_travel, out_travel) = if reverse {
                (
                    heading_toward(line.pts[i + 1], line.pts[i]),
                    heading_toward(line.pts[i], line.pts[i - 1]),
                )
            } else {
                (
                    heading_toward(line.pts[i - 1], line.pts[i]),
                    heading_toward(line.pts[i], line.pts[i + 1]),
                )
            };
            return bisect_headings(into_travel, out_travel);
        }
    }
    if reverse {
        (segment_heading + 180.0).rem_euclid(360.0)
    } else {
        segment_heading
    }
}

fn is_rtb_waypoint(entity: &Il2Entity) -> bool {
    entity.name().is_some_and(|n| n.starts_with("RTB"))
}

/// Path `MCU_Waypoint` distances (metres) ahead of the lead visual, along facing.
/// Skips RTB hops. Empty when the group has no Goto WP path.
pub fn inspect_path_waypoints(root: &Il2Entity) -> Vec<f64> {
    let units = collect_route_units(root);
    if !units.is_empty() {
        return waypoint_ahead(root, &units);
    }
    let origin = root.first_xz().unwrap_or((0.0, 0.0));
    waypoint_ahead_from(root, origin, 0.0)
}

/// Vehicles / trains with a Model, plus MCU_Waypoint distances along facing.
pub fn inspect_route(root: &Il2Entity) -> Option<RouteLayout> {
    let rail = root.count_block_type("Train") > 0;
    let units = collect_route_units(root);
    if units.is_empty() {
        return None;
    }
    if !rail && !is_perfect_column(&units) {
        return None;
    }
    let (behind, _) = column_gaps(&units);
    let wp_ahead = waypoint_ahead(root, &units);
    let zone_in_m = inspect_zone_in(root).unwrap_or(if rail { 19_000.0 } else { 10_000.0 });
    let zone_out_m = inspect_zone_out(root).unwrap_or(0.0);
    Some(RouteLayout {
        rail,
        behind,
        wp_ahead,
        zone_in_m,
        zone_out_m,
    })
}

/// Circular mean of `YOri` on every object with a Model (vehicles, trains, ships).
pub fn inspect_visual_heading(root: &Il2Entity) -> f64 {
    let mut cx = 0.0;
    let mut cz = 0.0;
    let mut n = 0.0;
    root.for_each(&mut |e| {
        if e.property("Model").is_none() {
            return;
        }
        let yori: f64 = e
            .property("YOri")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let r = yori.to_radians();
        cx += r.cos();
        cz += r.sin();
        n += 1.0;
    });
    if n < 1.0 {
        return 0.0;
    }
    cz.atan2(cx).to_degrees().rem_euclid(360.0)
}

/// World X/Z of path `MCU_Waypoint`s (not RTB), in Index order.
pub fn inspect_waypoint_xz(root: &Il2Entity) -> Vec<(f64, f64)> {
    let mut ids_pts = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "MCU_Waypoint" || is_rtb_waypoint(e) {
            return;
        }
        if let (Some(id), Some(p)) = (e.index, e.pos_xz()) {
            ids_pts.push((id, p));
        }
    });
    ids_pts.sort_by_key(|(id, _)| *id);
    ids_pts.into_iter().map(|(_, p)| p).collect()
}

/// Restore a column that is already sitting on a road or railroad (including a curve).
///
/// Straight authored templates often fail this (they are not on the network).
/// After Map export, vehicles follow the polyline, so `inspect_route`'s
/// perfect-column test can miss them — this snaps the lead and checks that
/// every vehicle still lies on the same line.
pub fn inspect_parked_network(root: &Il2Entity) -> Option<NetworkSpot> {
    let rail = root.count_block_type("Train") > 0;
    let units = collect_route_units(root);
    if units.is_empty() {
        return None;
    }
    if !rail && units.len() < 2 {
        return None;
    }
    let heading = mean_heading(&units);
    let n = units.len() as f64;
    let cx = units.iter().map(|u| u.x).sum::<f64>() / n;
    let cz = units.iter().map(|u| u.z).sum::<f64>() / n;
    let mut keyed: Vec<(f64, usize)> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (project_along(u.x, u.z, (cx, cz), heading).0, i))
        .collect();
    keyed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let lead = &units[keyed[0].1];
    let net = network_for(rail);
    let snap = net.nearest(lead.x, lead.z)?;
    let line = net.lines.get(snap.line)?;
    if polyline_dist(line, lead.x, lead.z) > PARKED_ON_LINE_M {
        return None;
    }
    if units
        .iter()
        .any(|u| polyline_dist(line, u.x, u.z) > PARKED_ON_LINE_M)
    {
        return None;
    }
    let reverse = heading_delta(snap.heading, heading).abs() > 90.0;
    let lead_along = keyed[0].0;
    let behind: Vec<f64> = keyed
        .iter()
        .skip(1)
        .map(|(a, _)| (lead_along - *a).max(0.0))
        .collect();
    let mut unit_xz = vec![(lead.x, lead.z)];
    for &(_, i) in keyed.iter().skip(1) {
        unit_xz.push((units[i].x, units[i].z));
    }
    Some(NetworkSpot {
        rail,
        line: snap.line,
        lead_dist: snap.dist,
        reverse,
        behind,
        wp_ahead: waypoint_ahead(root, &units),
        zone_in_m: inspect_zone_in(root).unwrap_or(if rail { 19_000.0 } else { 10_000.0 }),
        zone_out_m: inspect_zone_out(root).unwrap_or(0.0),
        unit_xz,
        waypoints: inspect_waypoint_xz(root),
    })
}

fn polyline_dist(line: &PolyLine, x: f64, z: f64) -> f64 {
    let mut best = f64::MAX;
    for w in line.pts.windows(2) {
        let (px, pz, _) = closest_on_segment(x, z, w[0], w[1]);
        let d2 = (px - x) * (px - x) + (pz - z) * (pz - z);
        if d2 < best {
            best = d2;
        }
    }
    best.sqrt()
}

fn inspect_zone_in(root: &Il2Entity) -> Option<f64> {
    root.find_by_name("Zone IN")
        .and_then(|z| z.property("Zone"))
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|r| *r > 0.0)
}

fn inspect_zone_out(root: &Il2Entity) -> Option<f64> {
    let mut found = None;
    root.for_each(&mut |entity| {
        if found.is_some() || entity.block_type != "MCU_CheckZone" {
            return;
        }
        let Some(name) = entity.name() else { return };
        if name.eq_ignore_ascii_case("zone out") {
            found = entity
                .property("Zone")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|r| *r > 0.0);
        }
    });
    found
}

struct RouteUnit {
    index: Option<i32>,
    x: f64,
    z: f64,
    yori: f64,
    link: Option<i32>,
}

fn collect_route_units(root: &Il2Entity) -> Vec<RouteUnit> {
    let mut out = Vec::new();
    root.for_each(&mut |e| {
        if !matches!(e.block_type.as_str(), "Vehicle" | "Train") {
            return;
        }
        if e.property("Model").is_none() {
            return;
        }
        let Some((x, z)) = e.pos_xz() else { return };
        let yori = e
            .property("YOri")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let link = e.property("LinkTrId").and_then(|v| v.parse().ok());
        out.push(RouteUnit {
            index: e.index,
            x,
            z,
            yori,
            link,
        });
    });
    out
}

fn mean_heading(units: &[RouteUnit]) -> f64 {
    if units.is_empty() {
        return 0.0;
    }
    let mut cx = 0.0;
    let mut cz = 0.0;
    for u in units {
        let r = u.yori.to_radians();
        cx += r.cos();
        cz += r.sin();
    }
    cz.atan2(cx).to_degrees().rem_euclid(360.0)
}

fn project_along(x: f64, z: f64, origin: (f64, f64), heading_deg: f64) -> (f64, f64) {
    let r = heading_deg.to_radians();
    let (sin, cos) = (r.sin(), r.cos());
    let dx = x - origin.0;
    let dz = z - origin.1;
    (dx * cos + dz * sin, -dx * sin + dz * cos)
}

fn is_perfect_column(units: &[RouteUnit]) -> bool {
    if units.len() < 2 {
        return false;
    }
    let heading = mean_heading(units);
    let n = units.len() as f64;
    let cx = units.iter().map(|u| u.x).sum::<f64>() / n;
    let cz = units.iter().map(|u| u.z).sum::<f64>() / n;
    let mut along_min = f64::MAX;
    let mut along_max = f64::MIN;
    let mut across_max = 0.0_f64;
    for u in units {
        let (along, across) = project_along(u.x, u.z, (cx, cz), heading);
        along_min = along_min.min(along);
        along_max = along_max.max(along);
        across_max = across_max.max(across.abs());
    }
    let along_span = along_max - along_min;
    let across_lim = COLUMN_ACROSS_ABS.max(COLUMN_ACROSS_FRAC * along_span);
    along_span >= COLUMN_MIN_ALONG && along_span >= across_max * 4.0 && across_max <= across_lim
}

fn column_gaps(units: &[RouteUnit]) -> (Vec<f64>, f64) {
    let heading = mean_heading(units);
    let n = units.len() as f64;
    let cx = units.iter().map(|u| u.x).sum::<f64>() / n;
    let cz = units.iter().map(|u| u.z).sum::<f64>() / n;
    let mut keyed: Vec<(f64, usize)> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (project_along(u.x, u.z, (cx, cz), heading).0, i))
        .collect();
    keyed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let lead_along = keyed[0].0;
    let behind: Vec<f64> = keyed
        .iter()
        .skip(1)
        .map(|(a, _)| (lead_along - *a).max(0.0))
        .collect();
    (behind, heading)
}

fn waypoint_ahead(root: &Il2Entity, units: &[RouteUnit]) -> Vec<f64> {
    if units.is_empty() {
        return Vec::new();
    }
    let heading = mean_heading(units);
    let n = units.len() as f64;
    let cx = units.iter().map(|u| u.x).sum::<f64>() / n;
    let cz = units.iter().map(|u| u.z).sum::<f64>() / n;
    let mut keyed: Vec<(f64, &RouteUnit)> = units
        .iter()
        .map(|u| (project_along(u.x, u.z, (cx, cz), heading).0, u))
        .collect();
    keyed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let lead = keyed[0].1;
    waypoint_ahead_from(root, (lead.x, lead.z), heading)
}

fn waypoint_ahead_from(root: &Il2Entity, origin: (f64, f64), heading_deg: f64) -> Vec<f64> {
    let mut dists = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "MCU_Waypoint" || is_rtb_waypoint(e) {
            return;
        }
        let Some((x, z)) = e.pos_xz() else { return };
        let (along, _) = project_along(x, z, origin, heading_deg);
        let d = along.abs();
        if d >= 50.0 {
            dists.push(d);
        }
    });
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dists
}

fn route_need(route: &RouteLayout) -> (f64, f64) {
    let behind = route.behind.last().copied().unwrap_or(0.0);
    let ahead = route.wp_ahead.last().copied().unwrap_or(0.0);
    (behind, ahead)
}

/// Roads (or rails) that share a vertex. Built once for the Korea networks.
struct JunctionGraph {
    /// `links[line][vertex]` lists other polylines that meet this point.
    links: Vec<Vec<Vec<(usize, usize)>>>,
}

const JOIN_CELL_M: f64 = 10.0;
const JOIN_M: f64 = 8.0;

impl JunctionGraph {
    fn build(net: &Network) -> Self {
        let mut buckets: HashMap<(i32, i32), Vec<(usize, usize)>> = HashMap::new();
        for (li, line) in net.lines.iter().enumerate() {
            for (vi, &(x, z)) in line.pts.iter().enumerate() {
                let key = (
                    (x / JOIN_CELL_M).round() as i32,
                    (z / JOIN_CELL_M).round() as i32,
                );
                buckets.entry(key).or_default().push((li, vi));
            }
        }
        let mut links: Vec<Vec<Vec<(usize, usize)>>> = net
            .lines
            .iter()
            .map(|line| vec![Vec::new(); line.pts.len()])
            .collect();
        for (&(cx, cz), verts) in &buckets {
            let mut near = Vec::new();
            for dx in -1..=1 {
                for dz in -1..=1 {
                    if let Some(cell) = buckets.get(&(cx + dx, cz + dz)) {
                        near.extend(cell.iter().copied());
                    }
                }
            }
            for &(li, vi) in verts {
                let (x, z) = net.lines[li].pts[vi];
                for &(oi, ov) in &near {
                    if oi == li {
                        continue;
                    }
                    let (ox, oz) = net.lines[oi].pts[ov];
                    if (x - ox).hypot(z - oz) > JOIN_M {
                        continue;
                    }
                    let slot = &mut links[li][vi];
                    if !slot.contains(&(oi, ov)) {
                        slot.push((oi, ov));
                    }
                }
            }
        }
        Self { links }
    }
}

fn road_junctions() -> &'static JunctionGraph {
    static G: OnceLock<JunctionGraph> = OnceLock::new();
    G.get_or_init(|| JunctionGraph::build(roads()))
}

fn rail_junctions() -> &'static JunctionGraph {
    static G: OnceLock<JunctionGraph> = OnceLock::new();
    G.get_or_init(|| JunctionGraph::build(railroads()))
}

fn with_junctions<R>(net: &Network, f: impl FnOnce(&JunctionGraph) -> R) -> R {
    if std::ptr::eq(net, roads()) {
        f(road_junctions())
    } else if std::ptr::eq(net, railroads()) {
        f(rail_junctions())
    } else {
        f(&JunctionGraph::build(net))
    }
}

fn inside_radius(line: &PolyLine, dist: f64, origin: (f64, f64), radius: f64) -> bool {
    let Some((x, z, _)) = line.at(dist) else {
        return false;
    };
    (x - origin.0).hypot(z - origin.1) <= radius
}

/// Last point on the segment from `from` toward `to` that is still inside the circle.
fn last_inside(line: &PolyLine, from: f64, to: f64, origin: (f64, f64), radius: f64) -> f64 {
    if !inside_radius(line, from, origin, radius) {
        return from;
    }
    if inside_radius(line, to, origin, radius) {
        return to;
    }
    let mut lo = from;
    let mut hi = to;
    for _ in 0..28 {
        let mid = (lo + hi) * 0.5;
        if inside_radius(line, mid, origin, radius) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn spur_end(line: &PolyLine, dist: f64, dir: i8, origin: (f64, f64), radius: f64) -> (f64, f64) {
    let end = if dir > 0 { line.len() } else { 0.0 };
    let hit = last_inside(line, dist, end, origin, radius);
    line.at(hit).map(|(x, z, _)| (x, z)).unwrap_or(origin)
}

fn next_vertex(line: &PolyLine, dist: f64, dir: i8) -> Option<usize> {
    if dir > 0 {
        line.cum
            .iter()
            .enumerate()
            .find(|(_, c)| **c > dist + 0.5)
            .map(|(i, _)| i)
    } else {
        line.cum
            .iter()
            .enumerate()
            .rfind(|(_, c)| **c < dist - 0.5)
            .map(|(i, _)| i)
    }
}

struct SideWalk {
    line: usize,
    dist: f64,
    dir: i8,
    arc: f64,
    path: HashSet<(usize, usize)>,
}

/// Longest road (or rail) walk from the lead in `dir` that stays inside `radius`.
fn farthest_on_side(
    net: &Network,
    graph: &JunctionGraph,
    line_i: usize,
    start_dist: f64,
    dir: i8,
    origin: (f64, f64),
    radius: f64,
) -> (f64, f64) {
    let line = &net.lines[line_i];
    let end = if dir > 0 { line.len() } else { 0.0 };
    let spur_hit = last_inside(line, start_dist, end, origin, radius);
    let mut best = line.at(spur_hit).map(|(x, z, _)| (x, z)).unwrap_or(origin);
    let mut best_arc = (spur_hit - start_dist).abs();
    let mut stack = vec![SideWalk {
        line: line_i,
        dist: start_dist,
        dir,
        arc: 0.0,
        path: HashSet::new(),
    }];
    let mut budget = 4_000i32;
    while let Some(mut walk) = stack.pop() {
        loop {
            if budget <= 0 || walk.arc > radius * 6.0 {
                break;
            }
            budget -= 1;
            let line = &net.lines[walk.line];
            let Some(vi) = next_vertex(line, walk.dist, walk.dir) else {
                break;
            };
            let vend = line.cum[vi];
            let hit = last_inside(line, walk.dist, vend, origin, radius);
            let arc_here = walk.arc + (hit - walk.dist).abs();
            if let Some((x, z, _)) = line.at(hit) {
                if arc_here > best_arc + 0.5 {
                    best_arc = arc_here;
                    best = (x, z);
                }
            }
            if (hit - vend).abs() > 1.0 {
                break;
            }
            if walk.path.len() > 256 || !walk.path.insert((walk.line, vi)) {
                break;
            }
            if let Some(exits) = graph.links.get(walk.line).and_then(|row| row.get(vi)) {
                for &(ol, ov) in exits {
                    if walk.path.contains(&(ol, ov)) {
                        continue;
                    }
                    let other = &net.lines[ol];
                    let od = other.cum[ov];
                    if ov + 1 < other.pts.len() {
                        stack.push(SideWalk {
                            line: ol,
                            dist: od,
                            dir: 1,
                            arc: arc_here,
                            path: walk.path.clone(),
                        });
                    }
                    if ov > 0 {
                        stack.push(SideWalk {
                            line: ol,
                            dist: od,
                            dir: -1,
                            arc: arc_here,
                            path: walk.path.clone(),
                        });
                    }
                }
            }
            walk.dist = vend;
            walk.arc = arc_here;
        }
    }
    best
}

/// World positions for a column's waypoints.
///
/// Two hops straddle the Zone IN rim when that path exists on this polyline.
/// Otherwise each side follows connected roads to the Zone OUT rim (or Zone
/// IN, when the group has no Zone OUT). A side that ends inside the circle,
/// while the other side still reaches the rim, stops at the end of its spur.
fn route_waypoints(net: &Network, line: &PolyLine, pose: &NetworkSpot) -> Vec<(f64, f64)> {
    let total = line.len();
    let dists = column_waypoint_dists(
        pose.zone_in_m,
        &pose.wp_ahead,
        pose.lead_dist,
        pose.reverse,
        total,
    );
    let edge = (pose.zone_in_m - 200.0).max(500.0);
    let ahead_at = if pose.reverse {
        pose.lead_dist - edge
    } else {
        pose.lead_dist + edge
    };
    let behind_at = if pose.reverse {
        pose.lead_dist + edge
    } else {
        pose.lead_dist - edge
    };
    let straddles = pose.wp_ahead.len() == 2
        && pose.zone_in_m > 500.0
        && dists.len() == 2
        && (dists[0] - ahead_at).abs() < 1.0
        && (dists[1] - behind_at).abs() < 1.0;
    if straddles || pose.wp_ahead.len() != 2 || pose.zone_in_m <= 500.0 {
        return dists
            .iter()
            .filter_map(|&d| line.at(d).map(|(x, z, _)| (x, z)))
            .collect();
    }
    let Some(origin) = line.at(pose.lead_dist).map(|(x, z, _)| (x, z)) else {
        return Vec::new();
    };
    let limit_m = if pose.zone_out_m > 500.0 {
        pose.zone_out_m
    } else {
        pose.zone_in_m
    };
    let limit = (limit_m - 200.0).max(500.0);
    let ahead_dir: i8 = if pose.reverse { -1 } else { 1 };
    let behind_dir = -ahead_dir;
    let on_rim = |point: (f64, f64)| (point.0 - origin.0).hypot(point.1 - origin.1) >= limit - 5.0;
    with_junctions(net, |graph| {
        let ahead_far = farthest_on_side(
            net,
            graph,
            pose.line,
            pose.lead_dist,
            ahead_dir,
            origin,
            limit,
        );
        let behind_far = farthest_on_side(
            net,
            graph,
            pose.line,
            pose.lead_dist,
            behind_dir,
            origin,
            limit,
        );
        // A side that dies inside the circle, while the other side can still
        // reach the rim, stops at the end of this spur. The open side keeps
        // the longest connected road that stays inside the circle.
        let ahead_open = on_rim(ahead_far);
        let behind_open = on_rim(behind_far);
        let ahead = if !ahead_open && behind_open {
            spur_end(line, pose.lead_dist, ahead_dir, origin, limit)
        } else {
            ahead_far
        };
        let behind = if !behind_open && ahead_open {
            spur_end(line, pose.lead_dist, behind_dir, origin, limit)
        } else {
            behind_far
        };
        vec![ahead, behind]
    })
}

/// Distances along the polyline for each authored waypoint.
///
/// A single-column group with exactly two waypoints tries to sit one hop near
/// the Zone IN rim ahead of the lead and the other near the opposite rim
/// behind, when both points fall on the path. Otherwise hops stay in order
/// ahead of the lead (the authored offsets). `sample_network` replaces that
/// fallback with a dead-end spur and a connected-road run out to Zone OUT.
pub fn column_waypoint_dists(
    zone_in_m: f64,
    authored_ahead: &[f64],
    lead_dist: f64,
    reverse: bool,
    line_len: f64,
) -> Vec<f64> {
    let ahead_at = |delta: f64| {
        if reverse {
            lead_dist - delta
        } else {
            lead_dist + delta
        }
    };
    let dist_at = |delta: f64| {
        if reverse {
            lead_dist + delta
        } else {
            lead_dist - delta
        }
    };
    let on_line = |d: f64| d >= 0.0 && d <= line_len;
    if authored_ahead.len() == 2 && zone_in_m > 500.0 {
        let edge = (zone_in_m - 200.0).max(500.0);
        let ad = ahead_at(edge);
        let bd = dist_at(edge);
        if on_line(ad) && on_line(bd) {
            return vec![ad, bd];
        }
    }
    authored_ahead.iter().map(|&d| ahead_at(d)).collect()
}

/// Fill unit poses from a polyline. Existing waypoint world positions stay put
/// so a WP can sit on another branch or behind the column.
///
/// The returned heading is the lead's. Within 1 m of an interior node it is
/// the corner bisector (`lead_heading`); mid-segment and endpoints keep the
/// segment heading. Waypoint distances are not moved.
pub fn sample_network(net: &Network, pose: &mut NetworkSpot) -> Option<(f64, f64, f64)> {
    let line = net.lines.get(pose.line)?;
    let behind_need = pose.behind.last().copied().unwrap_or(0.0);
    let pin_wps = !pose.waypoints.is_empty();
    let ahead_need = if pin_wps {
        0.0
    } else {
        pose.wp_ahead.last().copied().unwrap_or(0.0)
    };
    let total = line.len();
    let (lo, hi) = if pose.reverse {
        (ahead_need + 20.0, total - behind_need - 20.0)
    } else {
        (behind_need + 20.0, total - ahead_need - 20.0)
    };
    if hi <= lo {
        return None;
    }
    pose.lead_dist = pose.lead_dist.clamp(lo, hi);
    let lead = line.at(pose.lead_dist)?;
    let heading = lead_heading(line, pose.lead_dist, pose.rail, pose.reverse, lead.2);
    let dist_at = |delta: f64| {
        if pose.reverse {
            pose.lead_dist + delta
        } else {
            pose.lead_dist - delta
        }
    };
    let mut unit_xz = vec![(lead.0, lead.1)];
    for &d in &pose.behind {
        let p = line.at(dist_at(d))?;
        unit_xz.push((p.0, p.1));
    }
    pose.unit_xz = unit_xz;
    if !pin_wps {
        pose.waypoints = route_waypoints(net, line, pose);
    }
    Some((lead.0, lead.1, heading))
}

pub fn place_route(
    route: &RouteLayout,
    aabb: WorldAabb,
    seed: u64,
    index: usize,
    occupied: &[(f64, f64)],
    prefer: impl Fn(f64, f64) -> bool,
) -> Option<((f64, f64, f64, bool), NetworkSpot)> {
    place_route_filtered(route, aabb, seed, index, occupied, &prefer)
}

/// Distance along `line` at the middle of each segment that falls in `(lo, hi)`.
///
/// Waypoints are measured from this midpoint. The locomotive is moved onto
/// a node of the same segment afterwards; this anchor is not moved.
fn segment_mid_dists(
    line: &PolyLine,
    lo: f64,
    hi: f64,
    prefer: &impl Fn(f64, f64) -> bool,
) -> Vec<f64> {
    let mut mids = Vec::new();
    for i in 0..line.pts.len().saturating_sub(1) {
        let a = line.cum[i];
        let b = line.cum[i + 1];
        let left = a.max(lo);
        let right = b.min(hi);
        if right - left < 2.0 {
            continue;
        }
        let mid = (a + b) * 0.5;
        let d = if mid > left && mid < right {
            mid
        } else {
            (left + right) * 0.5
        };
        let Some((x, z, _)) = line.at(d) else {
            continue;
        };
        if prefer(x, z) {
            mids.push(d);
        }
    }
    mids
}

/// Segment index `PolyLine::at` uses for `dist`.
fn segment_index(line: &PolyLine, dist: f64) -> Option<usize> {
    if line.len() < 1.0 || line.pts.len() < 2 {
        return None;
    }
    let d = dist.clamp(0.0, line.len());
    let mut i = 0usize;
    while i + 1 < line.cum.len() && line.cum[i + 1] < d {
        i += 1;
    }
    Some(i.min(line.pts.len().saturating_sub(2)))
}

/// Rail node of the segment that contains `mid`. Interior corners win, then
/// a node inside `[lo, hi]`, then the one closer to `mid`.
fn locomotive_node_dist(line: &PolyLine, mid: f64, lo: f64, hi: f64) -> Option<f64> {
    let i = segment_index(line, mid)?;
    let mut nodes = vec![i, i + 1];
    let interior: Vec<usize> = nodes
        .iter()
        .copied()
        .filter(|&n| n > 0 && n + 1 < line.pts.len())
        .collect();
    if !interior.is_empty() {
        nodes = interior;
    }
    let in_window: Vec<usize> = nodes
        .iter()
        .copied()
        .filter(|&n| line.cum[n] >= lo && line.cum[n] <= hi)
        .collect();
    if !in_window.is_empty() {
        nodes = in_window;
    }
    let best = nodes.into_iter().min_by(|n1, n2| {
        let d1 = (line.cum[*n1] - mid).abs();
        let d2 = (line.cum[*n2] - mid).abs();
        d1.partial_cmp(&d2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(n1.cmp(n2))
    })?;
    Some(line.cum[best])
}

fn place_route_filtered(
    route: &RouteLayout,
    aabb: WorldAabb,
    seed: u64,
    index: usize,
    occupied: &[(f64, f64)],
    prefer: &impl Fn(f64, f64) -> bool,
) -> Option<((f64, f64, f64, bool), NetworkSpot)> {
    let net = network_for(route.rail);
    let (behind_need, ahead_need) = route_need(route);
    let need = behind_need + ahead_need + 80.0;
    let mut candidates: Vec<usize> = net
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.len() >= need && l.overlaps(aabb))
        .map(|(i, _)| i)
        .collect();
    if candidates.is_empty() {
        candidates = net
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.len() >= need)
            .map(|(i, _)| i)
            .collect();
    }
    if candidates.is_empty() {
        return None;
    }
    let min_d2 = NETWORK_SPACING * NETWORK_SPACING;
    let far = |x: f64, z: f64| {
        occupied.iter().all(|&(ox, oz)| {
            let dx = x - ox;
            let dz = z - oz;
            dx * dx + dz * dz >= min_d2
        })
    };
    for attempt in 0..64u64 {
        let mix = mix_index(seed, index as u64 * 64 + attempt);
        let li = candidates[(mix as usize) % candidates.len()];
        let line = &net.lines[li];
        let reverse = mix_index(seed, index as u64 * 3 + attempt + 11) & 1 == 1;
        let (lo, hi) = if reverse {
            (ahead_need + 30.0, line.len() - behind_need - 30.0)
        } else {
            (behind_need + 30.0, line.len() - ahead_need - 30.0)
        };
        if hi <= lo {
            continue;
        }
        let lead_dist = if route.rail {
            let mids = segment_mid_dists(line, lo, hi, prefer);
            if mids.is_empty() {
                continue;
            }
            mids[(mix as usize) % mids.len()]
        } else {
            let verts: Vec<f64> = line
                .pts
                .iter()
                .zip(line.cum.iter())
                .filter_map(|(&(x, z), &d)| {
                    if d >= lo && d <= hi && prefer(x, z) {
                        Some(d)
                    } else {
                        None
                    }
                })
                .collect();
            if verts.is_empty() {
                let t = (mix_index(seed, attempt.wrapping_mul(0x9E37) ^ index as u64) as f64)
                    / (u64::MAX as f64);
                lo + t * (hi - lo)
            } else {
                verts[(mix as usize) % verts.len()]
            }
        };
        let mut pose = NetworkSpot {
            rail: route.rail,
            line: li,
            lead_dist,
            reverse,
            behind: route.behind.clone(),
            wp_ahead: route.wp_ahead.clone(),
            zone_in_m: route.zone_in_m,
            zone_out_m: route.zone_out_m,
            unit_xz: Vec::new(),
            waypoints: Vec::new(),
        };
        let Some((mut x, mut z, mut heading)) = sample_network(net, &mut pose) else {
            continue;
        };
        if route.rail {
            let Some(node_d) = locomotive_node_dist(line, pose.lead_dist, lo, hi) else {
                continue;
            };
            let Some((nx, nz, seg_h)) = line.at(node_d) else {
                continue;
            };
            x = nx;
            z = nz;
            heading = lead_heading(line, node_d, true, pose.reverse, seg_h);
            pose.lead_dist = node_d;
            if let Some(lead) = pose.unit_xz.first_mut() {
                *lead = (nx, nz);
            }
        }
        if !far(x, z) || !prefer(x, z) {
            continue;
        }
        let in_ao = aabb.contains(x, z);
        if attempt < 48 && !in_ao {
            continue;
        }
        return Some(((x, z, heading, in_ao), pose));
    }
    None
}

pub fn snap_lead_to_pointer(
    pose: &mut NetworkSpot,
    x: f64,
    z: f64,
    keep_heading: f64,
) -> Option<(f64, f64, f64)> {
    let net = network_for(pose.rail);
    let snap = net.nearest(x, z)?;
    let rev_h = (snap.heading + 180.0).rem_euclid(360.0);
    pose.reverse = heading_diff(keep_heading, rev_h) < heading_diff(keep_heading, snap.heading);
    pose.line = snap.line;
    pose.lead_dist = snap.dist;
    sample_network(net, pose)
}

pub fn snap_waypoint_to_pointer(pose: &mut NetworkSpot, wp_i: usize, x: f64, z: f64) -> bool {
    let net = network_for(pose.rail);
    let Some(snap) = net.nearest(x, z) else {
        return false;
    };
    if wp_i >= pose.waypoints.len() {
        pose.waypoints.resize(wp_i + 1, (snap.x, snap.z));
    }
    pose.waypoints[wp_i] = (snap.x, snap.z);
    true
}

pub fn align_heading_to_path(pose: &mut NetworkSpot, requested: f64) -> Option<(f64, f64, f64)> {
    let net = network_for(pose.rail);
    let lead = net.lines.get(pose.line)?.at(pose.lead_dist)?;
    let rev = (lead.2 + 180.0).rem_euclid(360.0);
    pose.reverse = heading_diff(requested, rev) < heading_diff(requested, lead.2);
    sample_network(net, pose)
}

fn set_xz(entity: &mut Il2Entity, x: f64, z: f64) {
    let decimals = entity
        .property("XPos")
        .and_then(|v| v.split('.').nth(1))
        .map(|s| s.len())
        .unwrap_or(3);
    entity.set_property("XPos", format!("{x:.decimals$}"));
    entity.set_property("ZPos", format!("{z:.decimals$}"));
}

fn set_yori_abs(entity: &mut Il2Entity, heading: f64) {
    let decimals = entity
        .property("YOri")
        .and_then(|v| v.split('.').nth(1))
        .map(|s| s.len())
        .unwrap_or(3);
    entity.set_property("YOri", format!("{:.decimals$}", heading.rem_euclid(360.0)));
}

/// Move path `MCU_Waypoint` hops (not RTB) onto `waypoints`, matching Index order.
pub fn park_path_waypoints(root: &mut Il2Entity, waypoints: &[(f64, f64)]) {
    if waypoints.is_empty() {
        return;
    }
    let mut wp_ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_Waypoint" && !is_rtb_waypoint(e) {
            if let Some(id) = e.index {
                wp_ids.push(id);
            }
        }
    });
    wp_ids.sort_unstable();
    root.for_each_mut(&mut |e| {
        if e.block_type != "MCU_Waypoint" {
            return;
        }
        let Some(id) = e.index else { return };
        if let Some(pi) = wp_ids.iter().position(|&w| w == id) {
            if let Some(&(x, z)) = waypoints.get(pi) {
                set_xz(e, x, z);
            }
        }
    });
}

/// Translate the group so the lead visual sits on the path, then snap each
/// vehicle/train and MCU_Waypoint onto `unit_xz` / `waypoints`.
///
/// A train's `YPos`, and its linked entity (same delta, so the entity keeps
/// its offset), come from the ground placement height at the parked X/Z.
/// Waypoint X/Z and road-column Y are left as they were.
pub fn park_route_copy(
    root: &mut Il2Entity,
    lead: (f64, f64),
    heading_deg: f64,
    unit_xz: &[(f64, f64)],
    waypoints: &[(f64, f64)],
) {
    if unit_xz.is_empty() {
        return;
    }
    let units = collect_route_units(root);
    if units.is_empty() {
        let from = root.first_xz().unwrap_or((0.0, 0.0));
        move_anchor_to(root, from, lead);
        return;
    }
    let heading = mean_heading(&units);
    let n = units.len() as f64;
    let cx = units.iter().map(|u| u.x).sum::<f64>() / n;
    let cz = units.iter().map(|u| u.z).sum::<f64>() / n;
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by(|a, b| {
        let aa = project_along(units[*a].x, units[*a].z, (cx, cz), heading).0;
        let bb = project_along(units[*b].x, units[*b].z, (cx, cz), heading).0;
        bb.partial_cmp(&aa).unwrap_or(std::cmp::Ordering::Equal)
    });
    let lead_from = (units[order[0]].x, units[order[0]].z);
    move_anchor_to(root, lead_from, lead);

    let mut link_pos: Vec<(i32, f64, f64)> = Vec::new();
    let mut by_index: Vec<(i32, f64, f64)> = Vec::new();
    for (k, &ui) in order.iter().enumerate() {
        let Some(&(x, z)) = unit_xz.get(k) else { break };
        if let Some(id) = units[ui].index {
            by_index.push((id, x, z));
        }
        if let Some(link) = units[ui].link {
            link_pos.push((link, x, z));
        }
    }
    let mut wp_ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_Waypoint" {
            if let Some(id) = e.index {
                wp_ids.push(id);
            }
        }
    });
    wp_ids.sort_unstable();

    root.for_each_mut(&mut |e| {
        if let Some(id) = e.index {
            if let Some((_, x, z)) = by_index.iter().find(|(i, _, _)| *i == id) {
                set_xz(e, *x, *z);
                set_yori_abs(e, heading_deg);
            }
            if let Some((_, x, z)) = link_pos.iter().find(|(i, _, _)| *i == id) {
                set_xz(e, *x, *z);
                set_yori_abs(e, heading_deg);
            }
            if e.block_type == "MCU_Waypoint" {
                if let Some(pi) = wp_ids.iter().position(|&w| w == id) {
                    if let Some(&(x, z)) = waypoints.get(pi) {
                        set_xz(e, x, z);
                    }
                }
            }
        }
    });
    seat_parked_train_heights(root);
}

fn korea_height_store() -> Option<&'static crate::terrain::HeightStore> {
    static STORE: OnceLock<Option<crate::terrain::HeightStore>> = OnceLock::new();
    STORE
        .get_or_init(|| crate::terrain::open_store(&crate::terrain::default_store_path()).ok())
        .as_ref()
}

/// Trains keep the template altitude unless this writes the ground sample.
/// Road columns have no `Train` block, so their Y is untouched.
fn seat_parked_train_heights(root: &mut Il2Entity) {
    if root.count_block_type("Train") == 0 {
        return;
    }
    let Some(store) = korea_height_store() else {
        return;
    };
    struct Hit {
        index: Option<i32>,
        old_y: f64,
        new_y: f64,
        link: Option<i32>,
    }
    let mut hits = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "Train" {
            return;
        }
        let Some((x, z)) = e.pos_xz() else { return };
        let Some(new_y) = crate::terrain_apply::ground_placement_y(store, x, z) else {
            return;
        };
        let old_y = e
            .property("YPos")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(new_y);
        let link = e
            .property("LinkTrId")
            .and_then(|v| v.trim().parse().ok())
            .filter(|id: &i32| *id > 0);
        hits.push(Hit {
            index: e.index,
            old_y,
            new_y,
            link,
        });
    });
    if hits.is_empty() {
        return;
    }
    root.for_each_mut(&mut |e| {
        if e.block_type == "Train" {
            if let Some(hit) = hits
                .iter()
                .find(|h| h.index.is_some() && h.index == e.index)
            {
                e.set_ypos(hit.new_y);
            }
        } else if e.block_type == "MCU_TR_Entity" {
            if let Some(id) = e.index {
                if let Some(hit) = hits.iter().find(|h| h.link == Some(id)) {
                    if let Some(ey) = e
                        .property("YPos")
                        .and_then(|v| v.trim().parse::<f64>().ok())
                    {
                        e.set_ypos(ey + (hit.new_y - hit.old_y));
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;

    #[test]
    fn editor_ini_polylines_map_to_korea_world() {
        let rails = ini::parse_polylines(include_str!("../assets/gamedata/railroads.ini"));
        let roads_img = ini::parse_polylines(include_str!("../assets/gamedata/highways.ini"));
        assert_eq!(rails.len(), 382, "rail polylines {}", rails.len());
        assert_eq!(roads_img.len(), 1477, "road polylines {}", roads_img.len());
        let (sx, sy) = rails[0][0];
        let (x, z) = svg_to_world(sx, sy);
        assert!((z - sx * 50.0).abs() < 0.5);
        assert!((x - (MAP_MAX - sy * 50.0)).abs() < 0.5);
        assert_eq!(railroads().lines.len(), 382);
        assert_eq!(super::roads().lines.len(), 1477);
        let pose = railroads().lines[0].at(0.0).unwrap();
        assert!((pose.0 - x).abs() < 1.0);
        assert!((pose.1 - z).abs() < 1.0);
    }

    #[test]
    fn bridge_point_is_on_a_highway_segment() {
        let x = 165_033.0;
        let z = 293_247.15;
        let snap = roads().nearest(x, z).expect("highway network");
        let d = (snap.x - x).hypot(snap.z - z);
        assert!(
            d < 1.0,
            "bridge point is {d:.3} m from the nearest highways.ini segment (line {})",
            snap.line
        );

        let route = RouteLayout {
            rail: false,
            behind: vec![40.0],
            wp_ahead: vec![400.0],
            zone_in_m: 0.0,
            zone_out_m: 0.0,
        };
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 300_000.0, 300_000.0);
        let ((px, pz, _, _), pose) =
            place_route(&route, aabb, 3, 1, &[], |px, pz| aabb.contains(px, pz))
                .expect("truck column");
        assert!(
            !pose.rail,
            "a truck column must use highways.ini, not railroads.ini"
        );
        assert!(
            std::ptr::eq(network_for(false), roads()),
            "non-train placement reads the highway network"
        );
        let road = roads().lines.get(pose.line).expect("road polyline");
        let (lx, lz, _) = road.at(pose.lead_dist).expect("lead on the road");
        assert!(
            (px - lx).abs() < 1.0 && (pz - lz).abs() < 1.0,
            "truck lead ({px:.2}, {pz:.2}) left the road centerline ({lx:.2}, {lz:.2})"
        );
    }

    #[test]
    fn truck_run_is_a_road_column_with_waypoints() {
        let root = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group"
        ))
        .unwrap();
        let route = inspect_route(&root).expect("truck column");
        assert!(!route.rail);
        assert_eq!(route.behind.len(), 4);
        assert_eq!(route.wp_ahead.len(), 2);
        assert!((route.behind[0] - 138.0).abs() < 15.0);
        assert!(route.wp_ahead[0] > 4_000.0);
        assert!(route.wp_ahead[1] > route.wp_ahead[0]);
        assert!((route.zone_in_m - 12_000.0).abs() < 0.5);
        assert!((route.zone_out_m - 18_000.0).abs() < 0.5);
        assert_eq!(inspect_path_waypoints(&root), route.wp_ahead);
    }

    #[test]
    fn two_way_column_template_qualifies() {
        let root = crate::parser::parse_il2_document(include_str!(
            "../TemplateExamples/Simple Vehicle Formation 2 way column.Group"
        ))
        .unwrap();
        let route = inspect_route(&root).expect("2-way column");
        assert!(!route.rail);
        assert!(route.behind.len() >= 3);
    }

    #[test]
    fn tank_company_and_ml20_are_not_columns() {
        let tanks = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK Tank Company.Group"
        ))
        .unwrap();
        assert!(inspect_route(&tanks).is_none());
        assert!(inspect_path_waypoints(&tanks).is_empty());
        let ml20 = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK ML20 Arty.Group"
        ))
        .unwrap();
        assert!(inspect_route(&ml20).is_none());
        assert!(inspect_path_waypoints(&ml20).is_empty());
    }

    #[test]
    fn place_train_on_rail_inside_ao() {
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let route = RouteLayout {
            rail: true,
            behind: Vec::new(),
            wp_ahead: vec![2_000.0],
            zone_in_m: 0.0,
            zone_out_m: 0.0,
        };
        let ((x, z, heading, in_ao), pose) =
            place_route(&route, aabb, 7, 0, &[], |x, z| aabb.contains(x, z)).expect("rail spot");
        assert!(pose.rail);
        assert_eq!(pose.unit_xz.len(), 1);
        assert_eq!(pose.waypoints.len(), 1);
        assert!((pose.unit_xz[0].0 - x).abs() < 1.0);
        assert!((pose.unit_xz[0].1 - z).abs() < 1.0);
        let _ = in_ao;
        let line = &railroads().lines[pose.line];
        let (node_i, gap) = line
            .cum
            .iter()
            .enumerate()
            .map(|(i, &c)| (i, (pose.lead_dist - c).abs()))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .expect("rail line has nodes");
        let anchored = [node_i.checked_sub(1), Some(node_i)]
            .into_iter()
            .flatten()
            .filter(|&seg| seg + 1 < line.cum.len())
            .any(|seg| {
                let mid = (line.cum[seg] + line.cum[seg + 1]) * 0.5;
                let Some((mx, mz, _)) = line.at(mid) else {
                    return false;
                };
                let d = (pose.waypoints[0].0 - mx).hypot(pose.waypoints[0].1 - mz);
                (d - 2_000.0).abs() < 80.0
            });
        assert!(
            anchored,
            "waypoint should stay about 2 km from the segment midpoint, not follow the locomotive"
        );
        assert!(
            gap < 1.0,
            "locomotive should sit on a rail node ({gap:.2} m from the nearest corner)"
        );
        assert!(
            node_i > 0 && node_i + 1 < line.pts.len(),
            "lead should sit on a corner where two segments meet"
        );
        let (into_travel, out_travel) = if pose.reverse {
            (
                heading_toward(line.pts[node_i + 1], line.pts[node_i]),
                heading_toward(line.pts[node_i], line.pts[node_i - 1]),
            )
        } else {
            (
                heading_toward(line.pts[node_i - 1], line.pts[node_i]),
                heading_toward(line.pts[node_i], line.pts[node_i + 1]),
            )
        };
        let a = into_travel.to_radians();
        let b = out_travel.to_radians();
        let dx = a.cos() + b.cos();
        let dz = a.sin() + b.sin();
        let expected = dz.atan2(dx).to_degrees().rem_euclid(360.0);
        let err = heading_diff(heading, expected);
        assert!(
            err < 1.0,
            "heading {heading:.2} should bisect {into_travel:.2} and {out_travel:.2} (expected {expected:.2}, delta {err:.2})"
        );
    }

    #[test]
    fn park_route_moves_trucks_and_waypoints() {
        let mut root = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group"
        ))
        .unwrap();
        let route = inspect_route(&root).unwrap();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 300_000.0, 300_000.0);
        let ((x, z, heading, _), pose) =
            place_route(&route, aabb, 3, 1, &[], |x, z| aabb.contains(x, z)).expect("road column");
        park_route_copy(&mut root, (x, z), heading, &pose.unit_xz, &pose.waypoints);
        let units = collect_route_units(&root);
        assert_eq!(units.len(), pose.unit_xz.len());
        let lead = units
            .iter()
            .max_by(|a, b| {
                let h = mean_heading(&units);
                let n = units.len() as f64;
                let cx = units.iter().map(|u| u.x).sum::<f64>() / n;
                let cz = units.iter().map(|u| u.z).sum::<f64>() / n;
                project_along(a.x, a.z, (cx, cz), h)
                    .0
                    .partial_cmp(&project_along(b.x, b.z, (cx, cz), h).0)
                    .unwrap()
            })
            .unwrap();
        assert!((lead.x - x).abs() < 2.0);
        assert!((lead.z - z).abs() < 2.0);
        let mut wp_n = 0usize;
        root.for_each(&mut |e| {
            if e.block_type == "MCU_Waypoint" {
                wp_n += 1;
            }
        });
        assert_eq!(wp_n, pose.waypoints.len());
        let restored = inspect_parked_network(&root).expect("parked column on a road");
        assert!(!restored.rail);
        assert_eq!(restored.unit_xz.len(), pose.unit_xz.len());
        assert_eq!(restored.waypoints.len(), pose.waypoints.len());
        assert!((restored.unit_xz[0].0 - x).abs() < 2.0);
        assert!((restored.unit_xz[0].1 - z).abs() < 2.0);
    }

    #[test]
    fn parked_train_ypos_comes_from_height_sample() {
        let store = korea_height_store().expect("height store");
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 300_000.0, 300_000.0);
        let route = RouteLayout {
            rail: true,
            behind: Vec::new(),
            wp_ahead: vec![2_000.0],
            zone_in_m: 0.0,
            zone_out_m: 0.0,
        };
        let mut picked = None;
        for seed in 0..40 {
            let Some(((x, z, heading, _), pose)) =
                place_route(&route, aabb, seed, 0, &[], |x, z| aabb.contains(x, z))
            else {
                continue;
            };
            if crate::terrain_apply::ground_placement_y(store, pose.unit_xz[0].0, pose.unit_xz[0].1)
                .is_some()
            {
                picked = Some((x, z, heading, pose));
                break;
            }
        }
        let (x, z, heading, pose) = picked.expect("rail node on measured ground");
        let (px, pz) = pose.unit_xz[0];
        let mut root = parse_group_file(
            "Group\r\n{\r\n  Name = \"Train group\";\r\n  Index = 1;\r\n  Train\r\n  {\r\n    Name = \"Train\";\r\n    Index = 2;\r\n    LinkTrId = 3;\r\n    XPos = 40000.000;\r\n    YPos = 137.730;\r\n    ZPos = 40000.000;\r\n    XOri = 0;\r\n    YOri = 0.000;\r\n    ZOri = 0;\r\n    Script = \"LuaScripts\\\\WorldObjects\\\\Trains\\\\type475-1.txt\";\r\n    Model = \"graphics\\\\trains\\\\type475-1\\\\type475-1.mgm\";\r\n  }\r\n  MCU_TR_Entity\r\n  {\r\n    Index = 3;\r\n    Name = \"Train entity\";\r\n    XPos = 40000.000;\r\n    YPos = 137.930;\r\n    ZPos = 40000.000;\r\n    XOri = 0;\r\n    YOri = 0.000;\r\n    ZOri = 0;\r\n    MisObjID = 2;\r\n  }\r\n  MCU_Waypoint\r\n  {\r\n    Index = 4;\r\n    Name = \"WP 1\";\r\n    Objects = [3];\r\n    XPos = 42000.000;\r\n    YPos = 137.730;\r\n    ZPos = 40000.000;\r\n  }\r\n}\r\n",
        )
        .unwrap();
        park_route_copy(&mut root, (x, z), heading, &pose.unit_xz, &pose.waypoints);
        let train = root.find_by_name("Train").unwrap();
        let got: f64 = train.property("YPos").unwrap().parse().unwrap();
        let expected = crate::terrain_apply::ground_placement_y(store, px, pz).unwrap();
        let raw = store.height_at(px, pz).unwrap();
        assert!(
            (got - expected).abs() < 0.001,
            "parked Y {got} should be the ground sample {expected} (raw {raw}) at {px:.3},{pz:.3}, not template 137.730"
        );
        assert!(
            (got - 137.730).abs() > 0.05 || (expected - 137.730).abs() <= 0.05,
            "template Y 137.730 was kept at {px:.3},{pz:.3} (sample {expected})"
        );
        let (tx, tz) = train.pos_xz().unwrap();
        assert!((tx - px).abs() < 0.001 && (tz - pz).abs() < 0.001);
        assert_eq!(train.property("XOri"), Some("0"));
        assert_eq!(train.property("ZOri"), Some("0"));
        let entity = root.find_by_name("Train entity").unwrap();
        let ey: f64 = entity.property("YPos").unwrap().parse().unwrap();
        assert!((ey - (137.930 + (expected - 137.730))).abs() < 0.001);
        let wp = root.find_by_name("WP 1").unwrap();
        assert_eq!(wp.property("YPos"), Some("137.730"));
        let (wx, wz) = wp.pos_xz().unwrap();
        assert!((wx - pose.waypoints[0].0).abs() < 0.001);
        assert!((wz - pose.waypoints[0].1).abs() < 0.001);
        eprintln!(
            "TRAIN_HEIGHT x={px:.3} z={pz:.3} raw={raw:.3} placement={expected:.3} written={got:.3}"
        );
    }

    #[test]
    fn waypoint_can_sit_behind_or_on_another_line() {
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 300_000.0, 300_000.0);
        let route = RouteLayout {
            rail: false,
            behind: vec![80.0],
            wp_ahead: vec![1_200.0],
            zone_in_m: 0.0,
            zone_out_m: 0.0,
        };
        let (_, mut pose) =
            place_route(&route, aabb, 5, 0, &[], |x, z| aabb.contains(x, z)).expect("column");
        assert_eq!(pose.waypoints.len(), 1);
        let lead_line = pose.line;
        let line = &roads().lines[lead_line];
        let behind_dist = if pose.reverse {
            (pose.lead_dist + 450.0).min(line.len() - 20.0)
        } else {
            (pose.lead_dist - 450.0).max(20.0)
        };
        let (bx, bz, _) = line.at(behind_dist).unwrap();
        assert!(snap_waypoint_to_pointer(&mut pose, 0, bx, bz));
        let wp = pose.waypoints[0];
        assert!(
            (wp.0 - bx).hypot(wp.1 - bz) < 80.0,
            "behind WP should stay near the clicked track point"
        );
        let d = (wp.0 - pose.unit_xz[0].0).hypot(wp.1 - pose.unit_xz[0].1);
        assert!(
            d < 900.0,
            "behind WP should not stay at the original ~1.2 km ahead offset, d={d}"
        );
        let other = roads()
            .lines
            .iter()
            .enumerate()
            .find(|(i, l)| *i != lead_line && l.overlaps(aabb) && l.len() > 500.0)
            .map(|(i, l)| (i, l.pts[l.pts.len() / 2]));
        if let Some((oi, (ox, oz))) = other {
            assert!(snap_waypoint_to_pointer(&mut pose, 0, ox, oz));
            let snap = roads()
                .nearest(pose.waypoints[0].0, pose.waypoints[0].1)
                .unwrap();
            assert_eq!(snap.line, oi);
            assert_eq!(pose.line, lead_line);
        }
    }

    #[test]
    fn two_waypoints_straddle_zone_in_when_the_path_is_long_enough() {
        let edge = 9_800.0;
        let lead = 20_000.0;
        let dists = column_waypoint_dists(10_000.0, &[4_000.0, 8_000.0], lead, false, 50_000.0);
        assert_eq!(dists.len(), 2);
        assert!((dists[0] - (lead + edge)).abs() < 0.01);
        assert!((dists[1] - (lead - edge)).abs() < 0.01);

        let short = column_waypoint_dists(10_000.0, &[4_000.0, 8_000.0], 500.0, false, 6_000.0);
        assert!((short[0] - 4_500.0).abs() < 0.01);
        assert!((short[1] - 8_500.0).abs() < 0.01);

        let one = column_waypoint_dists(10_000.0, &[2_000.0], 1_000.0, false, 20_000.0);
        assert_eq!(one, vec![3_000.0]);

        let train = column_waypoint_dists(19_000.0, &[2_000.0, 4_000.0], 25_000.0, false, 80_000.0);
        assert!((train[0] - 43_800.0).abs() < 0.01);
        assert!((train[1] - 6_200.0).abs() < 0.01);
    }

    #[test]
    fn dead_end_spur_keeps_one_hop_and_runs_the_other_to_zone_out() {
        // The long side of this spur is a dead end. The short side meets
        // another road, which is the only way to reach the Zone OUT rim.
        let spur = PolyLine::from_pts(vec![(0.0, 0.0), (1_500.0, 0.0), (3_000.0, 0.0)]).unwrap();
        let cross = PolyLine::from_pts(vec![(0.0, 0.0), (0.0, 30_000.0)]).unwrap();
        let net = Network {
            lines: vec![spur, cross],
        };
        let mut pose = NetworkSpot {
            rail: false,
            line: 0,
            lead_dist: 400.0,
            reverse: false,
            behind: vec![40.0],
            wp_ahead: vec![50.0, 100.0],
            zone_in_m: 10_000.0,
            zone_out_m: 15_000.0,
            unit_xz: Vec::new(),
            waypoints: Vec::new(),
        };
        sample_network(&net, &mut pose).expect("pose");
        assert_eq!(pose.waypoints.len(), 2);
        let ahead = pose.waypoints[0];
        let behind = pose.waypoints[1];
        assert!(
            (ahead.0 - 3_000.0).abs() < 2.0 && ahead.1.abs() < 2.0,
            "dead-end hop should sit at the end of the spur, got {ahead:?}"
        );
        let lead = pose.unit_xz[0];
        let d = (behind.0 - lead.0).hypot(behind.1 - lead.1);
        assert!(
            (d - 14_800.0).abs() < 40.0,
            "the other hop should reach the Zone OUT rim, got {behind:?} ({d:.0} m)"
        );
        assert!(
            behind.1 > 1_000.0,
            "the long hop should leave the spur onto the connected road, got {behind:?}"
        );
    }

    #[test]
    fn sample_network_straddles_two_wps_on_a_long_road() {
        let net = roads();
        let (li, line) = net
            .lines
            .iter()
            .enumerate()
            .find(|(_, l)| l.len() > 25_000.0)
            .expect("Korea should have a road longer than 25 km");
        let mut pose = NetworkSpot {
            rail: false,
            line: li,
            lead_dist: line.len() * 0.5,
            reverse: false,
            behind: vec![80.0],
            wp_ahead: vec![1_200.0, 2_400.0],
            zone_in_m: 10_000.0,
            zone_out_m: 0.0,
            unit_xz: Vec::new(),
            waypoints: Vec::new(),
        };
        let (x, z, _) = sample_network(net, &mut pose).expect("pose");
        assert_eq!(pose.waypoints.len(), 2);
        let dists = column_waypoint_dists(
            10_000.0,
            &[1_200.0, 2_400.0],
            pose.lead_dist,
            false,
            line.len(),
        );
        let edge = 9_800.0;
        assert!(
            (dists[0] - (pose.lead_dist + edge)).abs() < 0.01,
            "path should be long enough to straddle, ahead={}",
            dists[0]
        );
        assert!(
            (dists[1] - (pose.lead_dist - edge)).abs() < 0.01,
            "path should be long enough to straddle, behind={}",
            dists[1]
        );
        let (ax, az, _) = line.at(dists[0]).unwrap();
        let (bx, bz, _) = line.at(dists[1]).unwrap();
        assert!((pose.waypoints[0].0 - ax).hypot(pose.waypoints[0].1 - az) < 1.0);
        assert!((pose.waypoints[1].0 - bx).hypot(pose.waypoints[1].1 - bz) < 1.0);
        assert!(
            (pose.waypoints[0].0 - x).hypot(pose.waypoints[0].1 - z) > 4_000.0,
            "ahead WP should be well past the authored 1.2 km hop"
        );
        assert!(
            dists[0] > pose.lead_dist && dists[1] < pose.lead_dist,
            "straddle WPs should sit on opposite sides of the lead"
        );
    }

    #[test]
    fn place_truck_column_bisects_road_corner() {
        let unit_bisector = |a_deg: f64, b_deg: f64| {
            let a = a_deg.to_radians();
            let b = b_deg.to_radians();
            (a.sin() + b.sin())
                .atan2(a.cos() + b.cos())
                .to_degrees()
                .rem_euclid(360.0)
        };

        let net = roads();
        let (line, i) = net
            .lines
            .iter()
            .find_map(|line| {
                (1..line.pts.len().saturating_sub(1)).find_map(|i| {
                    let len_in = (line.pts[i].0 - line.pts[i - 1].0)
                        .hypot(line.pts[i].1 - line.pts[i - 1].1);
                    let len_out = (line.pts[i + 1].0 - line.pts[i].0)
                        .hypot(line.pts[i + 1].1 - line.pts[i].1);
                    let into_h = heading_toward(line.pts[i - 1], line.pts[i]);
                    let out_h = heading_toward(line.pts[i], line.pts[i + 1]);
                    if len_in > 2.0 && len_out > 2.0 && heading_diff(into_h, out_h) > 15.0 {
                        Some((line, i))
                    } else {
                        None
                    }
                })
            })
            .expect("a road corner where two segments meet");
        let dist = line.cum[i];
        let (_, _, seg_h) = line.at(dist).expect("vertex");
        let into_h = heading_toward(line.pts[i - 1], line.pts[i]);
        let out_h = heading_toward(line.pts[i], line.pts[i + 1]);
        let expected = unit_bisector(into_h, out_h);
        let heading = lead_heading(line, dist, false, false, seg_h);
        let err = heading_diff(heading, expected);
        assert!(
            err < 1.0,
            "heading {heading:.2} should bisect the road corner (expected {expected:.2}, delta {err:.2})"
        );
        assert!(
            heading_diff(heading, seg_h) > 1.0,
            "bisector {heading:.2} should leave the incoming segment heading {seg_h:.2}"
        );

        let expected_rev = unit_bisector(
            heading_toward(line.pts[i + 1], line.pts[i]),
            heading_toward(line.pts[i], line.pts[i - 1]),
        );
        let rev = lead_heading(line, dist, false, true, seg_h);
        let rev_err = heading_diff(rev, expected_rev);
        assert!(
            rev_err < 1.0,
            "reverse heading {rev:.2} should bisect the reversed pair (expected {expected_rev:.2}, delta {rev_err:.2})"
        );

        let (_, _, start_h) = line.at(0.0).unwrap();
        assert!(heading_diff(lead_heading(line, 0.0, false, false, start_h), start_h) < 0.01);
        let end_dist = line.len();
        let (_, _, tail_h) = line.at(end_dist).unwrap();
        assert!(heading_diff(lead_heading(line, end_dist, false, false, tail_h), tail_h) < 0.01);

        let seg_len = line.cum[i + 1] - dist;
        if seg_len > 20.0 {
            let mid = dist + 5.0;
            let (_, _, mid_h) = line.at(mid).unwrap();
            let at_mid = lead_heading(line, mid, false, false, mid_h);
            assert!(
                heading_diff(at_mid, mid_h) < 0.01,
                "a lead off the vertex should keep the segment heading"
            );
        }

        // 350° then 20° straddles north. Averaging the degree numbers points
        // south (185°). Averaging the unit vectors points about 5° north.
        let h_in = 350.0_f64;
        let h_out = 20.0_f64;
        let step = 100.0;
        let corner = PolyLine::from_pts(vec![
            (
                -step * h_in.to_radians().cos(),
                -step * h_in.to_radians().sin(),
            ),
            (0.0, 0.0),
            (
                step * h_out.to_radians().cos(),
                step * h_out.to_radians().sin(),
            ),
        ])
        .expect("synthetic corner");
        assert!(heading_diff(heading_toward(corner.pts[0], corner.pts[1]), h_in) < 0.01);
        assert!(heading_diff(heading_toward(corner.pts[1], corner.pts[2]), h_out) < 0.01);
        let wrap_dist = corner.cum[1];
        let (_, _, wrap_seg) = corner.at(wrap_dist).unwrap();
        let wrap_h = lead_heading(&corner, wrap_dist, false, false, wrap_seg);
        let naive = (h_in + h_out) * 0.5;
        assert!(heading_diff(wrap_h, unit_bisector(h_in, h_out)) < 1.0);
        assert!(
            heading_diff(wrap_h, naive) > 90.0,
            "degree average {naive:.1} is not the bend bisector ({wrap_h:.1})"
        );
        let wrap_rev = lead_heading(&corner, wrap_dist, false, true, wrap_seg);
        let expected_wrap_rev = unit_bisector(
            heading_toward(corner.pts[2], corner.pts[1]),
            heading_toward(corner.pts[1], corner.pts[0]),
        );
        assert!(heading_diff(wrap_rev, expected_wrap_rev) < 1.0);

        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 300_000.0, 300_000.0);
        let route = RouteLayout {
            rail: false,
            behind: vec![40.0],
            wp_ahead: vec![400.0],
            zone_in_m: 0.0,
            zone_out_m: 0.0,
        };
        let ((x, z, placed_h, _), pose) =
            place_route(&route, aabb, 3, 1, &[], |px, pz| aabb.contains(px, pz))
                .expect("road column");
        assert!(!pose.rail);
        let placed_line = &roads().lines[pose.line];
        let (lx, lz, placed_seg) = placed_line.at(pose.lead_dist).unwrap();
        assert!((x - lx).abs() < 1.0 && (z - lz).abs() < 1.0);
        let from_fn = lead_heading(placed_line, pose.lead_dist, false, pose.reverse, placed_seg);
        assert!(
            heading_diff(placed_h, from_fn) < 0.05,
            "placed heading {placed_h:.2} should match lead_heading {from_fn:.2}"
        );
        if let Some(ni) = node_within(placed_line, pose.lead_dist, 1.0) {
            if ni > 0 && ni + 1 < placed_line.pts.len() {
                let (a_deg, b_deg) = if pose.reverse {
                    (
                        heading_toward(placed_line.pts[ni + 1], placed_line.pts[ni]),
                        heading_toward(placed_line.pts[ni], placed_line.pts[ni - 1]),
                    )
                } else {
                    (
                        heading_toward(placed_line.pts[ni - 1], placed_line.pts[ni]),
                        heading_toward(placed_line.pts[ni], placed_line.pts[ni + 1]),
                    )
                };
                let bend = unit_bisector(a_deg, b_deg);
                let bend_err = heading_diff(placed_h, bend);
                assert!(
                    bend_err < 1.0,
                    "placed column on a road vertex should bisect ({placed_h:.2} vs {bend:.2}, delta {bend_err:.2})"
                );
            }
        }
        let dists = column_waypoint_dists(
            pose.zone_in_m,
            &pose.wp_ahead,
            pose.lead_dist,
            pose.reverse,
            placed_line.len(),
        );
        assert_eq!(pose.waypoints.len(), dists.len());
        for (wp, d) in pose.waypoints.iter().zip(dists) {
            let (wx, wz, _) = placed_line.at(d).unwrap();
            assert!(
                (wp.0 - wx).hypot(wp.1 - wz) < 1.0,
                "waypoint should stay on the authored offset"
            );
        }
    }
}
