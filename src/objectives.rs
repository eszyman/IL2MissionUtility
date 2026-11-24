//! Mission objectives built from units and reference blocks.
//!
//! A checked unit scores for the other side. A map click watches every
//! reference `Block` within 500 m (a bridge, a mine, or any other reference).
//! One Trigger Counter per side fires when the chosen share of those objects
//! has been destroyed. The default share is 33%. Neutral objects (`Country`
//! 0) are written as country 601.
//!
//! The chain matches `Objective_demo.Group`: each object's `MCU_TR_Entity`
//! fires OnKilled into the counter, the counter pulses a 5 s delay, and that
//! delay pulses the success and failure Mission Objectives, a subtitle, and
//! a 65 s timer that pulses Translator Mission End.

use crate::ast::Il2Entity;
use crate::locale::LocaleTable;

/// The side that scores when the selected objects are destroyed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjectiveSide {
    Nato,
    Dprk,
}

impl ObjectiveSide {
    pub fn label(self) -> &'static str {
        match self {
            ObjectiveSide::Nato => "NATO",
            ObjectiveSide::Dprk => "DPRK",
        }
    }

    fn coalition(self) -> i32 {
        match self {
            ObjectiveSide::Nato => 2,
            ObjectiveSide::Dprk => 1,
        }
    }

    fn other(self) -> Self {
        match self {
            ObjectiveSide::Nato => ObjectiveSide::Dprk,
            ObjectiveSide::Dprk => ObjectiveSide::Nato,
        }
    }
}

/// A placed group from the tasking list. `eastern` is the side that owns it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ObjectiveSource {
    Army { eastern: bool, stem: String },
    Ground { eastern: bool },
    Ships { eastern: bool },
}

impl ObjectiveSource {
    pub fn eastern(&self) -> bool {
        match self {
            ObjectiveSource::Army { eastern, .. }
            | ObjectiveSource::Ground { eastern }
            | ObjectiveSource::Ships { eastern } => *eastern,
        }
    }

    /// Destroying this group scores for the other coalition.
    pub fn score_for(&self) -> ObjectiveSide {
        if self.eastern() {
            ObjectiveSide::Nato
        } else {
            ObjectiveSide::Dprk
        }
    }

    /// Name written on the generated group. Files keep "Eastern".
    pub fn group_name(&self) -> String {
        let side = if self.eastern() { "Eastern" } else { "NATO" };
        match self {
            ObjectiveSource::Army { stem, .. } => format!("{side} {stem}"),
            ObjectiveSource::Ground { .. } => format!("{side} Ground"),
            ObjectiveSource::Ships { .. } => format!("{side} Shipping"),
        }
    }

}

/// One map click: every reference block it captured.
#[derive(Clone, Debug)]
pub struct ObjectivePick {
    pub label: String,
    pub x: f64,
    pub z: f64,
    pub positions: Vec<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct ObjectiveGroup {
    pub name: String,
    pub score_for: ObjectiveSide,
}

/// Reference blocks watched for one side. Stored positions are exact.
/// An empty list means every reference block within 500 m of `x`, `z`.
#[derive(Clone, Debug)]
pub struct ObjectiveCluster {
    pub x: f64,
    pub z: f64,
    pub score_for: ObjectiveSide,
    pub positions: Vec<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct ObjectivePlan {
    pub groups: Vec<ObjectiveGroup>,
    pub clusters: Vec<ObjectiveCluster>,
    pub nato_pct: u32,
    pub dprk_pct: u32,
    /// How many DPRK automatic fighters NATO must destroy. 0 leaves them out.
    pub nato_aircraft: u32,
    /// How many NATO automatic fighters DPRK must destroy. 0 leaves them out.
    pub dprk_aircraft: u32,
    /// How many DPRK mission aircraft NATO must destroy. 0 leaves them out.
    pub nato_mission: u32,
    /// How many NATO mission aircraft DPRK must destroy. 0 leaves them out.
    pub dprk_mission: u32,
    /// Checked missions. Their aircraft are watched by the other side's counter.
    pub flights: Vec<ObjectiveFlight>,
}

/// A checked mission. Destroying its aircraft scores for the other side.
#[derive(Clone, Debug)]
pub struct ObjectiveFlight {
    pub name: String,
    /// The side that owns the aircraft.
    pub eastern: bool,
}

impl ObjectivePlan {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
            && self.clusters.is_empty()
            && self.nato_aircraft == 0
            && self.dprk_aircraft == 0
            && self.nato_mission == 0
            && self.dprk_mission == 0
    }

    fn pct(&self, side: ObjectiveSide) -> u32 {
        match side {
            ObjectiveSide::Nato => self.nato_pct,
            ObjectiveSide::Dprk => self.dprk_pct,
        }
        .clamp(1, 100)
    }

    fn aircraft(&self, side: ObjectiveSide) -> u32 {
        match side {
            ObjectiveSide::Nato => self.nato_aircraft,
            ObjectiveSide::Dprk => self.dprk_aircraft,
        }
    }

    fn mission_aircraft(&self, side: ObjectiveSide) -> u32 {
        match side {
            ObjectiveSide::Nato => self.nato_mission,
            ObjectiveSide::Dprk => self.dprk_mission,
        }
    }
}

/// Restored from a generated base map or mission group.
#[derive(Clone, Debug)]
pub struct RecalledObjectives {
    pub groups: Vec<String>,
    pub nato_picks: Vec<ObjectivePick>,
    pub dprk_picks: Vec<ObjectivePick>,
    pub nato_pct: u32,
    pub dprk_pct: u32,
    pub nato_aircraft: u32,
    pub dprk_aircraft: u32,
    pub nato_mission: u32,
    pub dprk_mission: u32,
    pub flights: Vec<ObjectiveFlight>,
    pub counter_ids: Vec<i32>,
}

/// A reference piece a map click can turn into a linked entity.
/// Airfield buildings are `Block`. The field itself is an `Airfield`.
pub fn is_reference_object(block_type: &str) -> bool {
    matches!(block_type, "Block" | "Bridge" | "Airfield")
}

const SCORED_TYPES: &[&str] = &[
    "Vehicle", "Ship", "Train", "Plane", "Block", "Bridge", "Airfield",
];

/// A placed object the objective counter can watch.
pub fn is_scored_object(block_type: &str) -> bool {
    SCORED_TYPES.contains(&block_type)
}

/// `Enabled = 0` is off. A missing Enabled is on, which is how vehicles are written.
pub fn is_active_object(entity: &Il2Entity) -> bool {
    entity.property("Enabled") != Some("0")
}

/// Individual objects that can be destroyed, skipping a disabled one and the group that holds them.
pub fn active_scored_count(root: &Il2Entity) -> usize {
    let mut n = 0usize;
    root.for_each(&mut |entity| {
        if is_scored_object(&entity.block_type) && is_active_object(entity) {
            n += 1;
        }
    });
    n
}
const POINT_REACH_M: f64 = 500.0;
const POSITION_MATCH_M: f64 = 5.0;
const ON_KILLED: i32 = 13;
/// Neutral `Country` 0 is written as USA, which is what the editor example uses.
const NEUTRAL_COUNTRY: i32 = 601;

/// How many objects must be destroyed for `pct` percent of `total`.
pub fn destruction_count(total: usize, pct: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    let pct = pct.clamp(1, 100) as usize;
    let rounded = (total * pct + 50) / 100;
    rounded.clamp(1, total) as u32
}

/// Add the objective chain. An empty plan leaves the group unchanged.
pub fn apply_objectives(
    root: &mut Il2Entity,
    locale: &mut LocaleTable,
    plan: &ObjectivePlan,
    lc_start: i32,
) -> Vec<String> {
    if plan.is_empty() {
        return Vec::new();
    }
    let mut notes = Vec::new();
    let mut nato = Vec::new();
    let mut dprk = Vec::new();
    for group in &plan.groups {
        let Some(node) = root.find_by_name(&group.name) else {
            notes.push(format!("{} is not in this base map.", group.name));
            continue;
        };
        let ids = killable_ids(node);
        if ids.is_empty() {
            notes.push(format!("{} has nothing that can be destroyed.", group.name));
            continue;
        }
        match group.score_for {
            ObjectiveSide::Nato => nato.extend(ids),
            ObjectiveSide::Dprk => dprk.extend(ids),
        }
    }
    for cluster in &plan.clusters {
        let ids = cluster_blocks(root, cluster);
        if ids.is_empty() {
            notes.push(format!(
                "No reference block within {POINT_REACH_M:.0} m of {:.0}, {:.0} for the {} objective.",
                cluster.x,
                cluster.z,
                cluster.score_for.label()
            ));
            continue;
        }
        match cluster.score_for {
            ObjectiveSide::Nato => nato.extend(ids),
            ObjectiveSide::Dprk => dprk.extend(ids),
        }
    }
    dedup(&mut nato);
    dedup(&mut dprk);

    let mut next = root.max_index().saturating_add(1);
    let mut lc = lc_start.max(1);
    let mut logic = Vec::new();
    let mut mission_end = None;
    for side in [ObjectiveSide::Nato, ObjectiveSide::Dprk] {
        let objects = match side {
            ObjectiveSide::Nato => &nato,
            ObjectiveSide::Dprk => &dprk,
        };
        let want_aircraft = plan.aircraft(side);
        let planes = if want_aircraft == 0 {
            Vec::new()
        } else {
            fighter_plane_ids(root, side == ObjectiveSide::Nato)
        };
        let mission_ids = mission_planes(root, plan, side, &mut notes);
        if objects.is_empty() && planes.is_empty() && mission_ids.is_empty() {
            if want_aircraft > 0 {
                notes.push(format!(
                    "{} objective asks for {want_aircraft} aircraft and no automatic fighters are placed.",
                    side.label()
                ));
            }
            continue;
        }
        if want_aircraft > 0 && (planes.len() as u32) < want_aircraft {
            notes.push(format!(
                "{} objective asks for {want_aircraft} aircraft and {} automatic fighters are placed.",
                side.label(),
                planes.len()
            ));
        }
        let pct = plan.pct(side);
        let want_mission = plan.mission_aircraft(side);
        let need = if !objects.is_empty() {
            destruction_count(objects.len(), pct)
        } else if want_aircraft > 0 {
            want_aircraft
        } else {
            want_mission.max(1)
        };
        let anchor_ids = if !objects.is_empty() {
            objects.as_slice()
        } else if !planes.is_empty() {
            planes.as_slice()
        } else {
            mission_ids.as_slice()
        };
        let (x, z) = object_anchor(root, anchor_ids);
        let end = *mission_end.get_or_insert_with(|| {
            let id = next;
            next += 1;
            logic.push(mission_end_mcu(id, x, z + 80.0));
            id
        });
        let delay = next;
        next += 1;
        let subtitle = next;
        next += 1;
        let next_mission = next;
        next += 1;
        let success = next;
        next += 1;
        let failure = next;
        next += 1;
        if !objects.is_empty() {
            let counter = next;
            next += 1;
            for id in objects {
                wire_object(root, *id, counter, &mut next);
            }
            logic.push(counter_mcu(
                &format!("{pct}% required"),
                counter,
                need,
                x,
                z,
                delay,
            ));
        }
        if !planes.is_empty() {
            let counter = next;
            next += 1;
            for id in &planes {
                wire_object(root, *id, counter, &mut next);
            }
            logic.push(counter_mcu(
                &format!("Aircraft {want_aircraft}"),
                counter,
                want_aircraft.max(1),
                x,
                z - 20.0,
                delay,
            ));
        }
        if !mission_ids.is_empty() {
            let counter = next;
            next += 1;
            for id in &mission_ids {
                wire_object(root, *id, counter, &mut next);
            }
            logic.push(counter_mcu(
                &format!("Mission Aircraft {want_mission}"),
                counter,
                want_mission.max(1),
                x,
                z - 40.0,
                delay,
            ));
        }
        logic.push(delay_timer(
            delay,
            x,
            z + 20.0,
            &[subtitle, next_mission, success, failure],
        ));
        let (name_id, desc_id, text_id) = push_success_text(locale, &mut lc, side, need, pct);
        logic.push(subtitle_mcu(side, subtitle, x, z + 40.0, text_id));
        logic.push(later_timer(next_mission, x, z + 60.0, end));
        logic.push(mission_objective(
            success,
            x - 40.0,
            z + 40.0,
            side,
            true,
            name_id,
            desc_id,
        ));
        let (fail_name, fail_desc) = push_fail_text(locale, &mut lc, side.other());
        logic.push(mission_objective(
            failure,
            x + 40.0,
            z + 40.0,
            side.other(),
            false,
            fail_name,
            fail_desc,
        ));
    }
    if !logic.is_empty() {
        let mut group = named_group("Objectives", &mut next);
        group.children = logic;
        root.children.push(group);
    }
    notes
}

/// Read an `Objectives` group back into the lists and the destruction sliders.
pub fn recall_objectives(root: &Il2Entity) -> Option<RecalledObjectives> {
    let group = root.find_by_name("Objectives")?;
    let mut counters = Vec::new();
    group.for_each(&mut |entity| {
        if entity.block_type == "MCU_Counter" {
            if let Some(id) = entity.index {
                counters.push(id);
            }
        }
    });
    if counters.is_empty() {
        return None;
    }
    let mut recalled = RecalledObjectives {
        groups: Vec::new(),
        nato_picks: Vec::new(),
        dprk_picks: Vec::new(),
        nato_pct: 33,
        dprk_pct: 33,
        nato_aircraft: 0,
        dprk_aircraft: 0,
        nato_mission: 0,
        dprk_mission: 0,
        flights: Vec::new(),
        counter_ids: counters.clone(),
    };
    for counter_id in counters {
        let Some(counter) = find_by_index(group, counter_id) else {
            continue;
        };
        let Some(side) = success_side(group, counter) else {
            continue;
        };
        let counter_name = counter.name().unwrap_or("");
        if let Some(count) = mission_aircraft_from_name(counter_name) {
            match side {
                ObjectiveSide::Nato => recalled.nato_mission = count,
                ObjectiveSide::Dprk => recalled.dprk_mission = count,
            }
            remember_missions(
                &mut recalled.flights,
                side == ObjectiveSide::Nato,
                &mission_names_for_counter(root, counter_id),
            );
            continue;
        }
        if let Some((count, mission)) = planes_from_name(counter_name) {
            let eastern = side == ObjectiveSide::Nato;
            remember_missions(&mut recalled.flights, eastern, &[mission]);
            match side {
                ObjectiveSide::Nato => {
                    recalled.nato_mission = recalled.nato_mission.saturating_add(count);
                }
                ObjectiveSide::Dprk => {
                    recalled.dprk_mission = recalled.dprk_mission.saturating_add(count);
                }
            }
            continue;
        }
        if let Some(count) = aircraft_from_name(counter_name) {
            match side {
                ObjectiveSide::Nato => recalled.nato_aircraft = count,
                ObjectiveSide::Dprk => recalled.dprk_aircraft = count,
            }
            continue;
        }
        let pct = percent_from_name(counter.name().unwrap_or("")).unwrap_or(33);
        match side {
            ObjectiveSide::Nato => recalled.nato_pct = pct,
            ObjectiveSide::Dprk => recalled.dprk_pct = pct,
        }
        let mut units = Vec::new();
        let mut refs: Vec<(f64, f64, String)> = Vec::new();
        for object_id in objects_for_counter(root, counter_id) {
            let mut path = Vec::new();
            let Some((x, z)) = find_located(root, object_id, &mut path) else {
                continue;
            };
            if let Some(name) = reference_name(&path) {
                refs.push((x, z, name));
            } else if let Some(name) = unit_group_name(root.name(), &path) {
                units.push(name);
            }
        }
        for name in units {
            if !recalled.groups.iter().any(|kept| kept == &name) {
                recalled.groups.push(name);
            }
        }
        let picks: Vec<ObjectivePick> = refs
            .into_iter()
            .map(|(x, z, name)| ObjectivePick {
                label: name,
                x,
                z,
                positions: vec![(x, z)],
            })
            .collect();
        match side {
            ObjectiveSide::Nato => recalled.nato_picks.extend(picks),
            ObjectiveSide::Dprk => recalled.dprk_picks.extend(picks),
        }
    }
    Some(recalled)
}

/// Drop OnKilled links that targeted the objective counters, so a reloaded
/// group does not pulse indexes that are no longer in the file.
pub fn strip_counter_events(root: &mut Il2Entity, counter_ids: &[i32]) {
    if counter_ids.is_empty() {
        return;
    }
    strip_events(root, counter_ids);
}

fn strip_events(entity: &mut Il2Entity, counter_ids: &[i32]) {
    for child in &mut entity.children {
        strip_events(child, counter_ids);
    }
    let Some(index) = entity
        .children
        .iter()
        .position(|child| child.block_type == "OnEvents")
    else {
        return;
    };
    entity.children[index].children.retain(|event| {
        !(event.block_type == "OnEvent"
            && event.property("Type") == Some("13")
            && event
                .property("TarId")
                .and_then(|raw| raw.parse().ok())
                .is_some_and(|id: i32| counter_ids.contains(&id)))
    });
    if entity.children[index].children.is_empty() {
        entity.children.remove(index);
    }
}

fn objects_for_counter(root: &Il2Entity, counter: i32) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |entity| {
        if entity.block_type != "MCU_TR_Entity" {
            return;
        }
        let Some(events) = entity.children.iter().find(|c| c.block_type == "OnEvents") else {
            return;
        };
        let tar = counter.to_string();
        let hits = events.children.iter().any(|event| {
            event.block_type == "OnEvent"
                && event.property("Type") == Some("13")
                && event.property("TarId") == Some(tar.as_str())
        });
        if !hits {
            return;
        }
        if let Some(id) = entity
            .property("MisObjID")
            .and_then(|raw| raw.parse().ok())
        {
            ids.push(id);
        }
    });
    ids
}

fn success_side(group: &Il2Entity, counter: &Il2Entity) -> Option<ObjectiveSide> {
    let delay = counter.targets.iter().find_map(|id| {
        let node = find_by_index(group, *id)?;
        (node.block_type == "MCU_Timer").then_some(node)
    })?;
    for id in &delay.targets {
        let Some(node) = find_by_index(group, *id) else {
            continue;
        };
        if node.block_type == "MCU_TR_MissionObjective" && node.property("Success") == Some("1") {
            return match node.property("Coalition") {
                Some("2") => Some(ObjectiveSide::Nato),
                Some("1") => Some(ObjectiveSide::Dprk),
                _ => None,
            };
        }
    }
    None
}

fn mission_aircraft_from_name(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("Mission Aircraft ")?;
    rest.trim().parse().ok()
}

fn remember_missions(flights: &mut Vec<ObjectiveFlight>, eastern: bool, names: &[String]) {
    for name in names {
        if flights
            .iter()
            .any(|flight| flight.eastern == eastern && flight.name == *name)
        {
            continue;
        }
        flights.push(ObjectiveFlight {
            name: name.clone(),
            eastern,
        });
    }
}

fn mission_names_for_counter(root: &Il2Entity, counter: i32) -> Vec<String> {
    let mut names = Vec::new();
    for id in objects_for_counter(root, counter) {
        let Some(name) = mission_group_name(root, id) else {
            continue;
        };
        if !names.iter().any(|kept| kept == &name) {
            names.push(name);
        }
    }
    names
}

/// `B-29`, `B-29 2`, and `B-29 [2]` are the same mission after a load merges copies.
pub fn mission_names_match(mission: &str, recalled: &str) -> bool {
    bare_mission_name(mission) == bare_mission_name(recalled)
}

fn bare_mission_name(name: &str) -> String {
    let name = strip_pack_suffix(name);
    match name.rsplit_once(' ') {
        Some((stem, tail))
            if !stem.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) =>
        {
            stem.to_string()
        }
        _ => name,
    }
}

fn planes_from_name(name: &str) -> Option<(u32, String)> {
    let rest = name.strip_prefix("Planes ")?;
    let (count, mission) = rest.split_once(' ')?;
    let count = count.parse().ok()?;
    if mission.is_empty() {
        return None;
    }
    Some((count, mission.to_string()))
}

fn aircraft_from_name(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("Aircraft ")?;
    let count = rest.trim().parse().ok()?;
    Some(count)
}

fn percent_from_name(name: &str) -> Option<u32> {
    let end = name.find('%')?;
    let start = name[..end]
        .rfind(|c: char| !c.is_ascii_digit())
        .map(|i| i + 1)
        .unwrap_or(0);
    let pct = name[start..end].parse().ok()?;
    (1..=100).contains(&pct).then_some(pct)
}

fn find_located(node: &Il2Entity, id: i32, path: &mut Vec<String>) -> Option<(f64, f64)> {
    let named = node
        .name()
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    if let Some(name) = &named {
        path.push(name.clone());
    }
    if node.index == Some(id) {
        return node.pos_xz();
    }
    for child in &node.children {
        if let Some(pos) = find_located(child, id, path) {
            return Some(pos);
        }
    }
    if named.is_some() {
        path.pop();
    }
    None
}

fn reference_name(path: &[String]) -> Option<String> {
    let index = path
        .iter()
        .position(|name| name == "Reference groups" || name == "Airfields")?;
    path[index + 1..]
        .iter()
        .find(|name| !matches!(name.as_str(), "NATO" | "DPRK" | "Eastern"))
        .cloned()
        .or_else(|| {
            Some(if path[index] == "Airfields" {
                "Airfield".into()
            } else {
                "Reference".into()
            })
        })
}

fn is_objective_container(name: &str) -> bool {
    matches!(
        name,
        "Objectives"
            | "Reference groups"
            | "Airfields"
            | "NATO"
            | "DPRK"
            | "Eastern"
            | "Air Tasking"
            | "Exclusive Activation"
            | "Randomizer"
    )
}

/// The mission group that owns this object. Skips the file root and the packs wrapped around it.
fn mission_group_name(root: &Il2Entity, object_id: i32) -> Option<String> {
    let mut names = Vec::new();
    if !collect_group_names(root, object_id, &mut names) {
        return None;
    }
    let root_name = root.name();
    names.into_iter().find(|name| {
        Some(name.as_str()) != root_name && !is_objective_container(name)
    }).map(|name| strip_pack_suffix(&name))
}

fn collect_group_names(node: &Il2Entity, id: i32, path: &mut Vec<String>) -> bool {
    let named = (node.block_type == "Group")
        .then(|| node.name().filter(|name| !name.is_empty()).map(str::to_string))
        .flatten();
    if let Some(name) = &named {
        path.push(name.clone());
    }
    if node.index == Some(id) {
        return true;
    }
    for child in &node.children {
        if collect_group_names(child, id, path) {
            return true;
        }
    }
    if named.is_some() {
        path.pop();
    }
    false
}

fn unit_group_name(root_name: Option<&str>, path: &[String]) -> Option<String> {
    path.iter()
        .find(|name| {
            Some(name.as_str()) != root_name && !is_objective_container(name)
        })
        .cloned()
}

fn cluster_blocks(root: &Il2Entity, cluster: &ObjectiveCluster) -> Vec<i32> {
    let mut ids = Vec::new();
    for name in ["Reference groups", "Airfields"] {
        let Some(group) = root.find_by_name(name) else {
            continue;
        };
        group.for_each(&mut |entity| {
            if !is_reference_object(&entity.block_type) {
                return;
            }
            let Some(id) = entity.index else {
                return;
            };
            let Some((x, z)) = entity.pos_xz() else {
                return;
            };
            let hit = if cluster.positions.is_empty() {
                (x - cluster.x).hypot(z - cluster.z) <= POINT_REACH_M
            } else {
                cluster
                    .positions
                    .iter()
                    .any(|(px, pz)| (x - px).hypot(z - pz) <= POSITION_MATCH_M)
            };
            if hit {
                ids.push(id);
            }
        });
    }
    dedup(&mut ids);
    ids
}

fn mission_planes(
    root: &Il2Entity,
    plan: &ObjectivePlan,
    side: ObjectiveSide,
    notes: &mut Vec<String>,
) -> Vec<i32> {
    let want = plan.mission_aircraft(side);
    if want == 0 {
        return Vec::new();
    }
    let mut ids = Vec::new();
    for flight in &plan.flights {
        if flight_scores_for(flight.eastern) != side {
            continue;
        }
        ids.extend(flight_plane_ids(root, flight, &plan.flights));
    }
    dedup(&mut ids);
    if ids.is_empty() {
        notes.push(format!(
            "{} objective asks for {want} mission aircraft and none of the checked missions are in the file.",
            side.label()
        ));
        return Vec::new();
    }
    if (ids.len() as u32) < want {
        notes.push(format!(
            "{} objective asks for {want} mission aircraft and {} are in the checked missions.",
            side.label(),
            ids.len()
        ));
    }
    ids
}

/// Destroying these aircraft scores for the other side.
fn flight_scores_for(eastern: bool) -> ObjectiveSide {
    if eastern {
        ObjectiveSide::Nato
    } else {
        ObjectiveSide::Dprk
    }
}

fn flight_plane_ids(
    root: &Il2Entity,
    flight: &ObjectiveFlight,
    flights: &[ObjectiveFlight],
) -> Vec<i32> {
    let mut ids = Vec::new();
    walk_flight(root, flight, flights, &mut ids);
    dedup(&mut ids);
    ids
}

fn walk_flight(
    node: &Il2Entity,
    flight: &ObjectiveFlight,
    flights: &[ObjectiveFlight],
    ids: &mut Vec<i32>,
) {
    if node.block_type == "Group" {
        if is_fighter_pack(node) {
            return;
        }
        if let Some(name) = node.name() {
            if let Some(owner) = flight_owner(name, node, flights) {
                if owner.name == flight.name && owner.eastern == flight.eastern {
                    collect_flight_planes(node, flights, ids);
                }
                return;
            }
        }
    }
    for child in &node.children {
        walk_flight(child, flight, flights, ids);
    }
}

fn collect_flight_planes(node: &Il2Entity, flights: &[ObjectiveFlight], ids: &mut Vec<i32>) {
    for child in &node.children {
        if child.block_type == "Group"
            && child
                .name()
                .is_some_and(|name| flight_owner(name, child, flights).is_some())
        {
            continue;
        }
        if child.block_type == "Plane" {
            if let Some(id) = child.index {
                ids.push(id);
            }
        }
        collect_flight_planes(child, flights, ids);
    }
}

fn flight_owner<'a>(
    name: &str,
    node: &Il2Entity,
    flights: &'a [ObjectiveFlight],
) -> Option<&'a ObjectiveFlight> {
    let bare = strip_pack_suffix(name);
    let eastern = group_is_eastern(node)?;
    flights
        .iter()
        .find(|flight| flight.eastern == eastern && flight_name_matches(&bare, &flight.name, flights))
}

fn flight_name_matches(bare: &str, flight_name: &str, flights: &[ObjectiveFlight]) -> bool {
    if bare == flight_name {
        return true;
    }
    let Some((stem, tail)) = bare.rsplit_once(' ') else {
        return false;
    };
    stem == flight_name
        && !tail.is_empty()
        && tail.chars().all(|c| c.is_ascii_digit())
        && !flights.iter().any(|flight| flight.name == bare)
}

fn strip_pack_suffix(name: &str) -> String {
    let name = name.trim();
    let Some(open) = name.rfind(" [") else {
        return name.to_string();
    };
    let digits = &name[open + 2..];
    if digits.ends_with(']')
        && digits.len() > 1
        && digits[..digits.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        let bare = name[..open].trim();
        if bare.is_empty() {
            name.to_string()
        } else {
            bare.to_string()
        }
    } else {
        name.to_string()
    }
}

fn group_is_eastern(node: &Il2Entity) -> Option<bool> {
    let mut country = None;
    node.for_each(&mut |entity| {
        if country.is_none() && entity.block_type == "Plane" {
            country = entity.property("Country").and_then(|raw| raw.parse().ok());
        }
    });
    country.map(|code: i32| code / 100 != 6)
}

fn fighter_plane_ids(root: &Il2Entity, eastern: bool) -> Vec<i32> {
    let mut ids = Vec::new();
    for child in &root.children {
        if child.block_type != "Group" || !is_fighter_pack(child) {
            continue;
        }
        if pack_is_eastern(child) != eastern {
            continue;
        }
        child.for_each(&mut |entity| {
            if entity.block_type == "Plane" {
                if let Some(id) = entity.index {
                    ids.push(id);
                }
            }
        });
    }
    dedup(&mut ids);
    ids
}

fn is_fighter_pack(entity: &Il2Entity) -> bool {
    entity
        .children
        .iter()
        .any(|child| child.name() == Some("NodeGates"))
        && entity
            .children
            .iter()
            .any(|child| child.name() == Some("Group 1"))
}

fn pack_is_eastern(entity: &Il2Entity) -> bool {
    let name = entity.name().unwrap_or("");
    if name.starts_with("NATO") {
        return false;
    }
    if name.starts_with("Eastern") || name.starts_with("DPRK") {
        return true;
    }
    let mut country = None;
    entity.for_each(&mut |node| {
        if country.is_none() && node.block_type == "Plane" {
            country = node.property("Country").and_then(|raw| raw.parse().ok());
        }
    });
    country.is_none_or(|code: i32| code / 100 != 6)
}

fn dedup(ids: &mut Vec<i32>) {
    ids.sort_unstable();
    ids.dedup();
}

fn object_anchor(root: &Il2Entity, ids: &[i32]) -> (f64, f64) {
    let Some(id) = ids.first() else {
        return (0.0, 0.0);
    };
    find_by_index(root, *id)
        .and_then(Il2Entity::pos_xz)
        .unwrap_or((0.0, 0.0))
}

fn wire_object(root: &mut Il2Entity, object_id: i32, target: i32, next: &mut i32) {
    if !assign_neutral_country(root, object_id) {
        return;
    }
    let link = find_by_index(root, object_id)
        .and_then(|object| object.property("LinkTrId"))
        .and_then(|raw| raw.parse().ok());
    if let Some(id) = link {
        if find_by_index(root, id).is_some_and(|entity| entity.block_type == "MCU_TR_Entity") {
            if let Some(entity) = find_mut(root, id) {
                push_on_killed(entity, target);
            }
            return;
        }
    }
    let _ = insert_entity(root, object_id, target, next);
}

fn assign_neutral_country(root: &mut Il2Entity, object_id: i32) -> bool {
    let Some(object) = find_mut(root, object_id) else {
        return false;
    };
    let country = object
        .property("Country")
        .and_then(|raw| raw.parse::<i32>().ok())
        .unwrap_or(0);
    if country == 0 {
        object.set_property("Country", NEUTRAL_COUNTRY.to_string());
    }
    true
}

fn insert_entity(node: &mut Il2Entity, object_id: i32, target: i32, next: &mut i32) -> bool {
    for i in 0..node.children.len() {
        if node.children[i].index == Some(object_id) {
            let (x, z) = node.children[i].pos_xz().unwrap_or((0.0, 0.0));
            let kind = node.children[i].block_type.clone();
            let id = *next;
            *next += 1;
            node.children[i].set_property("LinkTrId", id.to_string());
            let mut entity = translator_entity(&format!("{kind} entity"), id, x, z, object_id);
            push_on_killed(&mut entity, target);
            node.children.insert(i + 1, entity);
            return true;
        }
    }
    for child in &mut node.children {
        if insert_entity(child, object_id, target, next) {
            return true;
        }
    }
    false
}

fn killable_ids(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |entity| {
        if entity.index.is_some()
            && is_scored_object(&entity.block_type)
            && is_active_object(entity)
        {
            ids.push(entity.index.unwrap());
        }
    });
    dedup(&mut ids);
    ids
}

fn find_by_index(root: &Il2Entity, id: i32) -> Option<&Il2Entity> {
    if root.index == Some(id) {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_by_index(child, id) {
            return Some(found);
        }
    }
    None
}

fn find_mut(root: &mut Il2Entity, id: i32) -> Option<&mut Il2Entity> {
    if root.index == Some(id) {
        return Some(root);
    }
    for child in &mut root.children {
        if let Some(found) = find_mut(child, id) {
            return Some(found);
        }
    }
    None
}

fn push_on_killed(entity: &mut Il2Entity, target: i32) {
    let killed = ON_KILLED.to_string();
    let tar = target.to_string();
    let mut wrap = entity
        .children
        .iter()
        .find(|child| child.block_type == "OnEvents")
        .cloned()
        .unwrap_or_else(|| Il2Entity::new("OnEvents"));
    if wrap.children.iter().any(|event| {
        event.block_type == "OnEvent"
            && event.property("Type") == Some(killed.as_str())
            && event.property("TarId") == Some(tar.as_str())
    }) {
        return;
    }
    let mut event = Il2Entity::new("OnEvent");
    event.set_property("Type", killed);
    event.set_property("TarId", tar);
    wrap.children.push(event);
    entity.children.retain(|child| child.block_type != "OnEvents");
    entity.children.push(wrap);
}

fn push_success_text(
    locale: &mut LocaleTable,
    next: &mut i32,
    side: ObjectiveSide,
    need: u32,
    pct: u32,
) -> (i32, i32, i32) {
    let name_id = *next;
    let desc_id = *next + 1;
    let text_id = *next + 2;
    *next += 3;
    locale.insert(
        name_id,
        format!("Primary Objective Success {}", side.label()),
    );
    locale.insert(
        desc_id,
        format!("Destroy {need} objects ({pct}%). This scores for {}.", side.label()),
    );
    locale.insert(text_id, format!("{} Objective Completed!", side.label()));
    (name_id, desc_id, text_id)
}

fn push_fail_text(locale: &mut LocaleTable, next: &mut i32, side: ObjectiveSide) -> (i32, i32) {
    let name_id = *next;
    let desc_id = *next + 1;
    *next += 2;
    locale.insert(
        name_id,
        format!("Primary Objective Failed {}", side.label()),
    );
    locale.insert(desc_id, "Mission Objective Desc".to_string());
    (name_id, desc_id)
}

fn named_group(name: &str, next: &mut i32) -> Il2Entity {
    let id = *next;
    *next += 1;
    let mut group = Il2Entity::new("Group");
    group.index = Some(id);
    group.set_property("Index", id.to_string());
    group.set_name(name);
    group.set_property("Desc", "\"\"");
    group
}

fn counter_mcu(name: &str, id: i32, count: u32, x: f64, z: f64, target: i32) -> Il2Entity {
    let mut entity = mcu("MCU_Counter", name, id, x, z);
    entity.set_property("Counter", count.to_string());
    entity.set_property("Dropcount", "0");
    entity.set_targets(vec![target]);
    entity
}

fn delay_timer(id: i32, x: f64, z: f64, targets: &[i32]) -> Il2Entity {
    let mut entity = mcu("MCU_Timer", "delay timer", id, x, z);
    entity.set_property("Time", "5");
    entity.set_property("Random", "100");
    entity.set_targets(targets.to_vec());
    entity
}

fn later_timer(id: i32, x: f64, z: f64, target: i32) -> Il2Entity {
    let mut entity = mcu("MCU_Timer", "Next Mission", id, x, z);
    entity.set_property("Time", "65");
    entity.set_property("Random", "100");
    entity.set_targets(vec![target]);
    entity
}

fn mission_end_mcu(id: i32, x: f64, z: f64) -> Il2Entity {
    let mut entity = mcu("MCU_TR_MissionEnd", "Translator Mission End", id, x, z);
    entity.set_property("Enabled", "1");
    entity.set_property("Restart", "0");
    entity
}

fn subtitle_mcu(side: ObjectiveSide, id: i32, x: f64, z: f64, text: i32) -> Il2Entity {
    let mut entity = mcu("MCU_TR_Subtitle", "Subtitle: Mission Complete", id, x, z);
    entity.set_property("Enabled", "1");
    let (r, g, b) = match side {
        ObjectiveSide::Dprk => (190, 0, 0),
        ObjectiveSide::Nato => (0, 120, 150),
    };
    let mut info = Il2Entity::new("SubtitleInfo");
    info.set_property("Duration", "60");
    info.set_property("FontSize", "20");
    info.set_property("HAlign", "1");
    info.set_property("VAlign", "1");
    info.set_property("RColor", r.to_string());
    info.set_property("GColor", g.to_string());
    info.set_property("BColor", b.to_string());
    info.set_property("LCText", text.to_string());
    entity.children.push(info);
    entity.set_property("Coalitions", "[0, 1, 2]");
    entity
}

/// Mission Objective translators have no Name. The timer pulses them.
fn mission_objective(
    id: i32,
    x: f64,
    z: f64,
    side: ObjectiveSide,
    success: bool,
    lc_name: i32,
    lc_desc: i32,
) -> Il2Entity {
    let mut entity = Il2Entity::new("MCU_TR_MissionObjective");
    entity.index = Some(id);
    entity.set_property("Index", id.to_string());
    entity.set_targets(Vec::new());
    entity.set_objects(Vec::new());
    entity.set_property("XPos", format!("{x:.3}"));
    entity.set_property("YPos", "0.000");
    entity.set_property("ZPos", format!("{z:.3}"));
    entity.set_property("XOri", "0");
    entity.set_property("YOri", "0");
    entity.set_property("ZOri", "0");
    entity.set_property("Enabled", "1");
    entity.set_property("LCName", lc_name.to_string());
    entity.set_property("LCDesc", lc_desc.to_string());
    entity.set_property("TaskType", "0");
    entity.set_property("Coalition", side.coalition().to_string());
    entity.set_property("Success", if success { "1" } else { "0" });
    entity.set_property("IconType", "0");
    entity
}

fn translator_entity(name: &str, id: i32, x: f64, z: f64, object: i32) -> Il2Entity {
    let mut entity = mcu("MCU_TR_Entity", name, id, x, z);
    entity.set_property("YPos", "0.200");
    entity.set_property("Enabled", "1");
    entity.set_property("MisObjID", object.to_string());
    entity
}

fn mcu(block: &str, name: &str, id: i32, x: f64, z: f64) -> Il2Entity {
    let mut entity = Il2Entity::new(block);
    entity.index = Some(id);
    entity.set_property("Index", id.to_string());
    entity.set_name(name);
    entity.set_property("Desc", "\"\"");
    entity.set_targets(Vec::new());
    entity.set_objects(Vec::new());
    entity.set_property("XPos", format!("{x:.3}"));
    entity.set_property("YPos", "0.000");
    entity.set_property("ZPos", format!("{z:.3}"));
    entity.set_property("XOri", "0");
    entity.set_property("YOri", "0");
    entity.set_property("ZOri", "0");
    entity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle(id: i32, x: f64, z: f64, country: &str) -> Il2Entity {
        let mut entity = Il2Entity::new("Vehicle");
        entity.index = Some(id);
        entity.set_property("Index", id.to_string());
        entity.set_property("XPos", format!("{x:.3}"));
        entity.set_property("ZPos", format!("{z:.3}"));
        entity.set_property("Country", country);
        entity
    }

    fn block(id: i32, x: f64, z: f64, country: &str) -> Il2Entity {
        let mut entity = Il2Entity::new("Block");
        entity.index = Some(id);
        entity.set_property("Index", id.to_string());
        entity.set_name("Objective Block");
        entity.set_property("XPos", format!("{x:.3}"));
        entity.set_property("ZPos", format!("{z:.3}"));
        entity.set_property("Country", country);
        entity
    }

    fn group(name: &str, id: i32, children: Vec<Il2Entity>) -> Il2Entity {
        let mut group = Il2Entity::new("Group");
        group.index = Some(id);
        group.set_name(name);
        group.children = children;
        group
    }

    #[test]
    fn neutral_units_become_601_and_end_the_mission() {
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.children = vec![group(
            "Eastern Tanks",
            2,
            vec![
                vehicle(3, 1000.0, 2000.0, "0"),
                vehicle(4, 1100.0, 2000.0, "501"),
            ],
        )];
        let plan = ObjectivePlan {
            groups: vec![ObjectiveGroup {
                name: "Eastern Tanks".into(),
                score_for: ObjectiveSide::Nato,
            }],
            clusters: Vec::new(),
            nato_pct: 50,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 0,
            flights: Vec::new(),
        };
        let mut locale = LocaleTable::default();
        let notes = apply_objectives(&mut root, &mut locale, &plan, 10);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(
            find_by_index(&root, 3).unwrap().property("Country"),
            Some("601")
        );
        assert_eq!(
            find_by_index(&root, 4).unwrap().property("Country"),
            Some("501")
        );
        assert!(
            find_by_index(&root, 3)
                .unwrap()
                .children
                .iter()
                .all(|child| child.block_type != "OnEvents"),
            "OnKilled stays on the entity, not the vehicle"
        );
        let entity = root
            .children
            .iter()
            .flat_map(|child| child.children.iter())
            .find(|child| child.block_type == "MCU_TR_Entity")
            .unwrap();
        let counter_id: i32 = entity.children[0].children[0]
            .property("TarId")
            .unwrap()
            .parse()
            .unwrap();
        let counter = find_by_index(&root, counter_id).unwrap();
        assert_eq!(counter.block_type, "MCU_Counter");
        assert_eq!(counter.property("Counter"), Some("1"));
        assert_eq!(counter.property("Dropcount"), Some("0"));
        assert_eq!(counter.name(), Some("50% required"));
        let delay = find_by_index(&root, counter.targets[0]).unwrap();
        assert_eq!(delay.name(), Some("delay timer"));
        assert_eq!(delay.property("Time"), Some("5"));
        let success = delay
            .targets
            .iter()
            .find_map(|id| {
                let node = find_by_index(&root, *id)?;
                (node.block_type == "MCU_TR_MissionObjective" && node.property("Success") == Some("1"))
                    .then_some(node)
            })
            .expect("delay pulses the success objective");
        assert_eq!(success.property("Coalition"), Some("2"));
        assert_eq!(success.property("Enabled"), Some("1"));
        assert!(success.property("Name").is_none());
        let end = delay
            .targets
            .iter()
            .find_map(|id| {
                let timer = find_by_index(&root, *id)?;
                (timer.name() == Some("Next Mission")).then_some(timer)
            })
            .unwrap();
        let mission_end = find_by_index(&root, end.targets[0]).unwrap();
        assert_eq!(mission_end.block_type, "MCU_TR_MissionEnd");
        assert!(locale.get(10).unwrap().contains("NATO"));
    }

    #[test]
    fn a_click_watches_every_reference_block_within_500_m() {
        let mut bridge = group(
            "Bridges",
            2,
            vec![
                block(3, 1000.0, 2000.0, "0"),
                block(4, 1200.0, 2100.0, "0"),
                block(5, 4000.0, 2000.0, "0"),
            ],
        );
        bridge.index = Some(2);
        let mut refs = group("Reference groups", 9, vec![bridge]);
        refs.index = Some(9);
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.children = vec![refs];
        let plan = ObjectivePlan {
            groups: Vec::new(),
            clusters: vec![ObjectiveCluster {
                x: 1100.0,
                z: 2050.0,
                score_for: ObjectiveSide::Dprk,
                positions: vec![(1000.0, 2000.0), (1200.0, 2100.0)],
            }],
            nato_pct: 33,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 0,
            flights: Vec::new(),
        };
        let notes = apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 20);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(
            find_by_index(&root, 3).unwrap().property("Country"),
            Some("601")
        );
        assert!(
            find_by_index(&root, 5)
                .unwrap()
                .property("LinkTrId")
                .is_none(),
            "the block 3 km away is not watched"
        );
        let recalled = recall_objectives(&root).unwrap();
        assert_eq!(recalled.dprk_pct, 33);
        assert_eq!(recalled.dprk_picks.len(), 2);
        assert!(recalled.dprk_picks.iter().all(|pick| pick.positions.len() == 1));
        assert!(recalled.groups.is_empty());
        let ids = recalled.counter_ids.clone();
        strip_counter_events(&mut root, &ids);
        root.for_each(&mut |entity| {
            if entity.block_type == "MCU_TR_Entity" {
                assert!(
                    entity
                        .children
                        .iter()
                        .all(|child| child.block_type != "OnEvents")
                );
            }
        });
    }

    #[test]
    fn an_airfield_block_inside_the_area_is_watched() {
        let mut field = group(
            "K-14",
            4,
            vec![
                block(5, 1000.0, 2000.0, "601"),
                block(6, 8000.0, 2000.0, "601"),
            ],
        );
        field.index = Some(4);
        let mut airfield = Il2Entity::new("Airfield");
        airfield.index = Some(7);
        airfield.set_property("Index", "7");
        airfield.set_property("Name", "\"K-14\"");
        airfield.set_property("XPos", "1000.000");
        airfield.set_property("ZPos", "2000.000");
        airfield.set_property("Country", "601");
        field.children.push(airfield);
        let nato = group("NATO", 3, vec![field]);
        let airfields = group("Airfields", 2, vec![nato]);
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.set_name("Korea Base Map");
        root.children = vec![airfields];
        let plan = ObjectivePlan {
            groups: Vec::new(),
            clusters: vec![ObjectiveCluster {
                x: 1000.0,
                z: 2000.0,
                score_for: ObjectiveSide::Dprk,
                positions: vec![(1000.0, 2000.0)],
            }],
            nato_pct: 33,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 0,
            flights: Vec::new(),
        };
        let notes = apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 20);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(find_by_index(&root, 5).unwrap().property("LinkTrId").is_some());
        assert!(find_by_index(&root, 7).unwrap().property("LinkTrId").is_some());
        assert!(
            find_by_index(&root, 6).unwrap().property("LinkTrId").is_none(),
            "a block outside the circle is not watched"
        );
        let recalled = recall_objectives(&root).unwrap();
        assert_eq!(recalled.dprk_picks.len(), 2);
        assert!(recalled.groups.iter().all(|name| name != "Airfields"));
    }

    #[test]
    fn an_empty_plan_adds_nothing() {
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        let before = root.children.len();
        let notes = apply_objectives(
            &mut root,
            &mut LocaleTable::default(),
            &ObjectivePlan {
                groups: Vec::new(),
                clusters: Vec::new(),
                nato_pct: 33,
                dprk_pct: 33,
                nato_aircraft: 0,
                dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 0,
            flights: Vec::new(),
            },
            2,
        );
        assert!(notes.is_empty());
        assert_eq!(root.children.len(), before);
    }

    #[test]
    fn zero_aircraft_leaves_fighters_out() {
        let mut pack = group("Eastern Fighters", 2, Vec::new());
        let mut gates = group("NodeGates", 8, Vec::new());
        gates.index = Some(8);
        let mut flight = group("Group 1", 9, vec![plane(3, "501"), plane(4, "501")]);
        flight.index = Some(9);
        pack.children = vec![gates, flight];
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.set_name("Korea Base Map");
        root.children = vec![pack];
        let mut plan = ObjectivePlan {
            groups: Vec::new(),
            clusters: Vec::new(),
            nato_pct: 33,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 0,
            flights: Vec::new(),
        };
        let notes = apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 10);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(root.children.len(), 1);
        plan.nato_aircraft = 2;
        let notes = apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 10);
        assert!(notes.is_empty(), "{notes:?}");
        let recalled = recall_objectives(&root).unwrap();
        assert_eq!(recalled.nato_aircraft, 2);
        assert_eq!(recalled.dprk_aircraft, 0);
        assert!(recalled.groups.is_empty());
        let counter = root
            .find_by_name("Objectives")
            .unwrap()
            .children
            .iter()
            .find(|child| child.block_type == "MCU_Counter")
            .unwrap();
        assert_eq!(counter.name(), Some("Aircraft 2"));
        assert_eq!(counter.property("Counter"), Some("2"));
    }

    fn plane(id: i32, country: &str) -> Il2Entity {
        let mut entity = Il2Entity::new("Plane");
        entity.index = Some(id);
        entity.set_property("Index", id.to_string());
        entity.set_property("XPos", format!("{:.3}", 1000.0 + id as f64));
        entity.set_property("ZPos", "2000.000");
        entity.set_property("Country", country);
        entity
    }

    #[test]
    fn a_bomber_formation_counts_toward_the_other_side() {
        let mut bombers = group("B-29", 2, vec![plane(3, "601"), plane(4, "601"), plane(5, "601")]);
        bombers.index = Some(2);
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.set_name("Korea Base Map");
        root.children = vec![bombers];
        let plan = ObjectivePlan {
            groups: Vec::new(),
            clusters: Vec::new(),
            nato_pct: 33,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 2,
            flights: vec![ObjectiveFlight {
                name: "B-29".into(),
                eastern: false,
            }],
        };
        let notes = apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 10);
        assert!(notes.is_empty(), "{notes:?}");
        let counter = root
            .find_by_name("Objectives")
            .unwrap()
            .children
            .iter()
            .find(|child| child.block_type == "MCU_Counter")
            .unwrap();
        assert_eq!(counter.name(), Some("Mission Aircraft 2"));
        assert_eq!(counter.property("Counter"), Some("2"));
        let success = root
            .find_by_name("Objectives")
            .unwrap()
            .children
            .iter()
            .find(|child| {
                child.block_type == "MCU_TR_MissionObjective"
                    && child.property("Success") == Some("1")
            })
            .unwrap();
        assert_eq!(success.property("Coalition"), Some("1"));
        let recalled = recall_objectives(&root).unwrap();
        assert_eq!(recalled.flights.len(), 1);
        assert_eq!(recalled.flights[0].name, "B-29");
        assert!(!recalled.flights[0].eastern);
        assert_eq!(recalled.dprk_mission, 2);
        assert_eq!(recalled.dprk_aircraft, 0);
    }

    #[test]
    fn a_mission_inside_air_tasking_recalls_its_own_name() {
        let mut bombers = group("B-29 2", 3, vec![plane(4, "601"), plane(5, "601")]);
        bombers.index = Some(3);
        let mut pack = group("Air Tasking", 2, vec![bombers]);
        pack.index = Some(2);
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.set_name("Korea Base Map");
        root.children = vec![pack];
        let plan = ObjectivePlan {
            groups: Vec::new(),
            clusters: Vec::new(),
            nato_pct: 40,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 2,
            flights: vec![ObjectiveFlight {
                name: "B-29".into(),
                eastern: false,
            }],
        };
        let notes = apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 10);
        assert!(notes.is_empty(), "{notes:?}");
        let recalled = recall_objectives(&root).unwrap();
        assert_eq!(recalled.nato_pct, 33);
        assert_eq!(recalled.dprk_mission, 2);
        assert_eq!(recalled.nato_aircraft, 0);
        assert!(
            recalled
                .flights
                .iter()
                .any(|flight| flight.name == "B-29 2" && !flight.eastern),
            "{:?}",
            recalled.flights
        );
        assert!(mission_names_match("B-29", "B-29 2"));
    }

    #[test]
    fn a_disabled_object_is_not_counted() {
        let mut off = vehicle(4, 1100.0, 2000.0, "501");
        off.set_property("Enabled", "0");
        let mut root = Il2Entity::new("Group");
        root.index = Some(1);
        root.children = vec![group(
            "Eastern Tanks",
            2,
            vec![vehicle(3, 1000.0, 2000.0, "501"), off],
        )];
        assert_eq!(active_scored_count(&root), 1);
        let plan = ObjectivePlan {
            groups: vec![ObjectiveGroup {
                name: "Eastern Tanks".into(),
                score_for: ObjectiveSide::Nato,
            }],
            clusters: Vec::new(),
            nato_pct: 100,
            dprk_pct: 33,
            nato_aircraft: 0,
            dprk_aircraft: 0,
            nato_mission: 0,
            dprk_mission: 0,
            flights: Vec::new(),
        };
        apply_objectives(&mut root, &mut LocaleTable::default(), &plan, 10);
        let counter = root
            .find_by_name("Objectives")
            .unwrap()
            .children
            .iter()
            .find(|child| child.block_type == "MCU_Counter")
            .unwrap();
        assert_eq!(counter.property("Counter"), Some("1"));
        let recalled = recall_objectives(&root).unwrap();
        assert_eq!(recalled.nato_pct, 100);
        assert_eq!(recalled.groups, vec!["Eastern Tanks".to_string()]);
    }

    #[test]
    fn thirty_three_percent_rounds_like_the_demo() {
        assert_eq!(destruction_count(2, 50), 1);
        assert_eq!(destruction_count(3, 66), 2);
        assert_eq!(destruction_count(3, 33), 1);
        assert_eq!(destruction_count(6, 33), 2);
        assert_eq!(destruction_count(1, 33), 1);
    }
}
