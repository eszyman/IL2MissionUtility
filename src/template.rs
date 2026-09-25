//! template.rs — Template Builder core
//!
//! Builds one proximity-triggered unit group from `TemplateOptions`:
//! `Logic` (Zone IN / Zone Out checkzones, `ENABLE / PULSE IN` pulse,
//! Activate or Spawn bring-up with optional `DeathCount` → `COOLDOWN`
//! repeat, and the `MISSION END` cleanup hub — `Force Complete - High`,
//! optional `RTB DELAY` → `RTB East/West n`, then deactivate → delete),
//! `Units` (cloned object + `MCU_TR_Entity` pairs, wingmen target-linked
//! to their lead), `Orders` (command MCUs), and `Waypoints` (`WP n` path
//! hops). Owns the seat model and its bookkeeping — catalog loading
//! (`bundled_catalog` from `assets/Models.Group` + user groups), order
//! chain normalization and index remapping, Goto WP insertion,
//! attack-area suggestions from `weapon_range`, and the order tree layout
//! the GUI draws. It does NOT write `NodeGates` (fighter packs are linked
//! by `pack.rs`), does not own payload/mod data (the seat only holds
//! `payload_id` / `mod_mask` — `payloads.rs` owns the catalog), and does
//! not lay out the `WP n` path (hops sit at 4 km along +X from the fixed
//! 40 km origin; the path is hand-authored after generation).
//!
//! ## Public API
//! * `struct TemplateOptions` + `fn generate_template` — the build.
//! * `fn load_template` / `struct TemplateLoad` — fill the builder from a
//!   `.Group` (native Template Builder files round-trip; other layouts are
//!   rebuilt from units + orders, with warnings for anything dropped).
//! * Seats: `TemplateSeat`, `CatalogUnit`, `FlightRole`, `PlaneStart`,
//!   `append_seat`, `replace_seat_unit`, `copy_seat_attributes`,
//!   `move_seat`, `apply_formation_numbers`, `last_lead_index`,
//!   `apply_plane_start` / `AIR_START_ALTITUDE_M` (airstart defaults to
//!   1500 m for every aircraft),
//!   `remap_seat_index` / `remap_index_vec` / `remap_event_then`.
//! * Orders / events: `OrderSpec` (`for_kind` / `for_unit`), `OrderKind`,
//!   `EntityEvent`, `EventHook` / `EventThen`, `normalize_order_chain`,
//!   `insert_goto_waypoint_after`, `set_report_following`,
//!   `used_waypoint_count` / `next_waypoint_number`,
//!   `order_tree_columns` / `order_tree_layout`, `event_triggers_order`.
//! * Zones: `ZoneCoalition`, `ZoneMix`, `zone_defaults`, `visual_range_m`,
//!   `near_visual_range`, `zone_mix_for_seats`, AIR/GROUND/TRAIN zone
//!   constants.
//! * Placement / formations: `PlaceLayout`, `place_offset`,
//!   `finger_four_offset`, `PLACEMENT_SPACING`, `AIR_FORMATIONS` /
//!   `GROUND_FORMATIONS`, `formation_label` / `formation_density`.
//! * Waypoint / attack helpers: `waypoint_area_m`,
//!   `path_waypoint_display_m`, `waypoint_display_altitude`,
//!   `waypoint_display_priority`,
//!   `DEFAULT_TIME_ON_TARGET_S`, `DEFAULT_TIMER_S`, `apply_suggested_attack_area`,
//!   `priority_label`, `order_chip_detail`, `AttackAreaTarget`,
//!   `refresh_attack_areas_for_seat`, `attack_area_range_limit`,
//!   `DEFAULT_ATTACK_AREA_M`.
//! * Catalog: `bundled_catalog`, `builtin_plane_catalog`, `load_catalog`,
//!   `load_catalog_as_user_added`, `merge_catalog`,
//!   `catalog_carriage_scripts`, `carriage_label`.
//! * Load for edit: `load_template` / `TemplateLoad` /
//!   `looks_like_generated_template`.
//!
//! ## Used by
//! * ui.rs (Template mode) — the whole builder (catalog, seats, order
//!   tree, zones, waypoints). Load group → `load_template`; Generate →
//!   `serialize_group`.
//! * bombers.rs (Exclusive mode) — `inspect_plan` / cleanup validation
//!   reads the generated `MISSION END` / `Trigger Delete` graph.


use std::collections::{HashMap, HashSet};

use crate::aircraft::{
    callsign_for, encode_tcode, encode_tcode_color, flight_color, flight_number,
    plane_display_name, AircraftType, AIRCRAFT_TYPES,
};
use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::parser::parse_il2_document;
use crate::payloads;
use crate::weapon_range;

const ORIGIN_X: f64 = 40_000.0;
const ORIGIN_Z: f64 = 40_000.0;
const MCU_GAP: f64 = 150.0;
const BRANCH_GAP: f64 = 300.0;
const BAKED_ORDER_DELAY: f64 = 0.5;
const BRING_UP_DELAY: f64 = 0.5;
const AFTER_BRING_UP_DELAY: f64 = 0.5;
const MISSION_END_TIME: f64 = 0.1;
const MISSION_END_ORDERS_TIME: f64 = 0.05;
const RTB_DELAY: f64 = 0.5;
const DELAYED_END_TIME: f64 = 2.0;
const RTB_DEACTIVATE_DELAY: f64 = 60.0;
const DELETE_WAIT: f64 = 0.5;
pub const DEFAULT_TIME_ON_TARGET_S: f32 = 180.0;
/// Configurable pause (`MCU_Timer`) between sequential orders.
pub const DEFAULT_TIMER_S: f32 = 5.0;
/// World spacing between seats in a placement group. Not a UI control.
pub const PLACEMENT_SPACING: f32 = 150.0;
/// Path waypoint spacing along +X (north). Authored by hand after generate.
pub const WAYPOINT_SPACING_M: f32 = 4_000.0;
pub const DEFAULT_ATTACK_AREA_M: f32 = 3000.0;
/// MCU_Waypoint Area for aircraft path hops.
pub const PLANE_WAYPOINT_AREA_M: &str = "300";
/// MCU_Waypoint Area for ground / ship / train path hops.
pub const GROUND_WAYPOINT_AREA_M: &str = "100";

/// Airborne Zone IN / visual-range combat bubble (metres).
pub const AIR_ZONE_IN_M: f32 = 16_000.0;
pub const AIR_ZONE_OUT_M: f32 = 35_000.0;
/// Ground (vehicle / ship / fixed) Zone IN / visual-range combat bubble.
pub const GROUND_ZONE_IN_M: f32 = 10_000.0;
pub const GROUND_ZONE_OUT_M: f32 = 19_000.0;
/// Train-only (or primarily train) Zone IN / visual-range combat bubble.
pub const TRAIN_ZONE_IN_M: f32 = 19_000.0;
pub const TRAIN_ZONE_OUT_M: f32 = 30_000.0;
/// Slider / schematic slack for the visual-range flag.
pub const VISUAL_RANGE_SLACK_M: f32 = 800.0;

/// Which Zone IN / Out pair the current seat mix should use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneMix {
    Air,
    Ground,
    Train,
}

/// Default Zone IN and Zone Out for a mix. Empty seats keep the last pair.
pub fn zone_defaults(mix: ZoneMix) -> (f32, f32) {
    match mix {
        ZoneMix::Air => (AIR_ZONE_IN_M, AIR_ZONE_OUT_M),
        ZoneMix::Ground => (GROUND_ZONE_IN_M, GROUND_ZONE_OUT_M),
        ZoneMix::Train => (TRAIN_ZONE_IN_M, TRAIN_ZONE_OUT_M),
    }
}

/// Recommended visual-range radius for the mix (same as that mix’s Zone IN).
pub fn visual_range_m(mix: ZoneMix) -> f32 {
    zone_defaults(mix).0
}

pub fn near_visual_range(value: f32, visual: f32) -> bool {
    (value - visual).abs() <= VISUAL_RANGE_SLACK_M
}

/// Air wins if any plane is present. Otherwise trains win when they are at
/// least half the seats. Empty mix is `None` so user edits stay put.
pub fn zone_mix_for_seats(seats: &[TemplateSeat]) -> Option<ZoneMix> {
    if seats.is_empty() {
        return None;
    }
    if seats.iter().any(|s| s.unit.is_air()) {
        return Some(ZoneMix::Air);
    }
    let trains = seats.iter().filter(|s| s.unit.is_train()).count();
    if trains > 0 && trains * 2 >= seats.len() {
        Some(ZoneMix::Train)
    } else {
        Some(ZoneMix::Ground)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Plane,
    Vehicle,
    Infantry,
    Train,
    Ship,
    Fixed,
    UserAdded,
}

impl UnitKind {
    pub const ALL: [UnitKind; 7] = [
        UnitKind::Plane,
        UnitKind::Vehicle,
        UnitKind::Infantry,
        UnitKind::Train,
        UnitKind::Ship,
        UnitKind::Fixed,
        UnitKind::UserAdded,
    ];

    pub fn label(self) -> &'static str {
        match self {
            UnitKind::Plane => "Planes",
            UnitKind::Vehicle => "Vehicles",
            UnitKind::Infantry => "Infantry",
            UnitKind::Train => "Trains",
            UnitKind::Ship => "Ships",
            UnitKind::Fixed => "Fixed Units",
            UnitKind::UserAdded => "User Added",
        }
    }

    fn object_type(self) -> &'static str {
        match self {
            UnitKind::Plane => "Plane",
            UnitKind::Vehicle | UnitKind::Infantry | UnitKind::Fixed | UnitKind::UserAdded => {
                "Vehicle"
            }
            UnitKind::Train => "Train",
            UnitKind::Ship => "Ship",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderKind {
    Attack,
    AttackArea,
    Behaviour,
    Cover,
    Effect,
    Flare,
    ForceComplete,
    Formation,
    GotoWaypoint,
    TimeOnTarget,
    Timer,
    MissionComplete,
    Land,
    TakeOff,
    RtbOnZoneOut,
    OnSpawned,
    OnTargetAttacked,
    OnAreaAttacked,
    OnTookOff,
    OnLanded,
}

const AIR_ORDERS: &[OrderKind] = &[
    OrderKind::Attack,
    OrderKind::AttackArea,
    OrderKind::Behaviour,
    OrderKind::Cover,
    OrderKind::Effect,
    OrderKind::Flare,
    OrderKind::ForceComplete,
    OrderKind::Formation,
    OrderKind::GotoWaypoint,
    OrderKind::TimeOnTarget,
    OrderKind::Timer,
    OrderKind::MissionComplete,
    OrderKind::Land,
    OrderKind::TakeOff,
    OrderKind::OnSpawned,
    OrderKind::OnTargetAttacked,
    OrderKind::OnAreaAttacked,
    OrderKind::OnTookOff,
    OrderKind::OnLanded,
];

const GROUND_ORDERS: &[OrderKind] = &[
    OrderKind::Attack,
    OrderKind::AttackArea,
    OrderKind::Behaviour,
    OrderKind::Effect,
    OrderKind::Flare,
    OrderKind::ForceComplete,
    OrderKind::Formation,
    OrderKind::GotoWaypoint,
    OrderKind::TimeOnTarget,
    OrderKind::Timer,
    OrderKind::MissionComplete,
    OrderKind::OnSpawned,
    OrderKind::OnTargetAttacked,
    OrderKind::OnAreaAttacked,
];

impl OrderKind {
    pub fn label(self) -> &'static str {
        match self {
            OrderKind::Attack => "Attack",
            OrderKind::AttackArea => "AttackArea",
            OrderKind::Behaviour => "Behavior",
            OrderKind::Cover => "Cover",
            OrderKind::Effect => "Effect",
            OrderKind::Flare => "Flare",
            OrderKind::ForceComplete => "Force Complete",
            OrderKind::Formation => "Formation",
            OrderKind::GotoWaypoint => "Goto WP",
            OrderKind::TimeOnTarget => "Time on Target",
            OrderKind::Timer => "Timer",
            OrderKind::MissionComplete => "Mission Complete",
            OrderKind::Land => "Land",
            OrderKind::TakeOff => "Take Off",
            OrderKind::RtbOnZoneOut => "RTB on Zone Out",
            OrderKind::OnSpawned => "OnSpawned",
            OrderKind::OnTargetAttacked => "OnTargetAttacked",
            OrderKind::OnAreaAttacked => "OnAreaAttacked",
            OrderKind::OnTookOff => "OnTookOff",
            OrderKind::OnLanded => "OnLanded",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        [
            Self::Attack,
            Self::AttackArea,
            Self::Behaviour,
            Self::Cover,
            Self::Effect,
            Self::Flare,
            Self::ForceComplete,
            Self::Formation,
            Self::GotoWaypoint,
            Self::TimeOnTarget,
            Self::Timer,
            Self::MissionComplete,
            Self::Land,
            Self::TakeOff,
            Self::RtbOnZoneOut,
            Self::OnSpawned,
            Self::OnTargetAttacked,
            Self::OnAreaAttacked,
            Self::OnTookOff,
            Self::OnLanded,
        ]
        .into_iter()
        .find(|k| k.label().eq_ignore_ascii_case(label))
    }

    fn from_block_type(block: &str) -> Option<Self> {
        match block {
            "MCU_CMD_AttackTarget" => Some(Self::Attack),
            "MCU_CMD_AttackArea" => Some(Self::AttackArea),
            "MCU_CMD_Behaviour" => Some(Self::Behaviour),
            "MCU_CMD_Cover" => Some(Self::Cover),
            "MCU_CMD_Effect" => Some(Self::Effect),
            "MCU_CMD_Flare" => Some(Self::Flare),
            "MCU_CMD_ForceComplete" => Some(Self::ForceComplete),
            "MCU_CMD_Formation" => Some(Self::Formation),
            "MCU_CMD_Land" => Some(Self::Land),
            "MCU_CMD_TakeOff" => Some(Self::TakeOff),
            _ => None,
        }
    }

    pub fn is_report(self) -> bool {
        matches!(
            self,
            OrderKind::OnSpawned
                | OrderKind::OnTargetAttacked
                | OrderKind::OnAreaAttacked
                | OrderKind::OnTookOff
                | OrderKind::OnLanded
        )
    }

    /// Timers / RTB that are not command MCUs: Time on Target, Timer, Mission
    /// Complete, RTB on Zone Out.
    pub fn is_special(self) -> bool {
        matches!(
            self,
            OrderKind::TimeOnTarget
                | OrderKind::Timer
                | OrderKind::MissionComplete
                | OrderKind::RtbOnZoneOut
        )
    }

    pub fn has_priority(self) -> bool {
        matches!(
            self,
            OrderKind::Attack
                | OrderKind::AttackArea
                | OrderKind::Cover
                | OrderKind::ForceComplete
                | OrderKind::Land
                | OrderKind::GotoWaypoint
        )
    }

    pub fn is_command(self) -> bool {
        !self.is_report() && !self.is_special()
    }

    pub fn commands(kind: UnitKind) -> impl Iterator<Item = OrderKind> {
        Self::available(kind)
            .iter()
            .copied()
            .filter(|k| k.is_command())
    }

    pub fn reports(kind: UnitKind) -> impl Iterator<Item = OrderKind> {
        Self::available(kind)
            .iter()
            .copied()
            .filter(|k| k.is_report())
    }

    pub fn specials(kind: UnitKind) -> impl Iterator<Item = OrderKind> {
        Self::available(kind)
            .iter()
            .copied()
            .filter(|k| k.is_special())
    }

    /// OnReport Type. CmdId is the spawner or the matching command MCU.
    pub fn report_type(self) -> Option<i32> {
        match self {
            OrderKind::OnSpawned => Some(0),
            OrderKind::OnTargetAttacked => Some(1),
            OrderKind::OnAreaAttacked => Some(2),
            OrderKind::OnTookOff => Some(3),
            OrderKind::OnLanded => Some(4),
            _ => None,
        }
    }

    /// Command this report waits on. OnSpawned uses the spawner, not a command.
    pub fn report_follows(self) -> Option<OrderKind> {
        match self {
            OrderKind::OnTargetAttacked => Some(OrderKind::Attack),
            OrderKind::OnAreaAttacked => Some(OrderKind::AttackArea),
            OrderKind::OnTookOff => Some(OrderKind::TakeOff),
            OrderKind::OnLanded => Some(OrderKind::Land),
            _ => None,
        }
    }

    /// Orders the editor actually accepts for this unit kind. Land, cover,
    /// takeoff, OnTookOff, and OnLanded are aircraft-only.
    pub fn available(kind: UnitKind) -> &'static [OrderKind] {
        match kind {
            UnitKind::Plane => AIR_ORDERS,
            _ => GROUND_ORDERS,
        }
    }

    /// Commands that can follow a report in the chain.
    pub fn following(kind: UnitKind) -> impl Iterator<Item = OrderKind> {
        Self::available(kind)
            .iter()
            .copied()
            .filter(|k| !k.is_report() && *k != OrderKind::RtbOnZoneOut)
    }

    pub fn has_command_mcu(self) -> bool {
        self.block_type().is_some()
    }

    /// Attack / Time on Target start together from the waypoint, not in series.
    pub fn is_wp_parallel(self) -> bool {
        matches!(
            self,
            OrderKind::Attack | OrderKind::AttackArea | OrderKind::TimeOnTarget
        )
    }

    fn block_type(self) -> Option<&'static str> {
        match self {
            OrderKind::Attack => Some("MCU_CMD_AttackTarget"),
            OrderKind::AttackArea => Some("MCU_CMD_AttackArea"),
            OrderKind::Behaviour => Some("MCU_CMD_Behaviour"),
            OrderKind::Cover => Some("MCU_CMD_Cover"),
            OrderKind::Effect => Some("MCU_CMD_Effect"),
            OrderKind::Flare => Some("MCU_CMD_Flare"),
            OrderKind::ForceComplete => Some("MCU_CMD_ForceComplete"),
            OrderKind::Formation => Some("MCU_CMD_Formation"),
            OrderKind::GotoWaypoint
            | OrderKind::TimeOnTarget
            | OrderKind::Timer
            | OrderKind::MissionComplete
            | OrderKind::RtbOnZoneOut => None,
            OrderKind::Land => Some("MCU_CMD_Land"),
            OrderKind::TakeOff => Some("MCU_CMD_TakeOff"),
            OrderKind::OnSpawned
            | OrderKind::OnTargetAttacked
            | OrderKind::OnAreaAttacked
            | OrderKind::OnTookOff
            | OrderKind::OnLanded => None,
        }
    }
}

/// Entity OnEvent Type IDs. Trailer IDs sit between BingoCargo (79) and
/// radar (85, confirmed on K49 radar vehicles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityEvent {
    OnPilotKilled,
    OnPilotWounded,
    OnPlaneCrashed,
    OnPlaneCriticalDamage,
    OnPlaneDestroyed,
    OnPlaneLanded,
    OnPlaneTookOff,
    OnPlaneBingoFuel,
    OnPlaneBingoMainMG,
    OnPlaneBingoBombs,
    OnPlaneBingoTurrets,
    OnPlaneGunnersKilled,
    OnDamaged,
    OnKilled,
    OnMovedTo,
    OnPlaneBingoCargo,
    OnSpottingStarted,
    OnTrailerKilled,
    OnTrailerDamaged,
    OnTrailerAttached,
    OnTrailerDetached,
    OnRadarRequestAirSupport,
}

const AIR_EVENTS: &[EntityEvent] = &[
    EntityEvent::OnPilotKilled,
    EntityEvent::OnPilotWounded,
    EntityEvent::OnPlaneCrashed,
    EntityEvent::OnPlaneCriticalDamage,
    EntityEvent::OnPlaneDestroyed,
    EntityEvent::OnPlaneLanded,
    EntityEvent::OnPlaneTookOff,
    EntityEvent::OnPlaneBingoFuel,
    EntityEvent::OnPlaneBingoMainMG,
    EntityEvent::OnPlaneBingoBombs,
    EntityEvent::OnPlaneBingoTurrets,
    EntityEvent::OnPlaneGunnersKilled,
    EntityEvent::OnDamaged,
    EntityEvent::OnKilled,
    EntityEvent::OnMovedTo,
    EntityEvent::OnPlaneBingoCargo,
];

const GROUND_EVENTS: &[EntityEvent] = &[
    EntityEvent::OnDamaged,
    EntityEvent::OnKilled,
    EntityEvent::OnMovedTo,
    EntityEvent::OnSpottingStarted,
    EntityEvent::OnTrailerKilled,
    EntityEvent::OnTrailerDamaged,
    EntityEvent::OnTrailerAttached,
    EntityEvent::OnTrailerDetached,
    EntityEvent::OnRadarRequestAirSupport,
];

impl EntityEvent {
    pub fn label(self) -> &'static str {
        match self {
            EntityEvent::OnPilotKilled => "OnPilotKilled",
            EntityEvent::OnPilotWounded => "OnPilotWounded",
            EntityEvent::OnPlaneCrashed => "OnPlaneCrashed",
            EntityEvent::OnPlaneCriticalDamage => "OnPlaneCriticalDamage",
            EntityEvent::OnPlaneDestroyed => "OnPlaneDestroyed",
            EntityEvent::OnPlaneLanded => "OnPlaneLanded",
            EntityEvent::OnPlaneTookOff => "OnPlaneTookOff",
            EntityEvent::OnPlaneBingoFuel => "OnBingoFuel",
            EntityEvent::OnPlaneBingoMainMG => "OnBingoMainMG",
            EntityEvent::OnPlaneBingoBombs => "OnBingoBombs",
            EntityEvent::OnPlaneBingoTurrets => "OnBingoTurrets",
            EntityEvent::OnPlaneGunnersKilled => "OnPlaneGunnersKilled",
            EntityEvent::OnDamaged => "OnDamaged",
            EntityEvent::OnKilled => "OnKilled",
            EntityEvent::OnMovedTo => "OnMovedTo",
            EntityEvent::OnPlaneBingoCargo => "OnBingoCargo",
            EntityEvent::OnSpottingStarted => "OnSpottingStarted",
            EntityEvent::OnTrailerKilled => "OnTrailerKilled",
            EntityEvent::OnTrailerDamaged => "OnTrailerDamaged",
            EntityEvent::OnTrailerAttached => "OnTrailerAttached",
            EntityEvent::OnTrailerDetached => "OnTrailerDetached",
            EntityEvent::OnRadarRequestAirSupport => "OnRadarRequestAirSupport",
        }
    }

    pub fn type_id(self) -> i32 {
        match self {
            EntityEvent::OnPilotKilled => 0,
            EntityEvent::OnPilotWounded => 1,
            EntityEvent::OnPlaneCrashed => 2,
            EntityEvent::OnPlaneCriticalDamage => 3,
            EntityEvent::OnPlaneDestroyed => 4,
            EntityEvent::OnPlaneLanded => 5,
            EntityEvent::OnPlaneTookOff => 6,
            EntityEvent::OnPlaneBingoFuel => 7,
            EntityEvent::OnPlaneBingoMainMG => 8,
            EntityEvent::OnPlaneBingoBombs => 9,
            EntityEvent::OnPlaneBingoTurrets => 10,
            EntityEvent::OnPlaneGunnersKilled => 11,
            EntityEvent::OnDamaged => 12,
            EntityEvent::OnKilled => 13,
            EntityEvent::OnMovedTo => 15,
            EntityEvent::OnPlaneBingoCargo => 79,
            EntityEvent::OnSpottingStarted => 74,
            EntityEvent::OnTrailerAttached => 80,
            EntityEvent::OnTrailerDetached => 81,
            EntityEvent::OnTrailerDamaged => 82,
            EntityEvent::OnTrailerKilled => 83,
            EntityEvent::OnRadarRequestAirSupport => 85,
        }
    }

    fn from_type_id(id: i32) -> Option<Self> {
        AIR_EVENTS
            .iter()
            .chain(GROUND_EVENTS)
            .copied()
            .find(|e| e.type_id() == id)
    }

    pub fn available(kind: UnitKind) -> &'static [EntityEvent] {
        match kind {
            UnitKind::Plane => AIR_EVENTS,
            _ => GROUND_EVENTS,
        }
    }

    pub fn default_for(kind: UnitKind) -> Self {
        match kind {
            UnitKind::Plane => EntityEvent::OnPlaneDestroyed,
            _ => EntityEvent::OnKilled,
        }
    }
}

/// Bring units in by enabling parked entities, or by spawning them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BringUp {
    Activate,
    Spawn,
}

impl BringUp {
    pub fn label(self) -> &'static str {
        match self {
            BringUp::Activate => "Activate Units",
            BringUp::Spawn => "Spawn Units",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FormationPreset {
    pub id: i32,
    pub label: &'static str,
    pub density: i32,
}

const fn air_form(id: i32, label: &'static str) -> FormationPreset {
    FormationPreset {
        id,
        label,
        density: 0,
    }
}

const fn ground_form(id: i32, label: &'static str, density: i32) -> FormationPreset {
    FormationPreset {
        id,
        label,
        density,
    }
}

/// Aircraft formations from `TemplateExamples/FormationTypes.Group`.
pub const AIR_FORMATIONS: &[FormationPreset] = &[
    air_form(19, "Pairs"),
    air_form(20, "Wedge"),
    air_form(21, "Right"),
    air_form(22, "Left"),
    air_form(23, "Heavy Wedge"),
    air_form(24, "Heavy Echelon Right"),
    air_form(26, "Heavy Combat Box"),
    air_form(27, "User"),
];

/// Vehicle / convoy formations from `TemplateExamples/VehicleFormationTypes.Group`
/// and `TemplateExamples/Simple Vehicle Formation 2 way column.Group`.
pub const GROUND_FORMATIONS: &[FormationPreset] = &[
    ground_form(4, "Road Column 1 way", 0),
    ground_form(18, "Road Column 2 way", 1),
    ground_form(10, "Panic Stop", 0),
    ground_form(11, "Continue Moving", 0),
];

pub fn formations_for(kind: UnitKind) -> &'static [FormationPreset] {
    match kind {
        UnitKind::Plane => AIR_FORMATIONS,
        _ => GROUND_FORMATIONS,
    }
}

pub fn formation_label(id: i32, kind: UnitKind) -> String {
    formations_for(kind)
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.label.to_string())
        .unwrap_or_else(|| format!("Type {id}"))
}

pub fn formation_density(id: i32, kind: UnitKind) -> i32 {
    formations_for(kind)
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.density)
        .unwrap_or(0)
}

#[derive(Clone, Debug)]
pub struct CatalogUnit {
    pub kind: UnitKind,
    pub name: String,
    pub script: String,
    display: String,
    object: Il2Entity,
    entity: Il2Entity,
}

impl CatalogUnit {
    pub fn label(&self) -> &str {
        &self.display
    }

    pub fn is_air(&self) -> bool {
        self.kind == UnitKind::Plane || self.object.block_type.eq_ignore_ascii_case("Plane")
    }

    pub fn is_train(&self) -> bool {
        self.kind == UnitKind::Train
    }

    /// Country written on the prototype (`Country = 501`, …). Defaults to USSR.
    pub fn country(&self) -> i32 {
        prop_i32(&self.object, "Country", 501)
    }

    /// Carriage scripts listed on this prototype (catalog order).
    pub fn prototype_carriages(&self) -> Vec<String> {
        train_carriages(&self.object)
    }

    /// Tender for this locomotive, if the prototype lists one.
    pub fn default_carriages(&self) -> Vec<String> {
        default_train_carriages(&self.object)
    }
}

/// Unique carriage scripts from every Train in `catalog`, first-seen order.
pub fn catalog_carriage_scripts(catalog: &[CatalogUnit]) -> Vec<String> {
    let mut out = Vec::new();
    for unit in catalog.iter().filter(|u| u.is_train()) {
        for script in unit.prototype_carriages() {
            if !out.iter().any(|s: &String| s.eq_ignore_ascii_case(&script)) {
                out.push(script);
            }
        }
    }
    out
}

/// Short label for a carriage script path.
pub fn carriage_label(script: &str) -> String {
    let id = script_type_id(script);
    match id {
        "type475-1-tender" => "Tender (type475-1)".into(),
        "usatc-s160-tender" => "Tender (USATC S160)".into(),
        "carbox" => "Box car".into(),
        "cargondola" => "Gondola".into(),
        "carmail" => "Mail car".into(),
        "carpassenger" => "Passenger car".into(),
        "carplatform" => "Flatcar".into(),
        "carplatformaa" => "AA flatcar".into(),
        "carplatformaa-61k" => "AA flatcar (61-K)".into(),
        "carplatformaa-boforsl60" => "AA flatcar (Bofors L60)".into(),
        "carplatformaa-dshk" => "AA flatcar (DShK)".into(),
        "carplatformaa-m1919" => "AA flatcar (M1919)".into(),
        "carplatformaa-m2" => "AA flatcar (M2)".into(),
        "carplatformaa-sg" => "AA flatcar (SG)".into(),
        "carplatformaa-zpuvz53" => "AA flatcar (ZPU / VZ-53)".into(),
        "cartank" => "Tank car".into(),
        other => other.replace('-', " "),
    }
}

fn train_carriages(object: &Il2Entity) -> Vec<String> {
    object
        .children
        .iter()
        .find(|c| c.block_type == "Carriages")
        .map(|c| {
            c.properties
                .iter()
                .filter(|(k, _)| k.is_empty())
                .map(|(_, v)| v.trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn default_train_carriages(object: &Il2Entity) -> Vec<String> {
    train_carriages(object)
        .into_iter()
        .filter(|s| script_type_id(s).to_ascii_lowercase().contains("tender"))
        .collect()
}

fn set_train_carriages(object: &mut Il2Entity, cars: &[String]) {
    let items: Vec<(String, String)> = cars
        .iter()
        .map(|s| {
            let body = s.trim_matches('"');
            (String::new(), format!("\"{body}\""))
        })
        .collect();
    if let Some(child) = object
        .children
        .iter_mut()
        .find(|c| c.block_type == "Carriages")
    {
        child.properties.retain(|(k, _)| !k.is_empty());
        child.properties.extend(items);
        return;
    }
    let mut child = Il2Entity::new("Carriages");
    child.properties = items;
    object.children.push(child);
}

/// Who Zone IN / Zone Out watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneCoalition {
    Eastern,
    Western,
    Both,
}

impl ZoneCoalition {
    pub const ALL: [ZoneCoalition; 3] = [
        ZoneCoalition::Eastern,
        ZoneCoalition::Western,
        ZoneCoalition::Both,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ZoneCoalition::Eastern => "DPRK [1]",
            ZoneCoalition::Western => "NATO [2]",
            ZoneCoalition::Both => "Both [1, 2]",
        }
    }

    fn plane_coalitions(self) -> &'static str {
        match self {
            ZoneCoalition::Eastern => "[1]",
            ZoneCoalition::Western => "[2]",
            ZoneCoalition::Both => "[1, 2]",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrderSpec {
    pub kind: OrderKind,
    pub delay_s: f32,
    pub attack_area: f32,
    pub attack_air: bool,
    pub attack_ground: bool,
    pub attack_g_targets: bool,
    pub time_s: f32,
    pub priority: i32,
    pub formation_type: i32,
    pub behaviour_filter: i32,
    pub flare_color: i32,
    pub effect_start: bool,
    pub cover_lead: Option<usize>,
    pub attack_seat: Option<usize>,
    pub attack_group: bool,
    pub waypoint: u32,
    /// Altitude for this Goto WP hop. 0 = use the template waypoint altitude.
    pub altitude: f32,
    /// Other seats that receive this same command MCU (Objects). Empty = this unit only.
    pub shared_with: Vec<usize>,
}

impl Default for OrderSpec {
    fn default() -> Self {
        Self {
            kind: OrderKind::AttackArea,
            delay_s: BAKED_ORDER_DELAY as f32,
            attack_area: DEFAULT_ATTACK_AREA_M,
            attack_air: true,
            attack_ground: false,
            attack_g_targets: false,
            time_s: 600.0,
            priority: 1,
            formation_type: 23,
            behaviour_filter: 8,
            flare_color: 0,
            effect_start: true,
            cover_lead: None,
            attack_seat: None,
            attack_group: true,
            waypoint: 1,
            altitude: 0.0,
            shared_with: Vec::new(),
        }
    }
}

impl OrderSpec {
    pub fn for_kind(kind: UnitKind) -> Self {
        let mut order = Self::default();
        match kind {
            UnitKind::Plane => {
                order.kind = OrderKind::Formation;
                order.formation_type = 23;
            }
            _ => {
                order.kind = OrderKind::AttackArea;
                order.formation_type = 4;
                order.attack_air = false;
                order.attack_ground = true;
            }
        }
        order
    }

    /// Ground AttackArea radius follows the unit's known system range (capped at 3 km).
    pub fn for_unit(unit: &CatalogUnit) -> Self {
        let mut order = Self::for_kind(unit.kind);
        if order.kind == OrderKind::AttackArea {
            order.attack_area =
                weapon_range::attack_area_radius_m(&unit.script, DEFAULT_ATTACK_AREA_M);
        }
        order
    }

    pub fn attack_area_target(&self) -> AttackAreaTarget {
        if self.attack_g_targets {
            AttackAreaTarget::GroundTargets
        } else if self.attack_ground {
            AttackAreaTarget::Ground
        } else {
            AttackAreaTarget::Air
        }
    }

    pub fn set_attack_area_target(&mut self, target: AttackAreaTarget) {
        self.attack_air = target == AttackAreaTarget::Air;
        self.attack_ground = target == AttackAreaTarget::Ground;
        self.attack_g_targets = target == AttackAreaTarget::GroundTargets;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackAreaTarget {
    Air,
    Ground,
    GroundTargets,
}

impl AttackAreaTarget {
    pub const ALL: [AttackAreaTarget; 3] = [
        AttackAreaTarget::Air,
        AttackAreaTarget::Ground,
        AttackAreaTarget::GroundTargets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AttackAreaTarget::Air => "Attack Air Targets",
            AttackAreaTarget::Ground => "Attack Ground",
            AttackAreaTarget::GroundTargets => "Attack Ground Targets",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            AttackAreaTarget::Air => "Air",
            AttackAreaTarget::Ground => "Ground",
            AttackAreaTarget::GroundTargets => "Gnd tgt",
        }
    }
}

pub fn priority_label(priority: i32) -> &'static str {
    match priority {
        0 => "Low",
        2 => "High",
        _ => "Medium",
    }
}

/// Second line for a colored order chip: enough to check the tree at a glance.
pub fn order_chip_detail(order: &OrderSpec, unit_kind: UnitKind) -> String {
    let pri = || priority_label(order.priority);
    let time = |s: f32| {
        if s >= 60.0 && (s % 60.0).abs() < 0.05 {
            format!("{}m", (s / 60.0).round() as i32)
        } else if (s.fract()).abs() < 0.05 {
            format!("{}s", s.round() as i32)
        } else {
            format!("{s:.1}s")
        }
    };
    let area = |m: f32| {
        if m >= 1000.0 {
            format!("{:.0}km", m / 1000.0)
        } else {
            format!("{:.0}m", m)
        }
    };
    match order.kind {
        OrderKind::Attack => {
            let tgt = order
                .attack_seat
                .map(|i| format!("S{}", i + 1))
                .unwrap_or_else(|| "no tgt".into());
            format!("{tgt} · {}", pri())
        }
        OrderKind::AttackArea => {
            format!(
                "{} · {} · {} · {}",
                order.attack_area_target().short(),
                area(order.attack_area),
                time(order.time_s),
                pri()
            )
        }
        OrderKind::Cover => {
            let tgt = order
                .cover_lead
                .map(|i| format!("S{}", i + 1))
                .unwrap_or_else(|| "no tgt".into());
            format!("{tgt} · {}", pri())
        }
        OrderKind::ForceComplete | OrderKind::Land => pri().to_string(),
        OrderKind::GotoWaypoint => {
            if order.altitude > 0.0 {
                format!(
                    "WP {} · {:.0}m · {}",
                    order.waypoint.max(1),
                    order.altitude,
                    pri()
                )
            } else {
                format!("WP {} · {}", order.waypoint.max(1), pri())
            }
        }
        OrderKind::Formation => formation_label(order.formation_type, unit_kind),
        OrderKind::TimeOnTarget | OrderKind::Timer => time(order.time_s),
        OrderKind::Behaviour => format!("F{}", order.behaviour_filter),
        OrderKind::Flare => format!("C{}", order.flare_color),
        OrderKind::Effect => {
            if order.effect_start {
                "Start".into()
            } else {
                "Stop".into()
            }
        }
        OrderKind::TakeOff => String::new(),
        OrderKind::MissionComplete => "END".into(),
        OrderKind::RtbOnZoneOut => "Zone Out".into(),
        OrderKind::OnSpawned
        | OrderKind::OnTargetAttacked
        | OrderKind::OnAreaAttacked
        | OrderKind::OnTookOff
        | OrderKind::OnLanded => {
            if order.delay_s > 0.05 {
                time(order.delay_s)
            } else {
                String::new()
            }
        }
    }
}

/// Highest WP number referenced by a Goto WP order (0 if none).
pub fn used_waypoint_count(seats: &[TemplateSeat]) -> u32 {
    seats
        .iter()
        .flat_map(|s| s.orders.iter())
        .filter(|o| o.kind == OrderKind::GotoWaypoint)
        .map(|o| o.waypoint.max(1))
        .max()
        .unwrap_or(0)
}

pub fn next_waypoint_number(seats: &[TemplateSeat]) -> u32 {
    used_waypoint_count(seats) + 1
}

/// Insert a Goto WP order after `after`. The new hop is numbered one past the
/// current highest WP. Events pointing at later orders are shifted. Returns
/// the new order index after chain normalize.
pub fn insert_goto_waypoint_after(
    seats: &mut [TemplateSeat],
    seat: usize,
    after: usize,
) -> usize {
    let n = next_waypoint_number(seats);
    let mut spec = OrderSpec::default();
    spec.kind = OrderKind::GotoWaypoint;
    spec.waypoint = n;
    let idx = (after + 1).min(seats[seat].orders.len());
    seats[seat].orders.insert(idx, spec);
    for hook in &mut seats[seat].events {
        if let EventThen::Order(i) = &mut hook.then {
            if *i >= idx {
                *i += 1;
            }
        }
    }
    normalize_order_chain(&mut seats[seat].orders, &mut seats[seat].events, idx)
}

/// Where an entity event pulses: shared Force Complete, or an order in the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventThen {
    ForceComplete,
    Order(usize),
}

impl EventThen {
    pub fn label(self, orders: &[OrderSpec]) -> String {
        match self {
            EventThen::ForceComplete => "Force Complete".into(),
            EventThen::Order(i) => orders
                .get(i)
                .map(|o| format!("{} {}", i + 1, o.kind.label()))
                .unwrap_or_else(|| format!("Order {}", i + 1)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventHook {
    pub kind: EntityEvent,
    pub then: EventThen,
}

impl EventHook {
    pub fn default_for(kind: UnitKind) -> Self {
        Self {
            kind: EntityEvent::default_for(kind),
            then: EventThen::ForceComplete,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlightRole {
    Independent,
    Lead,
    Follows(usize),
}

/// IL-2 `StartType` for aircraft.
///
/// `0` air, `1` runway (engines running), `2` parking cold, `3` parking warm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneStart {
    Air,
    Running,
    Cold,
    Warm,
}

impl PlaneStart {
    pub const GROUND: [PlaneStart; 3] = [Self::Running, Self::Warm, Self::Cold];

    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Running,
            2 => Self::Cold,
            3 => Self::Warm,
            _ => Self::Air,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Air => 0,
            Self::Running => 1,
            Self::Cold => 2,
            Self::Warm => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Air => "Airstart",
            Self::Running => "Running",
            Self::Warm => "Warm",
            Self::Cold => "Cold",
        }
    }

    /// Status line for the model preview.
    pub fn preview_status(self, altitude: f32) -> String {
        if altitude > 0.0 {
            format!("Airstart · {:.0} m", altitude)
        } else {
            match self {
                Self::Air | Self::Running => "Engine running".into(),
                Self::Warm => "Warm start".into(),
                Self::Cold => "Cold start".into(),
            }
        }
    }

    /// Airborne is always airstart. On the ground, Air becomes Running.
    pub fn stored_for_altitude(stored: i32, altitude: f32) -> i32 {
        if altitude > 0.0 {
            Self::Air.as_i32()
        } else {
            match Self::from_i32(stored) {
                Self::Air => Self::Running.as_i32(),
                other => other.as_i32(),
            }
        }
    }

    /// Value written on a Plane. Airborne always `0`; ground never writes air.
    pub fn written_for_altitude(stored: i32, altitude: f32) -> i32 {
        Self::stored_for_altitude(stored, altitude)
    }
}

/// Airstart height for every aircraft. Ground starts stay at 0 m.
pub const AIR_START_ALTITUDE_M: f32 = 1500.0;

#[derive(Clone, Debug)]
pub struct TemplateSeat {
    pub unit: CatalogUnit,
    pub role: FlightRole,
    pub orders: Vec<OrderSpec>,
    pub events: Vec<EventHook>,
    pub country: i32,
    pub skill: i32,
    pub altitude: f32,
    pub number_in_formation: i32,
    /// When Lead: how many aircraft this flight uses in formation (1..=per_group).
    /// 0 means “use the global units-per-group value”.
    pub formation_count: u32,
    pub fuel: f32,
    pub payload_id: i32,
    /// IL-2 `ModMask` as a binary-looking decimal string (`1`, `11`, `10101`, …).
    pub mod_mask: String,
    pub vulnerable: bool,
    pub engageable: bool,
    pub limit_ammo: bool,
    pub ai_rtb: bool,
    pub start_type: i32,
    /// Selected carriage scripts, running order. Empty = locomotive only.
    pub carriages: Vec<String>,
}

impl TemplateSeat {
    pub fn new(unit: CatalogUnit) -> Self {
        let altitude = if unit.is_air() {
            prop_f32(&unit.object, "YPos", 1000.0).max(0.0)
        } else {
            0.0
        };
        let country = prop_i32(&unit.object, "Country", 501);
        let skill = prop_i32(&unit.object, "AILevel", 2).clamp(0, 4);
        let number_in_formation = prop_i32(&unit.object, "NumberInFormation", 0);
        let fuel = prop_f32(&unit.object, "Fuel", 1.0).clamp(0.0, 1.0);
        let payload_id = prop_i32(&unit.object, "PayloadId", 0);
        let mod_mask = if payloads::empty_mod_mask(&unit.script) == 0 {
            payloads::default_mod_mask_str(&unit.script)
        } else {
            prop_mod_mask(&unit.object)
        };
        let vulnerable = prop_bool(&unit.object, "Vulnerable", true);
        let engageable = prop_bool(&unit.object, "Engageable", true);
        let limit_ammo = prop_bool(&unit.object, "LimitAmmo", true);
        let ai_rtb = prop_bool(&unit.object, "AiRTBDecision", false);
        let start_type = PlaneStart::stored_for_altitude(
            prop_i32(&unit.object, "StartType", 0),
            altitude,
        );
        let carriages = if unit.is_train() {
            unit.default_carriages()
        } else {
            Vec::new()
        };
        Self {
            country,
            skill,
            altitude,
            number_in_formation,
            formation_count: 0,
            fuel,
            payload_id,
            mod_mask,
            vulnerable,
            engageable,
            limit_ammo,
            ai_rtb,
            start_type,
            carriages,
            unit,
            role: FlightRole::Independent,
            orders: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// Last seat marked Lead, if any. New units follow this seat.
pub fn last_lead_index(seats: &[TemplateSeat]) -> Option<usize> {
    seats
        .iter()
        .enumerate()
        .rev()
        .find(|(_, s)| s.role == FlightRole::Lead)
        .map(|(i, _)| i)
}

/// Copy country / skill / fuel / flags from `from` onto every other seat.
/// Payload and modifications copy only onto the same model (same script).
/// Role, model, orders, and formation index are left alone.
pub fn copy_seat_attributes(seats: &mut [TemplateSeat], from: usize) {
    if from >= seats.len() {
        return;
    }
    let src = seats[from].clone();
    for (i, seat) in seats.iter_mut().enumerate() {
        if i == from {
            continue;
        }
        seat.country = src.country;
        seat.skill = src.skill;
        seat.fuel = src.fuel;
        seat.vulnerable = src.vulnerable;
        seat.engageable = src.engageable;
        seat.limit_ammo = src.limit_ammo;
        seat.ai_rtb = src.ai_rtb;
        seat.start_type = src.start_type;
        if same_unit_model(&src.unit, &seat.unit) {
            seat.payload_id = src.payload_id;
            seat.mod_mask = src.mod_mask.clone();
        }
        if src.unit.is_air() && seat.unit.is_air() {
            // Never above the receiving plane's own service ceiling.
            seat.altitude = src
                .altitude
                .min(crate::model_spec::ceiling_m(&seat.unit.script));
        }
        if seat.unit.is_air() {
            seat.start_type = PlaneStart::stored_for_altitude(seat.start_type, seat.altitude);
        }
    }
}

/// Append a unit. Each unit keeps its own country (nation) from the catalog.
/// Skill / fuel flags are copied from the last seat and altitude from the last
/// plane (new planes sit near that height). Payload and modifications are
/// inherited from the last seat of the same model.
/// If a Lead is already set, the new unit follows it and `#` in formation
/// is numbered 1, 2, 3, …
pub fn append_seat(seats: &mut Vec<TemplateSeat>, unit: CatalogUnit, per_group: u32) {
    let index = seats.len();
    let last_plane_alt = seats
        .iter()
        .rev()
        .find(|s| s.unit.is_air())
        .map(|s| s.altitude);
    let follow_lead = last_lead_index(seats);
    let same_model = seats
        .iter()
        .rev()
        .find(|s| same_unit_model(&s.unit, &unit))
        .cloned();
    let prev = seats.last().cloned();
    let mut seat = TemplateSeat::new(unit);
    if let Some(p) = prev {
        // Keep the unit's own country from the catalog (its own nation).
        seat.skill = p.skill;
        seat.fuel = p.fuel;
        seat.vulnerable = p.vulnerable;
        seat.engageable = p.engageable;
        seat.limit_ammo = p.limit_ammo;
        seat.ai_rtb = p.ai_rtb;
        seat.start_type = p.start_type;
    }
    if let Some(p) = same_model {
        seat.payload_id = p.payload_id;
        seat.mod_mask = p.mod_mask.clone();
    }
    if seat.unit.is_air() {
        if let Some(alt) = last_plane_alt {
            seat.altitude = alt;
        }
        seat.start_type = PlaneStart::stored_for_altitude(seat.start_type, seat.altitude);
    } else {
        seat.altitude = 0.0;
    }
    let per = per_group.max(1);
    if let Some(lead) = follow_lead {
        seat.role = FlightRole::Follows(lead);
        seats.push(seat);
        let n = 1 + seats
            .iter()
            .filter(|s| s.role == FlightRole::Follows(lead))
            .count() as u32;
        apply_formation_numbers(seats, lead, n.max(1));
    } else {
        seat.number_in_formation = (index as u32 % per) as i32;
        seats.push(seat);
    }
}

/// Swap the model on an existing seat. Payload / mods reset; country, skill,
/// altitude, and start type stay. Same-kind swaps only (Planes stay Planes).
pub fn replace_seat_unit(seat: &mut TemplateSeat, unit: CatalogUnit) {
    let was_train = seat.unit.is_train();
    seat.unit = unit;
    if seat.unit.is_train() {
        seat.carriages = seat.unit.default_carriages();
    } else if was_train {
        seat.carriages.clear();
    }
    seat.payload_id = 0;
    seat.mod_mask = payloads::default_mod_mask_str(&seat.unit.script);
    if !seat.unit.is_air() {
        seat.altitude = 0.0;
    } else {
        seat.start_type = PlaneStart::stored_for_altitude(seat.start_type, seat.altitude);
    }
}

/// Apply a start choice on one plane. Airstart from the ground uses
/// [`AIR_START_ALTITUDE_M`], the same height for every aircraft, capped at
/// that plane's ceiling. An airstart that already has a height keeps it.
/// A ground start clears altitude. Ground units are left alone.
pub fn apply_plane_start(seat: &mut TemplateSeat, start: PlaneStart) {
    if !seat.unit.is_air() {
        return;
    }
    match start {
        PlaneStart::Air => {
            if seat.altitude <= 0.0 {
                let ceiling = crate::model_spec::ceiling_m(&seat.unit.script);
                seat.altitude = AIR_START_ALTITUDE_M.min(ceiling).max(0.0);
            }
            seat.start_type = PlaneStart::Air.as_i32();
        }
        ground => {
            seat.altitude = 0.0;
            seat.start_type = ground.as_i32();
        }
    }
}

/// Path waypoint `Area` in metres: 300 for any aircraft, 100 for ground-only.
pub fn waypoint_area_m(seats: &[TemplateSeat]) -> &'static str {
    if seats.iter().any(|s| s.unit.is_air()) {
        PLANE_WAYPOINT_AREA_M
    } else {
        GROUND_WAYPOINT_AREA_M
    }
}

fn prop_i32(obj: &Il2Entity, key: &str, default: i32) -> i32 {
    obj.property(key)
        .and_then(|s| s.trim_matches('"').parse().ok())
        .unwrap_or(default)
}

fn prop_f32(obj: &Il2Entity, key: &str, default: f32) -> f32 {
    obj.property(key)
        .and_then(|s| s.trim_matches('"').parse().ok())
        .unwrap_or(default)
}

fn prop_bool(obj: &Il2Entity, key: &str, default: bool) -> bool {
    match obj.property(key) {
        Some("0") => false,
        Some("1") => true,
        _ => default,
    }
}

fn prop_mod_mask(obj: &Il2Entity) -> String {
    obj.property("ModMask")
        .map(|s| s.trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "1".into())
}

fn same_unit_model(a: &CatalogUnit, b: &CatalogUnit) -> bool {
    script_type_id(&a.script).eq_ignore_ascii_case(script_type_id(&b.script))
}

/// Assign NumberInFormation for a lead and its followers: 0, 1, 2, …
pub fn apply_formation_numbers(seats: &mut [TemplateSeat], lead: usize, count: u32) {
    let n = count.max(1);
    if let Some(seat) = seats.get_mut(lead) {
        seat.formation_count = n;
        seat.number_in_formation = 0;
    }
    let mut next = 1i32;
    for (i, seat) in seats.iter_mut().enumerate() {
        if i != lead && seat.role == FlightRole::Follows(lead) {
            seat.number_in_formation = next.min(n.saturating_sub(1) as i32);
            next += 1;
        }
    }
}

pub fn remap_seat_index(slot: &mut Option<usize>, removed: usize) {
    match *slot {
        Some(t) if t == removed => *slot = None,
        Some(t) if t > removed => *slot = Some(t - 1),
        _ => {}
    }
}

pub fn remap_index_vec(ids: &mut Vec<usize>, removed: usize) {
    ids.retain(|&i| i != removed);
    for i in ids.iter_mut() {
        if *i > removed {
            *i -= 1;
        }
    }
}

fn swap_index(i: usize, a: usize, b: usize) -> usize {
    if i == a {
        b
    } else if i == b {
        a
    } else {
        i
    }
}

fn swap_opt_index(slot: &mut Option<usize>, a: usize, b: usize) {
    if let Some(i) = slot {
        *i = swap_index(*i, a, b);
    }
}

fn swap_index_vec(ids: &mut [usize], a: usize, b: usize) {
    for i in ids.iter_mut() {
        *i = swap_index(*i, a, b);
    }
}

/// Move seat `index` one place (`dir` = -1 up, +1 down). Remaps Follows,
/// Cover, Attack, and Also-apply indexes. Returns the new index.
pub fn move_seat(seats: &mut Vec<TemplateSeat>, index: usize, dir: i32) -> Option<usize> {
    let dest = index as i32 + dir;
    if dest < 0 || (dest as usize) >= seats.len() {
        return None;
    }
    let dest = dest as usize;
    seats.swap(index, dest);
    for seat in seats.iter_mut() {
        seat.role = match seat.role {
            FlightRole::Follows(t) => FlightRole::Follows(swap_index(t, index, dest)),
            other => other,
        };
        for order in &mut seat.orders {
            swap_opt_index(&mut order.cover_lead, index, dest);
            swap_opt_index(&mut order.attack_seat, index, dest);
            swap_index_vec(&mut order.shared_with, index, dest);
        }
    }
    Some(dest)
}

fn order_scripts<'a>(seats: &'a [TemplateSeat], owner: usize, order: usize) -> Vec<&'a str> {
    let Some(ord) = seats.get(owner).and_then(|s| s.orders.get(order)) else {
        return Vec::new();
    };
    let mut scripts = vec![seats[owner].unit.script.as_str()];
    for &i in &ord.shared_with {
        if let Some(s) = seats.get(i) {
            scripts.push(s.unit.script.as_str());
        }
    }
    scripts
}

/// Set AttackArea radius from the assigned units' known system ranges.
pub fn apply_suggested_attack_area(seats: &mut [TemplateSeat], owner: usize, order: usize) {
    if seats
        .get(owner)
        .and_then(|s| s.orders.get(order))
        .is_none_or(|o| o.kind != OrderKind::AttackArea)
    {
        return;
    }
    let scripts: Vec<String> = {
        let mut out = vec![seats[owner].unit.script.clone()];
        for &i in &seats[owner].orders[order].shared_with {
            if let Some(s) = seats.get(i) {
                out.push(s.unit.script.clone());
            }
        }
        out
    };
    seats[owner].orders[order].attack_area = weapon_range::suggested_attack_area_m(
        scripts.iter().map(|s| s.as_str()),
        DEFAULT_ATTACK_AREA_M,
    );
}

/// Refresh AttackArea radii that include `si` (owner or Also-apply).
pub fn refresh_attack_areas_for_seat(seats: &mut [TemplateSeat], si: usize) {
    let mut jobs = Vec::new();
    for (owner, seat) in seats.iter().enumerate() {
        for (oi, order) in seat.orders.iter().enumerate() {
            if order.kind == OrderKind::AttackArea && (owner == si || order.shared_with.contains(&si))
            {
                jobs.push((owner, oi));
            }
        }
    }
    for (owner, oi) in jobs {
        apply_suggested_attack_area(seats, owner, oi);
    }
}

/// Shortest known weapon range of units assigned to this AttackArea.
pub fn attack_area_range_limit(seats: &[TemplateSeat], owner: usize, order: usize) -> Option<f64> {
    weapon_range::shortest_range_m(order_scripts(seats, owner, order))
}

pub fn remap_event_then(then: &mut EventThen, removed: usize) {
    match *then {
        EventThen::Order(i) if i == removed => *then = EventThen::ForceComplete,
        EventThen::Order(i) if i > removed => *then = EventThen::Order(i - 1),
        _ => {}
    }
}

/// OnSpawned first; attack / takeoff / land reports sit after the matching command.
/// Remaps event order indexes. Returns the new index of the order that was at `keep`.
pub fn normalize_order_chain(
    orders: &mut Vec<OrderSpec>,
    events: &mut [EventHook],
    keep: usize,
) -> usize {
    let n = orders.len();
    if n == 0 {
        return 0;
    }
    let keep = keep.min(n - 1);
    let tagged: Vec<(usize, OrderSpec)> = orders.drain(..).enumerate().collect();
    let mut spawned = Vec::new();
    let mut commands = Vec::new();
    let mut reports = Vec::new();
    for (i, o) in tagged {
        if o.kind == OrderKind::OnSpawned {
            spawned.push((i, o));
        } else if o.kind.is_report() {
            reports.push((i, o));
        } else {
            commands.push((i, o));
        }
    }
    let mut out: Vec<(usize, OrderSpec)> = spawned;
    let mut used = vec![false; reports.len()];
    for (ci, cmd) in commands {
        let follows = cmd.kind;
        out.push((ci, cmd));
        for (ri, (orig, rep)) in reports.iter().enumerate() {
            if used[ri] {
                continue;
            }
            if rep.kind.report_follows() == Some(follows) {
                used[ri] = true;
                out.push((*orig, rep.clone()));
            }
        }
    }
    for (ri, (orig, rep)) in reports.iter().enumerate() {
        if !used[ri] {
            out.push((*orig, rep.clone()));
        }
    }
    let mut mapping = vec![0usize; n];
    for (new_i, (old_i, _)) in out.iter().enumerate() {
        mapping[*old_i] = new_i;
    }
    for hook in events {
        if let EventThen::Order(i) = hook.then {
            hook.then = EventThen::Order(mapping.get(i).copied().unwrap_or(i));
        }
    }
    let new_keep = mapping[keep];
    *orders = out.into_iter().map(|(_, o)| o).collect();
    new_keep
}

/// One hop for the order tree: a command plus its following report and TOT.
fn take_order_hop(orders: &[OrderSpec], i: &mut usize) -> Vec<usize> {
    if orders[*i].kind.is_wp_parallel() {
        let mut col = vec![*i];
        *i += 1;
        while *i < orders.len() {
            let k = orders[*i].kind;
            if k.is_wp_parallel() || k.is_report() {
                col.push(*i);
                *i += 1;
                continue;
            }
            break;
        }
        return col;
    }
    let mut col = vec![*i];
    *i += 1;
    if *i < orders.len() {
        if let Some(follows) = orders[*i].kind.report_follows() {
            if orders[col[0]].kind == follows {
                col.push(*i);
                *i += 1;
            }
        }
    }
    // TOT belongs with this hop unless an Attack cluster follows (then TOT
    // stacks with the attack instead).
    if *i < orders.len() && orders[*i].kind == OrderKind::TimeOnTarget {
        let attack_next = *i + 1 < orders.len()
            && matches!(
                orders[*i + 1].kind,
                OrderKind::Attack | OrderKind::AttackArea
            );
        if !attack_next {
            col.push(*i);
            *i += 1;
        }
    }
    col
}

/// Columns of the order tree. Sequential hops stay in line; Attack / AttackArea
/// / Time on Target that sit together are stacked as a parallel branch. A TOT
/// that follows a waypoint (or other hop) with no attack sits in that hop's
/// column. OnSpawned is its own column — it really does fire before the next
/// command. Other reports still attach to the hop they wait on.
pub fn order_tree_columns(orders: &[OrderSpec]) -> Vec<Vec<usize>> {
    let mut cols = Vec::new();
    let mut i = 0;
    while i < orders.len() {
        if orders[i].kind == OrderKind::OnSpawned {
            let start = i;
            while i < orders.len() && orders[i].kind == OrderKind::OnSpawned {
                i += 1;
            }
            cols.push((start..i).collect());
            continue;
        }
        if orders[i].kind.is_report() {
            let start = i;
            while i < orders.len() && orders[i].kind.is_report() {
                i += 1;
            }
            let reports: Vec<usize> = (start..i).collect();
            if i < orders.len() {
                let mut col = take_order_hop(orders, &mut i);
                let mut merged = reports;
                merged.append(&mut col);
                cols.push(merged);
            } else if let Some(prev) = cols.last_mut() {
                prev.extend(reports);
            } else {
                cols.push(reports);
            }
            continue;
        }
        cols.push(take_order_hop(orders, &mut i));
    }
    cols
}

/// One cell in the order tree: an order, or an event hooked to that column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderTreeNode {
    Order(usize),
    Event(usize),
}

fn is_stack_satellite(kind: OrderKind) -> bool {
    kind == OrderKind::TimeOnTarget
}

fn column_has_order(col: &[OrderTreeNode], oi: usize) -> bool {
    col.iter()
        .any(|n| matches!(n, OrderTreeNode::Order(i) if *i == oi))
}

/// Visual columns: events above reports above the spine hop, then TOT below.
/// Events that pulse an order sit on the previous hop (OnPlaneTookOff over
/// Take Off, OnBingoBombs over AttackArea). Reports that wait on a command
/// sit on that hop (OnTookOff over Take Off). Force Complete events sit in a
/// trailing column.
pub fn order_tree_layout(orders: &[OrderSpec], events: &[EventHook]) -> Vec<Vec<OrderTreeNode>> {
    let cols = order_tree_columns(orders);
    let mut laid: Vec<Vec<OrderTreeNode>> = cols
        .iter()
        .map(|col| {
            let mut reports = Vec::new();
            let mut primary = Vec::new();
            let mut stacked = Vec::new();
            for &i in col {
                if orders[i].kind == OrderKind::OnSpawned {
                    // Spine row, left of the first command — not stacked on it.
                    primary.push(OrderTreeNode::Order(i));
                } else if orders[i].kind.is_report() {
                    reports.push(OrderTreeNode::Order(i));
                } else if is_stack_satellite(orders[i].kind) {
                    stacked.push(OrderTreeNode::Order(i));
                } else {
                    primary.push(OrderTreeNode::Order(i));
                }
            }
            let mut out = reports;
            out.append(&mut primary);
            out.append(&mut stacked);
            out
        })
        .collect();

    let mut placed = vec![false; events.len()];
    for (ei, hook) in events.iter().enumerate() {
        let EventThen::Order(oi) = hook.then else {
            continue;
        };
        let Some(ci) = laid.iter().position(|col| column_has_order(col, oi)) else {
            continue;
        };
        let host = if ci > 0 { ci - 1 } else { ci };
        let at = laid[host]
            .iter()
            .position(|n| match n {
                OrderTreeNode::Order(i) => !is_stack_satellite(orders[*i].kind),
                OrderTreeNode::Event(_) => false,
            })
            .unwrap_or(0);
        laid[host].insert(at, OrderTreeNode::Event(ei));
        placed[ei] = true;
    }
    let trailing: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(ei, _)| !placed[*ei])
        .map(|(ei, _)| OrderTreeNode::Event(ei))
        .collect();
    if !trailing.is_empty() {
        laid.push(trailing);
    }
    for col in &mut laid {
        sort_events_longest_first(col, events);
        sort_reports_longest_first(col, orders);
    }
    laid
}

fn event_label_len(node: OrderTreeNode, events: &[EventHook]) -> usize {
    match node {
        OrderTreeNode::Event(ei) => events.get(ei).map(|h| h.kind.label().len()).unwrap_or(0),
        OrderTreeNode::Order(_) => 0,
    }
}

/// Longest event chip on top so S-feeds into the trunk don't cross.
fn sort_events_longest_first(col: &mut [OrderTreeNode], events: &[EventHook]) {
    let n = col
        .iter()
        .take_while(|n| matches!(n, OrderTreeNode::Event(_)))
        .count();
    if n > 1 {
        col[..n].sort_by(|a, b| event_label_len(*b, events).cmp(&event_label_len(*a, events)));
    }
}

fn report_label_len(node: OrderTreeNode, orders: &[OrderSpec]) -> usize {
    match node {
        OrderTreeNode::Order(oi) => orders
            .get(oi)
            .filter(|o| o.kind.is_report())
            .map(|o| o.kind.label().len())
            .unwrap_or(0),
        OrderTreeNode::Event(_) => 0,
    }
}

fn top_report_range(col: &[OrderTreeNode], orders: &[OrderSpec]) -> (usize, usize) {
    let start = col
        .iter()
        .take_while(|n| matches!(n, OrderTreeNode::Event(_)))
        .count();
    let n = col[start..]
        .iter()
        .take_while(|n| {
            matches!(
                n,
                OrderTreeNode::Order(i) if orders.get(*i).is_some_and(|o| o.kind.is_report())
            )
        })
        .count();
    (start, n)
}

/// Longest report chip on top, same idea as stacked events.
fn sort_reports_longest_first(col: &mut [OrderTreeNode], orders: &[OrderSpec]) {
    let (start, n) = top_report_range(col, orders);
    if n > 1 {
        col[start..start + n]
            .sort_by(|a, b| report_label_len(*b, orders).cmp(&report_label_len(*a, orders)));
    }
}

/// True when an entity event pulses this order instead of the previous hop.
pub fn event_triggers_order(events: &[EventHook], oi: usize) -> bool {
    events
        .iter()
        .any(|h| matches!(h.then, EventThen::Order(i) if i == oi))
}

fn event_triggered_sources(seats: &[TemplateSeat], owner: usize) -> HashSet<usize> {
    let mut out = HashSet::new();
    for (si, seat) in seats.iter().enumerate() {
        let chain = if receives_orders(seats, si) {
            si
        } else {
            flight_lead_of(seats, si)
        };
        if chain != owner {
            continue;
        }
        for hook in &seat.events {
            if let EventThen::Order(i) = hook.then {
                out.insert(i);
            }
        }
    }
    out
}

/// Set or insert the command that follows a report in the chain.
pub fn set_report_following(
    orders: &mut Vec<OrderSpec>,
    report_idx: usize,
    then_kind: OrderKind,
    unit_kind: UnitKind,
) {
    if report_idx >= orders.len() || !orders[report_idx].kind.is_report() {
        return;
    }
    let next = report_idx + 1;
    if next < orders.len() && !orders[next].kind.is_report() {
        orders[next].kind = then_kind;
        if then_kind == OrderKind::Formation {
            let presets = formations_for(unit_kind);
            if !presets.iter().any(|p| p.id == orders[next].formation_type) {
                orders[next].formation_type = OrderSpec::for_kind(unit_kind).formation_type;
            }
        }
        return;
    }
    let mut spec = OrderSpec::for_kind(unit_kind);
    spec.kind = then_kind;
    if then_kind == OrderKind::Formation {
        spec.formation_type = OrderSpec::for_kind(unit_kind).formation_type;
    }
    orders.insert(next, spec);
}

fn valid_lead(seats: &[TemplateSeat], lead: usize) -> bool {
    matches!(seats.get(lead).map(|s| s.role), Some(FlightRole::Lead))
}

/// Seat this unit takes orders from: a Follows target if that seat is a Lead,
/// otherwise itself (independent).
pub fn flight_lead_of(seats: &[TemplateSeat], index: usize) -> usize {
    match seats.get(index).map(|s| s.role) {
        Some(FlightRole::Follows(lead)) if valid_lead(seats, lead) => lead,
        _ => index,
    }
}

pub fn is_follower(seats: &[TemplateSeat], index: usize) -> bool {
    matches!(
        seats.get(index).map(|s| s.role),
        Some(FlightRole::Follows(lead)) if valid_lead(seats, lead)
    )
}

/// Independent units and flight leads receive orders. Followers do not.
pub fn receives_orders(seats: &[TemplateSeat], index: usize) -> bool {
    !is_follower(seats, index)
}

/// True when at least one unit is a wingman target-linked to a lead.
pub fn has_linked_wingmen(seats: &[TemplateSeat]) -> bool {
    seats
        .iter()
        .enumerate()
        .any(|(i, _)| is_follower(seats, i))
}

pub fn order_seat_indexes(seats: &[TemplateSeat]) -> Vec<usize> {
    seats
        .iter()
        .enumerate()
        .filter(|(i, _)| receives_orders(seats, *i))
        .map(|(i, _)| i)
        .collect()
}

pub fn lead_indexes(seats: &[TemplateSeat]) -> Vec<usize> {
    seats
        .iter()
        .enumerate()
        .filter(|(_, s)| s.role == FlightRole::Lead)
        .map(|(i, _)| i)
        .collect()
}

pub fn lead_has_followers(seats: &[TemplateSeat], lead: usize) -> bool {
    seats.iter().any(|s| s.role == FlightRole::Follows(lead))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceLayout {
    InvertedVee,
    Vee,
    CombatBox,
    Pairs,
    EchelonRight,
    EchelonLeft,
    LineAbreast,
    Column,
}

impl PlaceLayout {
    pub const ALL: [PlaceLayout; 8] = [
        PlaceLayout::InvertedVee,
        PlaceLayout::Vee,
        PlaceLayout::CombatBox,
        PlaceLayout::Pairs,
        PlaceLayout::EchelonRight,
        PlaceLayout::EchelonLeft,
        PlaceLayout::LineAbreast,
        PlaceLayout::Column,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PlaceLayout::InvertedVee => "Inverted Vee (finger-four)",
            PlaceLayout::Vee => "Vee",
            PlaceLayout::CombatBox => "Combat Box",
            PlaceLayout::Pairs => "Pairs",
            PlaceLayout::EchelonRight => "Echelon right",
            PlaceLayout::EchelonLeft => "Echelon left",
            PlaceLayout::LineAbreast => "Line abreast",
            PlaceLayout::Column => "Column",
        }
    }

    /// Default **Per group** when this layout is chosen in Template Builder.
    pub fn default_per_group(self) -> u32 {
        match self {
            PlaceLayout::CombatBox => 6,
            _ => 4,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TemplateOptions {
    pub name: String,
    pub zone_in: f32,
    pub zone_out: f32,
    pub spacing: f32,
    pub seats: Vec<TemplateSeat>,
    pub place_layout: PlaceLayout,
    pub per_group: u32,
    pub bring_up: BringUp,
    pub allow_multiple_spawns: bool,
    pub spawn_cooldown_min: f32,
    /// Unused by generate: waypoint MCUs come from Goto WP orders.
    pub waypoint_count: u32,
    pub waypoint_spacing: f32,
    pub waypoint_speed: f32,
    /// Path waypoint YPos. 0 = ground, or the first plane's spawn altitude.
    pub waypoint_altitude: f32,
    /// MCU_Waypoint Priority (0 Low / 1 Medium / 2 High).
    pub waypoint_priority: i32,
    pub zone_coalition: ZoneCoalition,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            name: "Unit Template".into(),
            zone_in: AIR_ZONE_IN_M,
            zone_out: AIR_ZONE_OUT_M,
            spacing: PLACEMENT_SPACING,
            seats: Vec::new(),
            place_layout: PlaceLayout::InvertedVee,
            per_group: 4,
            bring_up: BringUp::Activate,
            allow_multiple_spawns: false,
            spawn_cooldown_min: 5.0,
            waypoint_count: 0,
            waypoint_spacing: WAYPOINT_SPACING_M,
            waypoint_speed: 100.0,
            waypoint_altitude: 0.0,
            waypoint_priority: 1,
            zone_coalition: ZoneCoalition::Western,
        }
    }
}

/// World offset from the first seat. X is north (forward), Z is east (right).
pub fn place_offset(
    layout: PlaceLayout,
    index: usize,
    per_group: usize,
    spacing: f64,
) -> (f64, f64) {
    let per = per_group.max(1);
    let group = index / per;
    let seat = index % per;
    let group_back = -(group as f64) * spacing * 3.5;
    let group_side = (group as f64) * spacing * 2.5;
    let (dx, dz) = layout_seat(layout, seat, per, spacing);
    (group_back + dx, group_side + dz)
}

fn layout_seat(layout: PlaceLayout, seat: usize, per: usize, spacing: f64) -> (f64, f64) {
    match layout {
        PlaceLayout::InvertedVee => inverted_vee_seat(seat, spacing),
        PlaceLayout::CombatBox => combat_box_seat(seat, spacing),
        PlaceLayout::Vee => {
            if seat == 0 {
                (0.0, 0.0)
            } else {
                let row = ((seat + 1) / 2) as f64;
                let right = seat % 2 == 1;
                (
                    -spacing * 0.70 * row,
                    if right {
                        spacing * row
                    } else {
                        -spacing * row
                    },
                )
            }
        }
        PlaceLayout::Pairs => {
            let pair = (seat / 2) as f64;
            let right = seat % 2 == 1;
            (
                -spacing * 1.2 * pair,
                if right { spacing * 0.5 } else { -spacing * 0.5 },
            )
        }
        PlaceLayout::EchelonRight => (-spacing * 0.5 * seat as f64, spacing * seat as f64),
        PlaceLayout::EchelonLeft => (-spacing * 0.5 * seat as f64, -spacing * seat as f64),
        PlaceLayout::LineAbreast => {
            let mid = per.saturating_sub(1) as f64 / 2.0;
            (0.0, (seat as f64 - mid) * spacing)
        }
        PlaceLayout::Column => (-spacing * seat as f64, 0.0),
    }
}

fn inverted_vee_seat(seat: usize, spacing: f64) -> (f64, f64) {
    let cluster = seat / 4;
    let inner = seat % 4;
    let (dx, dz) = match inner {
        0 => (0.0, 0.0),
        1 => (-spacing * 0.35, spacing),
        2 => (-spacing * 0.70, -spacing),
        _ => (-spacing * 1.05, -spacing * 2.0),
    };
    let back = -(cluster as f64) * spacing * 2.0;
    let side = (cluster as f64) * spacing * 1.5;
    (back + dx, side + dz)
}

/// Six-ship combat box: two 3-plane vees, second element back and staggered right.
fn combat_box_seat(seat: usize, spacing: f64) -> (f64, f64) {
    let cluster = seat / 6;
    let inner = seat % 6;
    let (dx, dz) = match inner {
        0 => (0.0, 0.0),
        1 => (-spacing * 0.70, spacing),
        2 => (-spacing * 0.70, -spacing),
        3 => (-spacing * 2.00, spacing * 0.50),
        4 => (-spacing * 2.70, spacing * 1.50),
        _ => (-spacing * 2.70, -spacing * 0.50),
    };
    let back = -(cluster as f64) * spacing * 3.0;
    let side = (cluster as f64) * spacing * 2.0;
    (back + dx, side + dz)
}

/// World offset from the flight lead for seat `index` in stacked finger-fours.
/// X is north (forward), Z is east (right). Seat 0 is lead.
pub fn finger_four_offset(index: usize, spacing: f64) -> (f64, f64) {
    place_offset(PlaceLayout::InvertedVee, index, 4, spacing)
}

/// Built-in aircraft types as catalog entries so the mode works before a
/// user catalog is loaded.
pub fn builtin_plane_catalog() -> Vec<CatalogUnit> {
    AIRCRAFT_TYPES
        .iter()
        .map(synthetic_plane_unit)
        .collect()
}

/// Planes, vehicles, infantry, trains, ships, and fixed objects from the bundled catalogs.
pub fn bundled_catalog() -> Vec<CatalogUnit> {
    let mut cat = match parse_il2_document(include_str!("../assets/Models.Group")) {
        Ok(root) => load_catalog(&root),
        Err(_) => Vec::new(),
    };
    // infantry.Group carries the correct Country on each squad (Models.Group
    // stamps every vehicle 601). Overlay so Template Builder can group by nation.
    if let Ok(root) = parse_il2_document(include_str!("../TemplateExamples/infantry.Group")) {
        overlay_catalog(&mut cat, load_catalog(&root));
    }
    for plane in builtin_plane_catalog() {
        if let Some(existing) = cat
            .iter_mut()
            .find(|u| u.script.eq_ignore_ascii_case(&plane.script))
        {
            existing.display = plane.display.clone();
            existing.name = plane.name.clone();
        } else {
            cat.push(plane);
        }
    }
    if cat.is_empty() {
        builtin_plane_catalog()
    } else {
        cat
    }
}

/// Read a catalog `.Group`. Preferred layout is subgroups named
/// `Planes` / `All Planes`, `Vehicles` / `All Vehicles`, `Infantry`,
/// `Trains` / `All Trains`, `Ships` / `All Ships`, `Fixed` / `Fixed Units` /
/// `Fixed Objects`, and `User Added`. Loose Plane / Vehicle /
/// Train / Ship blocks at any depth are also collected. Infantry are
/// `Vehicle` objects (squad scripts / `infantry` model path) shown as their
/// own kind even though the editor writes them as vehicles.
pub fn load_catalog(root: &Il2Entity) -> Vec<CatalogUnit> {
    let mut out = Vec::new();
    collect_kind_group(root, "Planes", UnitKind::Plane, &mut out);
    collect_kind_group(root, "All Planes", UnitKind::Plane, &mut out);
    collect_kind_group(root, "Aircraft", UnitKind::Plane, &mut out);
    collect_kind_group(root, "Vehicles", UnitKind::Vehicle, &mut out);
    collect_kind_group(root, "All Vehicles", UnitKind::Vehicle, &mut out);
    collect_kind_group(root, "Infantry", UnitKind::Infantry, &mut out);
    collect_kind_group(root, "All Infantry", UnitKind::Infantry, &mut out);
    collect_kind_group(root, "Trains", UnitKind::Train, &mut out);
    collect_kind_group(root, "All Trains", UnitKind::Train, &mut out);
    collect_kind_group(root, "Ships", UnitKind::Ship, &mut out);
    collect_kind_group(root, "All Ships", UnitKind::Ship, &mut out);
    collect_kind_group(root, "Fixed", UnitKind::Fixed, &mut out);
    collect_kind_group(root, "Fixed Units", UnitKind::Fixed, &mut out);
    collect_kind_group(root, "Fixed Objects", UnitKind::Fixed, &mut out);
    collect_user_added_group(root, &mut out);
    if out.is_empty() {
        collect_loose(root, &mut out);
    } else {
        collect_objects(root, UnitKind::Infantry, &mut out);
    }
    out
}

/// Load a catalog and tag every prototype as **User Added** so extra groups
/// can be appended without replacing the built-in list.
pub fn load_catalog_as_user_added(root: &Il2Entity) -> Vec<CatalogUnit> {
    let mut cat = load_catalog(root);
    if cat.is_empty() {
        collect_loose(root, &mut cat);
    }
    for unit in &mut cat {
        unit.kind = UnitKind::UserAdded;
    }
    cat
}

pub fn merge_catalog(into: &mut Vec<CatalogUnit>, extra: Vec<CatalogUnit>) {
    for unit in extra {
        if into.iter().any(|e| {
            e.script.eq_ignore_ascii_case(&unit.script) && e.kind == unit.kind
        }) {
            continue;
        }
        into.push(unit);
    }
}

/// Replace matching prototypes (same script) so a later catalog can correct
/// Country / name; append anything new.
fn overlay_catalog(into: &mut Vec<CatalogUnit>, extra: Vec<CatalogUnit>) {
    for unit in extra {
        if let Some(existing) = into.iter_mut().find(|e| {
            e.script.eq_ignore_ascii_case(&unit.script)
        }) {
            *existing = unit;
        } else {
            into.push(unit);
        }
    }
}

fn collect_user_added_group(root: &Il2Entity, out: &mut Vec<CatalogUnit>) {
    let mut groups = Vec::new();
    find_groups_named(root, "User Added", &mut groups);
    for g in groups {
        let mut tmp = Vec::new();
        collect_objects(g, UnitKind::Plane, &mut tmp);
        collect_objects(g, UnitKind::Vehicle, &mut tmp);
        collect_objects(g, UnitKind::Infantry, &mut tmp);
        collect_objects(g, UnitKind::Train, &mut tmp);
        collect_objects(g, UnitKind::Ship, &mut tmp);
        collect_objects(g, UnitKind::Fixed, &mut tmp);
        for mut unit in tmp {
            unit.kind = UnitKind::UserAdded;
            if !out.iter().any(|e| {
                e.script.eq_ignore_ascii_case(&unit.script) && e.kind == UnitKind::UserAdded
            }) {
                out.push(unit);
            }
        }
    }
}

fn collect_kind_group(root: &Il2Entity, name: &str, kind: UnitKind, out: &mut Vec<CatalogUnit>) {
    let mut groups = Vec::new();
    find_groups_named(root, name, &mut groups);
    for g in groups {
        collect_objects(g, kind, out);
    }
}

fn find_groups_named<'a>(e: &'a Il2Entity, name: &str, out: &mut Vec<&'a Il2Entity>) {
    if e.block_type == "Group" && e.name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
        out.push(e);
    }
    for c in &e.children {
        find_groups_named(c, name, out);
    }
}

fn collect_loose(root: &Il2Entity, out: &mut Vec<CatalogUnit>) {
    collect_objects(root, UnitKind::Plane, out);
    collect_objects(root, UnitKind::Vehicle, out);
    collect_objects(root, UnitKind::Infantry, out);
    collect_objects(root, UnitKind::Train, out);
    collect_objects(root, UnitKind::Ship, out);
    collect_objects(root, UnitKind::Fixed, out);
}

fn is_fixed_script(script: &str) -> bool {
    script.to_ascii_lowercase().contains("fixedobjects")
}

fn collect_objects(root: &Il2Entity, kind: UnitKind, out: &mut Vec<CatalogUnit>) {
    let mut objects = Vec::new();
    collect_block(root, kind.object_type(), &mut objects);
    if kind == UnitKind::Train && objects.is_empty() {
        collect_block(root, "Vehicle", &mut objects);
        objects.retain(|o| {
            o.property("Script")
                .is_some_and(|s| s.to_ascii_lowercase().contains("train"))
        });
    }
    if kind == UnitKind::Fixed {
        if objects.is_empty() {
            collect_block(root, "Vehicle", &mut objects);
        }
        objects.retain(|o| {
            o.property("Script")
                .is_some_and(|s| is_fixed_script(s))
        });
    }
    if kind == UnitKind::Vehicle {
        objects.retain(|o| {
            !o.property("Script")
                .is_some_and(|s| is_fixed_script(s) || weapon_range::is_infantry_script(s))
        });
    }
    if kind == UnitKind::Infantry {
        objects.retain(|o| {
            o.property("Script")
                .is_some_and(weapon_range::is_infantry_script)
        });
    }
    for obj in objects {
        if out.iter().any(|u| {
            u.object.index.is_some() && u.object.index == obj.index && obj.index.is_some()
        }) {
            continue;
        }
        let entity = linked_entity(root, obj).unwrap_or_else(synthetic_entity);
        let name = obj.name().unwrap_or("").to_string();
        let script = obj
            .property("Script")
            .map(|s| s.trim_matches('"').to_string())
            .unwrap_or_default();
        let display = display_name(&name, &script);
        out.push(CatalogUnit {
            kind,
            name,
            script,
            display,
            object: obj.clone(),
            entity,
        });
    }
}

fn display_name(name: &str, script: &str) -> String {
    if let Some(spec) = crate::model_spec::spec_for(script) {
        return spec.label.to_string();
    }
    let generic = name.is_empty()
        || matches!(
            name,
            "Plane" | "Vehicle" | "Train" | "Ship" | "MCU_TR_Entity"
        );
    if generic {
        script_type_id(script).to_string()
    } else {
        name.to_string()
    }
}

fn collect_block<'a>(e: &'a Il2Entity, block: &str, out: &mut Vec<&'a Il2Entity>) {
    if e.block_type == block {
        out.push(e);
    }
    for c in &e.children {
        collect_block(c, block, out);
    }
}

fn linked_entity(root: &Il2Entity, obj: &Il2Entity) -> Option<Il2Entity> {
    let link = obj.property("LinkTrId")?.parse::<i32>().ok()?;
    find_index(root, link).cloned()
}

fn find_index<'a>(e: &'a Il2Entity, index: i32) -> Option<&'a Il2Entity> {
    if e.index == Some(index) {
        return Some(e);
    }
    e.children.iter().find_map(|c| find_index(c, index))
}

fn left_timer(branch: usize, row: usize) -> (f64, f64) {
    (
        ORIGIN_X - MCU_GAP * row as f64,
        ORIGIN_Z - MCU_GAP - BRANCH_GAP * branch as f64,
    )
}

fn left_order(branch: usize, row: usize) -> (f64, f64) {
    let (x, z) = left_timer(branch, row);
    (x, z - MCU_GAP)
}

fn right_timer(branch: usize, row: usize) -> (f64, f64) {
    (
        ORIGIN_X - MCU_GAP * row as f64,
        ORIGIN_Z + MCU_GAP + BRANCH_GAP * branch as f64,
    )
}

fn right_order(branch: usize, row: usize) -> (f64, f64) {
    let (x, z) = right_timer(branch, row);
    (x, z + MCU_GAP)
}

fn western_country(country: i32) -> bool {
    country / 100 == 6
}

fn rtb_destination(western: bool, group: usize) -> (f64, f64) {
    let side = if western { 1.0 } else { -1.0 };
    (
        ORIGIN_X - 5_000.0,
        ORIGIN_Z + side * (2_000.0 + group as f64 * BRANCH_GAP),
    )
}

/// Generate one proximity-triggered unit group.
pub fn generate_template(opts: &TemplateOptions) -> Result<Il2Entity, String> {
    if opts.seats.is_empty() {
        return Err("add at least one unit.".into());
    }
    let zone_in = opts.zone_in.max(200.0) as f64;
    let zone_out = opts.zone_out.max(opts.zone_in + 200.0) as f64;
    let spacing = opts.spacing.max(10.0) as f64;
    let per_group = opts.per_group.max(1) as usize;
    let path_altitude = path_waypoint_altitude(opts);
    let coalitions = opts.zone_coalition.plane_coalitions();
    let wants_rtb = opts
        .seats
        .iter()
        .any(|s| s.orders.iter().any(|o| o.kind == OrderKind::RtbOnZoneOut));
    let mut next_id = 1i32;

    let mut root = named_group(&opts.name, &mut next_id);
    let mut logic = named_group("Logic", &mut next_id);
    let mut units_g = named_group("Units", &mut next_id);
    let mut orders_g = named_group("Orders", &mut next_id);
    let mut wps_g = named_group("Waypoints", &mut next_id);

    let mut placed = Vec::new();
    for (i, seat) in opts.seats.iter().enumerate() {
        let (dx, dz) = place_offset(opts.place_layout, i, per_group, spacing);
        let y = if seat.unit.is_air() {
            seat.altitude as f64
        } else {
            0.0
        };
        placed.push(place_unit(seat, i, ORIGIN_X + dx, y, ORIGIN_Z + dz, per_group, &mut next_id)?);
    }

    for i in 0..placed.len() {
        if is_follower(&opts.seats, i) {
            let lead = flight_lead_of(&opts.seats, i);
            let lead_eid = placed[lead].entity_id;
            placed[i].entity.set_targets(vec![lead_eid]);
        }
    }

    let linked_wingmen = has_linked_wingmen(&opts.seats);
    let bring_up = if linked_wingmen {
        BringUp::Activate
    } else {
        opts.bring_up
    };

    let entity_ids: Vec<i32> = placed.iter().map(|p| p.entity_id).collect();
    let object_ids: Vec<i32> = placed.iter().map(|p| p.object_id).collect();
    let lead_indexes = order_seat_indexes(&opts.seats);
    let lead_entity_ids: Vec<i32> = lead_indexes.iter().map(|&i| placed[i].entity_id).collect();

    let (zx, zz) = (ORIGIN_X, ORIGIN_Z);
    let mut begin = mcu(
        "MCU_TR_MissionBegin",
        "Translator Mission Begin",
        &mut next_id,
        ORIGIN_X + MCU_GAP,
        ORIGIN_Z,
    );
    begin.set_property("Enabled", "1");

    let mut pulse = timer("ENABLE / PULSE IN", 0.1, &mut next_id, zx, zz);
    let mut pulse_out = timer("PULSE OUT", 0.1, &mut next_id, zx, zz);

    let mut zone_in_mcu = checkzone("Zone IN", zone_in, true, coalitions, &mut next_id, zx, zz);
    let mut zone_out_mcu = checkzone("Zone Out", zone_out, false, coalitions, &mut next_id, zx, zz);

    let mut deact_in = mcu("MCU_Deactivate", "Self Deactivate", &mut next_id, zx, zz);
    let mut react_out = mcu("MCU_Activate", "Zone Out ReActivate", &mut next_id, zx, zz);
    let mut deact_out = mcu("MCU_Deactivate", "Self Deactivate", &mut next_id, zx, zz);
    let mut react_in = mcu("MCU_Activate", "Zone In ReActivate", &mut next_id, zx, zz);
    let repeat = bring_up == BringUp::Spawn && opts.allow_multiple_spawns;
    let cooldown_s = if repeat {
        opts.spawn_cooldown_min.max(0.0) as f64 * 60.0
    } else {
        0.0
    };
    let mut cooldown = timer("COOLDOWN", cooldown_s, &mut next_id, zx, zz);

    let (bring_tx, bring_tz) = left_timer(0, 0);
    let (bring_ox, bring_oz) = left_order(0, 0);
    let mut activate = None;
    let mut spawn = None;
    let mut spawn_count = None;
    if bring_up == BringUp::Activate {
        let mut a = mcu(
            "MCU_Activate",
            "Activate Units",
            &mut next_id,
            bring_ox,
            bring_oz,
        );
        a.set_objects(entity_ids.clone());
        activate = Some(a);
    }
    if bring_up == BringUp::Spawn {
        let mut s = mcu(
            "MCU_Spawner",
            "Trigger Spawner",
            &mut next_id,
            bring_ox,
            bring_oz,
        );
        s.set_property("SpawnAtMe", "0");
        s.set_objects(entity_ids.clone());
        let drop = i32::from(repeat);
        let c = counter("SpawnCount", 1, drop, &mut next_id, bring_ox, bring_oz);
        spawn = Some(s);
        spawn_count = Some(c);
    }

    let mut death_count = None;
    let mut death_on = None;
    let mut death_off = None;
    let mut reset_counter = None;
    let mut reset_modifier = None;
    if repeat {
        death_count = Some(counter(
            "DeathCount",
            opts.seats.len().max(1) as i32,
            1,
            &mut next_id,
            zx,
            zz,
        ));
        death_on = Some(mcu("MCU_Activate", "DeathCount ReActivate", &mut next_id, zx, zz));
        death_off = Some(mcu(
            "MCU_Deactivate",
            "DeathCount Deactivate",
            &mut next_id,
            zx,
            zz,
        ));
        reset_modifier = Some(modifier_set_val(
            "Modifier Set Value",
            &mut next_id,
            zx,
            zz,
        ));
        reset_counter = Some(timer("Reset Counter", 0.5, &mut next_id, zx, zz));
    }

    let bring_up_name = match bring_up {
        BringUp::Activate => "MISSION BEGIN",
        BringUp::Spawn => "SPAWN UNITS",
    };
    let mut bring_up_timer = timer(bring_up_name, BRING_UP_DELAY, &mut next_id, bring_tx, bring_tz);
    let (after_tx, after_tz) = left_timer(0, 1);
    let mut after_bring_up = timer(
        "AFTER BRING UP",
        AFTER_BRING_UP_DELAY,
        &mut next_id,
        after_tx,
        after_tz,
    );

    let (end_tx, end_tz) = right_timer(0, 0);
    let (end_ox, end_oz) = right_order(0, 1);
    let (deact_ox, deact_oz) = right_order(0, 2);
    let (del_ox, del_oz) = right_order(0, 3);
    let mut force = mcu(
        "MCU_CMD_ForceComplete",
        "Force Complete - High",
        &mut next_id,
        end_ox,
        end_oz,
    );
    force.set_property("Priority", "2");
    force.set_property("EmergencyOrdnanceDrop", "0");
    force.set_objects(lead_entity_ids.clone());

    let mut deactivate_units = mcu(
        "MCU_Deactivate",
        "Deactivate Units",
        &mut next_id,
        deact_ox,
        deact_oz,
    );
    deactivate_units.set_objects(entity_ids.clone());

    let mut delete = mcu(
        "MCU_Delete",
        "Trigger Delete",
        &mut next_id,
        del_ox,
        del_oz,
    );
    delete.set_objects(object_ids.clone());

    let mut mission_end = timer("MISSION END", MISSION_END_TIME, &mut next_id, end_tx, end_tz);
    let (meo_tx, meo_tz) = right_timer(0, 1);
    let mut mission_end_orders = timer(
        "MISSION END ORDERS",
        MISSION_END_ORDERS_TIME,
        &mut next_id,
        meo_tx,
        meo_tz,
    );
    let delayed_time = if wants_rtb {
        RTB_DEACTIVATE_DELAY
    } else {
        DELAYED_END_TIME
    };
    let (del_tx, del_tz) = right_timer(0, 2);
    let mut delayed_end = timer("DELAYED END ORDERS", delayed_time, &mut next_id, del_tx, del_tz);
    let (dwait_tx, dwait_tz) = right_timer(0, 3);
    let mut delete_wait = timer("DELETE DELAY", DELETE_WAIT, &mut next_id, dwait_tx, dwait_tz);

    let mut rtb_wait = None;
    let mut rtb_waypoints: Vec<Il2Entity> = Vec::new();
    if wants_rtb {
        let (rtb_tx, rtb_tz) = right_timer(0, 4);
        rtb_wait = Some(timer("RTB DELAY", RTB_DELAY, &mut next_id, rtb_tx, rtb_tz));
        let mut buckets: Vec<(bool, usize, Vec<i32>)> = Vec::new();
        for (si, seat) in opts.seats.iter().enumerate() {
            if !seat
                .orders
                .iter()
                .any(|o| o.kind == OrderKind::RtbOnZoneOut)
            {
                continue;
            }
            let owner = if receives_orders(&opts.seats, si) {
                si
            } else {
                flight_lead_of(&opts.seats, si)
            };
            let western = western_country(opts.seats[owner].country);
            let group = owner / per_group;
            let eid = placed[owner].entity_id;
            if let Some(existing) = buckets.iter_mut().find(|(w, g, _)| *w == western && *g == group)
            {
                if !existing.2.contains(&eid) {
                    existing.2.push(eid);
                }
            } else {
                buckets.push((western, group, vec![eid]));
            }
        }
        for (western, group, objects) in buckets {
            let (x, z) = rtb_destination(western, group);
            let name = if western {
                format!("RTB West {}", group + 1)
            } else {
                format!("RTB East {}", group + 1)
            };
            let mut wp = mcu("MCU_Waypoint", &name, &mut next_id, x, z);
            wp.set_property("Area", "1000");
            wp.set_property("Speed", format!("{:.0}", opts.waypoint_speed));
            wp.set_property("Priority", "2");
            wp.set_objects(objects);
            wp.set_property("YPos", format!("{:.3}", path_altitude));
            rtb_waypoints.push(wp);
        }
    }

    let mut emitted: Vec<Vec<EmittedOrder>> = Vec::new();
    let mut order_branch = 0usize;
    for (si, seat) in opts.seats.iter().enumerate() {
        let mut steps = Vec::new();
        if !receives_orders(&opts.seats, si) {
            emitted.push(steps);
            continue;
        }
        let branch = order_branch;
        let will_emit = seat.orders.iter().any(|o| {
            o.kind != OrderKind::RtbOnZoneOut
                && (seat.unit.is_air() || OrderKind::available(seat.unit.kind).contains(&o.kind))
        });
        if will_emit {
            order_branch += 1;
        }
        for (orig_i, order) in seat.orders.iter().enumerate() {
            if order.kind == OrderKind::RtbOnZoneOut {
                continue;
            }
            if !seat.unit.is_air() && !OrderKind::available(seat.unit.kind).contains(&order.kind) {
                continue;
            }
            let oi = steps.len();
            let row = 3 + oi;
            let (tx, tz) = left_timer(branch, row);
            let (ox, oz) = if order.kind == OrderKind::AttackArea
                && (order.attack_ground || order.attack_g_targets)
            {
                (ORIGIN_X, ORIGIN_Z)
            } else {
                left_order(branch, row)
            };
            let wait = match order.kind {
                OrderKind::TimeOnTarget | OrderKind::Timer => order.time_s.max(0.0) as f64,
                _ => order.delay_s.max(0.0) as f64,
            };
            let delay = timer(
                &format!("{} {}", order.kind.label(), si + 1),
                wait,
                &mut next_id,
                tx,
                tz,
            );
            if order.kind.is_report() {
                steps.push(EmittedOrder {
                    delay,
                    cmd: None,
                    goto_wp: None,
                    report: order.kind.report_type(),
                    source_index: orig_i,
                    time_on_target: false,
                    mission_complete: false,
                    pause: false,
                });
                continue;
            }
            if order.kind == OrderKind::GotoWaypoint {
                steps.push(EmittedOrder {
                    delay,
                    cmd: None,
                    goto_wp: Some(order.waypoint.max(1)),
                    report: None,
                    source_index: orig_i,
                    time_on_target: false,
                    mission_complete: false,
                    pause: false,
                });
                continue;
            }
            if order.kind == OrderKind::TimeOnTarget {
                steps.push(EmittedOrder {
                    delay,
                    cmd: None,
                    goto_wp: None,
                    report: None,
                    source_index: orig_i,
                    time_on_target: true,
                    mission_complete: false,
                    pause: false,
                });
                continue;
            }
            if order.kind == OrderKind::Timer {
                steps.push(EmittedOrder {
                    delay,
                    cmd: None,
                    goto_wp: None,
                    report: None,
                    source_index: orig_i,
                    time_on_target: false,
                    mission_complete: false,
                    pause: true,
                });
                continue;
            }
            if order.kind == OrderKind::MissionComplete {
                steps.push(EmittedOrder {
                    delay,
                    cmd: None,
                    goto_wp: None,
                    report: None,
                    source_index: orig_i,
                    time_on_target: false,
                    mission_complete: true,
                    pause: false,
                });
                continue;
            }
            let Some(block) = order.kind.block_type() else {
                steps.push(EmittedOrder {
                    delay,
                    cmd: None,
                    goto_wp: None,
                    report: None,
                    source_index: orig_i,
                    time_on_target: false,
                    mission_complete: false,
                    pause: false,
                });
                continue;
            };
            let mut cmd = build_order(order, block, seat.unit.kind, &mut next_id, ox, oz);
            cmd.set_objects(command_object_ids(order, si, &placed));
            if order.kind == OrderKind::Cover {
                wire_other_unit(&mut cmd, &placed, &opts.seats, si, order.cover_lead);
                let group = i32::from(lead_has_followers(&opts.seats, si));
                cmd.set_property("CoverGroup", group.to_string());
            }
            if order.kind == OrderKind::Attack {
                wire_other_unit(&mut cmd, &placed, &opts.seats, si, order.attack_seat);
            }
            steps.push(EmittedOrder {
                delay,
                cmd: Some(cmd),
                goto_wp: None,
                report: None,
                source_index: orig_i,
                time_on_target: false,
                mission_complete: false,
                pause: false,
            });
        }
        emitted.push(steps);
    }

    let wp_count = used_waypoint_count(&opts.seats);
    let uses_goto = wp_count > 0;

    let mut waypoints = Vec::new();
    for w in 0..wp_count {
        let mut wp = mcu(
            "MCU_Waypoint",
            &format!("WP {}", w + 1),
            &mut next_id,
            ORIGIN_X + (w as f64 + 1.0) * opts.waypoint_spacing as f64,
            ORIGIN_Z,
        );
        wp.set_property("Area", waypoint_area_m(&opts.seats));
        wp.set_property("Speed", format!("{:.0}", opts.waypoint_speed));
        wp.set_property(
            "Priority",
            hop_waypoint_priority(&opts.seats, (w as u32) + 1, opts.waypoint_priority).to_string(),
        );
        wp.set_objects(lead_entity_ids.clone());
        let y = hop_waypoint_altitude(&opts.seats, (w as u32) + 1)
            .map(|a| a as f64)
            .unwrap_or(path_altitude);
        wp.set_property("YPos", format!("{y:.3}"));
        waypoints.push(wp);
    }

    if uses_goto {
        let mut wp_objects: Vec<Vec<i32>> = vec![Vec::new(); waypoints.len()];
        for (si, seat) in opts.seats.iter().enumerate() {
            if !receives_orders(&opts.seats, si) {
                continue;
            }
            for order in &seat.orders {
                if order.kind != OrderKind::GotoWaypoint {
                    continue;
                }
                let idx = order.waypoint.max(1) as usize - 1;
                if idx >= wp_objects.len() {
                    continue;
                }
                let eid = placed[si].entity_id;
                if !wp_objects[idx].contains(&eid) {
                    wp_objects[idx].push(eid);
                }
            }
        }
        for (i, wp) in waypoints.iter_mut().enumerate() {
            wp.set_objects(wp_objects[i].clone());
        }
    }

    let zone_in_id = zone_in_mcu.index.unwrap();
    let zone_out_id = zone_out_mcu.index.unwrap();
    let pulse_id = pulse.index.unwrap();
    let pulse_out_id = pulse_out.index.unwrap();
    let force_id = force.index.unwrap();
    let deactivate_id = deactivate_units.index.unwrap();
    let delete_id = delete.index.unwrap();
    let mission_end_id = mission_end.index.unwrap();
    let mission_end_orders_id = mission_end_orders.index.unwrap();
    let delayed_end_id = delayed_end.index.unwrap();
    let rtb_wait_id = rtb_wait.as_ref().and_then(|t| t.index);
    let delete_wait_id = delete_wait.index.unwrap();
    let bring_up_timer_id = bring_up_timer.index.unwrap();
    let after_bring_up_id = after_bring_up.index.unwrap();
    let deact_in_id = deact_in.index.unwrap();
    let react_out_id = react_out.index.unwrap();
    let deact_out_id = deact_out.index.unwrap();
    let react_in_id = react_in.index.unwrap();
    let cooldown_id = cooldown.index.unwrap();
    let activate_id = activate.as_ref().and_then(|a| a.index);
    let spawn_id = spawn.as_ref().and_then(|s| s.index);
    let spawn_count_id = spawn_count.as_ref().and_then(|c| c.index);
    let death_count_id = death_count.as_ref().and_then(|c| c.index);
    let death_on_id = death_on.as_ref().and_then(|m| m.index);
    let death_off_id = death_off.as_ref().and_then(|m| m.index);
    let reset_counter_id = reset_counter.as_ref().and_then(|t| t.index);
    let reset_modifier_id = reset_modifier.as_ref().and_then(|m| m.index);

    begin.set_targets(vec![pulse_id]);
    pulse.set_targets(vec![zone_in_id]);
    pulse_out.set_targets(vec![zone_out_id]);

    let mut zone_in_targets = vec![deact_in_id, react_out_id, pulse_out_id, bring_up_timer_id];
    if let Some(id) = death_on_id {
        zone_in_targets.push(id);
    }
    zone_in_mcu.set_targets(zone_in_targets);

    deact_in.set_targets(vec![zone_in_id]);
    react_out.set_targets(vec![zone_out_id]);
    if let (Some(on), Some(id)) = (death_on.as_mut(), death_count_id) {
        on.set_targets(vec![id]);
    }
    if let (Some(off), Some(id)) = (death_off.as_mut(), death_count_id) {
        off.set_targets(vec![id]);
    }
    if let (Some(md), Some(id)) = (reset_modifier.as_mut(), death_count_id) {
        md.set_targets(vec![id]);
    }
    if let (Some(tm), Some(id)) = (reset_counter.as_mut(), reset_modifier_id) {
        tm.set_targets(vec![id]);
    }

    let mut bring_up_targets = vec![after_bring_up_id];
    if bring_up == BringUp::Spawn {
        if let Some(id) = spawn_count_id {
            bring_up_targets.insert(0, id);
        }
    } else if let Some(id) = activate_id {
        bring_up_targets.insert(0, id);
    }
    bring_up_timer.set_targets(bring_up_targets);
    if let (Some(count), Some(spawner_id)) = (spawn_count.as_mut(), spawn_id) {
        count.set_targets(vec![spawner_id]);
    }

    let mut after_targets = Vec::new();
    let mut any_order_steps = false;
    for si in &lead_indexes {
        if let Some(first) = emitted.get(*si).and_then(|d| d.first()) {
            any_order_steps = true;
            let spawn_starts_chain = first.report == Some(0) && spawn_id.is_some();
            if !spawn_starts_chain {
                after_targets.push(first.delay.index.unwrap());
            }
        }
    }
    if after_targets.is_empty() && !any_order_steps {
        if let Some(wp0) = waypoints.first() {
            after_targets.push(wp0.index.unwrap());
        }
    }
    after_bring_up.set_targets(after_targets);

    let triggered: Vec<HashSet<usize>> = (0..emitted.len())
        .map(|si| event_triggered_sources(&opts.seats, si))
        .collect();
    for si in 0..emitted.len() {
        for oi in 0..emitted[si].len() {
            let mut targets = Vec::new();
            let is_report = emitted[si][oi].report.is_some();
            if !is_report {
                if let Some(cmd) = &emitted[si][oi].cmd {
                    targets.push(cmd.index.unwrap());
                } else if let Some(wp_n) = emitted[si][oi].goto_wp {
                    let idx = wp_n.max(1) as usize - 1;
                    if let Some(wp) = waypoints.get(idx) {
                        targets.push(wp.index.unwrap());
                    }
                }
            }
            if emitted[si][oi].mission_complete {
                targets.push(mission_end_id);
            } else if let Some(next_i) = chain_delay_to(&emitted[si], oi) {
                let src = emitted[si][next_i].source_index;
                if !triggered[si].contains(&src) {
                    targets.push(emitted[si][next_i].delay.index.unwrap());
                }
            }
            emitted[si][oi].delay.set_targets(targets);
        }
    }

    wire_waypoint_chain(&mut waypoints, &emitted, &triggered);

    if repeat {
        // Zone Out always cleans up and resets DeathCount. A full wipe only
        // starts COOLDOWN, which pulses the spawner so a new wave can appear
        // while the player is still inside.
        let mut out_targets = vec![deact_out_id, mission_end_id, react_in_id, pulse_id];
        if let Some(id) = death_off_id {
            out_targets.insert(0, id);
        }
        if let Some(id) = reset_counter_id {
            out_targets.push(id);
        }
        zone_out_mcu.set_targets(out_targets);
        if let Some(id) = spawn_id {
            cooldown.set_targets(vec![id]);
        }
        if let Some(dc) = death_count.as_mut() {
            dc.set_targets(vec![cooldown_id]);
        }
    } else {
        zone_out_mcu.set_targets(vec![deact_out_id, cooldown_id, mission_end_id]);
        cooldown.set_targets(vec![react_in_id, pulse_id]);
    }
    deact_out.set_targets(vec![zone_out_id]);
    react_in.set_targets(vec![zone_in_id]);
    mission_end.set_targets(vec![mission_end_orders_id, delayed_end_id]);
    let mut end_order_targets = vec![force_id];
    if let Some(id) = rtb_wait_id {
        end_order_targets.push(id);
    }
    mission_end_orders.set_targets(end_order_targets);
    if let Some(wait) = rtb_wait.as_mut() {
        wait.set_targets(rtb_waypoints.iter().filter_map(|w| w.index).collect());
    }
    delayed_end.set_targets(vec![deactivate_id, delete_wait_id]);
    delete_wait.set_targets(vec![delete_id]);

    for (si, steps) in emitted.iter().enumerate() {
        if !receives_orders(&opts.seats, si) {
            continue;
        }
        for (oi, step) in steps.iter().enumerate() {
            let Some(rt) = step.report else {
                continue;
            };
            let tar = step.delay.index.unwrap();
            let cmd_id = match rt {
                0 => spawn_id,
                1 => previous_cmd_id(steps, oi, "MCU_CMD_AttackTarget"),
                2 => previous_cmd_id(steps, oi, "MCU_CMD_AttackArea"),
                3 => previous_cmd_id(steps, oi, "MCU_CMD_TakeOff"),
                4 => previous_cmd_id(steps, oi, "MCU_CMD_Land"),
                _ => None,
            };
            let Some(cmd_id) = cmd_id else {
                continue;
            };
            attach_report(&mut placed[si].entity, rt, cmd_id, tar);
        }
    }

    for (si, seat) in opts.seats.iter().enumerate() {
        let chain_si = if receives_orders(&opts.seats, si) {
            si
        } else {
            flight_lead_of(&opts.seats, si)
        };
        for hook in &seat.events {
            let tar = match hook.then {
                EventThen::ForceComplete => Some(force_id),
                EventThen::Order(oi) => emitted
                    .get(chain_si)
                    .and_then(|steps| {
                        steps
                            .iter()
                            .find(|s| s.source_index == oi)
                            .and_then(|s| s.delay.index)
                    }),
            };
            let Some(tar) = tar else {
                continue;
            };
            attach_event(&mut placed[si].entity, hook.kind.type_id(), tar);
        }
    }

    if let Some(id) = death_count_id {
        for (i, unit) in placed.iter_mut().enumerate() {
            let kind = opts.seats.get(i).map(|s| s.unit.kind).unwrap_or(UnitKind::Plane);
            attach_event(&mut unit.entity, EntityEvent::default_for(kind).type_id(), id);
        }
    }

    logic.children.extend([
        begin,
        pulse,
        pulse_out,
        zone_in_mcu,
        zone_out_mcu,
        deact_in,
        react_out,
        deact_out,
        react_in,
        cooldown,
        force,
        deactivate_units,
        delete,
        mission_end,
        mission_end_orders,
        delayed_end,
        delete_wait,
        bring_up_timer,
        after_bring_up,
    ]);
    if let Some(wait) = rtb_wait {
        logic.children.push(wait);
    }
    if let Some(a) = activate {
        logic.children.push(a);
    }
    if let Some(c) = spawn_count {
        logic.children.push(c);
    }
    if let Some(s) = spawn {
        logic.children.push(s);
    }
    for extra in [
        death_count,
        death_on,
        death_off,
        reset_modifier,
        reset_counter,
    ] {
        if let Some(e) = extra {
            logic.children.push(e);
        }
    }
    for steps in &emitted {
        logic.children.extend(steps.iter().map(|s| s.delay.clone()));
    }

    for unit in placed {
        units_g.children.push(unit.object);
        units_g.children.push(unit.entity);
    }
    for steps in emitted {
        orders_g
            .children
            .extend(steps.into_iter().filter_map(|s| s.cmd));
    }
    wps_g.children.extend(rtb_waypoints);
    wps_g.children.extend(waypoints);

    root.children.push(logic);
    root.children.push(units_g);
    if !orders_g.children.is_empty() {
        root.children.push(orders_g);
    }
    root.children.push(wps_g);
    Ok(root)
}

struct PlacedUnit {
    object_id: i32,
    entity_id: i32,
    object: Il2Entity,
    entity: Il2Entity,
}

struct EmittedOrder {
    delay: Il2Entity,
    cmd: Option<Il2Entity>,
    goto_wp: Option<u32>,
    report: Option<i32>,
    source_index: usize,
    time_on_target: bool,
    mission_complete: bool,
    pause: bool,
}

fn is_attack_emitted(step: &EmittedOrder) -> bool {
    step.cmd.as_ref().is_some_and(|c| {
        c.block_type == "MCU_CMD_AttackTarget" || c.block_type == "MCU_CMD_AttackArea"
    })
}

/// Time on Target is pulsed from the waypoint, not the previous delay — even
/// when it is listed before Attack / AttackArea.
fn tot_owned_by_wp(steps: &[EmittedOrder], tot_oi: usize) -> bool {
    for s in steps[..tot_oi].iter().rev() {
        if s.report.is_some() || s.time_on_target || s.mission_complete {
            continue;
        }
        if is_attack_emitted(s) {
            continue;
        }
        return s.goto_wp.is_some();
    }
    false
}

fn hop_end(steps: &[EmittedOrder], after: usize) -> usize {
    steps[after..]
        .iter()
        .position(|s| s.goto_wp.is_some())
        .map(|p| after + p)
        .unwrap_or(steps.len())
}

fn hop_has_tot(steps: &[EmittedOrder], goto_oi: usize) -> bool {
    steps[goto_oi + 1..hop_end(steps, goto_oi + 1)]
        .iter()
        .any(|s| s.time_on_target)
}

fn hop_has_attack(steps: &[EmittedOrder], goto_oi: usize) -> bool {
    steps[goto_oi + 1..hop_end(steps, goto_oi + 1)]
        .iter()
        .any(is_attack_emitted)
}

fn prev_goto(steps: &[EmittedOrder], oi: usize) -> Option<usize> {
    steps[..oi].iter().rposition(|s| s.goto_wp.is_some())
}

fn tot_in_same_hop(steps: &[EmittedOrder], oi: usize) -> bool {
    prev_goto(steps, oi).is_some_and(|g| hop_has_tot(steps, g))
}

/// After TOT expires, skip sibling attacks and jump to the next real order.
fn first_after_wp_cluster(steps: &[EmittedOrder], tot_oi: usize) -> Option<usize> {
    for (j, s) in steps[tot_oi + 1..].iter().enumerate() {
        if s.time_on_target || is_attack_emitted(s) || s.report.is_some() {
            continue;
        }
        return Some(tot_oi + 1 + j);
    }
    None
}

fn pulse_from_wp(extra: &mut [Vec<i32>], wp_idx: usize, delay: &Il2Entity) {
    if let Some(id) = delay.index {
        if !extra[wp_idx].contains(&id) {
            extra[wp_idx].push(id);
        }
    }
}

/// Goto WP delay only pulses the WP MCU. On arrival the WP owns Attack, TOT,
/// the next Goto (when there is no attack/TOT), and Mission Complete when the
/// hop has neither an attack nor TOT.
fn wp_owns_next(steps: &[EmittedOrder], goto_oi: usize, next_oi: usize) -> bool {
    let hop = goto_oi + 1..hop_end(steps, goto_oi + 1);
    if next_oi == hop.end && next_oi < steps.len() && steps[next_oi].goto_wp.is_some() {
        return !hop_has_attack(steps, goto_oi) && !hop_has_tot(steps, goto_oi);
    }
    if !hop.contains(&next_oi) {
        return false;
    }
    let s = &steps[next_oi];
    if is_attack_emitted(s) || s.time_on_target || s.pause {
        return true;
    }
    if s.mission_complete {
        return !hop_has_attack(steps, goto_oi) && !hop_has_tot(steps, goto_oi);
    }
    false
}

fn chain_delay_to(steps: &[EmittedOrder], oi: usize) -> Option<usize> {
    let cur = &steps[oi];
    if cur.mission_complete {
        return None;
    }
    if cur.goto_wp.is_some() {
        let next = oi + 1;
        if next >= steps.len() || wp_owns_next(steps, oi, next) {
            return None;
        }
        if steps[next].report.is_some() && cur.report.is_none() {
            return None;
        }
        return Some(next);
    }
    if cur.time_on_target && tot_owned_by_wp(steps, oi) {
        return first_after_wp_cluster(steps, oi);
    }
    let next = oi + 1;
    if next >= steps.len() {
        return None;
    }
    if steps[next].report.is_some() && cur.report.is_none() {
        return None;
    }
    if steps[next].time_on_target && tot_owned_by_wp(steps, next) {
        return None;
    }
    if is_attack_emitted(cur) && tot_in_same_hop(steps, oi) {
        let n = &steps[next];
        if n.time_on_target || is_attack_emitted(n) || n.mission_complete || n.goto_wp.is_some() {
            return None;
        }
    }
    Some(next)
}

/// On arrival WP n pulses Attack / AttackArea and Time on Target together
/// (order in the list does not matter). TOT expiry continues the chain.
/// Mission Complete after a bare Goto WP is also pulsed from the waypoint.
fn wire_waypoint_chain(
    waypoints: &mut [Il2Entity],
    emitted: &[Vec<EmittedOrder>],
    triggered: &[HashSet<usize>],
) {
    let mut extra: Vec<Vec<i32>> = vec![Vec::new(); waypoints.len()];
    for (si, steps) in emitted.iter().enumerate() {
        let skip = triggered.get(si);
        for (oi, step) in steps.iter().enumerate() {
            let Some(wp_n) = step.goto_wp else {
                continue;
            };
            let idx = wp_n.max(1) as usize - 1;
            if idx >= extra.len() {
                continue;
            }
            let mut saw_attack = false;
            let mut saw_tot = false;
            for later in &steps[oi + 1..] {
                if later.goto_wp.is_some() {
                    if !saw_attack && !saw_tot {
                        pulse_from_wp(&mut extra, idx, &later.delay);
                    }
                    break;
                }
                if later.mission_complete {
                    let event_owns = skip.is_some_and(|s| s.contains(&later.source_index));
                    if !saw_attack && !saw_tot && !event_owns {
                        pulse_from_wp(&mut extra, idx, &later.delay);
                    }
                    break;
                }
                if later.report.is_some() {
                    break;
                }
                if is_attack_emitted(later) {
                    pulse_from_wp(&mut extra, idx, &later.delay);
                    saw_attack = true;
                    continue;
                }
                if later.time_on_target {
                    pulse_from_wp(&mut extra, idx, &later.delay);
                    saw_tot = true;
                    continue;
                }
                if later.pause {
                    pulse_from_wp(&mut extra, idx, &later.delay);
                    break;
                }
                break;
            }
        }
    }
    for i in 0..waypoints.len() {
        waypoints[i].set_targets(extra[i].clone());
    }
}

fn previous_cmd_id(steps: &[EmittedOrder], before: usize, block: &str) -> Option<i32> {
    steps[..before]
        .iter()
        .rev()
        .find_map(|s| {
            s.cmd
                .as_ref()
                .filter(|c| c.block_type == block)
                .and_then(|c| c.index)
        })
}

fn command_object_ids(order: &OrderSpec, owner: usize, placed: &[PlacedUnit]) -> Vec<i32> {
    let mut ids = Vec::new();
    if owner < placed.len() {
        ids.push(placed[owner].entity_id);
    }
    for &si in &order.shared_with {
        if si == owner || si >= placed.len() {
            continue;
        }
        let eid = placed[si].entity_id;
        if !ids.contains(&eid) {
            ids.push(eid);
        }
    }
    ids
}

fn place_unit(
    seat: &TemplateSeat,
    index: usize,
    x: f64,
    y: f64,
    z: f64,
    per_group: usize,
    next_id: &mut i32,
) -> Result<PlacedUnit, String> {
    let spec = &seat.unit;
    let (mut object, _) = duplicate_template(&spec.object, next_id);
    let (mut entity, _) = duplicate_template(&spec.entity, next_id);
    let object_id = object.index.ok_or("unit prototype has no Index")?;
    let entity_id = entity.index.ok_or("entity prototype has no Index")?;

    object.set_property("LinkTrId", entity_id.to_string());
    object.set_property("XPos", format!("{x:.3}"));
    object.set_property("YPos", format!("{y:.3}"));
    object.set_property("ZPos", format!("{z:.3}"));
    object.set_property("Country", seat.country.to_string());
    object.set_property("AILevel", seat.skill.clamp(0, 4).to_string());
    object.set_existing_property(
        "NumberInFormation",
        seat.number_in_formation.max(0).to_string(),
    );
    object.set_existing_property("Fuel", format!("{}", seat.fuel.clamp(0.0, 1.0)));
    object.set_existing_property("PayloadId", seat.payload_id.to_string());
    object.set_existing_property("ModMask", seat.mod_mask.clone());
    object.set_existing_property("Vulnerable", i32::from(seat.vulnerable).to_string());
    object.set_existing_property("Engageable", i32::from(seat.engageable).to_string());
    object.set_existing_property("LimitAmmo", i32::from(seat.limit_ammo).to_string());
    // Planes only. Writing these on a Vehicle makes the mission editor stop
    // parsing the rest of the group (one unit, no entity, no followers).
    object.set_existing_property("AiRTBDecision", i32::from(seat.ai_rtb).to_string());
    object.set_existing_property(
        "StartType",
        PlaneStart::written_for_altitude(seat.start_type, seat.altitude).to_string(),
    );
    if spec.is_train() {
        set_train_carriages(&mut object, &seat.carriages);
    }
    let per = per_group.max(1);
    let flight = index / per;
    let in_flight = (seat.number_in_formation.max(0) as usize) % per;
    if spec.is_air() {
        let ac_id = script_type_id(&spec.script);
        apply_plane_marks(
            &mut object,
            seat.country,
            ac_id,
            flight,
            in_flight,
            seat.skill.clamp(0, 4),
        );
        object.set_existing_property(
            "NumberInFormation",
            seat.number_in_formation.max(0).to_string(),
        );
    } else if object.name().is_none()
        || object.name() == Some("")
        || object.name() == Some(spec.kind.object_type())
    {
        object.set_name(&format!("{} {}", spec.label(), index + 1));
    }

    entity.set_property("MisObjID", object_id.to_string());
    entity.set_property("XPos", format!("{x:.3}"));
    entity.set_property("YPos", format!("{:.3}", y + 0.2));
    entity.set_property("ZPos", format!("{z:.3}"));
    entity.set_property("Enabled", "0");
    entity.set_name(&format!("{} entity", spec.kind.object_type()));
    entity
        .children
        .retain(|c| c.block_type != "OnEvents" && c.block_type != "OnReports");
    entity.set_targets(Vec::new());
    entity.set_objects(Vec::new());

    Ok(PlacedUnit {
        object_id,
        entity_id,
        object,
        entity,
    })
}

fn apply_plane_marks(
    plane: &mut Il2Entity,
    country: i32,
    type_id: &str,
    flight: usize,
    seat: usize,
    skill: i32,
) {
    let number = flight_number(flight, seat);
    let color = flight_color(flight);
    plane.set_name(&plane_display_name(flight, seat));
    plane.set_property("Callsign", callsign_for(country, type_id).to_string());
    plane.set_property("Callnum", "0");
    plane.set_property("AILevel", skill.to_string());
    plane.set_property("TCode", format!("\"{}\"", encode_tcode(number)));
    plane.set_property(
        "TCodeColor",
        format!("\"{}\"", encode_tcode_color(color, number)),
    );
    plane.set_property("NumberInFormation", seat.to_string());
}

fn script_type_id(script: &str) -> &str {
    script
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(script)
        .trim_end_matches(".txt")
        .trim_matches('"')
}

fn seats_have_planes(seats: &[TemplateSeat]) -> bool {
    seats.iter().any(|s| s.unit.is_air())
}

fn first_plane_altitude(seats: &[TemplateSeat]) -> f64 {
    seats
        .iter()
        .find(|s| s.unit.is_air())
        .map(|s| s.altitude as f64)
        .unwrap_or(0.0)
}

/// Path YPos shown in the UI: explicit template altitude, else the first plane
/// (aircraft) or 0 m (ground-only).
pub fn path_waypoint_display_m(seats: &[TemplateSeat], template_alt: f32) -> f32 {
    if template_alt > 0.0 {
        template_alt
    } else if seats_have_planes(seats) {
        first_plane_altitude(seats) as f32
    } else {
        template_alt
    }
}

fn path_waypoint_altitude(opts: &TemplateOptions) -> f64 {
    path_waypoint_display_m(&opts.seats, opts.waypoint_altitude) as f64
}

fn hop_waypoint_altitude(seats: &[TemplateSeat], wp_n: u32) -> Option<f32> {
    let n = wp_n.max(1);
    for seat in seats {
        if !seat.unit.is_air() {
            continue;
        }
        for order in &seat.orders {
            if order.kind == OrderKind::GotoWaypoint
                && order.waypoint.max(1) == n
                && order.altitude > 0.0
            {
                return Some(order.altitude);
            }
        }
    }
    None
}

fn hop_waypoint_priority(seats: &[TemplateSeat], wp_n: u32, fallback: i32) -> i32 {
    let n = wp_n.max(1);
    for seat in seats {
        for order in &seat.orders {
            if order.kind == OrderKind::GotoWaypoint && order.waypoint.max(1) == n {
                return order.priority.clamp(0, 2);
            }
        }
    }
    fallback.clamp(0, 2)
}

/// Priority for WP `wp_n`: first Goto WP hop, else `fallback`.
pub fn waypoint_display_priority(seats: &[TemplateSeat], wp_n: u32, fallback: i32) -> i32 {
    hop_waypoint_priority(seats, wp_n, fallback)
}

/// YPos for WP `wp_n`: hop override, else the template path altitude, else the
/// first plane’s spawn height (0 m when the group is ground-only).
pub fn waypoint_display_altitude(seats: &[TemplateSeat], wp_n: u32, template_alt: f32) -> f32 {
    if let Some(a) = hop_waypoint_altitude(seats, wp_n) {
        a
    } else {
        path_waypoint_display_m(seats, template_alt)
    }
}

fn wire_other_unit(
    cmd: &mut Il2Entity,
    placed: &[PlacedUnit],
    seats: &[TemplateSeat],
    from: usize,
    other_seat: Option<usize>,
) {
    let Some(target) = other_seat else {
        return;
    };
    if target == from || target >= placed.len() {
        return;
    }
    let other = if is_follower(seats, target) {
        flight_lead_of(seats, target)
    } else {
        target
    };
    if other == from {
        return;
    }
    cmd.set_targets(vec![placed[other].entity_id]);
}

fn build_order(
    order: &OrderSpec,
    block: &str,
    kind: UnitKind,
    next_id: &mut i32,
    x: f64,
    z: f64,
) -> Il2Entity {
    let mut cmd = mcu(block, order.kind.label(), next_id, x, z);
    match order.kind {
        OrderKind::Attack => {
            cmd.set_property("AttackGroup", i32::from(order.attack_group).to_string());
            cmd.set_property("Priority", order.priority.to_string());
        }
        OrderKind::AttackArea => {
            cmd.set_property("AttackGround", i32::from(order.attack_ground).to_string());
            cmd.set_property("AttackAir", i32::from(order.attack_air).to_string());
            cmd.set_property(
                "AttackGTargets",
                i32::from(order.attack_g_targets).to_string(),
            );
            cmd.set_property("AttackArea", format!("{:.0}", order.attack_area));
            cmd.set_property("Time", format!("{:.0}", order.time_s));
            cmd.set_property("Priority", order.priority.to_string());
        }
        OrderKind::Behaviour => {
            cmd.set_property("Filter", order.behaviour_filter.to_string());
            cmd.set_property("Vulnerable", "1");
            cmd.set_property("Engageable", "1");
            cmd.set_property("LimitAmmo", "1");
            cmd.set_property("AILevel", "3");
            cmd.set_property("Country", "0");
            cmd.set_property("FloatParam", "0");
        }
        OrderKind::Cover => {
            cmd.set_property("CoverGroup", "0");
            cmd.set_property("Priority", order.priority.to_string());
        }
        OrderKind::Effect => {
            cmd.set_property("ActionType", i32::from(!order.effect_start).to_string());
        }
        OrderKind::Flare => {
            cmd.set_property("Color", order.flare_color.to_string());
        }
        OrderKind::ForceComplete => {
            cmd.set_property("Priority", order.priority.to_string());
            cmd.set_property("EmergencyOrdnanceDrop", "0");
        }
        OrderKind::Formation => {
            cmd.set_property("FormationType", order.formation_type.to_string());
            cmd.set_property(
                "FormationDensity",
                formation_density(order.formation_type, kind).to_string(),
            );
            cmd.set_property("FlightSize", "1");
            cmd.set_property("WaitForWingmen", "1");
        }
        OrderKind::GotoWaypoint
        | OrderKind::TimeOnTarget
        | OrderKind::Timer
        | OrderKind::MissionComplete
        | OrderKind::RtbOnZoneOut => {}
        OrderKind::Land => {
            cmd.set_property("Priority", order.priority.to_string());
        }
        OrderKind::TakeOff => {
            cmd.set_property("NoTaxiTakeoff", "0");
        }
        OrderKind::OnSpawned
        | OrderKind::OnTargetAttacked
        | OrderKind::OnAreaAttacked
        | OrderKind::OnTookOff
        | OrderKind::OnLanded => {}
    }
    cmd
}

fn checkzone(
    name: &str,
    radius: f64,
    closer: bool,
    coalitions: &str,
    next_id: &mut i32,
    x: f64,
    z: f64,
) -> Il2Entity {
    let mut e = mcu("MCU_CheckZone", name, next_id, x, z);
    e.set_property("Zone", format!("{:.0}", radius));
    e.set_property("Cylinder", "1");
    e.set_property("Closer", i32::from(closer).to_string());
    e.set_property("PlaneCoalitions", coalitions.to_string());
    e
}

fn timer(name: &str, time: f64, next_id: &mut i32, x: f64, z: f64) -> Il2Entity {
    let mut e = mcu("MCU_Timer", name, next_id, x, z);
    e.set_property("Time", format_time(time));
    e.set_property("Random", "100");
    e
}

fn counter(name: &str, count: i32, drop: i32, next_id: &mut i32, x: f64, z: f64) -> Il2Entity {
    let mut e = mcu("MCU_Counter", name, next_id, x, z);
    e.set_property("Counter", count.to_string());
    e.set_property("Dropcount", drop.to_string());
    e
}

fn modifier_set_val(name: &str, next_id: &mut i32, x: f64, z: f64) -> Il2Entity {
    let mut e = mcu("MCU_ModifierSetVal", name, next_id, x, z);
    e.set_property("ParamIndex", "0");
    e.set_property("Data0", "0");
    e.set_property("Data1", "0");
    e.set_property("Data2", "0");
    e.set_property("Data3", "0");
    e
}

fn format_time(t: f64) -> String {
    if (t.fract()).abs() < 1e-6 {
        format!("{:.0}", t)
    } else {
        format!("{t:.2}")
    }
}

fn mcu(block: &str, name: &str, next_id: &mut i32, x: f64, z: f64) -> Il2Entity {
    let mut e = Il2Entity::new(block);
    let id = *next_id;
    *next_id += 1;
    e.index = Some(id);
    e.set_property("Index", id.to_string());
    e.set_name(name);
    e.set_property("Desc", "\"\"");
    e.set_targets(Vec::new());
    e.set_objects(Vec::new());
    e.set_property("XPos", format!("{x:.3}"));
    e.set_property("YPos", "0.000");
    e.set_property("ZPos", format!("{z:.3}"));
    e.set_property("XOri", "0");
    e.set_property("YOri", "0");
    e.set_property("ZOri", "0");
    e
}

fn named_group(name: &str, next_id: &mut i32) -> Il2Entity {
    let mut g = Il2Entity::new("Group");
    let id = *next_id;
    *next_id += 1;
    g.index = Some(id);
    g.set_property("Index", id.to_string());
    g.set_name(name);
    g.set_property("Desc", "\"\"");
    g
}

fn attach_event(entity: &mut Il2Entity, event_type: i32, tar_id: i32) {
    let mut ev = Il2Entity::new("OnEvent");
    ev.set_property("Type", event_type.to_string());
    ev.set_property("TarId", tar_id.to_string());
    let mut wrap = entity
        .children
        .iter()
        .find(|c| c.block_type == "OnEvents")
        .cloned()
        .unwrap_or_else(|| Il2Entity::new("OnEvents"));
    wrap.children.push(ev);
    entity.children.retain(|c| c.block_type != "OnEvents");
    entity.children.push(wrap);
}

fn attach_report(entity: &mut Il2Entity, report_type: i32, cmd_id: i32, tar_id: i32) {
    let mut r = Il2Entity::new("OnReport");
    r.set_property("Type", report_type.to_string());
    r.set_property("CmdId", cmd_id.to_string());
    r.set_property("TarId", tar_id.to_string());
    let mut wrap = entity
        .children
        .iter()
        .find(|c| c.block_type == "OnReports")
        .cloned()
        .unwrap_or_else(|| Il2Entity::new("OnReports"));
    wrap.children.push(r);
    entity.children.retain(|c| c.block_type != "OnReports");
    entity.children.push(wrap);
}

fn synthetic_plane_unit(ac: &AircraftType) -> CatalogUnit {
    let mut plane = Il2Entity::new("Plane");
    plane.index = Some(1);
    plane.set_property("Index", "1");
    plane.set_property("LinkTrId", "2");
    plane.set_name(ac.label);
    plane.set_property("XPos", "0.000");
    plane.set_property("YPos", "1000.000");
    plane.set_property("ZPos", "0.000");
    plane.set_property("XOri", "0");
    plane.set_property("YOri", "0");
    plane.set_property("ZOri", "0");
    plane.set_property("Script", format!("\"{}\"", ac.script));
    plane.set_property("Model", format!("\"{}\"", ac.model));
    plane.set_property("Country", "501");
    plane.set_property("Desc", "\"\"");
    plane.set_property("Skin", "\"\"");
    plane.set_property("BotSkin", "\"\"");
    plane.set_property("AILevel", "2");
    plane.set_property("CoopStart", "0");
    plane.set_property("NumberInFormation", "0");
    plane.set_property("Vulnerable", "1");
    plane.set_property("Engageable", "1");
    plane.set_property("LimitAmmo", "1");
    plane.set_property("StartType", "0");
    plane.set_property("Callsign", "0");
    plane.set_property("Callnum", "0");
    plane.set_property("DamageReport", "50");
    plane.set_property("DamageThreshold", "1");
    plane.set_property("PayloadId", "0");
    plane.set_property("ModMask", "1");
    plane.set_property("AiRTBDecision", "1");
    plane.set_property("DeleteAfterDeath", "1");
    plane.set_property("DeleteAfterLand", "1");
    plane.set_property("Spotter", "-1");
    plane.set_property("Fuel", "1");
    plane.set_property("TCode", "\"%20%20%20%20\"");
    plane.set_property("TCodeColor", "\"1111\"");
    plane.set_property("GunLoad", "[]");
    plane.set_property("GunBelt", "[]");
    plane.set_property("VictoryCount", "0");
    plane.set_property("Emblem", "0");

    let mut entity = synthetic_entity();
    entity.index = Some(2);
    entity.set_property("Index", "2");
    entity.set_property("MisObjID", "1");

    CatalogUnit {
        kind: UnitKind::Plane,
        name: ac.label.to_string(),
        script: ac.script.to_string(),
        display: ac.label.to_string(),
        object: plane,
        entity,
    }
}

fn synthetic_entity() -> Il2Entity {
    let mut e = Il2Entity::new("MCU_TR_Entity");
    e.index = Some(2);
    e.set_property("Index", "2");
    e.set_name("entity");
    e.set_property("Desc", "\"\"");
    e.set_targets(Vec::new());
    e.set_objects(Vec::new());
    e.set_property("XPos", "0.000");
    e.set_property("YPos", "0.200");
    e.set_property("ZPos", "0.000");
    e.set_property("XOri", "0");
    e.set_property("YOri", "0");
    e.set_property("ZOri", "0");
    e.set_property("Enabled", "0");
    e.set_property("MisObjID", "1");
    e
}

/// Result of reading a `.Group` back into Template Builder.
#[derive(Clone, Debug)]
pub struct TemplateLoad {
    pub options: TemplateOptions,
    /// True when the file already has Template Builder `Logic` / `Units` names.
    pub native_format: bool,
    /// Dropped MCUs, renamed zones, layout corrections, unmapped events, …
    pub warnings: Vec<String>,
}

/// Template Builder files keep `ENABLE / PULSE IN`, `Zone IN`, `MISSION END`,
/// and a `Units` subgroup. Other groups are rebuilt from units + orders.
pub fn looks_like_generated_template(root: &Il2Entity) -> bool {
    let has_units = root
        .children
        .iter()
        .any(|c| c.block_type == "Group" && c.name() == Some("Units"));
    has_units
        && root.find_by_name("ENABLE / PULSE IN").is_some()
        && root.find_by_name("Zone IN").is_some()
        && root.find_by_name("MISSION END").is_some()
}

/// Fill `TemplateOptions` from a loaded group so the builder can edit it.
///
/// Native files round-trip seats, orders, zones, and bring-up. Any other
/// layout is reconstructed from world objects and command MCUs; proximity
/// logic is rebuilt on the next Generate. `warnings` lists everything that
/// could not be kept or that was corrected.
pub fn load_template(root: &Il2Entity, catalog: &[CatalogUnit]) -> Result<TemplateLoad, String> {
    let native_format = looks_like_generated_template(root);
    let by_index = collect_by_index(root);
    let loaded = collect_loaded_units(root);
    if loaded.is_empty() {
        return Err("that group has no Plane / Vehicle / Train / Ship units to edit.".into());
    }

    let mut warnings = Vec::new();
    let mut consumed: HashSet<i32> = HashSet::new();
    if !native_format {
        warnings.push(
            "This group is not in Template Builder format. Units and orders were kept; Logic, checkzones, and formation layout will be rebuilt on Generate.".into(),
        );
    }

    let mut seats = Vec::new();
    let mut entity_to_seat: HashMap<i32, usize> = HashMap::new();
    for unit in &loaded {
        if let Some(id) = unit.object.index {
            consumed.insert(id);
        }
        if let Some(id) = unit.entity.index {
            consumed.insert(id);
            entity_to_seat.insert(id, seats.len());
        }
        seats.push(seat_from_loaded(unit, catalog));
    }
    apply_loaded_roles(&mut seats, &loaded, &entity_to_seat);

    let mut opts = TemplateOptions::default();
    opts.name = root
        .name()
        .filter(|n| !n.is_empty())
        .unwrap_or("Unit Template")
        .to_string();
    opts.seats = seats;

    read_bring_up(root, &mut opts, &mut consumed, &mut warnings);
    read_zones(root, &mut opts, native_format, &mut consumed, &mut warnings);

    let mut timer_to_order: HashMap<i32, (usize, usize)> = HashMap::new();
    if native_format {
        read_native_orders(
            root,
            &by_index,
            &entity_to_seat,
            &mut opts,
            &mut consumed,
            &mut timer_to_order,
            &mut warnings,
        );
    } else {
        read_foreign_orders(
            root,
            &by_index,
            &entity_to_seat,
            &mut opts,
            &mut consumed,
            &mut warnings,
        );
    }
    read_rtb_orders(root, &entity_to_seat, &mut opts, &mut consumed);
    read_events(
        &loaded,
        &by_index,
        &entity_to_seat,
        &timer_to_order,
        &mut opts,
        &mut consumed,
        &mut warnings,
    );
    for seat in &mut opts.seats {
        if !seat.orders.is_empty() {
            normalize_order_chain(&mut seat.orders, &mut seat.events, 0);
        }
    }

    read_waypoints_into_opts(root, &mut opts, &mut consumed);
    infer_layout(&loaded, &mut opts, &mut warnings);

    collect_dropped(root, &consumed, native_format, &mut warnings);

    Ok(TemplateLoad {
        options: opts,
        native_format,
        warnings,
    })
}

struct LoadedUnit {
    object: Il2Entity,
    entity: Il2Entity,
}

fn visit<'a>(e: &'a Il2Entity, f: &mut impl FnMut(&'a Il2Entity)) {
    f(e);
    for c in &e.children {
        visit(c, f);
    }
}

fn collect_by_index(root: &Il2Entity) -> HashMap<i32, &Il2Entity> {
    let mut map = HashMap::new();
    visit(root, &mut |e| {
        if let Some(id) = e.index {
            map.entry(id).or_insert(e);
        }
    });
    map
}

fn collect_loaded_units(root: &Il2Entity) -> Vec<LoadedUnit> {
    let mut objects = Vec::new();
    if let Some(units) = root
        .children
        .iter()
        .find(|c| c.block_type == "Group" && c.name() == Some("Units"))
    {
        collect_unit_objects(units, &mut objects);
    }
    if objects.is_empty() {
        collect_unit_objects(root, &mut objects);
    }
    objects
        .into_iter()
        .map(|object| {
            let entity = object
                .property("LinkTrId")
                .and_then(|s| s.parse::<i32>().ok())
                .and_then(|id| find_index(root, id))
                .cloned()
                .unwrap_or_else(synthetic_entity);
            LoadedUnit {
                object: object.clone(),
                entity,
            }
        })
        .collect()
}

fn collect_unit_objects<'a>(e: &'a Il2Entity, out: &mut Vec<&'a Il2Entity>) {
    if e.block_type == "Group" && e.name().is_some_and(|n| n.eq_ignore_ascii_case("NodeGates")) {
        return;
    }
    if matches!(
        e.block_type.as_str(),
        "Plane" | "Vehicle" | "Train" | "Ship"
    ) {
        out.push(e);
    }
    for c in &e.children {
        collect_unit_objects(c, out);
    }
}

fn kind_from_object(object: &Il2Entity) -> UnitKind {
    let script = object
        .property("Script")
        .map(|s| s.trim_matches('"'))
        .unwrap_or("");
    match object.block_type.as_str() {
        "Plane" => UnitKind::Plane,
        "Train" => UnitKind::Train,
        "Ship" => UnitKind::Ship,
        _ if weapon_range::is_infantry_script(script) => UnitKind::Infantry,
        _ if is_fixed_script(script) => UnitKind::Fixed,
        _ => UnitKind::Vehicle,
    }
}

fn catalog_unit_matching(catalog: &[CatalogUnit], object: &Il2Entity, entity: &Il2Entity) -> CatalogUnit {
    let script = object
        .property("Script")
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_default();
    if let Some(unit) = catalog
        .iter()
        .find(|u| u.script.eq_ignore_ascii_case(&script))
    {
        return unit.clone();
    }
    let name = object.name().unwrap_or("").to_string();
    let display = display_name(&name, &script);
    CatalogUnit {
        kind: kind_from_object(object),
        name,
        script,
        display,
        object: object.clone(),
        entity: entity.clone(),
    }
}

fn seat_from_loaded(unit: &LoadedUnit, catalog: &[CatalogUnit]) -> TemplateSeat {
    let spec = catalog_unit_matching(catalog, &unit.object, &unit.entity);
    let mut seat = TemplateSeat::new(spec);
    let obj = &unit.object;
    seat.country = prop_i32(obj, "Country", seat.country);
    seat.skill = prop_i32(obj, "AILevel", seat.skill).clamp(0, 4);
    seat.number_in_formation = prop_i32(obj, "NumberInFormation", seat.number_in_formation);
    seat.fuel = prop_f32(obj, "Fuel", seat.fuel).clamp(0.0, 1.0);
    seat.payload_id = prop_i32(obj, "PayloadId", seat.payload_id);
    if obj.property("ModMask").is_some() {
        seat.mod_mask = prop_mod_mask(obj);
    }
    seat.vulnerable = prop_bool(obj, "Vulnerable", seat.vulnerable);
    seat.engageable = prop_bool(obj, "Engageable", seat.engageable);
    seat.limit_ammo = prop_bool(obj, "LimitAmmo", seat.limit_ammo);
    seat.ai_rtb = prop_bool(obj, "AiRTBDecision", seat.ai_rtb);
    if seat.unit.is_air() {
        seat.altitude = prop_f32(obj, "YPos", seat.altitude).max(0.0);
        seat.start_type = PlaneStart::stored_for_altitude(
            prop_i32(obj, "StartType", seat.start_type),
            seat.altitude,
        );
    } else {
        seat.altitude = 0.0;
    }
    if seat.unit.is_train() {
        seat.carriages = train_carriages(obj);
    }
    seat
}

fn apply_loaded_roles(
    seats: &mut [TemplateSeat],
    loaded: &[LoadedUnit],
    entity_to_seat: &HashMap<i32, usize>,
) {
    for (i, unit) in loaded.iter().enumerate() {
        let Some(&lead_eid) = unit.entity.targets.first() else {
            continue;
        };
        let Some(&lead) = entity_to_seat.get(&lead_eid) else {
            continue;
        };
        if lead == i {
            continue;
        }
        seats[i].role = FlightRole::Follows(lead);
    }
    for i in 0..seats.len() {
        if seats
            .iter()
            .any(|s| s.role == FlightRole::Follows(i))
            && seats[i].role == FlightRole::Independent
        {
            seats[i].role = FlightRole::Lead;
        }
    }
}

fn read_bring_up(
    root: &Il2Entity,
    opts: &mut TemplateOptions,
    consumed: &mut HashSet<i32>,
    _warnings: &mut Vec<String>,
) {
    let mut spawn = None;
    let mut activate = None;
    let mut death = None;
    let mut cooldown = None;
    visit(root, &mut |e| {
        match (e.block_type.as_str(), e.name().unwrap_or("")) {
            ("MCU_Spawner", _) => spawn = Some(e),
            ("MCU_Activate", name)
                if name.eq_ignore_ascii_case("Activate Units")
                    || name.eq_ignore_ascii_case("Activate Unit(s)") =>
            {
                activate = Some(e);
            }
            ("MCU_Counter", "DeathCount") => death = Some(e),
            ("MCU_Timer", "COOLDOWN") => cooldown = Some(e),
            _ => {}
        }
    });
    if let Some(s) = spawn {
        opts.bring_up = BringUp::Spawn;
        mark_consumed(s, consumed);
        if let Some(c) = find_named(root, "SpawnCount") {
            mark_consumed(c, consumed);
        }
    } else if let Some(a) = activate {
        opts.bring_up = BringUp::Activate;
        mark_consumed(a, consumed);
    }
    if opts.bring_up == BringUp::Spawn && death.is_some() {
        opts.allow_multiple_spawns = true;
        if let Some(d) = death {
            mark_consumed(d, consumed);
        }
        for name in [
            "DeathCount ReActivate",
            "DeathCount Deactivate",
            "Reset Counter",
            "Modifier Set Value",
        ] {
            if let Some(e) = find_named(root, name) {
                mark_consumed(e, consumed);
            }
        }
        if let Some(c) = cooldown {
            mark_consumed(c, consumed);
            let secs = prop_f32(c, "Time", 0.0);
            if secs > 0.0 {
                opts.spawn_cooldown_min = (secs / 60.0).max(1.0);
            }
        }
    }
}

fn find_named<'a>(root: &'a Il2Entity, name: &str) -> Option<&'a Il2Entity> {
    root.find_by_name(name)
}

fn mark_consumed(e: &Il2Entity, consumed: &mut HashSet<i32>) {
    if let Some(id) = e.index {
        consumed.insert(id);
    }
}

fn read_zones(
    root: &Il2Entity,
    opts: &mut TemplateOptions,
    native: bool,
    consumed: &mut HashSet<i32>,
    warnings: &mut Vec<String>,
) {
    let zone_in = root
        .find_by_name("Zone IN")
        .or_else(|| find_checkzone_named(root, "Zone In"));
    let zone_out = root
        .find_by_name("Zone Out")
        .or_else(|| find_checkzone_named(root, "Zone Out"));
    if let Some(z) = zone_in {
        mark_consumed(z, consumed);
        if let Some(r) = z.property("Zone").and_then(|s| s.parse::<f32>().ok()) {
            opts.zone_in = r.max(200.0);
        }
        opts.zone_coalition = parse_zone_coalition(z.property("PlaneCoalitions").unwrap_or(""));
        if !native && z.name() != Some("Zone IN") {
            warnings.push(format!(
                "Checkzone \"{}\" was mapped to Zone In ({:.0} m).",
                z.name().unwrap_or("Zone In"),
                opts.zone_in
            ));
        }
    } else if let Some(mix) = zone_mix_for_seats(&opts.seats) {
        let (inn, out) = zone_defaults(mix);
        opts.zone_in = inn;
        opts.zone_out = out;
        warnings.push(format!(
            "No Zone In found — using {} defaults ({:.0} / {:.0} m).",
            match mix {
                ZoneMix::Air => "aircraft",
                ZoneMix::Ground => "ground",
                ZoneMix::Train => "train",
            },
            inn,
            out
        ));
    }
    if let Some(z) = zone_out {
        mark_consumed(z, consumed);
        if let Some(r) = z.property("Zone").and_then(|s| s.parse::<f32>().ok()) {
            opts.zone_out = r.max(opts.zone_in + 200.0);
        }
        if !native && z.name() != Some("Zone Out") {
            warnings.push(format!(
                "Checkzone \"{}\" was mapped to Zone Out ({:.0} m).",
                z.name().unwrap_or("Zone Out"),
                opts.zone_out
            ));
        }
    }
    if opts.zone_out < opts.zone_in + 200.0 {
        let old = opts.zone_out;
        opts.zone_out = opts.zone_in + 200.0;
        if old > 0.0 {
            warnings.push(format!(
                "Zone Out ({old:.0} m) was raised to {:.0} m so it stays larger than Zone In.",
                opts.zone_out
            ));
        }
    }
    for name in [
        "ENABLE / PULSE IN",
        "PULSE OUT",
        "Translator Mission Begin",
        "AFTER BRING UP",
        "MISSION BEGIN",
        "SPAWN UNITS",
        "MISSION END",
        "MISSION END ORDERS",
        "DELAYED END ORDERS",
        "DELETE DELAY",
        "Trigger Delete",
        "Deactivate Units",
        "Force Complete - High",
        "Zone Out ReActivate",
        "Zone In ReActivate",
        "RTB DELAY",
        "COOLDOWN",
    ] {
        if let Some(e) = root.find_by_name(name) {
            mark_consumed(e, consumed);
        }
    }
    consume_named_block(root, "MCU_Deactivate", "Self Deactivate", consumed);
    consume_named_block(root, "MCU_Deactivate", "Deactivate Unit(s)", consumed);
}

fn consume_named_block(root: &Il2Entity, block: &str, name: &str, consumed: &mut HashSet<i32>) {
    visit(root, &mut |e| {
        if e.block_type == block && e.name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            if let Some(id) = e.index {
                consumed.insert(id);
            }
        }
    });
}

fn find_checkzone_named<'a>(root: &'a Il2Entity, name: &str) -> Option<&'a Il2Entity> {
    let mut found = None;
    visit(root, &mut |e| {
        if found.is_none()
            && e.block_type == "MCU_CheckZone"
            && e.name().is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            found = Some(e);
        }
    });
    found
}

fn parse_zone_coalition(raw: &str) -> ZoneCoalition {
    let has1 = raw.contains('1');
    let has2 = raw.contains('2');
    match (has1, has2) {
        (true, true) => ZoneCoalition::Both,
        (true, false) => ZoneCoalition::Eastern,
        _ => ZoneCoalition::Western,
    }
}

fn parse_order_timer_name(name: &str) -> Option<(OrderKind, usize)> {
    let (label, num) = name.rsplit_once(' ')?;
    let seat = num.parse::<usize>().ok()?.checked_sub(1)?;
    let kind = OrderKind::from_label(label)?;
    Some((kind, seat))
}

fn parse_wp_number(name: &str) -> Option<u32> {
    name.strip_prefix("WP ")?.trim().parse().ok()
}

fn is_rtb_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("rtb east") || n.starts_with("rtb west") || n == "rtb delay"
}

fn read_native_orders(
    root: &Il2Entity,
    by_index: &HashMap<i32, &Il2Entity>,
    entity_to_seat: &HashMap<i32, usize>,
    opts: &mut TemplateOptions,
    consumed: &mut HashSet<i32>,
    timer_to_order: &mut HashMap<i32, (usize, usize)>,
    warnings: &mut Vec<String>,
) {
    let mut timers: Vec<(OrderKind, usize, &Il2Entity)> = Vec::new();
    visit(root, &mut |e| {
        if e.block_type != "MCU_Timer" {
            return;
        }
        let Some(name) = e.name() else {
            return;
        };
        if let Some((kind, seat)) = parse_order_timer_name(name) {
            if seat < opts.seats.len() {
                timers.push((kind, seat, e));
            }
        }
    });
    let mut ids: HashSet<i32> = HashSet::new();
    for (_, _, t) in &timers {
        if let Some(id) = t.index {
            ids.insert(id);
            consumed.insert(id);
        }
    }
    let n_seats = opts.seats.len();
    for seat in 0..n_seats {
        if !receives_orders(&opts.seats, seat) {
            continue;
        }
        let ordered = sort_native_timers(&timers, seat, &ids, by_index, root);
        for (kind, timer) in ordered {
            let spec = spec_from_native_timer(
                kind,
                timer,
                by_index,
                entity_to_seat,
                seat,
                consumed,
                warnings,
            );
            let oi = opts.seats[seat].orders.len();
            if let Some(id) = timer.index {
                timer_to_order.insert(id, (seat, oi));
            }
            opts.seats[seat].orders.push(spec);
        }
    }
}

fn sort_native_timers<'a>(
    timers: &[(OrderKind, usize, &'a Il2Entity)],
    seat: usize,
    order_ids: &HashSet<i32>,
    by_index: &HashMap<i32, &Il2Entity>,
    root: &Il2Entity,
) -> Vec<(OrderKind, &'a Il2Entity)> {
    let mine: Vec<(OrderKind, &Il2Entity)> = timers
        .iter()
        .filter(|(_, s, _)| *s == seat)
        .map(|(k, _, t)| (*k, *t))
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let mine_ids: HashSet<i32> = mine.iter().filter_map(|(_, t)| t.index).collect();
    let mut incoming: HashMap<i32, usize> = HashMap::new();
    for id in &mine_ids {
        incoming.insert(*id, 0);
    }
    for (_, t) in &mine {
        for next in next_order_timer_ids(t, by_index, order_ids) {
            if mine_ids.contains(&next) {
                *incoming.entry(next).or_insert(0) += 1;
            }
        }
    }
    // Events that Then an order timer are the real predecessor. Without this,
    // Mission Complete looks like a chain start (nothing in the timer graph
    // points at it) and load puts it before AttackArea / Cover.
    visit(root, &mut |e| {
        if e.block_type != "OnEvent" {
            return;
        }
        let tar = prop_i32(e, "TarId", -1);
        if mine_ids.contains(&tar) {
            *incoming.entry(tar).or_insert(0) += 1;
        }
    });
    let mut frontier: Vec<i32> = Vec::new();
    if let Some(after) = root.find_by_name("AFTER BRING UP") {
        for &id in &after.targets {
            if mine_ids.contains(&id) && !frontier.contains(&id) {
                frontier.push(id);
            }
        }
    }
    for (_, t) in &mine {
        let Some(id) = t.index else {
            continue;
        };
        if incoming.get(&id).copied().unwrap_or(0) == 0 && !frontier.contains(&id) {
            frontier.push(id);
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while !frontier.is_empty() {
        let id = frontier.remove(0);
        if !seen.insert(id) {
            continue;
        }
        let Some((kind, timer)) = mine.iter().find(|(_, t)| t.index == Some(id)) else {
            continue;
        };
        out.push((*kind, *timer));
        for next in next_order_timer_ids(timer, by_index, order_ids) {
            if mine_ids.contains(&next) && !seen.contains(&next) && !frontier.contains(&next) {
                frontier.push(next);
            }
        }
    }
    for (kind, t) in mine {
        if t.index.is_some_and(|id| !seen.contains(&id)) {
            out.push((kind, t));
        }
    }
    out
}

fn next_order_timer_ids(
    timer: &Il2Entity,
    by_index: &HashMap<i32, &Il2Entity>,
    order_ids: &HashSet<i32>,
) -> Vec<i32> {
    let mut out = Vec::new();
    for &tid in &timer.targets {
        if order_ids.contains(&tid) {
            out.push(tid);
            continue;
        }
        if let Some(e) = by_index.get(&tid) {
            if e.block_type == "MCU_Waypoint" {
                for &wid in &e.targets {
                    if order_ids.contains(&wid) {
                        out.push(wid);
                    }
                }
            }
        }
    }
    out
}

fn spec_from_native_timer(
    kind: OrderKind,
    timer: &Il2Entity,
    by_index: &HashMap<i32, &Il2Entity>,
    entity_to_seat: &HashMap<i32, usize>,
    owner: usize,
    consumed: &mut HashSet<i32>,
    warnings: &mut Vec<String>,
) -> OrderSpec {
    let mut spec = OrderSpec::default();
    spec.kind = kind;
    spec.delay_s = prop_f32(timer, "Time", BAKED_ORDER_DELAY as f32);
    if kind == OrderKind::TimeOnTarget || kind == OrderKind::Timer {
        spec.time_s = spec.delay_s;
    }
    for &tid in &timer.targets {
        let Some(e) = by_index.get(&tid) else {
            continue;
        };
        if e.block_type.starts_with("MCU_CMD_") {
            mark_consumed(e, consumed);
            fill_spec_from_cmd(&mut spec, e, entity_to_seat, owner, warnings);
        } else if e.block_type == "MCU_Waypoint" {
            mark_consumed(e, consumed);
            if let Some(n) = e.name().and_then(parse_wp_number) {
                spec.waypoint = n;
                spec.priority = prop_i32(e, "Priority", spec.priority);
                let y = prop_f32(e, "YPos", 0.0);
                if y > 0.0 {
                    spec.altitude = y;
                }
            }
        }
    }
    spec
}

fn fill_spec_from_cmd(
    spec: &mut OrderSpec,
    cmd: &Il2Entity,
    entity_to_seat: &HashMap<i32, usize>,
    owner: usize,
    warnings: &mut Vec<String>,
) {
    if spec.kind == OrderKind::AttackArea
        || cmd.block_type == "MCU_CMD_AttackArea"
    {
        if spec.kind == OrderKind::AttackArea {
            spec.attack_ground = prop_bool(cmd, "AttackGround", spec.attack_ground);
            spec.attack_air = prop_bool(cmd, "AttackAir", spec.attack_air);
            spec.attack_g_targets = prop_bool(cmd, "AttackGTargets", spec.attack_g_targets);
            spec.attack_area = prop_f32(cmd, "AttackArea", spec.attack_area);
            spec.time_s = prop_f32(cmd, "Time", spec.time_s);
        }
    }
    spec.priority = prop_i32(cmd, "Priority", spec.priority);
    spec.formation_type = prop_i32(cmd, "FormationType", spec.formation_type);
    spec.behaviour_filter = prop_i32(cmd, "Filter", spec.behaviour_filter);
    spec.flare_color = prop_i32(cmd, "Color", spec.flare_color);
    if cmd.block_type == "MCU_CMD_Effect" {
        spec.effect_start = !prop_bool(cmd, "ActionType", false);
    }
    spec.attack_group = prop_bool(cmd, "AttackGroup", spec.attack_group);
    spec.shared_with = cmd
        .objects
        .iter()
        .filter_map(|id| entity_to_seat.get(id).copied())
        .filter(|&si| si != owner)
        .collect();
    if spec.kind == OrderKind::Cover {
        spec.cover_lead = cmd
            .targets
            .first()
            .and_then(|id| entity_to_seat.get(id).copied())
            .filter(|&si| si != owner);
        if spec.cover_lead.is_none() && !cmd.targets.is_empty() {
            warnings.push(format!(
                "Cover order on seat {} had a target that is not a unit in this group.",
                owner + 1
            ));
        }
    }
    if spec.kind == OrderKind::Attack {
        spec.attack_seat = cmd
            .targets
            .first()
            .and_then(|id| entity_to_seat.get(id).copied())
            .filter(|&si| si != owner);
    }
}

fn read_foreign_orders(
    root: &Il2Entity,
    by_index: &HashMap<i32, &Il2Entity>,
    entity_to_seat: &HashMap<i32, usize>,
    opts: &mut TemplateOptions,
    consumed: &mut HashSet<i32>,
    warnings: &mut Vec<String>,
) {
    let mut cmds = Vec::new();
    visit(root, &mut |e| {
        if !e.block_type.starts_with("MCU_CMD_") {
            return;
        }
        if e.name() == Some("Force Complete - High") {
            return;
        }
        let Some(kind) = OrderKind::from_block_type(&e.block_type) else {
            return;
        };
        if kind == OrderKind::ForceComplete {
            warnings.push(format!(
                "Force Complete \"{}\" was treated as MISSION END cleanup, not a unit order.",
                e.name().unwrap_or("Force Complete")
            ));
            mark_consumed(e, consumed);
            return;
        }
        cmds.push(e);
    });
    let mut used_cmd = HashSet::new();
    for cmd in cmds {
        let Some(id) = cmd.index else {
            continue;
        };
        if !used_cmd.insert(id) {
            continue;
        }
        let Some(kind) = OrderKind::from_block_type(&cmd.block_type) else {
            continue;
        };
        let owner = cmd
            .objects
            .iter()
            .find_map(|oid| entity_to_seat.get(oid).copied())
            .or_else(|| {
                opts.seats
                    .iter()
                    .enumerate()
                    .find(|(i, _)| receives_orders(&opts.seats, *i))
                    .map(|(i, _)| i)
            });
        let Some(owner) = owner else {
            warnings.push(format!(
                "Dropped {} \"{}\" (not linked to a unit).",
                kind.label(),
                cmd.name().unwrap_or("command")
            ));
            continue;
        };
        if !receives_orders(&opts.seats, owner) {
            continue;
        }
        mark_consumed(cmd, consumed);
        let mut spec = OrderSpec::for_kind(opts.seats[owner].unit.kind);
        spec.kind = kind;
        fill_spec_from_cmd(&mut spec, cmd, entity_to_seat, owner, warnings);
        if let Some(timer) = unique_predecessor_timer(root, id, by_index) {
            let wait = prop_f32(timer, "Time", 0.0);
            if wait >= 1.0 {
                spec.delay_s = wait;
                warnings.push(format!(
                    "Timer \"{}\" ({wait:.0} s) became the delay before {} on seat {}.",
                    timer.name().unwrap_or("timer"),
                    kind.label(),
                    owner + 1
                ));
            }
            mark_consumed(timer, consumed);
        }
        if cmd.name().is_some_and(|n| n != kind.label()) {
            warnings.push(format!(
                "{} \"{}\" was imported as a {} order on seat {}.",
                cmd.block_type,
                cmd.name().unwrap_or("command"),
                kind.label(),
                owner + 1
            ));
        }
        opts.seats[owner].orders.push(spec);
    }

    let mut path_wps: Vec<&Il2Entity> = Vec::new();
    visit(root, &mut |e| {
        if e.block_type == "MCU_Waypoint" {
            if e.name().is_some_and(is_rtb_name) {
                return;
            }
            path_wps.push(e);
        }
    });
    path_wps.sort_by_key(|w| {
        w.name()
            .and_then(parse_wp_number)
            .unwrap_or(u32::MAX)
    });
    if path_wps.iter().all(|w| parse_wp_number(w.name().unwrap_or("")).is_none()) {
        path_wps.sort_by(|a, b| {
            let ax = a.pos_xz().map(|p| p.0).unwrap_or(0.0);
            let bx = b.pos_xz().map(|p| p.0).unwrap_or(0.0);
            ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    for (i, wp) in path_wps.iter().enumerate() {
        mark_consumed(wp, consumed);
        let n = parse_wp_number(wp.name().unwrap_or("")).unwrap_or((i as u32) + 1);
        let owners: Vec<usize> = wp
            .objects
            .iter()
            .filter_map(|id| entity_to_seat.get(id).copied())
            .filter(|&si| receives_orders(&opts.seats, si))
            .collect();
        let targets = if owners.is_empty() {
            order_seat_indexes(&opts.seats)
        } else {
            owners
        };
        for owner in targets {
            if opts.seats[owner]
                .orders
                .iter()
                .any(|o| o.kind == OrderKind::GotoWaypoint && o.waypoint == n)
            {
                continue;
            }
            let mut spec = OrderSpec::default();
            spec.kind = OrderKind::GotoWaypoint;
            spec.waypoint = n;
            spec.priority = prop_i32(wp, "Priority", 1);
            let y = prop_f32(wp, "YPos", 0.0);
            if y > 0.0 {
                spec.altitude = y;
            }
            let idx = opts.seats[owner]
                .orders
                .iter()
                .take_while(|o| o.kind == OrderKind::GotoWaypoint)
                .count();
            opts.seats[owner].orders.insert(idx, spec);
            warnings.push(format!(
                "Waypoint \"{}\" became Goto WP {n} on seat {}.",
                wp.name().unwrap_or("waypoint"),
                owner + 1
            ));
        }
    }

    read_foreign_reports(root, entity_to_seat, opts, warnings);
}

fn unique_predecessor_timer<'a>(
    root: &'a Il2Entity,
    cmd_id: i32,
    _by_index: &HashMap<i32, &Il2Entity>,
) -> Option<&'a Il2Entity> {
    let mut found = None;
    let mut count = 0;
    visit(root, &mut |e| {
        if e.block_type == "MCU_Timer" && e.targets.contains(&cmd_id) {
            count += 1;
            found = Some(e);
        }
    });
    if count == 1 {
        found
    } else {
        None
    }
}

fn read_foreign_reports(
    root: &Il2Entity,
    entity_to_seat: &HashMap<i32, usize>,
    opts: &mut TemplateOptions,
    warnings: &mut Vec<String>,
) {
    visit(root, &mut |e| {
        if e.block_type != "MCU_TR_Entity" {
            return;
        }
        let Some(&seat) = e.index.and_then(|id| entity_to_seat.get(&id)) else {
            return;
        };
        if !receives_orders(&opts.seats, seat) {
            return;
        }
        for wrap in &e.children {
            if wrap.block_type != "OnReports" {
                continue;
            }
            for r in &wrap.children {
                if r.block_type != "OnReport" {
                    continue;
                }
                let typ = prop_i32(r, "Type", -1);
                let kind = match typ {
                    0 => OrderKind::OnSpawned,
                    1 => OrderKind::OnTargetAttacked,
                    2 => OrderKind::OnAreaAttacked,
                    3 => OrderKind::OnTookOff,
                    4 => OrderKind::OnLanded,
                    _ => {
                        warnings.push(format!(
                            "Dropped OnReport Type {typ} on seat {}.",
                            seat + 1
                        ));
                        continue;
                    }
                };
                if opts.seats[seat].orders.iter().any(|o| o.kind == kind) {
                    continue;
                }
                opts.seats[seat].orders.push(OrderSpec {
                    kind,
                    ..OrderSpec::default()
                });
            }
        }
    });
}

fn read_rtb_orders(
    root: &Il2Entity,
    entity_to_seat: &HashMap<i32, usize>,
    opts: &mut TemplateOptions,
    consumed: &mut HashSet<i32>,
) {
    let mut rtb = Vec::new();
    visit(root, &mut |e| {
        if e.block_type == "MCU_Waypoint" && e.name().is_some_and(is_rtb_name) {
            rtb.push(e);
        }
    });
    if rtb.is_empty() && root.find_by_name("RTB DELAY").is_none() {
        return;
    }
    let mut owners: HashSet<usize> = HashSet::new();
    for wp in &rtb {
        mark_consumed(wp, consumed);
        for id in &wp.objects {
            if let Some(&si) = entity_to_seat.get(id) {
                let owner = if receives_orders(&opts.seats, si) {
                    si
                } else {
                    flight_lead_of(&opts.seats, si)
                };
                owners.insert(owner);
            }
        }
    }
    if owners.is_empty() {
        owners.extend(order_seat_indexes(&opts.seats));
    }
    for owner in owners {
        if opts.seats[owner]
            .orders
            .iter()
            .any(|o| o.kind == OrderKind::RtbOnZoneOut)
        {
            continue;
        }
        opts.seats[owner].orders.push(OrderSpec {
            kind: OrderKind::RtbOnZoneOut,
            ..OrderSpec::default()
        });
    }
}

fn read_events(
    loaded: &[LoadedUnit],
    by_index: &HashMap<i32, &Il2Entity>,
    entity_to_seat: &HashMap<i32, usize>,
    timer_to_order: &HashMap<i32, (usize, usize)>,
    opts: &mut TemplateOptions,
    consumed: &mut HashSet<i32>,
    warnings: &mut Vec<String>,
) {
    let death_id = by_index
        .values()
        .find(|e| e.name() == Some("DeathCount"))
        .and_then(|e| e.index);
    let force_id = by_index
        .values()
        .find(|e| e.name() == Some("Force Complete - High"))
        .and_then(|e| e.index);
    for unit in loaded {
        let Some(&si) = unit.entity.index.and_then(|id| entity_to_seat.get(&id)) else {
            continue;
        };
        let unit_name = unit.object.name().unwrap_or("unit");
        for wrap in &unit.entity.children {
            if wrap.block_type != "OnEvents" {
                continue;
            }
            for ev in &wrap.children {
                if ev.block_type != "OnEvent" {
                    continue;
                }
                let typ = prop_i32(ev, "Type", -1);
                let tar = prop_i32(ev, "TarId", -1);
                if death_id == Some(tar) {
                    continue;
                }
                let Some(kind) = EntityEvent::from_type_id(typ) else {
                    warnings.push(format!(
                        "Dropped OnEvent Type {typ} on {unit_name} (unknown event)."
                    ));
                    continue;
                };
                let then = if force_id == Some(tar) {
                    EventThen::ForceComplete
                } else if let Some(&(chain_si, oi)) = timer_to_order.get(&tar) {
                    let _ = chain_si;
                    EventThen::Order(oi)
                } else if let Some(target) = by_index.get(&tar) {
                    warnings.push(format!(
                        "{} on {unit_name} targeted \"{}\" — dropped (not Force Complete or an order).",
                        kind.label(),
                        target.name().unwrap_or("MCU")
                    ));
                    mark_consumed(target, consumed);
                    continue;
                } else {
                    warnings.push(format!(
                        "{} on {unit_name} targeted missing MCU {tar} — dropped.",
                        kind.label()
                    ));
                    continue;
                };
                opts.seats[si].events.push(EventHook { kind, then });
            }
        }
    }
}

fn read_waypoints_into_opts(
    root: &Il2Entity,
    opts: &mut TemplateOptions,
    consumed: &mut HashSet<i32>,
) {
    visit(root, &mut |e| {
        if e.block_type != "MCU_Waypoint" {
            return;
        }
        let Some(n) = e.name().and_then(parse_wp_number) else {
            return;
        };
        mark_consumed(e, consumed);
        opts.waypoint_speed = prop_f32(e, "Speed", opts.waypoint_speed);
        if n == 1 {
            opts.waypoint_priority = prop_i32(e, "Priority", opts.waypoint_priority);
            let y = prop_f32(e, "YPos", 0.0);
            let plane_alt = first_plane_altitude(&opts.seats) as f32;
            if y > 0.0 && (y - plane_alt).abs() > 1.0 {
                opts.waypoint_altitude = y;
            }
        }
        let y = prop_f32(e, "YPos", 0.0);
        let plane_alt = first_plane_altitude(&opts.seats) as f32;
        let hop_alt = if y > 0.0 && (y - plane_alt).abs() > 1.0 && (opts.waypoint_altitude - y).abs() > 1.0
        {
            y
        } else {
            0.0
        };
        for seat in &mut opts.seats {
            for order in &mut seat.orders {
                if order.kind == OrderKind::GotoWaypoint && order.waypoint.max(1) == n {
                    order.priority = prop_i32(e, "Priority", order.priority);
                    if hop_alt > 0.0 {
                        order.altitude = hop_alt;
                    } else if (y - plane_alt).abs() <= 1.0 {
                        order.altitude = 0.0;
                    }
                }
            }
        }
    });
}

fn infer_layout(loaded: &[LoadedUnit], opts: &mut TemplateOptions, warnings: &mut Vec<String>) {
    let positions: Vec<(f64, f64)> = loaded
        .iter()
        .filter_map(|u| u.object.pos_xz())
        .collect();
    if positions.len() < 2 {
        if opts.seats.iter().any(|s| s.unit.is_air()) {
            opts.place_layout = PlaceLayout::InvertedVee;
            opts.per_group = 4;
        } else {
            opts.place_layout = PlaceLayout::Column;
            opts.per_group = opts.seats.len().max(1) as u32;
        }
        return;
    }
    let origin = positions[0];
    let rel: Vec<(f64, f64)> = positions
        .iter()
        .map(|(x, z)| (x - origin.0, z - origin.1))
        .collect();
    let n = rel.len();
    let spacing = PLACEMENT_SPACING as f64;
    let mut best = (f64::MAX, PlaceLayout::Column, n.min(8) as u32);
    for layout in PlaceLayout::ALL {
        for per in 1..=n.min(8) {
            let mut err = 0.0;
            for (i, &(dx, dz)) in rel.iter().enumerate() {
                let (ex, ez) = place_offset(layout, i, per, spacing);
                let ddx = dx - ex;
                let ddz = dz - ez;
                err += ddx * ddx + ddz * ddz;
            }
            if err < best.0 {
                best = (err, layout, per as u32);
            }
        }
    }
    let rms = (best.0 / n as f64).sqrt();
    if rms <= 25.0 {
        opts.place_layout = best.1;
        opts.per_group = best.2;
    } else {
        if opts.seats.iter().any(|s| s.unit.is_air()) {
            opts.place_layout = PlaceLayout::InvertedVee;
            opts.per_group = 4.min(n as u32).max(1);
        } else {
            opts.place_layout = PlaceLayout::Column;
            opts.per_group = n.min(8) as u32;
        }
        warnings.push(format!(
            "Unit positions did not match a Template Builder formation (about {rms:.0} m off). Using {} at 150 m; original map placement will not be kept.",
            opts.place_layout.label()
        ));
    }
}

fn collect_dropped(
    root: &Il2Entity,
    consumed: &HashSet<i32>,
    native: bool,
    warnings: &mut Vec<String>,
) {
    let mut icons = 0;
    let mut subtitles = 0;
    let mut extra_zones = Vec::new();
    let mut extra_groups = Vec::new();
    let mut extra_cmds = Vec::new();
    let mut extra_other = Vec::new();
    let mut nodegates = false;
    visit(root, &mut |e| {
        if std::ptr::eq(e, root) {
            return;
        }
        if e.block_type == "Group" && e.name().is_some_and(|n| n.eq_ignore_ascii_case("NodeGates")) {
            nodegates = true;
            return;
        }
        if matches!(e.name(), Some("Logic" | "Units" | "Orders" | "Waypoints")) {
            return;
        }
        if consumed.contains(&e.index.unwrap_or(-1)) {
            return;
        }
        match e.block_type.as_str() {
            "MCU_Icon" => icons += 1,
            "MCU_TR_Subtitle" | "MCU_Subtitle" => subtitles += 1,
            "MCU_CheckZone" => {
                extra_zones.push(e.name().unwrap_or("checkzone").to_string());
            }
            "Group" => {
                if let Some(n) = e.name() {
                    if !matches!(n, "Logic" | "Units" | "Orders" | "Waypoints")
                        && !n.eq_ignore_ascii_case("NodeGates")
                    {
                        extra_groups.push(n.to_string());
                    }
                }
            }
            b if b.starts_with("MCU_CMD_") => {
                extra_cmds.push(format!(
                    "{} \"{}\"",
                    b,
                    e.name().unwrap_or("command")
                ));
            }
            "MCU_Timer" | "MCU_Counter" | "MCU_Activate" | "MCU_Deactivate"
            | "MCU_Delete" | "MCU_Spawner" | "MCU_TR_MissionBegin"
            | "MCU_Waypoint" | "MCU_TR_ComplexTrigger" | "MCU_Random" => {
                if native || distinctive_drop(e) {
                    extra_other.push(format!(
                        "{} \"{}\"",
                        e.block_type,
                        e.name().unwrap_or("MCU")
                    ));
                }
            }
            "Plane" | "Vehicle" | "Train" | "Ship" | "MCU_TR_Entity"
            | "OnEvents" | "OnEvent" | "OnReports" | "OnReport" | "Carriages" => {}
            _ => {
                if e.index.is_some() && distinctive_drop(e) {
                    extra_other.push(format!(
                        "{} \"{}\"",
                        e.block_type,
                        e.name().unwrap_or("block")
                    ));
                }
            }
        }
    });
    if nodegates {
        warnings.push("Dropped NodeGates (fighter-pack link logic).".into());
    }
    if icons > 0 {
        warnings.push(format!(
            "Dropped {icons} map icon{}.",
            if icons == 1 { "" } else { "s" }
        ));
    }
    if subtitles > 0 {
        warnings.push(format!(
            "Dropped {subtitles} subtitle{}.",
            if subtitles == 1 { "" } else { "s" }
        ));
    }
    extra_zones.sort();
    extra_zones.dedup();
    for name in extra_zones {
        warnings.push(format!("Dropped checkzone \"{name}\"."));
    }
    extra_groups.sort();
    extra_groups.dedup();
    for name in extra_groups {
        warnings.push(format!(
            "Dropped subgroup \"{name}\" (custom logic is not imported)."
        ));
    }
    for cmd in extra_cmds {
        warnings.push(format!("Dropped command {cmd}."));
    }
    extra_other.sort();
    extra_other.dedup();
    const MAX_OTHER: usize = 12;
    let extra_n = extra_other.len();
    for item in extra_other.into_iter().take(MAX_OTHER) {
        warnings.push(format!("Dropped {item}."));
    }
    if extra_n > MAX_OTHER {
        warnings.push(format!(
            "Dropped {} more custom MCU(s).",
            extra_n - MAX_OTHER
        ));
    }
}

fn distinctive_drop(e: &Il2Entity) -> bool {
    let name = e.name().unwrap_or("");
    if name.is_empty() {
        return false;
    }
    !matches!(
        name,
        "Self Deactivate"
            | "Translator Mission Begin"
            | "Trigger Delete"
            | "Deactivate Units"
            | "Deactivate Unit(s)"
            | "Activate Units"
            | "Activate Unit(s)"
            | "Trigger Activate"
            | "Trigger Deactivate"
            | "Trigger Timer"
            | "Trigger Check Zone"
            | "Force Complete"
            | "ORDERS"
            | "2s"
            | "3s"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_group_file, parse_il2_document};
    use crate::serialize::serialize_group;

    fn four_migs() -> TemplateOptions {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.seats = (0..4)
            .map(|i| {
                let mut seat = TemplateSeat::new(mig.clone());
                seat.role = if i == 0 {
                    FlightRole::Lead
                } else {
                    FlightRole::Follows(0)
                };
                if i == 0 {
                    seat.orders = vec![
                        OrderSpec {
                            kind: OrderKind::Formation,
                            formation_type: 23,
                            ..OrderSpec::default()
                        },
                        OrderSpec {
                            kind: OrderKind::AttackArea,
                            ..OrderSpec::default()
                        },
                    ];
                }
                seat.number_in_formation = i as i32;
                seat.country = 501;
                seat.skill = 2;
                seat.altitude = 1000.0;
                seat
            })
            .collect();
        opts
    }

    fn one_mig() -> TemplateOptions {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.seats = vec![TemplateSeat::new(mig)];
        opts.waypoint_count = 0;
        opts
    }

    #[test]
    fn finger_four_has_distinct_seats() {
        let pts: Vec<_> = (0..4).map(|i| finger_four_offset(i, 150.0)).collect();
        for (i, a) in pts.iter().enumerate() {
            for (j, b) in pts.iter().enumerate() {
                if i != j {
                    assert!(
                        (a.0 - b.0).abs() > 1.0 || (a.1 - b.1).abs() > 1.0,
                        "seats {i} and {j} overlap"
                    );
                }
            }
        }
        assert_eq!(pts[0], (0.0, 0.0));
        assert!(pts[1].1 > 0.0, "seat 2 is right of lead (east / +Z)");
        assert!(pts[2].1 < 0.0, "seat 3 is left of lead");
        let next = finger_four_offset(4, 150.0);
        assert!(next.0 < pts[0].0, "second flight sits behind the first");
    }

    #[test]
    fn catalog_reads_kind_subgroups() {
        let src = r#"Group
{
  Name = "Unit Catalog";
  Index = 1;
  Group
  {
    Name = "Planes";
    Index = 2;
    Plane
    {
      Name = "MiG";
      Index = 3;
      LinkTrId = 4;
      Script = "LuaScripts\WorldObjects\Planes\mig15bis.txt";
      Model = "graphics\planes\mig15bis\mig15bis.mgm";
    }
    MCU_TR_Entity
    {
      Index = 4;
      Name = "Plane entity";
      MisObjID = 3;
      Enabled = 0;
    }
  }
  Group
  {
    Name = "Vehicles";
    Index = 5;
    Vehicle
    {
      Name = "T-34";
      Index = 6;
      LinkTrId = 7;
      Script = "LuaScripts\WorldObjects\Vehicles\t34.txt";
    }
    MCU_TR_Entity
    {
      Index = 7;
      Name = "Vehicle entity";
      MisObjID = 6;
      Enabled = 0;
    }
  }
}
"#;
        let root = parse_group_file(src).unwrap();
        let cat = load_catalog(&root);
        assert_eq!(cat.len(), 2);
        assert_eq!(cat[0].kind, UnitKind::Plane);
        assert_eq!(cat[0].name, "MiG");
        assert_eq!(cat[1].kind, UnitKind::Vehicle);
        assert_eq!(cat[1].name, "T-34");
    }

    #[test]
    fn catalog_reads_all_trains_from_model_types() {
        let root =
            parse_il2_document(include_str!("../assets/Models.Group")).unwrap();
        let cat = load_catalog(&root);
        assert!(cat.iter().any(|u| u.kind == UnitKind::Plane));
        assert!(cat.iter().any(|u| u.kind == UnitKind::Vehicle));
        assert!(
            cat.iter().any(|u| u.kind == UnitKind::Train),
            "All Trains prototypes should load as Train units"
        );
        assert!(cat.iter().any(|u| u.kind == UnitKind::Ship));
        let train = cat
            .iter()
            .find(|u| u.kind == UnitKind::Train)
            .expect("train prototype");
        let cars = train.prototype_carriages();
        assert!(
            cars.iter().any(|s| s.to_ascii_lowercase().contains("carbox")),
            "catalog train should list rail cars, got {cars:?}"
        );
        let tender = train.default_carriages();
        assert_eq!(tender.len(), 1, "default consist is the tender, got {tender:?}");
        assert!(script_type_id(&tender[0]).contains("tender"));
        assert!(tender.len() < cars.len());
    }

    #[test]
    fn bundled_catalog_splits_infantry_by_country() {
        let cat = bundled_catalog();
        let infantry: Vec<_> = cat.iter().filter(|u| u.kind == UnitKind::Infantry).collect();
        assert_eq!(infantry.len(), 9, "nine Korea squads");
        assert!(
            !cat.iter().any(|u| u.kind == UnitKind::Vehicle
                && weapon_range::is_infantry_script(&u.script)),
            "infantry scripts must not stay under Vehicles"
        );
        let mut countries: Vec<i32> = infantry.iter().map(|u| u.country()).collect();
        countries.sort();
        countries.dedup();
        assert_eq!(countries, vec![502, 503, 601], "PRC, DPRK, USA");
        let dprk = infantry.iter().filter(|u| u.country() == 503).count();
        let prc = infantry.iter().filter(|u| u.country() == 502).count();
        let usa = infantry.iter().filter(|u| u.country() == 601).count();
        assert_eq!((dprk, prc, usa), (3, 3, 3));
    }

    #[test]
    fn generate_train_writes_selected_carriages_in_order() {
        let train = bundled_catalog()
            .into_iter()
            .find(|u| u.kind == UnitKind::Train)
            .expect("train in catalog");
        let cars = train.prototype_carriages();
        let box_car = cars
            .iter()
            .find(|s| script_type_id(s) == "carbox")
            .cloned()
            .expect("box car");
        let tank = cars
            .iter()
            .find(|s| script_type_id(s) == "cartank")
            .cloned()
            .expect("tank car");
        let mut seat = TemplateSeat::new(train);
        let tender = seat.carriages.clone();
        assert_eq!(tender.len(), 1);
        seat.carriages = vec![tender[0].clone(), box_car.clone(), tank.clone()];
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        opts.per_group = 1;
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("Train"), 1);
        let mut written = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Train" {
                written = train_carriages(e);
            }
        });
        assert_eq!(written, vec![tender[0].clone(), box_car, tank]);
        let text = serialize_group(&pack);
        assert!(text.contains("carbox.txt"));
        assert!(text.contains("cartank.txt"));
        assert!(
            !text.contains("carpassenger.txt"),
            "unselected cars must not be written"
        );
        pack.for_each(&mut |e| {
            if e.block_type == "Train" {
                assert_eq!(
                    e.property("ModMask"),
                    Some("0"),
                    "stock train ModMask is 0, not aircraft 1"
                );
            }
        });
    }

    #[test]
    fn generate_train_writes_selected_carriage_mod_mask() {
        let train = bundled_catalog()
            .into_iter()
            .find(|u| u.kind == UnitKind::Train)
            .expect("train in catalog");
        let box_car = train
            .prototype_carriages()
            .into_iter()
            .find(|s| script_type_id(s) == "carbox")
            .expect("box car");
        let mut seat = TemplateSeat::new(train);
        assert_eq!(seat.mod_mask, "0");
        seat.carriages.push(box_car);
        seat.mod_mask = "100".into();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        opts.per_group = 1;
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        pack.for_each(&mut |e| {
            if e.block_type == "Train" {
                assert_eq!(e.property("ModMask"), Some("100"));
            }
        });
    }

    #[test]
    fn generate_vehicle_writes_stock_mod_mask_zero() {
        let gaz = bundled_catalog()
            .into_iter()
            .find(|u| u.script.to_ascii_lowercase().contains("gaz63.txt"))
            .expect("GAZ-63 in catalog");
        let mut seat = TemplateSeat::new(gaz);
        assert_eq!(seat.mod_mask, "0");
        seat.mod_mask = "100".into();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        opts.per_group = 1;
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        pack.for_each(&mut |e| {
            if e.block_type == "Vehicle" {
                assert_eq!(e.property("ModMask"), Some("100"));
            }
        });
        let stock = bundled_catalog()
            .into_iter()
            .find(|u| u.script.to_ascii_lowercase().contains("gaz63.txt"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        opts.per_group = 1;
        opts.seats = vec![TemplateSeat::new(stock)];
        let pack = generate_template(&opts).unwrap();
        pack.for_each(&mut |e| {
            if e.block_type == "Vehicle" {
                assert_eq!(
                    e.property("ModMask"),
                    Some("0"),
                    "stock vehicle ModMask is 0, not aircraft 1"
                );
            }
        });
    }

    #[test]
    fn bundled_catalog_includes_fixed_and_user_added() {
        let cat = bundled_catalog();
        assert!(
            cat.iter().any(|u| u.kind == UnitKind::Fixed && u.script.to_ascii_lowercase().contains("fixedobjects")),
            "Fixed Objects group should load as Fixed Units"
        );
        assert!(
            !cat.iter().any(|u| u.kind == UnitKind::UserAdded),
            "User Added stays empty until the user appends a group"
        );
        let src = include_str!("../assets/Models.Group");
        let expected: Vec<String> = src
            .lines()
            .filter_map(|line| {
                let t = line.trim();
                let rest = t.strip_prefix("Script = \"")?;
                let path = rest.strip_suffix("\";")?;
                if path.to_ascii_lowercase().contains("fixedobjects") {
                    Some(path.to_string())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            !expected.is_empty(),
            "Models.Group should list fixed-object scripts"
        );
        let mut loaded: Vec<String> = cat
            .iter()
            .filter(|u| u.kind == UnitKind::Fixed)
            .map(|u| u.script.clone())
            .collect();
        loaded.sort();
        let mut expected = expected;
        expected.sort();
        expected.dedup();
        loaded.dedup();
        assert_eq!(
            loaded, expected,
            "Fixed Units catalog should include every prototype in Models.Group"
        );
    }

    #[test]
    fn bundled_catalog_units_have_model_specs() {
        for unit in bundled_catalog() {
            assert!(
                crate::model_spec::spec_for(&unit.script).is_some(),
                "missing ModelSpec for {} ({})",
                unit.label(),
                unit.script
            );
        }
    }

    #[test]
    fn zone_defaults_follow_air_ground_train_mix() {
        assert_eq!(zone_defaults(ZoneMix::Air), (16_000.0, 35_000.0));
        assert_eq!(zone_defaults(ZoneMix::Ground), (10_000.0, 19_000.0));
        assert_eq!(zone_defaults(ZoneMix::Train), (19_000.0, 30_000.0));
        assert_eq!(visual_range_m(ZoneMix::Air), 16_000.0);
        assert_eq!(visual_range_m(ZoneMix::Ground), 10_000.0);
        assert_eq!(visual_range_m(ZoneMix::Train), 19_000.0);
        assert!(near_visual_range(16_000.0, 16_000.0));
        assert!(near_visual_range(16_400.0, 16_000.0));
        assert!(!near_visual_range(18_000.0, 16_000.0));
        assert!(near_visual_range(10_200.0, 10_000.0));
        assert!(near_visual_range(18_500.0, 19_000.0));
        assert!(zone_mix_for_seats(&[]).is_none());

        let plane = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let vehicle = bundled_catalog()
            .into_iter()
            .find(|u| u.kind == UnitKind::Vehicle)
            .unwrap();
        let train = bundled_catalog()
            .into_iter()
            .find(|u| u.kind == UnitKind::Train)
            .unwrap();
        assert_eq!(
            zone_mix_for_seats(&[TemplateSeat::new(plane.clone())]),
            Some(ZoneMix::Air)
        );
        assert_eq!(
            zone_mix_for_seats(&[TemplateSeat::new(vehicle.clone())]),
            Some(ZoneMix::Ground)
        );
        assert_eq!(
            zone_mix_for_seats(&[TemplateSeat::new(train.clone())]),
            Some(ZoneMix::Train)
        );
        assert_eq!(
            zone_mix_for_seats(&[
                TemplateSeat::new(train.clone()),
                TemplateSeat::new(vehicle.clone()),
            ]),
            Some(ZoneMix::Train)
        );
        assert_eq!(
            zone_mix_for_seats(&[
                TemplateSeat::new(train.clone()),
                TemplateSeat::new(vehicle.clone()),
                TemplateSeat::new(vehicle.clone()),
            ]),
            Some(ZoneMix::Ground)
        );
        assert_eq!(
            zone_mix_for_seats(&[TemplateSeat::new(plane), TemplateSeat::new(train)]),
            Some(ZoneMix::Air)
        );
        let opts = TemplateOptions::default();
        assert!((opts.zone_in - AIR_ZONE_IN_M).abs() < f32::EPSILON);
        assert!((opts.zone_out - AIR_ZONE_OUT_M).abs() < f32::EPSILON);
    }

    #[test]
    fn generate_writes_hooks_other_modes_need() {
        let pack = generate_template(&four_migs()).unwrap();
        assert!(pack.find_by_name("Zone IN").is_some());
        assert!(pack.find_by_name("Zone Out").is_some());
        assert!(pack.find_by_name("ENABLE / PULSE IN").is_some());
        assert!(pack.find_by_name("PULSE OUT").is_some());
        assert!(pack.find_by_name("COOLDOWN").is_some());
        assert!(pack.find_by_name("END").is_none());
        assert!(pack.find_by_name("MISSION END").is_some());
        assert!(pack.find_by_name("MISSION END ORDERS").is_some());
        assert!(pack.find_by_name("DELAYED END ORDERS").is_some());
        assert!(pack.find_by_name("Translator Mission Begin").is_some());
        assert!(pack.find_by_name("NodeGates").is_none());
        let zone_in = pack.find_by_name("Zone IN").unwrap();
        assert_eq!(zone_in.property("Closer"), Some("1"));
        assert_eq!(zone_in.property("PlaneCoalitions"), Some("[2]"));
        let zone_out = pack.find_by_name("Zone Out").unwrap();
        assert_eq!(zone_out.property("Closer"), Some("0"));
        let in_r: f64 = zone_in.property("Zone").unwrap().parse().unwrap();
        let out_r: f64 = zone_out.property("Zone").unwrap().parse().unwrap();
        assert!(out_r > in_r);
        assert!(pack.find_by_name("AttackArea").is_some());
        assert!(pack.find_by_name("WP 1").is_none());
        assert!(pack.find_by_name("WP DELAY").is_none());
        assert_eq!(pack.count_block_type("Plane"), 4);
        assert_eq!(pack.count_block_type("MCU_TR_Entity"), 4);
        let text = serialize_group(&pack);
        let again = parse_group_file(&text).unwrap();
        assert!(again.find_by_name("Zone IN").is_some());
        assert!(text.contains("MCU_CMD_AttackArea"));
        assert!(text.contains("MCU_CMD_Formation"));
        assert!(!text.contains('\u{2013}'));
        assert!(!text.contains("NodeGates"));
        let formation = pack.find_by_name("Formation").unwrap();
        assert_eq!(formation.property("FormationType"), Some("23"));
        assert_eq!(formation.property("FormationDensity"), Some("0"));
        assert_eq!(formation.property("FlightSize"), Some("1"));
    }

    #[test]
    fn empty_units_is_an_error() {
        let opts = TemplateOptions::default();
        assert!(generate_template(&opts).is_err());
    }

    #[test]
    fn mission_end_is_cleanup_hub() {
        let pack = generate_template(&four_migs()).unwrap();
        let delete = pack.find_by_name("Trigger Delete").unwrap();
        assert_eq!(delete.objects.len(), 4);
        let hub = pack.find_by_name("MISSION END").unwrap();
        let end_orders = pack.find_by_name("MISSION END ORDERS").unwrap();
        let delayed = pack.find_by_name("DELAYED END ORDERS").unwrap();
        assert!(hub.targets.contains(&end_orders.index.unwrap()));
        assert!(hub.targets.contains(&delayed.index.unwrap()));
        let force = pack.find_by_name("Force Complete - High").unwrap();
        assert_eq!(force.property("Priority"), Some("2"));
        assert!(end_orders.targets.contains(&force.index.unwrap()));
        assert!(pack.find_by_name("RTB").is_none());
        assert!(pack.find_by_name("RTB DELAY").is_none());
        assert!(!end_orders.targets.contains(&pack.find_by_name("Deactivate Units").unwrap().index.unwrap()));
        assert!(delayed.targets.contains(&pack.find_by_name("DELETE DELAY").unwrap().index.unwrap()));
        assert!(pack.find_by_name("Deactivate Units").is_some());
        let info = crate::bombers::inspect_plan(&pack).unwrap();
        assert_eq!(info.suggested_completion, hub.index);
        assert!(
            info.cleanup_warnings.get(&hub.index.unwrap()).is_none(),
            "MISSION END should reach Delete covering every unit, got {:?}",
            info.cleanup_warnings.get(&hub.index.unwrap())
        );
    }

    #[test]
    fn activate_skips_spawner() {
        let pack = generate_template(&four_migs()).unwrap();
        assert_eq!(pack.count_block_type("MCU_Spawner"), 0);
        assert_eq!(pack.count_block_type("MCU_Counter"), 0);
        assert!(pack.find_by_name("Activate Units").is_some());
        let zone_in = pack.find_by_name("Zone IN").unwrap();
        let activate = pack.find_by_name("Activate Units").unwrap();
        assert!(
            !zone_in.targets.contains(&activate.index.unwrap()),
            "Zone IN must not pulse Activate directly"
        );
        let bring = pack.find_by_name("MISSION BEGIN").unwrap();
        assert!(zone_in.targets.contains(&bring.index.unwrap()));
        assert!(bring.targets.contains(&activate.index.unwrap()));
    }

    #[test]
    fn zone_in_activates_then_pulses_zone_out() {
        let pack = generate_template(&four_migs()).unwrap();
        let zone_in = pack.find_by_name("Zone IN").unwrap();
        let zone_out = pack.find_by_name("Zone Out").unwrap();
        let react_out = pack.find_by_name("Zone Out ReActivate").unwrap();
        let pulse_out = pack.find_by_name("PULSE OUT").unwrap();
        assert_eq!(pulse_out.block_type, "MCU_Timer");
        assert_eq!(pulse_out.property("Time"), Some("0.10"));
        assert!(zone_in.targets.contains(&react_out.index.unwrap()));
        assert!(zone_in.targets.contains(&pulse_out.index.unwrap()));
        assert!(react_out.targets.contains(&zone_out.index.unwrap()));
        assert!(pulse_out.targets.contains(&zone_out.index.unwrap()));
        assert_eq!(react_out.block_type, "MCU_Activate");
    }

    fn assert_repeat_spawn_graph(root: &Il2Entity) {
        assert!(root.find_by_name("SpawnCount ReActivate").is_none());
        assert!(root.find_by_name("SpawnCount Deactivate").is_none());
        assert!(root.find_by_name("CATCH ALL").is_none());

        let spawn_count = root.find_by_name("SpawnCount").unwrap();
        let spawner = root.find_by_name("Trigger Spawner").unwrap();
        let death = root.find_by_name("DeathCount").unwrap();
        let cooldown = root.find_by_name("COOLDOWN").unwrap();
        let zone_in = root.find_by_name("Zone IN").unwrap();
        let zone_out = root.find_by_name("Zone Out").unwrap();
        let mission_end = root.find_by_name("MISSION END").unwrap();
        let react_in = root.find_by_name("Zone In ReActivate").unwrap();
        let pulse_in = root.find_by_name("ENABLE / PULSE IN").unwrap();
        let death_on = root.find_by_name("DeathCount ReActivate").unwrap();
        let death_off = root.find_by_name("DeathCount Deactivate").unwrap();
        let reset = root.find_by_name("Reset Counter").unwrap();
        let modifier = root.find_by_name("Modifier Set Value").unwrap();

        assert_eq!(spawn_count.property("Dropcount"), Some("1"));
        assert_eq!(spawn_count.targets, vec![spawner.index.unwrap()]);

        assert_eq!(modifier.block_type, "MCU_ModifierSetVal");
        assert_eq!(modifier.property("ParamIndex"), Some("0"));
        assert_eq!(modifier.property("Data0"), Some("0"));
        assert_eq!(modifier.targets, vec![death.index.unwrap()]);
        assert_eq!(reset.block_type, "MCU_Timer");
        assert_eq!(reset.targets, vec![modifier.index.unwrap()]);

        assert_eq!(death.targets, vec![cooldown.index.unwrap()]);
        assert_eq!(cooldown.targets, vec![spawner.index.unwrap()]);
        assert!(
            !death.targets.contains(&mission_end.index.unwrap()),
            "a wipe must not cleanup mid-fight"
        );

        assert!(zone_in.targets.contains(&death_on.index.unwrap()));
        assert!(zone_out.targets.contains(&death_off.index.unwrap()));
        assert!(zone_out.targets.contains(&mission_end.index.unwrap()));
        assert!(zone_out.targets.contains(&react_in.index.unwrap()));
        assert!(zone_out.targets.contains(&pulse_in.index.unwrap()));
        assert!(zone_out.targets.contains(&reset.index.unwrap()));
        assert!(
            !zone_out.targets.contains(&cooldown.index.unwrap()),
            "Zone Out must not start cooldown"
        );
    }

    #[test]
    fn improved_cooldown_logic_example_matches_repeat_spawn_contract() {
        let root = parse_group_file(include_str!(
            "../TemplateExamples/ImprovedCooldownLogic.Group"
        ))
        .expect("parse ImprovedCooldownLogic.Group");
        assert_repeat_spawn_graph(&root);
        let death = root.find_by_name("DeathCount").unwrap();
        let events = entity_events(&root);
        let hits = events
            .iter()
            .filter(|(t, tar)| *t == 4 && *tar == death.index.unwrap())
            .count();
        assert_eq!(hits, 4);
    }

    #[test]
    fn spawn_uses_counter_reset() {
        let mut opts = one_mig();
        opts.bring_up = BringUp::Spawn;
        opts.allow_multiple_spawns = true;
        opts.spawn_cooldown_min = 5.0;
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("MCU_Spawner"), 1);
        assert!(pack.find_by_name("Activate Units").is_none());
        let counter = pack.find_by_name("SpawnCount").unwrap();
        assert_eq!(counter.property("Counter"), Some("1"));
        let spawner = pack.find_by_name("Trigger Spawner").unwrap();
        assert_eq!(spawner.objects.len(), 1);
        let mut entities = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_TR_Entity" {
                entities.push(e.index.unwrap());
            }
        });
        assert_eq!(spawner.objects, entities);
        assert!(pack.find_by_name("SPAWN UNITS").is_some());
        let cooldown = pack.find_by_name("COOLDOWN").unwrap();
        assert_eq!(cooldown.property("Time"), Some("300"));
        let death = pack.find_by_name("DeathCount").unwrap();
        assert_eq!(death.property("Counter"), Some("1"));
        assert_repeat_spawn_graph(&pack);
        let delete = pack.find_by_name("Trigger Delete").unwrap();
        let mut planes = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                planes.push(e.index.unwrap());
            }
        });
        assert_eq!(delete.objects, planes);
        let events = entity_events(&pack);
        assert!(
            events.iter().any(|(t, tar)| *t == 4 && *tar == death.index.unwrap()),
            "aircraft OnPlaneDestroyed should pulse DeathCount, got {events:?}"
        );
        let text = serialize_group(&pack);
        assert!(text.contains("MCU_ModifierSetVal"));
        assert!(text.contains("ParamIndex = 0;"));
    }

    #[test]
    fn repeat_spawn_counts_every_destroyed_unit() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.bring_up = BringUp::Spawn;
        opts.allow_multiple_spawns = true;
        opts.spawn_cooldown_min = 5.0;
        opts.waypoint_count = 0;
        opts.seats = (0..3)
            .map(|_| {
                let mut seat = TemplateSeat::new(mig.clone());
                seat.role = FlightRole::Independent;
                seat
            })
            .collect();
        let pack = generate_template(&opts).unwrap();
        let death = pack.find_by_name("DeathCount").unwrap();
        assert_eq!(death.property("Counter"), Some("3"));
        let death_id = death.index.unwrap();
        let events = entity_events(&pack);
        let hits = events.iter().filter(|(t, tar)| *t == 4 && *tar == death_id).count();
        assert_eq!(hits, 3);
        assert_repeat_spawn_graph(&pack);
        let zone_out = pack.find_by_name("Zone Out").unwrap();
        let mission_end = pack.find_by_name("MISSION END").unwrap();
        assert!(zone_out.targets.contains(&mission_end.index.unwrap()));
        assert!(!death.targets.contains(&mission_end.index.unwrap()));
    }

    #[test]
    fn spawn_oneshot_has_no_reset() {
        let mut opts = one_mig();
        opts.bring_up = BringUp::Spawn;
        opts.allow_multiple_spawns = false;
        let pack = generate_template(&opts).unwrap();
        let counter = pack.find_by_name("SpawnCount").unwrap();
        assert_eq!(counter.property("Counter"), Some("1"));
        assert_eq!(counter.property("Dropcount"), Some("0"));
        assert_eq!(pack.find_by_name("COOLDOWN").unwrap().property("Time"), Some("0"));
        assert!(pack.find_by_name("DeathCount").is_none());
        assert!(pack.find_by_name("Reset Counter").is_none());
        assert!(pack.find_by_name("Modifier Set Value").is_none());
        let zone_out = pack.find_by_name("Zone Out").unwrap();
        let cooldown = pack.find_by_name("COOLDOWN").unwrap();
        let mission_end = pack.find_by_name("MISSION END").unwrap();
        assert!(zone_out.targets.contains(&cooldown.index.unwrap()));
        assert!(zone_out.targets.contains(&mission_end.index.unwrap()));
    }

    #[test]
    fn flights_with_wingmen_cannot_spawn() {
        let mut opts = four_migs();
        opts.bring_up = BringUp::Spawn;
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("MCU_Spawner"), 0);
        assert!(pack.find_by_name("Activate Units").is_some());
    }

    #[test]
    fn wingmen_target_link_the_flight_lead() {
        let pack = generate_template(&four_migs()).unwrap();
        let mut planes = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                planes.push((e.property("LinkTrId").unwrap().parse::<i32>().unwrap(), e.name().unwrap().to_string()));
            }
        });
        assert_eq!(planes.len(), 4);
        let mut entity_targets = std::collections::HashMap::new();
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_TR_Entity" {
                entity_targets.insert(e.index.unwrap(), e.targets.clone());
            }
        });
        let lead_entity = planes[0].0;
        let mut wing_links = 0;
        for (eid, _) in planes.iter().skip(1) {
            let targets = entity_targets.get(eid).unwrap();
            assert_eq!(targets, &vec![lead_entity]);
            wing_links += 1;
        }
        assert_eq!(wing_links, 3);
        let lead_targets = entity_targets.get(&lead_entity).unwrap();
        assert!(lead_targets.is_empty());
    }

    #[test]
    fn cover_orders_the_lead_to_cover_another_flight() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.seats = (0..8)
            .map(|i| {
                let mut seat = TemplateSeat::new(mig.clone());
                seat.role = match i {
                    0 | 4 => FlightRole::Lead,
                    1 | 2 | 3 => FlightRole::Follows(0),
                    _ => FlightRole::Follows(4),
                };
                seat
            })
            .collect();
        opts.seats[0].orders = vec![OrderSpec {
            kind: OrderKind::Cover,
            cover_lead: Some(4),
            ..OrderSpec::default()
        }];
        opts.waypoint_count = 0;
        let pack = generate_template(&opts).unwrap();
        let cover = pack.find_by_name("Cover").unwrap();
        assert_eq!(cover.property("CoverGroup"), Some("1"));
        assert_eq!(cover.objects.len(), 1);
        assert_eq!(cover.targets.len(), 1);
        assert_ne!(cover.objects[0], cover.targets[0]);
        let mut plane_entities = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                plane_entities.push(e.property("LinkTrId").unwrap().parse::<i32>().unwrap());
            }
        });
        assert_eq!(cover.objects[0], plane_entities[0]);
        assert_eq!(cover.targets[0], plane_entities[4]);
    }

    #[test]
    fn independent_units_can_spawn() {
        let mut opts = four_migs();
        for seat in &mut opts.seats {
            seat.role = FlightRole::Independent;
        }
        opts.bring_up = BringUp::Spawn;
        opts.allow_multiple_spawns = false;
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("MCU_Spawner"), 1);
        let spawner = pack.find_by_name("Trigger Spawner").unwrap();
        assert_eq!(spawner.objects.len(), 4);
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_TR_Entity" {
                assert!(e.targets.is_empty(), "independent units are not target-linked");
            }
        });
    }

    #[test]
    fn units_sit_in_finger_four() {
        let pack = generate_template(&four_migs()).unwrap();
        let mut xz = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                xz.push(e.pos_xz().unwrap());
            }
        });
        assert_eq!(xz.len(), 4);
        let lead = xz
            .iter()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .unwrap();
        assert!((lead.0 - ORIGIN_X).abs() < 1.0);
        assert!((lead.1 - ORIGIN_Z).abs() < 1.0);
    }

    #[test]
    fn air_formation_presets_match_export() {
        assert_eq!(AIR_FORMATIONS[0].id, 19);
        assert_eq!(AIR_FORMATIONS[0].label, "Pairs");
        assert_eq!(formation_label(23, UnitKind::Plane), "Heavy Wedge");
        assert_eq!(formation_label(4, UnitKind::Vehicle), "Road Column 1 way");
    }

    #[test]
    fn inverted_vee_matches_finger_four() {
        for i in 0..8 {
            assert_eq!(
                place_offset(PlaceLayout::InvertedVee, i, 4, 150.0),
                finger_four_offset(i, 150.0)
            );
        }
    }

    #[test]
    fn combat_box_defaults_to_six_per_group() {
        assert_eq!(PlaceLayout::CombatBox.default_per_group(), 6);
        assert_eq!(PlaceLayout::InvertedVee.default_per_group(), 4);
        assert_eq!(PlaceLayout::ALL.iter().filter(|l| **l == PlaceLayout::CombatBox).count(), 1);
    }

    #[test]
    fn combat_box_six_are_distinct_with_lead_forward() {
        let spacing = 150.0;
        let mut pts = Vec::new();
        for i in 0..6 {
            pts.push(place_offset(PlaceLayout::CombatBox, i, 6, spacing));
        }
        for i in 0..6 {
            for j in (i + 1)..6 {
                assert_ne!(pts[i], pts[j], "seats {i} and {j} overlap");
            }
        }
        let lead = pts[0];
        assert!((lead.0).abs() < 1e-9 && (lead.1).abs() < 1e-9);
        for &(x, _) in &pts[1..] {
            assert!(x < lead.0, "wingmen must sit behind the lead");
        }
        let second = place_offset(PlaceLayout::CombatBox, 6, 6, spacing);
        assert_ne!(second, lead);
        assert!(second.0 < lead.0);
    }

    #[test]
    fn units_sit_in_combat_box() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.place_layout = PlaceLayout::CombatBox;
        opts.per_group = PlaceLayout::CombatBox.default_per_group();
        opts.waypoint_count = 0;
        opts.seats = (0..6)
            .map(|i| {
                let mut seat = TemplateSeat::new(mig.clone());
                seat.number_in_formation = i as i32;
                seat.altitude = 1000.0;
                seat
            })
            .collect();
        let pack = generate_template(&opts).unwrap();
        let mut xz = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                xz.push(e.pos_xz().unwrap());
            }
        });
        assert_eq!(xz.len(), 6);
        assert_eq!(opts.per_group, 6);
        for (i, &(x, z)) in xz.iter().enumerate() {
            let (dx, dz) = place_offset(PlaceLayout::CombatBox, i, 6, 150.0);
            assert!(
                (x - (ORIGIN_X + dx)).abs() < 1.0 && (z - (ORIGIN_Z + dz)).abs() < 1.0,
                "seat {i} at ({x}, {z})"
            );
        }
    }

    #[test]
    fn attack_writes_attack_target_links() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        let mut a = TemplateSeat::new(mig.clone());
        let mut b = TemplateSeat::new(mig);
        b.country = 601;
        a.orders = vec![OrderSpec {
            kind: OrderKind::Attack,
            attack_seat: Some(1),
            attack_group: true,
            priority: 1,
            ..OrderSpec::default()
        }];
        opts.seats = vec![a, b];
        let pack = generate_template(&opts).unwrap();
        let attack = pack.find_by_name("Attack").unwrap();
        assert_eq!(attack.block_type, "MCU_CMD_AttackTarget");
        assert_eq!(attack.property("AttackGroup"), Some("1"));
        assert_eq!(attack.property("Priority"), Some("1"));
        assert_eq!(attack.objects.len(), 1);
        assert_eq!(attack.targets.len(), 1);
        assert_ne!(attack.objects[0], attack.targets[0]);
        let text = serialize_group(&pack);
        assert!(text.contains("MCU_CMD_AttackTarget"));
        assert!(!text.contains("MCU_CMD_Attack\n") && !text.contains("MCU_CMD_Attack {"));
        let mut countries = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                countries.push(e.property("Country").unwrap().to_string());
            }
        });
        assert_eq!(countries, vec!["501".to_string(), "601".to_string()]);
    }

    #[test]
    fn shared_attack_area_uses_one_mcu_for_several_units() {
        let vehicle = catalog_vehicle();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        let mut a = TemplateSeat::new(vehicle.clone());
        let b = TemplateSeat::new(vehicle.clone());
        let c = TemplateSeat::new(vehicle);
        a.orders = vec![OrderSpec {
            kind: OrderKind::AttackArea,
            attack_ground: true,
            attack_air: false,
            shared_with: vec![1, 2],
            ..OrderSpec::default()
        }];
        opts.seats = vec![a, b, c];
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("MCU_CMD_AttackArea"), 1);
        let area = pack.find_by_name("AttackArea").unwrap();
        assert_eq!(area.objects.len(), 3);
        let mut entities = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_TR_Entity" {
                entities.push(e.index.unwrap());
            }
        });
        for id in &area.objects {
            assert!(entities.contains(id), "AttackArea object {id} should be a unit entity");
        }
    }

    #[test]
    fn goto_waypoint_pulses_wp_without_cmd() {
        let mut opts = one_mig();
        opts.waypoint_count = 2;
        opts.seats[0].orders = vec![OrderSpec {
            kind: OrderKind::GotoWaypoint,
            waypoint: 2,
            ..OrderSpec::default()
        }];
        let pack = generate_template(&opts).unwrap();
        assert!(pack.find_by_name("Attack").is_none());
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let wp2 = pack.find_by_name("WP 2").unwrap();
        let goto = pack.find_by_name("Goto WP 1").unwrap();
        assert_eq!(goto.block_type, "MCU_Timer");
        assert!(goto.targets.contains(&wp2.index.unwrap()));
        assert!(!goto.targets.contains(&wp1.index.unwrap()));
        let after = pack.find_by_name("AFTER BRING UP").unwrap();
        assert!(after.targets.contains(&goto.index.unwrap()));
        assert_eq!(wp2.objects.len(), 1);
        assert!(wp1.objects.is_empty());
        assert_eq!(pack.count_block_type("MCU_CMD_AttackTarget"), 0);
    }

    #[test]
    fn timer_order_pauses_the_chain() {
        let mut opts = one_mig();
        opts.waypoint_count = 0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::Formation,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::Timer,
                time_s: 12.0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                attack_air: true,
                attack_ground: false,
                attack_g_targets: false,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let pause = pack.find_by_name("Timer 1").unwrap();
        assert_eq!(pause.block_type, "MCU_Timer");
        assert_eq!(pause.property("Time"), Some("12"));
        let form = pack.find_by_name("Formation 1").unwrap();
        let area = pack.find_by_name("AttackArea 1").unwrap();
        assert!(form.targets.contains(&pause.index.unwrap()));
        assert!(pause.targets.contains(&area.index.unwrap()));
        assert_eq!(pack.count_block_type("MCU_CMD_AttackArea"), 1);
    }

    #[test]
    fn attack_area_writes_exclusive_target_mode_and_priority() {
        let mut opts = one_mig();
        opts.waypoint_count = 0;
        opts.seats[0].orders = vec![OrderSpec {
            kind: OrderKind::AttackArea,
            attack_air: false,
            attack_ground: false,
            attack_g_targets: true,
            priority: 2,
            ..OrderSpec::default()
        }];
        let pack = generate_template(&opts).unwrap();
        let area = pack.find_by_name("AttackArea").unwrap();
        assert_eq!(area.property("AttackAir"), Some("0"));
        assert_eq!(area.property("AttackGround"), Some("0"));
        assert_eq!(area.property("AttackGTargets"), Some("1"));
        assert_eq!(area.property("Priority"), Some("2"));
    }

    #[test]
    fn cover_and_force_complete_use_order_priority() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        let mut a = TemplateSeat::new(mig.clone());
        let b = TemplateSeat::new(mig);
        a.orders = vec![
            OrderSpec {
                kind: OrderKind::Cover,
                cover_lead: Some(1),
                priority: 0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::ForceComplete,
                priority: 1,
                ..OrderSpec::default()
            },
        ];
        opts.seats = vec![a, b];
        let pack = generate_template(&opts).unwrap();
        let cover = pack.find_by_name("Cover").unwrap();
        assert_eq!(cover.property("Priority"), Some("0"));
        let force = pack.find_by_name("Force Complete").expect("seat Force Complete MCU");
        assert_eq!(force.property("Priority"), Some("1"));
    }

    #[test]
    fn order_chip_detail_summarizes_attack_area() {
        let mut order = OrderSpec {
            kind: OrderKind::AttackArea,
            attack_air: false,
            attack_ground: true,
            attack_g_targets: false,
            attack_area: 1000.0,
            time_s: 600.0,
            priority: 1,
            ..OrderSpec::default()
        };
        let text = order_chip_detail(&order, UnitKind::Plane);
        assert!(text.contains("Ground"), "{text}");
        assert!(text.contains("1km") || text.contains("1000"), "{text}");
        assert!(text.contains("Medium"), "{text}");
        order.kind = OrderKind::Timer;
        order.time_s = 12.0;
        assert_eq!(order_chip_detail(&order, UnitKind::Plane), "12s");
        order.kind = OrderKind::GotoWaypoint;
        order.waypoint = 2;
        order.altitude = 0.0;
        order.priority = 2;
        assert_eq!(order_chip_detail(&order, UnitKind::Plane), "WP 2 · High");
    }

    fn catalog_plane(script_part: &str) -> CatalogUnit {
        builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains(script_part))
            .expect("plane in builtin catalog")
    }

    fn catalog_script(script_part: &str) -> CatalogUnit {
        bundled_catalog()
            .into_iter()
            .find(|u| u.script.to_ascii_lowercase().contains(script_part))
            .expect("unit in bundled catalog")
    }

    #[test]
    fn airstart_defaults_to_1500_for_every_aircraft() {
        for script in ["mig15bis", "f86a5", "f51d", "b29", "il10"] {
            let mut seat = TemplateSeat::new(catalog_script(script));
            seat.altitude = 0.0;
            seat.start_type = PlaneStart::Running.as_i32();
            apply_plane_start(&mut seat, PlaneStart::Air);
            assert_eq!(seat.altitude, AIR_START_ALTITUDE_M, "{script}");
            assert_eq!(seat.start_type, PlaneStart::Air.as_i32(), "{script}");
        }
    }

    #[test]
    fn airstart_keeps_an_existing_height() {
        let mut seat = TemplateSeat::new(catalog_plane("mig15bis"));
        seat.altitude = 2200.0;
        seat.start_type = PlaneStart::Air.as_i32();
        apply_plane_start(&mut seat, PlaneStart::Air);
        assert_eq!(seat.altitude, 2200.0);
        assert_eq!(seat.start_type, PlaneStart::Air.as_i32());
    }

    #[test]
    fn ground_start_clears_altitude() {
        let mut seat = TemplateSeat::new(catalog_plane("f86a5"));
        seat.altitude = AIR_START_ALTITUDE_M;
        apply_plane_start(&mut seat, PlaneStart::Cold);
        assert_eq!(seat.altitude, 0.0);
        assert_eq!(seat.start_type, PlaneStart::Cold.as_i32());
    }

    #[test]
    fn plane_start_leaves_ground_units_alone() {
        let mut seat = TemplateSeat::new(catalog_vehicle());
        let start = seat.start_type;
        apply_plane_start(&mut seat, PlaneStart::Air);
        assert_eq!(seat.altitude, 0.0);
        assert_eq!(seat.start_type, start);
    }

    #[test]
    fn per_seat_altitude_and_formation_index() {
        let mut opts = four_migs();
        opts.seats[0].altitude = 1200.0;
        opts.seats[1].altitude = 1250.0;
        opts.seats[2].altitude = 1180.0;
        opts.seats[3].altitude = 1210.0;
        opts.seats[2].number_in_formation = 2;
        let pack = generate_template(&opts).unwrap();
        let mut planes = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                planes.push((
                    e.property("YPos").unwrap().to_string(),
                    e.property("NumberInFormation").unwrap().to_string(),
                ));
            }
        });
        assert_eq!(planes[0].0, "1200.000");
        assert_eq!(planes[1].0, "1250.000");
        assert_eq!(planes[0].1, "0");
        assert_eq!(planes[2].1, "2");
    }

    #[test]
    fn ground_units_have_no_altitude() {
        let cat = bundled_catalog();
        let vehicle = cat
            .iter()
            .find(|u| u.kind == UnitKind::Vehicle)
            .cloned()
            .expect("vehicle in catalog");
        let mut opts = TemplateOptions::default();
        opts.place_layout = PlaceLayout::Column;
        opts.per_group = 1;
        opts.waypoint_count = 0;
        let mut seat = TemplateSeat::new(vehicle);
        seat.altitude = 999.0;
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        pack.for_each(&mut |e| {
            if e.block_type == "Vehicle" {
                let y: f64 = e.property("YPos").unwrap().parse().unwrap();
                assert!(y.abs() < 0.01, "ground YPos should be 0, got {y}");
            }
        });
    }

    fn catalog_vehicle() -> CatalogUnit {
        bundled_catalog()
            .into_iter()
            .find(|u| u.kind == UnitKind::Vehicle)
            .expect("vehicle in catalog")
    }

    #[test]
    fn ground_vehicles_omit_plane_only_keys() {
        let vehicle = catalog_vehicle();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        opts.seats = (0..3)
            .map(|i| {
                let mut seat = TemplateSeat::new(vehicle.clone());
                seat.role = if i == 0 {
                    FlightRole::Lead
                } else {
                    FlightRole::Follows(0)
                };
                seat.number_in_formation = i as i32;
                if i == 0 {
                    seat.orders = vec![OrderSpec {
                        kind: OrderKind::Formation,
                        formation_type: 18,
                        ..OrderSpec::default()
                    }];
                }
                seat
            })
            .collect();
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("Vehicle"), 3);
        assert_eq!(pack.count_block_type("MCU_TR_Entity"), 3);
        let mut numbers = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type == "Vehicle" {
                assert!(
                    e.property("AiRTBDecision").is_none(),
                    "vehicles must not write AiRTBDecision"
                );
                assert!(
                    e.property("StartType").is_none(),
                    "vehicles must not write StartType"
                );
                assert_eq!(e.property("PinToTerrain"), Some("1"));
                numbers.push(e.property("NumberInFormation").unwrap().to_string());
            }
        });
        assert_eq!(numbers, vec!["0", "1", "2"]);
        let text = serialize_group(&pack);
        assert!(!text.contains("AiRTBDecision"));
        assert!(!text.contains("StartType"));
        let formation = pack.find_by_name("Formation").unwrap();
        assert_eq!(formation.property("FormationType"), Some("18"));
        assert_eq!(formation.property("FormationDensity"), Some("1"));
        let activate = pack.find_by_name("Activate Units").unwrap();
        assert_eq!(activate.objects.len(), 3);
    }

    #[test]
    fn reference_vehicle_column_has_no_plane_keys() {
        let root = parse_il2_document(include_str!(
            "../TemplateExamples/Simple Vehicle Formation 2 way column.Group"
        ))
        .unwrap();
        let mut vehicles = 0;
        root.for_each(&mut |e| {
            if e.block_type == "Vehicle" {
                vehicles += 1;
                assert!(e.property("AiRTBDecision").is_none());
                assert!(e.property("StartType").is_none());
                assert_eq!(e.property("PinToTerrain"), Some("1"));
            }
        });
        assert_eq!(vehicles, 5);
        let form = root.find_by_name("Command Formation").unwrap();
        assert_eq!(form.property("FormationType"), Some("18"));
        assert_eq!(form.property("FormationDensity"), Some("1"));
    }

    #[test]
    fn append_seat_follows_lead_and_numbers_formation() {
        let vehicle = catalog_vehicle();
        let mut seats = Vec::new();
        append_seat(&mut seats, vehicle.clone(), 4);
        seats[0].role = FlightRole::Lead;
        append_seat(&mut seats, vehicle.clone(), 4);
        append_seat(&mut seats, vehicle, 4);
        assert_eq!(seats[0].role, FlightRole::Lead);
        assert_eq!(seats[1].role, FlightRole::Follows(0));
        assert_eq!(seats[2].role, FlightRole::Follows(0));
        assert_eq!(seats[0].number_in_formation, 0);
        assert_eq!(seats[1].number_in_formation, 1);
        assert_eq!(seats[2].number_in_formation, 2);
    }

    #[test]
    fn copy_seat_attributes_clones_flags_not_role() {
        let vehicle = catalog_vehicle();
        let mut seats = vec![
            TemplateSeat::new(vehicle.clone()),
            TemplateSeat::new(vehicle),
        ];
        seats[0].role = FlightRole::Lead;
        seats[0].country = 601;
        seats[0].skill = 4;
        seats[0].vulnerable = false;
        seats[1].role = FlightRole::Follows(0);
        seats[1].country = 501;
        copy_seat_attributes(&mut seats, 0);
        assert_eq!(seats[1].country, 601);
        assert_eq!(seats[1].skill, 4);
        assert!(!seats[1].vulnerable);
        assert_eq!(seats[1].role, FlightRole::Follows(0));
        assert_eq!(seats[0].role, FlightRole::Lead);
    }

    #[test]
    fn copy_seat_attributes_caps_altitude_at_each_ceiling() {
        let mut seats = vec![
            TemplateSeat::new(catalog_plane("f86a5")),
            TemplateSeat::new(catalog_script("il10")),
            TemplateSeat::new(catalog_plane("mig15bis")),
        ];
        seats[0].altitude = 12_000.0;
        copy_seat_attributes(&mut seats, 0);
        assert_eq!(seats[1].altitude, 6950.0, "IL-10 capped at its ceiling");
        assert_eq!(seats[2].altitude, 12_000.0, "MiG ceiling is above 12000 m");
        assert_eq!(seats[1].start_type, PlaneStart::Air.as_i32());
    }

    #[test]
    fn copy_and_append_inherit_payload_mods_on_same_type() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let sabre = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("f86a5"))
            .unwrap();
        let mut seats = vec![
            TemplateSeat::new(mig.clone()),
            TemplateSeat::new(mig.clone()),
            TemplateSeat::new(sabre.clone()),
        ];
        seats[0].payload_id = 2;
        seats[0].mod_mask = "11".into();
        seats[0].country = 601;
        copy_seat_attributes(&mut seats, 0);
        assert_eq!(seats[1].payload_id, 2);
        assert_eq!(seats[1].mod_mask, "11");
        assert_eq!(seats[1].country, 601);
        assert_eq!(seats[2].payload_id, 0, "different type keeps its payload");
        assert_eq!(seats[2].mod_mask, "1");
        assert_eq!(seats[2].country, 601);

        append_seat(&mut seats, mig, 4);
        let last = seats.last().unwrap();
        assert_eq!(last.payload_id, 2);
        assert_eq!(last.mod_mask, "11");
        append_seat(&mut seats, sabre, 4);
        let last = seats.last().unwrap();
        assert_eq!(last.payload_id, 0);
        assert_eq!(last.mod_mask, "1");
    }

    #[test]
    fn generated_plane_writes_payload_and_mod_mask() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        let mut seat = TemplateSeat::new(mig);
        seat.payload_id = 2;
        seat.mod_mask = "10101".into();
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        let mut seen = false;
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                assert_eq!(e.property("PayloadId"), Some("2"));
                assert_eq!(e.property("ModMask"), Some("10101"));
                seen = true;
            }
        });
        assert!(seen);
        let text = crate::serialize::serialize_group(&pack);
        assert!(text.contains("PayloadId = 2;"));
        assert!(text.contains("ModMask = 10101;"));
    }

    #[test]
    fn move_seat_swaps_and_remaps_follows() {
        let vehicle = catalog_vehicle();
        let mut seats = Vec::new();
        append_seat(&mut seats, vehicle.clone(), 4);
        seats[0].role = FlightRole::Lead;
        append_seat(&mut seats, vehicle.clone(), 4);
        append_seat(&mut seats, vehicle, 4);
        seats[0].orders.push(OrderSpec {
            kind: OrderKind::Attack,
            attack_seat: Some(1),
            shared_with: vec![2],
            ..OrderSpec::default()
        });
        let dest = move_seat(&mut seats, 0, 1).unwrap();
        assert_eq!(dest, 1);
        assert_eq!(seats[1].role, FlightRole::Lead);
        assert_eq!(seats[0].role, FlightRole::Follows(1));
        assert_eq!(seats[2].role, FlightRole::Follows(1));
        assert_eq!(seats[1].orders[0].attack_seat, Some(0));
        assert_eq!(seats[1].orders[0].shared_with, vec![2]);
    }

    #[test]
    fn for_unit_sets_attack_area_from_mg_range() {
        let mut unit = catalog_vehicle();
        unit.script = "fixedobjects\\squad-mg-1950-dprk.txt".into();
        let spec = OrderSpec::for_unit(&unit);
        assert_eq!(spec.kind, OrderKind::AttackArea);
        assert!((spec.attack_area - 1000.0).abs() < 0.5);
        let mut seats = vec![TemplateSeat::new(unit)];
        seats[0].orders.push(OrderSpec::for_kind(UnitKind::Vehicle));
        assert!((seats[0].orders[0].attack_area - 3000.0).abs() < 0.5);
        apply_suggested_attack_area(&mut seats, 0, 0);
        assert!((seats[0].orders[0].attack_area - 1000.0).abs() < 0.5);
    }

    #[test]
    fn rtb_on_zone_out_is_optional_and_per_coalition() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut east = TemplateSeat::new(mig.clone());
        east.country = 501;
        east.orders = vec![OrderSpec {
            kind: OrderKind::RtbOnZoneOut,
            ..OrderSpec::default()
        }];
        let mut west = TemplateSeat::new(mig);
        west.country = 601;
        west.orders = vec![OrderSpec {
            kind: OrderKind::RtbOnZoneOut,
            ..OrderSpec::default()
        }];
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        opts.seats = vec![east, west];
        let pack = generate_template(&opts).unwrap();
        assert!(pack.find_by_name("RTB East 1").is_some());
        assert!(pack.find_by_name("RTB West 1").is_some());
        assert!(pack.find_by_name("RTB DELAY").is_some());
        assert!(pack.find_by_name("RTB").is_none());
        let delayed = pack.find_by_name("DELAYED END ORDERS").unwrap();
        assert_eq!(delayed.property("Time"), Some("60"));
        let end_orders = pack.find_by_name("MISSION END ORDERS").unwrap();
        let rtb_wait = pack.find_by_name("RTB DELAY").unwrap();
        assert!(end_orders.targets.contains(&rtb_wait.index.unwrap()));
        assert!(!end_orders.targets.contains(&pack.find_by_name("RTB East 1").unwrap().index.unwrap()));
        let delete = pack.find_by_name("Trigger Delete").unwrap();
        assert_eq!(delete.objects.len(), 2);
    }

    #[test]
    fn checkzone_coalition_is_explicit() {
        let mut opts = one_mig();
        opts.zone_coalition = ZoneCoalition::Both;
        let pack = generate_template(&opts).unwrap();
        assert_eq!(
            pack.find_by_name("Zone IN").unwrap().property("PlaneCoalitions"),
            Some("[1, 2]")
        );
        assert_eq!(
            pack.find_by_name("Zone Out").unwrap().property("PlaneCoalitions"),
            Some("[1, 2]")
        );
    }

    #[test]
    fn logic_mcus_sit_on_the_requested_grid() {
        let pack = generate_template(&four_migs()).unwrap();
        let zone = pack.find_by_name("Zone IN").unwrap().pos_xz().unwrap();
        assert!((zone.0 - ORIGIN_X).abs() < 0.1);
        assert!((zone.1 - ORIGIN_Z).abs() < 0.1);
        let begin = pack.find_by_name("Translator Mission Begin").unwrap().pos_xz().unwrap();
        assert!((begin.0 - (ORIGIN_X + 150.0)).abs() < 0.1);
        assert!((begin.1 - ORIGIN_Z).abs() < 0.1);
        let pulse = pack.find_by_name("ENABLE / PULSE IN").unwrap().pos_xz().unwrap();
        assert!((pulse.0 - ORIGIN_X).abs() < 0.1);
        assert!((pulse.1 - ORIGIN_Z).abs() < 0.1);
        let deact = pack.find_by_name("Zone Out ReActivate").unwrap().pos_xz().unwrap();
        assert!((deact.0 - ORIGIN_X).abs() < 0.1);
        assert!((deact.1 - ORIGIN_Z).abs() < 0.1);
        let bring = pack.find_by_name("MISSION BEGIN").unwrap().pos_xz().unwrap();
        assert!((bring.1 - (ORIGIN_Z - 150.0)).abs() < 0.1);
        let activate = pack.find_by_name("Activate Units").unwrap().pos_xz().unwrap();
        assert!((activate.1 - (ORIGIN_Z - 300.0)).abs() < 0.1);
        let force = pack.find_by_name("Force Complete - High").unwrap().pos_xz().unwrap();
        assert!((force.1 - (ORIGIN_Z + 300.0)).abs() < 0.1);
        let delete = pack.find_by_name("Trigger Delete").unwrap().pos_xz().unwrap();
        assert!((delete.1 - (ORIGIN_Z + 300.0)).abs() < 0.1);
        let form = pack.find_by_name("Formation").unwrap().pos_xz().unwrap();
        let form_tm = pack.find_by_name("Formation 1").unwrap().pos_xz().unwrap();
        assert!((form.1 - (form_tm.1 - 150.0)).abs() < 0.1);
        let cooldown = pack.find_by_name("COOLDOWN").unwrap();
        assert_eq!(cooldown.property("Time"), Some("0"));
        let cool_xz = cooldown.pos_xz().unwrap();
        assert!((cool_xz.0 - ORIGIN_X).abs() < 0.1);
        assert!((cool_xz.1 - ORIGIN_Z).abs() < 0.1);
    }

    fn entity_reports(pack: &Il2Entity) -> Vec<(i32, i32, i32)> {
        let mut out = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type != "MCU_TR_Entity" {
                return;
            }
            for wrap in &e.children {
                if wrap.block_type != "OnReports" {
                    continue;
                }
                for r in &wrap.children {
                    if r.block_type != "OnReport" {
                        continue;
                    }
                    let t: i32 = r.property("Type").unwrap().parse().unwrap();
                    let c: i32 = r.property("CmdId").unwrap().parse().unwrap();
                    let tar: i32 = r.property("TarId").unwrap().parse().unwrap();
                    out.push((t, c, tar));
                }
            }
        });
        out
    }

    fn entity_events(pack: &Il2Entity) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        pack.for_each(&mut |e| {
            if e.block_type != "MCU_TR_Entity" {
                return;
            }
            for wrap in &e.children {
                if wrap.block_type != "OnEvents" {
                    continue;
                }
                for ev in &wrap.children {
                    if ev.block_type != "OnEvent" {
                        continue;
                    }
                    let t: i32 = ev.property("Type").unwrap().parse().unwrap();
                    let tar: i32 = ev.property("TarId").unwrap().parse().unwrap();
                    out.push((t, tar));
                }
            }
        });
        out
    }

    #[test]
    fn on_spawned_report_uses_spawner_and_starts_next_order() {
        let mut opts = one_mig();
        opts.bring_up = BringUp::Spawn;
        opts.waypoint_count = 0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::Formation,
                formation_type: 23,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let spawner = pack.find_by_name("Trigger Spawner").unwrap();
        let timer = pack.find_by_name("OnSpawned 1").unwrap();
        let form_delay = pack.find_by_name("Formation 1").unwrap();
        let after = pack.find_by_name("AFTER BRING UP").unwrap();
        assert!(!after.targets.contains(&timer.index.unwrap()));
        assert!(timer.targets.contains(&form_delay.index.unwrap()));
        let reports = entity_reports(&pack);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, 0);
        assert_eq!(reports[0].1, spawner.index.unwrap());
        assert_eq!(reports[0].2, timer.index.unwrap());
        assert!(entity_events(&pack).is_empty());
    }

    #[test]
    fn on_target_attacked_waits_for_attack_not_timer_chain() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 2;
        let mut a = TemplateSeat::new(mig.clone());
        let b = TemplateSeat::new(mig);
        a.orders = vec![
            OrderSpec {
                kind: OrderKind::Attack,
                attack_seat: Some(1),
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnTargetAttacked,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
        ];
        opts.seats = vec![a, b];
        let pack = generate_template(&opts).unwrap();
        let attack = pack.find_by_name("Attack").unwrap();
        let atk_delay = pack.find_by_name("Attack 1").unwrap();
        let report_tm = pack.find_by_name("OnTargetAttacked 1").unwrap();
        let goto = pack.find_by_name("Goto WP 1").unwrap();
        assert!(!atk_delay.targets.contains(&report_tm.index.unwrap()));
        assert!(atk_delay.targets.contains(&attack.index.unwrap()));
        assert!(report_tm.targets.contains(&goto.index.unwrap()));
        let reports = entity_reports(&pack);
        assert!(
            reports.iter().any(|(t, c, tar)| {
                *t == 1 && *c == attack.index.unwrap() && *tar == report_tm.index.unwrap()
            }),
            "expected OnTargetAttacked report, got {reports:?}"
        );
    }

    #[test]
    fn vehicle_on_killed_event_targets_force_complete() {
        let vehicle = catalog_vehicle();
        let mut opts = TemplateOptions::default();
        opts.waypoint_count = 0;
        let mut seat = TemplateSeat::new(vehicle);
        seat.events = vec![EventHook {
            kind: EntityEvent::OnKilled,
            then: EventThen::ForceComplete,
        }];
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        let force = pack.find_by_name("Force Complete - High").unwrap();
        let events = entity_events(&pack);
        assert_eq!(events, vec![(13, force.index.unwrap())]);
    }

    #[test]
    fn aircraft_events_append_and_on_took_off_report() {
        let mut opts = one_mig();
        opts.bring_up = BringUp::Activate;
        opts.waypoint_count = 0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::TakeOff,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnTookOff,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::Formation,
                formation_type: 23,
                ..OrderSpec::default()
            },
        ];
        opts.seats[0].events = vec![
            EventHook {
                kind: EntityEvent::OnPlaneDestroyed,
                then: EventThen::ForceComplete,
            },
            EventHook {
                kind: EntityEvent::OnPilotKilled,
                then: EventThen::Order(2),
            },
        ];
        let pack = generate_template(&opts).unwrap();
        assert_eq!(pack.count_block_type("MCU_CMD_TakeOff"), 1);
        let takeoff = pack.find_by_name("Take Off").unwrap();
        let took_tm = pack.find_by_name("OnTookOff 1").unwrap();
        let reports = entity_reports(&pack);
        assert!(
            reports.iter().any(|(t, c, tar)| {
                *t == 3 && *c == takeoff.index.unwrap() && *tar == took_tm.index.unwrap()
            })
        );
        let force = pack.find_by_name("Force Complete - High").unwrap();
        let form_tm = pack.find_by_name("Formation 1").unwrap();
        let events = entity_events(&pack);
        assert!(events.contains(&(4, force.index.unwrap())));
        assert!(events.contains(&(0, form_tm.index.unwrap())));
    }

    #[test]
    fn normalize_puts_spawned_first_and_attack_report_after_attack() {
        let mut orders = vec![
            OrderSpec {
                kind: OrderKind::Formation,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnTargetAttacked,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::Attack,
                ..OrderSpec::default()
            },
        ];
        let mut events = vec![EventHook {
            kind: EntityEvent::OnKilled,
            then: EventThen::Order(3),
        }];
        let keep = normalize_order_chain(&mut orders, &mut events, 3);
        assert_eq!(orders[0].kind, OrderKind::OnSpawned);
        assert_eq!(orders[1].kind, OrderKind::Formation);
        assert_eq!(orders[2].kind, OrderKind::Attack);
        assert_eq!(orders[3].kind, OrderKind::OnTargetAttacked);
        assert_eq!(keep, 2);
        assert_eq!(events[0].then, EventThen::Order(2));
    }

    #[test]
    fn ground_events_include_trailer_and_radar() {
        let ids: Vec<_> = EntityEvent::available(UnitKind::Vehicle)
            .iter()
            .map(|e| e.type_id())
            .collect();
        assert!(ids.contains(&12));
        assert!(ids.contains(&13));
        assert!(ids.contains(&74));
        assert!(ids.contains(&80));
        assert!(ids.contains(&85));
        assert!(!EntityEvent::available(UnitKind::Plane)
            .iter()
            .any(|e| *e == EntityEvent::OnTrailerKilled));
        assert!(OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::OnSpawned));
        assert!(!OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::OnTookOff));
        assert!(OrderKind::available(UnitKind::Plane).contains(&OrderKind::OnTookOff));
        assert!(OrderKind::available(UnitKind::Plane).contains(&OrderKind::TakeOff));
        assert!(OrderKind::available(UnitKind::Plane).contains(&OrderKind::TimeOnTarget));
        assert!(OrderKind::Attack.has_priority());
        assert!(OrderKind::AttackArea.has_priority());
        assert!(OrderKind::Cover.has_priority());
        assert!(OrderKind::ForceComplete.has_priority());
        assert!(OrderKind::Land.has_priority());
        assert!(OrderKind::GotoWaypoint.has_priority());
        assert!(!OrderKind::Timer.has_priority());
        assert!(OrderKind::available(UnitKind::Plane).contains(&OrderKind::MissionComplete));
        assert!(!OrderKind::available(UnitKind::Plane).contains(&OrderKind::RtbOnZoneOut));
        assert!(!OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::RtbOnZoneOut));
        assert!(OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::TimeOnTarget));
        assert!(OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::Timer));
        assert!(OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::MissionComplete));
    }

    #[test]
    fn path_waypoints_use_300m_area_for_planes_and_attack_area_defaults_to_3000() {
        let mut opts = four_migs();
        opts.seats[0].orders.insert(
            0,
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
        );
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        assert_eq!(wp1.property("Area"), Some("300"));
        let area = pack.find_by_name("AttackArea").unwrap();
        assert_eq!(area.property("AttackArea"), Some("3000"));
    }

    #[test]
    fn ground_path_waypoints_use_100m_area() {
        let vehicle = catalog_vehicle();
        let mut seat = TemplateSeat::new(vehicle);
        seat.orders = vec![OrderSpec {
            kind: OrderKind::GotoWaypoint,
            waypoint: 1,
            ..OrderSpec::default()
        }];
        let mut opts = TemplateOptions::default();
        opts.seats = vec![seat];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        assert_eq!(wp1.property("Area"), Some("100"));
        assert_eq!(wp1.property("YPos"), Some("0.000"));
        assert_eq!(wp1.property("Priority"), Some("1"));
    }

    #[test]
    fn ground_waypoint_speed_and_priority_are_unclamped() {
        let vehicle = catalog_vehicle();
        let mut seat = TemplateSeat::new(vehicle);
        seat.orders = vec![OrderSpec {
            kind: OrderKind::GotoWaypoint,
            waypoint: 1,
            priority: 0,
            ..OrderSpec::default()
        }];
        let mut opts = TemplateOptions::default();
        opts.seats = vec![seat];
        opts.waypoint_speed = 5.0;
        opts.waypoint_altitude = 0.0;
        opts.waypoint_priority = 2;
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        assert_eq!(wp1.property("Speed"), Some("5"));
        assert_eq!(wp1.property("Priority"), Some("0"));
        assert_eq!(wp1.property("YPos"), Some("0.000"));
        assert_eq!(
            path_waypoint_display_m(&opts.seats, 0.0),
            0.0,
            "ground-only waypoints display 0 m, not a plane fallback"
        );
    }

    #[test]
    fn airborne_plane_writes_airstart_even_if_ground_start_was_selected() {
        let mut opts = one_mig();
        opts.seats[0].altitude = 2500.0;
        opts.seats[0].start_type = PlaneStart::Cold.as_i32();
        let pack = generate_template(&opts).unwrap();
        pack.for_each(&mut |e| {
            if e.block_type == "Plane" {
                assert_eq!(e.property("StartType"), Some("0"));
                assert_eq!(e.property("YPos").unwrap(), "2500.000");
            }
        });
    }

    #[test]
    fn ground_plane_writes_cold_warm_or_running_start() {
        for (start, expected) in [
            (PlaneStart::Running, "1"),
            (PlaneStart::Cold, "2"),
            (PlaneStart::Warm, "3"),
            (PlaneStart::Air, "1"),
        ] {
            let mut opts = one_mig();
            opts.seats[0].altitude = 0.0;
            opts.seats[0].start_type = start.as_i32();
            let pack = generate_template(&opts).unwrap();
            pack.for_each(&mut |e| {
                if e.block_type == "Plane" {
                    assert_eq!(
                        e.property("StartType"),
                        Some(expected),
                        "start {start:?} should write {expected}"
                    );
                    assert_eq!(e.property("YPos").unwrap(), "0.000");
                }
            });
        }
    }

    #[test]
    fn waypoint_target_links_attack_delay_not_mcu() {
        let mut opts = one_mig();
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                attack_ground: true,
                attack_air: false,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let attack = pack.find_by_name("AttackArea").unwrap();
        let goto = pack.find_by_name("Goto WP 1").unwrap();
        let atk_delay = pack.find_by_name("AttackArea 1").unwrap();
        assert!(wp1.targets.contains(&atk_delay.index.unwrap()));
        assert!(!wp1.targets.contains(&attack.index.unwrap()));
        assert!(atk_delay.targets.contains(&attack.index.unwrap()));
        assert!(goto.targets.contains(&wp1.index.unwrap()));
        assert!(!goto.targets.contains(&atk_delay.index.unwrap()));
        assert!(pack.find_by_name("WP 2").is_none());
        assert!(pack.find_by_name("WP DELAY").is_none());
    }

    #[test]
    fn consecutive_gotos_link_via_next_delay_not_wp_mcu() {
        let mut opts = one_mig();
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 2,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let wp2 = pack.find_by_name("WP 2").unwrap();
        let goto1 = pack.find_by_name("Goto WP 1").unwrap();
        assert!(goto1.targets.contains(&wp1.index.unwrap()));
        assert!(!goto1.targets.contains(&wp2.index.unwrap()));
        assert!(!wp1.targets.contains(&wp2.index.unwrap()));
        let wp2_id = wp2.index.unwrap();
        let mut hop = None;
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_Timer" && e.targets.contains(&wp2_id) {
                hop = e.index;
            }
        });
        let hop = hop.expect("second Goto WP delay should pulse WP 2");
        assert_ne!(hop, goto1.index.unwrap());
        assert!(wp1.targets.contains(&hop));
    }

    #[test]
    fn ground_attack_area_mcu_sits_on_group_origin() {
        let mut opts = one_mig();
        opts.waypoint_count = 0;
        opts.seats[0].unit = catalog_vehicle();
        opts.seats[0].orders = vec![OrderSpec {
            kind: OrderKind::AttackArea,
            attack_ground: true,
            attack_air: false,
            attack_g_targets: true,
            ..OrderSpec::default()
        }];
        let pack = generate_template(&opts).unwrap();
        let area = pack.find_by_name("AttackArea").unwrap();
        let p = area.pos_xz().unwrap();
        assert!((p.0 - ORIGIN_X).abs() < 0.1);
        assert!((p.1 - ORIGIN_Z).abs() < 0.1);
    }

    #[test]
    fn time_on_target_is_pulsed_from_wp_then_continues_chain() {
        let mut opts = one_mig();
        opts.waypoint_count = 3;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 2,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                attack_ground: true,
                attack_air: false,
                attack_g_targets: true,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                time_s: DEFAULT_TIME_ON_TARGET_S,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 3,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp2 = pack.find_by_name("WP 2").unwrap();
        let wp3 = pack.find_by_name("WP 3").unwrap();
        let attack = pack.find_by_name("AttackArea").unwrap();
        let atk_delay = pack.find_by_name("AttackArea 1").unwrap();
        let tot = pack.find_by_name("Time on Target 1").unwrap();
        let goto2 = pack.find_by_name("Goto WP 1").unwrap();
        assert_eq!(tot.property("Time"), Some("180"));
        assert!(wp2.targets.contains(&atk_delay.index.unwrap()));
        assert!(!wp2.targets.contains(&attack.index.unwrap()));
        assert!(atk_delay.targets.contains(&attack.index.unwrap()));
        assert!(wp2.targets.contains(&tot.index.unwrap()));
        assert!(
            !wp2.targets.contains(&wp3.index.unwrap()),
            "TOT hop must not auto-link the next WP, got {:?}",
            wp2.targets
        );
        assert!(!goto2.targets.contains(&tot.index.unwrap()));
        let tot_targets = tot.targets.clone();
        let wp3_id = wp3.index.unwrap();
        let mut next_pulses_wp3 = false;
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_Timer" && tot_targets.contains(&e.index.unwrap()) {
                next_pulses_wp3 |= e.targets.contains(&wp3_id);
            }
        });
        assert!(next_pulses_wp3, "TOT should pulse the next Goto WP timer");
    }

    #[test]
    fn mission_complete_in_chain_pulses_mission_end() {
        let mut opts = one_mig();
        opts.waypoint_count = 1;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                time_s: 60.0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::MissionComplete,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let tot = pack.find_by_name("Time on Target 1").unwrap();
        let done = pack.find_by_name("Mission Complete 1").unwrap();
        let hub = pack.find_by_name("MISSION END").unwrap();
        assert!(wp1.targets.contains(&tot.index.unwrap()));
        assert!(tot.targets.contains(&done.index.unwrap()));
        assert!(done.targets.contains(&hub.index.unwrap()));
        assert!(pack.find_by_name("WP DELAY").is_none());
    }

    #[test]
    fn time_on_target_before_attack_is_parallel_from_wp() {
        let mut opts = one_mig();
        opts.waypoint_count = 2;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                time_s: DEFAULT_TIME_ON_TARGET_S,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                attack_ground: true,
                attack_air: false,
                attack_g_targets: true,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 2,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let wp2 = pack.find_by_name("WP 2").unwrap();
        let atk_delay = pack.find_by_name("AttackArea 1").unwrap();
        let tot = pack.find_by_name("Time on Target 1").unwrap();
        let goto1 = pack.find_by_name("Goto WP 1").unwrap();
        assert!(wp1.targets.contains(&atk_delay.index.unwrap()));
        assert!(wp1.targets.contains(&tot.index.unwrap()));
        assert!(!tot.targets.contains(&atk_delay.index.unwrap()));
        assert!(!goto1.targets.contains(&tot.index.unwrap()));
        assert!(!goto1.targets.contains(&atk_delay.index.unwrap()));
        let tot_targets = tot.targets.clone();
        let wp2_id = wp2.index.unwrap();
        let mut next_pulses_wp2 = false;
        pack.for_each(&mut |e| {
            if e.block_type == "MCU_Timer" && tot_targets.contains(&e.index.unwrap()) {
                next_pulses_wp2 |= e.targets.contains(&wp2_id);
            }
        });
        assert!(
            next_pulses_wp2,
            "TOT should skip the sibling attack and pulse the next Goto WP timer"
        );
    }

    #[test]
    fn mission_complete_after_goto_is_pulsed_from_waypoint() {
        let mut opts = one_mig();
        opts.waypoint_count = 1;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::MissionComplete,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let goto = pack.find_by_name("Goto WP 1").unwrap();
        let done = pack.find_by_name("Mission Complete 1").unwrap();
        let hub = pack.find_by_name("MISSION END").unwrap();
        assert!(goto.targets.contains(&wp1.index.unwrap()));
        assert!(
            !goto.targets.contains(&done.index.unwrap()),
            "Goto timer must not start Mission Complete before arrival, got {:?}",
            goto.targets
        );
        assert!(wp1.targets.contains(&done.index.unwrap()));
        assert!(done.targets.contains(&hub.index.unwrap()));
    }

    #[test]
    fn events_break_attack_area_chain_to_mission_complete() {
        let mut opts = one_mig();
        opts.bring_up = BringUp::Spawn;
        opts.waypoint_count = 0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                attack_air: true,
                attack_ground: false,
                attack_g_targets: false,
                attack_area: 18_000.0,
                time_s: 600.0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::MissionComplete,
                ..OrderSpec::default()
            },
        ];
        opts.seats[0].events = vec![
            EventHook {
                kind: EntityEvent::OnPlaneCriticalDamage,
                then: EventThen::Order(2),
            },
            EventHook {
                kind: EntityEvent::OnPilotWounded,
                then: EventThen::Order(2),
            },
            EventHook {
                kind: EntityEvent::OnPlaneBingoMainMG,
                then: EventThen::Order(2),
            },
            EventHook {
                kind: EntityEvent::OnPlaneBingoFuel,
                then: EventThen::Order(2),
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let spawn_tm = pack.find_by_name("OnSpawned 1").unwrap();
        let atk_delay = pack.find_by_name("AttackArea 1").unwrap();
        let attack = pack.find_by_name("AttackArea").unwrap();
        let done = pack.find_by_name("Mission Complete 1").unwrap();
        let hub = pack.find_by_name("MISSION END").unwrap();
        assert!(spawn_tm.targets.contains(&atk_delay.index.unwrap()));
        assert!(atk_delay.targets.contains(&attack.index.unwrap()));
        assert!(
            !atk_delay.targets.contains(&done.index.unwrap()),
            "AttackArea timer must not start Mission Complete when events Then it, got {:?}",
            atk_delay.targets
        );
        assert!(done.targets.contains(&hub.index.unwrap()));
        let events = entity_events(&pack);
        let done_id = done.index.unwrap();
        for ty in [3, 1, 8, 7] {
            assert!(
                events.iter().any(|(t, tar)| *t == ty && *tar == done_id),
                "expected OnEvent type {ty} → Mission Complete, got {events:?}"
            );
        }
        let loaded = load_template(&pack, &builtin_plane_catalog()).unwrap();
        let kinds: Vec<_> = loaded.options.seats[0].orders.iter().map(|o| o.kind).collect();
        assert_eq!(
            kinds,
            vec![
                OrderKind::OnSpawned,
                OrderKind::AttackArea,
                OrderKind::MissionComplete,
            ]
        );
        assert!(loaded.options.seats[0].events.iter().all(|e| {
            e.then == EventThen::Order(2)
        }));
    }

    #[test]
    fn order_tree_stacks_tot_with_attack() {
        let orders = vec![
            OrderSpec {
                kind: OrderKind::Formation,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::MissionComplete,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_columns(&orders),
            vec![vec![0], vec![1], vec![2, 3], vec![4]]
        );
        let after_attack = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_columns(&after_attack),
            vec![vec![0], vec![1, 2]]
        );
        let events = vec![
            EventHook {
                kind: EntityEvent::OnPlaneBingoBombs,
                then: EventThen::Order(1),
            },
            EventHook {
                kind: EntityEvent::OnKilled,
                then: EventThen::ForceComplete,
            },
        ];
        assert_eq!(
            order_tree_layout(&after_attack, &events),
            vec![
                vec![OrderTreeNode::Event(0), OrderTreeNode::Order(0)],
                vec![OrderTreeNode::Order(1), OrderTreeNode::Order(2)],
                vec![OrderTreeNode::Event(1)],
            ]
        );
        let after_tot_then_wp = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 2,
                ..OrderSpec::default()
            },
        ];
        let bingo_next = vec![EventHook {
            kind: EntityEvent::OnPlaneBingoBombs,
            then: EventThen::Order(3),
        }];
        assert_eq!(
            order_tree_layout(&after_tot_then_wp, &bingo_next),
            vec![
                vec![OrderTreeNode::Order(0)],
                vec![
                    OrderTreeNode::Event(0),
                    OrderTreeNode::Order(1),
                    OrderTreeNode::Order(2),
                ],
                vec![OrderTreeNode::Order(3)],
            ]
        );
        let takeoff_then = vec![
            OrderSpec {
                kind: OrderKind::TakeOff,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::Formation,
                ..OrderSpec::default()
            },
        ];
        let took_off = vec![EventHook {
            kind: EntityEvent::OnPlaneTookOff,
            then: EventThen::Order(1),
        }];
        assert_eq!(
            order_tree_layout(&takeoff_then, &took_off),
            vec![
                vec![OrderTreeNode::Event(0), OrderTreeNode::Order(0)],
                vec![OrderTreeNode::Order(1)],
            ]
        );
        let two_events = vec![
            EventHook {
                kind: EntityEvent::OnPlaneBingoBombs,
                then: EventThen::Order(3),
            },
            EventHook {
                kind: EntityEvent::OnPlaneCriticalDamage,
                then: EventThen::Order(3),
            },
        ];
        assert_eq!(
            order_tree_layout(&after_tot_then_wp, &two_events),
            vec![
                vec![OrderTreeNode::Order(0)],
                vec![
                    OrderTreeNode::Event(1),
                    OrderTreeNode::Event(0),
                    OrderTreeNode::Order(1),
                    OrderTreeNode::Order(2),
                ],
                vec![OrderTreeNode::Order(3)],
            ]
        );
    }

    #[test]
    fn order_tree_stacks_reports_above_commands() {
        let spawned_then_form = vec![
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::Formation,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_columns(&spawned_then_form),
            vec![vec![0], vec![1]]
        );
        assert_eq!(
            order_tree_layout(&spawned_then_form, &[]),
            vec![
                vec![OrderTreeNode::Order(0)],
                vec![OrderTreeNode::Order(1)],
            ]
        );
        let takeoff_report = vec![
            OrderSpec {
                kind: OrderKind::TakeOff,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnTookOff,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_layout(&takeoff_report, &[]),
            vec![vec![OrderTreeNode::Order(1), OrderTreeNode::Order(0)]]
        );
        let attack_report = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnAreaAttacked,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::TimeOnTarget,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_layout(&attack_report, &[]),
            vec![
                vec![OrderTreeNode::Order(0)],
                vec![
                    OrderTreeNode::Order(2),
                    OrderTreeNode::Order(1),
                    OrderTreeNode::Order(3),
                ],
            ]
        );
        let spawned_and_area = vec![
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::OnAreaAttacked,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_layout(&spawned_and_area, &[]),
            vec![
                vec![OrderTreeNode::Order(0)],
                vec![OrderTreeNode::Order(2), OrderTreeNode::Order(1)],
            ]
        );
        let bingo = vec![EventHook {
            kind: EntityEvent::OnPlaneBingoBombs,
            then: EventThen::Order(1),
        }];
        assert_eq!(
            order_tree_layout(&attack_report, &bingo),
            vec![
                vec![OrderTreeNode::Event(0), OrderTreeNode::Order(0)],
                vec![
                    OrderTreeNode::Order(2),
                    OrderTreeNode::Order(1),
                    OrderTreeNode::Order(3),
                ],
            ]
        );
    }

    #[test]
    fn order_tree_puts_onspawned_left_and_events_on_mission_complete() {
        let orders = vec![
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::MissionComplete,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            order_tree_columns(&orders),
            vec![vec![0], vec![1], vec![2]]
        );
        let events = vec![
            EventHook {
                kind: EntityEvent::OnPlaneCriticalDamage,
                then: EventThen::Order(2),
            },
            EventHook {
                kind: EntityEvent::OnPilotWounded,
                then: EventThen::Order(2),
            },
        ];
        assert_eq!(
            order_tree_layout(&orders, &events),
            vec![
                vec![OrderTreeNode::Order(0)],
                vec![
                    OrderTreeNode::Event(0),
                    OrderTreeNode::Event(1),
                    OrderTreeNode::Order(1),
                ],
                vec![OrderTreeNode::Order(2)],
            ]
        );
        assert!(event_triggers_order(&events, 2));
        assert!(!event_triggers_order(&events, 1));
    }

    #[test]
    fn waypoint_uses_template_and_hop_altitude() {
        let mut opts = one_mig();
        opts.waypoint_altitude = 2500.0;
        opts.seats[0].altitude = 1000.0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                altitude: 0.0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 2,
                altitude: 3200.0,
                ..OrderSpec::default()
            },
        ];
        let pack = generate_template(&opts).unwrap();
        let wp1 = pack.find_by_name("WP 1").unwrap();
        let wp2 = pack.find_by_name("WP 2").unwrap();
        assert_eq!(wp1.property("YPos").unwrap(), "2500.000");
        assert_eq!(wp2.property("YPos").unwrap(), "3200.000");
    }

    #[test]
    fn waypoint_display_follows_plane_until_overridden() {
        let mut opts = one_mig();
        opts.waypoint_altitude = 0.0;
        opts.seats[0].altitude = 4200.0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                altitude: 0.0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 2,
                altitude: 1800.0,
                ..OrderSpec::default()
            },
        ];
        assert_eq!(
            path_waypoint_display_m(&opts.seats, opts.waypoint_altitude),
            4200.0
        );
        assert_eq!(
            waypoint_display_altitude(&opts.seats, 1, opts.waypoint_altitude),
            4200.0
        );
        assert_eq!(
            waypoint_display_altitude(&opts.seats, 2, opts.waypoint_altitude),
            1800.0
        );
        opts.waypoint_altitude = 900.0;
        assert_eq!(
            waypoint_display_altitude(&opts.seats, 1, opts.waypoint_altitude),
            900.0
        );
        assert_eq!(
            waypoint_display_altitude(&opts.seats, 2, opts.waypoint_altitude),
            1800.0
        );
        let pack = generate_template(&opts).unwrap();
        assert_eq!(
            pack.find_by_name("WP 1").unwrap().property("YPos").unwrap(),
            "900.000"
        );
        assert_eq!(
            pack.find_by_name("WP 2").unwrap().property("YPos").unwrap(),
            "1800.000"
        );
    }

    #[test]
    fn ground_spawn_stores_engine_running() {
        assert_eq!(
            PlaneStart::stored_for_altitude(PlaneStart::Air.as_i32(), 0.0),
            PlaneStart::Running.as_i32()
        );
        assert_eq!(
            PlaneStart::stored_for_altitude(PlaneStart::Cold.as_i32(), 0.0),
            PlaneStart::Cold.as_i32()
        );
        assert_eq!(
            PlaneStart::stored_for_altitude(PlaneStart::Running.as_i32(), 2500.0),
            PlaneStart::Air.as_i32()
        );
    }

    #[test]
    fn insert_goto_waypoint_after_numbers_next_hop() {
        let mig = builtin_plane_catalog()
            .into_iter()
            .find(|u| u.script.contains("mig15bis"))
            .unwrap();
        let mut a = TemplateSeat::new(mig);
        a.orders = vec![OrderSpec {
            kind: OrderKind::GotoWaypoint,
            waypoint: 1,
            ..OrderSpec::default()
        }];
        a.events = vec![EventHook {
            kind: EntityEvent::OnKilled,
            then: EventThen::Order(0),
        }];
        let mut seats = vec![a];
        let idx = insert_goto_waypoint_after(&mut seats, 0, 0);
        assert_eq!(idx, 1);
        assert_eq!(seats[0].orders.len(), 2);
        assert_eq!(seats[0].orders[1].kind, OrderKind::GotoWaypoint);
        assert_eq!(seats[0].orders[1].waypoint, 2);
        assert_eq!(used_waypoint_count(&seats), 2);
        assert_eq!(seats[0].events[0].then, EventThen::Order(0));
    }

    #[test]
    fn load_round_trips_generated_four_migs() {
        let src = four_migs();
        let pack = generate_template(&src).unwrap();
        let loaded = load_template(&pack, &builtin_plane_catalog()).unwrap();
        assert!(loaded.native_format);
        assert!(
            loaded.warnings.is_empty(),
            "unexpected warnings: {:?}",
            loaded.warnings
        );
        let opts = loaded.options;
        assert_eq!(opts.seats.len(), 4);
        assert_eq!(opts.seats[0].role, FlightRole::Lead);
        assert_eq!(opts.seats[1].role, FlightRole::Follows(0));
        assert_eq!(opts.seats[2].role, FlightRole::Follows(0));
        assert_eq!(opts.seats[3].role, FlightRole::Follows(0));
        assert_eq!(opts.bring_up, BringUp::Activate);
        assert_eq!(opts.zone_coalition, ZoneCoalition::Western);
        assert!((opts.zone_in - AIR_ZONE_IN_M).abs() < 1.0);
        assert!((opts.zone_out - AIR_ZONE_OUT_M).abs() < 1.0);
        assert_eq!(opts.place_layout, PlaceLayout::InvertedVee);
        assert_eq!(opts.per_group, 4);
        let kinds: Vec<_> = opts.seats[0].orders.iter().map(|o| o.kind).collect();
        assert_eq!(kinds, vec![OrderKind::Formation, OrderKind::AttackArea]);
        assert_eq!(opts.seats[0].orders[0].formation_type, 23);
        assert!(opts.seats[1].orders.is_empty());
        assert!(opts.seats[0].unit.script.contains("mig15bis"));
        assert_eq!(opts.seats[0].country, 501);
        assert!((opts.seats[0].altitude - 1000.0).abs() < 0.5);
    }

    #[test]
    fn load_round_trips_spawn_goto_and_rtb() {
        let mut opts = one_mig();
        opts.bring_up = BringUp::Spawn;
        opts.allow_multiple_spawns = true;
        opts.spawn_cooldown_min = 5.0;
        opts.seats[0].orders = vec![
            OrderSpec {
                kind: OrderKind::OnSpawned,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::GotoWaypoint,
                waypoint: 1,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::AttackArea,
                attack_area: 2000.0,
                time_s: 120.0,
                ..OrderSpec::default()
            },
            OrderSpec {
                kind: OrderKind::RtbOnZoneOut,
                ..OrderSpec::default()
            },
        ];
        opts.seats[0].events = vec![EventHook {
            kind: EntityEvent::OnPlaneDestroyed,
            then: EventThen::ForceComplete,
        }];
        let pack = generate_template(&opts).unwrap();
        let loaded = load_template(&pack, &builtin_plane_catalog()).unwrap();
        assert!(loaded.native_format);
        let got = loaded.options;
        assert_eq!(got.bring_up, BringUp::Spawn);
        assert!(got.allow_multiple_spawns);
        assert!((got.spawn_cooldown_min - 5.0).abs() < 0.1);
        let kinds: Vec<_> = got.seats[0].orders.iter().map(|o| o.kind).collect();
        assert!(kinds.contains(&OrderKind::OnSpawned));
        assert!(kinds.contains(&OrderKind::GotoWaypoint));
        assert!(kinds.contains(&OrderKind::AttackArea));
        assert!(kinds.contains(&OrderKind::RtbOnZoneOut));
        let area = got.seats[0]
            .orders
            .iter()
            .find(|o| o.kind == OrderKind::AttackArea)
            .unwrap();
        assert!((area.attack_area - 2000.0).abs() < 0.5);
        assert!(got.seats[0].events.iter().any(|e| {
            e.kind == EntityEvent::OnPlaneDestroyed && e.then == EventThen::ForceComplete
        }));
    }

    #[test]
    fn load_foreign_tank_platoon_rebuilds_from_units_and_orders() {
        let text = include_str!("../TemplateExamples/GroundUnits/DropIns/DPRK Tank Platoon.Group");
        let root = parse_group_file(text).unwrap();
        assert!(!looks_like_generated_template(&root));
        let loaded = load_template(&root, &bundled_catalog()).unwrap();
        assert!(!loaded.native_format);
        assert_eq!(loaded.options.seats.len(), 3);
        assert!(loaded.options.seats.iter().all(|s| s.unit.script.contains("t34-85")));
        assert_eq!(loaded.options.seats[0].country, 503);
        let area = loaded.options.seats[0]
            .orders
            .iter()
            .find(|o| o.kind == OrderKind::AttackArea)
            .expect("AttackArea kept");
        assert!((area.attack_area - 1500.0).abs() < 0.5);
        assert_eq!(area.shared_with, vec![1, 2]);
        assert!((loaded.options.zone_in - 7500.0).abs() < 1.0);
        assert!((loaded.options.zone_out - 8500.0).abs() < 1.0);
        assert_eq!(loaded.options.zone_coalition, ZoneCoalition::Both);
        assert_eq!(loaded.options.bring_up, BringUp::Activate);
        assert!(
            loaded.warnings.iter().any(|w| w.contains("not in Template Builder format")),
            "{:?}",
            loaded.warnings
        );
        assert!(
            loaded.warnings.iter().any(|w| w.contains("icon")),
            "expected dropped icons, got {:?}",
            loaded.warnings
        );
        assert!(
            loaded.warnings.iter().any(|w| w.contains("OnKilled") || w.contains("Damaged")),
            "expected dropped OnKilled → Damaged Counter, got {:?}",
            loaded.warnings
        );
        let rebuilt = generate_template(&loaded.options).unwrap();
        assert!(looks_like_generated_template(&rebuilt));
        assert!(rebuilt.find_by_name("ENABLE / PULSE IN").is_some());
        assert_eq!(rebuilt.count_block_type("Vehicle"), 3);
    }

    #[test]
    fn load_empty_group_is_an_error() {
        let mut root = Il2Entity::new("Group");
        root.set_name("Empty");
        assert!(load_template(&root, &[]).is_err());
    }
}
