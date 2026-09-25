//! flights.rs — fighter flight configuration
//!
//! Rebuilds **Group 1** from Template Builder pair logic (Independent
//! seats, OnSpawned → AttackArea lead / Cover wing including leftover
//! and extra pair leads, events → that plane’s Mission Complete →
//! Force Complete / RTB / deactivate for that bird only, AI RTB off,
//! Spawn + repeat) then injects the fighter-pack randomizer, per-flight
//! spawners / DeathCounts, pack hook names, finger-four placement, and
//! GUI altitude / timer / identity settings. Zones stay
//! at the original pack sizes (16 km IN / 35 km OUT); AttackArea is
//! 30 km air / 600 s. NodeGates and `RTB - 1` come from the loaded
//! linked pack. Flight sizes cycle `max, max-1, …, 1`. `pack.rs` clones
//! the finished Group 1.
//!
//! ## Public API
//! * `struct FlightConfig` — GUI inputs: flight count (clamped 1–10),
//!   max per flight (clamped 1–8), aircraft `type_ids` + recommended
//!   `type_skills` (0–4), country, cooldown / delete-order seconds,
//!   altitude range. `reinforcement` is kept on the config but not written:
//!   the respawn timer is omitted so it cannot pulse SPAWN UNITS during
//!   cleanup. `Default`: 4 flights, max 4,
//!   mig15bis + la11 (skills 3/2), country 501, 180 s cooldown, 60 s delete,
//!   1000–5500 m.
//! * `fn configure_aircraft` — replace Group 1 on a linked-pack root.
//! * `fn flight_sizes` — size per flight (`max - (i % max)`; 4/4 → 4,3,2,1).
//!
//! ## Used by
//! * ui.rs (Fighter Pack) — writes settings into the loaded/builtin
//!   template before pack generation; `flight_sizes` drives the UI summary.
//! * ui.rs (Map) — flight packs placed on the map.

use crate::aircraft::{
    aircraft_by_id, callsign_for, encode_tcode, encode_tcode_color, flight_color,
    pair_skills, plane_display_name, AircraftType, AIRCRAFT_TYPES,
};
use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::template::{
    builtin_plane_catalog, finger_four_offset, generate_template, BringUp, CatalogUnit,
    EntityEvent, EventHook, EventThen, FlightRole, OrderKind, OrderSpec, PlaceLayout, PlaneStart,
    TemplateOptions, TemplateSeat, ZoneCoalition, AIR_ZONE_IN_M, AIR_ZONE_OUT_M, PLACEMENT_SPACING,
};

/// AttackArea radius from the original linked fighter pack (metres).
const PACK_ATTACK_AREA_M: f32 = 30_000.0;
const PACK_ATTACK_TIME_S: f32 = 600.0;

#[derive(Debug, Clone)]
pub struct FlightConfig {
    pub flight_count: u32,
    pub max_in_flight: u32,
    pub type_ids: Vec<String>,
    /// Recommended AILevel 0–4, parallel to `type_ids`.
    pub type_skills: Vec<i32>,
    pub country: i32,
    pub cooldown: f32,
    /// Kept for saved settings. Not written: the reinforcement MCU is omitted.
    #[allow(dead_code)]
    pub reinforcement: f32,
    pub delete_orders: f32,
    pub altitude_min: f32,
    pub altitude_max: f32,
}

impl Default for FlightConfig {
    fn default() -> Self {
        Self {
            flight_count: 4,
            max_in_flight: 4,
            type_ids: vec!["mig15bis".into(), "la11".into()],
            type_skills: vec![3, 2],
            country: 501,
            cooldown: 180.0,
            reinforcement: 300.0,
            delete_orders: 60.0,
            altitude_min: 1000.0,
            altitude_max: 5500.0,
        }
    }
}

/// Seconds between each randomizer waterfall timer. 100ms is too tight for IL-2.
const WATERFALL_STEP_S: f64 = 0.5;
/// Second pair of each complete 4-ship sits this far above the low-cover pair.
const HIGH_COVER_OFFSET_M: f64 = 2000.0;
const PAIR_STACK_MIN_M: f64 = 25.0;
const PAIR_STACK_MAX_M: f64 = 50.0;
/// Canonical low-cover band; scales with the GUI max altitude (ref 5500 m).
const REF_ALT_MAX: f64 = 5500.0;
const REF_LOW_MIN: f64 = 500.0;
const REF_LOW_MAX: f64 = 1500.0;

const FIGHTER_EVENTS: &[EntityEvent] = &[
    EntityEvent::OnPlaneCriticalDamage,
    EntityEvent::OnPilotWounded,
    EntityEvent::OnPlaneBingoMainMG,
    EntityEvent::OnPlaneBingoFuel,
];

struct BuiltFlight {
    entity_ids: Vec<i32>,
    leads: Vec<i32>,
    wings: Vec<i32>,
}

struct SeatPlan {
    flight: usize,
    seat: usize,
    size: usize,
    ac: &'static AircraftType,
    ailevel: i32,
}

/// Replace **Group 1** with template pair logic plus pack randomizer / hooks.
/// Keeps **RTB - 1** and **NodeGates** from `root`.
pub fn configure_aircraft(root: &mut Il2Entity, cfg: &FlightConfig) -> Result<(), String> {
    let flight_count = cfg.flight_count.clamp(1, 10) as usize;
    let max_in_flight = cfg.max_in_flight.clamp(1, 8) as usize;
    let types = resolve_types(&cfg.type_ids, &cfg.type_skills)?;
    let sizes = flight_sizes(flight_count, max_in_flight);

    require_named(root, "Group", "Group 1")?;
    let rtb_proto = require_named(root, "MCU_Waypoint", "RTB - 1")?.clone();
    require_named(root, "Group", "NodeGates")?;

    let catalog = builtin_plane_catalog();
    let (seats, plans) = build_seats(&types, &sizes, cfg, &catalog)?;
    let opts = TemplateOptions {
        name: "Group 1".into(),
        zone_in: AIR_ZONE_IN_M,
        zone_out: AIR_ZONE_OUT_M,
        seats,
        place_layout: PlaceLayout::InvertedVee,
        per_group: 4,
        bring_up: BringUp::Spawn,
        allow_multiple_spawns: true,
        spawn_cooldown_min: (cfg.cooldown / 60.0).max(0.0),
        zone_coalition: zone_coalition_for_country(cfg.country),
        ..TemplateOptions::default()
    };

    let mut group1 = generate_template(&opts)?;
    install_pack_logic(&mut group1, &sizes, cfg, &rtb_proto)?;
    apply_identities(&mut group1, &plans, cfg)?;

    let mut next_id = root.max_index().saturating_add(1);
    let (mut group1, _) = duplicate_template(&group1, &mut next_id);
    let entity_ids = unit_entity_ids(&group1);

    let mut rtb = None;
    let mut gates = None;
    for child in root.children.drain(..) {
        match (child.block_type.as_str(), child.name()) {
            ("MCU_Waypoint", Some("RTB - 1")) => rtb = Some(child),
            ("Group", Some("NodeGates")) => gates = Some(child),
            _ => {}
        }
    }
    let mut rtb = rtb.ok_or("builtin template has no RTB - 1")?;
    let gates = gates.ok_or("template has no NodeGates group — load a linked fighter pack")?;
    rtb.set_objects(entity_ids);
    wire_zones_to_gates(&mut group1, &gates);
    root.children.push(rtb);
    root.children.push(group1);
    root.children.push(gates);
    Ok(())
}

fn require_named<'a>(
    root: &'a Il2Entity,
    block: &str,
    name: &str,
) -> Result<&'a Il2Entity, String> {
    root.children
        .iter()
        .find(|c| c.block_type == block && c.name() == Some(name))
        .ok_or_else(|| format!("template has no {name}"))
}

fn zone_coalition_for_country(country: i32) -> ZoneCoalition {
    if country / 100 == 6 {
        ZoneCoalition::Eastern
    } else {
        ZoneCoalition::Western
    }
}

fn catalog_unit<'a>(catalog: &'a [CatalogUnit], ac: &AircraftType) -> Result<CatalogUnit, String> {
    catalog
        .iter()
        .find(|u| u.script.eq_ignore_ascii_case(ac.script))
        .cloned()
        .ok_or_else(|| format!("no catalog unit for `{}`", ac.id))
}

fn build_seats(
    types: &[(&'static AircraftType, i32)],
    sizes: &[usize],
    cfg: &FlightConfig,
    catalog: &[CatalogUnit],
) -> Result<(Vec<TemplateSeat>, Vec<SeatPlan>), String> {
    let alt_min = cfg.altitude_min.min(cfg.altitude_max) as f64;
    let alt_max = cfg.altitude_max.max(cfg.altitude_min) as f64;
    let flight_count = sizes.len();
    let mut seats = Vec::new();
    let mut plans = Vec::new();
    let mut global = 0usize;
    for (f, &size) in sizes.iter().enumerate() {
        let (ac, recommended) = types[f % types.len()];
        let unit = catalog_unit(catalog, ac)?;
        for seat in 0..size {
            let pair = seat / 2;
            let (lead_skill, wing_skill) = pair_skills(recommended, f, pair);
            let ailevel = if seat % 2 == 0 {
                lead_skill
            } else {
                wing_skill
            };
            let mut tpl = TemplateSeat::new(unit.clone());
            tpl.role = FlightRole::Independent;
            tpl.country = cfg.country;
            tpl.skill = ailevel;
            tpl.altitude = plane_altitude(alt_min, alt_max, f, flight_count, seat, size) as f32;
            tpl.ai_rtb = false;
            tpl.start_type = PlaneStart::Air.as_i32();
            tpl.orders = fighter_orders(seat, global);
            tpl.events = fighter_events(tpl.orders.len().saturating_sub(1));
            seats.push(tpl);
            plans.push(SeatPlan {
                flight: f,
                seat,
                size,
                ac,
                ailevel,
            });
        }
        global += size;
    }
    Ok((seats, plans))
}

fn fighter_orders(seat: usize, global: usize) -> Vec<OrderSpec> {
    let mut orders = vec![on_spawned_order()];
    if seat % 2 == 1 {
        let mut cover = OrderSpec::default();
        cover.kind = OrderKind::Cover;
        cover.delay_s = 0.5;
        cover.priority = 1;
        cover.cover_lead = Some(global + seat - 1);
        orders.push(cover);
    } else {
        let mut area = OrderSpec::default();
        area.kind = OrderKind::AttackArea;
        area.delay_s = 0.5;
        area.attack_area = PACK_ATTACK_AREA_M;
        area.attack_air = true;
        area.attack_ground = false;
        area.attack_g_targets = false;
        area.time_s = PACK_ATTACK_TIME_S;
        area.priority = 1;
        orders.push(area);
    }
    let mut done = OrderSpec::default();
    done.kind = OrderKind::MissionComplete;
    done.delay_s = 0.5;
    orders.push(done);
    orders
}

fn on_spawned_order() -> OrderSpec {
    let mut spec = OrderSpec::default();
    spec.kind = OrderKind::OnSpawned;
    spec.delay_s = 0.5;
    spec
}

fn fighter_events(mission_complete_index: usize) -> Vec<EventHook> {
    FIGHTER_EVENTS
        .iter()
        .map(|&kind| EventHook {
            kind,
            then: EventThen::Order(mission_complete_index),
        })
        .collect()
}

fn resolve_types(
    ids: &[String],
    skills: &[i32],
) -> Result<Vec<(&'static AircraftType, i32)>, String> {
    let mut out = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let ac = aircraft_by_id(id).ok_or_else(|| format!("unknown aircraft type `{id}`"))?;
        let skill = skills.get(i).copied().unwrap_or(2).clamp(0, 4);
        out.push((ac, skill));
    }
    if out.is_empty() {
        let ac = aircraft_by_id(AIRCRAFT_TYPES[0].id).ok_or("missing default aircraft type")?;
        out.push((ac, 2));
    }
    Ok(out)
}

/// One flight as the Fighter Pack preview shows it.
pub struct FlightPreview {
    pub type_label: &'static str,
    /// Per seat: (altitude in metres, true for an AttackArea lead, false for a Cover wing).
    pub seats: Vec<(f64, bool)>,
}

/// What `configure_aircraft` will build for Group 1, without building it:
/// the same type cycling, seat roles and altitudes as `build_seats`.
pub fn preview_flights(cfg: &FlightConfig) -> Vec<FlightPreview> {
    let flight_count = cfg.flight_count.clamp(1, 10) as usize;
    let max_in_flight = cfg.max_in_flight.clamp(1, 8) as usize;
    let Ok(types) = resolve_types(&cfg.type_ids, &cfg.type_skills) else {
        return Vec::new();
    };
    let alt_min = cfg.altitude_min.min(cfg.altitude_max) as f64;
    let alt_max = cfg.altitude_max.max(cfg.altitude_min) as f64;
    let sizes = flight_sizes(flight_count, max_in_flight);
    sizes
        .iter()
        .enumerate()
        .map(|(f, &size)| FlightPreview {
            type_label: types[f % types.len()].0.label,
            seats: (0..size)
                .map(|seat| {
                    let alt = plane_altitude(alt_min, alt_max, f, sizes.len(), seat, size);
                    (alt, seat % 2 == 0)
                })
                .collect(),
        })
        .collect()
}

/// `max` is a ceiling. Flights cycle `max, max-1, …, 1` so a setting of 4
/// produces 4-, 3-, 2-, and 1-ship elements rather than four 4-ships.
pub fn flight_sizes(count: usize, max: usize) -> Vec<usize> {
    let max = max.max(1);
    (0..count).map(|i| max - (i % max)).collect()
}

fn install_pack_logic(
    group: &mut Il2Entity,
    sizes: &[usize],
    cfg: &FlightConfig,
    rtb_proto: &Il2Entity,
) -> Result<(), String> {
    if let Some(zone) = group.find_by_name_mut("Zone Out") {
        zone.set_name("Zone OUT");
    }
    if let Some(delayed) = group.find_by_name_mut("DELAYED END ORDERS") {
        delayed.set_name("Delay Delete");
        delayed.set_property("Time", format_time(cfg.delete_orders));
    }
    if let Some(cool) = group.find_by_name_mut("COOLDOWN") {
        cool.set_property("Time", format_time(cfg.cooldown));
    }

    rename_covers(group);

    let proto_timer = named_clone(group, "ENABLE / PULSE IN")?;
    let proto_deact = first_block(group, "MCU_Deactivate")
        .cloned()
        .ok_or("missing deactivate prototype")?;
    let proto_act = first_block(group, "MCU_Activate")
        .cloned()
        .ok_or("missing activate prototype")?;
    let proto_spawn = named_clone(group, "Trigger Spawner")?;
    let proto_death = named_clone(group, "DeathCount")?;
    let proto_force = named_clone(group, "Force Complete - High")?;
    let proto_deact_units = named_clone(group, "Deactivate Units")
        .ok()
        .unwrap_or_else(|| proto_deact.clone());
    let old_spawn_id = proto_spawn.index.ok_or("Trigger Spawner missing Index")?;
    let old_death_id = proto_death.index.ok_or("DeathCount missing Index")?;
    let mission_end_id = group
        .find_by_name("MISSION END")
        .and_then(|e| e.index)
        .ok_or("missing MISSION END")?;
    let zone_in_id = group
        .find_by_name("Zone IN")
        .and_then(|e| e.index)
        .ok_or("missing Zone IN")?;

    let entity_ids = unit_entity_ids(group);
    let flights = split_flights(&entity_ids, sizes)?;
    let mut next_id = group.max_index().saturating_add(1);

    let mut spawners = Vec::new();
    let mut death_counts = Vec::new();
    for (f, flight) in flights.iter().enumerate() {
        let mut spawner = clone_mcu(&proto_spawn, &mut next_id, &format!("Spawn {}", f + 1));
        spawner.set_objects(flight.entity_ids.clone());
        let mut death = clone_mcu(&proto_death, &mut next_id, "DeathCount");
        death.set_property("Counter", flight.entity_ids.len().to_string());
        death.set_property("Dropcount", "1");
        spawners.push(spawner);
        death_counts.push(death);
    }

    let randomizer_nodes = build_randomizer(&proto_timer, &proto_deact, &proto_act, &spawners, &mut next_id)?;
    let input_id = randomizer_nodes
        .iter()
        .find(|n| n.name() == Some("Randomizer:INPUT"))
        .and_then(|n| n.index)
        .ok_or("randomizer input missing Index")?;
    let death_ids: Vec<i32> = death_counts.iter().filter_map(|d| d.index).collect();
    for death in &mut death_counts {
        if let Some(cool) = group.find_by_name("COOLDOWN").and_then(|e| e.index) {
            death.set_targets(vec![cool]);
        }
    }

    patch_plane_links(group, &flights, &spawners, &death_ids, old_spawn_id, old_death_id)?;
    install_plane_ends(
        group,
        &entity_ids,
        sizes,
        &death_ids,
        &proto_force,
        &proto_timer,
        &proto_deact_units,
        rtb_proto,
        mission_end_id,
        cfg.delete_orders,
        &mut next_id,
    )?;

    let mut enable = clone_mcu(&proto_act, &mut next_id, "Enable Spawner");
    enable.set_targets(vec![zone_in_id]);
    enable.set_objects(Vec::new());
    let mut disable = clone_mcu(&proto_deact, &mut next_id, "Disable Spawner");
    disable.set_targets(vec![zone_in_id]);
    disable.set_objects(Vec::new());
    let mut delete_orders = clone_mcu(&proto_timer, &mut next_id, "Delete Orders");
    delete_orders.set_property("Time", "0.02");
    delete_orders.set_property("Random", "100");
    delete_orders.set_targets(vec![mission_end_id]);

    // Reinforcement timer is omitted. It pulsed SPAWN UNITS again on a 33%
    // roll, which could start another flight while cleanup was running.
    if let Some(spawn_units) = group.find_by_name_mut("SPAWN UNITS") {
        spawn_units.set_targets(vec![input_id]);
    }
    if let Some(cool) = group.find_by_name_mut("COOLDOWN") {
        cool.set_targets(vec![input_id]);
    }
    if let Some(on) = group.find_by_name_mut("DeathCount ReActivate") {
        on.set_targets(death_ids.clone());
    }
    if let Some(off) = group.find_by_name_mut("DeathCount Deactivate") {
        off.set_targets(death_ids.clone());
    }
    if let Some(md) = group.find_by_name_mut("Modifier Set Value") {
        md.set_targets(death_ids);
    }

    if let Some(logic) = group.find_by_name_mut("Logic") {
        logic.children.retain(|c| {
            !matches!(
                c.name(),
                Some("Trigger Spawner") | Some("SpawnCount") | Some("DeathCount")
            )
        });
        logic.children.extend(death_counts);
        logic.children.push(enable);
        logic.children.push(disable);
        logic.children.push(delete_orders);
    }

    let mut randomizer = Il2Entity::new("Group");
    let rid = next_id;
    next_id += 1;
    randomizer.index = Some(rid);
    randomizer.set_property("Index", rid.to_string());
    randomizer.set_name("Randomizer");
    randomizer.set_property("Desc", "\"\"");
    randomizer.children = randomizer_nodes;
    group.children.push(randomizer);
    let _ = next_id;
    layout_pack_mcus(group, sizes.iter().sum());
    Ok(())
}

const MCU_GAP: f64 = 150.0;
const BRANCH_GAP: f64 = 300.0;

fn left_xz(origin: (f64, f64), branch: usize, row: i32) -> (f64, f64) {
    (
        origin.0 - MCU_GAP * f64::from(row),
        origin.1 - MCU_GAP - BRANCH_GAP * branch as f64,
    )
}

fn right_xz(origin: (f64, f64), branch: usize, row: i32) -> (f64, f64) {
    (
        origin.0 - MCU_GAP * f64::from(row),
        origin.1 + MCU_GAP + BRANCH_GAP * branch as f64,
    )
}

fn right_order_xz(origin: (f64, f64), branch: usize, row: i32) -> (f64, f64) {
    let (x, z) = right_xz(origin, branch, row);
    (x, z + MCU_GAP)
}

fn set_xz(entity: &mut Il2Entity, xz: (f64, f64)) {
    entity.set_property("XPos", format!("{:.3}", xz.0));
    entity.set_property("ZPos", format!("{:.3}", xz.1));
}

/// Spawn / orders stay on the left (north = spawn). Zone logic stays on the
/// origin. Cleanup is a right-hand stack: CLEANUP at the top, deactivate
/// and delete at the bottom. One column per plane.
fn layout_pack_mcus(group: &mut Il2Entity, plane_count: usize) {
    let origin = group
        .find_by_name("Zone IN")
        .and_then(|z| z.pos_xz())
        .unwrap_or((40_000.0, 40_000.0));

    if let Some(e) = group.find_by_name_mut("Enable Spawner") {
        set_xz(e, (origin.0 + MCU_GAP, origin.1));
    }
    if let Some(e) = group.find_by_name_mut("Disable Spawner") {
        set_xz(e, (origin.0 + MCU_GAP, origin.1 + MCU_GAP));
    }
    if let Some(e) = group.find_by_name_mut("Delete Orders") {
        set_xz(e, right_xz(origin, 0, -1));
    }

    if let Some(logic) = group.find_by_name_mut("Logic") {
        let mut death_i = 0usize;
        for child in &mut logic.children {
            if child.name() == Some("DeathCount") {
                death_i += 1;
                set_xz(child, (origin.0 - MCU_GAP * death_i as f64, origin.1));
            }
        }
    }

    if let Some(randomizer) = group.find_by_name_mut("Randomizer") {
        let mut spawn_i = 0usize;
        let mut random_i = 0usize;
        let mut out_i = 0usize;
        let mut closer_i = 0usize;
        for child in &mut randomizer.children {
            let name = child.name().unwrap_or("").to_string();
            if name == "Randomizer:INPUT" {
                set_xz(child, left_xz(origin, 0, -2));
            } else if name.starts_with("Wait for Output") {
                set_xz(child, left_xz(origin, 1, -2));
            } else if name == "ReOpen Outputs" {
                set_xz(child, left_xz(origin, 2, -2));
            } else if name == "CloseInput" {
                set_xz(child, left_xz(origin, 3, -2));
            } else if name.starts_with("Spawn ") {
                let (x, z) = left_xz(origin, spawn_i, -1);
                set_xz(child, (x, z - MCU_GAP));
                spawn_i += 1;
            } else if name.starts_with("Random ") {
                set_xz(child, left_xz(origin, random_i, -3));
                random_i += 1;
            } else if name.starts_with("Out ") {
                set_xz(child, left_xz(origin, out_i, -1));
                out_i += 1;
            } else if name.starts_with("Close_Remaining") {
                closer_i += 1;
                set_xz(child, left_xz(origin, closer_i, -3));
            }
        }
    }

    for i in 0..plane_count {
        let n = i + 1;
        let branch = i + 1;
        if let Some(e) = group.find_by_name_mut(&format!("CLEANUP {n}")) {
            set_xz(e, right_xz(origin, branch, 0));
        }
        if let Some(e) = group.find_by_name_mut(&format!("Force Complete {n}")) {
            set_xz(e, right_order_xz(origin, branch, 1));
        }
        if let Some(e) = group.find_by_name_mut(&format!("RTB DELAY {n}")) {
            set_xz(e, right_xz(origin, branch, 2));
        }
        if let Some(e) = group.find_by_name_mut(&format!("Delay Delete {n}")) {
            set_xz(e, right_xz(origin, branch, 3));
        }
        if let Some(e) = group.find_by_name_mut(&format!("Deactivate {n}")) {
            set_xz(e, right_order_xz(origin, branch, 4));
        }
    }
}

fn named_clone(group: &Il2Entity, name: &str) -> Result<Il2Entity, String> {
    group
        .find_by_name(name)
        .cloned()
        .ok_or_else(|| format!("missing `{name}` prototype"))
}

fn first_block<'a>(group: &'a Il2Entity, block: &str) -> Option<&'a Il2Entity> {
    if group.block_type == block {
        return Some(group);
    }
    group.children.iter().find_map(|c| first_block(c, block))
}

fn unit_entity_ids(group: &Il2Entity) -> Vec<i32> {
    let Some(units) = group.find_by_name("Units") else {
        return Vec::new();
    };
    units
        .children
        .iter()
        .filter(|c| c.block_type == "MCU_TR_Entity")
        .filter_map(|c| c.index)
        .collect()
}

fn split_flights(entity_ids: &[i32], sizes: &[usize]) -> Result<Vec<BuiltFlight>, String> {
    let need: usize = sizes.iter().sum();
    if entity_ids.len() != need {
        return Err(format!(
            "template placed {} units, expected {need}",
            entity_ids.len()
        ));
    }
    let mut out = Vec::with_capacity(sizes.len());
    let mut i = 0;
    for &size in sizes {
        let slice = &entity_ids[i..i + size];
        let mut built = BuiltFlight {
            entity_ids: slice.to_vec(),
            leads: Vec::new(),
            wings: Vec::new(),
        };
        for (seat, &eid) in slice.iter().enumerate() {
            if seat % 2 == 0 {
                built.leads.push(eid);
            } else {
                built.wings.push(eid);
            }
        }
        out.push(built);
        i += size;
    }
    Ok(out)
}

fn clone_mcu(proto: &Il2Entity, next_id: &mut i32, name: &str) -> Il2Entity {
    let (mut cloned, _) = duplicate_template(proto, next_id);
    cloned.set_name(name);
    cloned
}

fn apply_plane_identity(
    plane: &mut Il2Entity,
    country: i32,
    ac: &AircraftType,
    flight: usize,
    seat: usize,
    ailevel: i32,
) {
    let number = crate::aircraft::flight_number(flight, seat);
    let color = flight_color(flight);
    plane.set_name(&plane_display_name(flight, seat));
    plane.set_property("Script", format!("\"{}\"", ac.script));
    plane.set_property("Model", format!("\"{}\"", ac.model));
    plane.set_property("Country", country.to_string());
    plane.set_property("Skin", "\"\"");
    plane.set_property("BotSkin", "\"\"");
    plane.set_property("Callsign", callsign_for(country, ac.id).to_string());
    plane.set_property("Callnum", "0");
    plane.set_property("AILevel", ailevel.to_string());
    plane.set_property("TCode", format!("\"{}\"", encode_tcode(number)));
    plane.set_property("TCodeColor", format!("\"{}\"", encode_tcode_color(color, number)));
    plane.set_property("VictoryCount", "0");
    plane.set_property("Emblem", "0");
    plane.set_existing_property("AiRTBDecision", "0");
}

fn flight_place_xz(origin: (f64, f64), flight: usize, seat: usize) -> (f64, f64) {
    let spacing = f64::from(PLACEMENT_SPACING);
    let (dx, dz) = finger_four_offset(seat, spacing);
    (
        origin.0 - (flight as f64) * spacing * 3.5 + dx,
        origin.1 + (flight as f64) * spacing * 2.5 + dz,
    )
}

fn apply_identities(group: &mut Il2Entity, plans: &[SeatPlan], cfg: &FlightConfig) -> Result<(), String> {
    let origin = group
        .find_by_name("Zone IN")
        .and_then(|z| z.pos_xz())
        .unwrap_or((40_000.0, 40_000.0));
    let alt_min = cfg.altitude_min.min(cfg.altitude_max) as f64;
    let alt_max = cfg.altitude_max.max(cfg.altitude_min) as f64;
    let flight_count = plans.last().map(|p| p.flight + 1).unwrap_or(0);
    let units = group
        .find_by_name_mut("Units")
        .ok_or("generated Group 1 has no Units")?;
    let mut plane_i = 0usize;
    let mut i = 0;
    while i + 1 < units.children.len() {
        if units.children[i].block_type != "Plane"
            || units.children[i + 1].block_type != "MCU_TR_Entity"
        {
            i += 1;
            continue;
        }
        let plan = plans
            .get(plane_i)
            .ok_or("more planes than seat plans")?;
        let y = plane_altitude(alt_min, alt_max, plan.flight, flight_count, plan.seat, plan.size);
        let xz = flight_place_xz(origin, plan.flight, plan.seat);
        apply_plane_identity(
            &mut units.children[i],
            cfg.country,
            plan.ac,
            plan.flight,
            plan.seat,
            plan.ailevel,
        );
        set_xz(&mut units.children[i], xz);
        set_xz(&mut units.children[i + 1], xz);
        units.children[i].set_ypos(y);
        units.children[i + 1].set_ypos(y + 0.2);
        plane_i += 1;
        i += 2;
    }
    if plane_i != plans.len() {
        return Err(format!(
            "applied identity to {plane_i} planes, expected {}",
            plans.len()
        ));
    }
    Ok(())
}

fn patch_plane_links(
    group: &mut Il2Entity,
    flights: &[BuiltFlight],
    spawners: &[Il2Entity],
    death_ids: &[i32],
    old_spawn_id: i32,
    old_death_id: i32,
) -> Result<(), String> {
    let Some(units) = group.find_by_name_mut("Units") else {
        return Ok(());
    };
    for (f, flight) in flights.iter().enumerate() {
        let death_id = *death_ids.get(f).ok_or("DeathCount missing Index")?;
        let spawner_id = spawners[f].index.ok_or("Spawner missing Index")?;
        for node in units.children.iter_mut() {
            if node.block_type != "MCU_TR_Entity" {
                continue;
            }
            let Some(id) = node.index else { continue };
            if !flight.entity_ids.contains(&id) {
                continue;
            }
            replace_nested_id(node, "OnEvent", "TarId", old_death_id, death_id);
            replace_nested_id(node, "OnReport", "CmdId", old_spawn_id, spawner_id);
        }
    }
    Ok(())
}

fn replace_nested_id(entity: &mut Il2Entity, child_type: &str, key: &str, old: i32, new: i32) {
    entity.for_each_mut(&mut |e| {
        if e.block_type != child_type {
            return;
        }
        if e.property(key).and_then(|s| s.parse::<i32>().ok()) == Some(old) {
            e.set_property(key, new.to_string());
        }
    });
}

fn install_plane_ends(
    group: &mut Il2Entity,
    entity_ids: &[i32],
    sizes: &[usize],
    death_ids: &[i32],
    proto_force: &Il2Entity,
    proto_timer: &Il2Entity,
    proto_deact: &Il2Entity,
    rtb_proto: &Il2Entity,
    mission_end_id: i32,
    delete_orders: f32,
    next_id: &mut i32,
) -> Result<(), String> {
    let mut logic_nodes = Vec::new();
    let mut waypoints = Vec::new();
    let mut end_ids = Vec::new();
    for (si, &eid) in entity_ids.iter().enumerate() {
        let n = si + 1;
        let death_id = flight_of_seat(sizes, si).and_then(|f| death_ids.get(f).copied());

        let mut force = clone_mcu(proto_force, next_id, &format!("Force Complete {n}"));
        force.set_objects(vec![eid]);
        let force_id = force.index.ok_or("plane Force Complete missing Index")?;

        let mut rtb = clone_mcu(rtb_proto, next_id, &format!("RTB Plane {n}"));
        rtb.set_objects(vec![eid]);
        rtb.set_targets(Vec::new());
        let rtb_id = rtb.index.ok_or("RTB Plane missing Index")?;

        let mut rtb_delay = clone_mcu(proto_timer, next_id, &format!("RTB DELAY {n}"));
        rtb_delay.set_property("Time", "0.5");
        rtb_delay.set_property("Random", "100");
        rtb_delay.set_targets(vec![rtb_id]);
        let rtb_delay_id = rtb_delay.index.ok_or("RTB DELAY missing Index")?;

        let mut deact = clone_mcu(proto_deact, next_id, &format!("Deactivate {n}"));
        deact.set_objects(vec![eid]);
        deact.set_targets(Vec::new());
        let deact_id = deact.index.ok_or("Deactivate missing Index")?;

        let mut delay_deact = clone_mcu(proto_timer, next_id, &format!("Delay Delete {n}"));
        delay_deact.set_property("Time", format_time(delete_orders));
        delay_deact.set_property("Random", "100");
        let mut delay_targets = vec![deact_id];
        if let Some(id) = death_id {
            delay_targets.push(id);
        }
        delay_deact.set_targets(delay_targets);
        let delay_deact_id = delay_deact.index.ok_or("Delay Delete missing Index")?;

        let mut end = clone_mcu(proto_timer, next_id, &format!("CLEANUP {n}"));
        end.set_property("Time", "0.1");
        end.set_property("Random", "100");
        end.set_targets(vec![force_id, rtb_delay_id, delay_deact_id]);
        let end_id = end.index.ok_or("CLEANUP missing Index")?;
        end_ids.push(end_id);

        logic_nodes.push(end);
        logic_nodes.push(force);
        logic_nodes.push(rtb_delay);
        logic_nodes.push(delay_deact);
        logic_nodes.push(deact);
        waypoints.push(rtb);
    }
    group.for_each_mut(&mut |e| {
        let Some(name) = e.name() else { return };
        let Some(rest) = name.strip_prefix("Mission Complete ") else {
            return;
        };
        let Ok(n) = rest.parse::<usize>() else { return };
        let si = n.saturating_sub(1);
        if let Some(&id) = end_ids.get(si) {
            e.replace_target_id(mission_end_id, id);
        }
    });
    if let Some(logic) = group.find_by_name_mut("Logic") {
        logic.children.extend(logic_nodes);
    }
    if let Some(wps) = group.find_by_name_mut("Waypoints") {
        wps.children.extend(waypoints);
    } else {
        let mut wps = Il2Entity::new("Group");
        let id = *next_id;
        *next_id += 1;
        wps.index = Some(id);
        wps.set_property("Index", id.to_string());
        wps.set_name("Waypoints");
        wps.set_property("Desc", "\"\"");
        wps.children = waypoints;
        group.children.push(wps);
    }
    Ok(())
}

fn flight_of_seat(sizes: &[usize], si: usize) -> Option<usize> {
    let mut n = 0;
    for (f, &sz) in sizes.iter().enumerate() {
        if si < n + sz {
            return Some(f);
        }
        n += sz;
    }
    None
}

fn rename_covers(group: &mut Il2Entity) {
    let mut n = 0usize;
    group.for_each_mut(&mut |e| {
        if e.block_type == "MCU_CMD_Cover" {
            n += 1;
            e.set_name(&format!("Cover Wing {n}"));
        }
    });
}

fn wire_zones_to_gates(group1: &mut Il2Entity, gates: &Il2Entity) {
    if let Some(id) = gates.find_by_name("1OUT - DISABLE").and_then(|e| e.index) {
        if let Some(z) = group1.find_by_name_mut("Zone IN") {
            z.append_target(id);
        }
    }
    if let Some(id) = gates.find_by_name("1OUT - ENABLE").and_then(|e| e.index) {
        if let Some(z) = group1.find_by_name_mut("Zone OUT") {
            z.append_target(id);
        }
    }
}

fn build_randomizer(
    proto_timer: &Il2Entity,
    proto_deact: &Il2Entity,
    proto_act: &Il2Entity,
    spawners: &[Il2Entity],
    next_id: &mut i32,
) -> Result<Vec<Il2Entity>, String> {
    let n = spawners.len();
    let mut input = clone_mcu(proto_timer, next_id, "Randomizer:INPUT");
    input.set_property("Time", "0");
    input.set_property("Random", "100");

    let wait_s = WATERFALL_STEP_S * (n + 1) as f64;
    let mut wait = clone_mcu(
        proto_timer,
        next_id,
        &format!("Wait for Output {}ms", (wait_s * 1000.0).round() as i32),
    );
    wait.set_property("Time", format_waterfall_time(wait_s));
    wait.set_property("Random", "100");

    let mut reopen = clone_mcu(proto_act, next_id, "ReOpen Outputs");
    let mut close_input = clone_mcu(proto_deact, next_id, "CloseInput");

    let mut randoms = Vec::new();
    let mut outs = Vec::new();
    let mut closers = Vec::new();
    for i in 0..n {
        let pct = random_pct(i, n);
        let t = WATERFALL_STEP_S * (i + 1) as f64;
        let ms = (t * 1000.0).round() as i32;
        let mut rnd = clone_mcu(
            proto_timer,
            next_id,
            &format!("Random {}:{}% {ms}ms", i + 1, pct),
        );
        rnd.set_property("Time", format_waterfall_time(t));
        rnd.set_property("Random", pct.to_string());
        randoms.push(rnd);

        let mut out = clone_mcu(proto_timer, next_id, &format!("Out {}", i + 1));
        out.set_property("Time", "0");
        out.set_property("Random", "100");
        outs.push(out);

        if i + 1 < n {
            closers.push(clone_mcu(
                proto_deact,
                next_id,
                "Close_Remaining_Output(s)",
            ));
        }
    }

    let wait_id = wait.index.unwrap();
    let reopen_id = reopen.index.unwrap();
    let close_input_id = close_input.index.unwrap();
    let out_ids: Vec<i32> = outs.iter().filter_map(|o| o.index).collect();
    let random_ids: Vec<i32> = randoms.iter().filter_map(|o| o.index).collect();
    let closer_ids: Vec<i32> = closers.iter().filter_map(|o| o.index).collect();
    let spawner_ids: Vec<i32> = spawners.iter().filter_map(|o| o.index).collect();
    let input_id = input.index.unwrap();

    let mut input_targets = vec![close_input_id, wait_id];
    input_targets.extend(random_ids.iter().copied());
    input.set_targets(input_targets);
    wait.set_targets(vec![reopen_id]);
    close_input.set_targets(vec![input_id]);

    let mut reopen_targets = out_ids.clone();
    reopen_targets.push(input_id);
    reopen.set_targets(reopen_targets);

    for i in 0..n {
        outs[i].set_targets(vec![spawner_ids[i]]);
        if i + 1 == n {
            randoms[i].set_targets(vec![out_ids[i]]);
        } else {
            randoms[i].set_targets(vec![out_ids[i], closer_ids[i]]);
            closers[i].set_targets(out_ids[i + 1..].to_vec());
        }
    }

    let mut children = vec![input, wait, reopen, close_input];
    children.extend(randoms);
    children.extend(outs);
    children.extend(closers);
    children.extend(spawners.iter().cloned());
    Ok(children)
}

/// Equal-probability waterfall: 4 choices → 25 / 33 / 50 / 100.
fn random_pct(index: usize, n: usize) -> i32 {
    let remaining = n.saturating_sub(index).max(1);
    ((100 + remaining / 2) / remaining).clamp(1, 100) as i32
}

fn format_time(v: f32) -> String {
    if (v.fract()).abs() < 0.001 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

fn lerp(min: f64, max: f64, i: usize, n: usize) -> f64 {
    if n <= 1 {
        (min + max) / 2.0
    } else {
        min + (max - min) * i as f64 / (n - 1) as f64
    }
}

fn format_waterfall_time(s: f64) -> String {
    if (s - s.round()).abs() < 1e-9 {
        format!("{:.0}", s)
    } else {
        format!("{:.1}", s)
    }
}

/// Low-cover window: canonical 500–1500 m, scaled so it rises with max altitude.
fn low_cover_band(alt_min: f64, alt_max: f64) -> (f64, f64) {
    let scale = (alt_max / REF_ALT_MAX).max(0.1);
    let lo = (REF_LOW_MIN * scale).max(alt_min);
    let hi = (REF_LOW_MAX * scale).max(lo);
    (lo, hi)
}

/// 25–50 m stack between lead and wingman, stable per flight/pair.
fn pair_stack_m(flight: usize, pair: usize) -> f64 {
    let span = (PAIR_STACK_MAX_M - PAIR_STACK_MIN_M) as usize;
    PAIR_STACK_MIN_M + ((flight * 31 + pair * 17) % (span + 1)) as f64
}

/// Second pair of each complete 4-ship is high cover (~2000 m). Incomplete
/// leftover pairs stay low (3-ship, 5-ship, 6-ship trailing pair).
fn pair_is_high(seat: usize, size: usize) -> bool {
    let cluster_start = (seat / 4) * 4;
    let cluster_len = size.saturating_sub(cluster_start).min(4);
    cluster_len == 4 && seat % 4 >= 2
}

/// 1- and 2-ships stay on the min–max spread. Larger flights split each
/// complete finger-four 2 down / 2 up and keep leftovers low.
fn plane_altitude(
    alt_min: f64,
    alt_max: f64,
    flight: usize,
    flight_count: usize,
    seat: usize,
    size: usize,
) -> f64 {
    let pair = seat / 2;
    let base = if size > 2 {
        let (lo, hi) = low_cover_band(alt_min, alt_max);
        let low = lerp(lo, hi, flight, flight_count);
        if pair_is_high(seat, size) {
            low + HIGH_COVER_OFFSET_M
        } else {
            low
        }
    } else {
        lerp(alt_min, alt_max, flight, flight_count)
    };
    if seat % 2 == 1 {
        base + pair_stack_m(flight, pair)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_flights_matches_built_seats() {
        let cfg = FlightConfig {
            flight_count: 5,
            max_in_flight: 4,
            type_ids: vec!["mig15bis".into(), "la11".into(), "yak9p".into()],
            type_skills: vec![3, 2, 2],
            altitude_min: 800.0,
            altitude_max: 6200.0,
            ..FlightConfig::default()
        };
        let types = resolve_types(&cfg.type_ids, &cfg.type_skills).expect("types");
        let sizes = flight_sizes(5, 4);
        let (seats, plans) =
            build_seats(&types, &sizes, &cfg, &builtin_plane_catalog()).expect("seats");
        let preview = preview_flights(&cfg);
        let flat: Vec<(f64, bool)> = preview.iter().flat_map(|f| f.seats.clone()).collect();
        assert_eq!(flat.len(), seats.len());
        for ((alt, lead), (seat, plan)) in flat.iter().zip(seats.iter().zip(&plans)) {
            assert!((*alt as f32 - seat.altitude).abs() < 0.01);
            assert_eq!(*lead, seat.orders[1].kind == OrderKind::AttackArea);
            assert_eq!(preview[plan.flight].type_label, plan.ac.label);
        }
    }
    use crate::pack::{builtin_template, generate_pack};
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;

    fn configured(cfg: FlightConfig) -> Il2Entity {
        let mut root = builtin_template().expect("builtin");
        configure_aircraft(&mut root, &cfg).expect("configure");
        root
    }

    fn collect_blocks<'a>(e: &'a Il2Entity, block: &str, out: &mut Vec<&'a Il2Entity>) {
        if e.block_type == block {
            out.push(e);
        }
        for c in &e.children {
            collect_blocks(c, block, out);
        }
    }

    fn attack_areas(g1: &Il2Entity) -> Vec<&Il2Entity> {
        let mut out = Vec::new();
        collect_blocks(g1, "MCU_CMD_AttackArea", &mut out);
        out
    }

    fn area_objects(g1: &Il2Entity) -> usize {
        let mut ids = Vec::new();
        for a in attack_areas(g1) {
            for id in &a.objects {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
        }
        ids.len()
    }

    #[test]
    fn two_flights_mix_sizes_and_separate_covers() {
        let cfg = FlightConfig {
            flight_count: 2,
            max_in_flight: 2,
            type_ids: vec!["mig15bis".into()],
            type_skills: vec![3],
            country: 501,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(g1.count_block_type("Plane"), 3);
        assert_eq!(g1.find_by_name("Spawn 1").unwrap().objects.len(), 2);
        assert_eq!(g1.find_by_name("Spawn 2").unwrap().objects.len(), 1);
        let cover1 = g1.find_by_name("Cover Wing 1").unwrap();
        assert_eq!(cover1.objects.len(), 1);
        assert_eq!(cover1.targets.len(), 1);
        assert!(g1.find_by_name("Cover Wing 2").is_none());
        assert_eq!(area_objects(g1), 2);
    }

    #[test]
    fn max_four_spreads_1_through_4() {
        let cfg = FlightConfig {
            flight_count: 4,
            max_in_flight: 4,
            type_ids: vec!["yak9p".into()],
            type_skills: vec![2],
            country: 503,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(flight_sizes(4, 4), vec![4, 3, 2, 1]);
        assert_eq!(g1.count_block_type("Plane"), 10);
        assert_eq!(g1.find_by_name("Spawn 1").unwrap().objects.len(), 4);
        assert_eq!(g1.find_by_name("Spawn 4").unwrap().objects.len(), 1);
        assert!(g1.find_by_name("Cover Wing 1").is_some());
        assert!(g1.find_by_name("Cover Wing 4").is_some());
        assert!(g1.find_by_name("Cover Wing 5").is_none());
        assert_eq!(
            g1.find_by_name("Red 11").unwrap().property("Model"),
            Some("\"graphics\\planes\\yak9p\\yak9p.mgm\"")
        );
    }

    #[test]
    fn each_cover_is_one_wingman_on_one_lead() {
        let cfg = FlightConfig {
            flight_count: 1,
            max_in_flight: 4,
            type_ids: vec!["mig15bis".into()],
            type_skills: vec![4],
            country: 501,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        let c1 = g1.find_by_name("Cover Wing 1").unwrap();
        let c2 = g1.find_by_name("Cover Wing 2").unwrap();
        assert_eq!(c1.objects.len(), 1);
        assert_eq!(c1.targets.len(), 1);
        assert_eq!(c2.objects.len(), 1);
        assert_ne!(c1.objects[0], c2.objects[0]);
        assert_ne!(c1.targets[0], c2.targets[0]);
        assert_eq!(attack_areas(g1).len(), 2);
        let lead1 = g1.find_by_name("Red 11").unwrap();
        let wing1 = g1.find_by_name("Red 12").unwrap();
        let lead_ai: i32 = lead1.property("AILevel").unwrap().parse().unwrap();
        let wing_ai: i32 = wing1.property("AILevel").unwrap().parse().unwrap();
        assert!(lead_ai >= wing_ai);
    }

    #[test]
    fn singleton_flight_is_attack_only() {
        let cfg = FlightConfig {
            flight_count: 3,
            max_in_flight: 1,
            type_ids: vec!["la11".into()],
            country: 503,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(g1.count_block_type("Plane"), 3);
        assert_eq!(area_objects(g1), 3);
        assert!(g1.find_by_name("Cover Wing 1").is_none());
    }

    #[test]
    fn usa_sets_western_checkzones() {
        let cfg = FlightConfig {
            country: 601,
            type_ids: vec!["f86a5".into()],
            flight_count: 1,
            max_in_flight: 2,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(g1.find_by_name("Zone IN").unwrap().property("PlaneCoalitions"), Some("[1]"));
        assert_eq!(g1.find_by_name("Zone OUT").unwrap().property("PlaneCoalitions"), Some("[1]"));
        assert_eq!(g1.find_by_name("Red 11").unwrap().property("Country"), Some("601"));
        assert_eq!(
            g1.find_by_name("Red 11").unwrap().property("Model"),
            Some("\"graphics\\planes\\f86a5\\f86a5.mgm\"")
        );
        let pack = generate_pack(&root, 3).expect("pack");
        assert_eq!(pack.name(), Some("USA Fighters 3pack - Linked"));
    }

    #[test]
    fn timers_and_altitude_apply() {
        let cfg = FlightConfig {
            flight_count: 2,
            max_in_flight: 1,
            cooldown: 90.0,
            reinforcement: 120.0,
            delete_orders: 45.0,
            altitude_min: 2000.0,
            altitude_max: 4000.0,
            type_ids: vec!["f51d".into()],
            country: 601,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(g1.find_by_name("COOLDOWN").unwrap().property("Time"), Some("90"));
        assert!(g1.find_by_name("REENFORCEMENTS (33%)").is_none());
        assert_eq!(
            g1.find_by_name("SPAWN UNITS").unwrap().targets,
            vec![g1
                .find_by_name("Randomizer:INPUT")
                .unwrap()
                .index
                .unwrap()]
        );
        assert_eq!(g1.find_by_name("Delay Delete").unwrap().property("Time"), Some("45"));
        let y11: f64 = g1.find_by_name("Red 11").unwrap().property("YPos").unwrap().parse().unwrap();
        let y21: f64 = g1.find_by_name("Blue 21").unwrap().property("YPos").unwrap().parse().unwrap();
        assert!((y11 - 2000.0).abs() < 0.01);
        assert!((y21 - 4000.0).abs() < 0.01);
    }

    fn ypos(g1: &Il2Entity, name: &str) -> f64 {
        g1.find_by_name(name)
            .unwrap()
            .property("YPos")
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn random_pct_is_equal_waterfall() {
        assert_eq!(random_pct(0, 1), 100);
        assert_eq!(random_pct(0, 2), 50);
        assert_eq!(random_pct(1, 2), 100);
        assert_eq!(random_pct(0, 4), 25);
        assert_eq!(random_pct(1, 4), 33);
        assert_eq!(random_pct(2, 4), 50);
        assert_eq!(random_pct(3, 4), 100);
    }

    #[test]
    fn randomizer_uses_equal_odds_and_half_second_steps() {
        let cfg = FlightConfig {
            flight_count: 4,
            max_in_flight: 1,
            type_ids: vec!["la11".into()],
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        let r1 = g1.find_by_name("Random 1:25% 500ms").unwrap();
        assert_eq!(r1.property("Random"), Some("25"));
        assert_eq!(r1.property("Time"), Some("0.5"));
        let r2 = g1.find_by_name("Random 2:33% 1000ms").unwrap();
        assert_eq!(r2.property("Random"), Some("33"));
        assert_eq!(r2.property("Time"), Some("1"));
        let r3 = g1.find_by_name("Random 3:50% 1500ms").unwrap();
        assert_eq!(r3.property("Random"), Some("50"));
        let r4 = g1.find_by_name("Random 4:100% 2000ms").unwrap();
        assert_eq!(r4.property("Random"), Some("100"));
        assert_eq!(r4.property("Time"), Some("2"));
        let wait = g1.find_by_name("Wait for Output 2500ms").unwrap();
        assert_eq!(wait.property("Time"), Some("2.5"));
        assert!(g1.find_by_name("Random 1:5% 100ms").is_none());
        assert!(g1.find_by_name("Wait for Output 600ms").is_none());
    }

    #[test]
    fn pair_stacks_25_to_50m() {
        let cfg = FlightConfig {
            flight_count: 1,
            max_in_flight: 2,
            altitude_min: 2000.0,
            altitude_max: 4000.0,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        let lead = ypos(g1, "Red 11");
        let wing = ypos(g1, "Red 12");
        let delta = wing - lead;
        assert!((3000.0 - lead).abs() < 0.01);
        assert!(delta >= 25.0 && delta <= 50.0, "pair stack {delta}");
        assert!(delta < 100.0, "2-ship must not use the 2000 m high-cover offset");
    }

    #[test]
    fn four_ship_has_low_and_high_cover() {
        let cfg = FlightConfig {
            flight_count: 1,
            max_in_flight: 4,
            altitude_min: 500.0,
            altitude_max: 5500.0,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        let y11 = ypos(g1, "Red 11");
        let y12 = ypos(g1, "Red 12");
        let y13 = ypos(g1, "Red 13");
        let y14 = ypos(g1, "Red 14");
        assert!(y11 >= 500.0 && y11 <= 1500.0, "low cover {y11}");
        assert!(((y12 - y11).abs() - pair_stack_m(0, 0)).abs() < 0.01);
        assert!((y13 - y11 - 2000.0).abs() < 0.01, "high cover offset {}", y13 - y11);
        let high_stack = y14 - y13;
        assert!(high_stack >= 25.0 && high_stack <= 50.0, "high pair stack {high_stack}");
    }

    #[test]
    fn high_cover_shifts_when_max_altitude_rises() {
        let low_max = FlightConfig {
            flight_count: 1,
            max_in_flight: 4,
            altitude_min: 500.0,
            altitude_max: 5500.0,
            type_ids: vec!["la11".into()],
            ..FlightConfig::default()
        };
        let high_max = FlightConfig {
            altitude_max: 11000.0,
            ..low_max.clone()
        };
        let y_low = ypos(configured(low_max).find_by_name("Group 1").unwrap(), "Red 11");
        let y_high = ypos(configured(high_max).find_by_name("Group 1").unwrap(), "Red 11");
        assert!(y_high > y_low + 100.0, "low cover should rise with max ({y_low} vs {y_high})");
    }

    #[test]
    fn pack_clone_keeps_configured_flights() {
        let cfg = FlightConfig {
            flight_count: 3,
            max_in_flight: 2,
            type_ids: vec!["mig15bis".into(), "f84e".into()],
            country: 501,
            ..FlightConfig::default()
        };
        let mut root = configured(cfg);
        let out = generate_pack(&root, 2).expect("pack");
        assert_eq!(out.name(), Some("USSR Fighters 2pack - Linked"));
        assert_eq!(out.find_by_name("Group 2").unwrap().count_block_type("Plane"), 5);
        let text = serialize_group(&out);
        parse_group_file(&text).expect("reparse");
        assert!(out.find_by_name("Group 2").unwrap().find_by_name("Spawn 3").is_some());
        let _ = &mut root;
    }

    #[test]
    fn ussr_keeps_eastern_checkzones() {
        let root = configured(FlightConfig::default());
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(g1.find_by_name("Zone IN").unwrap().property("PlaneCoalitions"), Some("[2]"));
    }

    #[test]
    fn pair_uses_template_logic_and_original_pack_zones() {
        let cfg = FlightConfig {
            flight_count: 1,
            max_in_flight: 2,
            type_ids: vec!["f51d".into()],
            country: 601,
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(g1.find_by_name("Zone IN").unwrap().property("Zone"), Some("16000"));
        assert_eq!(g1.find_by_name("Zone OUT").unwrap().property("Zone"), Some("35000"));
        let areas = attack_areas(g1);
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].property("AttackArea"), Some("30000"));
        assert_eq!(areas[0].property("AttackAir"), Some("1"));
        assert_eq!(areas[0].property("Time"), Some("600"));
        assert_eq!(
            g1.find_by_name("Red 11").unwrap().property("AiRTBDecision"),
            Some("0")
        );
        assert!(g1.find_by_name("OnSpawned 1").is_some());
        assert!(g1.find_by_name("Mission Complete 1").is_some());
        assert!(g1.find_by_name("Enable Spawner").is_some());
        assert!(g1.find_by_name("Delete Orders").is_some());
        let events: Vec<i32> = g1
            .find_by_name("Units")
            .unwrap()
            .children
            .iter()
            .find(|c| c.block_type == "MCU_TR_Entity")
            .map(|ent| {
                ent.children
                    .iter()
                    .filter(|c| c.block_type == "OnEvents")
                    .flat_map(|w| w.children.iter())
                    .filter_map(|ev| ev.property("Type").and_then(|s| s.parse().ok()))
                    .collect()
            })
            .unwrap_or_default();
        assert!(events.contains(&3), "OnPlaneCriticalDamage");
        assert!(events.contains(&1), "OnPilotWounded");
        assert!(events.contains(&8), "OnBingoMainMG");
        assert!(events.contains(&7), "OnBingoFuel");
        let gates = root.find_by_name("NodeGates").unwrap();
        let out_disable = gates.find_by_name("1OUT - DISABLE").unwrap().index.unwrap();
        assert!(g1.find_by_name("Zone IN").unwrap().targets.contains(&out_disable));
    }

    #[test]
    fn leftover_and_high_cover_leads_get_attack_area() {
        let cfg = FlightConfig {
            flight_count: 4,
            max_in_flight: 5,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        assert_eq!(flight_sizes(4, 5), vec![5, 4, 3, 2]);
        assert_eq!(g1.count_block_type("Plane"), 14);
        // 5-ship: 3 leads, 4-ship: 2, 3-ship: 2, 2-ship: 1
        assert_eq!(attack_areas(g1).len(), 8);
        assert_eq!(area_objects(g1), 8);
        for i in 1..=14 {
            let spawned = g1
                .find_by_name(&format!("OnSpawned {i}"))
                .unwrap_or_else(|| panic!("missing OnSpawned {i}"));
            assert!(
                !spawned.targets.is_empty(),
                "OnSpawned {i} has no next order"
            );
        }
    }

    #[test]
    fn bingo_sends_only_that_plane_home() {
        let cfg = FlightConfig {
            flight_count: 1,
            max_in_flight: 2,
            delete_orders: 45.0,
            type_ids: vec!["f51d".into()],
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        let end = g1.find_by_name("CLEANUP 1").unwrap();
        let force = g1.find_by_name("Force Complete 1").unwrap();
        let rtb_delay = g1.find_by_name("RTB DELAY 1").unwrap();
        let rtb = g1.find_by_name("RTB Plane 1").unwrap();
        let delay = g1.find_by_name("Delay Delete 1").unwrap();
        let deact = g1.find_by_name("Deactivate 1").unwrap();
        assert!(end.targets.contains(&force.index.unwrap()));
        assert!(end.targets.contains(&rtb_delay.index.unwrap()));
        assert!(end.targets.contains(&delay.index.unwrap()));
        assert!(rtb_delay.targets.contains(&rtb.index.unwrap()));
        assert!(delay.targets.contains(&deact.index.unwrap()));
        assert_eq!(delay.property("Time"), Some("45"));
        assert_eq!(rtb.objects.len(), 1);
        assert_eq!(force.objects.len(), 1);
        assert_eq!(deact.objects.len(), 1);
        let force2 = g1.find_by_name("Force Complete 2").unwrap();
        let rtb2 = g1.find_by_name("RTB Plane 2").unwrap();
        assert_eq!(force2.objects.len(), 1);
        assert_ne!(force.objects[0], force2.objects[0]);
        assert_ne!(rtb.objects[0], rtb2.objects[0]);
        let death = g1
            .find_by_name("Logic")
            .unwrap()
            .children
            .iter()
            .find(|c| c.name() == Some("DeathCount"))
            .unwrap()
            .index
            .unwrap();
        assert!(delay.targets.contains(&death));
        assert!(deact.targets.is_empty());
        let mc1 = g1.find_by_name("Mission Complete 1").unwrap();
        let mc2 = g1.find_by_name("Mission Complete 2").unwrap();
        assert!(mc1.targets.contains(&end.index.unwrap()));
        assert!(!mc1.targets.contains(&g1.find_by_name("CLEANUP 2").unwrap().index.unwrap()));
        assert!(mc2.targets.contains(&g1.find_by_name("CLEANUP 2").unwrap().index.unwrap()));
        let parked = {
            let mut pack = generate_pack(&root, 2).expect("pack");
            crate::pack::park_rtbs(&mut pack, &[(80_000.0, 210_000.0), (80_000.0, 230_000.0)]);
            pack
        };
        let dest = parked.find_by_name("RTB - 1").unwrap().pos_xz().unwrap();
        let plane_wp = parked
            .find_by_name("Group 1")
            .unwrap()
            .find_by_name("RTB Plane 1")
            .unwrap()
            .pos_xz()
            .unwrap();
        assert!((plane_wp.0 - dest.0).abs() < 0.5 && (plane_wp.1 - dest.1).abs() < 0.5);
    }

    #[test]
    fn cleanup_stack_sits_right_of_spawn() {
        let cfg = FlightConfig {
            flight_count: 2,
            max_in_flight: 2,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let root = configured(cfg);
        let g1 = root.find_by_name("Group 1").unwrap();
        let spawn = g1.find_by_name("SPAWN UNITS").unwrap().pos_xz().unwrap();
        let zone = g1.find_by_name("Zone IN").unwrap().pos_xz().unwrap();
        let end = g1.find_by_name("CLEANUP 1").unwrap().pos_xz().unwrap();
        let force = g1.find_by_name("Force Complete 1").unwrap().pos_xz().unwrap();
        let delay = g1.find_by_name("Delay Delete 1").unwrap().pos_xz().unwrap();
        let deact = g1.find_by_name("Deactivate 1").unwrap().pos_xz().unwrap();
        let delete = g1.find_by_name("Trigger Delete").unwrap().pos_xz().unwrap();
        assert!(spawn.1 < zone.1, "spawn stack west of zone");
        assert!(end.1 > zone.1, "cleanup east of zone");
        assert!(end.1 > spawn.1, "cleanup east of spawn");
        assert!(force.1 > end.1, "force complete sits outboard of the timer");
        assert!(deact.0 < end.0, "deactivate south of cleanup");
        assert!(delay.0 < end.0, "delay delete south of cleanup");
        assert!(deact.0 <= delay.0, "deactivate at the bottom of the stack");
        assert!(delete.1 > zone.1, "group delete on the cleanup side");
        let end2 = g1.find_by_name("CLEANUP 2").unwrap().pos_xz().unwrap();
        assert!(end2.1 > end.1, "second plane cleanup is its own column further east");
    }

    fn xz(g1: &Il2Entity, name: &str) -> (f64, f64) {
        g1.find_by_name(name).unwrap().pos_xz().unwrap()
    }

    #[test]
    fn four_ship_lays_out_finger_four() {
        let cfg = FlightConfig {
            flight_count: 1,
            max_in_flight: 4,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let g1 = configured(cfg).find_by_name("Group 1").unwrap().clone();
        let lead = xz(&g1, "Red 11");
        let right = xz(&g1, "Red 12");
        let left = xz(&g1, "Red 13");
        let far = xz(&g1, "Red 14");
        assert!(lead.0 > right.0, "lead north of right wing");
        assert!(lead.0 > left.0, "lead north of left wing");
        assert!(right.1 > lead.1, "right wing east of lead");
        assert!(left.1 < lead.1, "left wing west of lead");
        assert!(far.1 < left.1, "second left further west");
    }

    #[test]
    fn leftover_ships_stay_low() {
        assert!(!pair_is_high(0, 4) && pair_is_high(2, 4));
        assert!(!pair_is_high(2, 3), "3-ship leftover stays low");
        assert!(!pair_is_high(4, 5), "5-ship leftover stays low");
        assert!(pair_is_high(2, 6) && !pair_is_high(4, 6), "6-ship is 4 down 2 up");
        assert!(pair_is_high(2, 8) && !pair_is_high(4, 8) && pair_is_high(6, 8));

        let three = FlightConfig {
            flight_count: 1,
            max_in_flight: 3,
            altitude_min: 500.0,
            altitude_max: 5500.0,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let g1 = configured(three).find_by_name("Group 1").unwrap().clone();
        let y11 = ypos(&g1, "Red 11");
        let y13 = ypos(&g1, "Red 13");
        assert!((y13 - y11).abs() < 1.0, "3-ship leftover must not climb 2000 m ({y11} vs {y13})");

        let six = FlightConfig {
            flight_count: 1,
            max_in_flight: 6,
            altitude_min: 500.0,
            altitude_max: 5500.0,
            type_ids: vec!["mig15bis".into()],
            ..FlightConfig::default()
        };
        let g1 = configured(six).find_by_name("Group 1").unwrap().clone();
        let y11 = ypos(&g1, "Red 11");
        let y13 = ypos(&g1, "Red 13");
        let y15 = ypos(&g1, "Red 15");
        assert!((y13 - y11 - 2000.0).abs() < 0.01, "second pair of the 4-ship is high");
        assert!((y15 - y11).abs() < 1.0, "trailing 6-ship pair stays low");
    }
}
