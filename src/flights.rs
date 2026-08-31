//! Rebuild Group 1's airplanes, randomizer, pairing, and related MCU links
//! from GUI settings. The rest of the pack logic (zones, NodeGates) stays.

use crate::aircraft::{
    aircraft_by_id, callsign_for, encode_tcode, encode_tcode_color, flight_color,
    pair_skills, plane_coalitions_for_country, plane_display_name, AircraftType, AIRCRAFT_TYPES,
};
use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;

#[derive(Debug, Clone)]
pub struct FlightConfig {
    pub flight_count: u32,
    pub max_in_flight: u32,
    pub type_ids: Vec<String>,
    /// Recommended AILevel 0–4, parallel to `type_ids`.
    pub type_skills: Vec<i32>,
    pub country: i32,
    pub cooldown: f32,
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
/// Second pair in a 3/4-ship sits this far above the low-cover pair.
const HIGH_COVER_OFFSET_M: f64 = 2000.0;
const PAIR_STACK_MIN_M: f64 = 25.0;
const PAIR_STACK_MAX_M: f64 = 50.0;
/// Canonical low-cover band; scales with the GUI max altitude (ref 5500 m).
const REF_ALT_MAX: f64 = 5500.0;
const REF_LOW_MIN: f64 = 500.0;
const REF_LOW_MAX: f64 = 1500.0;

struct BuiltFlight {
    entity_ids: Vec<i32>,
    leads: Vec<i32>,
    wings: Vec<i32>,
}

/// Apply flight composition to **Group 1** and **RTB - 1** inside `root`.
pub fn configure_aircraft(root: &mut Il2Entity, cfg: &FlightConfig) -> Result<(), String> {
    let flight_count = cfg.flight_count.clamp(1, 10) as usize;
    let max_in_flight = cfg.max_in_flight.clamp(1, 8) as usize;
    let types = resolve_types(&cfg.type_ids, &cfg.type_skills)?;
    let sizes = flight_sizes(flight_count, max_in_flight);

    let mut next_id = root.max_index().saturating_add(1);

    let group1_pos = root
        .children
        .iter()
        .position(|c| c.block_type == "Group" && c.name() == Some("Group 1"))
        .ok_or("builtin template has no Group 1")?;
    let rtb_pos = root
        .children
        .iter()
        .position(|c| c.block_type == "MCU_Waypoint" && c.name() == Some("RTB - 1"))
        .ok_or("builtin template has no RTB - 1")?;

    let (proto_plane, proto_entity) = {
        let airplanes = root.children[group1_pos]
            .find_by_name("Airplanes")
            .ok_or("Group 1 has no Airplanes group")?;
        prototype_pair(airplanes)?
    };
    let proto_timer;
    let proto_spawner;
    let proto_deact;
    let proto_act;
    let proto_death;
    let proto_spawn_count;
    let proto_cover;
    let proto_delay;
    {
        let group1 = &root.children[group1_pos];
        let randomizer = group1
            .find_by_name("Randomizer")
            .ok_or("Group 1 has no Randomizer")?;
        proto_timer = randomizer
            .find_by_name("Wait for Output 600ms")
            .cloned()
            .ok_or("missing timer prototype")?;
        proto_spawner = randomizer
            .find_by_name("Spawn 1")
            .cloned()
            .ok_or("missing spawner prototype")?;
        proto_deact = randomizer
            .find_by_name("CloseInput")
            .cloned()
            .ok_or("missing deactivate prototype")?;
        proto_act = randomizer
            .find_by_name("ReOpen Outputs")
            .cloned()
            .ok_or("missing activate prototype")?;
        proto_death = group1
            .find_by_name("DeathCount")
            .cloned()
            .ok_or("missing DeathCount prototype")?;
        proto_spawn_count = group1
            .find_by_name("SpawnCount")
            .cloned()
            .ok_or("missing SpawnCount prototype")?;
        proto_cover = group1
            .find_by_name("Cover Lead")
            .cloned()
            .ok_or("missing Cover MCU prototype")?;
        proto_delay = group1
            .children
            .iter()
            .find_map(|c| {
                if c.name() == Some("Logics") {
                    c.children.iter().find(|t| t.name() == Some("50ms")).cloned()
                } else {
                    None
                }
            })
            .or_else(|| group1.find_by_name("50ms").cloned())
            .ok_or("missing 50ms timer prototype")?;
    }

    let alt_min = cfg.altitude_min.min(cfg.altitude_max) as f64;
    let alt_max = cfg.altitude_max.max(cfg.altitude_min) as f64;

    let mut flights: Vec<BuiltFlight> = Vec::with_capacity(flight_count);
    let mut plane_nodes: Vec<Il2Entity> = Vec::new();

    for f in 0..flight_count {
        let (ac, recommended) = types[f % types.len()];
        let size = sizes[f];
        let mut built = BuiltFlight {
            entity_ids: Vec::new(),
            leads: Vec::new(),
            wings: Vec::new(),
        };
        for seat in 0..size {
            let pair = seat / 2;
            let (lead_skill, wing_skill) = pair_skills(recommended, f, pair);
            let ailevel = if seat % 2 == 0 { lead_skill } else { wing_skill };
            let (mut plane, mut entity) =
                clone_pair(&proto_plane, &proto_entity, &mut next_id);
            let dx = f as f64 * 180.0 + seat as f64 * 55.0;
            let dz = f as f64 * -130.0 + seat as f64 * 45.0;
            plane.translate_xz(dx, dz);
            entity.translate_xz(dx, dz);
            let y = plane_altitude(alt_min, alt_max, f, flight_count, seat, size);
            plane.set_ypos(y);
            entity.set_ypos(y + 0.2);
            apply_plane_identity(&mut plane, cfg.country, ac, f, seat, ailevel);
            let eid = entity.index.ok_or("cloned entity missing Index")?;
            built.entity_ids.push(eid);
            if seat % 2 == 0 {
                built.leads.push(eid);
            } else {
                built.wings.push(eid);
            }
            plane_nodes.push(plane);
            plane_nodes.push(entity);
        }
        flights.push(built);
    }

    let mut spawners = Vec::new();
    let mut death_counts = Vec::new();
    let mut spawn_counts = Vec::new();
    for (f, flight) in flights.iter().enumerate() {
        let mut spawner = clone_mcu(&proto_spawner, &mut next_id, &format!("Spawn {}", f + 1));
        spawner.set_objects(flight.entity_ids.clone());
        let mut death = clone_mcu(&proto_death, &mut next_id, "DeathCount");
        death.set_property("Counter", flight.entity_ids.len().to_string());
        let mut scount = clone_mcu(&proto_spawn_count, &mut next_id, "SpawnCount");
        scount.set_property("Counter", flight.entity_ids.len().to_string());
        spawners.push(spawner);
        death_counts.push(death);
        spawn_counts.push(scount);
    }

    patch_plane_events(&mut plane_nodes, &flights, &spawners, &death_counts, &spawn_counts)?;

    let all_entities: Vec<i32> = flights.iter().flat_map(|f| f.entity_ids.iter().copied()).collect();
    let all_leads: Vec<i32> = flights.iter().flat_map(|f| f.leads.iter().copied()).collect();
    let mut pairs: Vec<(i32, i32)> = Vec::new();
    for flight in &flights {
        for (lead, wing) in flight.leads.iter().zip(flight.wings.iter()) {
            pairs.push((*lead, *wing));
        }
    }

    let randomizer_nodes = build_randomizer(
        &proto_timer,
        &proto_deact,
        &proto_act,
        &spawners,
        &mut next_id,
    )?;
    let input_id = randomizer_nodes
        .iter()
        .find(|n| n.name() == Some("Randomizer:INPUT"))
        .and_then(|n| n.index)
        .ok_or("randomizer input missing Index")?;

    {
        let group1 = &mut root.children[group1_pos];
        let airplanes = group1
            .find_by_name_mut("Airplanes")
            .ok_or("Group 1 has no Airplanes group")?;
        airplanes.children = plane_nodes;

        let randomizer = group1
            .find_by_name_mut("Randomizer")
            .ok_or("Group 1 has no Randomizer")?;
        randomizer.children = randomizer_nodes;

        let logics = group1
            .find_by_name_mut("Logics")
            .ok_or("Group 1 has no Logics")?;
        replace_named(logics, "DeathCount", death_counts);
        replace_named(logics, "SpawnCount", spawn_counts);
        install_cover_wings(logics, &proto_cover, &proto_delay, &pairs, &mut next_id)?;
        rewire_logics(logics, input_id, &all_entities, &all_leads, cfg);
    }

    root.children[rtb_pos].set_objects(all_entities);
    Ok(())
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

/// `max` is a ceiling. Flights cycle `max, max-1, …, 1` so a setting of 4
/// produces 4-, 3-, 2-, and 1-ship elements rather than four 4-ships.
pub fn flight_sizes(count: usize, max: usize) -> Vec<usize> {
    let max = max.max(1);
    (0..count).map(|i| max - (i % max)).collect()
}

fn prototype_pair(airplanes: &Il2Entity) -> Result<(Il2Entity, Il2Entity), String> {
    let plane = airplanes
        .children
        .iter()
        .find(|c| c.block_type == "Plane")
        .cloned()
        .ok_or("no prototype Plane")?;
    let link = plane
        .property("LinkTrId")
        .and_then(|s| s.parse::<i32>().ok());
    let entity = airplanes
        .children
        .iter()
        .find(|c| c.block_type == "MCU_TR_Entity" && c.index == link)
        .cloned()
        .ok_or("no prototype plane entity")?;
    Ok((plane, entity))
}

fn clone_pair(plane: &Il2Entity, entity: &Il2Entity, next_id: &mut i32) -> (Il2Entity, Il2Entity) {
    let mut wrapper = Il2Entity::new("Group");
    wrapper.children.push(plane.clone());
    wrapper.children.push(entity.clone());
    let (mut cloned, _) = duplicate_template(&wrapper, next_id);
    let entity = cloned.children.pop().unwrap();
    let plane = cloned.children.pop().unwrap();
    (plane, entity)
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
}

fn patch_plane_events(
    nodes: &mut [Il2Entity],
    flights: &[BuiltFlight],
    spawners: &[Il2Entity],
    deaths: &[Il2Entity],
    spawns: &[Il2Entity],
) -> Result<(), String> {
    for (f, flight) in flights.iter().enumerate() {
        let death_id = deaths[f].index.ok_or("DeathCount missing Index")?;
        let spawn_id = spawns[f].index.ok_or("SpawnCount missing Index")?;
        let spawner_id = spawners[f].index.ok_or("Spawner missing Index")?;
        for node in nodes.iter_mut() {
            if node.block_type != "MCU_TR_Entity" {
                continue;
            }
            let Some(id) = node.index else { continue };
            if !flight.entity_ids.contains(&id) {
                continue;
            }
            set_nested_id(node, "OnEvent", "TarId", death_id);
            set_nested_id(node, "OnReport", "TarId", spawn_id);
            set_nested_id(node, "OnReport", "CmdId", spawner_id);
        }
    }
    Ok(())
}

fn set_nested_id(entity: &mut Il2Entity, child_type: &str, key: &str, value: i32) {
    entity.for_each_mut(&mut |e| {
        if e.block_type == child_type {
            e.set_property(key, value.to_string());
        }
    });
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

    let input_id = input.index.unwrap();
    let wait_id = wait.index.unwrap();
    let reopen_id = reopen.index.unwrap();
    let close_input_id = close_input.index.unwrap();
    let out_ids: Vec<i32> = outs.iter().filter_map(|o| o.index).collect();
    let random_ids: Vec<i32> = randoms.iter().filter_map(|o| o.index).collect();
    let closer_ids: Vec<i32> = closers.iter().filter_map(|o| o.index).collect();
    let spawner_ids: Vec<i32> = spawners.iter().filter_map(|o| o.index).collect();

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

fn replace_named(parent: &mut Il2Entity, name: &str, replacements: Vec<Il2Entity>) {
    let first = parent.children.iter().position(|c| c.name() == Some(name));
    parent.children.retain(|c| c.name() != Some(name));
    if let Some(i) = first.filter(|i| *i <= parent.children.len()) {
        let mut rest = parent.children.split_off(i);
        parent.children.extend(replacements);
        parent.children.append(&mut rest);
    } else {
        parent.children.extend(replacements);
    }
}

fn install_cover_wings(
    logics: &mut Il2Entity,
    proto_cover: &Il2Entity,
    proto_delay: &Il2Entity,
    pairs: &[(i32, i32)],
    next_id: &mut i32,
) -> Result<(), String> {
    logics.children.retain(|c| {
        let name = c.name().unwrap_or("");
        !(name == "Cover Lead" || name == "50ms" || name.starts_with("Cover Wing"))
    });

    let mut covers = Vec::new();
    for (i, (lead, wing)) in pairs.iter().enumerate() {
        let mut cover = clone_mcu(proto_cover, next_id, &format!("Cover Wing {}", i + 1));
        cover.set_objects(vec![*wing]);
        cover.set_targets(vec![*lead]);
        covers.push(cover);
    }

    let mut delays = Vec::new();
    if covers.len() > 1 {
        for _ in 0..covers.len() - 1 {
            let mut delay = clone_mcu(proto_delay, next_id, "50ms");
            delay.set_property("Time", "0.05");
            delay.set_property("Random", "100");
            delays.push(delay);
        }
    }

    let cover_ids: Vec<i32> = covers.iter().filter_map(|c| c.index).collect();
    let delay_ids: Vec<i32> = delays.iter().filter_map(|d| d.index).collect();

    for i in 0..delays.len() {
        if i + 1 < delay_ids.len() {
            delays[i].set_targets(vec![cover_ids[i + 1], delay_ids[i + 1]]);
        } else {
            delays[i].set_targets(vec![cover_ids[i + 1]]);
        }
    }

    let more_targets = if cover_ids.is_empty() {
        Vec::new()
    } else if delay_ids.is_empty() {
        vec![cover_ids[0]]
    } else {
        vec![cover_ids[0], delay_ids[0]]
    };
    if let Some(orders) = logics.find_by_name_mut("MORE ORDERS") {
        orders.set_targets(more_targets);
    }

    logics.children.extend(covers);
    logics.children.extend(delays);
    Ok(())
}

fn rewire_logics(
    logics: &mut Il2Entity,
    input_id: i32,
    all_entities: &[i32],
    leads: &[i32],
    cfg: &FlightConfig,
) {
    let old_input = logics
        .find_by_name("ACTIONS / SPAWN")
        .and_then(|e| e.targets.first().copied());
    let coalitions = plane_coalitions_for_country(cfg.country).to_string();
    let cooldown = format_time(cfg.cooldown);
    let reinforcement = format_time(cfg.reinforcement);
    let delete_orders = format_time(cfg.delete_orders);

    logics.for_each_mut(&mut |e| {
        let name = e.name().map(str::to_string);
        match name.as_deref() {
            Some("ACTIONS / SPAWN") => {
                if let Some(old) = old_input {
                    for t in &mut e.targets {
                        if *t == old {
                            *t = input_id;
                        }
                    }
                    e.set_targets(e.targets.clone());
                }
            }
            Some("command AttackArea") => e.set_objects(leads.to_vec()),
            Some("Deactivate ALL") | Some("Trigger Delete") | Some("command Force Complete") => {
                e.set_objects(all_entities.to_vec());
            }
            Some("COOLDOWN") => e.set_property("Time", cooldown.clone()),
            Some("REENFORCEMENTS (33%)") => e.set_property("Time", reinforcement.clone()),
            Some("Delay Delete") => e.set_property("Time", delete_orders.clone()),
            Some("Zone IN") | Some("Zone OUT") => {
                e.set_property("PlaneCoalitions", coalitions.clone());
            }
            _ => {}
        }
    });
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

/// Pair 0 is low cover. A second pair (3- and 4-ships) is high cover, ~2000 m up.
/// 1- and 2-ships stay on the min–max spread.
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
        if pair >= 1 {
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
    use crate::pack::{builtin_template, generate_pack};
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;

    fn configured(cfg: FlightConfig) -> Il2Entity {
        let mut root = builtin_template().expect("builtin");
        configure_aircraft(&mut root, &cfg).expect("configure");
        root
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
        // max=2 → sizes 2 then 1
        assert_eq!(g1.count_block_type("Plane"), 3);
        assert_eq!(g1.find_by_name("Spawn 1").unwrap().objects.len(), 2);
        assert_eq!(g1.find_by_name("Spawn 2").unwrap().objects.len(), 1);
        let cover1 = g1.find_by_name("Cover Wing 1").unwrap();
        assert_eq!(cover1.objects.len(), 1);
        assert_eq!(cover1.targets.len(), 1);
        assert!(g1.find_by_name("Cover Wing 2").is_none());
        assert_eq!(g1.find_by_name("command AttackArea").unwrap().objects.len(), 2);
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
        // 4-ship has 2 pairs, 3-ship has 1, 2-ship has 1, singleton 0 → 4 covers
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
        assert_eq!(g1.find_by_name("command AttackArea").unwrap().objects.len(), 3);
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
        assert_eq!(
            g1.find_by_name("REENFORCEMENTS (33%)").unwrap().property("Time"),
            Some("120")
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
}
