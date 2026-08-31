//! Strip Freeflight / Task Editor player logic from an airfield group so it
//! can be used in multiplayer.
//!
//! IL-2 object-links a player plane into nearby MCU_CheckZone `Objects` lists
//! (proximity). Those zones stay; the player id is removed and
//! `PlaneCoalitions` is set to the friendly coalition (Western `[2]` for USA).
//! The player aircraft, its entity, and the single-player graph hanging off
//! them (takeoff, music, objectives, tiny CZ_PLAYER_OUT bubbles) are deleted.

use std::collections::{HashMap, HashSet};

use crate::ast::Il2Entity;

/// Western / UN / USA friendly planes. Matches Seoul AFB FRND checkzones.
pub const WESTERN_PLANE_COALITIONS: &str = "[2]";
/// Eastern / USSR / DPRK friendly planes.
pub const EASTERN_PLANE_COALITIONS: &str = "[1]";

const AUTO_REMOVE: &str = "AutoRemove";
/// Checkzones smaller than this (meters) that object-link the player are SP bubbles.
const PLAYER_BUBBLE_ZONE_M: f64 = 200.0;
const PLAYER_ORBIT_M: f64 = 2500.0;

const WORLD_TYPES: &[&str] = &[
    "Vehicle",
    "Ship",
    "Plane",
    "Block",
    "Ground",
    "MCU_TR_Entity",
];

#[derive(Debug, Clone)]
pub struct PlayerPlaneInfo {
    pub name: String,
    pub country: i32,
}

#[derive(Debug, Clone)]
pub struct AirfieldInfo {
    pub name: String,
    pub in_group: bool,
    pub has_autoremove: bool,
    pub player_planes: Vec<PlayerPlaneInfo>,
    pub unlink_zones: Vec<String>,
    pub strip_count: usize,
    pub vehicle_count: usize,
    pub ai_plane_count: usize,
    pub block_count: usize,
    pub checkzone_count: usize,
    pub origin_xz: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub struct CleanReport {
    pub stripped: usize,
    pub unlinked_checkzones: usize,
    pub plane_coalitions: String,
}

struct Plan {
    strip: HashSet<i32>,
    unlink: Vec<i32>,
}

pub fn inspect_airfield(root: &Il2Entity) -> AirfieldInfo {
    let plan = plan_clean(root);
    let unlink_zones = plan
        .unlink
        .iter()
        .filter_map(|&id| {
            find_index(root, id).map(|z| z.name().unwrap_or("Check Zone").to_string())
        })
        .collect();
    AirfieldInfo {
        name: root.name().unwrap_or("Airfield").to_string(),
        in_group: root.block_type == "Group",
        has_autoremove: root.find_by_name(AUTO_REMOVE).is_some(),
        player_planes: collect_player_planes(root),
        unlink_zones,
        strip_count: plan.strip.len(),
        vehicle_count: root.count_block_type("Vehicle") + root.count_block_type("Ship"),
        ai_plane_count: count_ai_planes(root),
        block_count: root.count_block_type("Block") + root.count_block_type("Ground"),
        checkzone_count: root.count_block_type("MCU_CheckZone"),
        origin_xz: root.first_xz(),
    }
}

/// Remove the player and SP logic; retarget proximity checkzones to `plane_coalitions`.
pub fn clean_airfield(
    root: &mut Il2Entity,
    plane_coalitions: &str,
) -> Result<CleanReport, String> {
    let plan = plan_clean(root);
    if plan.strip.is_empty() && plan.unlink.is_empty() {
        return Ok(CleanReport {
            stripped: 0,
            unlinked_checkzones: 0,
            plane_coalitions: plane_coalitions.to_string(),
        });
    }
    let unlink_n = plan.unlink.len();
    for id in &plan.unlink {
        if let Some(zone) = find_index_mut(root, *id) {
            let objects: Vec<i32> = zone
                .objects
                .iter()
                .copied()
                .filter(|oid| !plan.strip.contains(oid))
                .collect();
            zone.set_objects(objects);
            zone.set_property("PlaneCoalitions", plane_coalitions);
        }
    }
    let stripped = plan.strip.len();
    remove_stripped(root, &plan.strip);
    scrub_deleted(root, &plan.strip);
    Ok(CleanReport {
        stripped,
        unlinked_checkzones: unlink_n,
        plane_coalitions: plane_coalitions.to_string(),
    })
}

fn plan_clean(root: &Il2Entity) -> Plan {
    let by_index = index_map(root);
    let player_ids = player_object_ids(root);
    let mut strip = player_ids.clone();

    if let Some(auto) = root.find_by_name(AUTO_REMOVE) {
        auto.for_each(&mut |e| {
            if let Some(id) = e.index {
                strip.insert(id);
            }
        });
    }

    for e in by_index.values() {
        if e.objects.iter().any(|id| player_ids.contains(id)) {
            if e.block_type == "MCU_CheckZone" {
                if zone_radius(e) < PLAYER_BUBBLE_ZONE_M {
                    if let Some(id) = e.index {
                        strip.insert(id);
                    }
                }
            } else if let Some(id) = e.index {
                strip.insert(id);
            }
        }
        if e.block_type == "MCU_TR_MissionObjective" {
            if let Some(id) = e.index {
                if near_any_player(root, e) {
                    strip.insert(id);
                }
            }
        }
        if matches!(e.block_type.as_str(), "MCU_Icon" | "MCU_TR_Subtitle") {
            if let Some(id) = e.index {
                if near_any_player(root, e) {
                    strip.insert(id);
                }
            }
        }
    }

    let mut queue: Vec<i32> = strip.iter().copied().collect();
    while let Some(id) = queue.pop() {
        let Some(node) = by_index.get(&id) else {
            continue;
        };
        if node.block_type == "MCU_CheckZone" && !strip.contains(&id) {
            continue;
        }
        for next in node_link_ids(node) {
            if strip.contains(&next) {
                continue;
            }
            let Some(n) = by_index.get(&next) else {
                continue;
            };
            if n.block_type == "MCU_CheckZone" {
                continue;
            }
            if WORLD_TYPES.contains(&n.block_type.as_str()) && n.block_type != "MCU_TR_Entity" {
                continue;
            }
            strip.insert(next);
            queue.push(next);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (id, node) in &by_index {
            if strip.contains(id) {
                continue;
            }
            if node.block_type == "Group" || node.block_type == "MCU_CheckZone" {
                continue;
            }
            if WORLD_TYPES.contains(&node.block_type.as_str()) {
                continue;
            }
            let links: Vec<i32> = node_link_ids(node)
                .into_iter()
                .filter(|lid| by_index.contains_key(lid))
                .collect();
            if links.is_empty() {
                continue;
            }
            if links.iter().all(|lid| strip.contains(lid)) {
                strip.insert(*id);
                changed = true;
            }
        }
    }

    let mut unlink = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "MCU_CheckZone" {
            return;
        }
        let Some(id) = e.index else {
            return;
        };
        if strip.contains(&id) {
            return;
        }
        if e.objects.iter().any(|oid| player_ids.contains(oid)) {
            unlink.push(id);
        }
    });
    unlink.sort_unstable();
    unlink.dedup();
    Plan { strip, unlink }
}

fn player_object_ids(root: &Il2Entity) -> HashSet<i32> {
    let mut ids = HashSet::new();
    root.for_each(&mut |e| {
        if !is_player_plane(e) {
            return;
        }
        if let Some(id) = e.index {
            ids.insert(id);
        }
        if let Some(link) = e.property("LinkTrId").and_then(|v| v.parse::<i32>().ok()) {
            if link > 0 {
                ids.insert(link);
            }
        }
    });
    let snapshot: Vec<i32> = ids.iter().copied().collect();
    root.for_each(&mut |e| {
        if e.block_type != "MCU_TR_Entity" {
            return;
        }
        let Some(id) = e.index else {
            return;
        };
        if snapshot.contains(&id) {
            if let Some(mis) = e.property("MisObjID").and_then(|v| v.parse::<i32>().ok()) {
                if mis > 0 {
                    ids.insert(mis);
                }
            }
            ids.insert(id);
        }
    });
    ids
}

fn collect_player_planes(root: &Il2Entity) -> Vec<PlayerPlaneInfo> {
    let mut out = Vec::new();
    root.for_each(&mut |e| {
        if !is_player_plane(e) {
            return;
        }
        let country = e
            .property("Country")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let name = plane_type_label(e);
        out.push(PlayerPlaneInfo { name, country });
    });
    out
}

fn is_player_plane(e: &Il2Entity) -> bool {
    e.block_type == "Plane" && e.property("AILevel") == Some("0")
}

fn plane_type_label(plane: &Il2Entity) -> String {
    plane
        .property("Script")
        .and_then(|s| s.rsplit(['/', '\\']).next())
        .map(|s| s.trim_end_matches(".txt").to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| plane.name().map(|n| n.to_string()))
        .unwrap_or_else(|| "Player plane".into())
}

fn count_ai_planes(root: &Il2Entity) -> usize {
    let mut n = 0usize;
    root.for_each(&mut |e| {
        if e.block_type == "Plane" && e.property("AILevel") != Some("0") {
            n += 1;
        }
    });
    n
}

fn zone_radius(e: &Il2Entity) -> f64 {
    e.property("Zone")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

fn near_any_player(root: &Il2Entity, e: &Il2Entity) -> bool {
    let Some((ex, ez)) = e.pos_xz().or_else(|| e.first_xz()) else {
        return false;
    };
    let mut near = false;
    root.for_each(&mut |p| {
        if near || !is_player_plane(p) {
            return;
        }
        if let Some((px, pz)) = p.pos_xz() {
            let dx = ex - px;
            let dz = ez - pz;
            if dx * dx + dz * dz <= PLAYER_ORBIT_M * PLAYER_ORBIT_M {
                near = true;
            }
        }
    });
    near
}

fn node_link_ids(e: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    ids.extend_from_slice(&e.targets);
    ids.extend_from_slice(&e.objects);
    for key in ["LinkTrId", "MisObjID", "TarId", "CmdId"] {
        if let Some(v) = e.property(key) {
            if let Ok(id) = v.parse::<i32>() {
                if id > 0 {
                    ids.push(id);
                }
            }
        }
    }
    for child in &e.children {
        if matches!(
            child.block_type.as_str(),
            "OnEvents" | "OnReports" | "OnEvent" | "OnReport"
        ) {
            ids.extend(node_link_ids(child));
        }
    }
    ids
}

fn index_map(root: &Il2Entity) -> HashMap<i32, &Il2Entity> {
    let mut map = HashMap::new();
    fill_index_map(root, &mut map);
    map
}

fn fill_index_map<'a>(e: &'a Il2Entity, map: &mut HashMap<i32, &'a Il2Entity>) {
    if let Some(id) = e.index {
        map.insert(id, e);
    }
    for child in &e.children {
        fill_index_map(child, map);
    }
}

fn find_index(root: &Il2Entity, index: i32) -> Option<&Il2Entity> {
    if root.index == Some(index) {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_index(c, index))
}

fn find_index_mut(root: &mut Il2Entity, index: i32) -> Option<&mut Il2Entity> {
    if root.index == Some(index) {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|c| find_index_mut(c, index))
}

fn remove_stripped(root: &mut Il2Entity, strip: &HashSet<i32>) {
    root.children.retain(|c| {
        if c.name() == Some(AUTO_REMOVE) {
            return false;
        }
        !c.index.is_some_and(|id| strip.contains(&id))
    });
    for child in &mut root.children {
        remove_stripped(child, strip);
    }
}

fn scrub_deleted(root: &mut Il2Entity, deleted: &HashSet<i32>) {
    root.for_each_mut(&mut |e| {
        let targets: Vec<i32> = e
            .targets
            .iter()
            .copied()
            .filter(|id| !deleted.contains(id))
            .collect();
        if targets.len() != e.targets.len() {
            e.set_targets(targets);
        }
        let objects: Vec<i32> = e
            .objects
            .iter()
            .copied()
            .filter(|id| !deleted.contains(id))
            .collect();
        if objects.len() != e.objects.len() {
            e.set_objects(objects);
        }
    });
    scrub_event_children(root, deleted);
}

fn scrub_event_children(e: &mut Il2Entity, deleted: &HashSet<i32>) {
    if e.block_type == "OnEvents" || e.block_type == "OnReports" {
        e.children.retain(|c| {
            let tar = c.property("TarId").and_then(|v| v.parse::<i32>().ok());
            let cmd = c.property("CmdId").and_then(|v| v.parse::<i32>().ok());
            !tar.is_some_and(|id| deleted.contains(&id))
                && !cmd.is_some_and(|id| deleted.contains(&id))
        });
    }
    for child in &mut e.children {
        scrub_event_children(child, deleted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_group_file, parse_il2_document};
    use crate::serialize::serialize_group;

    fn seoul_grouped() -> Il2Entity {
        parse_group_file(include_str!("../TemplateExamples/Seoul AFB no helper.Group"))
            .expect("Seoul AFB no helper")
    }

    fn seoul_flat() -> Il2Entity {
        parse_il2_document(include_str!("../TemplateExamples/Seoul AFB.Group"))
            .expect("Seoul AFB helper")
    }

    fn leader_zones(root: &Il2Entity) -> Vec<&Il2Entity> {
        let mut out = Vec::new();
        collect_named_zones(root, "CZ_LEADER_PLAYER_OUT", &mut out);
        out
    }

    fn collect_named_zones<'a>(e: &'a Il2Entity, name: &str, out: &mut Vec<&'a Il2Entity>) {
        if e.block_type == "MCU_CheckZone" && e.name() == Some(name) {
            out.push(e);
        }
        for child in &e.children {
            collect_named_zones(child, name, out);
        }
    }

    #[test]
    fn inspect_grouped_seoul_finds_player_and_four_zones() {
        let root = seoul_grouped();
        let info = inspect_airfield(&root);
        assert!(info.in_group);
        assert!(!info.has_autoremove);
        assert_eq!(info.player_planes.len(), 1);
        assert_eq!(info.player_planes[0].country, 601);
        assert!(info.player_planes[0].name.to_lowercase().contains("f80"));
        assert_eq!(info.unlink_zones.len(), 4);
        assert!(
            info.unlink_zones
                .iter()
                .all(|n| n == "CZ_LEADER_PLAYER_OUT")
        );
        assert!(info.strip_count > 10);
        assert!(info.vehicle_count > 0);
        assert!(info.checkzone_count > 4);
    }

    #[test]
    fn clean_grouped_seoul_unlinks_leader_zones_and_drops_player() {
        let mut root = seoul_grouped();
        let report = clean_airfield(&mut root, WESTERN_PLANE_COALITIONS).unwrap();
        assert_eq!(report.unlinked_checkzones, 4);
        assert!(report.stripped > 10);

        let mut player = 0usize;
        root.for_each(&mut |e| {
            if is_player_plane(e) {
                player += 1;
            }
        });
        assert_eq!(player, 0);
        assert!(root.find_by_name("CZ_PLAYER_OUT").is_none());
        assert!(root.find_by_name("YOU_ARE_KILLED").is_none());
        assert!(root.find_by_name("Take off").is_none());
        assert!(root.find_by_name("Mission Objective Subtitle").is_some());

        let zones = leader_zones(&root);
        assert_eq!(zones.len(), 4);
        for z in &zones {
            assert!(z.objects.is_empty(), "zone still object-linked: {:?}", z.objects);
            assert_eq!(z.property("PlaneCoalitions"), Some("[2]"));
        }

        let mut ids = Vec::new();
        root.for_each(&mut |e| {
            if let Some(i) = e.index {
                ids.push(i);
            }
        });
        assert!(!ids.contains(&92), "player entity index survived");

        let text = serialize_group(&root);
        parse_group_file(&text).expect("reparse cleaned grouped airfield");
    }

    #[test]
    fn clean_flat_helper_drops_autoremove_and_keeps_airfield_zones() {
        let mut root = seoul_flat();
        let info = inspect_airfield(&root);
        assert!(info.has_autoremove);
        assert_eq!(info.unlink_zones.len(), 4);

        clean_airfield(&mut root, WESTERN_PLANE_COALITIONS).unwrap();
        assert!(root.find_by_name(AUTO_REMOVE).is_none());
        assert!(root.find_by_name("CZ_PLAYER_OUT").is_none());
        let zones = leader_zones(&root);
        assert_eq!(zones.len(), 4);
        for z in zones {
            assert!(z.objects.is_empty());
            assert_eq!(z.property("PlaneCoalitions"), Some("[2]"));
        }
        let mut player = 0usize;
        root.for_each(&mut |e| {
            if is_player_plane(e) {
                player += 1;
            }
        });
        assert_eq!(player, 0);
        assert!(root.find_by_name("VERTICAL_SearchLightArea").is_some());
        let text = serialize_group(&root);
        parse_group_file(&text).expect("reparse cleaned flat airfield");
    }

    #[test]
    fn already_clean_file_is_a_no_op() {
        let mut root = seoul_grouped();
        clean_airfield(&mut root, WESTERN_PLANE_COALITIONS).unwrap();
        let before = inspect_airfield(&root);
        assert!(before.player_planes.is_empty());
        let report = clean_airfield(&mut root, WESTERN_PLANE_COALITIONS).unwrap();
        assert_eq!(report.stripped, 0);
        assert_eq!(report.unlinked_checkzones, 0);
    }
}
