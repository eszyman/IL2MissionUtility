//! Axis-aligned bounding box clipping for Korea map icons.
//!
//! Game axes: **XPos is north**, **ZPos is east**. In `geo_types` that is
//! `Coord { x: XPos, y: ZPos }` so a `Rect` is an AABB in mission space.
//!
//! Polygon intersection uses `geo::BooleanOps` (the maintained successor of
//! `geo-booleanop` / Martinez–Rueda).

use geo::{BooleanOps, Coord, LineString, Polygon, Rect};
use geo_types::MultiPolygon;

use crate::geo::{yalu_x_at_z, MAP_MAX, MAP_MIN};

/// Keep the front this far south of the Yalu (Korean bank only).
const YALU_STANDOFF: f64 = 8_000.0;

/// World AABB in mission X (north) / Z (east).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldAabb {
    pub x_min: f64,
    pub x_max: f64,
    pub z_min: f64,
    pub z_max: f64,
}

impl WorldAabb {
    pub fn full_map() -> Self {
        Self {
            x_min: MAP_MIN,
            x_max: MAP_MAX,
            z_min: MAP_MIN,
            z_max: MAP_MAX,
        }
    }

    pub fn from_corners(x0: f64, z0: f64, x1: f64, z1: f64) -> Self {
        Self {
            x_min: x0.min(x1),
            x_max: x0.max(x1),
            z_min: z0.min(z1),
            z_max: z0.max(z1),
        }
    }

    pub fn is_valid(self) -> bool {
        self.x_max > self.x_min && self.z_max > self.z_min
    }

    /// Strict interior test (points on the boundary are discarded).
    pub fn contains_strict(self, x: f64, z: f64) -> bool {
        x > self.x_min && x < self.x_max && z > self.z_min && z < self.z_max
    }

    pub fn contains(self, x: f64, z: f64) -> bool {
        x >= self.x_min && x <= self.x_max && z >= self.z_min && z <= self.z_max
    }

    /// Clamp a world point onto the box (inclusive).
    pub fn clamp_point(self, p: (f64, f64)) -> (f64, f64) {
        (
            p.0.clamp(self.x_min, self.x_max),
            p.1.clamp(self.z_min, self.z_max),
        )
    }

    /// Grow the box by `meters` on every side (placement margin).
    pub fn expanded(self, meters: f64) -> Self {
        Self {
            x_min: self.x_min - meters,
            x_max: self.x_max + meters,
            z_min: self.z_min - meters,
            z_max: self.z_max + meters,
        }
    }

    pub fn as_rect(self) -> Rect<f64> {
        Rect::new(
            Coord {
                x: self.x_min,
                y: self.z_min,
            },
            Coord {
                x: self.x_max,
                y: self.z_max,
            },
        )
    }

    pub fn as_polygon(self) -> Polygon<f64> {
        self.as_rect().to_polygon()
    }
}

pub fn points_to_linestring(pts: &[(f64, f64)]) -> LineString<f64> {
    LineString::from(
        pts.iter()
            .map(|&(x, z)| Coord { x, y: z })
            .collect::<Vec<_>>(),
    )
}

pub fn linestring_to_points(line: &LineString<f64>) -> Vec<(f64, f64)> {
    line.coords().map(|c| (c.x, c.y)).collect()
}

/// Cohen–Sutherland clip of a `LineString` against an AABB `Rect`.
/// Returns zero or more open polylines that lie inside the rectangle.
pub fn clip_linestring_to_rect(line: &LineString<f64>, rect: &Rect<f64>) -> Vec<LineString<f64>> {
    let coords: Vec<Coord<f64>> = line.coords().copied().collect();
    if coords.len() < 2 {
        return Vec::new();
    }
    let mut runs: Vec<LineString<f64>> = Vec::new();
    let mut cur: Vec<Coord<f64>> = Vec::new();
    for w in coords.windows(2) {
        if let Some((a, b)) = clip_segment(w[0], w[1], rect) {
            if cur.last().is_none_or(|p| *p != a) {
                if cur.len() >= 2 {
                    runs.push(LineString::from(std::mem::take(&mut cur)));
                } else {
                    cur.clear();
                }
                cur.push(a);
            }
            cur.push(b);
        } else if cur.len() >= 2 {
            runs.push(LineString::from(std::mem::take(&mut cur)));
        } else {
            cur.clear();
        }
    }
    if cur.len() >= 2 {
        runs.push(LineString::from(cur));
    }
    runs
}

const INSIDE: u8 = 0;
const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const BOTTOM: u8 = 4;
const TOP: u8 = 8;

fn outcode(p: Coord<f64>, rect: &Rect<f64>) -> u8 {
    let min = rect.min();
    let max = rect.max();
    let mut code = INSIDE;
    if p.x < min.x {
        code |= LEFT;
    } else if p.x > max.x {
        code |= RIGHT;
    }
    if p.y < min.y {
        code |= BOTTOM;
    } else if p.y > max.y {
        code |= TOP;
    }
    code
}

fn clip_segment(mut a: Coord<f64>, mut b: Coord<f64>, rect: &Rect<f64>) -> Option<(Coord<f64>, Coord<f64>)> {
    let min = rect.min();
    let max = rect.max();
    let mut code_a = outcode(a, rect);
    let mut code_b = outcode(b, rect);
    for _ in 0..8 {
        if code_a | code_b == INSIDE {
            return Some((a, b));
        }
        if code_a & code_b != 0 {
            return None;
        }
        let code = if code_a != INSIDE { code_a } else { code_b };
        let mut p = Coord { x: 0.0, y: 0.0 };
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        if code & TOP != 0 {
            p.x = a.x + dx * (max.y - a.y) / dy;
            p.y = max.y;
        } else if code & BOTTOM != 0 {
            p.x = a.x + dx * (min.y - a.y) / dy;
            p.y = min.y;
        } else if code & RIGHT != 0 {
            p.y = a.y + dy * (max.x - a.x) / dx;
            p.x = max.x;
        } else {
            p.y = a.y + dy * (min.x - a.x) / dx;
            p.x = min.x;
        }
        if code == code_a {
            a = p;
            code_a = outcode(a, rect);
        } else {
            b = p;
            code_b = outcode(b, rect);
        }
    }
    None
}

/// Boolean intersection of faction territory with the AO rectangle.
pub fn intersect_polygon_with_aabb(territory: &Polygon<f64>, aabb: WorldAabb) -> MultiPolygon<f64> {
    territory.intersection(&aabb.as_polygon())
}

/// Longest open polyline run that lies inside `aabb`.
pub fn clip_polyline_to_aabb(path: &[(f64, f64)], aabb: WorldAabb) -> Vec<(f64, f64)> {
    if path.len() < 2 {
        return path.to_vec();
    }
    clip_linestring_to_rect(&points_to_linestring(path), &aabb.as_rect())
        .into_iter()
        .map(|ls| linestring_to_points(&ls))
        .max_by_key(|p| p.len())
        .unwrap_or_default()
}

/// Clip a closed ring to `aabb`. Returns zero or more closed rings.
pub fn clip_ring_to_aabb(ring: &[(f64, f64)], aabb: WorldAabb) -> Vec<Vec<(f64, f64)>> {
    let mut pts = ring.to_vec();
    if pts.len() >= 2 && pts.first() != pts.last() {
        pts.push(pts[0]);
    }
    let Some(poly) = ring_polygon(&pts) else {
        return Vec::new();
    };
    multipolygon_rings(&intersect_polygon_with_aabb(&poly, aabb))
}

/// Offset a polyline to the left of travel by `dist` meters (west→east, left is north).
pub fn offset_polyline(pts: &[(f64, f64)], dist: f64) -> Vec<(f64, f64)> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(pts.len());
    for i in 0..pts.len() {
        let (x, z) = pts[i];
        let (x0, z0) = if i == 0 { pts[0] } else { pts[i - 1] };
        let (x1, z1) = if i + 1 == pts.len() { pts[i] } else { pts[i + 1] };
        let dx = x1 - x0;
        let dz = z1 - z0;
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        let nx = dz / len;
        let nz = -dx / len;
        out.push((x + nx * dist, z + nz * dist));
    }
    out
}

/// Clip the front to `aabb`, keep the longest run, and stretch the ends to the
/// west/east box edges (out over water) so influence can fill the whole AO.
pub fn extend_front_to_aabb(front: &[(f64, f64)], aabb: WorldAabb) -> Vec<(f64, f64)> {
    extend_front_to_aabb_ex(front, aabb, true, true)
}

/// Same as [`extend_front_to_aabb`], with independent west/east stretch.
/// Skip east stretch when an east-coast pocket sits off the main front.
pub fn extend_front_to_aabb_ex(
    front: &[(f64, f64)],
    aabb: WorldAabb,
    stretch_west: bool,
    stretch_east: bool,
) -> Vec<(f64, f64)> {
    if front.len() < 2 {
        return front.to_vec();
    }
    let clipped = clip_linestring_to_rect(&points_to_linestring(front), &aabb.as_rect());
    let mut best: Vec<(f64, f64)> = clipped
        .into_iter()
        .map(|ls| linestring_to_points(&ls))
        .max_by_key(|p| p.len())
        .unwrap_or_default();
    if best.len() < 2 {
        return best;
    }
    if best[0].1 > best.last().map(|p| p.1).unwrap_or(0.0) {
        best.reverse();
    }
    let first = best[0];
    let last = *best.last().unwrap();
    let mut out = Vec::new();
    if stretch_west && first.1 > aabb.z_min + 1.0 {
        out.push((first.0, aabb.z_min));
    }
    out.extend(best);
    if stretch_east && last.1 < aabb.z_max - 1.0 {
        out.push((last.0, aabb.z_max));
    }
    out
}

/// One salient spliced onto a west→east base front, or a detached pocket.
///
/// Influence is **not** offset around this bulge. The base-front AoI is kept,
/// then [`SalientPatch::ring`] is subtracted from the side the bulge cuts into.
#[derive(Clone, Debug)]
pub struct SalientPatch {
    /// New front vertices (west→east) that replace the bypassed span.
    pub path: Vec<(f64, f64)>,
    /// Old front between the two attachment points (draw dotted).
    /// For a detached pocket this is the southern mouth of the ring.
    pub bypassed: Vec<(f64, f64)>,
    /// Closed ring: salient path + reversed bypass, or the pocket itself.
    pub ring: Vec<(f64, f64)>,
    /// True when the bulge sits north of the old front (into DPRK / red).
    pub cuts_north: bool,
    /// False for a Hungnam-style bubble that does not share a front span.
    pub attached: bool,
}

/// Splice S-shape salients onto a base front.
/// Returns the composite front and one patch per salient (spliced or detached).
pub fn apply_salients(
    mut composite: Vec<(f64, f64)>,
    salients: &[Vec<(f64, f64)>],
) -> (Vec<(f64, f64)>, Vec<SalientPatch>) {
    let mut patches = Vec::new();
    for salient in salients {
        if salient.len() < 2 || composite.len() < 2 {
            continue;
        }
        if let Some((next, patch)) = splice_one_salient(&composite, salient) {
            composite = next;
            patches.push(patch);
        } else if let Some(patch) = detached_salient_patch(&composite, salient) {
            patches.push(patch);
        }
    }
    (composite, patches)
}

/// Closed ring that does not share a span with the front (Hungnam-style pocket).
pub fn detached_salient_patch(
    front: &[(f64, f64)],
    path: &[(f64, f64)],
) -> Option<SalientPatch> {
    if path.len() < 3 {
        return None;
    }
    let mut open = path.to_vec();
    if open.first() == open.last() {
        open.pop();
    }
    if open.len() < 3 {
        return None;
    }
    let west_i = open
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap())
        .map(|(i, _)| i)?;
    let east_i = open
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap())
        .map(|(i, _)| i)?;
    if west_i == east_i {
        return None;
    }
    let fwd = walk_arc(&open, west_i, east_i, true);
    let back = walk_arc(&open, west_i, east_i, false);
    let bypassed = if mean_x(&fwd) <= mean_x(&back) {
        fwd
    } else {
        back
    };
    let mut ring = open.clone();
    ring.push(open[0]);
    if ring_self_intersects(&ring) {
        return None;
    }
    Some(SalientPatch {
        path: open,
        bypassed,
        ring,
        cuts_north: mean_x(path) > mean_x(front),
        attached: false,
    })
}

fn splice_one_salient(
    front: &[(f64, f64)],
    salient: &[(f64, f64)],
) -> Option<(Vec<(f64, f64)>, SalientPatch)> {
    let start = *salient.first()?;
    let end = *salient.last()?;
    let (i0, t0, p0, _) = project_on_polyline(front, start)?;
    let (i1, t1, p1, _) = project_on_polyline(front, end)?;

    let (i_lo, t_lo, p_lo, i_hi, t_hi, p_hi, mut path) =
        if (i0, t0) <= (i1, t1) {
            (i0, t0, p0, i1, t1, p1, salient.to_vec())
        } else {
            let mut rev = salient.to_vec();
            rev.reverse();
            (i1, t1, p1, i0, t0, p0, rev)
        };

    if i_lo == i_hi && (t_hi - t_lo).abs() < 1e-4 {
        return None;
    }

    // Force path to start/end on the front so the ring closes cleanly.
    if path.first() != Some(&p_lo) {
        path.insert(0, p_lo);
    }
    if path.last() != Some(&p_hi) {
        path.push(p_hi);
    }

    let mut bypassed = vec![p_lo];
    if i_lo == i_hi {
        bypassed.push(p_hi);
    } else {
        bypassed.extend_from_slice(&front[i_lo + 1..=i_hi]);
        if bypassed.last() != Some(&p_hi) {
            bypassed.push(p_hi);
        }
    }

    let cuts_north = mean_x(&path) > mean_x(&bypassed);
    let mut ring = path.clone();
    for p in bypassed.iter().rev().skip(1) {
        ring.push(*p);
    }
    if ring.first() != ring.last() {
        ring.push(ring[0]);
    }
    if ring_self_intersects(&ring) {
        return None;
    }

    let mut next = front[..=i_lo].to_vec();
    if next.last() != Some(&p_lo) {
        next.push(p_lo);
    }
    next.extend(path.iter().copied().skip(1));
    if i_hi + 1 < front.len() {
        let rest = &front[i_hi + 1..];
        if next.last() != rest.first() {
            next.extend_from_slice(rest);
        } else if rest.len() > 1 {
            next.extend_from_slice(&rest[1..]);
        }
    }

    let _ = (t_lo, t_hi);
    Some((
        next,
        SalientPatch {
            path,
            bypassed,
            ring,
            cuts_north,
            attached: true,
        },
    ))
}

fn mean_x(pts: &[(f64, f64)]) -> f64 {
    if pts.is_empty() {
        return 0.0;
    }
    pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64
}

/// Closest point on a polyline: (segment index, t in 0..=1, point, dist²).
fn project_on_polyline(
    line: &[(f64, f64)],
    p: (f64, f64),
) -> Option<(usize, f64, (f64, f64), f64)> {
    if line.len() < 2 {
        return None;
    }
    let mut best: Option<(usize, f64, (f64, f64), f64)> = None;
    for (i, w) in line.windows(2).enumerate() {
        let (x0, z0) = w[0];
        let (x1, z1) = w[1];
        let vx = x1 - x0;
        let vz = z1 - z0;
        let len2 = vx * vx + vz * vz;
        let t = if len2 < 1e-6 {
            0.0
        } else {
            (((p.0 - x0) * vx + (p.1 - z0) * vz) / len2).clamp(0.0, 1.0)
        };
        let q = (x0 + vx * t, z0 + vz * t);
        let d2 = (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2);
        if best.is_none_or(|b| d2 < b.3) {
            best = Some((i, t, q, d2));
        }
    }
    best
}

/// Snap `p` onto `front` (for starting/ending a salient stroke).
pub fn snap_to_front(front: &[(f64, f64)], p: (f64, f64)) -> Option<(f64, f64)> {
    project_on_polyline(front, p).map(|(_, _, q, _)| q)
}

/// How close unit placement stays to the front (metres).
pub const FRONT_PLACE_BAND: f64 = 10_000.0;

/// Distance from `p` to the closest point on `line`.
pub fn polyline_distance(line: &[(f64, f64)], p: (f64, f64)) -> Option<f64> {
    project_on_polyline(line, p).map(|(_, _, _, d2)| d2.sqrt())
}

/// True when `p` is within `band` metres of `front`. Empty fronts do not filter.
pub fn within_front_band(front: &[(f64, f64)], p: (f64, f64), band: f64) -> bool {
    match polyline_distance(front, p) {
        Some(d) => d <= band,
        None => true,
    }
}

/// Drop points outside the front band. If that would empty the set, keep the original.
pub fn filter_front_band(
    pts: Vec<(f64, f64)>,
    front: &[(f64, f64)],
    band: Option<f64>,
) -> Vec<(f64, f64)> {
    let Some(band) = band else {
        return pts;
    };
    if front.len() < 2 || pts.is_empty() {
        return pts;
    }
    let kept: Vec<(f64, f64)> = pts
        .iter()
        .copied()
        .filter(|p| within_front_band(front, *p, band))
        .collect();
    if kept.is_empty() {
        pts
    } else {
        kept
    }
}

/// Base front strokes must run west→east (increasing Z) so the line cannot
/// fold over itself. `min_step` is metres between vertices.
pub fn can_extend_west_east(
    line: &[(f64, f64)],
    p: (f64, f64),
    min_step: f64,
) -> bool {
    let Some(&(x0, z0)) = line.last() else {
        return true;
    };
    let dx = p.0 - x0;
    let dz = p.1 - z0;
    if (dx * dx + dz * dz).sqrt() < min_step {
        return false;
    }
    if dz < 400.0 {
        return false;
    }
    !segment_hits_polyline((x0, z0), p, line)
}

/// Salient strokes may bulge north/south but must not cross themselves.
pub fn can_extend_salient(line: &[(f64, f64)], p: (f64, f64), min_step: f64) -> bool {
    let Some(&last) = line.last() else {
        return true;
    };
    let dx = p.0 - last.0;
    let dz = p.1 - last.1;
    if (dx * dx + dz * dz).sqrt() < min_step {
        return false;
    }
    let mut probe = line.to_vec();
    probe.push(p);
    !stroke_self_intersects(&probe)
}

/// True when an open stroke crosses itself (used when snapping the last vertex).
pub fn stroke_self_intersects(pts: &[(f64, f64)]) -> bool {
    if pts.len() < 4 {
        return false;
    }
    for (i, w) in pts.windows(2).enumerate() {
        let skip_from = i.saturating_sub(1);
        for (j, v) in pts.windows(2).enumerate() {
            if j >= skip_from && j <= i + 1 {
                continue;
            }
            if j > i {
                continue;
            }
            if segments_cross(w[0], w[1], v[0], v[1]) {
                return true;
            }
        }
    }
    false
}

/// Closed ring with a proper crossing (bowtie / figure-8).
pub fn ring_self_intersects(ring: &[(f64, f64)]) -> bool {
    let mut pts = ring.to_vec();
    if pts.len() >= 2 && pts.first() == pts.last() {
        pts.pop();
    }
    let n = pts.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        if (a.0 - b.0).hypot(a.1 - b.1) < 1.0 {
            continue;
        }
        for dj in 2..n - 1 {
            let j = (i + dj) % n;
            let c = pts[j];
            let d = pts[(j + 1) % n];
            if (c.0 - d.0).hypot(c.1 - d.1) < 1.0 {
                continue;
            }
            if segments_cross(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

fn segment_hits_polyline(a: (f64, f64), b: (f64, f64), line: &[(f64, f64)]) -> bool {
    if line.len() < 2 {
        return false;
    }
    let skip = line.len().saturating_sub(2);
    for (i, w) in line.windows(2).enumerate() {
        if i >= skip {
            continue;
        }
        if segments_cross(a, b, w[0], w[1]) {
            return true;
        }
    }
    false
}

fn segments_cross(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let d1 = orient(c, d, a);
    let d2 = orient(c, d, b);
    let d3 = orient(a, b, c);
    let d4 = orient(a, b, d);
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.1 - a.1) * (c.0 - b.0) - (b.0 - a.0) * (c.1 - b.1)
}

/// Turn a closed pocket ring into a salient path attached to `front`.
pub fn pocket_ring_to_salient(
    front: &[(f64, f64)],
    ring: &[(f64, f64)],
) -> Option<Vec<(f64, f64)>> {
    if front.len() < 2 || ring.len() < 3 {
        return None;
    }
    let mut open = ring.to_vec();
    if open.first() == open.last() {
        open.pop();
    }
    if open.len() < 3 {
        return None;
    }
    let west_i = open
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap())
        .map(|(i, _)| i)?;
    let east_i = open
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap())
        .map(|(i, _)| i)?;
    if west_i == east_i {
        return None;
    }
    let mut arc = walk_arc(&open, west_i, east_i, true);
    if arc.len() > open.len() + 1 {
        return None;
    }
    let other_len = open.len() + 2 - arc.len();
    if other_len > 1 && mean_x(&arc) < mean_x(&open) {
        arc = walk_arc(&open, west_i, east_i, false);
        if arc.len() > open.len() + 1 {
            return None;
        }
    }
    Some(arc)
}

fn walk_arc(open: &[(f64, f64)], start: usize, end: usize, forward: bool) -> Vec<(f64, f64)> {
    let n = open.len();
    let mut arc = Vec::new();
    let mut i = start;
    loop {
        arc.push(open[i]);
        if i == end {
            break;
        }
        i = if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        };
        if arc.len() > n + 1 {
            break;
        }
    }
    arc
}

/// North (higher X) and south (lower X) influence polygons from a west–east front.
pub fn influence_polygons(front_xz: &[(f64, f64)]) -> Option<(Polygon<f64>, Polygon<f64>)> {
    influence_polygons_in(front_xz, WorldAabb::full_map(), 0.0)
}

/// Influence polygons clipped to `aabb`, with `gap` meters cleared on each side of the front.
///
/// The front is treated as a single-valued northing vs easting (west→east) so
/// offset spikes cannot overlap the two fills. North of the Yalu is clipped out.
pub fn influence_polygons_in(
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    gap: f64,
) -> Option<(Polygon<f64>, Polygon<f64>)> {
    influence_polygons_stretched(front_xz, aabb, gap, true, true)
}

/// Same as [`influence_polygons_in`], with independent west/east front stretch.
pub fn influence_polygons_stretched(
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    gap: f64,
    stretch_west: bool,
    stretch_east: bool,
) -> Option<(Polygon<f64>, Polygon<f64>)> {
    let (north_edge, south_edge, cap) =
        influence_edges(front_xz, aabb, gap, stretch_west, stretch_east)?;
    if north_edge.len() < 2 || south_edge.len() < 2 {
        return None;
    }
    let mut north = north_edge.clone();
    for p in cap.iter().rev() {
        north.push(*p);
    }
    north.push(north_edge[0]);

    let mut south = south_edge.clone();
    for p in south_edge.iter().rev() {
        south.push((aabb.x_min, p.1));
    }
    south.push(south_edge[0]);

    Some((ring_polygon(&north)?, ring_polygon(&south)?))
}

/// Base-front influence with salient rings subtracted from the side they cut into.
pub fn influence_minus_salients(
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    gap: f64,
    stretch_west: bool,
    stretch_east: bool,
    patches: &[SalientPatch],
) -> Option<(MultiPolygon<f64>, MultiPolygon<f64>)> {
    let (north, south) =
        influence_polygons_stretched(front_xz, aabb, gap, stretch_west, stretch_east)?;
    let mut north_mp = intersect_polygon_with_aabb(&north, aabb);
    let mut south_mp = intersect_polygon_with_aabb(&south, aabb);
    for patch in patches {
        if ring_self_intersects(&patch.ring) {
            continue;
        }
        let Some(poly) = ring_polygon(&patch.ring) else {
            continue;
        };
        let hole = MultiPolygon(vec![poly]);
        if patch.cuts_north {
            if let Some(next) = safe_difference(&north_mp, &hole) {
                north_mp = next;
            }
        } else if let Some(next) = safe_difference(&south_mp, &hole) {
            south_mp = next;
        }
    }
    Some((north_mp, south_mp))
}

/// Convex quads for preview fill (avoids egui convex-hull overlap on wiggly fronts).
pub fn influence_fill_quads(
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    gap: f64,
) -> (Vec<[(f64, f64); 4]>, Vec<[(f64, f64); 4]>) {
    influence_fill_quads_ex(front_xz, aabb, gap, true, true)
}

/// Same as [`influence_fill_quads`], with independent west/east front stretch.
pub fn influence_fill_quads_ex(
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    gap: f64,
    stretch_west: bool,
    stretch_east: bool,
) -> (Vec<[(f64, f64); 4]>, Vec<[(f64, f64); 4]>) {
    let Some((north_edge, south_edge, cap)) =
        influence_edges(front_xz, aabb, gap, stretch_west, stretch_east)
    else {
        return (Vec::new(), Vec::new());
    };
    let mut north = Vec::new();
    let mut south = Vec::new();
    for i in 0..north_edge.len().saturating_sub(1) {
        north.push([north_edge[i], north_edge[i + 1], cap[i + 1], cap[i]]);
        south.push([
            south_edge[i],
            (aabb.x_min, south_edge[i].1),
            (aabb.x_min, south_edge[i + 1].1),
            south_edge[i + 1],
        ]);
    }
    (north, south)
}

/// Preview strips with salient bubbles punched out (no AoI colour in the hole).
pub fn quads_minus_salients(
    quads: &[[(f64, f64); 4]],
    patches: &[SalientPatch],
) -> Vec<Vec<(f64, f64)>> {
    let holes: Vec<Polygon<f64>> = patches
        .iter()
        .filter(|p| !ring_self_intersects(&p.ring))
        .filter_map(|p| ring_polygon(&p.ring))
        .collect();
    if holes.is_empty() {
        return quads
            .iter()
            .map(|q| {
                let mut r = q.to_vec();
                r.push(q[0]);
                r
            })
            .collect();
    }
    let hole_mp = MultiPolygon(holes);
    let mut out = Vec::new();
    for q in quads {
        let mut pts = q.to_vec();
        pts.push(q[0]);
        let Some(poly) = ring_polygon(&pts) else {
            continue;
        };
        let remain = match safe_difference(&MultiPolygon(vec![poly.clone()]), &hole_mp) {
            Some(mp) => mp,
            None => MultiPolygon(vec![poly]),
        };
        for ring in remain.iter().map(|p| linestring_to_points(p.exterior())) {
            if ring.len() >= 4 {
                out.push(ring);
            }
        }
    }
    out
}

fn safe_difference(base: &MultiPolygon<f64>, hole: &MultiPolygon<f64>) -> Option<MultiPolygon<f64>> {
    if hole.0.is_empty() {
        return Some(base.clone());
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| base.difference(hole))).ok()
}

fn influence_edges(
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    gap: f64,
    stretch_west: bool,
    stretch_east: bool,
) -> Option<(Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>)> {
    let extended = extend_front_to_aabb_ex(front_xz, aabb, stretch_west, stretch_east);
    let front = prepare_front(&extended);
    if front.len() < 2 {
        return None;
    }
    let mut zs: Vec<f64> = front.iter().map(|p| p.1).collect();
    if stretch_west {
        zs.push(aabb.z_min);
    }
    if stretch_east {
        zs.push(aabb.z_max);
    }
    zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    zs.dedup_by(|a, b| (*a - *b).abs() < 50.0);
    let zs: Vec<f64> = zs
        .into_iter()
        .filter(|z| *z >= aabb.z_min - 1.0 && *z <= aabb.z_max + 1.0)
        .collect();
    if zs.len() < 2 {
        return None;
    }
    let mut north_edge = Vec::new();
    let mut south_edge = Vec::new();
    let mut cap = Vec::new();
    for z in zs {
        let xf = front_x_at_z(&front, z);
        let top = aabb.x_max;
        let xn = (xf + gap).min(top);
        let mut xs = (xf - gap).max(aabb.x_min);
        if let Some(yx) = yalu_x_at_z(z) {
            xs = xs.min(yx - YALU_STANDOFF);
        }
        north_edge.push((xn, z));
        south_edge.push((xs, z));
        cap.push((top, z));
    }
    Some((north_edge, south_edge, cap))
}

/// Hard cap: never let a **south** fill vertex sit on or north of the Yalu.
pub fn clamp_point_south_of_yalu(x: f64, z: f64) -> f64 {
    clamp_south_of_yalu(x, z)
}

/// West→east, one northing per easting, clamped south of the Yalu.
pub fn prepare_front(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut ordered = pts.to_vec();
    if ordered[0].1 > ordered.last().map(|p| p.1).unwrap_or(0.0) {
        ordered.reverse();
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &(x, z) in &ordered {
        let x = clamp_south_of_yalu(x, z);
        if let Some(last) = out.last_mut() {
            if (z - last.1).abs() < 1.0 {
                last.0 = last.0.max(x);
                continue;
            }
            if z + 1.0 < last.1 {
                continue;
            }
        }
        out.push((x, z));
    }
    out
}

fn clamp_south_of_yalu(x: f64, z: f64) -> f64 {
    match yalu_x_at_z(z) {
        Some(yx) => x.min(yx - YALU_STANDOFF),
        None => x,
    }
}

/// True when `(x, z)` sits north of the west→east front (DPRK / Eastern side).
pub fn point_north_of_front(front: &[(f64, f64)], x: f64, z: f64) -> bool {
    x > front_x_at_z(front, z)
}

fn front_x_at_z(front: &[(f64, f64)], z: f64) -> f64 {
    if front.is_empty() {
        return 0.0;
    }
    if z <= front[0].1 {
        return front[0].0;
    }
    if let Some(last) = front.last() {
        if z >= last.1 {
            return last.0;
        }
    }
    for w in front.windows(2) {
        let (x0, z0) = w[0];
        let (x1, z1) = w[1];
        if z >= z0 && z <= z1 {
            let t = (z - z0) / (z1 - z0).max(1e-6);
            return x0 + (x1 - x0) * t;
        }
    }
    front.last().map(|p| p.0).unwrap_or(0.0)
}

fn ring_polygon(pts: &[(f64, f64)]) -> Option<Polygon<f64>> {
    if pts.len() < 4 {
        return None;
    }
    Some(Polygon::new(points_to_linestring(pts), vec![]))
}

pub fn multipolygon_rings(mp: &MultiPolygon<f64>) -> Vec<Vec<(f64, f64)>> {
    mp.iter()
        .filter_map(polygon_boundary_ring)
        .filter(|ring| ring.len() >= 4)
        .collect()
}

/// MCU_TR_InfluenceArea Boundary is a single ring. Punch holes with a keyhole slit.
fn polygon_boundary_ring(poly: &Polygon<f64>) -> Option<Vec<(f64, f64)>> {
    let mut outer = linestring_to_points(poly.exterior());
    if outer.len() < 4 {
        return None;
    }
    for hole in poly.interiors() {
        let inner = linestring_to_points(hole);
        if inner.len() >= 4 {
            splice_keyhole(&mut outer, &inner);
        }
    }
    Some(outer)
}

fn splice_keyhole(outer: &mut Vec<(f64, f64)>, hole: &[(f64, f64)]) {
    let close_outer = outer.len() >= 2 && outer.first() == outer.last();
    if close_outer {
        outer.pop();
    }
    let hole_open: Vec<(f64, f64)> = if hole.len() >= 2 && hole.first() == hole.last() {
        hole[..hole.len() - 1].to_vec()
    } else {
        hole.to_vec()
    };
    if outer.is_empty() || hole_open.len() < 3 {
        if close_outer {
            if let Some(&p) = outer.first() {
                outer.push(p);
            }
        }
        return;
    }
    let mut best_d = f64::MAX;
    let mut best_i = 0usize;
    let mut best_j = 0usize;
    for (i, op) in outer.iter().enumerate() {
        for (j, hp) in hole_open.iter().enumerate() {
            let d = (op.0 - hp.0).powi(2) + (op.1 - hp.1).powi(2);
            if d < best_d {
                best_d = d;
                best_i = i;
                best_j = j;
            }
        }
    }
    let mut insert = Vec::with_capacity(hole_open.len() + 2);
    for k in 0..hole_open.len() {
        insert.push(hole_open[(best_j + k) % hole_open.len()]);
    }
    insert.push(hole_open[best_j]);
    insert.push(outer[best_i]);
    outer.splice(best_i + 1..best_i + 1, insert);
    if let Some(&p) = outer.first() {
        if outer.last() != Some(&p) {
            outer.push(p);
        }
    }
}

/// Mock mission-file dump of a clip result (debug / preview).
pub fn format_clip_preview(
    aabb: WorldAabb,
    point_count: usize,
    front_runs: usize,
    influence_rings: usize,
) -> String {
    format!(
        "Group {{\n  Name = \"Map Clip Preview\";\n  AABB = X[{:.0}..{:.0}] Z[{:.0}..{:.0}];\n  Points = {point_count};\n  FrontRuns = {front_runs};\n  InfluenceRings = {influence_rings};\n}}\n",
        aabb.x_min, aabb.x_max, aabb.z_min, aabb.z_max
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_contains_drops_boundary() {
        let box_ = WorldAabb::from_corners(0.0, 0.0, 10.0, 10.0);
        assert!(box_.contains_strict(5.0, 5.0));
        assert!(!box_.contains_strict(0.0, 5.0));
        assert!(!box_.contains_strict(10.0, 5.0));
    }

    #[test]
    fn clips_a_line_that_crosses_the_rect() {
        let line = points_to_linestring(&[(0.0, 5.0), (20.0, 5.0)]);
        let rect = WorldAabb::from_corners(5.0, 0.0, 15.0, 10.0).as_rect();
        let clipped = clip_linestring_to_rect(&line, &rect);
        assert_eq!(clipped.len(), 1);
        let pts = linestring_to_points(&clipped[0]);
        assert!((pts[0].0 - 5.0).abs() < 1e-9);
        assert!((pts.last().unwrap().0 - 15.0).abs() < 1e-9);
    }

    #[test]
    fn drops_a_line_entirely_outside() {
        let line = points_to_linestring(&[(0.0, 0.0), (1.0, 1.0)]);
        let rect = WorldAabb::from_corners(10.0, 10.0, 20.0, 20.0).as_rect();
        assert!(clip_linestring_to_rect(&line, &rect).is_empty());
    }

    #[test]
    fn clip_polyline_keeps_the_longest_interior_run() {
        let path = vec![(0.0, 5.0), (20.0, 5.0)];
        let aabb = WorldAabb::from_corners(5.0, 0.0, 15.0, 10.0);
        let pts = clip_polyline_to_aabb(&path, aabb);
        assert!((pts[0].0 - 5.0).abs() < 1e-9);
        assert!((pts.last().unwrap().0 - 15.0).abs() < 1e-9);
    }

    #[test]
    fn clip_ring_stays_inside_the_aabb() {
        let ring = vec![
            (0.0, 0.0),
            (200.0, 0.0),
            (200.0, 200.0),
            (0.0, 200.0),
            (0.0, 0.0),
        ];
        let aabb = WorldAabb::from_corners(50.0, 50.0, 150.0, 150.0);
        let clipped = clip_ring_to_aabb(&ring, aabb);
        assert!(!clipped.is_empty());
        for ring in &clipped {
            for &(x, z) in ring {
                assert!(
                    aabb.expanded(1e-6).contains(x, z),
                    "clipped ring vertex ({x}, {z}) left the AO"
                );
            }
        }
    }

    #[test]
    fn influence_intersection_is_nonempty_on_the_mlr() {
        let front = vec![(150_000.0, 80_000.0), (160_000.0, 200_000.0), (155_000.0, 350_000.0)];
        let (north, south) = influence_polygons(&front).unwrap();
        let ao = WorldAabb::from_corners(140_000.0, 150_000.0, 180_000.0, 250_000.0);
        let n = intersect_polygon_with_aabb(&north, ao);
        let s = intersect_polygon_with_aabb(&south, ao);
        assert!(!n.0.is_empty() || !s.0.is_empty());
        let preview = format_clip_preview(ao, 3, 1, n.0.len() + s.0.len());
        assert!(preview.contains("Map Clip Preview"));
    }

    #[test]
    fn fall_style_front_fills_do_not_overlap() {
        use geo::Area;
        // West-east line that wobbles in northing the way Oct 1950 did.
        let front = vec![
            (300_000.0, 80_000.0),
            (320_000.0, 150_000.0),
            (280_000.0, 220_000.0),
            (310_000.0, 300_000.0),
        ];
        let aabb = WorldAabb::from_corners(40_000.0, 40_000.0, 470_000.0, 470_000.0);
        let (north, south) = influence_polygons_in(&front, aabb, 5_000.0).unwrap();
        let overlap: f64 = north.intersection(&south).iter().map(|p| p.unsigned_area()).sum();
        assert!(overlap < 50_000.0, "overlap area {overlap}");
        let (nq, sq) = influence_fill_quads(&front, aabb, 5_000.0);
        assert!(!nq.is_empty() && !sq.is_empty());
    }

    #[test]
    fn expanded_box_includes_a_10km_margin() {
        let box_ = WorldAabb::from_corners(100_000.0, 100_000.0, 200_000.0, 200_000.0);
        let pad = box_.expanded(10_000.0);
        assert!(pad.contains(95_000.0, 150_000.0));
        assert!(!box_.contains(95_000.0, 150_000.0));
    }

    #[test]
    fn dprk_fill_extends_north_of_the_yalu() {
        let front = vec![
            (120_000.0, MAP_MIN + 20_000.0),
            (130_000.0, 200_000.0),
            (125_000.0, MAP_MAX - 20_000.0),
        ];
        let aabb = WorldAabb::full_map();
        let (north, south) = influence_polygons_in(&front, aabb, 5_000.0).unwrap();
        let north_pts = linestring_to_points(north.exterior());
        assert!(
            north_pts
                .iter()
                .any(|&(x, z)| crate::geo::yalu_x_at_z(z)
                    .is_some_and(|yx| x > yx + 1_000.0)),
            "DPRK fill must continue north of the Yalu"
        );
        assert!(north_pts.iter().any(|&(x, _)| (x - aabb.x_max).abs() < 1.0));
        for &(x, z) in &linestring_to_points(south.exterior()) {
            if let Some(yx) = crate::geo::yalu_x_at_z(z) {
                assert!(
                    x <= yx - 1_000.0,
                    "USA fill x={x} at z={z} must stay south of Yalu {yx}"
                );
            }
        }
        let (nq, _) = influence_fill_quads(&front, aabb, 5_000.0);
        assert!(nq.iter().any(|q| q.iter().any(|&(x, z)| {
            crate::geo::yalu_x_at_z(z).is_some_and(|yx| x > yx + 1_000.0)
        })));
    }

    #[test]
    fn offset_eastbound_front_north_increases_x() {
        let line = vec![(100_000.0, 80_000.0), (100_000.0, 120_000.0)];
        let north = offset_polyline(&line, 5_000.0);
        assert!((north[0].0 - 105_000.0).abs() < 1.0);
        assert!((north[1].0 - 105_000.0).abs() < 1.0);
    }

    #[test]
    fn extend_front_reaches_the_aabb_z_edges() {
        let front = vec![(150_000.0, 180_000.0), (152_000.0, 220_000.0)];
        let aabb = WorldAabb::from_corners(100_000.0, 100_000.0, 200_000.0, 300_000.0);
        let ext = extend_front_to_aabb(&front, aabb);
        assert!((ext[0].1 - 100_000.0).abs() < 1.0);
        assert!((ext.last().unwrap().1 - 300_000.0).abs() < 1.0);
    }

    #[test]
    fn extend_front_can_skip_east_stretch() {
        let front = vec![(150_000.0, 180_000.0), (152_000.0, 220_000.0)];
        let aabb = WorldAabb::from_corners(100_000.0, 100_000.0, 200_000.0, 300_000.0);
        let ext = extend_front_to_aabb_ex(&front, aabb, true, false);
        assert!((ext[0].1 - 100_000.0).abs() < 1.0);
        assert!(ext.last().unwrap().1 < aabb.z_max - 1.0);
    }

    #[test]
    fn west_east_draw_rejects_westbound_and_self_cross() {
        let line = vec![(100_000.0, 100_000.0), (100_000.0, 120_000.0)];
        assert!(!can_extend_west_east(&line, (100_000.0, 110_000.0), 2_500.0));
        assert!(!can_extend_west_east(&line, (110_000.0, 90_000.0), 2_500.0));
        assert!(can_extend_west_east(&line, (110_000.0, 140_000.0), 2_500.0));
    }

    #[test]
    fn salient_draw_rejects_self_cross() {
        let line = vec![
            (100_000.0, 100_000.0),
            (130_000.0, 130_000.0),
            (130_000.0, 100_000.0),
        ];
        assert!(!can_extend_salient(&line, (100_000.0, 130_000.0), 2_500.0));
        assert!(can_extend_salient(&line, (150_000.0, 90_000.0), 2_500.0));
        assert!(stroke_self_intersects(&[
            (0.0, 0.0),
            (10.0, 10.0),
            (10.0, 0.0),
            (0.0, 10.0),
        ]));
        assert!(ring_self_intersects(&[
            (0.0, 0.0),
            (10.0, 10.0),
            (10.0, 0.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ]));
    }

    #[test]
    fn self_intersecting_salient_does_not_panic() {
        let front = vec![(100_000.0, 80_000.0), (100_000.0, 220_000.0)];
        let bowtie = vec![
            (100_000.0, 120_000.0),
            (150_000.0, 180_000.0),
            (100_000.0, 180_000.0),
            (150_000.0, 120_000.0),
        ];
        let aabb = WorldAabb::from_corners(40_000.0, 40_000.0, 300_000.0, 400_000.0);
        let (_, patches) = apply_salients(front.clone(), &[bowtie.clone()]);
        let _ = influence_minus_salients(&front, aabb, 5_000.0, true, true, &patches);
        let (nq, sq) = influence_fill_quads(&front, aabb, 5_000.0);
        let _ = quads_minus_salients(&nq, &patches);
        let _ = quads_minus_salients(&sq, &patches);
        let mut ring = bowtie.clone();
        ring.push(bowtie[0]);
        let bad = SalientPatch {
            path: bowtie,
            bypassed: Vec::new(),
            ring,
            cuts_north: true,
            attached: false,
        };
        let _ = influence_minus_salients(&front, aabb, 5_000.0, true, true, &[bad.clone()]);
        let _ = quads_minus_salients(&nq, &[bad]);
    }

    #[test]
    fn salient_splice_cuts_north_and_keeps_bypassed() {
        let front = vec![
            (100_000.0, 100_000.0),
            (100_000.0, 150_000.0),
            (100_000.0, 200_000.0),
        ];
        let bulge = vec![
            (100_000.0, 120_000.0),
            (140_000.0, 140_000.0),
            (140_000.0, 170_000.0),
            (100_000.0, 180_000.0),
        ];
        let (composite, patches) = apply_salients(front, &[bulge]);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].cuts_north);
        assert!(patches[0].bypassed.len() >= 2);
        assert!(composite.iter().any(|p| (p.0 - 140_000.0).abs() < 1.0));
        assert!(patches[0].ring.len() >= 4);
        assert!(patches[0].attached);
        assert!(patches[0].path.len() >= 2);
    }

    #[test]
    fn detached_pocket_keeps_base_front_and_fills_ring() {
        let front = vec![
            (100_000.0, 80_000.0),
            (100_000.0, 200_000.0),
        ];
        let pocket = vec![
            (160_000.0, 240_000.0),
            (180_000.0, 250_000.0),
            (180_000.0, 270_000.0),
            (160_000.0, 280_000.0),
            (150_000.0, 260_000.0),
            (160_000.0, 240_000.0),
        ];
        let (composite, patches) = apply_salients(front.clone(), &[pocket]);
        assert_eq!(patches.len(), 1);
        assert!(!patches[0].attached);
        assert!(patches[0].cuts_north);
        assert!(patches[0].bypassed.len() >= 2);
        assert!(patches[0].ring.len() >= 4);
        assert_eq!(composite, front);
    }

    #[test]
    fn pocket_ring_prefers_the_northern_arc() {
        let front = vec![(100_000.0, 100_000.0), (100_000.0, 200_000.0)];
        let ring = vec![
            (100_000.0, 130_000.0),
            (140_000.0, 140_000.0),
            (140_000.0, 160_000.0),
            (100_000.0, 170_000.0),
            (100_000.0, 130_000.0),
        ];
        let path = pocket_ring_to_salient(&front, &ring).expect("open the ring");
        assert!(path.iter().any(|p| (p.0 - 140_000.0).abs() < 1.0));
        assert!(path.first().unwrap().1 < path.last().unwrap().1);
    }

    #[test]
    fn influence_subtracts_north_salient_from_dprk() {
        use geo::Area;
        let front = vec![
            (120_000.0, 80_000.0),
            (120_000.0, 200_000.0),
            (120_000.0, 320_000.0),
        ];
        let bulge = vec![
            (120_000.0, 140_000.0),
            (160_000.0, 160_000.0),
            (160_000.0, 180_000.0),
            (120_000.0, 200_000.0),
        ];
        let aabb = WorldAabb::from_corners(40_000.0, 40_000.0, 300_000.0, 400_000.0);
        let (_, patches) = apply_salients(front.clone(), &[bulge]);
        let (north_cut, _) =
            influence_minus_salients(&front, aabb, 5_000.0, true, true, &patches).unwrap();
        let (north_raw, _) =
            influence_polygons_stretched(&front, aabb, 5_000.0, true, true).unwrap();
        let raw_area: f64 = intersect_polygon_with_aabb(&north_raw, aabb)
            .iter()
            .map(|p| p.unsigned_area())
            .sum();
        let cut_area: f64 = north_cut.iter().map(|p| p.unsigned_area()).sum();
        assert!(
            cut_area < raw_area - 1_000_000.0,
            "salient should be subtracted from DPRK fill ({cut_area} vs {raw_area})"
        );
    }
}