//! Fighter-pack generation: duplicate **Group 1** and rebuild **NodeGates**.
//!
//! Aircraft composition is applied to Group 1 first (`configure_aircraft`);
//! this module then clones that group N times and relinks NodeGates.

use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::parser::parse_group_file;
use crate::placement;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackInfo {
    pub existing_groups: usize,
    pub plane_count: usize,
    pub spawn_choices: usize,
    pub has_node_gates: bool,
}

struct GateCell {
    in_disable: Il2Entity,
    out_disable: Il2Entity,
    in_enable: Il2Entity,
    out_enable: Il2Entity,
    fanout_enable: Il2Entity,
    fanout_disable: Il2Entity,
}

impl GateCell {
    fn into_timers(self) -> Vec<Il2Entity> {
        vec![
            self.in_disable,
            self.out_disable,
            self.in_enable,
            self.out_enable,
            self.fanout_enable,
            self.fanout_disable,
        ]
    }
}

/// Built-in 3-pack used when the user does not load a custom template.
pub fn builtin_template() -> Result<Il2Entity, String> {
    parse_group_file(include_str!(
        "../TemplateExamples/Eastern_Fighters_Random_3pack_V6.Group"
    ))
}

/// Summarize a loaded fighter-pack template (Group 1 + NodeGates).
#[allow(dead_code)]
pub fn inspect_pack(root: &Il2Entity) -> Result<PackInfo, String> {
    let group1 = require_group(root, 1)?;
    let gates = root
        .children
        .iter()
        .find(|c| c.name() == Some("NodeGates"));
    Ok(PackInfo {
        existing_groups: count_named_groups(root),
        plane_count: group1.count_block_type("Plane"),
        spawn_choices: count_spawn_choices(group1),
        has_node_gates: gates.is_some(),
    })
}

/// Radius of Group 1's `Zone IN` checkzone (metres). Defaults to 16 km.
pub fn zone_in_radius(root: &Il2Entity) -> f64 {
    root.find_by_name("Zone IN")
        .and_then(|z| z.property("Zone"))
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|r| *r > 0.0)
        .unwrap_or(16_000.0)
}

/// World X/Z of a group's `Zone IN`, or the first positioned child.
pub fn group_anchor_xz(group: &Il2Entity) -> (f64, f64) {
    group
        .find_by_name("Zone IN")
        .and_then(|z| z.pos_xz())
        .or_else(|| group.first_xz())
        .unwrap_or((0.0, 0.0))
}

/// Build an N-pack from **Group 1** of `root`, rewiring NodeGates so the
/// copies stay linked the same way the 3-pack and 5-pack are.
pub fn generate_pack(root: &Il2Entity, group_count: usize) -> Result<Il2Entity, String> {
    let group_count = group_count.max(1);
    generate_pack_ex(
        root,
        group_count,
        None,
        &format!("Eastern Fighters {group_count}pack - Linked"),
    )
}

/// Same as [`generate_pack`], with each group's Zone IN parked on `positions`.
pub fn generate_pack_at(
    root: &Il2Entity,
    positions: &[(f64, f64)],
    name: &str,
) -> Result<Il2Entity, String> {
    if positions.is_empty() {
        return Err("need at least one fighter position".into());
    }
    generate_pack_ex(root, positions.len(), Some(positions), name)
}

/// Move each `RTB - N` waypoint so it sits on `targets[n-1]`.
pub fn park_rtbs(pack: &mut Il2Entity, targets: &[(f64, f64)]) {
    for (i, &target) in targets.iter().enumerate() {
        let name = format!("RTB - {}", i + 1);
        if let Some(wp) = pack.children.iter_mut().find(|c| {
            c.block_type == "MCU_Waypoint" && c.name() == Some(name.as_str())
        }) {
            let from = wp.pos_xz().unwrap_or((0.0, 0.0));
            placement::move_anchor_to(wp, from, target);
        }
    }
}

fn generate_pack_ex(
    root: &Il2Entity,
    group_count: usize,
    positions: Option<&[(f64, f64)]>,
    name: &str,
) -> Result<Il2Entity, String> {
    let group_count = group_count.max(1);
    let group1 = require_group(root, 1)?.clone();
    let waypoint1 = require_rtb(root, 1)?.clone();
    let node_gates = root
        .children
        .iter()
        .find(|c| c.name() == Some("NodeGates"))
        .ok_or("template has no NodeGates group — load a linked fighter pack")?
        .clone();
    let cell1 = extract_gate_cell(&node_gates, 1)?;

    let mut next_id = root.max_index().saturating_add(1);

    let mut waypoints = Vec::with_capacity(group_count);
    let mut groups = Vec::with_capacity(group_count);
    waypoints.push(waypoint1);
    groups.push(group1);

    for i in 2..=group_count {
        let (mut wp, mut grp) = clone_flight_unit(&waypoints[0], &groups[0], &mut next_id);
        wp.set_name(&format!("RTB - {i}"));
        grp.set_name(&format!("Group {i}"));
        waypoints.push(wp);
        groups.push(grp);
    }

    let mut cells = Vec::with_capacity(group_count);
    cells.push(cell1);
    for i in 2..=group_count {
        let mut cell = clone_gate_cell(&cells[0], &mut next_id);
        rename_gate_cell(&mut cell, i);
        cells.push(cell);
    }

    for i in 0..group_count {
        let (dx, dz) = if let Some(pts) = positions {
            let anchor = group_anchor_xz(&groups[i]);
            placement::move_anchor_to(&mut groups[i], anchor, pts[i])
        } else {
            placement::move_to_grid(&mut groups[i], i, group_count)
        };
        waypoints[i].translate_xz(dx, dz);
        translate_gate_cell(&mut cells[i], dx, dz);
    }

    for i in 0..group_count {
        wire_group_to_cell(&mut groups[i], &cells[0], &cells[i]);
        wire_cell_into_group(&mut cells[i], &groups[i]);
    }
    wire_fanouts(&mut cells);

    let mut out = Il2Entity::new("Group");
    out.index = root.index;
    out.properties = root.properties.clone();
    out.set_name(name);
    if let Some(id) = out.index {
        out.set_property("Index", id.to_string());
    }

    for i in 0..group_count {
        out.children.push(waypoints[i].clone());
        out.children.push(groups[i].clone());
    }

    let mut gates_out = Il2Entity::new("Group");
    gates_out.index = node_gates.index;
    gates_out.properties = node_gates.properties.clone();
    gates_out.set_name("NodeGates");
    if let Some(id) = gates_out.index {
        gates_out.set_property("Index", id.to_string());
    }
    for cell in cells {
        gates_out.children.extend(cell.into_timers());
    }
    out.children.push(gates_out);
    Ok(out)
}

fn require_group(root: &Il2Entity, n: usize) -> Result<&Il2Entity, String> {
    let expected = format!("Group {n}");
    root.children
        .iter()
        .find(|c| c.block_type == "Group" && c.name() == Some(expected.as_str()))
        .ok_or_else(|| format!("template has no Group {n}"))
}

fn require_rtb(root: &Il2Entity, n: usize) -> Result<&Il2Entity, String> {
    let expected = format!("RTB - {n}");
    root.children
        .iter()
        .find(|c| c.block_type == "MCU_Waypoint" && c.name() == Some(expected.as_str()))
        .ok_or_else(|| format!("template has no RTB - {n} waypoint"))
}

#[allow(dead_code)]
fn count_named_groups(root: &Il2Entity) -> usize {
    root.children
        .iter()
        .filter(|c| {
            c.block_type == "Group"
                && c.name()
                    .is_some_and(|n| n.starts_with("Group ") && n[6..].parse::<usize>().is_ok())
        })
        .count()
}

#[allow(dead_code)]
fn count_spawn_choices(group: &Il2Entity) -> usize {
    fn walk(entity: &Il2Entity) -> usize {
        let here = usize::from(
            entity.block_type == "MCU_Spawner"
                && entity.name().is_some_and(|n| n.starts_with("Spawn ")),
        );
        here + entity.children.iter().map(walk).sum::<usize>()
    }
    walk(group)
}

fn clone_flight_unit(
    waypoint: &Il2Entity,
    group: &Il2Entity,
    next_id: &mut i32,
) -> (Il2Entity, Il2Entity) {
    let mut wrapper = Il2Entity::new("Group");
    wrapper.children.push(waypoint.clone());
    wrapper.children.push(group.clone());
    let (mut cloned, _) = duplicate_template(&wrapper, next_id);
    let group = cloned.children.pop().expect("cloned group");
    let waypoint = cloned.children.pop().expect("cloned waypoint");
    (waypoint, group)
}

fn extract_gate_cell(node_gates: &Il2Entity, n: usize) -> Result<GateCell, String> {
    let in_disable = named_timer(node_gates, &format!("{n}IN - DISABLE"))?;
    let out_disable = named_timer(node_gates, &format!("{n}OUT - DISABLE"))?;
    let in_enable = named_timer(node_gates, &format!("{n}IN - ENABLE"))?;
    let out_enable = named_timer(node_gates, &format!("{n}OUT - ENABLE"))?;

    let fanout_enable_id = *out_enable
        .targets
        .first()
        .ok_or_else(|| format!("{n}OUT - ENABLE has no fanout target"))?;
    let fanout_disable_id = *out_disable
        .targets
        .first()
        .ok_or_else(|| format!("{n}OUT - DISABLE has no fanout target"))?;

    let fanout_enable = node_gates
        .children
        .iter()
        .find(|c| c.index == Some(fanout_enable_id))
        .cloned()
        .ok_or_else(|| format!("missing fanout-enable timer {fanout_enable_id}"))?;
    let fanout_disable = node_gates
        .children
        .iter()
        .find(|c| c.index == Some(fanout_disable_id))
        .cloned()
        .ok_or_else(|| format!("missing fanout-disable timer {fanout_disable_id}"))?;

    Ok(GateCell {
        in_disable,
        out_disable,
        in_enable,
        out_enable,
        fanout_enable,
        fanout_disable,
    })
}

fn named_timer(node_gates: &Il2Entity, name: &str) -> Result<Il2Entity, String> {
    node_gates
        .children
        .iter()
        .find(|c| c.name() == Some(name))
        .cloned()
        .ok_or_else(|| format!("NodeGates is missing `{name}`"))
}

fn clone_gate_cell(cell: &GateCell, next_id: &mut i32) -> GateCell {
    let mut wrapper = Il2Entity::new("Group");
    wrapper.children = vec![
        cell.in_disable.clone(),
        cell.out_disable.clone(),
        cell.in_enable.clone(),
        cell.out_enable.clone(),
        cell.fanout_enable.clone(),
        cell.fanout_disable.clone(),
    ];
    let (mut cloned, _) = duplicate_template(&wrapper, next_id);
    let mut kids = cloned.children.drain(..);
    GateCell {
        in_disable: kids.next().unwrap(),
        out_disable: kids.next().unwrap(),
        in_enable: kids.next().unwrap(),
        out_enable: kids.next().unwrap(),
        fanout_enable: kids.next().unwrap(),
        fanout_disable: kids.next().unwrap(),
    }
}

fn rename_gate_cell(cell: &mut GateCell, n: usize) {
    cell.in_disable.set_name(&format!("{n}IN - DISABLE"));
    cell.out_disable.set_name(&format!("{n}OUT - DISABLE"));
    cell.in_enable.set_name(&format!("{n}IN - ENABLE"));
    cell.out_enable.set_name(&format!("{n}OUT - ENABLE"));
    cell.fanout_enable.set_name(&format!("{n}"));
    cell.fanout_disable.set_name(&format!("{n}"));
}

fn translate_gate_cell(cell: &mut GateCell, dx: f64, dz: f64) {
    cell.in_disable.translate_xz(dx, dz);
    cell.out_disable.translate_xz(dx, dz);
    cell.in_enable.translate_xz(dx, dz);
    cell.out_enable.translate_xz(dx, dz);
    cell.fanout_enable.translate_xz(dx, dz);
    cell.fanout_disable.translate_xz(dx, dz);
}

fn wire_group_to_cell(group: &mut Il2Entity, cell1: &GateCell, cell: &GateCell) {
    let old_out_enable = cell1.out_enable.index;
    let old_out_disable = cell1.out_disable.index;
    let new_out_enable = cell.out_enable.index;
    let new_out_disable = cell.out_disable.index;
    if let (Some(old), Some(new)) = (old_out_enable, new_out_enable) {
        group.replace_target_id(old, new);
    }
    if let (Some(old), Some(new)) = (old_out_disable, new_out_disable) {
        group.replace_target_id(old, new);
    }
}

fn wire_cell_into_group(cell: &mut GateCell, group: &Il2Entity) {
    let enable_spawner = require_index(group, "Enable Spawner");
    let pulse_in = require_index(group, "ENABLE / PULSE IN");
    let delete_orders = require_index(group, "Delete Orders");
    let disable_spawner = require_index(group, "Disable Spawner");

    if let (Some(a), Some(b)) = (enable_spawner, pulse_in) {
        cell.in_enable.set_targets(vec![a, b]);
    }
    if let (Some(a), Some(b)) = (delete_orders, disable_spawner) {
        cell.in_disable.set_targets(vec![a, b]);
    }
    if let Some(id) = cell.fanout_enable.index {
        cell.out_enable.set_targets(vec![id]);
    }
    if let Some(id) = cell.fanout_disable.index {
        cell.out_disable.set_targets(vec![id]);
    }
}

fn require_index(group: &Il2Entity, name: &str) -> Option<i32> {
    group.find_by_name(name).and_then(|e| e.index)
}

fn wire_fanouts(cells: &mut [GateCell]) {
    let enable_ids: Vec<i32> = cells.iter().filter_map(|c| c.in_enable.index).collect();
    let disable_ids: Vec<i32> = cells.iter().filter_map(|c| c.in_disable.index).collect();
    for (i, cell) in cells.iter_mut().enumerate() {
        let others_enable: Vec<i32> = enable_ids
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, id)| *id)
            .collect();
        let others_disable: Vec<i32> = disable_ids
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, id)| *id)
            .collect();
        cell.fanout_enable.set_targets(others_enable);
        cell.fanout_disable.set_targets(others_disable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;
    use std::collections::HashSet;

    fn pack3() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/Eastern_Fighters_Random_3pack_V6.Group"
        ))
        .expect("parse 3pack")
    }

    fn pack5() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/Eastern_Fighters_Random_5pack_V6.Group"
        ))
        .expect("parse 5pack")
    }

    #[test]
    fn inspect_reads_3pack_and_5pack() {
        let a = inspect_pack(&pack3()).unwrap();
        assert_eq!(a.existing_groups, 3);
        assert_eq!(a.plane_count, 16);
        assert_eq!(a.spawn_choices, 4);
        assert!(a.has_node_gates);

        let b = inspect_pack(&pack5()).unwrap();
        assert_eq!(b.existing_groups, 5);
        assert_eq!(b.plane_count, 16);
        assert_eq!(b.spawn_choices, 4);
    }

    #[test]
    fn generate_10pack_from_3pack() {
        let out = generate_pack(&pack3(), 10).expect("generate 10pack");
        assert_eq!(out.name(), Some("Eastern Fighters 10pack - Linked"));
        assert_eq!(count_named_groups(&out), 10);
        assert_eq!(
            out.children
                .iter()
                .filter(|c| c.block_type == "MCU_Waypoint")
                .count(),
            10
        );
        let gates = out.find_by_name("NodeGates").expect("NodeGates");
        assert!(gates.find_by_name("10IN - ENABLE").is_some());
        assert!(gates.find_by_name("10OUT - DISABLE").is_some());
        assert_eq!(gates.children.len(), 60); // 10 groups × 6 timers
    }

    #[test]
    fn generate_2pack_shrinks() {
        let out = generate_pack(&pack5(), 2).expect("generate 2pack");
        assert_eq!(count_named_groups(&out), 2);
        assert!(out.find_by_name("Group 3").is_none());
        let gates = out.find_by_name("NodeGates").unwrap();
        assert!(gates.find_by_name("3IN - ENABLE").is_none());
        assert_eq!(gates.children.len(), 12);
    }

    #[test]
    fn indexes_are_unique() {
        let out = generate_pack(&pack3(), 4).unwrap();
        let mut ids = Vec::new();
        out.collect_indexes(&mut ids);
        let set: HashSet<i32> = ids.iter().copied().collect();
        assert_eq!(ids.len(), set.len(), "duplicate Indexes: {ids:?}");
    }

    #[test]
    fn nodegates_mutex_skips_self() {
        let out = generate_pack(&pack3(), 3).unwrap();
        let gates = out.find_by_name("NodeGates").unwrap();
        let cell = extract_gate_cell(gates, 1).unwrap();
        let two_in = gates.find_by_name("2IN - ENABLE").unwrap().index.unwrap();
        let three_in = gates.find_by_name("3IN - ENABLE").unwrap().index.unwrap();
        let one_in = gates.find_by_name("1IN - ENABLE").unwrap().index.unwrap();
        assert!(cell.fanout_enable.targets.contains(&two_in));
        assert!(cell.fanout_enable.targets.contains(&three_in));
        assert!(!cell.fanout_enable.targets.contains(&one_in));
    }

    #[test]
    fn cloned_group_zone_in_points_at_its_out_disable() {
        let out = generate_pack(&pack3(), 3).unwrap();
        let group2 = out.find_by_name("Group 2").unwrap();
        let zone_in = group2.find_by_name("Zone IN").unwrap();
        let gates = out.find_by_name("NodeGates").unwrap();
        let out_disable = gates.find_by_name("2OUT - DISABLE").unwrap().index.unwrap();
        let out_enable = gates.find_by_name("2OUT - ENABLE").unwrap().index.unwrap();
        assert!(
            zone_in.targets.contains(&out_disable),
            "Zone IN targets {:?}, expected {out_disable}",
            zone_in.targets
        );
        let zone_out = group2.find_by_name("Zone OUT").unwrap();
        assert!(zone_out.targets.contains(&out_enable));
    }

    #[test]
    fn cloned_group_keeps_aircraft_and_pairing_mcus() {
        let out = generate_pack(&pack3(), 4).unwrap();
        let group4 = out.find_by_name("Group 4").unwrap();
        assert_eq!(group4.count_block_type("Plane"), 16);
        assert_eq!(count_spawn_choices(group4), 4);
        assert!(group4.find_by_name("command AttackArea").is_some());
        assert!(group4.find_by_name("Cover Lead").is_some());
        assert!(group4.find_by_name("Cover Wing 2").is_some());
        assert!(group4.find_by_name("Cover Wing 3").is_some());
    }

    #[test]
    fn group1_keeps_original_index() {
        let src = pack3();
        let original = require_group(&src, 1).unwrap().index;
        let out = generate_pack(&src, 6).unwrap();
        assert_eq!(require_group(&out, 1).unwrap().index, original);
        assert_ne!(require_group(&out, 2).unwrap().index, original);
    }

    #[test]
    fn round_trip_generated_pack() {
        let out = generate_pack(&pack3(), 4).unwrap();
        let text = serialize_group(&out);
        let reparsed = parse_group_file(&text).expect("reparse generated pack");
        assert_eq!(count_named_groups(&reparsed), 4);
        assert_eq!(inspect_pack(&reparsed).unwrap().spawn_choices, 4);
    }

    #[test]
    fn in_enable_targets_pulse_in_of_same_group() {
        let out = generate_pack(&pack3(), 3).unwrap();
        let group2 = out.find_by_name("Group 2").unwrap();
        let pulse = group2
            .find_by_name("ENABLE / PULSE IN")
            .unwrap()
            .index
            .unwrap();
        let spawner = group2
            .find_by_name("Enable Spawner")
            .unwrap()
            .index
            .unwrap();
        let gates = out.find_by_name("NodeGates").unwrap();
        let in_enable = gates.find_by_name("2IN - ENABLE").unwrap();
        assert_eq!(in_enable.targets, vec![spawner, pulse]);
    }

    #[test]
    fn groups_park_on_map_grid() {
        let out = generate_pack(&pack3(), 3).unwrap();
        let g1 = out.find_by_name("Group 1").unwrap();
        let g2 = out.find_by_name("Group 2").unwrap();
        let (x1, z1) = g1.first_xz().unwrap();
        let (x2, z2) = g2.first_xz().unwrap();
        assert!((x1 - crate::placement::MAP_MIN).abs() < 1.0);
        assert!((z1 - crate::placement::MAP_MIN).abs() < 1.0);
        assert!((x2 - x1 - crate::placement::GRID_STEP).abs() < 1.0);
        assert!((z2 - z1).abs() < 1.0);
    }

    #[test]
    fn builtin_zone_in_is_16km() {
        assert!((zone_in_radius(&pack3()) - 16_000.0).abs() < 0.1);
    }

    #[test]
    fn generate_pack_at_parks_zone_in() {
        let pts = [(120_000.0, 210_000.0), (140_000.0, 230_000.0)];
        let out = generate_pack_at(&pack3(), &pts, "NATO Fighters Wave 1 pack 1").unwrap();
        assert_eq!(out.name(), Some("NATO Fighters Wave 1 pack 1"));
        let a = group_anchor_xz(out.find_by_name("Group 1").unwrap());
        let b = group_anchor_xz(out.find_by_name("Group 2").unwrap());
        assert!((a.0 - pts[0].0).abs() < 0.5 && (a.1 - pts[0].1).abs() < 0.5);
        assert!((b.0 - pts[1].0).abs() < 0.5 && (b.1 - pts[1].1).abs() < 0.5);
        let dist = ((b.0 - a.0).hypot(b.1 - a.1) - 20_000.0 * 2.0_f64.sqrt()).abs();
        assert!(dist < 1.0);
    }

    #[test]
    fn park_rtbs_moves_waypoints_to_targets() {
        let pts = [(120_000.0, 210_000.0), (140_000.0, 230_000.0)];
        let mut out = generate_pack_at(&pack3(), &pts, "NATO Fighters Wave 1 pack 1").unwrap();
        let rtb = [(80_000.0, 210_000.0), (80_000.0, 230_000.0)];
        park_rtbs(&mut out, &rtb);
        let a = out.find_by_name("RTB - 1").unwrap().pos_xz().unwrap();
        let b = out.find_by_name("RTB - 2").unwrap().pos_xz().unwrap();
        assert!((a.0 - rtb[0].0).abs() < 0.5 && (a.1 - rtb[0].1).abs() < 0.5);
        assert!((b.0 - rtb[1].0).abs() < 0.5 && (b.1 - rtb[1].1).abs() < 0.5);
        let zone = group_anchor_xz(out.find_by_name("Group 1").unwrap());
        assert!((zone.0 - pts[0].0).abs() < 0.5);
    }
}
