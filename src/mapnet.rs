//! Korea road and railroad polylines (mission-editor SVG) in world X/Z.
//!
//! SVG X increases east (game Z); SVG Y increases south (game X decreases).
//! The square is 9984 units across the 499_200 m map, so 50 m per SVG unit.

use std::sync::OnceLock;

use crate::ast::Il2Entity;
use crate::geo::{MAP_MAX, MAP_MIN};
use crate::mapclip::WorldAabb;
use crate::placement::{heading_toward, mix_index, move_anchor_to};

const SVG_SIZE: f64 = 9984.0;
const M_PER_SVG: f64 = (MAP_MAX - MAP_MIN) / SVG_SIZE;
/// Keep network groups this far apart (metres).
pub const NETWORK_SPACING: f64 = 2_500.0;
const COLUMN_MIN_ALONG: f64 = 40.0;
const COLUMN_ACROSS_ABS: f64 = 25.0;
const COLUMN_ACROSS_FRAC: f64 = 0.025;

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
    fn from_svg(svg: &str) -> Self {
        let lines = parse_svg_polylines(svg)
            .into_iter()
            .filter_map(|svg_pts| {
                let world = svg_pts
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
    pub unit_xz: Vec<(f64, f64)>,
    pub waypoints: Vec<(f64, f64)>,
}

pub fn roads() -> &'static Network {
    static N: OnceLock<Network> = OnceLock::new();
    N.get_or_init(|| Network::from_svg(include_str!("../assets/roads.svg")))
}

pub fn railroads() -> &'static Network {
    static N: OnceLock<Network> = OnceLock::new();
    N.get_or_init(|| Network::from_svg(include_str!("../assets/railroads.svg")))
}

pub fn network_for(rail: bool) -> &'static Network {
    if rail { railroads() } else { roads() }
}

pub fn svg_to_world(sx: f64, sy: f64) -> (f64, f64) {
    let z = MAP_MIN + sx * M_PER_SVG;
    let x = MAP_MAX - sy * M_PER_SVG;
    (x, z)
}

/// Scan `points="…"` attribute bodies. Not a regex — byte search for the key.
pub fn parse_svg_polylines(svg: &str) -> Vec<Vec<(f64, f64)>> {
    let bytes = svg.as_bytes();
    let key = b"points=\"";
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + key.len() < bytes.len() {
        if bytes[i..].starts_with(key) {
            i += key.len();
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let body = &svg[start..i];
            let pts = parse_point_list(body);
            if pts.len() >= 2 {
                out.push(pts);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn parse_point_list(s: &str) -> Vec<(f64, f64)> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c == ',' || c.is_whitespace() {
            flush_num(&mut cur, &mut nums);
        } else {
            cur.push(c);
        }
    }
    flush_num(&mut cur, &mut nums);
    let mut pts = Vec::with_capacity(nums.len() / 2);
    let mut i = 0usize;
    while i + 1 < nums.len() {
        pts.push((nums[i], nums[i + 1]));
        i += 2;
    }
    pts
}

fn flush_num(cur: &mut String, nums: &mut Vec<f64>) {
    if cur.is_empty() {
        return;
    }
    if let Ok(v) = cur.parse::<f64>() {
        nums.push(v);
    }
    cur.clear();
}

fn closest_on_segment(
    x: f64,
    z: f64,
    a: (f64, f64),
    b: (f64, f64),
) -> (f64, f64, f64) {
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
    Some(RouteLayout {
        rail,
        behind,
        wp_ahead,
    })
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
    along_span >= COLUMN_MIN_ALONG
        && along_span >= across_max * 4.0
        && across_max <= across_lim
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
    let mut dists = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "MCU_Waypoint" {
            return;
        }
        let Some((x, z)) = e.pos_xz() else { return };
        let (along, _) = project_along(x, z, (lead.x, lead.z), heading);
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

/// Fill unit poses from a polyline. Existing waypoint world positions stay put
/// so a WP can sit on another branch or behind the column.
pub fn sample_network(
    net: &Network,
    pose: &mut NetworkSpot,
) -> Option<(f64, f64, f64)> {
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
    let heading = if pose.reverse {
        (lead.2 + 180.0).rem_euclid(360.0)
    } else {
        lead.2
    };
    let dist_at = |delta: f64| {
        if pose.reverse {
            pose.lead_dist + delta
        } else {
            pose.lead_dist - delta
        }
    };
    let ahead_at = |delta: f64| {
        if pose.reverse {
            pose.lead_dist - delta
        } else {
            pose.lead_dist + delta
        }
    };
    let mut unit_xz = vec![(lead.0, lead.1)];
    for &d in &pose.behind {
        let p = line.at(dist_at(d))?;
        unit_xz.push((p.0, p.1));
    }
    pose.unit_xz = unit_xz;
    if !pin_wps {
        let mut waypoints = Vec::new();
        for &d in &pose.wp_ahead {
            let p = line.at(ahead_at(d))?;
            waypoints.push((p.0, p.1));
        }
        pose.waypoints = waypoints;
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
        let lead_dist = {
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
            unit_xz: Vec::new(),
            waypoints: Vec::new(),
        };
        let Some((x, z, heading)) = sample_network(net, &mut pose) else {
            continue;
        };
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

pub fn snap_waypoint_to_pointer(
    pose: &mut NetworkSpot,
    wp_i: usize,
    x: f64,
    z: f64,
) -> bool {
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

/// Translate the group so the lead visual sits on the path, then snap each
/// vehicle/train and MCU_Waypoint onto `unit_xz` / `waypoints`.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;

    #[test]
    fn svg_polylines_map_to_korea_world() {
        let rails = parse_svg_polylines(include_str!("../assets/railroads.svg"));
        let roads = parse_svg_polylines(include_str!("../assets/roads.svg"));
        assert!(rails.len() >= 10, "rail polylines {}", rails.len());
        assert!(roads.len() >= 20, "road polylines {}", roads.len());
        let (sx, sy) = rails[0][0];
        let (x, z) = svg_to_world(sx, sy);
        assert!((z - sx * 50.0).abs() < 0.5);
        assert!((x - (MAP_MAX - sy * 50.0)).abs() < 0.5);
        assert!(railroads().lines.len() >= 10);
        assert!(super::roads().lines.len() >= 20);
        let pose = railroads().lines[0].at(0.0).unwrap();
        assert!((pose.0 - x).abs() < 1.0);
        assert!((pose.1 - z).abs() < 1.0);
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
        let ml20 = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK ML20 Arty.Group"
        ))
        .unwrap();
        assert!(inspect_route(&ml20).is_none());
    }

    #[test]
    fn place_train_on_rail_inside_ao() {
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let route = RouteLayout {
            rail: true,
            behind: Vec::new(),
            wp_ahead: vec![2_000.0],
        };
        let ((x, z, _, in_ao), pose) =
            place_route(&route, aabb, 7, 0, &[], |x, z| aabb.contains(x, z)).expect("rail spot");
        assert!(pose.rail);
        assert_eq!(pose.unit_xz.len(), 1);
        assert_eq!(pose.waypoints.len(), 1);
        assert!((pose.unit_xz[0].0 - x).abs() < 1.0);
        assert!((pose.unit_xz[0].1 - z).abs() < 1.0);
        let _ = in_ao;
        let d = (pose.waypoints[0].0 - x).hypot(pose.waypoints[0].1 - z);
        assert!((d - 2_000.0).abs() < 80.0);
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
    }

    #[test]
    fn waypoint_can_sit_behind_or_on_another_line() {
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 300_000.0, 300_000.0);
        let route = RouteLayout {
            rail: false,
            behind: vec![80.0],
            wp_ahead: vec![1_200.0],
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
            let snap = roads().nearest(pose.waypoints[0].0, pose.waypoints[0].1).unwrap();
            assert_eq!(snap.line, oi);
            assert_eq!(pose.line, lead_line);
        }
    }
}
