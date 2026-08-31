//! Place randomizer ship groups on water inside one coalition's AO.

use geo::{Contains, Point};

use crate::frontlines::{densify, AOI_GAP};
use crate::geo::{MAP_MAX, MAP_MIN};
use crate::mapclip::{
    apply_salients, filter_front_band, influence_minus_salients, point_north_of_front, WorldAabb,
};
use crate::placement::{hashed_pick, heading_toward, subsample_spaced, PlaceOpts, UNIT_PLACE_SPACING};
use crate::watermap::WaterMap;

pub const MAX_SHIPS: usize = 64;
pub const SHIP_SPACING: f64 = 8_000.0;
pub const START_DELAY_S: f64 = 5.0;
pub const GROUP_DELAY_S: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipSpot {
    pub x: f64,
    pub z: f64,
    pub in_ao: bool,
    /// Degrees, 0 = north (+X), 90 = east (+Z). Templates are authored pointing north.
    pub heading_deg: f64,
}

#[derive(Clone, Debug)]
pub struct MapShipLayout {
    pub eastern: bool,
    pub spots: Vec<ShipSpot>,
}

impl MapShipLayout {
    /// Scatter headings 0..360 in tenth-degree steps. Same seed is deterministic.
    pub fn randomize_headings(&mut self, seed: u64) {
        for (i, spot) in self.spots.iter_mut().enumerate() {
            spot.heading_deg = mix_heading(seed, i as u64, spot.x, spot.z);
        }
    }

    /// Face each spot at a hashed objective (stable per index + seed).
    pub fn aim_at_hashed_objectives(&mut self, objectives: &[(f64, f64)], seed: u64) {
        if objectives.is_empty() {
            return;
        }
        for (i, spot) in self.spots.iter_mut().enumerate() {
            if let Some(&t) = hashed_pick(objectives, seed, i as u64) {
                spot.heading_deg = heading_toward((spot.x, spot.z), t);
            }
        }
    }
}

fn mix_heading(seed: u64, index: u64, x: f64, z: f64) -> f64 {
    let mut n = seed
        ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ x.to_bits()
        ^ z.to_bits().rotate_left(17);
    n ^= n >> 30;
    n = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94D0_49BB_1331_11EB);
    n ^= n >> 31;
    (n % 3600) as f64 / 10.0
}

/// Grid ships on coalition water inside the AO. Leftovers go to the nearest
/// friendly water outside the box.
pub fn place_ships(
    eastern: bool,
    count: usize,
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    salients: &[Vec<(f64, f64)>],
    stretch_east: bool,
    water: &WaterMap,
    opts: PlaceOpts<'_>,
) -> Result<MapShipLayout, String> {
    let count = count.clamp(1, MAX_SHIPS);
    let side = coalition_water(eastern, front_xz, aabb, salients, stretch_east);
    let mut inside = sample_water(water, aabb, SHIP_SPACING, &side);
    inside = filter_front_band(inside, front_xz, opts.front_band);
    inside = drop_near(inside, opts.occupied, UNIT_PLACE_SPACING);
    for step in [4_000.0, 2_000.0, 1_000.0] {
        if inside.len() >= count {
            break;
        }
        inside = sample_water(water, aabb, step, &side);
        inside = filter_front_band(inside, front_xz, opts.front_band);
        inside = drop_near(inside, opts.occupied, UNIT_PLACE_SPACING);
    }
    let take = count.min(inside.len());
    let mut spots: Vec<ShipSpot> = subsample_spaced(
        inside,
        take,
        opts.favor,
        opts.seed,
        UNIT_PLACE_SPACING,
        opts.occupied,
    )
        .into_iter()
        .map(|(x, z)| ShipSpot {
            x,
            z,
            in_ao: true,
            heading_deg: 0.0,
        })
        .collect();

    if spots.len() < count {
        let outside_box = WorldAabb::full_map();
        let mut outside = sample_water(water, outside_box, SHIP_SPACING, &side);
        outside.retain(|(x, z)| !aabb.contains(*x, *z));
        outside = filter_front_band(outside, front_xz, opts.front_band);
        outside = drop_near(outside, opts.occupied, UNIT_PLACE_SPACING);
        for step in [4_000.0, 2_000.0, 1_000.0] {
            if outside.len() >= count - spots.len() {
                break;
            }
            outside = sample_water(water, outside_box, step, &side);
            outside.retain(|(x, z)| !aabb.contains(*x, *z));
            outside = filter_front_band(outside, front_xz, opts.front_band);
            outside = drop_near(outside, opts.occupied, UNIT_PLACE_SPACING);
        }
        let mut blocked = opts.occupied.to_vec();
        blocked.extend(spots.iter().map(|s| (s.x, s.z)));
        let need = count - spots.len();
        let extra = subsample_spaced(
            outside,
            need,
            opts.favor,
            opts.seed.wrapping_add(1),
            UNIT_PLACE_SPACING,
            &blocked,
        );
        for (x, z) in extra {
            spots.push(ShipSpot {
                x,
                z,
                in_ao: false,
                heading_deg: 0.0,
            });
        }
    }

    if spots.is_empty() {
        return Err(
            "no water on that coalition's side of the front. Enlarge the AO or pick the other coalition."
                .into(),
        );
    }
    Ok(MapShipLayout { eastern, spots })
}

enum WaterSide {
    Any,
    Polygon(geo::MultiPolygon<f64>),
    Front { eastern: bool, front: Vec<(f64, f64)> },
}

fn coalition_water(
    eastern: bool,
    front_xz: &[(f64, f64)],
    aabb: WorldAabb,
    salients: &[Vec<(f64, f64)>],
    stretch_east: bool,
) -> WaterSide {
    let dense = densify(front_xz, 4_000.0);
    if dense.len() < 2 {
        return WaterSide::Any;
    }
    let (_, patches) = apply_salients(dense.clone(), salients);
    if let Some((north, south)) =
        influence_minus_salients(&dense, aabb, AOI_GAP, true, stretch_east, &patches)
    {
        WaterSide::Polygon(if eastern { north } else { south })
    } else {
        WaterSide::Front {
            eastern,
            front: dense,
        }
    }
}

fn allowed(side: &WaterSide, x: f64, z: f64) -> bool {
    match side {
        WaterSide::Any => true,
        WaterSide::Polygon(mp) => mp.contains(&Point::new(x, z)),
        WaterSide::Front { eastern, front } => point_north_of_front(front, x, z) == *eastern,
    }
}

fn sample_water(
    water: &WaterMap,
    aabb: WorldAabb,
    step: f64,
    side: &WaterSide,
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
            if water.is_water_xz(x, z) && allowed(side, x, z) {
                out.push((x, z));
            }
            z += step;
        }
        x += step;
    }
    out
}

fn drop_near(pts: Vec<(f64, f64)>, used: &[(f64, f64)], min_d: f64) -> Vec<(f64, f64)> {
    if used.is_empty() {
        return pts;
    }
    let m2 = min_d * min_d;
    pts.into_iter()
        .filter(|&(x, z)| {
            used.iter().all(|&(ux, uz)| {
                let dx = x - ux;
                let dz = z - uz;
                dx * dx + dz * dz >= m2
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_map(width: u32, cells: Vec<u8>) -> WaterMap {
        let mut bytes = Vec::from(*b"WMAP");
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&cells);
        WaterMap::from_bytes(&bytes).unwrap()
    }

    fn all_water() -> WaterMap {
        let w = 8u32;
        pack_map(w, vec![1u8; (w * w) as usize])
    }

    fn west_water_only() -> WaterMap {
        let w = 8u32;
        let mut cells = vec![0u8; (w * w) as usize];
        for y in 0..w {
            for x in 0..2 {
                cells[(y * w + x) as usize] = 1;
            }
        }
        pack_map(w, cells)
    }

    #[test]
    fn grid_fills_ao_water() {
        let water = all_water();
        let aabb = WorldAabb::from_corners(100_000.0, 100_000.0, 180_000.0, 180_000.0);
        let layout = place_ships(true, 4, &[], aabb, &[], true, &water, PlaceOpts::UNCONSTRAINED).unwrap();
        assert_eq!(layout.spots.len(), 4);
        assert!(layout.spots.iter().all(|s| s.in_ao));
        for s in &layout.spots {
            assert!(aabb.contains(s.x, s.z));
            assert!(water.is_water_xz(s.x, s.z));
        }
    }

    #[test]
    fn landlocked_ao_overflows_to_water_outside() {
        let water = west_water_only();
        let aabb = WorldAabb::from_corners(200_000.0, 300_000.0, 280_000.0, 380_000.0);
        let layout = place_ships(false, 3, &[], aabb, &[], true, &water, PlaceOpts::UNCONSTRAINED).unwrap();
        assert_eq!(layout.spots.len(), 3);
        assert!(layout.spots.iter().all(|s| !s.in_ao));
        assert!(layout.spots.iter().all(|s| water.is_water_xz(s.x, s.z)));
    }

    #[test]
    fn front_keeps_eastern_ships_north() {
        let water = all_water();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 200_000.0, 200_000.0);
        let front = vec![(140_000.0, 80_000.0), (140_000.0, 200_000.0)];
        let layout = place_ships(true, 6, &front, aabb, &[], true, &water, PlaceOpts::UNCONSTRAINED).unwrap();
        for s in &layout.spots {
            if s.in_ao {
                assert!(
                    s.x > 140_000.0 - 1.0,
                    "eastern ship should sit north of the front, got x={}",
                    s.x
                );
            }
        }
    }

    #[test]
    fn randomize_headings_is_deterministic_and_in_range() {
        let water = all_water();
        let aabb = WorldAabb::from_corners(100_000.0, 100_000.0, 180_000.0, 180_000.0);
        let mut a = place_ships(true, 4, &[], aabb, &[], true, &water, PlaceOpts::UNCONSTRAINED).unwrap();
        let mut b = a.clone();
        a.randomize_headings(42);
        b.randomize_headings(42);
        assert_eq!(
            a.spots.iter().map(|s| s.heading_deg).collect::<Vec<_>>(),
            b.spots.iter().map(|s| s.heading_deg).collect::<Vec<_>>()
        );
        assert!(a.spots.iter().all(|s| (0.0..360.0).contains(&s.heading_deg)));
        b.randomize_headings(43);
        assert_ne!(
            a.spots.iter().map(|s| s.heading_deg).collect::<Vec<_>>(),
            b.spots.iter().map(|s| s.heading_deg).collect::<Vec<_>>()
        );
    }

    #[test]
    fn placed_ships_are_at_least_4500m_apart() {
        let water = all_water();
        let aabb = WorldAabb::from_corners(80_000.0, 80_000.0, 220_000.0, 220_000.0);
        let layout =
            place_ships(true, 8, &[], aabb, &[], true, &water, PlaceOpts::UNCONSTRAINED).unwrap();
        assert_eq!(layout.spots.len(), 8);
        for (i, a) in layout.spots.iter().enumerate() {
            for b in &layout.spots[i + 1..] {
                let d = (a.x - b.x).hypot(a.z - b.z);
                assert!(
                    d + 1.0 >= UNIT_PLACE_SPACING,
                    "ships ({}, {}) and ({}, {}) are {d:.0} m apart",
                    a.x,
                    a.z,
                    b.x,
                    b.z
                );
            }
        }
    }
}
