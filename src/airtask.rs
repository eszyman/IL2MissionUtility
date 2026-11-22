//! airtask.rs — Air Tasking: a sortie library, one permutation, then the clock
//!
//! Sorties come from the 1950 library (aircraft, side, size) or from a strike
//! group the user already built. Each library flight is generated with
//! Template Builder's current strike package (`generate_template` after
//! `apply_mission_package`), so Zone IN, Time on Target proximity, FAILSAFE
//! IP, FAILSAFE WP2, WP 2 Prox, and MISSION END are the current graph.
//! The 1950 `.Group` files are not copied: that older emitter does not build
//! this package, and a two-flight Il-10 file becomes one flight here.
//!
//! Mission Begin (two or more sorties) runs a skip-chain once. Each stage
//! claims one sortie that has not been claimed yet, so the order is a full
//! permutation, not a rotated list. Only that first sortie's Zone IN is
//! opened, and only after the clock. Each MISSION END opens the next claimed
//! sortie and no other. PACKAGE WINDOW (3 hours) stops the relays and cleans
//! up the live plan without starting another.
//!
//! The fork's clock, quiet slots, zone chance, per-side budget, and on-station
//! extension are in the graph. The fork's player-count areas are not: a timer
//! launch starts the flight itself, and a zone launch arms the template's own
//! Zone IN only until the next mark. Defaults spawn: no wait, no quiet slot,
//! zone chance 100, budget 1, no extension, 180 minute window.
//!
//! The fork's mission-log replay window is not ported. It reads a server log
//! against that fork's probe names. `Trace` records this chain instead.
//!
//! ## Public API
//! * `builtin_library` / `build_sortie` — library data through the strike builder
//! * `AirOptions` — clock, zone chance, budget, extension, mission window
//! * `inspect_air_plan` / `link_air_task` — queue built groups
//!
//! ## Used by
//! * ui.rs — Air Tasking tab and the Map dock's Air Tasking section

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::template::{
    FlightRole, MissionPackage, TemplateOptions, ZoneCoalition, append_seat, apply_mission_package,
    bundled_catalog, center_cap_on, generate_template,
};

const LOGIC_GROUP: &str = "Air Tasking Logic";
const BEGIN_NAME: &str = "Air Tasking Mission Begin";
pub const WINDOW_NAME: &str = "PACKAGE WINDOW";
pub const CLOCK_FIRST: &str = "Clock First";
pub const QUIET_SLOT: &str = "Quiet Slot";
pub const ZONE_CHANCE: &str = "Zone Chance";
pub const FORCE_HOT: &str = "Force Hot";
pub const COLD_HOLD: &str = "Cold Hold";
pub const EXTEND_WORD: &str = "Extend";
pub const TRACE_NAME: &str = "Trace";
const STOP_NAME: &str = "Stop Advances";
const DISARM_START: &str = "Disarm At Start";

/// What has to be true for the first sortie to spawn on a fresh scenario.
pub const FIRING_HINT: &str = "On a timer only, each sortie keeps its own marks and its own number of firings. Generate builds that many flights on that path, and each mark spawns one of them with no check zone. A mark that arrives while that sortie is still out is missed, not moved later, until the sortie finishes or is destroyed. Launch while a sortie is out lets each side keep its own number up. Active at once starts at one NATO and one DPRK. On a timer with a check zone, the mark opens the area and a player there starts the sortie. The gold bar is how long the route on the map takes at the set speed. 180 minutes is 3 hours. With these settings left as they are, the first mark can start immediately and one sortie is out at a time.";

/// How a pool entry is allowed to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirTrigger {
    /// The mark starts the flight. Nobody has to be in the area.
    Clock,
    /// The mark arms the template Zone IN until the next mark.
    Zone,
}

impl AirTrigger {
    pub const ALL: [AirTrigger; 2] = [AirTrigger::Clock, AirTrigger::Zone];

    pub fn label(self) -> &'static str {
        match self {
            AirTrigger::Clock => "On a timer only",
            AirTrigger::Zone => "On a timer with check zone trigger",
        }
    }
}

/// Where the launch marks sit inside the mission window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AirSpread {
    #[default]
    Even,
    Front,
    Middle,
    End,
}

impl AirSpread {
    pub const ALL: [AirSpread; 4] = [
        AirSpread::Even,
        AirSpread::Front,
        AirSpread::Middle,
        AirSpread::End,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AirSpread::Even => "Even",
            AirSpread::Front => "Front",
            AirSpread::Middle => "Middle",
            AirSpread::End => "End",
        }
    }
}

impl Default for AirTrigger {
    fn default() -> Self {
        AirTrigger::Clock
    }
}

/// Clock, zone chance, budget, extension, and the mission window.
///
/// Defaults spawn the first sortie as soon as the mission starts. The fork
/// waited 10 minutes, skipped 20% of slots, and rolled a 50% zone chance.
/// Those values are legal here; they are not the default. The window is 180
/// minutes. A test mission can set it to 5–10.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AirOptions {
    pub trigger: AirTrigger,
    pub first_delay_min: u32,
    pub period_min: u32,
    pub quiet_pct: u32,
    pub zone_chance_pct: u32,
    pub budget: u32,
    /// Timer mode, once launches are allowed while a sortie is out.
    /// How many NATO flights can be airborne together.
    pub nato_active: u32,
    /// How many DPRK flights can be airborne together.
    pub dprk_active: u32,
    pub extension_min: u32,
    /// Package length. 180 is a 3 hour mission. 5–10 is a short test. 360 is the long end.
    pub window_min: u32,
    /// How many launch marks to spread across the window. 0 fills every mark that fits.
    pub launches: u32,
    /// Even uses the wait and the minutes between. The others pack the marks into that part of the window.
    pub spread: AirSpread,
    /// When set, a later mark can start another sortie that is not already out.
    pub launch_while_active: bool,
    /// Check-zone mode. 0 keeps the area open until the next mark. A shorter value closes it sooner.
    pub zone_open_min: u32,
}

impl AirOptions {
    /// Seconds until PACKAGE WINDOW stops the clock and closes zone plans.
    pub fn window_secs(&self) -> f64 {
        f64::from(self.window_min.clamp(5, 360)) * 60.0
    }

    /// How long a check zone stays open after its mark. 0 means until the next mark.
    pub fn zone_open_secs(&self) -> f64 {
        let period = f64::from(self.period_min.max(1)) * 60.0;
        if self.zone_open_min == 0 {
            period
        } else {
            f64::from(self.zone_open_min.min(self.period_min.max(1))) * 60.0
        }
    }
}

/// How many marks fit after the first wait, stepping by the minutes between, inside the window.
pub fn marks_that_fit(options: &AirOptions) -> u32 {
    let window = options.window_secs();
    let start = f64::from(options.first_delay_min) * 60.0;
    let period = f64::from(options.period_min.max(1)) * 60.0;
    if start >= window || period <= 0.0 {
        return 1;
    }
    let mut n = 0u32;
    let mut t = start;
    while t < window && n < 48 {
        n += 1;
        t += period;
    }
    n.max(1)
}

/// Launch marks actually drawn. A stored 0 means every mark that fits.
pub fn launch_count(options: &AirOptions) -> u32 {
    let cap = marks_that_fit(options);
    if options.launches == 0 {
        cap
    } else {
        options.launches.clamp(1, cap)
    }
}

/// Seconds from mission start for each launch mark.
///
/// Even spacing matches the wait and the minutes between. Front, middle, and
/// end pack that same count into that part of the window.
pub fn schedule_secs(options: &AirOptions) -> Vec<f64> {
    let cap = marks_that_fit(options);
    let count = launch_count(options);
    let start = f64::from(options.first_delay_min) * 60.0;
    let period = f64::from(options.period_min.max(1)) * 60.0;
    let end = start + f64::from(cap.saturating_sub(1)) * period;
    if count <= 1 {
        let t = match options.spread {
            AirSpread::End => end,
            AirSpread::Middle => start + (end - start) * 0.5,
            AirSpread::Even | AirSpread::Front => start,
        };
        return vec![t];
    }
    let span = (end - start).max(0.0);
    let (from, to) = match options.spread {
        AirSpread::Even => (0.0, 1.0),
        AirSpread::Front => (0.0, 0.5),
        AirSpread::Middle => (0.25, 0.75),
        AirSpread::End => (0.5, 1.0),
    };
    (0..count)
        .map(|i| {
            let u = i as f64 / f64::from(count - 1);
            start + span * (from + (to - from) * u)
        })
        .collect()
}

/// Route seconds at a waypoint speed in km/h. Distance is meters.
pub fn flight_secs(meters: f64, speed_kmh: f64) -> f64 {
    let speed = if speed_kmh < 1.0 { 100.0 } else { speed_kmh };
    if meters <= 0.0 {
        0.0
    } else {
        meters / (speed * 1000.0 / 3600.0)
    }
}

/// The sortie that owns this mark when only one flight estimate covers it.
/// None means the mark is a draw: several sorties overlap, or none do.
pub fn exclusive_plan(mark: f64, first: f64, durations: &[f64]) -> Option<usize> {
    if durations.iter().all(|d| *d <= 0.0) {
        return None;
    }
    let mut hit = Vec::new();
    for (i, duration) in durations.iter().enumerate() {
        if *duration > 0.0 && mark + 0.01 >= first && mark < first + duration {
            hit.push(i);
        }
    }
    if hit.len() == 1 { Some(hit[0]) } else { None }
}

impl Default for AirOptions {
    fn default() -> Self {
        Self {
            trigger: AirTrigger::Clock,
            first_delay_min: 0,
            period_min: 20,
            quiet_pct: 0,
            zone_chance_pct: 100,
            budget: 1,
            nato_active: 1,
            dprk_active: 1,
            extension_min: 0,
            window_min: 180,
            launches: 0,
            spread: AirSpread::Even,
            launch_while_active: false,
            zone_open_min: 0,
        }
    }
}

/// One historical sortie. The flight is built by the current strike template,
/// not by the 1950 file this name comes from.
#[derive(Clone, Debug)]
pub struct SortieDef {
    pub id: &'static str,
    pub name: &'static str,
    pub side: &'static str,
    pub script_part: &'static str,
    pub ships: u32,
    pub country: i32,
    pub skill: i32,
    pub nato: bool,
}

/// F-80, F-51, Yak-9, and Il-10 sorties named for the 1950 set.
pub fn builtin_library() -> &'static [SortieDef] {
    &[
        SortieDef {
            id: "f80_alert",
            name: "F-80C air alert",
            side: "NATO",
            script_part: "f80c10",
            ships: 4,
            country: 601,
            skill: 2,
            nato: true,
        },
        SortieDef {
            id: "f80_strike",
            name: "F-80C HVAR strike",
            side: "NATO",
            script_part: "f80c10",
            ships: 4,
            country: 601,
            skill: 2,
            nato: true,
        },
        SortieDef {
            id: "f51_cas",
            name: "F-51D close support",
            side: "NATO",
            script_part: "f51d",
            ships: 4,
            country: 601,
            skill: 2,
            nato: true,
        },
        SortieDef {
            id: "f51_recon",
            name: "F-51D armed recon",
            side: "NATO",
            script_part: "f51d",
            ships: 4,
            country: 601,
            skill: 2,
            nato: true,
        },
        SortieDef {
            id: "yak_raid",
            name: "Yak-9P airfield raid",
            side: "DPRK",
            script_part: "yak9p",
            ships: 4,
            country: 502,
            skill: 2,
            nato: false,
        },
        SortieDef {
            id: "il10_strike",
            name: "Il-10 strike",
            side: "DPRK",
            script_part: "il10",
            ships: 4,
            country: 502,
            skill: 2,
            nato: false,
        },
    ]
}

/// Where sortie `index` sits when the user has not picked a map objective.
pub fn default_target(index: usize) -> (f64, f64) {
    (200_000.0 + index as f64 * 25_000.0, 150_000.0)
}

/// Build one strike with the current template. `target` is the Zone IN point.
pub fn build_sortie(def: &SortieDef, target: (f64, f64)) -> Result<Il2Entity, String> {
    let unit = catalog_plane(def.script_part)?;
    let ships = def.ships.max(1);
    let mut seats = Vec::new();
    append_seat(&mut seats, unit.clone(), ships);
    seats[0].role = FlightRole::Lead;
    seats[0].country = def.country;
    seats[0].skill = def.skill;
    for _ in 1..ships {
        append_seat(&mut seats, unit.clone(), ships);
    }
    for seat in &mut seats {
        seat.country = def.country;
        seat.skill = def.skill;
    }
    apply_mission_package(&mut seats, 0, MissionPackage::Strike, 8_000.0)?;
    let mut opts = TemplateOptions::default();
    opts.name = def.name.to_string();
    opts.seats = seats;
    opts.per_group = ships;
    opts.zone_out_enabled = false;
    // The zone watches the other side. A NATO flight watches DPRK.
    opts.zone_coalition = if def.nato {
        ZoneCoalition::Eastern
    } else {
        ZoneCoalition::Western
    };
    let mut group = generate_template(&opts)?;
    if let Some(zone_id) = zone_in_ids(&group).first().copied() {
        if let Some(zone) = find_ref(&group, zone_id) {
            let (x, _, z) = pos_of(zone);
            group.translate_xz(target.0 - x, target.1 - z);
        }
    }
    Ok(group)
}

/// Build one CAP patrol with the current template. `center` is the Zone IN point.
/// The four corners and the RTB waypoint stay where the profile put them.
pub fn build_cap(def: &SortieDef, center: (f64, f64)) -> Result<Il2Entity, String> {
    let unit = catalog_plane(def.script_part)?;
    let ships = def.ships.max(1);
    let mut seats = Vec::new();
    append_seat(&mut seats, unit.clone(), ships);
    seats[0].role = FlightRole::Lead;
    seats[0].country = def.country;
    seats[0].skill = def.skill;
    for _ in 1..ships {
        append_seat(&mut seats, unit.clone(), ships);
    }
    for seat in &mut seats {
        seat.country = def.country;
        seat.skill = def.skill;
    }
    apply_mission_package(&mut seats, 0, MissionPackage::Cap, 8_000.0)?;
    let mut opts = TemplateOptions::default();
    opts.name = def.name.to_string();
    opts.seats = seats;
    opts.per_group = ships;
    opts.zone_coalition = if def.nato {
        ZoneCoalition::Eastern
    } else {
        ZoneCoalition::Western
    };
    let mut group = generate_template(&opts)?;
    if !center_cap_on(&mut group, center.0, center.1) {
        return Err("the CAP has no Zone IN to place".into());
    }
    Ok(group)
}

#[derive(Debug, Clone)]
pub struct AirPlanInfo {
    pub name: String,
    pub zone_count: usize,
    pub end_name: String,
}

#[derive(Debug, Clone)]
pub struct AirTaskInput {
    pub label: String,
    pub root: Il2Entity,
    /// Relative chance inside one stage. 1 is an even share.
    pub weight: i32,
    /// Route time in seconds. 0 means the draw does not use an overlap.
    pub duration_s: f64,
    /// How many times this path fires. Each firing is its own flight.
    pub runs: u32,
    /// Where those firings sit in the mission window.
    pub spread: AirSpread,
}

/// Marks for one sortie. `runs` is how many times that path fires.
pub fn sortie_marks(options: &AirOptions, runs: u32, spread: AirSpread) -> Vec<f64> {
    let mut local = options.clone();
    local.launches = runs.max(1);
    local.spread = spread;
    schedule_secs(&local)
}

/// The template Mission Begin pulses Zone IN on its own. Enabled does not stop
/// a Mission Begin, so the targets are cleared and only the tasking clock can
/// start the flight.
fn silence_template_begin(root: &mut Il2Entity) {
    root.for_each_mut(&mut |entity| {
        if entity.block_type == "MCU_TR_MissionBegin" {
            entity.set_targets(Vec::new());
        }
    });
}

/// Zone IN plus an end timer, or an error that names which one is missing.
pub fn inspect_air_plan(root: &Il2Entity) -> Result<AirPlanInfo, String> {
    let zones = zone_in_ids(root);
    if zones.is_empty() {
        return Err("this group has no Zone IN".into());
    }
    let Some(end) = end_timer_id(root) else {
        return Err("this group has no end timer".into());
    };
    let end_name = find_ref(root, end)
        .and_then(|e| e.name())
        .unwrap_or("end timer")
        .to_string();
    Ok(AirPlanInfo {
        name: root.name().unwrap_or("Mission").to_string(),
        zone_count: zones.len(),
        end_name,
    })
}

fn mark_buckets(times: &[f64]) -> Vec<(f64, Vec<usize>)> {
    let mut buckets: Vec<(f64, Vec<usize>)> = Vec::new();
    for (index, time) in times.iter().enumerate() {
        if let Some((_, members)) = buckets
            .iter_mut()
            .find(|(stamp, _)| (*stamp - *time).abs() < 0.5)
        {
            members.push(index);
        } else {
            buckets.push((*time, vec![index]));
        }
    }
    buckets
}

struct MarkChain {
    mark: i32,
    hold: Vec<i32>,
}

/// One launch attempt. Several flights that share the time roll once, and the
/// winner is the only gate this mark starts.
fn wire_mark(
    logic: &mut Vec<Il2Entity>,
    next_id: &mut i32,
    options: &AirOptions,
    gates: &[i32],
    weights: &[i32],
    time: f64,
    members: &[usize],
    ordinal: usize,
    classic: bool,
    place: (f64, f64, f64),
) -> MarkChain {
    let (x, y, z) = place;
    let mut hold = Vec::new();
    let dest = if members.len() >= 2 {
        let draw = wire_draw(
            logic,
            next_id,
            gates,
            weights,
            members,
            ordinal,
            classic,
            (x, y, z),
        );
        hold.push(draw);
        draw
    } else {
        gates[members[0]]
    };
    let quiet_name = if classic {
        QUIET_SLOT.to_string()
    } else {
        format!("Quiet Slot {ordinal}")
    };
    let chance_name = if classic {
        ZONE_CHANCE.to_string()
    } else {
        format!("Zone Chance {ordinal}")
    };
    let mut quiet = timer(
        &quiet_name,
        0.2,
        options.quiet_pct as i32,
        true,
        next_id,
        x,
        y,
        z + 60.0,
    );
    let quiet_id = quiet.index.unwrap();
    let mut quiet_pass = timer(
        &format!("Quiet Pass {ordinal}"),
        0.5,
        100,
        true,
        next_id,
        x,
        y,
        z + 80.0,
    );
    let quiet_pass_id = quiet_pass.index.unwrap();
    let mut off_quiet = deactivate(
        &format!("Off Quiet Pass {ordinal}"),
        next_id,
        x,
        y,
        z + 100.0,
    );
    affect(&mut off_quiet, vec![quiet_pass_id]);
    let off_quiet_id = off_quiet.index.unwrap();
    stash(logic, off_quiet);
    let mut chance = timer(
        &chance_name,
        0.2,
        options.zone_chance_pct as i32,
        true,
        next_id,
        x,
        y,
        z + 120.0,
    );
    let chance_id = chance.index.unwrap();
    let mut miss = timer(
        &format!("Zone Miss {ordinal}"),
        0.5,
        100,
        true,
        next_id,
        x,
        y,
        z + 140.0,
    );
    let miss_id = miss.index.unwrap();
    let mut off_miss = deactivate(
        &format!("Off Zone Miss {ordinal}"),
        next_id,
        x,
        y,
        z + 160.0,
    );
    affect(&mut off_miss, vec![miss_id]);
    let off_miss_id = off_miss.index.unwrap();
    stash(logic, off_miss);
    chance.set_targets(vec![off_miss_id, dest]);
    miss.set_targets(Vec::new());
    quiet.set_targets(vec![off_quiet_id]);
    quiet_pass.set_targets(vec![chance_id, miss_id]);
    logic.push(quiet);
    logic.push(quiet_pass);
    logic.push(chance);
    logic.push(miss);
    let mut launch = timer(
        &format!("Launch {ordinal}"),
        0.0,
        100,
        true,
        next_id,
        x,
        y,
        z + 180.0,
    );
    launch.set_targets(vec![quiet_id, quiet_pass_id]);
    let launch_id = stash(logic, launch);
    hold.push(launch_id);
    let mut mark = timer(
        &format!("Mark {ordinal}"),
        time,
        100,
        true,
        next_id,
        x,
        y,
        z,
    );
    mark.set_targets(vec![launch_id]);
    MarkChain {
        mark: stash(logic, mark),
        hold,
    }
}

fn wire_draw(
    logic: &mut Vec<Il2Entity>,
    next_id: &mut i32,
    gates: &[i32],
    weights: &[i32],
    members: &[usize],
    ordinal: usize,
    classic: bool,
    place: (f64, f64, f64),
) -> i32 {
    let (x, y, z) = place;
    let subset: Vec<i32> = members.iter().map(|index| weights[*index].max(1)).collect();
    let n = members.len();
    let mut roll = Vec::with_capacity(n);
    let mut pass = Vec::with_capacity(n);
    let mut claim = Vec::with_capacity(n);
    let mut sweep = Vec::with_capacity(n);
    let mut skip = Vec::with_capacity(n);
    for (local, member) in members.iter().enumerate() {
        let plan = member + 1;
        let roll_name = if classic {
            format!("Stage 1 Plan {plan} Roll")
        } else {
            format!("Mark {ordinal} Plan {plan} Roll")
        };
        let pct = weighted_pct(local, &subset);
        let col = *member;
        roll.push(stash(
            logic,
            timer(
                &roll_name,
                0.2,
                pct,
                true,
                next_id,
                x + col as f64 * 50.0,
                y,
                z + 200.0,
            ),
        ));
        pass.push(stash(
            logic,
            timer(
                &format!("Stage 1 Plan {plan} Pass"),
                0.5,
                100,
                true,
                next_id,
                x + col as f64 * 50.0,
                y,
                z + 208.0,
            ),
        ));
        claim.push(stash(
            logic,
            timer(
                &format!("Stage 1 Plan {plan} Claim"),
                0.0,
                100,
                true,
                next_id,
                x + col as f64 * 50.0,
                y,
                z + 216.0,
            ),
        ));
        sweep.push(stash(
            logic,
            timer(
                &format!("Stage 1 Sweep {plan}"),
                0.2,
                100,
                true,
                next_id,
                x + col as f64 * 50.0,
                y,
                z + 224.0,
            ),
        ));
        skip.push(stash(
            logic,
            timer(
                &format!("Stage 1 Sweep {plan} Skip"),
                0.5,
                100,
                true,
                next_id,
                x + col as f64 * 50.0,
                y,
                z + 232.0,
            ),
        ));
    }
    let mut rearm_targets: Vec<i32> = Vec::new();
    rearm_targets.extend(
        roll.iter()
            .chain(pass.iter())
            .chain(sweep.iter())
            .chain(skip.iter())
            .copied(),
    );
    let mut offs = Vec::new();
    for local in 0..n {
        let plan = members[local] + 1;
        let mut off_pass = deactivate(
            &format!("Off Stage 1 Plan {plan} Pass"),
            next_id,
            x,
            y,
            z + 250.0,
        );
        affect(&mut off_pass, vec![pass[local]]);
        let off_pass = stash(logic, off_pass);
        let mut off_skip = deactivate(
            &format!("Off Stage 1 Sweep {plan} Skip"),
            next_id,
            x,
            y,
            z + 260.0,
        );
        affect(&mut off_skip, vec![skip[local]]);
        let off_skip = stash(logic, off_skip);
        offs.push((off_pass, off_skip));
    }
    let sweep_name = if classic {
        "Stage 1 Stop Sweep".to_string()
    } else {
        format!("Stage 1 Stop Sweep {ordinal}")
    };
    let mut stop_sweep = deactivate(&sweep_name, next_id, x, y, z + 270.0);
    affect(&mut stop_sweep, sweep.clone());
    let stop_sweep = stash(logic, stop_sweep);
    let rearm_name = if classic {
        "Rearm Draw".to_string()
    } else {
        format!("Rearm Draw {ordinal}")
    };
    let mut rearm = activate(&rearm_name, next_id, x, y, z + 290.0);
    affect(&mut rearm, rearm_targets);
    let rearm = stash(logic, rearm);
    let rearm_wait = stash(
        logic,
        timer(
            &format!("Rearm Draw Wait {ordinal}"),
            0.5,
            100,
            true,
            next_id,
            x,
            y,
            z + 300.0,
        ),
    );
    set_targets_on(logic, rearm_wait, vec![rearm]);
    for local in 0..n {
        let (off_pass, off_skip) = offs[local];
        let next_pass = if local + 1 < n {
            vec![roll[local + 1], pass[local + 1]]
        } else {
            vec![sweep[0], skip[0]]
        };
        let next_skip = if local + 1 < n {
            vec![sweep[local + 1], skip[local + 1]]
        } else {
            Vec::new()
        };
        set_targets_on(logic, roll[local], vec![off_pass, stop_sweep, claim[local]]);
        set_targets_on(logic, pass[local], next_pass);
        set_targets_on(logic, sweep[local], vec![off_skip, claim[local]]);
        set_targets_on(logic, skip[local], next_skip);
        set_targets_on(logic, claim[local], vec![gates[members[local]], rearm_wait]);
    }
    let draw_name = if classic {
        "Draw".to_string()
    } else {
        format!("Draw {ordinal}")
    };
    let mut draw = timer(&draw_name, 0.0, 100, true, next_id, x, y, z + 320.0);
    draw.set_targets(vec![roll[0], pass[0]]);
    stash(logic, draw)
}

/// Credits for one side. A launch spends the first free credit and starts that
/// plan 0.1 s later. The plan's end gives one credit back. While every credit
/// is spent, the next mark arms nothing and the flight stays on the ground.
fn wire_side_active(
    logic: &mut Vec<Il2Entity>,
    next_id: &mut i32,
    x: f64,
    y: f64,
    z: f64,
    side: &str,
    plans: &[usize],
    slots: u32,
    gates: &[i32],
    starts: &[i32],
) -> (Vec<i32>, Vec<i32>) {
    let mut stop = Vec::new();
    if plans.is_empty() {
        return (stop, Vec::new());
    }
    let slots = (slots.max(1) as usize).min(plans.len());
    let mut credits = Vec::with_capacity(slots);
    let mut passes = Vec::new();
    let mut restores = Vec::with_capacity(slots);
    for i in 0..slots {
        let credit = timer(
            &format!("{side} Credit {}", i + 1),
            0.0,
            100,
            true,
            next_id,
            x,
            y,
            z + i as f64 * 12.0,
        );
        credits.push(stash(logic, credit));
        let restore = timer(
            &format!("{side} Restore {}", i + 1),
            0.0,
            100,
            false,
            next_id,
            x + 20.0,
            y,
            z + i as f64 * 12.0,
        );
        restores.push(stash(logic, restore));
        if i + 1 < slots {
            let pass = timer(
                &format!("{side} Pass {}", i + 1),
                0.0,
                100,
                true,
                next_id,
                x + 40.0,
                y,
                z + i as f64 * 12.0,
            );
            passes.push(stash(logic, pass));
        }
    }
    let mut launch = timer(
        &format!("{side} Launch"),
        0.1,
        100,
        true,
        next_id,
        x,
        y,
        z + 80.0,
    );
    let mut starters = Vec::with_capacity(plans.len());
    for &p in plans {
        let starter = timer(
            &format!("Plan {} Starter", p + 1),
            0.0,
            100,
            false,
            next_id,
            x + 60.0,
            y,
            z + p as f64 * 12.0,
        );
        starters.push(stash(logic, starter));
    }
    launch.set_targets(starters.clone());
    let launch_id = stash(logic, launch);
    stop.extend(credits.iter().copied());
    stop.extend(passes.iter().copied());
    stop.extend(restores.iter().copied());
    stop.push(launch_id);
    stop.extend(starters.iter().copied());

    for i in 0..slots {
        let mut off = vec![credits[i]];
        if i < passes.len() {
            off.push(passes[i]);
        }
        let mut spend = deactivate(
            &format!("{side} Spend {}", i + 1),
            next_id,
            x + 120.0,
            y,
            z + i as f64 * 12.0,
        );
        affect(&mut spend, off);
        let spend_id = stash(logic, spend);
        let mut arm_restore = activate(
            &format!("{side} Arm Restore {}", i + 1),
            next_id,
            x + 130.0,
            y,
            z + i as f64 * 12.0,
        );
        affect(&mut arm_restore, vec![restores[i]]);
        let arm_restore_id = stash(logic, arm_restore);
        let mut credit_targets = vec![spend_id, arm_restore_id, launch_id];
        if i < passes.len() {
            let mut rearm = timer(
                &format!("{side} Rearm {}", i + 1),
                0.1,
                100,
                true,
                next_id,
                x + 80.0,
                y,
                z + i as f64 * 12.0,
            );
            let mut on = activate(
                &format!("{side} Rearm Pass {}", i + 1),
                next_id,
                x + 100.0,
                y,
                z + i as f64 * 12.0,
            );
            affect(&mut on, vec![passes[i]]);
            let on_id = stash(logic, on);
            rearm.set_targets(vec![on_id]);
            let rearm_id = stash(logic, rearm);
            credit_targets.push(rearm_id);
            stop.push(rearm_id);
            stop.push(on_id);
            let mut hop = vec![credits[i + 1]];
            if i + 1 < passes.len() {
                hop.push(passes[i + 1]);
            }
            set_targets_on(logic, passes[i], hop);
        }
        set_targets_on(logic, credits[i], credit_targets);
        stop.push(spend_id);
        stop.push(arm_restore_id);

        let mut spent = vec![restores[i]];
        spent.extend(restores[i + 1..].iter().copied());
        let mut close_restore = deactivate(
            &format!("{side} Close Restore {}", i + 1),
            next_id,
            x + 150.0,
            y,
            z + i as f64 * 12.0,
        );
        affect(&mut close_restore, spent);
        let close_id = stash(logic, close_restore);
        let mut give = activate(
            &format!("{side} Give {}", i + 1),
            next_id,
            x + 140.0,
            y,
            z + i as f64 * 12.0,
        );
        affect(&mut give, vec![credits[i]]);
        let give_id = stash(logic, give);
        let mut restore_targets = vec![close_id, give_id];
        if i + 1 < slots {
            let mut rearm_restore = timer(
                &format!("{side} Rearm Restore {}", i + 1),
                0.1,
                100,
                true,
                next_id,
                x + 160.0,
                y,
                z + i as f64 * 12.0,
            );
            let mut on = activate(
                &format!("{side} Rearm Restore On {}", i + 1),
                next_id,
                x + 180.0,
                y,
                z + i as f64 * 12.0,
            );
            affect(&mut on, restores[i + 1..].to_vec());
            let on_id = stash(logic, on);
            rearm_restore.set_targets(vec![on_id]);
            let rearm_id = stash(logic, rearm_restore);
            restore_targets.push(rearm_id);
            stop.push(rearm_id);
            stop.push(on_id);
        }
        set_targets_on(logic, restores[i], restore_targets);
        stop.push(close_id);
        stop.push(give_id);
    }

    for (nth, &p) in plans.iter().enumerate() {
        let mut arm = activate(
            &format!("Arm Plan {}", p + 1),
            next_id,
            x + 200.0,
            y,
            z + nth as f64 * 12.0,
        );
        affect(&mut arm, vec![starters[nth]]);
        let arm_id = stash(logic, arm);
        let mut cancel = timer(
            &format!("Plan {} Cancel", p + 1),
            0.2,
            100,
            true,
            next_id,
            x + 220.0,
            y,
            z + nth as f64 * 12.0,
        );
        let mut clear = deactivate(
            &format!("Plan {} Clear", p + 1),
            next_id,
            x + 240.0,
            y,
            z + nth as f64 * 12.0,
        );
        affect(&mut clear, vec![starters[nth]]);
        let clear_id = stash(logic, clear);
        cancel.set_targets(vec![clear_id]);
        let cancel_id = stash(logic, cancel);
        set_targets_on(logic, starters[nth], vec![starts[p], clear_id]);
        let mut entry = vec![arm_id, credits[0], cancel_id];
        if !passes.is_empty() {
            entry.insert(2, passes[0]);
        }
        set_targets_on(logic, gates[p], entry);
        stop.push(arm_id);
        stop.push(cancel_id);
        stop.push(clear_id);
    }
    (stop, restores)
}

/// On a timer only. Each mark is an absolute time. A mark that arrives while
/// the cap of sorties is already out is missed. The next mark is not delayed.
fn link_timer_only(plans: &[AirTaskInput], options: &AirOptions) -> Result<Il2Entity, String> {
    let n = plans.len();
    let mut next_id = plans
        .iter()
        .map(|p| p.root.max_index())
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    struct Copy {
        root: Il2Entity,
        end: i32,
        bring: Vec<i32>,
        time: f64,
        weight: i32,
        route_s: f64,
        cap: bool,
        nato: bool,
    }
    let mut copies: Vec<Copy> = Vec::with_capacity(n);
    for (i, plan) in plans.iter().enumerate() {
        if zone_in_ids(&plan.root).is_empty() {
            return Err(format!("plan {} has no Zone IN", i + 1));
        }
        if end_timer_id(&plan.root).is_none() {
            return Err(format!("plan {} has no end timer", i + 1));
        }
        let base = plan.root.name().unwrap_or(plan.label.as_str()).to_string();
        let times = sortie_marks(options, plan.runs, plan.spread);
        for (k, time) in times.iter().enumerate() {
            let (mut clone, id_map) = duplicate_template(&plan.root, &mut next_id);
            silence_template_begin(&mut clone);
            let cap = crate::template::is_cap_group(&clone);
            if cap {
                // The clock starts the flight. Zone IN still watches the other
                // side, and Zone Out no longer ends the patrol when the area
                // is empty. The package window closes both and runs cleanup.
                rewire_cap_timer_zones(&mut clone);
            }
            if times.len() == 1 {
                clone.set_name(&base);
            } else if k == 0 {
                clone.set_name(&base);
            } else {
                clone.set_name(&format!("{base} {}", k + 1));
            }
            for id in zone_in_ids(&clone) {
                if let Some(zone) = find_mut(&mut clone, id) {
                    zone.set_property("Enabled", "0");
                }
            }
            let end = *id_map
                .get(&end_timer_id(&plan.root).unwrap())
                .ok_or_else(|| format!("plan {} end timer was lost while cloning", i + 1))?;
            let bring = bring_up_ids(&clone);
            if bring.is_empty() {
                return Err(format!("plan {} has no bring-up timer", i + 1));
            }
            copies.push(Copy {
                time: *time,
                weight: plan.weight.max(1),
                route_s: plan.duration_s,
                bring,
                end,
                cap,
                nato: plan_is_nato(&clone),
                root: clone,
            });
        }
    }

    let n = copies.len();
    let anchor = copies[0]
        .root
        .find_by_name("Zone IN")
        .map(pos_of)
        .unwrap_or((0.0, 0.0, 0.0));
    let (ax, ay, az) = anchor;
    let at = |col: usize, row: usize| (ax + col as f64 * 50.0, ay, az + 200.0 + row as f64 * 30.0);
    let mut logic: Vec<Il2Entity> = Vec::new();
    let mut gate = Vec::with_capacity(n);
    let mut start = Vec::with_capacity(n);
    for p in 0..n {
        let (x, y, z) = at(p, 0);
        let mut started = timer(
            &format!("Plan {} Start", p + 1),
            0.1,
            100,
            true,
            &mut next_id,
            x,
            y,
            z,
        );
        let mut start_targets = copies[p].bring.clone();
        if copies[p].cap {
            // Arm the check after Disarm At Start. PULSE OUT would enable
            // Zone Out, and an empty Zone Out used to end the flight.
            start_targets.extend(ids_named(
                &copies[p].root,
                "MCU_Activate",
                "Zone In ReActivate",
            ));
            start_targets.extend(ids_named(&copies[p].root, "MCU_Timer", "CAP Shift"));
        } else {
            // Time on Target is armed by PULSE OUT, not by the bring-up timer.
            // Without it the flight attacks until a failsafe, and the home
            // waypoint never gets the order.
            start_targets.extend(pulse_out_ids(&copies[p].root));
        }
        started.set_targets(start_targets);
        start.push(stash(&mut logic, started));
        let gated = timer(
            &format!("Plan {} Gate", p + 1),
            0.0,
            100,
            true,
            &mut next_id,
            x,
            y,
            z + 20.0,
        );
        gate.push(stash(&mut logic, gated));
    }

    let (x, y, z) = at(0, 2);
    let weights: Vec<i32> = copies.iter().map(|copy| copy.weight).collect();
    let times: Vec<f64> = copies.iter().map(|copy| copy.time).collect();
    let buckets = mark_buckets(&times);
    let mut held = Vec::new();
    let mut mark_ids = Vec::new();
    for (index, (time, members)) in buckets.iter().enumerate() {
        let chain = wire_mark(
            &mut logic,
            &mut next_id,
            options,
            &gate,
            &weights,
            *time,
            members,
            index + 1,
            buckets.len() == 1,
            (x, y, z + index as f64 * 40.0),
        );
        mark_ids.push(chain.mark);
        held.extend(chain.hold);
    }
    held.extend(gate.iter().copied());
    let mut release_ids = Vec::new();
    if options.launch_while_active {
        let nato: Vec<usize> = (0..n).filter(|&p| copies[p].nato).collect();
        let dprk: Vec<usize> = (0..n).filter(|&p| !copies[p].nato).collect();
        let (nx, ny, nz) = at(0, 6);
        let (nato_stop, nato_restore) = wire_side_active(
            &mut logic,
            &mut next_id,
            nx,
            ny,
            nz,
            "NATO",
            &nato,
            options.nato_active,
            &gate,
            &start,
        );
        let (dx, dy, dz) = at(4, 6);
        let (dprk_stop, dprk_restore) = wire_side_active(
            &mut logic,
            &mut next_id,
            dx,
            dy,
            dz,
            "DPRK",
            &dprk,
            options.dprk_active,
            &gate,
            &start,
        );
        for (plans, restores) in [(&nato, &nato_restore), (&dprk, &dprk_restore)] {
            for &p in plans {
                let end = copies[p].end;
                if let Some(node) = find_mut(&mut copies[p].root, end) {
                    for id in restores {
                        node.append_target(*id);
                    }
                }
            }
        }
        release_ids.extend(nato_stop);
        release_ids.extend(dprk_stop);
    } else {
        let mut hold = deactivate("Hold Launches", &mut next_id, x, y, z + 140.0);
        affect(&mut hold, held.clone());
        let hold_id = stash(&mut logic, hold);
        for p in 0..n {
            set_targets_on(&mut logic, gate[p], vec![start[p], hold_id]);
        }
        let mut release = activate("Release Launches", &mut next_id, x, y, z + 200.0);
        affect(&mut release, held.clone());
        let release_id = stash(&mut logic, release);
        release_ids.push(release_id);
        for copy in &mut copies {
            if let Some(node) = find_mut(&mut copy.root, copy.end) {
                node.append_target(release_id);
            }
        }
    }

    let mut stop_ids = held;
    stop_ids.extend(release_ids);
    stop_ids.extend(mark_ids.iter().copied());
    let mut stop = deactivate(STOP_NAME, &mut next_id, x, y, z + 240.0);
    affect(&mut stop, stop_ids);
    let stop_id = stash(&mut logic, stop);
    let mut close = deactivate("Close Zones", &mut next_id, x, y, z + 260.0);
    let mut zones: Vec<i32> = copies.iter().flat_map(|c| zone_in_ids(&c.root)).collect();
    for copy in &copies {
        if copy.cap {
            zones.extend(zone_out_ids(&copy.root));
        }
    }
    affect(&mut close, zones);
    let close_id = stash(&mut logic, close);
    let mut window = timer(
        WINDOW_NAME,
        options.window_secs(),
        100,
        true,
        &mut next_id,
        x,
        y,
        z + 280.0,
    );
    let mut window_targets = vec![stop_id, close_id];
    for copy in &copies {
        if copy.cap {
            window_targets.push(copy.end);
        }
    }
    window.set_targets(window_targets);
    let window_id = stash(&mut logic, window);
    let mut trace = timer(TRACE_NAME, 0.0, 100, true, &mut next_id, x, y, z + 300.0);
    let route_s = copies.iter().map(|copy| copy.route_s).fold(0.0, f64::max);
    trace.set_property(
        "Desc",
        &format!(
            "\"Timer only. Each mark spawns with no check zone. A mark while the cap is full is missed. Longest route {route_s:.0} s.\""
        ),
    );
    let trace_id = stash(&mut logic, trace);
    let mut begin = mcu(
        "MCU_TR_MissionBegin",
        BEGIN_NAME,
        &mut next_id,
        x,
        y,
        z + 320.0,
    );
    begin.set_property("Enabled", "1");
    let mut begin_targets = vec![trace_id, window_id];
    begin_targets.extend(mark_ids);
    begin.set_targets(begin_targets);
    logic.push(begin);

    let mut logic_group = Il2Entity::new("Group");
    logic_group.index = Some(next_id);
    logic_group.set_property("Index", next_id.to_string());
    next_id += 1;
    logic_group.set_name(LOGIC_GROUP);
    logic_group.set_property("Desc", "\"\"");
    logic_group.children = logic;
    let _ = next_id;

    let mut out = Il2Entity::new("Group");
    out.index = Some(next_id);
    out.set_property("Index", next_id.to_string());
    out.set_name("Air Tasking");
    out.set_property("Desc", "\"\"");
    out.children = copies.into_iter().map(|c| c.root).collect();
    out.children.push(logic_group);
    disarm_illegal_enabled(&mut out);
    Ok(out)
}

/// Timer mode owns the launch and the cleanup. Zone IN still watches the
/// other coalition, but it does not bring the flight up. Zone Out still
/// turns itself off when that check clears, and it does not end the patrol
/// or arm the check again. The package window closes both zones and pulses
/// MISSION END.
fn rewire_cap_timer_zones(root: &mut Il2Entity) {
    let bring = bring_up_ids(root);
    let pulse = pulse_out_ids(root);
    let mission_end = root.find_by_name("MISSION END").and_then(|e| e.index);
    let cooldown = root.find_by_name("COOLDOWN").and_then(|e| e.index);
    let react_in = root
        .find_by_name("Zone In ReActivate")
        .and_then(|e| e.index);
    if let Some(zone) = root.find_by_name_mut("Zone IN") {
        zone.targets
            .retain(|id| !bring.contains(id) && !pulse.contains(id));
    }
    if let Some(zone) = root.find_by_name_mut("Zone Out") {
        zone.targets.retain(|id| {
            Some(*id) != mission_end && Some(*id) != cooldown && Some(*id) != react_in
        });
        zone.set_property("Enabled", "0");
    }
}

fn ids_named(root: &Il2Entity, block: &str, name: &str) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |entity| {
        if entity.block_type == block && entity.name() == Some(name) {
            if let Some(id) = entity.index {
                ids.push(id);
            }
        }
    });
    ids
}

fn zone_out_ids(root: &Il2Entity) -> Vec<i32> {
    ids_named(root, "MCU_CheckZone", "Zone Out")
}

fn bring_up_ids(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_Timer"
            && matches!(e.name(), Some("MISSION BEGIN") | Some("SPAWN UNITS"))
        {
            if let Some(id) = e.index {
                ids.push(id);
            }
        }
    });
    ids
}

fn pulse_out_ids(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_Timer" && e.name() == Some("PULSE OUT") {
            if let Some(id) = e.index {
                ids.push(id);
            }
        }
    });
    ids
}

/// Clone `plans` into one Air Tasking group and wire the queue.
pub fn link_air_task(plans: &[AirTaskInput], options: &AirOptions) -> Result<Il2Entity, String> {
    if plans.is_empty() {
        return Err("add at least one sortie".into());
    }
    let options = clamped(options);
    if options.trigger == AirTrigger::Clock {
        return link_timer_only(plans, &options);
    }
    let n = plans.len();
    let mut wanted: Vec<(Vec<i32>, i32)> = Vec::with_capacity(n);
    for (i, plan) in plans.iter().enumerate() {
        let zones = zone_in_ids(&plan.root);
        if zones.is_empty() {
            return Err(format!("plan {} has no Zone IN", i + 1));
        }
        let end =
            end_timer_id(&plan.root).ok_or_else(|| format!("plan {} has no end timer", i + 1))?;
        wanted.push((zones, end));
    }

    let mut next_id = plans
        .iter()
        .map(|p| p.root.max_index())
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    let anchor = find_ref(&plans[0].root, wanted[0].0[0])
        .map(pos_of)
        .unwrap_or((0.0, 0.0, 0.0));

    struct Copy {
        root: Il2Entity,
        zones: Vec<i32>,
        end: i32,
        nato: bool,
        /// What Zone IN would start. A timer launch fires these directly.
        start_targets: Vec<i32>,
    }
    let mut copies: Vec<Copy> = Vec::with_capacity(n);
    for (i, plan) in plans.iter().enumerate() {
        let (mut clone, id_map) = duplicate_template(&plan.root, &mut next_id);
        let base = plan.root.name().unwrap_or(plan.label.as_str());
        let copies_of_base = plans
            .iter()
            .filter(|other| other.root.name().unwrap_or(other.label.as_str()) == base)
            .count();
        if copies_of_base > 1 {
            let ord = plans
                .iter()
                .take(i + 1)
                .filter(|other| other.root.name().unwrap_or(other.label.as_str()) == base)
                .count();
            clone.set_name(&format!("{base} [{ord}]"));
        } else {
            clone.set_name(base);
        }
        silence_template_begin(&mut clone);
        let zones = map_ids(&wanted[i].0, &id_map)
            .ok_or_else(|| format!("plan {} Zone IN was lost while cloning", i + 1))?;
        let end = *id_map
            .get(&wanted[i].1)
            .ok_or_else(|| format!("plan {} end timer was lost while cloning", i + 1))?;
        // The template Mission Begin no longer pulses Zone IN. A Mission Begin
        // still runs when Enabled is 0, so its targets are cleared instead.
        for id in &zones {
            if let Some(zone) = find_mut(&mut clone, *id) {
                zone.set_property("Enabled", "0");
            }
        }
        let start_targets = zones
            .iter()
            .filter_map(|id| find_ref(&clone, *id).map(|zone| zone.targets.clone()))
            .flatten()
            .collect();
        copies.push(Copy {
            nato: plan_is_nato(&clone),
            start_targets,
            root: clone,
            zones,
            end,
        });
    }

    let weights: Vec<i32> = plans.iter().map(|p| p.weight.max(1)).collect();
    let (ax, ay, az) = anchor;
    let at = |col: usize, row: usize| (ax + col as f64 * 50.0, ay, az + 200.0 + row as f64 * 30.0);

    let mut logic: Vec<Il2Entity> = Vec::new();

    // Per-plan open, live, budget gate, and player area.
    let mut open_zone = Vec::with_capacity(n);
    let mut pulse_zone = Vec::with_capacity(n);
    let mut live = Vec::with_capacity(n);
    let mut arm_live = Vec::with_capacity(n);
    let mut pause_live = Vec::with_capacity(n);
    let mut extend = Vec::with_capacity(n);
    let mut gate = Vec::with_capacity(n);
    let mut slot = Vec::with_capacity(n);
    let mut arm_slot = Vec::with_capacity(n);
    let mut disarm_live = Vec::with_capacity(n);
    // Zone mode: these fire when a player enters, not when the mark only arms the area.
    let mut launch_on_zone: Vec<(usize, Vec<i32>)> = Vec::new();

    for p in 0..n {
        let (x, y, z) = at(p, 0);
        let mut open = activate(&format!("Plan {} Open Zone", p + 1), &mut next_id, x, y, z);
        affect(&mut open, copies[p].zones.clone());
        open_zone.push(stash(&mut logic, open));
        let mut pulse = timer(
            &format!("Plan {} Pulse Zone", p + 1),
            0.1,
            100,
            true,
            &mut next_id,
            x,
            y,
            z + 10.0,
        );
        pulse.set_targets(copies[p].zones.clone());
        pulse_zone.push(stash(&mut logic, pulse));
        let mut lived = timer(
            &format!("Plan {} Live", p + 1),
            1.0,
            100,
            false,
            &mut next_id,
            x,
            y,
            z + 20.0,
        );
        lived.set_targets(vec![copies[p].end]);
        live.push(stash(&mut logic, lived));
        let mut arm = activate(
            &format!("Plan {} Arm Live", p + 1),
            &mut next_id,
            x,
            y,
            z + 30.0,
        );
        affect(&mut arm, vec![live[p]]);
        arm_live.push(stash(&mut logic, arm));
        let mut pause = deactivate(
            &format!("Plan {} Pause Live", p + 1),
            &mut next_id,
            x,
            y,
            z + 40.0,
        );
        affect(&mut pause, vec![live[p]]);
        pause_live.push(stash(&mut logic, pause));
        let mut ext = timer(
            &format!("Plan {} {EXTEND_WORD}", p + 1),
            (options.extension_min as f64) * 60.0,
            100,
            true,
            &mut next_id,
            x,
            y,
            z + 50.0,
        );
        ext.set_targets(vec![arm_live[p]]);
        extend.push(stash(&mut logic, ext));
        let mut gated = timer(
            &format!("Plan {} Gate", p + 1),
            0.0,
            100,
            true,
            &mut next_id,
            x,
            y,
            z + 60.0,
        );
        // A timer mark starts the flight. A zone mark only arms Zone IN, and
        // a window of one period closes it again if nobody enters.
        let mut launch = Vec::new();
        if options.extension_min > 0 {
            launch.push(pause_live[p]);
            launch.push(extend[p]);
        } else {
            launch.push(arm_live[p]);
        }
        if options.trigger == AirTrigger::Clock {
            let mut started = timer(
                &format!("Plan {} Start", p + 1),
                0.1,
                100,
                true,
                &mut next_id,
                x,
                y,
                z + 95.0,
            );
            started.set_targets(copies[p].start_targets.clone());
            launch.push(stash(&mut logic, started));
            gated.set_targets(launch);
        } else {
            let mut close_one = deactivate(
                &format!("Plan {} Close Zone", p + 1),
                &mut next_id,
                x,
                y,
                z + 95.0,
            );
            affect(&mut close_one, copies[p].zones.clone());
            let close_one = stash(&mut logic, close_one);
            let mut zone_window = timer(
                &format!("Plan {} Zone Window", p + 1),
                options.zone_open_secs(),
                100,
                false,
                &mut next_id,
                x,
                y,
                z + 100.0,
            );
            zone_window.set_targets(vec![close_one]);
            let zone_window = stash(&mut logic, zone_window);
            let mut arm_window = activate(
                &format!("Plan {} Arm Zone Window", p + 1),
                &mut next_id,
                x,
                y,
                z + 105.0,
            );
            affect(&mut arm_window, vec![zone_window]);
            let arm_window = stash(&mut logic, arm_window);
            gated.set_targets(vec![open_zone[p], pulse_zone[p], arm_window, zone_window]);
            launch_on_zone.push((p, launch));
        }
        gate.push(stash(&mut logic, gated));
        let mut ready = timer(
            &format!("Slot {} Ready", p + 1),
            0.0,
            100,
            n == 1,
            &mut next_id,
            x,
            y,
            z + 70.0,
        );
        ready.set_targets(vec![gate[p]]);
        slot.push(stash(&mut logic, ready));
        let mut arm_ready = activate(&format!("Arm Slot {}", p + 1), &mut next_id, x, y, z + 80.0);
        affect(&mut arm_ready, vec![slot[p]]);
        arm_slot.push(stash(&mut logic, arm_ready));
        let mut disarm = deactivate(
            &format!("Plan {} Disarm Live", p + 1),
            &mut next_id,
            x,
            y,
            z + 90.0,
        );
        affect(&mut disarm, vec![live[p]]);
        disarm_live.push(stash(&mut logic, disarm));
    }

    // Per-side budget. A real launch ticks that side's counter. At zero the
    // side's gates switch off, so the other side can still be released.
    // Arming a zone does not spend the budget: the player still has to enter.
    // gates switch off, so the other side can still be released.
    let nato_plans: Vec<usize> = (0..n).filter(|&p| copies[p].nato).collect();
    let dprk_plans: Vec<usize> = (0..n).filter(|&p| !copies[p].nato).collect();
    let (x, y, z) = at(0, 4);
    let nato_budget = stash(
        &mut logic,
        counter("NATO Budget", options.budget, &mut next_id, x, y, z),
    );
    let dprk_budget = stash(
        &mut logic,
        counter("DPRK Budget", options.budget, &mut next_id, x + 40.0, y, z),
    );
    let mut nato_closed = deactivate("NATO Budget Closed", &mut next_id, x, y, z + 20.0);
    affect(
        &mut nato_closed,
        nato_plans.iter().map(|&p| gate[p]).collect(),
    );
    let nato_closed = stash(&mut logic, nato_closed);
    let mut dprk_closed = deactivate("DPRK Budget Closed", &mut next_id, x + 40.0, y, z + 20.0);
    affect(
        &mut dprk_closed,
        dprk_plans.iter().map(|&p| gate[p]).collect(),
    );
    let dprk_closed = stash(&mut logic, dprk_closed);
    set_targets_on(&mut logic, nato_budget, vec![nato_closed]);
    set_targets_on(&mut logic, dprk_budget, vec![dprk_closed]);
    let mut nato_release = activate("NATO Release", &mut next_id, x, y, z + 40.0);
    affect(
        &mut nato_release,
        nato_plans.iter().map(|&p| gate[p]).collect(),
    );
    let nato_release = stash(&mut logic, nato_release);
    let mut dprk_release = activate("DPRK Release", &mut next_id, x + 40.0, y, z + 40.0);
    affect(
        &mut dprk_release,
        dprk_plans.iter().map(|&p| gate[p]).collect(),
    );
    let dprk_release = stash(&mut logic, dprk_release);
    let mut off_slots = deactivate("Off Slots", &mut next_id, x, y, z + 60.0);
    affect(&mut off_slots, slot.clone());
    let off_slots = stash(&mut logic, off_slots);
    for p in 0..n {
        let budget = if copies[p].nato {
            nato_budget
        } else {
            dprk_budget
        };
        if options.trigger == AirTrigger::Clock {
            let mut targets = entity_targets(&logic, gate[p]);
            targets.push(budget);
            targets.push(off_slots);
            set_targets_on(&mut logic, gate[p], targets);
        }
    }
    for (p, mut launch) in launch_on_zone {
        let budget = if copies[p].nato {
            nato_budget
        } else {
            dprk_budget
        };
        launch.push(budget);
        launch.push(off_slots);
        let zones = copies[p].zones.clone();
        for id in zones {
            if let Some(zone) = find_mut(&mut copies[p].root, id) {
                for target in &launch {
                    zone.append_target(*target);
                }
            }
        }
    }

    // Clock: first delay, then a quiet slot, then the zone chance.
    let (x, y, z) = at(1, 6);
    let mut period = timer(
        "Period",
        (options.period_min as f64) * 60.0,
        100,
        true,
        &mut next_id,
        x,
        y,
        z,
    );
    let period_id = period.index.unwrap();
    let mut quiet = timer(
        QUIET_SLOT,
        0.2,
        options.quiet_pct as i32,
        true,
        &mut next_id,
        x,
        y,
        z + 20.0,
    );
    let quiet_id = quiet.index.unwrap();
    let mut quiet_pass = timer("Quiet Pass", 0.5, 100, true, &mut next_id, x, y, z + 40.0);
    let quiet_pass_id = quiet_pass.index.unwrap();
    let mut off_quiet_pass = deactivate("Off Quiet Pass", &mut next_id, x, y, z + 60.0);
    affect(&mut off_quiet_pass, vec![quiet_pass_id]);
    let off_quiet_pass_id = off_quiet_pass.index.unwrap();
    let mut chance = timer(
        ZONE_CHANCE,
        0.2,
        options.zone_chance_pct as i32,
        true,
        &mut next_id,
        x,
        y,
        z + 80.0,
    );
    let chance_id = chance.index.unwrap();
    let mut miss = timer("Zone Miss", 0.5, 100, true, &mut next_id, x, y, z + 100.0);
    let miss_id = miss.index.unwrap();
    let mut off_miss = deactivate("Off Zone Miss", &mut next_id, x, y, z + 120.0);
    affect(&mut off_miss, vec![miss_id]);
    let off_miss_id = off_miss.index.unwrap();
    let mut force = timer(FORCE_HOT, 0.0, 100, true, &mut next_id, x, y, z + 140.0);
    force.set_targets(slot.clone());
    let force_id = force.index.unwrap();
    chance.set_targets(vec![off_miss_id, force_id]);
    miss.set_targets(vec![period_id]);
    quiet.set_targets(vec![off_quiet_pass_id, period_id]);
    quiet_pass.set_targets(vec![chance_id, miss_id]);
    period.set_targets(vec![quiet_id, quiet_pass_id]);
    let mut clock = timer(
        CLOCK_FIRST,
        (options.first_delay_min as f64) * 60.0,
        100,
        true,
        &mut next_id,
        x,
        y,
        z + 180.0,
    );
    clock.set_targets(vec![quiet_id, quiet_pass_id]);
    let clock_id = clock.index.unwrap();
    logic.push(period);
    logic.push(quiet);
    logic.push(quiet_pass);
    logic.push(off_quiet_pass);
    logic.push(chance);
    logic.push(miss);
    logic.push(off_miss);
    logic.push(force);
    logic.push(clock);

    // Both modes wait for the clock. A zone mark arms the area; it does not
    // start the flight by itself.
    let spawn_start = clock_id;

    // One-shot permutation. Stage 1 claims the first sortie. Each later stage
    // claims a different one. A taken sortie's later roll is off, so the pass
    // skips it. The sweep is the 100% fallback when the last roll was taken.
    let mut hold: Vec<Vec<i32>> = Vec::new();
    let mut open_relay: Vec<Vec<i32>> = Vec::new();
    let mut stage_start: Vec<i32> = Vec::new();
    if n >= 2 {
        let handoffs = n - 1;
        for h in 0..handoffs {
            let mut holds = Vec::with_capacity(n);
            let mut opens = Vec::with_capacity(n);
            for p in 0..n {
                let (x, y, z) = at(p, 10 + h);
                let rel = timer(
                    &format!("Hold {} Plan {}", h + 1, p + 1),
                    0.0,
                    100,
                    false,
                    &mut next_id,
                    x,
                    y,
                    z,
                );
                holds.push(stash(&mut logic, rel));
                let mut op = timer(
                    &format!("Open {} Plan {}", h + 1, p + 1),
                    0.0,
                    100,
                    false,
                    &mut next_id,
                    x,
                    y,
                    z + 15.0,
                );
                op.set_targets(vec![gate[p]]);
                opens.push(stash(&mut logic, op));
            }
            for p in 0..n {
                let mut off_hold = deactivate(
                    &format!("Off Hold {} Plan {}", h + 1, p + 1),
                    &mut next_id,
                    ax,
                    ay,
                    az,
                );
                affect(&mut off_hold, vec![holds[p]]);
                let off_hold = stash(&mut logic, off_hold);
                let mut targets = opens.clone();
                targets.push(off_hold);
                set_targets_on(&mut logic, holds[p], targets);
            }
            hold.push(holds);
            open_relay.push(opens);
        }

        let mut roll = vec![vec![0; n]; n];
        let mut pass = vec![vec![0; n]; n];
        let mut claim = vec![vec![0; n]; n];
        let mut sweep = vec![vec![0; n]; n];
        let mut skip = vec![vec![0; n]; n];
        let mut off_roll = vec![vec![0; n]; n];
        let mut off_sweep = vec![vec![0; n]; n];
        let mut off_pass = vec![vec![0; n]; n];
        let mut off_skip = vec![vec![0; n]; n];
        let mut arm_claim = vec![vec![0; n]; n];
        let mut stop_sweep = vec![0; n];

        for s in 0..n {
            let (sx, sy, sz) = at(0, 20 + s * 3);
            for p in 0..n {
                let pct = weighted_pct(p, &weights);
                let (x, y, z) = at(p, 20 + s * 3);
                roll[s][p] = stash(
                    &mut logic,
                    timer(
                        &format!("Stage {} Plan {} Roll", s + 1, p + 1),
                        0.2,
                        pct,
                        true,
                        &mut next_id,
                        x,
                        y,
                        z,
                    ),
                );
                pass[s][p] = stash(
                    &mut logic,
                    timer(
                        &format!("Stage {} Plan {} Pass", s + 1, p + 1),
                        0.5,
                        100,
                        true,
                        &mut next_id,
                        x,
                        y,
                        z + 8.0,
                    ),
                );
                // The claim stays on. It is only targeted by a roll or sweep that
                // already succeeded. Starting it off and arming it in that same
                // target list drops the pulse, so the clock never starts.
                claim[s][p] = stash(
                    &mut logic,
                    timer(
                        &format!("Stage {} Plan {} Claim", s + 1, p + 1),
                        0.0,
                        100,
                        true,
                        &mut next_id,
                        x,
                        y,
                        z + 16.0,
                    ),
                );
                sweep[s][p] = stash(
                    &mut logic,
                    timer(
                        &format!("Stage {} Sweep {}", s + 1, p + 1),
                        0.2,
                        100,
                        true,
                        &mut next_id,
                        x,
                        y,
                        z + 24.0,
                    ),
                );
                skip[s][p] = stash(
                    &mut logic,
                    timer(
                        &format!("Stage {} Sweep {} Skip", s + 1, p + 1),
                        0.5,
                        100,
                        true,
                        &mut next_id,
                        x,
                        y,
                        z + 32.0,
                    ),
                );
                off_roll[s][p] = stash(
                    &mut logic,
                    deactivate(
                        &format!("Off Stage {} Plan {} Roll", s + 1, p + 1),
                        &mut next_id,
                        x,
                        y,
                        z + 40.0,
                    ),
                );
                off_sweep[s][p] = stash(
                    &mut logic,
                    deactivate(
                        &format!("Off Stage {} Plan {} Sweep", s + 1, p + 1),
                        &mut next_id,
                        x,
                        y,
                        z + 48.0,
                    ),
                );
                off_pass[s][p] = stash(
                    &mut logic,
                    deactivate(
                        &format!("Off Stage {} Plan {} Pass", s + 1, p + 1),
                        &mut next_id,
                        x,
                        y,
                        z + 56.0,
                    ),
                );
                off_skip[s][p] = stash(
                    &mut logic,
                    deactivate(
                        &format!("Off Stage {} Sweep {} Skip", s + 1, p + 1),
                        &mut next_id,
                        x,
                        y,
                        z + 64.0,
                    ),
                );
                arm_claim[s][p] = stash(
                    &mut logic,
                    activate(
                        &format!("Arm Stage {} Plan {} Claim", s + 1, p + 1),
                        &mut next_id,
                        x,
                        y,
                        z + 72.0,
                    ),
                );
            }
            stop_sweep[s] = stash(
                &mut logic,
                deactivate(
                    &format!("Stage {} Stop Sweep", s + 1),
                    &mut next_id,
                    sx,
                    sy,
                    sz + 80.0,
                ),
            );
            stage_start.push(stash(
                &mut logic,
                timer(
                    &format!("Stage {} Start", s + 1),
                    0.0,
                    100,
                    true,
                    &mut next_id,
                    sx,
                    sy,
                    sz + 88.0,
                ),
            ));
        }

        for s in 0..n {
            set_affected_on(&mut logic, stop_sweep[s], sweep[s].clone());
            set_targets_on(&mut logic, stage_start[s], vec![roll[s][0], pass[s][0]]);
            for p in 0..n {
                set_affected_on(&mut logic, off_roll[s][p], vec![roll[s][p]]);
                set_affected_on(&mut logic, off_sweep[s][p], vec![sweep[s][p]]);
                set_affected_on(&mut logic, off_pass[s][p], vec![pass[s][p]]);
                set_affected_on(&mut logic, off_skip[s][p], vec![skip[s][p]]);
                set_affected_on(&mut logic, arm_claim[s][p], vec![claim[s][p]]);
                set_targets_on(
                    &mut logic,
                    roll[s][p],
                    vec![off_pass[s][p], stop_sweep[s], arm_claim[s][p], claim[s][p]],
                );
                let next_pass = if p + 1 < n {
                    vec![roll[s][p + 1], pass[s][p + 1]]
                } else {
                    vec![sweep[s][0], skip[s][0]]
                };
                set_targets_on(&mut logic, pass[s][p], next_pass);
                set_targets_on(
                    &mut logic,
                    sweep[s][p],
                    vec![off_skip[s][p], arm_claim[s][p], claim[s][p]],
                );
                let next_skip = if p + 1 < n {
                    vec![sweep[s][p + 1], skip[s][p + 1]]
                } else {
                    Vec::new()
                };
                set_targets_on(&mut logic, skip[s][p], next_skip);

                let mut targets = Vec::new();
                for later in (s + 1)..n {
                    targets.push(off_roll[later][p]);
                    targets.push(off_sweep[later][p]);
                }
                if s == 0 {
                    targets.push(arm_slot[p]);
                    if n > 1 {
                        let arm_hold = stash(
                            &mut logic,
                            activate(
                                &format!("Arm Hold 1 Plan {}", p + 1),
                                &mut next_id,
                                ax,
                                ay,
                                az,
                            ),
                        );
                        set_affected_on(&mut logic, arm_hold, vec![hold[0][p]]);
                        targets.push(arm_hold);
                    }
                    targets.push(spawn_start);
                } else {
                    let arm_open = stash(
                        &mut logic,
                        activate(
                            &format!("Arm Open {} Plan {}", s, p + 1),
                            &mut next_id,
                            ax,
                            ay,
                            az,
                        ),
                    );
                    set_affected_on(&mut logic, arm_open, vec![open_relay[s - 1][p]]);
                    targets.push(arm_open);
                    if s < n - 1 {
                        let arm_hold = stash(
                            &mut logic,
                            activate(
                                &format!("Arm Hold {} Plan {}", s + 1, p + 1),
                                &mut next_id,
                                ax,
                                ay,
                                az,
                            ),
                        );
                        set_affected_on(&mut logic, arm_hold, vec![hold[s][p]]);
                        targets.push(arm_hold);
                    }
                }
                if s + 1 < n {
                    targets.push(stage_start[s + 1]);
                }
                set_targets_on(&mut logic, claim[s][p], targets);
            }
        }
    }

    // The order is fixed at mission start. Each finished sortie arms only its
    // own advance. The next clock mark fires that advance, which opens the
    // next sortie. A mark does not open the next flight by itself.
    let (x, y, z) = at(3, 8);
    let mut advance_ids = Vec::with_capacity(n);
    let mut arm_advance = Vec::with_capacity(n);
    for p in 0..n {
        let mut adv = timer(
            &format!("Advance {}", p + 1),
            0.0,
            100,
            false,
            &mut next_id,
            x + p as f64 * 40.0,
            y,
            z,
        );
        let adv_id = adv.index.unwrap();
        let mut off = deactivate(
            &format!("Advance {} Off", p + 1),
            &mut next_id,
            x + p as f64 * 40.0,
            y,
            z + 15.0,
        );
        affect(&mut off, vec![adv_id]);
        let off_id = stash(&mut logic, off);
        let mut targets: Vec<i32> = hold.iter().map(|row| row[p]).collect();
        targets.push(off_id);
        adv.set_targets(targets);
        advance_ids.push(stash(&mut logic, adv));
        let mut arm = activate(
            &format!("Arm Advance {}", p + 1),
            &mut next_id,
            x + p as f64 * 40.0,
            y,
            z + 30.0,
        );
        affect(&mut arm, vec![adv_id]);
        arm_advance.push(stash(&mut logic, arm));
    }
    let mut force_targets = slot.clone();
    force_targets.extend(advance_ids.iter().copied());
    force_targets.push(period_id);
    set_targets_on(&mut logic, force_id, force_targets);

    let all_zones: Vec<i32> = copies.iter().flat_map(|c| c.zones.clone()).collect();
    let (x, y, z) = at(2, 8);
    let mut close_zones = deactivate("Close Zones", &mut next_id, x, y, z);
    affect(&mut close_zones, all_zones);
    let close_zones_id = stash(&mut logic, close_zones);

    let mut stop_targets = Vec::new();
    stop_targets.extend(slot.iter().copied());
    stop_targets.extend(gate.iter().copied());
    stop_targets.extend(hold.iter().flatten().copied());
    stop_targets.extend(open_relay.iter().flatten().copied());
    stop_targets.extend([clock_id, quiet_pass_id, chance_id, force_id, period_id]);
    stop_targets.extend(advance_ids.iter().copied());
    let mut stop = deactivate(STOP_NAME, &mut next_id, x, y, z + 20.0);
    affect(&mut stop, stop_targets);
    let stop_id = stash(&mut logic, stop);

    // The mission window. It switches the relays off, closes every Zone IN,
    // then pulses the running plan's end timer one second later. That cleanup
    // cannot open the next sortie: the advances are already off.
    let mut window = timer(
        WINDOW_NAME,
        options.window_secs(),
        100,
        true,
        &mut next_id,
        x,
        y,
        z + 40.0,
    );
    let mut window_targets = vec![stop_id, close_zones_id];
    window_targets.extend(live.iter().copied());
    window.set_targets(window_targets);
    let window_id = stash(&mut logic, window);

    let mut trace = timer(TRACE_NAME, 0.0, 100, true, &mut next_id, x, y, z + 60.0);
    trace.set_property(
        "Desc",
        "\"Clock First, Quiet Slot, Zone Chance, Force Hot, Slot Ready, then the flight or Zone IN. MISSION END arms the next mark. PACKAGE WINDOW stops advances.\"",
    );
    let trace_id = stash(&mut logic, trace);

    let mut begin = mcu(
        "MCU_TR_MissionBegin",
        BEGIN_NAME,
        &mut next_id,
        x,
        y,
        z + 80.0,
    );
    begin.set_property("Enabled", "1");
    let mut begin_targets = vec![trace_id, window_id];
    if n >= 2 {
        begin_targets.push(stage_start[0]);
    } else {
        begin_targets.push(spawn_start);
    }
    begin.set_targets(begin_targets);
    logic.push(begin);

    for p in 0..n {
        let release = if copies[p].nato {
            nato_release
        } else {
            dprk_release
        };
        let end_targets = vec![release, disarm_live[p], arm_advance[p]];
        let end_id = copies[p].end;
        if let Some(node) = find_mut(&mut copies[p].root, end_id) {
            for id in end_targets {
                node.append_target(id);
            }
        }
    }

    let mut logic_group = Il2Entity::new("Group");
    logic_group.index = Some(next_id);
    logic_group.set_property("Index", next_id.to_string());
    next_id += 1;
    logic_group.set_name(LOGIC_GROUP);
    logic_group.set_property("Desc", "\"\"");
    logic_group.children = logic;

    let mut out = Il2Entity::new("Group");
    out.index = Some(next_id);
    out.set_property("Index", next_id.to_string());
    out.set_name("Air Tasking");
    out.set_property("Desc", "\"\"");
    out.children = copies.into_iter().map(|c| c.root).collect();
    out.children.push(logic_group);
    disarm_illegal_enabled(&mut out);
    Ok(out)
}

/// The editor loads `Enabled` on translators and entities. On a timer, checkzone,
/// activate, deactivate, or counter the same line fails the group import.
/// Objects marked `Enabled = 0` are turned off by Disarm At Start, which Mission
/// Begin runs before the clock, so a closed zone still drops the template pulse.
/// A claim is not in that set: the roll triggers it in the same breath as the
/// arm, and the game drops that pulse.
fn disarm_illegal_enabled(root: &mut Il2Entity) {
    let mut off = Vec::new();
    collect_disabled(&mut root.children, &mut off);
    if off.is_empty() {
        return;
    }
    let mut next = root.max_index().saturating_add(1);
    let Some(logic) = root
        .children
        .iter_mut()
        .find(|child| child.name() == Some(LOGIC_GROUP))
    else {
        return;
    };
    let (x, y, z) = logic
        .find_by_name(BEGIN_NAME)
        .map(pos_of)
        .unwrap_or((0.0, 0.0, 0.0));
    let mut disarm = deactivate(DISARM_START, &mut next, x, y, z);
    affect(&mut disarm, off);
    let disarm_id = disarm.index.unwrap();
    if let Some(begin) = logic.find_by_name_mut(BEGIN_NAME) {
        let mut targets = vec![disarm_id];
        targets.extend(begin.targets.iter().copied());
        begin.set_targets(targets);
    }
    logic.children.push(disarm);
}

fn collect_disabled(nodes: &mut [Il2Entity], off: &mut Vec<i32>) {
    for node in nodes {
        if !editor_keeps_enabled(&node.block_type) {
            if let Some(value) = node.property("Enabled").map(str::to_string) {
                node.properties.retain(|(key, _)| key != "Enabled");
                if value == "0" {
                    if let Some(id) = node.index {
                        off.push(id);
                    }
                }
            }
        }
        collect_disabled(&mut node.children, off);
    }
}

/// Block types whose saved form includes `Enabled`. Everything else loses the key.
fn editor_keeps_enabled(block: &str) -> bool {
    matches!(
        block,
        "MCU_TR_Entity"
            | "MCU_TR_MissionBegin"
            | "MCU_Icon"
            | "MCU_DateTime"
            | "MCU_TR_TaxiGraph"
            | "MCU_TR_InfluenceArea"
            | "MCU_TR_CustomTrigger"
            | "MCU_TR_ComplexTrigger"
    )
}

fn clamped(options: &AirOptions) -> AirOptions {
    let mut o = options.clone();
    o.quiet_pct = o.quiet_pct.min(100);
    o.zone_chance_pct = o.zone_chance_pct.min(100);
    o.budget = o.budget.max(1);
    o.nato_active = o.nato_active.max(1);
    o.dprk_active = o.dprk_active.max(1);
    o.period_min = o.period_min.max(1);
    o.window_min = o.window_min.clamp(5, 360);
    o
}

/// Arm checkzones that live outside this group, and close them with the package window.
///
/// Map export uses this so Exclusive Activation plans can fire only while the
/// mission window is open. Mission Begin turns the zones off. Clock First turns
/// them back on. The editor does not load `Enabled` on a checkzone.
pub fn bind_outside_zones(air: &mut Il2Entity, zone_ids: &[i32], next_id: &mut i32) {
    if zone_ids.is_empty() {
        return;
    }
    let Some(logic) = air
        .children
        .iter_mut()
        .find(|child| child.name() == Some(LOGIC_GROUP))
    else {
        return;
    };
    let (x, y, z) = logic
        .find_by_name(CLOCK_FIRST)
        .map(pos_of)
        .unwrap_or((0.0, 0.0, 0.0));
    let mut arm = activate("Arm Zone Plans", next_id, x, y, z + 15.0);
    affect(&mut arm, zone_ids.to_vec());
    drop_enabled(&mut arm);
    let arm_id = arm.index.unwrap();
    let mut pulse = timer("Pulse Zone Plans", 0.1, 100, true, next_id, x, y, z + 30.0);
    pulse.set_targets(zone_ids.to_vec());
    drop_enabled(&mut pulse);
    let pulse_id = pulse.index.unwrap();
    if let Some(clock) = logic.find_by_name_mut(CLOCK_FIRST) {
        clock.append_target(arm_id);
        clock.append_target(pulse_id);
    }
    if let Some(close) = logic.find_by_name_mut("Close Zones") {
        for id in zone_ids {
            append_affected(close, *id);
        }
    }
    // The editor will not load Enabled on a checkzone. Mission Begin turns
    // these zones off; Clock First's activate turns them back on.
    prepend_disarm(logic, zone_ids, next_id, x, y, z);
    logic.children.push(arm);
    logic.children.push(pulse);
}

fn drop_enabled(entity: &mut Il2Entity) {
    if !editor_keeps_enabled(&entity.block_type) {
        entity.properties.retain(|(key, _)| key != "Enabled");
    }
}

fn prepend_disarm(logic: &mut Il2Entity, ids: &[i32], next_id: &mut i32, x: f64, y: f64, z: f64) {
    if ids.is_empty() {
        return;
    }
    let disarm_id = if let Some(disarm) = logic.find_by_name_mut(DISARM_START) {
        for id in ids {
            append_affected(disarm, *id);
        }
        disarm.index
    } else {
        let mut disarm = deactivate(DISARM_START, next_id, x, y, z);
        affect(&mut disarm, ids.to_vec());
        let id = disarm.index;
        logic.children.push(disarm);
        id
    };
    let Some(disarm_id) = disarm_id else {
        return;
    };
    if let Some(begin) = logic.find_by_name_mut(BEGIN_NAME) {
        if !begin.targets.contains(&disarm_id) {
            let mut targets = vec![disarm_id];
            targets.extend(begin.targets.iter().copied());
            begin.set_targets(targets);
        }
    }
}

fn catalog_plane(script_part: &str) -> Result<crate::template::CatalogUnit, String> {
    let cat = CATALOG.get_or_init(bundled_catalog);
    let needle = script_part.to_ascii_lowercase();
    cat.iter()
        .find(|u| u.script.to_ascii_lowercase().contains(&needle))
        .cloned()
        .ok_or_else(|| format!("no aircraft matching {script_part} in the catalog"))
}

static CATALOG: OnceLock<Vec<crate::template::CatalogUnit>> = OnceLock::new();

fn plan_is_nato(root: &Il2Entity) -> bool {
    let mut country = None;
    root.for_each(&mut |e| {
        if e.block_type == "Plane" && country.is_none() {
            country = e.property("Country").and_then(|v| v.parse().ok());
        }
    });
    country.unwrap_or(501) / 100 == 6
}

fn weighted_pct(index: usize, weights: &[i32]) -> i32 {
    let rest: i32 = weights[index..]
        .iter()
        .copied()
        .map(|w| w.max(1))
        .sum::<i32>()
        .max(1);
    let w = weights[index].max(1);
    ((100 * w + rest / 2) / rest).clamp(1, 100)
}

fn stash(logic: &mut Vec<Il2Entity>, entity: Il2Entity) -> i32 {
    let id = entity.index.expect("mcu index");
    logic.push(entity);
    id
}

fn entity_targets(logic: &[Il2Entity], id: i32) -> Vec<i32> {
    logic
        .iter()
        .find(|e| e.index == Some(id))
        .map(|e| e.targets.clone())
        .unwrap_or_default()
}

fn set_targets_on(logic: &mut [Il2Entity], id: i32, targets: Vec<i32>) {
    if let Some(e) = logic.iter_mut().find(|e| e.index == Some(id)) {
        e.set_targets(targets);
    }
}

/// An activate or deactivate names the MCUs it toggles in both lists.
/// Checkzones follow Targets. Timers in the strike template follow Objects.
fn affect(entity: &mut Il2Entity, ids: Vec<i32>) {
    entity.set_targets(ids.clone());
    entity.set_objects(ids);
}

fn set_affected_on(logic: &mut [Il2Entity], id: i32, ids: Vec<i32>) {
    if let Some(e) = logic.iter_mut().find(|e| e.index == Some(id)) {
        affect(e, ids);
    }
}

fn append_affected(entity: &mut Il2Entity, id: i32) {
    entity.append_target(id);
    if entity.objects.contains(&id) {
        return;
    }
    let mut ids = entity.objects.clone();
    ids.push(id);
    entity.set_objects(ids);
}

fn zone_in_ids(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "MCU_CheckZone" {
            return;
        }
        if is_zone_in_name(e.name().unwrap_or("")) {
            if let Some(id) = e.index {
                ids.push(id);
            }
        }
    });
    ids
}

fn is_zone_in_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("zone in") && !n.contains("out") && !n.contains("reactivat")
}

/// `MISSION END`, then a timer named `END`, then `Mission Complete`.
fn end_timer_id(root: &Il2Entity) -> Option<i32> {
    let mut mission_end = None;
    let mut end = None;
    let mut complete = None;
    root.for_each(&mut |e| {
        if e.block_type != "MCU_Timer" {
            return;
        }
        let Some(id) = e.index else {
            return;
        };
        let name = e.name().unwrap_or("");
        if name.eq_ignore_ascii_case("MISSION END") {
            mission_end.get_or_insert(id);
        } else if name.eq_ignore_ascii_case("END") {
            end.get_or_insert(id);
        } else if name.eq_ignore_ascii_case("Mission Complete") {
            complete.get_or_insert(id);
        }
    });
    mission_end.or(end).or(complete)
}

fn map_ids(ids: &[i32], id_map: &HashMap<i32, i32>) -> Option<Vec<i32>> {
    let mapped: Vec<i32> = ids
        .iter()
        .filter_map(|id| id_map.get(id).copied())
        .collect();
    (mapped.len() == ids.len()).then_some(mapped)
}

fn pos_of(entity: &Il2Entity) -> (f64, f64, f64) {
    let p = |key: &str| {
        entity
            .property(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    };
    (p("XPos"), p("YPos"), p("ZPos"))
}

fn find_ref(root: &Il2Entity, index: i32) -> Option<&Il2Entity> {
    if root.index == Some(index) {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_ref(c, index))
}

fn find_mut(root: &mut Il2Entity, index: i32) -> Option<&mut Il2Entity> {
    if root.index == Some(index) {
        return Some(root);
    }
    for child in &mut root.children {
        if let Some(found) = find_mut(child, index) {
            return Some(found);
        }
    }
    None
}

fn mcu(kind: &str, name: &str, next_id: &mut i32, x: f64, y: f64, z: f64) -> Il2Entity {
    let mut e = Il2Entity::new(kind);
    let id = *next_id;
    *next_id += 1;
    e.index = Some(id);
    e.set_property("Index", id.to_string());
    e.set_name(name);
    e.set_property("Desc", "\"\"");
    e.set_targets(Vec::new());
    e.set_objects(Vec::new());
    e.set_property("XPos", format!("{x:.3}"));
    e.set_property("YPos", format!("{y:.3}"));
    e.set_property("ZPos", format!("{z:.3}"));
    e.set_property("XOri", "0");
    e.set_property("YOri", "0");
    e.set_property("ZOri", "0");
    if kind == "MCU_Timer" {
        e.set_property("Time", "0");
        e.set_property("Random", "100");
    }
    if kind == "MCU_TR_MissionBegin" {
        e.set_property("Enabled", "1");
    }
    e
}

fn timer(
    name: &str,
    time: f64,
    random: i32,
    enabled: bool,
    next_id: &mut i32,
    x: f64,
    y: f64,
    z: f64,
) -> Il2Entity {
    let mut e = mcu("MCU_Timer", name, next_id, x, y, z);
    e.set_property("Time", format_secs(time));
    e.set_property("Random", random.to_string());
    e.set_property("Enabled", if enabled { "1" } else { "0" });
    e
}

fn activate(name: &str, next_id: &mut i32, x: f64, y: f64, z: f64) -> Il2Entity {
    mcu("MCU_Activate", name, next_id, x, y, z)
}

fn deactivate(name: &str, next_id: &mut i32, x: f64, y: f64, z: f64) -> Il2Entity {
    mcu("MCU_Deactivate", name, next_id, x, y, z)
}

fn counter(name: &str, count: u32, next_id: &mut i32, x: f64, y: f64, z: f64) -> Il2Entity {
    let mut e = mcu("MCU_Counter", name, next_id, x, y, z);
    e.set_property("Counter", count.to_string());
    e.set_property("Dropcount", "1");
    e
}

fn format_secs(s: f64) -> String {
    if (s - s.round()).abs() < 1e-9 {
        format!("{s:.0}")
    } else {
        format!("{s:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;
    use crate::template::is_cap_group;

    fn one(name: &'static str, nato: bool) -> SortieDef {
        SortieDef {
            id: "test",
            name,
            side: if nato { "NATO" } else { "DPRK" },
            script_part: if nato { "f80c10" } else { "yak9p" },
            ships: 1,
            country: if nato { 601 } else { 502 },
            skill: 2,
            nato,
        }
    }

    fn input(def: &SortieDef, index: usize, weight: i32) -> AirTaskInput {
        let root = build_sortie(def, default_target(index)).expect("sortie");
        AirTaskInput {
            label: def.name.to_string(),
            root,
            weight,
            duration_s: 0.0,
            runs: 1,
            spread: AirSpread::Even,
        }
    }

    fn linked(n: usize, options: &AirOptions) -> Il2Entity {
        let defs: Vec<SortieDef> = (0..n)
            .map(|i| {
                one(
                    if i % 2 == 0 {
                        "NATO strike"
                    } else {
                        "DPRK strike"
                    },
                    i % 2 == 0,
                )
            })
            .collect();
        let plans: Vec<AirTaskInput> = defs
            .iter()
            .enumerate()
            .map(|(i, def)| input(def, i, 1))
            .collect();
        link_air_task(&plans, options).expect("link")
    }

    fn collect<'a>(root: &'a Il2Entity, name: &str, out: &mut Vec<&'a Il2Entity>) {
        if root.name() == Some(name) {
            out.push(root);
        }
        for child in &root.children {
            collect(child, name, out);
        }
    }

    fn only<'a>(root: &'a Il2Entity, name: &str) -> &'a Il2Entity {
        let mut found = Vec::new();
        collect(root, name, &mut found);
        assert_eq!(found.len(), 1, "{name} count {}", found.len());
        found[0]
    }

    fn many<'a>(root: &'a Il2Entity, name: &str) -> Vec<&'a Il2Entity> {
        let mut found = Vec::new();
        collect(root, name, &mut found);
        found
    }

    fn find_id(root: &Il2Entity, id: i32) -> Option<&Il2Entity> {
        if root.index == Some(id) {
            return Some(root);
        }
        root.children.iter().find_map(|c| find_id(c, id))
    }

    fn names_of(root: &Il2Entity, ids: &[i32]) -> Vec<String> {
        ids.iter()
            .filter_map(|id| {
                find_id(root, *id)
                    .and_then(|e| e.name())
                    .map(str::to_string)
            })
            .collect()
    }

    #[test]
    fn a_sortie_uses_the_current_strike_template() {
        assert!(builtin_library().iter().all(|def| !def.id.is_empty()));
        let group = build_sortie(&one("NATO strike", true), default_target(0)).unwrap();
        for name in [
            "WP 2 Prox",
            "Time on Target Prox",
            "FAILSAFE IP",
            "FAILSAFE WP2",
            "MISSION END",
        ] {
            assert!(group.find_by_name(name).is_some(), "missing {name}");
        }
        let prox = only(&group, "WP 2 Prox");
        let prox_id = prox.index.unwrap();
        let goto = only(&group, "Goto WP 2");
        assert!(
            goto.targets.contains(&prox_id),
            "Goto WP 2 targets {:?}",
            goto.targets
        );
        let pulse = only(&group, "PULSE OUT");
        assert!(
            !pulse.targets.contains(&prox_id),
            "PULSE OUT must not arm WP 2 Prox"
        );
        let fake = only(&group, "Fake Object WP2");
        let mut plane_country = None;
        group.for_each(&mut |e| {
            if e.block_type == "Plane" && plane_country.is_none() {
                plane_country = e.property("Country").map(str::to_string);
            }
        });
        assert_eq!(fake.property("Country"), plane_country.as_deref());
        assert_eq!(fake.property("Country"), Some("601"));
    }

    #[test]
    fn two_sorties_are_one_permutation_and_the_window_stops_the_queue() {
        let mut options = AirOptions::default();
        options.trigger = AirTrigger::Zone;
        let group = linked(2, &options);
        let text = serialize_group(&group);
        let group = parse_group_file(&text).expect("round trip");

        assert!(only(&group, "Stage 1 Plan 1 Roll").index.is_some());
        assert!(only(&group, "Stage 1 Plan 2 Roll").index.is_some());
        assert!(only(&group, "Stage 2 Plan 1 Roll").index.is_some());
        assert!(only(&group, "Stage 2 Plan 2 Roll").index.is_some());
        assert_eq!(
            only(&group, "Stage 1 Plan 1 Roll").property("Random"),
            Some("50")
        );
        assert_eq!(
            only(&group, "Stage 1 Plan 2 Roll").property("Random"),
            Some("100")
        );

        let begin = only(&group, BEGIN_NAME);
        let begin_names = names_of(&group, &begin.targets);
        assert!(begin_names.iter().any(|n| n == "Stage 1 Start"));
        assert!(!begin_names.iter().any(|n| n == "Stage 2 Start"));
        assert!(!begin_names.iter().any(|n| n == CLOCK_FIRST));

        let claim = only(&group, "Stage 1 Plan 1 Claim");
        let disarm = only(&group, DISARM_START);
        let claim_id = claim.index.unwrap();
        assert!(
            !disarm.targets.contains(&claim_id) && !disarm.objects.contains(&claim_id),
            "the first draw stays on so its pulse is not dropped"
        );
        let roll = only(&group, "Stage 1 Plan 1 Roll");
        assert!(
            roll.targets.contains(&claim_id),
            "a winning roll still fires the claim"
        );
        let slot = only(&group, "Slot 1 Ready");
        let slot_id = slot.index.unwrap();
        assert!(disarm.targets.contains(&slot_id));
        let arm_slot = only(&group, "Arm Slot 1");
        assert!(arm_slot.objects.contains(&slot_id));
        assert!(arm_slot.targets.contains(&slot_id));
        let claim1 = names_of(&group, &claim.targets);
        assert!(claim1.iter().any(|n| n == "Arm Slot 1"));
        assert!(!claim1.iter().any(|n| n.starts_with("Arm Slot 2")));
        assert!(claim1.iter().any(|n| n == "Off Stage 2 Plan 1 Roll"));
        assert!(!claim1.iter().any(|n| n == "Off Stage 2 Plan 2 Roll"));
        assert!(claim1.iter().any(|n| n == "Arm Hold 1 Plan 1"));
        assert!(claim1.iter().any(|n| n == CLOCK_FIRST));
        assert!(!claim1.iter().any(|n| n.starts_with("Arm Open")));

        let claim2 = names_of(&group, &only(&group, "Stage 2 Plan 2 Claim").targets);
        assert!(claim2.iter().any(|n| n == "Arm Open 1 Plan 2"));
        assert!(!claim2.iter().any(|n| n == "Arm Open 1 Plan 1"));
        assert!(!claim2.iter().any(|n| n.starts_with("Arm Slot")));

        let hold = only(&group, "Hold 1 Plan 1");
        let hold_names = names_of(&group, &hold.targets);
        assert_eq!(hold_names.len(), 3);
        assert!(hold_names.iter().any(|n| n == "Open 1 Plan 1"));
        assert!(hold_names.iter().any(|n| n == "Open 1 Plan 2"));
        assert!(hold_names.iter().any(|n| n == "Off Hold 1 Plan 1"));
        assert_eq!(
            names_of(&group, &only(&group, "Open 1 Plan 2").targets),
            vec!["Plan 2 Gate".to_string()]
        );

        let mut zone_ids = Vec::new();
        group.for_each(&mut |e| {
            if matches!(
                e.block_type.as_str(),
                "MCU_Timer" | "MCU_Activate" | "MCU_Deactivate" | "MCU_Counter" | "MCU_CheckZone"
            ) {
                assert!(
                    e.property("Enabled").is_none(),
                    "{} {} still has Enabled",
                    e.block_type,
                    e.name().unwrap_or("")
                );
            }
            if e.block_type == "MCU_CheckZone" && is_zone_in_name(e.name().unwrap_or("")) {
                zone_ids.push(e.index.unwrap());
            }
        });
        assert!(zone_ids.len() >= 2);
        let disarm = only(&group, DISARM_START);
        assert_eq!(begin.targets.first().copied(), disarm.index);
        for id in &zone_ids {
            assert!(
                disarm.targets.contains(id) && disarm.objects.contains(id),
                "Zone IN starts off without an Enabled field"
            );
        }

        let ends = many(&group, "MISSION END");
        assert_eq!(ends.len(), 2);
        for end in &ends {
            let targeted = names_of(&group, &end.targets);
            assert!(
                targeted.iter().any(|n| n.starts_with("Arm Advance")),
                "the end arms the next mark: {targeted:?}"
            );
            assert!(
                !targeted.iter().any(|n| n.starts_with("Hold ")),
                "the end waits for the next mark: {targeted:?}"
            );
            assert!(!targeted.iter().any(|n| n.contains("Open Zone")));
            for id in &end.targets {
                assert!(!zone_ids.contains(id), "MISSION END must not enable a zone");
            }
        }
        let force_names = names_of(&group, &only(&group, FORCE_HOT).targets);
        assert!(force_names.iter().any(|n| n == "Period"));
        assert!(force_names.iter().any(|n| n == "Advance 1"));
        assert!(force_names.iter().any(|n| n == "Slot 1 Ready"));
        assert!(many(&group, "Plan 1 Start").is_empty());
        assert!(many(&group, "Plan 1 Area").is_empty());
        assert!(many(&group, "Plan 1 Players").is_empty());

        let window = only(&group, WINDOW_NAME);
        assert_eq!(window.property("Time"), Some("10800"));
        assert_eq!(
            window.targets.first().copied(),
            only(&group, STOP_NAME).index
        );
        let stop_names = names_of(&group, &only(&group, STOP_NAME).targets);
        assert!(stop_names.iter().any(|n| n == "Hold 1 Plan 1"));
        assert!(stop_names.iter().any(|n| n == "Hold 1 Plan 2"));
        assert!(stop_names.iter().any(|n| n == "Slot 1 Ready"));
        let window_names = names_of(&group, &window.targets);
        assert!(!window_names.iter().any(|n| n.starts_with("Open ")));
        assert!(!window_names.iter().any(|n| n.starts_with("Slot ")));
        assert!(
            window_names
                .iter()
                .any(|n| n.starts_with("Plan ") && n.ends_with("Live"))
        );

        for name in [
            "WP 2 Prox",
            "Time on Target Prox",
            "FAILSAFE IP",
            "FAILSAFE WP2",
        ] {
            assert_eq!(many(&group, name).len(), 2, "{name}");
        }
        let prox_ids: Vec<i32> = many(&group, "WP 2 Prox")
            .iter()
            .map(|e| e.index.unwrap())
            .collect();
        for pulse in many(&group, "PULSE OUT") {
            for id in &prox_ids {
                assert!(!pulse.targets.contains(id));
            }
        }
        for goto in many(&group, "Goto WP 2") {
            assert!(prox_ids.iter().any(|id| goto.targets.contains(id)));
        }
    }

    #[test]
    fn one_sortie_spawns_from_the_mark_without_a_zone() {
        let group = linked(1, &AirOptions::default());
        assert!(many(&group, "Stage 1 Plan 1 Roll").is_empty());
        let start = only(&group, "Plan 1 Start");
        let started = names_of(&group, &start.targets);
        assert!(started.iter().any(|n| n == "MISSION BEGIN"));
        assert!(started.iter().any(|n| n == "PULSE OUT"));
        assert!(!started.iter().any(|n| n == "Self Deactivate"));
        let pulse = only(&group, "PULSE OUT");
        let tot = only(&group, "Time on Target Prox");
        assert!(
            pulse.targets.contains(&tot.index.unwrap()),
            "PULSE OUT arms Time on Target"
        );
        assert!(
            tot.targets
                .iter()
                .any(|id| find_id(&group, *id).and_then(|e| e.name()) == Some("Time on Target 1")),
            "the proximity starts the 8 minute timer"
        );
        let timer = only(&group, "Time on Target 1");
        assert!(
            timer
                .targets
                .iter()
                .any(|id| { find_id(&group, *id).and_then(|e| e.name()) == Some("Goto WP 2") }),
            "Time on Target orders the home waypoint"
        );
        let gate = names_of(&group, &only(&group, "Plan 1 Gate").targets);
        assert!(gate.iter().any(|n| n == "Plan 1 Start"));
        assert!(gate.iter().any(|n| n == "Hold Launches"));
        let begin = names_of(&group, &only(&group, BEGIN_NAME).targets);
        assert!(begin.iter().any(|n| n == "Mark 1"));
        assert_eq!(only(&group, "Mark 1").property("Time"), Some("0"));
        let end = names_of(&group, &only(&group, "MISSION END").targets);
        assert!(end.iter().any(|n| n == "Release Launches"));
        let zone = only(&group, "Zone IN").index.unwrap();
        let disarm = only(&group, DISARM_START);
        assert!(disarm.targets.contains(&zone));
        assert!(disarm.objects.contains(&zone));
        assert_eq!(only(&group, WINDOW_NAME).property("Time"), Some("10800"));
        assert_template_begin_is_silent(&group);
    }

    #[test]
    fn cap_on_a_timer_checks_the_other_side_and_cleans_up_with_the_window() {
        let nato = build_cap(&one("NATO CAP", true), default_target(0)).unwrap();
        assert!(is_cap_group(&nato));
        assert_eq!(
            nato.find_by_name("Zone IN")
                .unwrap()
                .property("PlaneCoalitions"),
            Some("[1]")
        );
        assert_eq!(
            nato.find_by_name("Zone Out")
                .unwrap()
                .property("PlaneCoalitions"),
            Some("[1]")
        );
        assert!(nato.find_by_name("Goto WP 4").is_some());
        assert!(nato.find_by_name("Return to WP 1").is_some());
        let dprk = build_cap(&one("DPRK CAP", false), default_target(1)).unwrap();
        assert_eq!(
            dprk.find_by_name("Zone IN")
                .unwrap()
                .property("PlaneCoalitions"),
            Some("[2]")
        );

        let mut nato = nato;
        assert!(crate::template::ensure_cap_shift(&mut nato, 25));
        let plan = AirTaskInput {
            label: "NATO CAP".into(),
            root: nato,
            weight: 1,
            duration_s: 25.0 * 60.0,
            runs: 1,
            spread: AirSpread::Even,
        };
        let group = link_air_task(&[plan], &AirOptions::default()).unwrap();
        let started = names_of(&group, &only(&group, "Plan 1 Start").targets);
        assert!(started.iter().any(|n| n == "MISSION BEGIN"));
        assert!(started.iter().any(|n| n == "Zone In ReActivate"));
        assert!(started.iter().any(|n| n == "CAP Shift"));
        assert!(!started.iter().any(|n| n == "PULSE OUT"));
        let shift = only(&group, "CAP Shift");
        assert_eq!(shift.property("Time"), Some("1500"));
        assert_eq!(shift.property("Random"), Some("0"));
        let shift_names = names_of(&group, &shift.targets);
        assert!(shift_names.iter().any(|n| n == "Stop Patrol"));
        assert!(shift_names.iter().any(|n| n == "Force Complete - High"));
        assert!(shift_names.iter().any(|n| n == "CAP Head Home"));
        assert!(
            !shift_names.iter().any(|n| n == "Goto WP 5"),
            "Goto WP 5 force-completes in the same pulse and cancels the home leg"
        );
        let home = only(&group, "CAP Head Home");
        assert_eq!(home.property("Time"), Some("0.50"));
        assert!(
            home.targets
                .iter()
                .any(|id| { find_id(&group, *id).and_then(|e| e.name()) == Some("WP 5") }),
            "the delayed timer orders the home waypoint"
        );
        let stop = only(&group, "Stop Patrol");
        assert!(
            stop.objects.iter().any(|id| {
                find_id(&group, *id).and_then(|e| e.name()) == Some("Return to WP 1")
            }),
            "the loop cannot order another lap"
        );

        let zone_in = only(&group, "Zone IN");
        let zone_in_names = names_of(&group, &zone_in.targets);
        assert!(zone_in_names.iter().any(|n| n == "Zone Out ReActivate"));
        assert!(!zone_in_names.iter().any(|n| n == "MISSION BEGIN"));
        assert!(!zone_in_names.iter().any(|n| n == "PULSE OUT"));

        let zone_out = only(&group, "Zone Out");
        let zone_out_names = names_of(&group, &zone_out.targets);
        assert!(!zone_out_names.iter().any(|n| n == "MISSION END"));
        assert!(!zone_out_names.iter().any(|n| n == "COOLDOWN"));
        assert!(!zone_out_names.iter().any(|n| n == "Zone In ReActivate"));
        assert!(zone_out_names.iter().any(|n| n == "Self Deactivate"));

        let window = names_of(&group, &only(&group, WINDOW_NAME).targets);
        assert!(window.iter().any(|n| n == "Close Zones"));
        assert!(window.iter().any(|n| n == "MISSION END"));
        let close = only(&group, "Close Zones");
        assert!(close.objects.contains(&zone_in.index.unwrap()));
        assert!(close.objects.contains(&zone_out.index.unwrap()));
        let disarm = only(&group, DISARM_START);
        assert!(disarm.objects.contains(&zone_in.index.unwrap()));
        assert!(disarm.objects.contains(&zone_out.index.unwrap()));
    }

    #[test]
    fn clock_quiet_zone_and_hot_gate_follow_the_options() {
        let mut options = AirOptions::default();
        options.first_delay_min = 10;
        options.quiet_pct = 20;
        options.zone_chance_pct = 50;
        options.window_min = 10;
        let group = linked(1, &options);
        assert_eq!(only(&group, "Mark 1").property("Time"), Some("600"));
        assert_eq!(only(&group, QUIET_SLOT).property("Random"), Some("20"));
        let chance = only(&group, ZONE_CHANCE);
        assert_eq!(chance.property("Random"), Some("50"));
        let chance_names = names_of(&group, &chance.targets);
        assert!(chance_names.iter().any(|n| n == "Plan 1 Gate"));
        assert!(!chance_names.iter().any(|n| n == COLD_HOLD));
        assert!(many(&group, "Plan 1 Area").is_empty());
        assert_eq!(only(&group, WINDOW_NAME).property("Time"), Some("600"));
    }

    #[test]
    fn a_zone_mark_arms_the_template_zone_for_one_period() {
        let mut options = AirOptions::default();
        options.trigger = AirTrigger::Zone;
        options.period_min = 5;
        let group = linked(1, &options);
        assert!(many(&group, "Plan 1 Start").is_empty());
        assert!(many(&group, "Plan 1 Players").is_empty());
        let gate = names_of(&group, &only(&group, "Plan 1 Gate").targets);
        assert!(gate.iter().any(|n| n == "Plan 1 Open Zone"));
        assert!(gate.iter().any(|n| n == "Plan 1 Zone Window"));
        assert!(!gate.iter().any(|n| n == "NATO Budget"));
        assert_eq!(
            only(&group, "Plan 1 Zone Window").property("Time"),
            Some("300")
        );
        let zone = only(&group, "Zone IN");
        let zone_names = names_of(&group, &zone.targets);
        assert!(zone_names.iter().any(|n| n == "NATO Budget"));
        assert!(zone_names.iter().any(|n| n == "Plan 1 Arm Live"));
        let begin = names_of(&group, &only(&group, BEGIN_NAME).targets);
        assert!(begin.iter().any(|n| n == CLOCK_FIRST));
    }

    #[test]
    fn the_default_path_does_not_wait_or_require_players() {
        let group = linked(1, &AirOptions::default());
        assert_eq!(only(&group, "Mark 1").property("Time"), Some("0"));
        assert_eq!(only(&group, QUIET_SLOT).property("Random"), Some("0"));
        let chance = only(&group, ZONE_CHANCE);
        assert_eq!(chance.property("Random"), Some("100"));
        let names = names_of(&group, &chance.targets);
        assert!(names.iter().any(|n| n == "Plan 1 Gate"));
        assert!(!names.iter().any(|n| n == COLD_HOLD));
        assert!(many(&group, "Plan 1 Area").is_empty());
        assert!(
            only(&group, TRACE_NAME)
                .property("Desc")
                .unwrap()
                .contains("no check zone")
        );
    }

    #[test]
    fn budget_and_extension_change_the_spawn_gate() {
        let mut options = AirOptions::default();
        options.budget = 2;
        options.nato_active = 1;
        options.dprk_active = 2;
        options.launch_while_active = true;
        let group = linked(2, &options);
        let nato_gate = names_of(&group, &only(&group, "Plan 1 Gate").targets);
        assert!(nato_gate.iter().any(|n| n == "Arm Plan 1"));
        assert!(nato_gate.iter().any(|n| n == "NATO Credit 1"));
        assert!(!nato_gate.iter().any(|n| n == "Plan 1 Start"));
        assert!(!nato_gate.iter().any(|n| n == "Hold Launches"));
        assert!(!nato_gate.iter().any(|n| n == "DPRK Credit 1"));
        let dprk_gate = names_of(&group, &only(&group, "Plan 2 Gate").targets);
        assert!(dprk_gate.iter().any(|n| n == "DPRK Credit 1"));
        assert!(
            only(&group, "Plan 1 Starter")
                .targets
                .iter()
                .any(|id| find_id(&group, *id).and_then(|e| e.name()) == Some("Plan 1 Start"))
        );
        let nato_end = many(&group, "MISSION END")
            .into_iter()
            .find(|e| {
                names_of(&group, &e.targets)
                    .iter()
                    .any(|n| n == "NATO Restore 1")
            })
            .expect("the NATO sortie returns a NATO credit");
        assert!(
            !names_of(&group, &nato_end.targets)
                .iter()
                .any(|n| n.starts_with("DPRK"))
        );
        assert!(
            many(&group, "DPRK Credit 2").is_empty(),
            "one DPRK plan keeps one credit"
        );
    }

    #[test]
    fn inspect_rejects_a_group_with_no_zone_or_end() {
        let mut root = Il2Entity::new("Group");
        root.set_name("Bare");
        assert_eq!(
            inspect_air_plan(&root).unwrap_err(),
            "this group has no Zone IN"
        );
        let mut zone = Il2Entity::new("MCU_CheckZone");
        zone.index = Some(1);
        zone.set_name("Zone IN");
        root.children.push(zone);
        assert_eq!(
            inspect_air_plan(&root).unwrap_err(),
            "this group has no end timer"
        );
        let mut end = Il2Entity::new("MCU_Timer");
        end.index = Some(2);
        end.set_name("MISSION END");
        root.children.push(end);
        let info = inspect_air_plan(&root).unwrap();
        assert_eq!(info.zone_count, 1);
        assert_eq!(info.end_name, "MISSION END");
    }

    #[test]
    fn authored_targets_are_kept() {
        let options = AirOptions::default();
        let a = build_sortie(&one("NATO strike", true), (12345.0, 67890.0)).unwrap();
        let b = build_sortie(&one("DPRK strike", false), (50000.0, 60000.0)).unwrap();
        let group = link_air_task(
            &[
                AirTaskInput {
                    label: "a".into(),
                    root: a,
                    weight: 1,
                    duration_s: 0.0,
                    runs: 1,
                    spread: AirSpread::Even,
                },
                AirTaskInput {
                    label: "b".into(),
                    root: b,
                    weight: 9,
                    duration_s: 0.0,
                    runs: 1,
                    spread: AirSpread::Even,
                },
            ],
            &options,
        )
        .unwrap();
        let mut xs = Vec::new();
        group.for_each(&mut |e| {
            if e.block_type == "MCU_CheckZone" && is_zone_in_name(e.name().unwrap_or("")) {
                if let Some(x) = e.property("XPos") {
                    xs.push(x.to_string());
                }
            }
        });
        assert!(
            xs.iter().any(|x| x.starts_with("12345")),
            "zones moved off the authored target: {xs:?}"
        );
        assert!(
            xs.iter().any(|x| x.starts_with("50000")),
            "zones moved off the authored target: {xs:?}"
        );
        assert_eq!(
            only(&group, "Stage 1 Plan 1 Roll").property("Random"),
            Some("10")
        );
        assert_eq!(
            only(&group, "Stage 1 Plan 2 Roll").property("Random"),
            Some("100")
        );
    }

    #[test]
    fn ten_minute_steps_fill_a_hundred_minute_window() {
        let mut options = AirOptions::default();
        options.window_min = 100;
        options.first_delay_min = 10;
        options.period_min = 10;
        options.launches = 9;
        assert_eq!(marks_that_fit(&options), 9);
        let marks = schedule_secs(&options);
        assert_eq!(marks.len(), 9);
        assert!((marks[0] - 600.0).abs() < 0.01);
        assert!((marks[8] - 5400.0).abs() < 0.01);
        options.launches = 40;
        assert_eq!(launch_count(&options), 9);
        options.spread = AirSpread::Front;
        options.launches = 9;
        let front = schedule_secs(&options);
        assert!(front.last().copied().unwrap_or(0.0) <= 600.0 + (5400.0 - 600.0) * 0.5 + 0.01);
        options.spread = AirSpread::End;
        let end = schedule_secs(&options);
        assert!(end[0] >= 600.0 + (5400.0 - 600.0) * 0.5 - 0.01);
        options.launches = 1;
        options.spread = AirSpread::End;
        let single = schedule_secs(&options);
        assert!((single[0] - 5400.0).abs() < 0.01);
        options.spread = AirSpread::Even;
        let single = schedule_secs(&options);
        assert!((single[0] - 600.0).abs() < 0.01);
    }

    #[test]
    fn draw_applies_where_the_flight_estimates_overlap() {
        let both = [600.0, 1200.0];
        assert_eq!(exclusive_plan(100.0, 0.0, &both), None);
        assert_eq!(exclusive_plan(700.0, 0.0, &both), Some(1));
        assert_eq!(exclusive_plan(1300.0, 0.0, &both), None);
        assert_eq!(exclusive_plan(10.0, 0.0, &[0.0, 0.0]), None);
        assert!((flight_secs(36_000.0, 360.0) - 360.0).abs() < 0.01);
    }

    #[test]
    fn a_shorter_zone_window_is_not_the_whole_period() {
        let mut options = AirOptions::default();
        options.trigger = AirTrigger::Zone;
        options.period_min = 10;
        options.zone_open_min = 4;
        let group = linked(1, &options);
        assert_eq!(
            only(&group, "Plan 1 Zone Window").property("Time"),
            Some("240")
        );
    }

    fn assert_template_begin_is_silent(group: &Il2Entity) {
        let mut found = 0;
        group.for_each(&mut |entity| {
            if entity.block_type == "MCU_TR_MissionBegin"
                && entity.name() == Some("Translator Mission Begin")
            {
                found += 1;
                assert!(
                    entity.targets.is_empty(),
                    "the template still starts its own flight"
                );
            }
        });
        assert!(found > 0, "the cloned sortie lost Translator Mission Begin");
    }

    #[test]
    fn two_sorties_that_share_a_mark_launch_one() {
        let group = linked(2, &AirOptions::default());
        assert_eq!(many(&group, "Mark 1").len(), 1);
        assert!(many(&group, "Mark 2").is_empty());
        let mark_names = names_of(&group, &only(&group, "Mark 1").targets);
        assert_eq!(mark_names, vec!["Launch 1".to_string()]);
        assert!(only(&group, "Draw").index.is_some());
        assert_template_begin_is_silent(&group);
        assert_eq!(many(&group, "Translator Mission Begin").len(), 2);
    }

    #[test]
    fn times_in_the_order_builds_one_flight_per_firing() {
        let mut plan = input(&one("NATO strike", true), 0, 1);
        plan.runs = 3;
        plan.spread = AirSpread::Even;
        let mut options = AirOptions::default();
        options.window_min = 60;
        options.period_min = 10;
        options.first_delay_min = 0;
        let group = link_air_task(std::slice::from_ref(&plan), &options).unwrap();
        assert_eq!(many(&group, "MISSION END").len(), 3);
        assert_eq!(only(&group, "Mark 1").property("Time"), Some("0"));
        assert_eq!(only(&group, "Mark 2").property("Time"), Some("1500"));
        assert_eq!(only(&group, "Mark 3").property("Time"), Some("3000"));
        assert!(many(&group, "Mark 4").is_empty());
        assert!(many(&group, "Draw").is_empty());
        assert_template_begin_is_silent(&group);
        let mark = names_of(&group, &only(&group, "Mark 2").targets);
        assert_eq!(mark, vec!["Launch 2".to_string()]);
        let chance = names_of(&group, &only(&group, "Zone Chance 2").targets);
        assert!(chance.iter().any(|name| name == "Plan 2 Gate"));
    }
}
