//! Build a memory-efficient unit template: proximity checkzones, activate or
//! spawn, formation placement, per-unit orders, and waypoints.
//!
//! Generated names other modes look for: `Zone IN`, `ENABLE / PULSE IN`,
//! `MISSION END`. This mode never writes `NodeGates`.

use crate::aircraft::{
    callsign_for, encode_tcode, encode_tcode_color, flight_color, flight_number,
    plane_display_name, AircraftType, AIRCRAFT_TYPES,
};
use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::parser::parse_il2_document;
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
const WAYPOINT_AREA_M: &str = "200";
pub const DEFAULT_TIME_ON_TARGET_S: f32 = 180.0;
/// World spacing between seats in a placement group. Not a UI control.
pub const PLACEMENT_SPACING: f32 = 150.0;
pub const DEFAULT_ATTACK_AREA_M: f32 = 3000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Plane,
    Vehicle,
    Train,
    Ship,
    Fixed,
    UserAdded,
}

impl UnitKind {
    pub const ALL: [UnitKind; 6] = [
        UnitKind::Plane,
        UnitKind::Vehicle,
        UnitKind::Train,
        UnitKind::Ship,
        UnitKind::Fixed,
        UnitKind::UserAdded,
    ];

    pub fn label(self) -> &'static str {
        match self {
            UnitKind::Plane => "Planes",
            UnitKind::Vehicle => "Vehicles",
            UnitKind::Train => "Trains",
            UnitKind::Ship => "Ships",
            UnitKind::Fixed => "Fixed Units",
            UnitKind::UserAdded => "User Added",
        }
    }

    fn object_type(self) -> &'static str {
        match self {
            UnitKind::Plane => "Plane",
            UnitKind::Vehicle | UnitKind::Fixed | UnitKind::UserAdded => "Vehicle",
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
    OrderKind::MissionComplete,
    OrderKind::Land,
    OrderKind::TakeOff,
    OrderKind::RtbOnZoneOut,
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
    /// takeoff, RTB, OnTookOff, and OnLanded are aircraft-only.
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
            ZoneCoalition::Eastern => "Eastern [1]",
            ZoneCoalition::Western => "Western [2]",
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
        let vulnerable = prop_bool(&unit.object, "Vulnerable", true);
        let engageable = prop_bool(&unit.object, "Engageable", true);
        let limit_ammo = prop_bool(&unit.object, "LimitAmmo", true);
        let ai_rtb = prop_bool(&unit.object, "AiRTBDecision", false);
        let start_type = prop_i32(&unit.object, "StartType", 0);
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

/// Copy country / skill / fuel / payload / flags from `from` onto every other
/// seat. Role, model, orders, and formation index are left alone.
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
        seat.payload_id = src.payload_id;
        seat.vulnerable = src.vulnerable;
        seat.engageable = src.engageable;
        seat.limit_ammo = src.limit_ammo;
        seat.ai_rtb = src.ai_rtb;
        seat.start_type = src.start_type;
        if src.unit.is_air() && seat.unit.is_air() {
            seat.altitude = src.altitude;
        }
    }
}

/// Append a unit, copying country / skill / fuel flags from the last seat and
/// altitude from the last plane (new planes sit near that height).
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
    let prev = seats.last().cloned();
    let mut seat = TemplateSeat::new(unit);
    if let Some(p) = prev {
        seat.country = p.country;
        seat.skill = p.skill;
        seat.fuel = p.fuel;
        seat.payload_id = p.payload_id;
        seat.vulnerable = p.vulnerable;
        seat.engageable = p.engageable;
        seat.limit_ammo = p.limit_ammo;
        seat.ai_rtb = p.ai_rtb;
        seat.start_type = p.start_type;
    }
    if seat.unit.is_air() {
        if let Some(alt) = last_plane_alt {
            seat.altitude = alt;
        }
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
    pub zone_coalition: ZoneCoalition,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            name: "Unit Template".into(),
            zone_in: 7_500.0,
            zone_out: 8_500.0,
            spacing: PLACEMENT_SPACING,
            seats: Vec::new(),
            place_layout: PlaceLayout::InvertedVee,
            per_group: 4,
            bring_up: BringUp::Activate,
            allow_multiple_spawns: false,
            spawn_cooldown_min: 5.0,
            waypoint_count: 0,
            waypoint_spacing: 4_000.0,
            waypoint_speed: 100.0,
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

/// Planes, vehicles, trains, ships, and fixed objects from the bundled catalogs.
pub fn bundled_catalog() -> Vec<CatalogUnit> {
    let mut cat = match parse_il2_document(include_str!("../TemplateExamples/ModelTypes.Group")) {
        Ok(root) => load_catalog(&root),
        Err(_) => Vec::new(),
    };
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
    if let Ok(fixed) = parse_il2_document(include_str!("../TemplateExamples/Unit_Template_Fixed.Group"))
    {
        merge_catalog(&mut cat, load_catalog(&fixed));
    }
    if cat.is_empty() {
        builtin_plane_catalog()
    } else {
        cat
    }
}

/// Read a catalog `.Group`. Preferred layout is subgroups named
/// `Planes` / `All Planes`, `Vehicles` / `All Vehicles`, `Trains` /
/// `All Trains`, `Ships` / `All Ships`, `Fixed` / `Fixed Units` /
/// `Fixed Objects`, and `User Added`. Loose Plane / Vehicle /
/// Train / Ship blocks at any depth are also collected.
pub fn load_catalog(root: &Il2Entity) -> Vec<CatalogUnit> {
    let mut out = Vec::new();
    collect_kind_group(root, "Planes", UnitKind::Plane, &mut out);
    collect_kind_group(root, "All Planes", UnitKind::Plane, &mut out);
    collect_kind_group(root, "Aircraft", UnitKind::Plane, &mut out);
    collect_kind_group(root, "Vehicles", UnitKind::Vehicle, &mut out);
    collect_kind_group(root, "All Vehicles", UnitKind::Vehicle, &mut out);
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

fn collect_user_added_group(root: &Il2Entity, out: &mut Vec<CatalogUnit>) {
    let mut groups = Vec::new();
    find_groups_named(root, "User Added", &mut groups);
    for g in groups {
        let mut tmp = Vec::new();
        collect_objects(g, UnitKind::Plane, &mut tmp);
        collect_objects(g, UnitKind::Vehicle, &mut tmp);
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
                .is_some_and(|s| is_fixed_script(s))
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
    let path_altitude = first_plane_altitude(&opts.seats);
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

    let mut zone_in_mcu = checkzone("Zone IN", zone_in, true, coalitions, &mut next_id, zx, zz);
    let mut zone_out_mcu = checkzone("Zone Out", zone_out, false, coalitions, &mut next_id, zx, zz);

    let mut deact_in = mcu("MCU_Deactivate", "Self Deactivate", &mut next_id, zx, zz);
    let mut react_out = mcu("MCU_Activate", "Zone Out ReActivate", &mut next_id, zx, zz);
    let mut deact_out = mcu("MCU_Deactivate", "Self Deactivate", &mut next_id, zx, zz);
    let mut react_in = mcu("MCU_Activate", "Zone In ReActivate", &mut next_id, zx, zz);
    let cooldown_s = if bring_up == BringUp::Spawn && opts.allow_multiple_spawns {
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
    match bring_up {
        BringUp::Activate => {
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
        BringUp::Spawn => {
            let mut s = mcu(
                "MCU_Spawner",
                "Trigger Spawner",
                &mut next_id,
                bring_ox,
                bring_oz,
            );
            s.set_property("SpawnAtMe", "0");
            s.set_objects(entity_ids.clone());
            let drop = i32::from(opts.allow_multiple_spawns);
            let mut c = mcu(
                "MCU_Counter",
                "SpawnCount",
                &mut next_id,
                bring_ox,
                bring_oz,
            );
            c.set_property("Counter", "1");
            c.set_property("Dropcount", drop.to_string());
            spawn = Some(s);
            spawn_count = Some(c);
        }
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
            wp.set_property("Speed", format!("{:.0}", opts.waypoint_speed.max(10.0)));
            wp.set_property("Priority", "2");
            wp.set_objects(objects);
            if seats_have_planes(&opts.seats) {
                wp.set_property("YPos", format!("{:.3}", path_altitude));
            }
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
            let wait = if order.kind == OrderKind::TimeOnTarget {
                order.time_s.max(0.0) as f64
            } else {
                order.delay_s.max(0.0) as f64
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
        wp.set_property("Area", WAYPOINT_AREA_M);
        wp.set_property("Speed", format!("{:.0}", opts.waypoint_speed.max(10.0)));
        wp.set_property("Priority", "1");
        wp.set_objects(lead_entity_ids.clone());
        if seats_have_planes(&opts.seats) {
            wp.set_property("YPos", format!("{:.3}", path_altitude));
        }
        waypoints.push(wp);
    }

    if uses_goto {
        let mut wp_objects: Vec<Vec<i32>> = vec![Vec::new(); waypoints.len()];
        let mut wp_alt: Vec<Option<f64>> = vec![None; waypoints.len()];
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
                if seat.unit.is_air() && wp_alt[idx].is_none() {
                    wp_alt[idx] = Some(seat.altitude as f64);
                }
            }
        }
        for (i, wp) in waypoints.iter_mut().enumerate() {
            wp.set_objects(wp_objects[i].clone());
            if let Some(y) = wp_alt[i] {
                wp.set_property("YPos", format!("{y:.3}"));
            }
        }
    }

    let zone_in_id = zone_in_mcu.index.unwrap();
    let zone_out_id = zone_out_mcu.index.unwrap();
    let pulse_id = pulse.index.unwrap();
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

    begin.set_targets(vec![pulse_id]);
    pulse.set_targets(vec![zone_in_id]);

    zone_in_mcu.set_targets(vec![deact_in_id, react_out_id, bring_up_timer_id]);

    deact_in.set_targets(vec![zone_in_id]);
    react_out.set_targets(vec![zone_out_id]);

    let mut bring_up_targets = vec![after_bring_up_id];
    match bring_up {
        BringUp::Activate => {
            if let Some(id) = activate_id {
                bring_up_targets.insert(0, id);
            }
        }
        BringUp::Spawn => {
            if let Some(id) = spawn_count_id {
                bring_up_targets.insert(0, id);
            }
        }
    }
    bring_up_timer.set_targets(bring_up_targets);
    if let (Some(counter), Some(spawner_id)) = (spawn_count.as_mut(), spawn_id) {
        counter.set_targets(vec![spawner_id]);
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

    for si in 0..emitted.len() {
        for oi in 0..emitted[si].len() {
            let is_report = emitted[si][oi].report.is_some();
            let next_is_report = emitted[si]
                .get(oi + 1)
                .is_some_and(|s| s.report.is_some());
            let mut targets = Vec::new();
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
            } else if oi + 1 < emitted[si].len() {
                if (is_report || !next_is_report)
                    && !skip_delay_chain(&emitted[si], oi)
                {
                    targets.push(emitted[si][oi + 1].delay.index.unwrap());
                }
            }
            emitted[si][oi].delay.set_targets(targets);
        }
    }

    wire_waypoint_chain(&mut waypoints, &emitted);

    zone_out_mcu.set_targets(vec![deact_out_id, cooldown_id, mission_end_id]);
    deact_out.set_targets(vec![zone_out_id]);
    cooldown.set_targets(vec![react_in_id]);
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

    logic.children.extend([
        begin,
        pulse,
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
}

fn is_attack_emitted(step: &EmittedOrder) -> bool {
    step.cmd.as_ref().is_some_and(|c| {
        c.block_type == "MCU_CMD_AttackTarget" || c.block_type == "MCU_CMD_AttackArea"
    })
}

/// Time on Target is pulsed from the waypoint before the attack, not the previous delay.
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

fn skip_delay_chain(steps: &[EmittedOrder], oi: usize) -> bool {
    let Some(next) = steps.get(oi + 1) else {
        return false;
    };
    let cur = &steps[oi];
    if next.time_on_target {
        return tot_owned_by_wp(steps, oi + 1);
    }
    if cur.goto_wp.is_some() && (is_attack_emitted(next) || next.goto_wp.is_some()) {
        return true;
    }
    false
}

/// On arrival WP n pulses the next order's timer (Attack / AttackArea delay,
/// Time on Target, or the next Goto WP delay). The next waypoint MCU is reached
/// through that delay, not a WP n → WP n+1 MCU link.
fn wire_waypoint_chain(waypoints: &mut [Il2Entity], emitted: &[Vec<EmittedOrder>]) {
    let mut extra: Vec<Vec<i32>> = vec![Vec::new(); waypoints.len()];
    for steps in emitted {
        for (oi, step) in steps.iter().enumerate() {
            let Some(wp_n) = step.goto_wp else {
                continue;
            };
            let idx = wp_n.max(1) as usize - 1;
            if idx >= extra.len() {
                continue;
            }
            let mut saw_attack = false;
            for later in &steps[oi + 1..] {
                if later.mission_complete {
                    break;
                }
                if later.report.is_some() {
                    break;
                }
                if later.goto_wp.is_some() {
                    if !saw_attack {
                        if let Some(id) = later.delay.index {
                            if !extra[idx].contains(&id) {
                                extra[idx].push(id);
                            }
                        }
                    }
                    break;
                }
                if is_attack_emitted(later) {
                    if let Some(id) = later.delay.index {
                        if !extra[idx].contains(&id) {
                            extra[idx].push(id);
                        }
                    }
                    saw_attack = true;
                    continue;
                }
                if later.time_on_target {
                    if let Some(id) = later.delay.index {
                        if !extra[idx].contains(&id) {
                            extra[idx].push(id);
                        }
                    }
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
    object.set_existing_property("Vulnerable", i32::from(seat.vulnerable).to_string());
    object.set_existing_property("Engageable", i32::from(seat.engageable).to_string());
    object.set_existing_property("LimitAmmo", i32::from(seat.limit_ammo).to_string());
    // Planes only. Writing these on a Vehicle makes the mission editor stop
    // parsing the rest of the group (one unit, no entity, no followers).
    object.set_existing_property("AiRTBDecision", i32::from(seat.ai_rtb).to_string());
    object.set_existing_property("StartType", seat.start_type.to_string());
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
        .unwrap_or(1000.0)
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
            cmd.set_property("Priority", "2");
        }
        OrderKind::Effect => {
            cmd.set_property("ActionType", i32::from(!order.effect_start).to_string());
        }
        OrderKind::Flare => {
            cmd.set_property("Color", order.flare_color.to_string());
        }
        OrderKind::ForceComplete => {
            cmd.set_property("Priority", "2");
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
            parse_il2_document(include_str!("../TemplateExamples/ModelTypes.Group")).unwrap();
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
        let src = include_str!("../TemplateExamples/Unit_Template_Fixed.Group");
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
            "Unit_Template_Fixed.Group should list fixed-object scripts"
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
            "Fixed Units catalog should include every prototype in Unit_Template_Fixed.Group"
        );
    }

    #[test]
    fn generate_writes_hooks_other_modes_need() {
        let pack = generate_template(&four_migs()).unwrap();
        assert!(pack.find_by_name("Zone IN").is_some());
        assert!(pack.find_by_name("Zone Out").is_some());
        assert!(pack.find_by_name("ENABLE / PULSE IN").is_some());
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
        assert_eq!(counter.property("Dropcount"), Some("1"));
        let spawner = pack.find_by_name("Trigger Spawner").unwrap();
        assert!(counter.targets.contains(&spawner.index.unwrap()));
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
        let zone_out = pack.find_by_name("Zone Out").unwrap();
        let react_in = pack.find_by_name("Zone In ReActivate").unwrap();
        assert!(zone_out.targets.contains(&cooldown.index.unwrap()));
        assert!(!zone_out.targets.contains(&react_in.index.unwrap()));
        assert!(cooldown.targets.contains(&react_in.index.unwrap()));
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
        assert!(OrderKind::available(UnitKind::Plane).contains(&OrderKind::MissionComplete));
        assert!(OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::TimeOnTarget));
        assert!(OrderKind::available(UnitKind::Vehicle).contains(&OrderKind::MissionComplete));
    }

    #[test]
    fn path_waypoints_use_200m_area_and_attack_area_defaults_to_3000() {
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
        assert_eq!(wp1.property("Area"), Some("200"));
        let area = pack.find_by_name("AttackArea").unwrap();
        assert_eq!(area.property("AttackArea"), Some("3000"));
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
}
