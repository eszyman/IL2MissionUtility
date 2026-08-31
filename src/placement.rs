//! Park generated copies on a compact square grid from the lower left.

use std::collections::HashSet;

use crate::ast::Il2Entity;

/// Lower-left of the usable map.
pub const MAP_MIN: f64 = 40_000.0;
/// Upper-right of the usable map.
pub const MAP_MAX: f64 = 470_000.0;
/// Center-to-center spacing between parked copies.
pub const GRID_STEP: f64 = 10_000.0;

/// Side length of the square that can hold `count` copies (2→2, 4→2, 9→3, 93→10).
pub fn grid_side(count: usize) -> usize {
    let n = count.max(1);
    (n as f64).sqrt().ceil() as usize
}

pub fn grid_xz(slot: usize, count: usize) -> (f64, f64) {
    grid_xz_at(slot, count, MAP_MIN, MAP_MIN)
}

pub fn grid_xz_at(slot: usize, count: usize, origin_x: f64, origin_z: f64) -> (f64, f64) {
    let side = grid_side(count);
    let col = slot % side;
    let row = slot / side;
    (
        origin_x + col as f64 * GRID_STEP,
        origin_z + row as f64 * GRID_STEP,
    )
}

/// Lower-left of each template's square, left-to-right then north, with a 10 km gutter.
pub fn template_square_origins(counts: &[usize]) -> Vec<(f64, f64)> {
    let gutter = GRID_STEP;
    let mut out = Vec::with_capacity(counts.len());
    let mut x = MAP_MIN;
    let mut z = MAP_MIN;
    let mut row_h = 0.0;
    for &n in counts {
        let side = grid_side(n.max(1));
        let w = side as f64 * GRID_STEP;
        let h = w;
        if x > MAP_MIN && x + w - GRID_STEP > MAP_MAX {
            x = MAP_MIN;
            z += row_h + gutter;
            row_h = 0.0;
        }
        out.push((x, z));
        x += w + gutter;
        row_h = row_h.max(h);
    }
    out
}

/// Translate `entity` so its first X/Z lands on grid slot `slot` of `count`.
/// Returns the (dx, dz) applied so related objects can share the move.
pub fn move_to_grid(entity: &mut Il2Entity, slot: usize, count: usize) -> (f64, f64) {
    move_to_grid_at(entity, slot, count, MAP_MIN, MAP_MIN)
}

pub fn move_to_grid_at(
    entity: &mut Il2Entity,
    slot: usize,
    count: usize,
    origin_x: f64,
    origin_z: f64,
) -> (f64, f64) {
    let (tx, tz) = grid_xz_at(slot, count, origin_x, origin_z);
    let (x, z) = entity.first_xz().unwrap_or((0.0, 0.0));
    let dx = tx - x;
    let dz = tz - z;
    entity.translate_xz(dx, dz);
    (dx, dz)
}

/// Translate `entity` so `anchor` lands on `target`. Returns the (dx, dz) applied.
pub fn move_anchor_to(entity: &mut Il2Entity, anchor: (f64, f64), target: (f64, f64)) -> (f64, f64) {
    let dx = target.0 - anchor.0;
    let dz = target.1 - anchor.1;
    entity.translate_xz(dx, dz);
    (dx, dz)
}

/// Yaw a copy so it faces `heading_deg` (0 = north / +X, 90 = east / +Z).
///
/// Only world visuals move: objects with a `Model` (vehicles, ships, planes),
/// `Block` / `Ground`, and `MCU_TR_Entity` nodes linked from those models via
/// `LinkTrId`. Timers, checkzones, AttackArea, and other logic stay put.
///
/// The pivot is the centroid of every `Model`. That keeps a Template Builder
/// battery together without spinning it around a distant command MCU.
pub fn apply_group_heading(root: &mut Il2Entity, heading_deg: f64) {
    if heading_deg.rem_euclid(360.0).abs() < 1e-9 {
        return;
    }
    let models = model_positions(root);
    let pivot = if models.is_empty() {
        return;
    } else {
        let n = models.len() as f64;
        (
            models.iter().map(|p| p.0).sum::<f64>() / n,
            models.iter().map(|p| p.1).sum::<f64>() / n,
        )
    };
    let linked = linked_tr_ids(root);
    let rad = heading_deg.to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    rotate_visual_walk(root, pivot, sin, cos, heading_deg, &linked);
}

fn has_model(entity: &Il2Entity) -> bool {
    entity.property("Model").is_some()
}

fn linked_tr_ids(root: &Il2Entity) -> HashSet<i32> {
    let mut ids = HashSet::new();
    root.for_each(&mut |e| {
        if has_model(e) {
            if let Some(id) = e.property("LinkTrId").and_then(|v| v.parse().ok()) {
                ids.insert(id);
            }
        }
    });
    ids
}

fn is_visual_body(entity: &Il2Entity, linked: &HashSet<i32>) -> bool {
    if has_model(entity) {
        return true;
    }
    matches!(entity.block_type.as_str(), "Block" | "Ground")
        || (entity.block_type == "MCU_TR_Entity"
            && entity.index.is_some_and(|id| linked.contains(&id)))
}

fn model_positions(entity: &Il2Entity) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    collect_model_positions(entity, &mut out);
    out
}

fn collect_model_positions(entity: &Il2Entity, out: &mut Vec<(f64, f64)>) {
    if has_model(entity) {
        if let Some(p) = entity.pos_xz() {
            out.push(p);
        }
    }
    for child in &entity.children {
        collect_model_positions(child, out);
    }
}

fn rotate_visual_walk(
    entity: &mut Il2Entity,
    pivot: (f64, f64),
    sin: f64,
    cos: f64,
    heading_deg: f64,
    linked: &HashSet<i32>,
) {
    if is_visual_body(entity, linked) {
        if let Some((x, z)) = entity.pos_xz() {
            let dx = x - pivot.0;
            let dz = z - pivot.1;
            let nx = pivot.0 + dx * cos - dz * sin;
            let nz = pivot.1 + dx * sin + dz * cos;
            set_coord(entity, "XPos", nx);
            set_coord(entity, "ZPos", nz);
        }
        if entity.property("YOri").is_some() || has_model(entity) {
            add_yori(entity, heading_deg);
        }
    }
    for child in &mut entity.children {
        rotate_visual_walk(child, pivot, sin, cos, heading_deg, linked);
    }
}

fn set_coord(entity: &mut Il2Entity, key: &str, value: f64) {
    let decimals = entity
        .property(key)
        .and_then(|v| v.split('.').nth(1))
        .map(|s| s.len())
        .unwrap_or(3);
    entity.set_property(key, format!("{value:.decimals$}"));
}

fn add_yori(entity: &mut Il2Entity, delta: f64) {
    let raw = entity.property("YOri");
    let current = raw.and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let decimals = raw
        .and_then(|v| v.split('.').nth(1))
        .map(|s| s.len())
        .unwrap_or(3);
    let next = (current + delta).rem_euclid(360.0);
    entity.set_property("YOri", format!("{next:.decimals$}"));
}

/// Yaw in degrees from `from` toward `to`. 0 = north (+X), 90 = east (+Z).
pub fn heading_toward(from: (f64, f64), to: (f64, f64)) -> f64 {
    let dx = to.0 - from.0;
    let dz = to.1 - from.1;
    if dx * dx + dz * dz < 1.0 {
        return 0.0;
    }
    dz.atan2(dx).to_degrees().rem_euclid(360.0)
}

/// Mix `seed` and `index` into a 64-bit value.
pub fn mix_index(seed: u64, index: u64) -> u64 {
    let mut n = seed
        ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ index.rotate_left(17);
    n ^= n >> 30;
    n = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94D0_49BB_1331_11EB);
    n ^= n >> 31;
    n
}

/// Pick `items[hash % len]`.
pub fn hashed_pick<'a, T>(items: &'a [T], seed: u64, index: u64) -> Option<&'a T> {
    if items.is_empty() {
        return None;
    }
    let i = (mix_index(seed, index) as usize) % items.len();
    items.get(i)
}

/// Keep `n` points. With `targets`, each pick takes the leftover point closest
/// to a hashed target. Without targets, spread evenly through the list.
pub fn subsample_favoring(
    pts: Vec<(f64, f64)>,
    n: usize,
    targets: &[(f64, f64)],
    seed: u64,
) -> Vec<(f64, f64)> {
    subsample_spaced(pts, n, targets, seed, 0.0, &[])
}

/// Like [`subsample_favoring`], but skips candidates closer than `min_dist` to
/// an already chosen point or to `occupied`.
pub fn subsample_spaced(
    pts: Vec<(f64, f64)>,
    n: usize,
    targets: &[(f64, f64)],
    seed: u64,
    min_dist: f64,
    occupied: &[(f64, f64)],
) -> Vec<(f64, f64)> {
    if n == 0 || pts.is_empty() {
        return Vec::new();
    }
    let min_d2 = min_dist * min_dist;
    let far = |p: (f64, f64), chosen: &[(f64, f64)]| {
        occupied.iter().chain(chosen.iter()).all(|&(x, z)| {
            let dx = p.0 - x;
            let dz = p.1 - z;
            dx * dx + dz * dz >= min_d2
        })
    };
    let mut left = pts;
    let mut out = Vec::with_capacity(n.min(left.len()));
    if !targets.is_empty() {
        for i in 0..n {
            if left.is_empty() {
                break;
            }
            let t = hashed_pick(targets, seed, i as u64).copied().unwrap();
            let mut best = None;
            let mut best_d = f64::MAX;
            for (j, p) in left.iter().enumerate() {
                if !far(*p, &out) {
                    continue;
                }
                let d = (p.0 - t.0) * (p.0 - t.0) + (p.1 - t.1) * (p.1 - t.1);
                if d < best_d {
                    best_d = d;
                    best = Some(j);
                }
            }
            match best {
                Some(j) => out.push(left.swap_remove(j)),
                None => break,
            }
        }
        for p in left {
            if out.len() >= n {
                break;
            }
            if far(p, &out) {
                out.push(p);
            }
        }
    } else if left.len() > n {
        let mut taken = vec![false; left.len()];
        for i in 0..n {
            let j = i * left.len() / n;
            if !taken[j] && far(left[j], &out) {
                taken[j] = true;
                out.push(left[j]);
            }
        }
        for (j, p) in left.iter().enumerate() {
            if out.len() >= n {
                break;
            }
            if taken[j] {
                continue;
            }
            if far(*p, &out) {
                out.push(*p);
            }
        }
    } else {
        for p in left {
            if out.len() >= n {
                break;
            }
            if far(p, &out) {
                out.push(p);
            }
        }
    }
    out
}

/// Minimum centre-to-centre spacing for automatically placed unit groups (metres).
pub const UNIT_PLACE_SPACING: f64 = 4_500.0;

/// Optional front-band filter and objective-favoring for map unit placement.
#[derive(Clone, Copy)]
pub struct PlaceOpts<'a> {
    /// Keep candidates within this many metres of the front, if any.
    pub front_band: Option<f64>,
    /// Objectives (or other points) to bias picks toward.
    pub favor: &'a [(f64, f64)],
    pub seed: u64,
    /// Already placed unit positions to keep clear of.
    pub occupied: &'a [(f64, f64)],
}

impl PlaceOpts<'static> {
    pub const UNCONSTRAINED: Self = Self {
        front_band: None,
        favor: &[],
        seed: 0,
        occupied: &[],
    };
}

/// Heading from `from` to the nearest point in `targets`, if any.
pub fn heading_toward_nearest(from: (f64, f64), targets: &[(f64, f64)]) -> Option<f64> {
    let mut best: Option<(f64, f64, f64)> = None;
    for &(x, z) in targets {
        let d = (x - from.0) * (x - from.0) + (z - from.1) * (z - from.1);
        let closer = match best {
            None => true,
            Some((bd, _, _)) => d < bd,
        };
        if closer {
            best = Some((d, x, z));
        }
    }
    best.map(|(_, x, z)| heading_toward(from, (x, z)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Il2Entity;

    fn at(x: f64, z: f64) -> Il2Entity {
        let mut e = Il2Entity::new("Vehicle");
        e.set_property("XPos", format!("{x:.3}"));
        e.set_property("YPos", "0.000");
        e.set_property("ZPos", format!("{z:.3}"));
        e
    }

    #[test]
    fn square_side_for_typical_counts() {
        assert_eq!(grid_side(1), 1);
        assert_eq!(grid_side(2), 2);
        assert_eq!(grid_side(4), 2);
        assert_eq!(grid_side(9), 3);
        assert_eq!(grid_side(10), 4);
        assert_eq!(grid_side(93), 10);
    }

    #[test]
    fn nine_copies_fill_a_3x3_from_lower_left() {
        assert_eq!(grid_xz(0, 9), (MAP_MIN, MAP_MIN));
        assert_eq!(grid_xz(2, 9), (MAP_MIN + 2.0 * GRID_STEP, MAP_MIN));
        assert_eq!(grid_xz(3, 9), (MAP_MIN, MAP_MIN + GRID_STEP));
        assert_eq!(grid_xz(8, 9), (MAP_MIN + 2.0 * GRID_STEP, MAP_MIN + 2.0 * GRID_STEP));
        let (x, z) = grid_xz(8, 9);
        assert!(x < MAP_MAX);
        assert!(z < MAP_MAX);
    }

    #[test]
    fn ninety_three_stays_in_a_10x10_block() {
        let (x, z) = grid_xz(92, 93);
        assert!((x - (MAP_MIN + 2.0 * GRID_STEP)).abs() < 0.01);
        assert!((z - (MAP_MIN + 9.0 * GRID_STEP)).abs() < 0.01);
        assert!(x <= MAP_MIN + 9.0 * GRID_STEP);
        assert!(z <= MAP_MIN + 9.0 * GRID_STEP);
    }

    #[test]
    fn template_squares_sit_side_by_side_with_a_gutter() {
        let origins = template_square_origins(&[4, 1, 9]);
        assert_eq!(origins[0], (MAP_MIN, MAP_MIN));
        // 2×2 occupies two cells, then a 10 km gutter.
        assert_eq!(origins[1], (MAP_MIN + 3.0 * GRID_STEP, MAP_MIN));
        // 1×1 occupies one cell, then a gutter, then the 3×3.
        assert_eq!(origins[2], (MAP_MIN + 5.0 * GRID_STEP, MAP_MIN));
    }

    #[test]
    fn move_to_grid_keeps_internal_offset() {
        let mut root = Il2Entity::new("Group");
        root.children.push(at(12_000.0, 8_000.0));
        root.children.push(at(12_250.0, 8_000.0));
        move_to_grid(&mut root, 0, 1);
        let (x0, z0) = root.children[0].pos_xz().unwrap();
        let (x1, z1) = root.children[1].pos_xz().unwrap();
        assert!((x0 - MAP_MIN).abs() < 0.01);
        assert!((z0 - MAP_MIN).abs() < 0.01);
        assert!((x1 - x0 - 250.0).abs() < 0.01);
        assert!((z1 - z0).abs() < 0.01);
    }

    fn ship_at(x: f64, z: f64, yori: f64) -> Il2Entity {
        let mut e = Il2Entity::new("Ship");
        e.set_name("GunBoat");
        e.set_property("XPos", format!("{x:.3}"));
        e.set_property("YPos", "0.000");
        e.set_property("ZPos", format!("{z:.3}"));
        e.set_property("YOri", format!("{yori:.3}"));
        e.set_property("Model", "\"graphics\\ships\\foo.mgm\"");
        e
    }

    fn mcu_at(x: f64, z: f64) -> Il2Entity {
        let mut e = Il2Entity::new("MCU_TR_Entity");
        e.set_property("XPos", format!("{x:.3}"));
        e.set_property("ZPos", format!("{z:.3}"));
        e.set_property("YOri", "0.000");
        e
    }

    fn block_at(x: f64, z: f64) -> Il2Entity {
        let mut e = Il2Entity::new("Block");
        e.set_name("Sandbags");
        e.set_property("XPos", format!("{x:.3}"));
        e.set_property("YPos", "0.000");
        e.set_property("ZPos", format!("{z:.3}"));
        e.set_property("YOri", "0.000");
        e
    }

    #[test]
    fn heading_yaws_models_and_blocks_not_logic() {
        let mut root = Il2Entity::new("Group");
        root.set_name("Copy");
        root.children.push(ship_at(0.0, 0.0, 0.0));
        root.children.push(block_at(0.0, 100.0));
        root.children.push(mcu_at(50.0, 50.0));

        apply_group_heading(&mut root, 90.0);

        let ship = root.find_by_name("GunBoat").unwrap();
        let (sx, sz) = ship.pos_xz().unwrap();
        assert!((sx - 0.0).abs() < 0.02);
        assert!((sz - 0.0).abs() < 0.02);
        assert!((ship.property("YOri").unwrap().parse::<f64>().unwrap() - 90.0).abs() < 0.02);

        let block = root.find_by_name("Sandbags").unwrap();
        let (bx, bz) = block.pos_xz().unwrap();
        assert!(
            (bx + 100.0).abs() < 0.02,
            "block east of the model should become south (-X), got {bx}"
        );
        assert!(bz.abs() < 0.02);
        assert!((block.property("YOri").unwrap().parse::<f64>().unwrap() - 90.0).abs() < 0.02);

        let timer = &root.children[2];
        let (tx, tz) = timer.pos_xz().unwrap();
        assert!((tx - 50.0).abs() < 0.02);
        assert!((tz - 50.0).abs() < 0.02);
        assert_eq!(timer.property("YOri").unwrap(), "0.000");
    }

    #[test]
    fn heading_leaves_unlinked_commands_put() {
        let mut rotate = Il2Entity::new("Group");
        rotate.set_name("rotate");
        rotate.children.push(mcu_at(16_000.0, 0.0));
        rotate.children.push(ship_at(0.0, 0.0, 0.0));
        rotate.children.push(ship_at(0.0, 100.0, 0.0));
        let mut root = Il2Entity::new("Group");
        root.set_name("Copy");
        root.children.push(rotate);

        apply_group_heading(&mut root, 90.0);

        let ships: Vec<(f64, f64)> = root
            .find_by_name("rotate")
            .unwrap()
            .children
            .iter()
            .filter(|c| c.name() == Some("GunBoat"))
            .map(|c| c.pos_xz().unwrap())
            .collect();
        assert_eq!(ships.len(), 2);
        for (x, z) in &ships {
            assert!(
                x.abs() < 200.0 && z.abs() < 200.0,
                "models should orbit their own centroid, not the distant command, got ({x}, {z})"
            );
        }
        let cmd = root.find_by_name("rotate").unwrap().children[0].pos_xz().unwrap();
        assert!(
            (cmd.0 - 16_000.0).abs() < 0.02 && cmd.1.abs() < 0.02,
            "unlinked command MCU should stay put, got {cmd:?}"
        );
    }

    #[test]
    fn ml20_rotate_keeps_guns_near_logic() {
        use crate::parser::parse_group_file;
        let mut root = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK ML20 Arty V2 w_spawn.Group"
        ))
        .unwrap();
        let zone = root.find_by_name("Zone In").unwrap().pos_xz().unwrap();
        let gun_before = root.find_by_name("ml20").unwrap().pos_xz().unwrap();
        let dist_before = (gun_before.0 - zone.0).hypot(gun_before.1 - zone.1);
        apply_group_heading(&mut root, 90.0);
        let gun_after = root.find_by_name("ml20").unwrap().pos_xz().unwrap();
        let dist_after = (gun_after.0 - zone.0).hypot(gun_after.1 - zone.1);
        assert!(
            dist_after < dist_before + 2_000.0,
            "ML20 guns should stay near Zone In after yaw (before {dist_before:.0} m, after {dist_after:.0} m)"
        );
        assert!(
            (gun_after.0 - gun_before.0).abs() > 1.0
                || (gun_after.1 - gun_before.1).abs() > 1.0,
            "guns should actually move on XZ, stayed at {gun_after:?}"
        );
    }

    #[test]
    fn bm13_rotate_keeps_guns_near_logic() {
        use crate::parser::parse_group_file;
        let mut root = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK BM13 Arty V2 w_spawn.Group"
        ))
        .unwrap();
        let zone = root.find_by_name("Zone In").unwrap().pos_xz().unwrap();
        let gun_before = root.find_by_name("BM13").unwrap().pos_xz().unwrap();
        let dist_before = (gun_before.0 - zone.0).hypot(gun_before.1 - zone.1);
        apply_group_heading(&mut root, 90.0);
        let gun_after = root.find_by_name("BM13").unwrap().pos_xz().unwrap();
        let dist_after = (gun_after.0 - zone.0).hypot(gun_after.1 - zone.1);
        assert!(
            dist_after < dist_before + 2_000.0,
            "BM13 guns should stay near Zone In after yaw (before {dist_before:.0} m, after {dist_after:.0} m)"
        );
    }

    #[test]
    fn model_heading_yaws_the_model_and_leaves_unlinked_entity() {
        let mut root = Il2Entity::new("Group");
        root.set_name("Convoy");
        root.children.push(ship_at(0.0, 0.0, 0.227));
        root.children.push(mcu_at(0.0, 100.0));

        apply_group_heading(&mut root, 90.0);

        let ship = root.find_by_name("GunBoat").unwrap();
        assert!((ship.property("YOri").unwrap().parse::<f64>().unwrap() - 90.227).abs() < 0.02);
        let (mx, mz) = root.children[1].pos_xz().unwrap();
        assert!((mx - 0.0).abs() < 0.02);
        assert!((mz - 100.0).abs() < 0.02);
    }

    #[test]
    fn heading_wraps_past_360() {
        let mut ship = ship_at(0.0, 0.0, 350.0);
        apply_group_heading(&mut ship, 20.0);
        let y = ship.property("YOri").unwrap().parse::<f64>().unwrap();
        assert!((y - 10.0).abs() < 0.02);
    }

    #[test]
    fn heading_toward_north_east_and_nearest() {
        assert!((heading_toward((0.0, 0.0), (100.0, 0.0)) - 0.0).abs() < 0.01);
        assert!((heading_toward((0.0, 0.0), (0.0, 100.0)) - 90.0).abs() < 0.01);
        let h = heading_toward_nearest((0.0, 0.0), &[(10.0, 0.0), (0.0, 1000.0)]).unwrap();
        assert!((h - 0.0).abs() < 0.01);
        assert!(heading_toward_nearest((0.0, 0.0), &[]).is_none());
    }

    #[test]
    fn subsample_favoring_picks_near_hashed_target() {
        let pts = vec![(0.0, 0.0), (10.0, 0.0), (100.0, 0.0), (1000.0, 0.0)];
        let picked = subsample_favoring(pts, 1, &[(9.0, 0.0)], 1);
        assert_eq!(picked.len(), 1);
        assert!((picked[0].0 - 10.0).abs() < 0.01);
    }

    #[test]
    fn subsample_spaced_rejects_neighbors() {
        let pts: Vec<(f64, f64)> = (0..10).map(|i| (i as f64 * 1_000.0, 0.0)).collect();
        let picked = subsample_spaced(pts, 10, &[], 0, 4_500.0, &[]);
        assert!(picked.len() >= 2);
        for (i, a) in picked.iter().enumerate() {
            for b in &picked[i + 1..] {
                let d = (a.0 - b.0).hypot(a.1 - b.1);
                assert!(d + 1e-6 >= 4_500.0, "pair {a:?} {b:?} is {d} m");
            }
        }
    }
}
