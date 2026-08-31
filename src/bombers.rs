//! Exclusive activation: link templates so only one plan can trigger at a time.
//!
//! Each loaded `.Group` is cloned as its own plan. Trigger checkzones
//! (`Zone IN`, `MISSION START`, or every checkzone except end/out) close
//! the other plans through NodeGates. When the end timer fires — `END`,
//! `MISSION END`, or a timer that pulses `MCU_Delete` / `MCU_Deactivate` on
//! the template's units — the remaining plans' zones reopen. A good example
//! is preplanned bomber flights. Multiple `Zone IN` MCUs in one template
//! (B-29 corridor) count as a single plan.

use std::collections::{HashMap, HashSet};

use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::placement;

/// Names the linker auto-selects. Use these in new templates to skip the picker.
pub const SUGGESTED_TRIGGER_NAMES: &str = "Zone IN or MISSION START";
pub const SUGGESTED_END_NAMES: &str = "END or MISSION END";

#[derive(Debug, Clone)]
pub struct McuChoice {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct BomberPlanInfo {
    pub name: String,
    pub plane_count: usize,
    pub unit_count: usize,
    pub checkzones: Vec<McuChoice>,
    pub timers: Vec<McuChoice>,
    pub suggested_triggers: Vec<i32>,
    pub suggested_completion: Option<i32>,
    /// End-timer Index → warning when a linked Deactivate/Delete misses units.
    pub cleanup_warnings: HashMap<i32, String>,
    /// Checkzone Index → warning when Closer, coalitions, or unit-activate path is wrong.
    pub trigger_warnings: HashMap<i32, String>,
}

#[derive(Debug, Clone)]
pub struct BomberInput {
    pub label: String,
    #[allow(dead_code)]
    pub source_key: String,
    pub root: Il2Entity,
    /// Source-file Indexes of checkzones that open/close this plan.
    pub trigger_zone_ids: Vec<i32>,
    /// Source-file Index of the timer that fires when bombers are gone.
    pub completion_timer_id: i32,
}

struct GateCell {
    in_enable: Il2Entity,
    in_disable: Il2Entity,
    out_enable: Il2Entity,
    out_disable: Il2Entity,
    fanout_enable: Il2Entity,
    fanout_disable: Il2Entity,
    open_zones: Il2Entity,
    close_zones: Il2Entity,
}

impl GateCell {
    fn into_children(self) -> Vec<Il2Entity> {
        vec![
            self.in_disable,
            self.out_disable,
            self.in_enable,
            self.out_enable,
            self.fanout_enable,
            self.fanout_disable,
            self.open_zones,
            self.close_zones,
        ]
    }
}

/// List checkzones and timers, with name-based suggestions for the linker.
pub fn inspect_plan(root: &Il2Entity) -> Result<BomberPlanInfo, String> {
    let checkzones = collect_choices(root, "MCU_CheckZone");
    if checkzones.is_empty() {
        return Err("template has no MCU_CheckZone".into());
    }
    let timers = collect_choices(root, "MCU_Timer");
    if timers.is_empty() {
        return Err("template has no MCU_Timer (need one as the end trigger)".into());
    }
    Ok(BomberPlanInfo {
        name: root.name().unwrap_or("Plan").to_string(),
        plane_count: root.count_block_type("Plane"),
        unit_count: root.count_block_type("Plane")
            + root.count_block_type("Vehicle")
            + root.count_block_type("Train")
            + root.count_block_type("Ship"),
        suggested_triggers: suggested_trigger_ids(root),
        suggested_completion: suggested_completion_id(root),
        cleanup_warnings: cleanup_warnings_for_timers(root, &timers),
        trigger_warnings: trigger_warnings_for_zones(root, &checkzones),
        checkzones,
        timers,
    })
}

fn is_node_gates(entity: &Il2Entity) -> bool {
    entity.block_type == "Group" && entity.name() == Some("NodeGates")
}

fn looks_like_exclusive_here(root: &Il2Entity) -> bool {
    if root.name() == Some("Exclusive Activation") {
        return true;
    }
    let has_gates = root.children.iter().any(is_node_gates);
    let plans = root
        .children
        .iter()
        .filter(|c| c.block_type == "Group" && !is_node_gates(c))
        .count();
    has_gates && plans >= 1
}

fn exclusive_root(root: &Il2Entity) -> &Il2Entity {
    if looks_like_exclusive_here(root) {
        return root;
    }
    if let Some(child) = root.children.iter().find(|c| looks_like_exclusive_here(c)) {
        return child;
    }
    if root.block_type == "Group" && root.children.len() == 1 {
        return exclusive_root(&root.children[0]);
    }
    root
}

/// True for a generated Exclusive Activation pack (including an editor Group wrapper).
pub fn looks_like_exclusive_pack(root: &Il2Entity) -> bool {
    looks_like_exclusive_here(exclusive_root(root))
}

/// Plan groups from a generated pack, with NodeGates links stripped so they can
/// be re-linked. Trailing ` [n]` / `*n*` copy suffixes are removed from names.
pub fn extract_exclusive_plans(root: &Il2Entity) -> Result<Vec<Il2Entity>, String> {
    let pack = exclusive_root(root);
    if !looks_like_exclusive_here(pack) {
        return Err(
            "not an Exclusive Activation pack — export the generated group from the editor".into(),
        );
    }
    let mut gate_ids_vec = Vec::new();
    if let Some(gates) = pack.children.iter().find(|c| is_node_gates(c)) {
        gates.collect_indexes(&mut gate_ids_vec);
    }
    let gate_ids: HashSet<i32> = gate_ids_vec.into_iter().collect();
    let mut plans = Vec::new();
    for child in &pack.children {
        if is_node_gates(child) || child.block_type != "Group" {
            continue;
        }
        let mut plan = child.clone();
        strip_gate_links(&mut plan, &gate_ids);
        if let Some(cleaned) = plan.name().map(|n| strip_copy_suffix(n).to_string()) {
            if plan.name() != Some(cleaned.as_str()) {
                plan.set_name(&cleaned);
            }
        }
        plans.push(plan);
    }
    if plans.is_empty() {
        return Err("that Exclusive Activation file has no plan groups".into());
    }
    Ok(plans)
}

fn strip_gate_links(entity: &mut Il2Entity, gate_ids: &HashSet<i32>) {
    entity.for_each_mut(&mut |e| {
        if e.targets.iter().any(|id| gate_ids.contains(id)) {
            let keep: Vec<i32> = e
                .targets
                .iter()
                .copied()
                .filter(|id| !gate_ids.contains(id))
                .collect();
            e.set_targets(keep);
        }
        if e.objects.iter().any(|id| gate_ids.contains(id)) {
            let keep: Vec<i32> = e
                .objects
                .iter()
                .copied()
                .filter(|id| !gate_ids.contains(id))
                .collect();
            e.set_objects(keep);
        }
    });
}

fn strip_copy_suffix(name: &str) -> &str {
    let name = name.trim_end();
    if let Some(star) = name.rfind('*') {
        if name.ends_with('*') && star > 0 {
            let inner = &name[..name.len() - 1];
            if let Some(open) = inner.rfind('*') {
                let digits = &inner[open + 1..];
                if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                    return inner[..open].trim_end();
                }
            }
        }
    }
    if let Some(open) = name.rfind('[') {
        if name.ends_with(']') && open > 0 {
            let digits = &name[open + 1..name.len() - 1];
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return name[..open].trim_end();
            }
        }
    }
    name
}

/// Clone `plans` into one file with a NodeGates mutex: one plan live at a time.
pub fn link_bomber_plans(plans: &[BomberInput]) -> Result<Il2Entity, String> {
    link_bomber_plans_with(plans, false)
}

/// Same as [`link_bomber_plans`]. When `keep_positions` is true, copies stay
/// where they already sit (export in place) instead of parking on the 10 km grid.
pub fn link_bomber_plans_with(
    plans: &[BomberInput],
    keep_positions: bool,
) -> Result<Il2Entity, String> {
    if plans.is_empty() {
        return Err("add at least one template".into());
    }
    for (i, plan) in plans.iter().enumerate() {
        if plan.trigger_zone_ids.is_empty() {
            return Err(format!("plan {} needs at least one trigger checkzone", i + 1));
        }
        if find_index(&plan.root, plan.completion_timer_id).is_none() {
            return Err(format!("plan {} end trigger timer was not found in the template", i + 1));
        }
        for id in &plan.trigger_zone_ids {
            if find_index(&plan.root, *id).is_none() {
                return Err(format!("plan {} checkzone {id} was not found in the template", i + 1));
            }
        }
    }

    let mut next_id = plans
        .iter()
        .map(|p| p.root.max_index())
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    let proto_timer = find_block(&plans[0].root, "MCU_Timer")
        .cloned()
        .ok_or("template has no MCU_Timer to clone for NodeGates")?;
    let proto_activate = find_block(&plans[0].root, "MCU_Activate")
        .cloned()
        .or_else(|| find_block_any(plans, "MCU_Activate").cloned());
    let proto_deactivate = find_block(&plans[0].root, "MCU_Deactivate")
        .cloned()
        .or_else(|| find_block_any(plans, "MCU_Deactivate").cloned());

    let mut copies: Vec<Il2Entity> = Vec::with_capacity(plans.len());
    let mut mapped_triggers: Vec<Vec<i32>> = Vec::with_capacity(plans.len());
    let mut mapped_completion: Vec<i32> = Vec::with_capacity(plans.len());
    for (i, plan) in plans.iter().enumerate() {
        let (mut clone, id_map) = duplicate_template(&plan.root, &mut next_id);
        if !keep_positions {
            placement::move_to_grid(&mut clone, i, plans.len());
        }
        let base = plan.root.name().unwrap_or(plan.label.as_str());
        clone.set_name(&format!("{} [{}]", base, i + 1));
        let triggers: Vec<i32> = plan
            .trigger_zone_ids
            .iter()
            .filter_map(|id| id_map.get(id).copied())
            .collect();
        let completion = *id_map
            .get(&plan.completion_timer_id)
            .ok_or_else(|| format!("plan {} end trigger lost during clone", i + 1))?;
        mapped_triggers.push(triggers);
        mapped_completion.push(completion);
        copies.push(clone);
    }

    let mut cells = Vec::with_capacity(copies.len());
    for (i, plan) in copies.iter().enumerate() {
        let n = i + 1;
        let triggers = &mapped_triggers[i];
        let (gx, gy, gz) = pos_of(plan, triggers[0]).unwrap_or((0.0, 0.0, 0.0));
        let mut cell = build_gate_cell(
            &proto_timer,
            proto_activate.as_ref(),
            proto_deactivate.as_ref(),
            n,
            triggers,
            gx,
            gy,
            gz,
            &mut next_id,
        )?;
        let open_id = cell.open_zones.index.unwrap();
        let close_id = cell.close_zones.index.unwrap();
        cell.in_enable.set_targets(vec![open_id]);
        cell.in_disable.set_targets(vec![close_id]);
        if let Some(id) = cell.fanout_enable.index {
            cell.out_enable.set_targets(vec![id]);
        }
        if let Some(id) = cell.fanout_disable.index {
            cell.out_disable.set_targets(vec![id]);
        }
        cells.push(cell);
    }

    for (i, (plan, cell)) in copies.iter_mut().zip(cells.iter()).enumerate() {
        let out_disable = cell.out_disable.index.unwrap();
        let out_enable = cell.out_enable.index.unwrap();
        let triggers = &mapped_triggers[i];
        let completion = mapped_completion[i];
        plan.for_each_mut(&mut |e| {
            if e.index.is_some_and(|id| triggers.contains(&id)) {
                e.append_target(out_disable);
            }
            if e.index == Some(completion) {
                e.append_target(out_enable);
            }
        });
    }

    wire_fanouts(&mut cells);

    let mut out = Il2Entity::new("Group");
    out.index = Some(next_id);
    out.set_property("Index", next_id.to_string());
    next_id += 1;
    out.set_name("Exclusive Activation");
    out.set_property("Desc", "\"\"");
    out.children = copies;

    let mut gates = Il2Entity::new("Group");
    gates.index = Some(next_id);
    gates.set_property("Index", next_id.to_string());
    gates.set_name("NodeGates");
    gates.set_property("Desc", "\"\"");
    for cell in cells {
        gates.children.extend(cell.into_children());
    }
    out.children.push(gates);
    Ok(out)
}

fn build_gate_cell(
    proto_timer: &Il2Entity,
    proto_activate: Option<&Il2Entity>,
    proto_deactivate: Option<&Il2Entity>,
    n: usize,
    trigger_ids: &[i32],
    x: f64,
    y: f64,
    z: f64,
    next_id: &mut i32,
) -> Result<GateCell, String> {
    let dx = (n as f64 - 1.0) * 80.0;
    let mut in_enable = clone_named(proto_timer, next_id, &format!("{n}IN - ENABLE"));
    let mut in_disable = clone_named(proto_timer, next_id, &format!("{n}IN - DISABLE"));
    let mut out_enable = clone_named(proto_timer, next_id, &format!("{n}OUT - ENABLE"));
    let mut out_disable = clone_named(proto_timer, next_id, &format!("{n}OUT - DISABLE"));
    let mut fanout_enable = clone_named(proto_timer, next_id, &format!("{n}"));
    let mut fanout_disable = clone_named(proto_timer, next_id, &format!("{n}"));
    for timer in [
        &mut in_enable,
        &mut in_disable,
        &mut out_enable,
        &mut out_disable,
        &mut fanout_enable,
        &mut fanout_disable,
    ] {
        timer.set_property("Time", "0");
        timer.set_property("Random", "100");
        timer.set_targets(vec![]);
        timer.set_objects(vec![]);
        set_pos(timer, x + dx, y, z);
    }

    let mut open_zones = match proto_activate {
        Some(proto) => clone_named(proto, next_id, &format!("{n} Open Zones")),
        None => synthesize_mcu("MCU_Activate", next_id, &format!("{n} Open Zones")),
    };
    let mut close_zones = match proto_deactivate {
        Some(proto) => clone_named(proto, next_id, &format!("{n} Close Zones")),
        None => synthesize_mcu("MCU_Deactivate", next_id, &format!("{n} Close Zones")),
    };
    open_zones.set_targets(trigger_ids.to_vec());
    open_zones.set_objects(vec![]);
    close_zones.set_targets(trigger_ids.to_vec());
    close_zones.set_objects(vec![]);
    set_pos(&mut open_zones, x + dx, y, z + 40.0);
    set_pos(&mut close_zones, x + dx, y, z + 80.0);

    Ok(GateCell {
        in_enable,
        in_disable,
        out_enable,
        out_disable,
        fanout_enable,
        fanout_disable,
        open_zones,
        close_zones,
    })
}

fn wire_fanouts(cells: &mut [GateCell]) {
    let enable_ids: Vec<i32> = cells.iter().filter_map(|c| c.in_enable.index).collect();
    let disable_ids: Vec<i32> = cells.iter().filter_map(|c| c.in_disable.index).collect();
    for (i, cell) in cells.iter_mut().enumerate() {
        cell.fanout_enable.set_targets(
            enable_ids
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, id)| *id)
                .collect(),
        );
        cell.fanout_disable.set_targets(
            disable_ids
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, id)| *id)
                .collect(),
        );
    }
}

fn collect_choices(root: &Il2Entity, block_type: &str) -> Vec<McuChoice> {
    let mut out = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == block_type {
            if let Some(index) = e.index {
                out.push(McuChoice {
                    index,
                    name: e.name().unwrap_or("(unnamed)").to_string(),
                });
            }
        }
    });
    out
}

fn suggested_trigger_ids(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_CheckZone" && is_start_zone_name(e.name().unwrap_or("")) {
            if let Some(id) = e.index {
                ids.push(id);
            }
        }
    });
    ids
}

fn suggested_completion_id(root: &Il2Entity) -> Option<i32> {
    let cleanup_ids = cleanup_mcu_indexes(root);
    let mut preferred = None;
    let mut legacy = None;
    let mut structural = None;
    root.for_each(&mut |e| {
        if e.block_type != "MCU_Timer" {
            return;
        }
        let Some(id) = e.index else {
            return;
        };
        let name = e.name().unwrap_or("");
        if is_preferred_end_name(name) {
            preferred.get_or_insert(id);
        } else if is_legacy_end_name(name) {
            legacy.get_or_insert(id);
        } else if e.targets.iter().any(|t| cleanup_ids.contains(t)) {
            structural.get_or_insert(id);
        }
    });
    preferred.or(legacy).or(structural)
}

fn is_start_zone_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("zone in") || name.eq_ignore_ascii_case("mission start")
}

fn trigger_warnings_for_zones(
    root: &Il2Entity,
    zones: &[McuChoice],
) -> HashMap<i32, String> {
    let mut out = HashMap::new();
    for zone in zones {
        if let Some(msg) = trigger_zone_warning(root, zone.index) {
            out.insert(zone.index, msg);
        }
    }
    out
}

/// Closer = 1, at least one coalition, and a Targets chain that reaches
/// MCU_Activate / MCU_Spawner listing at least one Plane / Vehicle / Ship.
pub fn trigger_zone_warning(root: &Il2Entity, zone_id: i32) -> Option<String> {
    let zone = find_index(root, zone_id)?;
    if zone.block_type != "MCU_CheckZone" {
        return None;
    }
    let label = zone.name().unwrap_or("CheckZone");
    let mut parts = Vec::new();
    let closer = zone.property("Closer").unwrap_or("");
    if closer != "1" {
        parts.push(format!(
            "Distance Type is not Closer (Closer = {}).",
            if closer.is_empty() { "unset" } else { closer }
        ));
    }
    if coalition_ids(zone).is_empty() {
        parts.push("no coalitions are listed.".into());
    }
    let units = collect_world_units(root);
    if !units.is_empty() && !reaches_unit_activation(root, zone, &units) {
        parts.push(
            "does not reach an Activate or Spawner that lists units in this group.".into(),
        );
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("\"{label}\": {}", parts.join(" ")))
    }
}

fn coalition_ids(entity: &Il2Entity) -> Vec<i32> {
    const KEYS: &[&str] = &[
        "PlaneCoalitions",
        "VehicleCoalitions",
        "ShipCoalitions",
        "CountryCoalitions",
    ];
    for key in KEYS {
        let Some(raw) = entity.property(key) else {
            continue;
        };
        let Ok((_, ids)) = crate::parser::parse_integer_array(raw) else {
            continue;
        };
        if !ids.is_empty() {
            return ids;
        }
    }
    Vec::new()
}

fn reaches_unit_activation(root: &Il2Entity, start: &Il2Entity, units: &[WorldUnit]) -> bool {
    let mut seen = HashSet::new();
    let mut queue = start.targets.clone();
    let mut steps = 0usize;
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        steps += 1;
        if steps > 256 {
            break;
        }
        let Some(node) = find_index(root, id) else {
            continue;
        };
        if matches!(node.block_type.as_str(), "MCU_Activate" | "MCU_Spawner") {
            let covered: HashSet<i32> = node.objects.iter().copied().collect();
            if units.iter().any(|u| unit_is_covered(u, &covered)) {
                return true;
            }
        }
        queue.extend(node.targets.iter().copied());
    }
    false
}

fn is_preferred_end_name(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case("END") || n.eq_ignore_ascii_case("MISSION END")
}

fn is_legacy_end_name(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case("Delay Delete")
        || n.eq_ignore_ascii_case("MISSION CLEAN UP")
        || starts_with_ignore_ascii(n, "MISSION CLEAN UP")
}

fn starts_with_ignore_ascii(name: &str, prefix: &str) -> bool {
    name.len() >= prefix.len()
        && name
            .chars()
            .zip(prefix.chars())
            .all(|(a, b)| a.eq_ignore_ascii_case(&b))
}

fn cleanup_warnings_for_timers(
    root: &Il2Entity,
    timers: &[McuChoice],
) -> HashMap<i32, String> {
    let mut out = HashMap::new();
    for timer in timers {
        if let Some(msg) = cleanup_coverage_warning(root, timer.index) {
            out.insert(timer.index, msg);
        }
    }
    out
}

fn collect_cleanup_from_timer<'a>(root: &'a Il2Entity, timer: &'a Il2Entity) -> Vec<&'a Il2Entity> {
    let mut seen = HashSet::new();
    let mut queue = timer.targets.clone();
    let mut out = Vec::new();
    let mut steps = 0usize;
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        steps += 1;
        if steps > 64 {
            break;
        }
        let Some(node) = find_index(root, id) else {
            continue;
        };
        match node.block_type.as_str() {
            "MCU_Deactivate" | "MCU_Delete" => out.push(node),
            "MCU_Timer" => queue.extend(node.targets.iter().copied()),
            _ => {}
        }
    }
    out
}

/// Warn when a timer's linked Deactivate/Delete MCUs do not list every
/// Plane / Vehicle / Train / Ship (by Index or LinkTrId).
pub fn cleanup_coverage_warning(root: &Il2Entity, timer_id: i32) -> Option<String> {
    let timer = find_index(root, timer_id)?;
    if timer.block_type != "MCU_Timer" {
        return None;
    }
    let units = collect_world_units(root);
    if units.is_empty() {
        return None;
    }
    let cleanup_mcus = collect_cleanup_from_timer(root, timer);
    if cleanup_mcus.is_empty() {
        return Some(
            "This timer does not target a Deactivate or Delete MCU that lists the group's units."
                .into(),
        );
    }

    let mut lines = Vec::new();
    for mcu in cleanup_mcus {
        if mcu.objects.is_empty() {
            continue;
        }
        let covered: HashSet<i32> = mcu.objects.iter().copied().collect();
        let missing: Vec<&str> = units
            .iter()
            .filter(|u| !unit_is_covered(u, &covered))
            .map(|u| u.name.as_str())
            .collect();
        if missing.is_empty() {
            continue;
        }
        let kind = if mcu.block_type == "MCU_Delete" {
            "Delete"
        } else {
            "Deactivate"
        };
        let mcu_name = mcu.name().unwrap_or(kind);
        lines.push(format!(
            "{kind} \"{mcu_name}\" does not cover {}: {}.",
            count_label(missing.len(), "unit"),
            join_names(&missing)
        ));
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

fn count_label(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn join_names(names: &[&str]) -> String {
    const MAX: usize = 6;
    if names.len() <= MAX {
        names.join(", ")
    } else {
        format!(
            "{}, and {} more",
            names[..MAX].join(", "),
            names.len() - MAX
        )
    }
}

struct WorldUnit {
    name: String,
    index: i32,
    link_tr: Option<i32>,
}

fn collect_world_units(root: &Il2Entity) -> Vec<WorldUnit> {
    let mut out = Vec::new();
    root.for_each(&mut |e| {
        if !matches!(e.block_type.as_str(), "Vehicle" | "Ship" | "Plane" | "Train") {
            return;
        }
        let Some(index) = e.index else {
            return;
        };
        let link_tr = e.property("LinkTrId").and_then(|v| v.parse().ok());
        out.push(WorldUnit {
            name: e.name().unwrap_or("(unnamed)").to_string(),
            index,
            link_tr,
        });
    });
    out
}

fn unit_is_covered(unit: &WorldUnit, objects: &HashSet<i32>) -> bool {
    objects.contains(&unit.index)
        || unit
            .link_tr
            .is_some_and(|id| objects.contains(&id))
}

fn cleanup_mcu_indexes(root: &Il2Entity) -> Vec<i32> {
    let units = collect_world_units(root);
    let unit_ids: HashSet<i32> = units
        .iter()
        .flat_map(|u| std::iter::once(u.index).chain(u.link_tr))
        .collect();
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        let name = e.name().unwrap_or("");
        let named = name == "Deactive ALL"
            || name == "Deactivate ALL"
            || name == "Trigger Delete";
        let points_at_units = e.objects.iter().any(|id| unit_ids.contains(id));
        let is_cleanup = e.block_type == "MCU_Delete"
            || (e.block_type == "MCU_Deactivate" && (named || points_at_units));
        if is_cleanup {
            if let Some(id) = e.index {
                ids.push(id);
            }
        }
    });
    ids
}

fn find_index<'a>(root: &'a Il2Entity, index: i32) -> Option<&'a Il2Entity> {
    if root.index == Some(index) {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_index(c, index))
}

fn pos_of(root: &Il2Entity, index: i32) -> Option<(f64, f64, f64)> {
    let e = find_index(root, index)?;
    let x = e.property("XPos")?.parse().ok()?;
    let y = e.property("YPos")?.parse().ok()?;
    let z = e.property("ZPos")?.parse().ok()?;
    Some((x, y, z))
}

fn find_block<'a>(root: &'a Il2Entity, block_type: &str) -> Option<&'a Il2Entity> {
    if root.block_type == block_type {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|c| find_block(c, block_type))
}

fn find_block_any<'a>(plans: &'a [BomberInput], block_type: &str) -> Option<&'a Il2Entity> {
    plans.iter().find_map(|p| find_block(&p.root, block_type))
}

fn clone_named(proto: &Il2Entity, next_id: &mut i32, name: &str) -> Il2Entity {
    let (mut cloned, _) = duplicate_template(proto, next_id);
    cloned.set_name(name);
    cloned
}

fn synthesize_mcu(block_type: &str, next_id: &mut i32, name: &str) -> Il2Entity {
    let mut e = Il2Entity::new(block_type);
    e.index = Some(*next_id);
    e.set_property("Index", next_id.to_string());
    *next_id += 1;
    e.set_name(name);
    e.set_property("Desc", "\"\"");
    e.set_targets(vec![]);
    e.set_objects(vec![]);
    e.set_property("XPos", "0");
    e.set_property("YPos", "0");
    e.set_property("ZPos", "0");
    e.set_property("XOri", "0");
    e.set_property("YOri", "0");
    e.set_property("ZOri", "0");
    e
}

fn set_pos(entity: &mut Il2Entity, x: f64, y: f64, z: f64) {
    entity.set_property("XPos", format!("{x:.3}"));
    entity.set_property("YPos", format!("{y:.3}"));
    entity.set_property("ZPos", format!("{z:.3}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;
    use std::collections::HashSet;

    fn dprk() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/BomberMissions/DPRK Bombers - Small Formation.Group"
        ))
        .expect("parse DPRK")
    }

    fn b29() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/BomberMissions/B29Mission.Group"
        ))
        .expect("parse B-29")
    }

    fn input(key: &str, root: Il2Entity) -> BomberInput {
        let info = inspect_plan(&root).expect("inspect");
        BomberInput {
            label: key.into(),
            source_key: key.into(),
            trigger_zone_ids: info.suggested_triggers.clone(),
            completion_timer_id: info.suggested_completion.expect("suggested end timer"),
            root,
        }
    }

    fn all_named<'a>(root: &'a Il2Entity, name: &'a str) -> Vec<&'a Il2Entity> {
        let mut out = Vec::new();
        root.find_all_by_name(name, &mut out);
        out
    }

    #[test]
    fn inspect_dprk_one_start_zone() {
        let info = inspect_plan(&dprk()).unwrap();
        assert_eq!(info.suggested_triggers.len(), 1);
        assert_eq!(info.checkzones.len(), 2);
        let start = info
            .checkzones
            .iter()
            .find(|z| z.name == "MISSION START")
            .unwrap();
        assert_eq!(info.suggested_triggers, vec![start.index]);
        let end = info.timers.iter().find(|t| t.name.starts_with("MISSION CLEAN UP"));
        assert_eq!(info.suggested_completion, end.map(|t| t.index));
        assert_eq!(info.plane_count, 8);
        assert!(
            info.trigger_warnings.get(&start.index).is_none(),
            "MISSION START should pass the Zone IN audit"
        );
    }

    #[test]
    fn inspect_b29_corridor() {
        let info = inspect_plan(&b29()).unwrap();
        assert_eq!(info.suggested_triggers.len(), 9);
        assert_eq!(info.checkzones.len(), 9);
        assert!(info.checkzones.iter().all(|z| z.name == "Zone IN"));
        let end = info.timers.iter().find(|t| t.name == "END").unwrap();
        assert_eq!(info.suggested_completion, Some(end.index));
        assert_eq!(info.plane_count, 17);
        for zone in info.checkzones.iter().filter(|z| z.name == "Zone IN") {
            assert!(
                info.trigger_warnings.get(&zone.index).is_none(),
                "Zone IN {} should pass the trigger audit, got {:?}",
                zone.index,
                info.trigger_warnings.get(&zone.index)
            );
        }
    }

    #[test]
    fn inspect_russian_arty_end_timer_warns_on_partial_deactivate() {
        let root = parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK ML20 Arty.Group"
        ))
        .expect("arty");
        let info = inspect_plan(&root).unwrap();
        let end = info.suggested_completion.expect("2s timer");
        let timer = info.timers.iter().find(|t| t.index == end).unwrap();
        assert_eq!(timer.name, "2s");
        let warn = info.cleanup_warnings.get(&end).expect("partial deactivate");
        assert!(warn.contains("Deactivate"));
        assert!(warn.contains("unit"));
        let zone = info
            .checkzones
            .iter()
            .find(|z| z.name.eq_ignore_ascii_case("zone in"))
            .unwrap();
        assert!(info.suggested_triggers.contains(&zone.index));
        assert!(
            info.trigger_warnings.get(&zone.index).is_none(),
            "Zone In should pass the trigger audit, got {:?}",
            info.trigger_warnings.get(&zone.index)
        );
    }

    #[test]
    fn inspect_trigger_zone_flags_closer_coalitions_and_activate_path() {
        let mut plane = Il2Entity::new("Plane");
        plane.index = Some(10);
        plane.set_property("Index", "10");
        plane.set_property("LinkTrId", "11");
        plane.set_name("Lead");

        let mut bad = Il2Entity::new("MCU_CheckZone");
        bad.index = Some(1);
        bad.set_property("Index", "1");
        bad.set_name("Zone IN");
        bad.set_property("Closer", "0");

        let mut timer = Il2Entity::new("MCU_Timer");
        timer.index = Some(2);
        timer.set_property("Index", "2");
        timer.set_name("END");

        let mut root = Il2Entity::new("Group");
        root.set_name("Bad Zone");
        root.children = vec![bad, plane.clone(), timer.clone()];
        let info = inspect_plan(&root).unwrap();
        let msg = info.trigger_warnings.get(&1).expect("bad zone");
        assert!(msg.contains("Closer"), "{msg}");
        assert!(msg.contains("coalitions"), "{msg}");
        assert!(msg.contains("Activate") || msg.contains("Spawner"), "{msg}");

        let mut good = Il2Entity::new("MCU_CheckZone");
        good.index = Some(1);
        good.set_property("Index", "1");
        good.set_name("Zone IN");
        good.set_property("Closer", "1");
        good.set_property("PlaneCoalitions", "[2]");
        good.set_targets(vec![3]);
        let mut act = Il2Entity::new("MCU_Activate");
        act.index = Some(3);
        act.set_property("Index", "3");
        act.set_name("Activate Units");
        act.set_objects(vec![11]);
        let mut root = Il2Entity::new("Group");
        root.set_name("Good Zone");
        root.children = vec![good, plane, act, timer];
        let info = inspect_plan(&root).unwrap();
        assert!(
            info.trigger_warnings.get(&1).is_none(),
            "got {:?}",
            info.trigger_warnings.get(&1)
        );
    }

    #[test]
    fn unnamed_template_has_no_suggestions() {
        let mut zone = Il2Entity::new("MCU_CheckZone");
        zone.index = Some(1);
        zone.set_property("Index", "1");
        zone.set_name("Alpha");
        let mut timer = Il2Entity::new("MCU_Timer");
        timer.index = Some(2);
        timer.set_property("Index", "2");
        timer.set_name("Done");
        let mut root = Il2Entity::new("Group");
        root.set_name("Odd Plan");
        root.children = vec![zone, timer];
        let info = inspect_plan(&root).unwrap();
        assert!(info.suggested_triggers.is_empty());
        assert!(info.suggested_completion.is_none());
        assert_eq!(info.checkzones.len(), 1);
        assert_eq!(info.timers.len(), 1);
    }

    #[test]
    fn inspect_prefers_mission_end_timer() {
        let mut zone = Il2Entity::new("MCU_CheckZone");
        zone.index = Some(1);
        zone.set_property("Index", "1");
        zone.set_name("Zone IN");
        let mut plane = Il2Entity::new("Plane");
        plane.index = Some(2);
        plane.set_property("Index", "2");
        let mut dump = Il2Entity::new("MCU_Delete");
        dump.index = Some(3);
        dump.set_property("Index", "3");
        dump.set_objects(vec![2]);
        let mut end = Il2Entity::new("MCU_Timer");
        end.index = Some(4);
        end.set_property("Index", "4");
        end.set_name("MISSION END");
        end.set_targets(vec![3]);
        let mut other = Il2Entity::new("MCU_Timer");
        other.index = Some(5);
        other.set_property("Index", "5");
        other.set_name("Delay Delete");
        other.set_targets(vec![3]);
        let mut root = Il2Entity::new("Group");
        root.set_name("End Named");
        root.children = vec![zone, plane, dump, end, other];
        let info = inspect_plan(&root).unwrap();
        assert_eq!(info.suggested_completion, Some(4));
    }

    #[test]
    fn inspect_end_timer_that_deletes_units() {
        let mut zone = Il2Entity::new("MCU_CheckZone");
        zone.index = Some(1);
        zone.set_property("Index", "1");
        zone.set_name("Zone IN");
        let mut vehicle = Il2Entity::new("Vehicle");
        vehicle.index = Some(10);
        vehicle.set_property("Index", "10");
        let mut deact = Il2Entity::new("MCU_Deactivate");
        deact.index = Some(11);
        deact.set_property("Index", "11");
        deact.set_name("Park");
        deact.set_objects(vec![10]);
        let mut end = Il2Entity::new("MCU_Timer");
        end.index = Some(12);
        end.set_property("Index", "12");
        end.set_name("Wrap up");
        end.set_targets(vec![11]);
        let mut root = Il2Entity::new("Group");
        root.set_name("Structural End");
        root.children = vec![zone, vehicle, deact, end];
        let info = inspect_plan(&root).unwrap();
        assert_eq!(info.suggested_completion, Some(12));
        assert_eq!(info.unit_count, 1);
    }

    #[test]
    fn user_can_gate_a_subset_of_b29_zones() {
        let root = b29();
        let info = inspect_plan(&root).unwrap();
        let one_zone = info.suggested_triggers[0];
        let end = info.suggested_completion.unwrap();
        let out = link_bomber_plans(&[BomberInput {
            label: "b29".into(),
            source_key: "b29".into(),
            trigger_zone_ids: vec![one_zone],
            completion_timer_id: end,
            root,
        }])
        .unwrap();
        let closer = out.find_by_name("1 Close Zones").unwrap();
        assert_eq!(closer.targets.len(), 1);
        let gated = closer.targets[0];
        let mut hit = 0;
        let mut missed = 0;
        for zone in all_named(&out, "Zone IN") {
            if zone.targets.contains(&out.find_by_name("1OUT - DISABLE").unwrap().index.unwrap()) {
                hit += 1;
                assert_eq!(zone.index, Some(gated));
            } else {
                missed += 1;
            }
        }
        assert_eq!(hit, 1);
        assert_eq!(missed, 8);
    }

    #[test]
    fn two_dprk_plans_mutex() {
        let out = link_bomber_plans(&[input("dprk", dprk()), input("dprk", dprk())]).unwrap();
        assert_eq!(out.name(), Some("Exclusive Activation"));
        let gates = out.find_by_name("NodeGates").unwrap();
        assert!(gates.find_by_name("1IN - ENABLE").is_some());
        assert!(gates.find_by_name("2IN - DISABLE").is_some());
        let one_out_dis = gates.find_by_name("1OUT - DISABLE").unwrap().index.unwrap();
        let two_out_en = gates.find_by_name("2OUT - ENABLE").unwrap().index.unwrap();
        let starts: Vec<_> = all_named(&out, "MISSION START");
        assert_eq!(starts.len(), 2);
        assert!(starts[0].targets.contains(&one_out_dis));
        let (x1, _) = starts[0].pos_xz().unwrap();
        let (x2, _) = starts[1].pos_xz().unwrap();
        assert!((x2 - x1 - crate::placement::GRID_STEP).abs() < 1.0);
        let cleanups: Vec<_> = all_named(&out, "MISSION CLEAN UP (2m)");
        assert_eq!(cleanups.len(), 2);
        assert!(cleanups[1].targets.contains(&two_out_en));
        let two_in = gates.find_by_name("2IN - ENABLE").unwrap().index.unwrap();
        let two_in_dis = gates.find_by_name("2IN - DISABLE").unwrap().index.unwrap();
        let one_in = gates.find_by_name("1IN - ENABLE").unwrap().index.unwrap();
        let mut enable_fanout = false;
        let mut disable_fanout = false;
        gates.for_each(&mut |e| {
            if e.name() == Some("1") && e.targets.contains(&two_in) {
                enable_fanout = true;
                assert!(!e.targets.contains(&one_in));
            }
            if e.name() == Some("1") && e.targets.contains(&two_in_dis) {
                disable_fanout = true;
            }
        });
        assert!(enable_fanout);
        assert!(disable_fanout);
        assert!(out.find_by_name("DPRK Bombers - Small Formation [1]").is_some());
        assert!(out.find_by_name("DPRK Bombers - Small Formation [2]").is_some());
    }

    #[test]
    fn b29_all_zone_ins_close_other_plans() {
        let out = link_bomber_plans(&[input("b29", b29()), input("dprk", dprk())]).unwrap();
        let gates = out.find_by_name("NodeGates").unwrap();
        let out_dis = gates.find_by_name("1OUT - DISABLE").unwrap().index.unwrap();
        let zones = all_named(&out, "Zone IN");
        assert_eq!(zones.len(), 9);
        for zone in &zones {
            assert!(
                zone.targets.contains(&out_dis),
                "Zone IN {} missing OUT-DISABLE",
                zone.index.unwrap()
            );
        }
        let end = out.find_by_name("END").unwrap();
        let out_en = gates.find_by_name("1OUT - ENABLE").unwrap().index.unwrap();
        assert!(end.targets.contains(&out_en));
        assert!(out.find_by_name("END MISSION").unwrap().targets.iter().all(|t| {
            *t != gates.find_by_name("2OUT - DISABLE").unwrap().index.unwrap()
        }));
        let closer = gates.find_by_name("1 Close Zones").unwrap();
        assert_eq!(closer.targets.len(), 9);
        let opener = gates.find_by_name("2 Open Zones").unwrap();
        assert_eq!(opener.targets.len(), 1);
    }

    #[test]
    fn mixed_profiles_keep_unique_indexes() {
        let out = link_bomber_plans(&[
            input("b29", b29()),
            input("dprk", dprk()),
            input("b29", b29()),
        ])
        .unwrap();
        let mut ids = Vec::new();
        out.collect_indexes(&mut ids);
        let set: HashSet<i32> = ids.iter().copied().collect();
        assert_eq!(ids.len(), set.len());
        assert!(out.find_by_name("B-29 XCountryFlight [1]").is_some());
        assert!(out.find_by_name("DPRK Bombers - Small Formation [2]").is_some());
        assert!(out.find_by_name("B-29 XCountryFlight [3]").is_some());
        assert_eq!(all_named(&out, "Zone IN").len(), 18);
        let text = serialize_group(&out);
        let reparsed = parse_group_file(&text).expect("reparse");
        assert_eq!(reparsed.name(), Some("Exclusive Activation"));
        assert!(reparsed.find_by_name("3IN - ENABLE").is_some());
    }

    #[test]
    fn single_plan_still_links_gates() {
        let out = link_bomber_plans(&[input("dprk", dprk())]).unwrap();
        let gates = out.find_by_name("NodeGates").unwrap();
        let fanouts: Vec<_> = all_named(gates, "1");
        assert_eq!(fanouts.len(), 2);
        for f in fanouts {
            assert!(f.targets.is_empty());
        }
    }

    #[test]
    fn looks_like_pack_and_extract_strips_gates() {
        let out = link_bomber_plans(&[input("dprk", dprk()), input("b29", b29())]).unwrap();
        assert!(looks_like_exclusive_pack(&out));
        assert!(!looks_like_exclusive_pack(&dprk()));
        let plans = extract_exclusive_plans(&out).unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].name(), Some("DPRK Bombers - Small Formation"));
        assert_eq!(plans[1].name(), Some("B-29 XCountryFlight"));
        let mut wrapped = Il2Entity::new("Group");
        wrapped.set_name("Mission");
        wrapped.children.push(out.clone());
        assert!(looks_like_exclusive_pack(&wrapped));
        assert_eq!(extract_exclusive_plans(&wrapped).unwrap().len(), 2);

        let gates = out.find_by_name("NodeGates").unwrap();
        let mut gate_ids = Vec::new();
        gates.collect_indexes(&mut gate_ids);
        for plan in &plans {
            plan.for_each(&mut |e| {
                for id in &e.targets {
                    assert!(!gate_ids.contains(id), "stale NodeGates target {id}");
                }
            });
            inspect_plan(plan).expect("extracted plan still inspects");
        }
    }

    #[test]
    fn keep_positions_leaves_plan_coords() {
        let a = dprk();
        let mut b = dprk();
        b.translate_xz(55_000.0, 12_000.0);
        let (ax, az) = a.first_xz().unwrap();
        let (bx, bz) = b.first_xz().unwrap();
        let out = link_bomber_plans_with(
            &[input("a", a), input("b", b)],
            true,
        )
        .unwrap();
        let p1 = out
            .find_by_name("DPRK Bombers - Small Formation [1]")
            .unwrap()
            .first_xz()
            .unwrap();
        let p2 = out
            .find_by_name("DPRK Bombers - Small Formation [2]")
            .unwrap()
            .first_xz()
            .unwrap();
        assert!((p1.0 - ax).abs() < 1.0 && (p1.1 - az).abs() < 1.0);
        assert!((p2.0 - bx).abs() < 1.0 && (p2.1 - bz).abs() < 1.0);
        let (x1, _) = all_named(&out, "MISSION START")[0].pos_xz().unwrap();
        let (x2, _) = all_named(&out, "MISSION START")[1].pos_xz().unwrap();
        assert!((x2 - x1 - 55_000.0).abs() < 1.0);
    }

    #[test]
    fn extract_then_add_plan_relinks() {
        let packed = link_bomber_plans(&[input("dprk", dprk())]).unwrap();
        let mut plans = extract_exclusive_plans(&packed).unwrap();
        plans.push(b29());
        let inputs: Vec<BomberInput> = plans
            .into_iter()
            .enumerate()
            .map(|(i, root)| input(&format!("p{i}"), root))
            .collect();
        let out = link_bomber_plans_with(&inputs, true).unwrap();
        assert_eq!(out.children.iter().filter(|c| c.block_type == "Group" && c.name() != Some("NodeGates")).count(), 2);
        assert!(out.find_by_name("2IN - ENABLE").is_some());
        assert!(out.find_by_name("B-29 XCountryFlight [2]").is_some());
    }
}
