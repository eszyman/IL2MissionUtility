//! Place fighter packs on the Korea map inside one coalition's influence area.

use geo::{BoundingRect, Contains, Coord, LineString, MultiPolygon, Point, Polygon};

use crate::frontlines::{densify, AOI_GAP};
use crate::mapclip::{apply_salients, influence_minus_salients, WorldAabb};

/// Hard cap on NodeGates packs written into one base map.
pub const MAX_PACKS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FighterSpot {
    pub x: f64,
    pub z: f64,
    /// 1-based wave (preview only; not written as a map icon).
    pub wave: u32,
    /// Which generated N-pack this group belongs to.
    pub pack: u32,
    /// 1-based `Group N` index inside that pack.
    pub slot: u32,
}

#[derive(Clone, Debug)]
pub struct MapFighterLayout {
    pub eastern: bool,
    pub spots: Vec<FighterSpot>,
}

#[derive(Clone, Copy, Debug)]
struct GridPt {
    x: f64,
    z: f64,
    row: i32,
    col: i32,
}

/// Country used when placing that coalition. Eastern keeps a 500-series setpoint
/// (or 501 if the Fighter Pack page is on USA). NATO is always 601.
pub fn country_for_coalition(eastern: bool, selected: i32) -> i32 {
    if eastern {
        if selected / 100 == 5 {
            selected
        } else {
            501
        }
    } else {
        601
    }
}

/// Friendly-rear corner of the AO for an RTB waypoint.
///
/// Eastern parks on the north edge (`x_max`), NATO on the south (`x_min`).
/// Of those two rear corners, the one closer to the group is used (west vs
/// east). Equal distance prefers west.
pub fn rtb_ao_point(eastern: bool, group_x: f64, group_z: f64, aabb: WorldAabb) -> (f64, f64) {
    let rear_x = if eastern { aabb.x_max } else { aabb.x_min };
    let west = (rear_x, aabb.z_min);
    let east = (rear_x, aabb.z_max);
    let d_west = dist2(group_x, group_z, west.0, west.1);
    let d_east = dist2(group_x, group_z, east.0, east.1);
    if d_east < d_west {
        east
    } else {
        west
    }
}

fn dist2(x0: f64, z0: f64, x1: f64, z1: f64) -> f64 {
    let dx = x0 - x1;
    let dz = z0 - z1;
    dx * dx + dz * dz
}

/// Checkerboard fighter pins in the coalition half of the AO.
pub fn place_in_coalition(
    eastern: bool,
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    salients: &[Vec<(f64, f64)>],
    stretch_east: bool,
    min_spacing: f64,
    waves: usize,
    groups_per_wave: usize,
    fill: bool,
) -> Result<MapFighterLayout, String> {
    let waves = waves.clamp(1, MAX_PACKS);
    let groups_per_wave = groups_per_wave.clamp(1, 10);
    let min_spacing = min_spacing.max(1_000.0);
    let dense = densify(front_xz, 4_000.0);
    if dense.len() < 2 {
        return Err("draw or select a front before placing fighters.".into());
    }
    let (_, patches) = apply_salients(dense.clone(), salients);
    let Some((north, south)) =
        influence_minus_salients(&dense, aabb, AOI_GAP, true, stretch_east, &patches)
    else {
        return Err("could not build areas of influence for this front.".into());
    };
    let mp = if eastern { north } else { south };
    let spots = checkerboard_spots(&mp, min_spacing, waves, groups_per_wave, fill);
    if spots.is_empty() {
        return Err(
            "coalition area is too small for Zone IN spacing. Enlarge the AO or lower linked groups."
                .into(),
        );
    }
    Ok(MapFighterLayout { eastern, spots })
}

fn checkerboard_spots(
    mp: &MultiPolygon<f64>,
    min_step: f64,
    waves: usize,
    groups_per_wave: usize,
    fill: bool,
) -> Vec<FighterSpot> {
    let want = waves.saturating_mul(groups_per_wave).max(1);
    let step = if fill {
        min_step
    } else {
        spread_step(mp, min_step, want)
    };
    let mut cells = grid_candidates(mp, step);
    if cells.is_empty() {
        return Vec::new();
    }
    let cap = if fill {
        (MAX_PACKS * groups_per_wave).min(cells.len())
    } else {
        want.min(cells.len())
    };
    if cells.len() > cap {
        cells = subsample(cells, cap);
    }

    let mut buckets: Vec<Vec<GridPt>> = vec![Vec::new(); waves];
    for p in cells {
        let w = (p.row + p.col).rem_euclid(waves as i32) as usize;
        buckets[w].push(p);
    }

    let mut spots = Vec::new();
    let mut pack = 0u32;
    for (wi, bucket) in buckets.iter().enumerate() {
        let wave = (wi + 1) as u32;
        if fill {
            for chunk in bucket.chunks(groups_per_wave) {
                if pack as usize >= MAX_PACKS {
                    break;
                }
                push_pack(&mut spots, chunk, wave, pack);
                pack += 1;
            }
        } else {
            let take: Vec<GridPt> = bucket.iter().copied().take(groups_per_wave).collect();
            if take.is_empty() {
                continue;
            }
            push_pack(&mut spots, &take, wave, pack);
            pack += 1;
        }
    }
    spots
}

fn push_pack(spots: &mut Vec<FighterSpot>, chunk: &[GridPt], wave: u32, pack: u32) {
    for (i, p) in chunk.iter().enumerate() {
        spots.push(FighterSpot {
            x: p.x,
            z: p.z,
            wave,
            pack,
            slot: (i + 1) as u32,
        });
    }
}

fn spread_step(mp: &MultiPolygon<f64>, min_step: f64, want: usize) -> f64 {
    let Some(rect) = mp.bounding_rect() else {
        return min_step;
    };
    let area = rect.width() * rect.height();
    if area <= 0.0 || want == 0 {
        return min_step;
    }
    let mut step = (area / want as f64).sqrt().max(min_step);
    for _ in 0..14 {
        let n = grid_candidates(mp, step).len();
        if n >= want && n <= want.saturating_mul(4) {
            break;
        }
        if n < want {
            let next = (step * 0.82).max(min_step);
            if (next - min_step).abs() < 1.0 {
                step = min_step;
                break;
            }
            step = next;
        } else {
            step *= 1.15;
        }
    }
    step.max(min_step)
}

fn grid_candidates(mp: &MultiPolygon<f64>, step: f64) -> Vec<GridPt> {
    let Some(rect) = mp.bounding_rect() else {
        return Vec::new();
    };
    if step < 100.0 {
        return Vec::new();
    }
    let x0 = rect.min().x;
    let z0 = rect.min().y;
    let x1 = rect.max().x;
    let z1 = rect.max().y;
    let mut out = Vec::new();
    let mut row = 0i32;
    let mut x = x0 + step * 0.5;
    while x <= x1 - step * 0.2 {
        let mut col = 0i32;
        let mut z = z0 + step * 0.5;
        while z <= z1 - step * 0.2 {
            if mp_contains(mp, x, z) {
                out.push(GridPt { x, z, row, col });
            }
            z += step;
            col += 1;
        }
        x += step;
        row += 1;
    }
    out
}

fn subsample(pts: Vec<GridPt>, n: usize) -> Vec<GridPt> {
    if pts.len() <= n {
        return pts;
    }
    (0..n)
        .map(|i| pts[i * pts.len() / n])
        .collect()
}

fn mp_contains(mp: &MultiPolygon<f64>, x: f64, z: f64) -> bool {
    mp.contains(&Point::new(x, z))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_mp(x0: f64, z0: f64, x1: f64, z1: f64) -> MultiPolygon<f64> {
        let ring = LineString::from(vec![
            Coord { x: x0, y: z0 },
            Coord { x: x1, y: z0 },
            Coord { x: x1, y: z1 },
            Coord { x: x0, y: z1 },
            Coord { x: x0, y: z0 },
        ]);
        MultiPolygon(vec![Polygon::new(ring, vec![])])
    }

    fn min_pair_dist(spots: &[FighterSpot]) -> f64 {
        let mut best = f64::MAX;
        for i in 0..spots.len() {
            for j in (i + 1)..spots.len() {
                let d = (spots[i].x - spots[j].x).hypot(spots[i].z - spots[j].z);
                if d < best {
                    best = d;
                }
            }
        }
        best
    }

    #[test]
    fn country_split_is_semi_automatic() {
        assert_eq!(country_for_coalition(true, 502), 502);
        assert_eq!(country_for_coalition(true, 601), 501);
        assert_eq!(country_for_coalition(false, 501), 601);
        assert_eq!(country_for_coalition(false, 601), 601);
    }

    #[test]
    fn rtb_uses_coalition_ao_corner() {
        let aabb = WorldAabb::from_corners(80_000.0, 200_000.0, 160_000.0, 300_000.0);
        let nato = rtb_ao_point(false, 120_000.0, 250_000.0, aabb);
        assert!((nato.0 - 80_000.0).abs() < 0.1);
        assert!((nato.1 - 200_000.0).abs() < 0.1);
        let east = rtb_ao_point(true, 120_000.0, 250_000.0, aabb);
        assert!((east.0 - 160_000.0).abs() < 0.1);
        assert!((east.1 - 200_000.0).abs() < 0.1);
        let near_east = rtb_ao_point(false, 100_000.0, 298_000.0, aabb);
        assert!((near_east.0 - 80_000.0).abs() < 0.1);
        assert!((near_east.1 - 300_000.0).abs() < 0.1);
        let east_east = rtb_ao_point(true, 140_000.0, 290_000.0, aabb);
        assert!((east_east.0 - 160_000.0).abs() < 0.1);
        assert!((east_east.1 - 300_000.0).abs() < 0.1);
    }

    #[test]
    fn checkerboard_respects_zone_in_and_mixes_waves() {
        let mp = rect_mp(100_000.0, 100_000.0, 164_000.0, 164_000.0);
        let spots = checkerboard_spots(&mp, 16_000.0, 2, 5, false);
        assert!(!spots.is_empty());
        assert!(spots.len() <= 10);
        assert!(min_pair_dist(&spots) + 1.0 >= 16_000.0);
        let w1 = spots.iter().filter(|s| s.wave == 1).count();
        let w2 = spots.iter().filter(|s| s.wave == 2).count();
        assert!(w1 >= 1 && w2 >= 1, "waves should mix, got {w1} and {w2}");
        let z1: f64 = spots.iter().filter(|s| s.wave == 1).map(|s| s.z).sum::<f64>() / w1 as f64;
        let z2: f64 = spots.iter().filter(|s| s.wave == 2).map(|s| s.z).sum::<f64>() / w2 as f64;
        assert!(
            (z1 - z2).abs() < 24_000.0,
            "waves should interleave, not sit in opposite corners ({z1} vs {z2})"
        );
        for s in &spots {
            assert!(mp_contains(&mp, s.x, s.z));
        }
    }

    #[test]
    fn two_waves_are_separate_packs() {
        let mp = rect_mp(80_000.0, 80_000.0, 176_000.0, 176_000.0);
        let spots = checkerboard_spots(&mp, 16_000.0, 2, 5, false);
        let packs: std::collections::BTreeSet<u32> = spots.iter().map(|s| s.pack).collect();
        assert_eq!(packs.len(), 2);
        for p in packs {
            let n = spots.iter().filter(|s| s.pack == p).count();
            assert!(n >= 1 && n <= 5);
            let waves: std::collections::BTreeSet<u32> = spots
                .iter()
                .filter(|s| s.pack == p)
                .map(|s| s.wave)
                .collect();
            assert_eq!(waves.len(), 1);
        }
    }

    #[test]
    fn spread_opens_up_when_the_zone_is_large() {
        let mp = rect_mp(50_000.0, 50_000.0, 250_000.0, 250_000.0);
        let tight = checkerboard_spots(&mp, 16_000.0, 2, 4, true);
        let spread = checkerboard_spots(&mp, 16_000.0, 2, 4, false);
        assert!(spread.len() <= 8);
        assert!(tight.len() >= spread.len());
        if spread.len() >= 2 && tight.len() >= 2 {
            assert!(min_pair_dist(&spread) + 50.0 >= min_pair_dist(&tight));
        }
    }

    #[test]
    fn fill_caps_at_max_packs() {
        let mp = rect_mp(0.0, 0.0, 400_000.0, 400_000.0);
        let spots = checkerboard_spots(&mp, 16_000.0, 2, 5, true);
        let packs: std::collections::BTreeSet<u32> = spots.iter().map(|s| s.pack).collect();
        assert!(packs.len() <= MAX_PACKS);
        assert!(spots.len() <= MAX_PACKS * 5);
    }

    #[test]
    fn tiny_zone_returns_what_fits() {
        let mp = rect_mp(100_000.0, 100_000.0, 118_000.0, 118_000.0);
        let spots = checkerboard_spots(&mp, 16_000.0, 2, 5, false);
        assert!(spots.len() <= 2);
    }
}
