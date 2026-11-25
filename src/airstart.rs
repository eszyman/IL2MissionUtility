//! airstart.rs — fake-field air starts
//!
//! An air start is the small `Airfield` + `MCU_TR_Entity` pair in
//! `TemplateExamples/Airstart.Group`: a `fakefield` with a `Planes` list,
//! not a harvested airfield. The Airfield tab keeps a bank of them
//! (coalition and aircraft options). Map mode places copies and sets facing.
//! The field icon stays upright; `assets/direction.svg` points along `YOri`
//! (0 = grid north, +X), the same arrow the other map units use.
//!
//! ## Public API
//! * `struct AirStartPlane` / `struct AirStartField` / `struct PlacedAirStart`
//! * `fn default_field` / `fn default_plane` / `fn plane_paths`
//! * `fn field_group` — one field as a `Group`
//! * `fn combined_group` — several fields in one `Group`
//! * `fn read_mission` / `fn is_air_start_group` / `fn merge_bank`
//! * `COUNTRY_NATO` (601) / `COUNTRY_DPRK` (502)
//!
//! ## Used by
//! * ui.rs (Airfield) — the bank and its export
//! * ui/map.rs — place, move, turn, and stamp into the base map

use crate::ast::{Il2Entity, format_int_array};
use crate::model_spec::{self, aircraft_specs};
use crate::payloads;

/// USA. Matches `TemplateExamples/Airstart.Group`.
pub const COUNTRY_NATO: i32 = 601;
/// DPRK, from `aircraft::COUNTRIES`.
pub const COUNTRY_DPRK: i32 = 502;

const FAKE_MODEL: &str = r"graphics\airfields\fakefield.mgm";
const FAKE_SCRIPT: &str = r"LuaScripts\WorldObjects\Airfields\fakefield.txt";

/// One aircraft the fake field can launch. `StartType` stays 0 (air).
#[derive(Clone, Debug)]
pub struct AirStartPlane {
    pub type_id: String,
    pub ai_level: i32,
    pub altitude_m: i32,
    pub fuel: f64,
    pub payload_id: i32,
    pub mod_mask: String,
    pub renewable: bool,
    pub renew_time_s: i32,
    pub limit_ammo: bool,
    pub engageable: bool,
    pub vulnerable: bool,
    /// `AIRTBDecision` on the airfield plane entry.
    pub rtb: bool,
    /// `Number` on the airfield plane entry. `-1` is unlimited.
    pub number: i32,
    /// Field spawn only. Air starts ignore this and still write `StartType` 0
    /// and `SnapTo` 0. `true` is engine off, snapped to parking.
    pub parking_snap: bool,
    /// Editor `Name`. Empty means the automatic start-and-load name
    /// (`In air - Strike 2`, `At ramp - Long range`). A typed value is kept.
    pub name: String,
}

/// One fake field in the bank. `nato` is the coalition; `heading_deg` is `YOri`.
#[derive(Clone, Debug)]
pub struct AirStartField {
    pub name: String,
    pub nato: bool,
    pub heading_deg: f64,
    pub planes: Vec<AirStartPlane>,
}

/// A bank field parked on the map. The field is a snapshot from placement.
#[derive(Clone, Debug)]
pub struct PlacedAirStart {
    pub field: AirStartField,
    pub x: f64,
    pub z: f64,
}

impl AirStartField {
    pub fn country(&self) -> i32 {
        if self.nato {
            COUNTRY_NATO
        } else {
            COUNTRY_DPRK
        }
    }

    pub fn side_label(&self) -> &'static str {
        if self.nato { "NATO" } else { "DPRK" }
    }
}

impl AirStartPlane {
    pub fn script(&self) -> String {
        plane_paths(&self.type_id).1
    }
}

/// `(model, script)` for a `model_spec` aircraft id.
pub fn plane_paths(type_id: &str) -> (String, String) {
    (
        format!(r"graphics\planes\{type_id}\{type_id}.mgm"),
        format!(r"LuaScripts\WorldObjects\Planes\{type_id}.txt"),
    )
}

pub fn default_plane(type_id: &str) -> AirStartPlane {
    let script = plane_paths(type_id).1;
    AirStartPlane {
        type_id: type_id.to_string(),
        ai_level: 2,
        altitude_m: 1500,
        fuel: default_fuel(type_id),
        payload_id: 0,
        mod_mask: payloads::default_mod_mask_str(&script),
        renewable: true,
        renew_time_s: 1800,
        limit_ammo: true,
        engageable: true,
        vulnerable: true,
        rtb: true,
        number: -1,
        parking_snap: false,
        name: String::new(),
    }
}

/// A new bank entry. NATO starts with an F-51D; DPRK with a MiG-15bis.
pub fn default_field(name: impl Into<String>, nato: bool) -> AirStartField {
    let type_id = if nato { "f51d" } else { "mig15bis" };
    AirStartField {
        name: name.into(),
        nato,
        heading_deg: 0.0,
        planes: vec![default_plane(type_id)],
    }
}

/// Fuel for a new aircraft. The F-51 stays at 65%. Every other type starts full.
pub fn default_fuel(type_id: &str) -> f64 {
    if type_id.eq_ignore_ascii_case("f51d") {
        0.65
    } else {
        1.0
    }
}

/// Reset payload, mods, and fuel when the aircraft type changes.
pub fn apply_type(plane: &mut AirStartPlane, type_id: &str) {
    if plane.type_id == type_id {
        return;
    }
    let fresh = default_plane(type_id);
    plane.type_id = fresh.type_id;
    plane.payload_id = fresh.payload_id;
    plane.mod_mask = fresh.mod_mask;
    plane.fuel = fresh.fuel;
}

/// One field wrapped in a `Group` named after it.
pub fn field_group(field: &AirStartField, x: f64, z: f64, y: f64) -> Il2Entity {
    let mut next = 1i32;
    let mut root = named_group(&field.name, &mut next);
    append_field(&mut root, field, x, y, z, &mut next);
    root
}

/// Every field in one `Group` named `Air starts`. `ys` is ground height per field.
pub fn combined_group(placed: &[PlacedAirStart], ys: &[f64]) -> Il2Entity {
    let mut next = 1i32;
    let mut root = named_group("Air starts", &mut next);
    for (i, placed) in placed.iter().enumerate() {
        let y = ys.get(i).copied().unwrap_or(0.0);
        let mut child = named_group(&placed.field.name, &mut next);
        append_field(&mut child, &placed.field, placed.x, y, placed.z, &mut next);
        root.children.push(child);
    }
    root
}

/// Bank entries parked on a 2 km grid from 40000, 40000 so an export can be moved in the editor.
pub fn parked(fields: &[AirStartField]) -> Vec<PlacedAirStart> {
    fields
        .iter()
        .enumerate()
        .map(|(i, field)| PlacedAirStart {
            field: field.clone(),
            x: 40_000.0 + i as f64 * 2_000.0,
            z: 40_000.0,
        })
        .collect()
}

pub fn file_stem(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "AirStart".into()
    } else {
        safe
    }
}

fn named_group(name: &str, next_id: &mut i32) -> Il2Entity {
    let mut g = Il2Entity::new("Group");
    g.set_name(name);
    g.index = Some(*next_id);
    g.set_property("Index", next_id.to_string());
    *next_id += 1;
    g.set_property("Desc", "\"\"");
    g
}

fn append_field(
    parent: &mut Il2Entity,
    field: &AirStartField,
    x: f64,
    y: f64,
    z: f64,
    next_id: &mut i32,
) {
    let air_id = *next_id;
    *next_id += 1;
    let ent_id = *next_id;
    *next_id += 1;

    let mut air = Il2Entity::new("Airfield");
    air.index = Some(air_id);
    air.set_name(&field.name);
    air.set_property("Index", air_id.to_string());
    air.set_property("LinkTrId", ent_id.to_string());
    set_pos(&mut air, x, y, z, field.heading_deg);
    air.set_property("Model", quoted(FAKE_MODEL));
    air.set_property("Script", quoted(FAKE_SCRIPT));
    air.set_property("Country", field.country().to_string());
    air.set_property("Desc", "\"\"");
    air.set_property("DamageReport", "50");
    air.set_property("DamageThreshold", "1");
    air.set_property("DeleteAfterDeath", "1");
    air.set_property("Callsign", "0");
    air.set_property("Callnum", "0");
    air.set_property("ReturnPlanes", "0");
    air.set_property("Hydrodrome", "0");
    air.set_property("RepairTimeMultiplier", "0");
    air.set_property("RehealTimeMultiplier", "0");
    air.set_property("RearmTimeMultiplier", "0");
    air.set_property("RefuelTimeMultiplier", "0");
    air.set_property("MaintenanceRadius", "10");

    let mut planes = Il2Entity::new("Planes");
    for (i, plane) in field.planes.iter().enumerate() {
        planes.children.push(plane_block(plane, 36 + i as i32));
    }
    air.children.push(planes);

    let mut entity = Il2Entity::new("MCU_TR_Entity");
    entity.index = Some(ent_id);
    entity.set_name("Airfield entity");
    entity.set_property("Index", ent_id.to_string());
    entity.set_property("Desc", "\"\"");
    entity.set_targets(vec![]);
    entity.set_objects(vec![]);
    set_pos(&mut entity, x, y + 0.2, z, field.heading_deg);
    entity.set_property("Enabled", "1");
    entity.set_property("MisObjID", air_id.to_string());

    parent.children.push(air);
    parent.children.push(entity);
}

/// A `Planes` entry. `start_type` is IL-2's `StartType` (`0` in air, `1` engine
/// on, `2` engine off, `3` engine cold). `snap_to` is `SnapTo` (`0` none,
/// `1` runway, `2` parking). Altitude is written as given; a ground start
/// passes 0.
pub fn plane_entry(
    plane: &AirStartPlane,
    callsign: i32,
    start_type: i32,
    altitude_m: i32,
    snap_to: i32,
) -> Il2Entity {
    let (model, script) = plane_paths(&plane.type_id);
    let mut e = Il2Entity::new("Plane");
    e.set_property("SetIndex", "0");
    e.set_property("Number", plane.number.to_string());
    e.set_property("AILevel", plane.ai_level.clamp(0, 4).to_string());
    e.set_property("StartType", start_type.to_string());
    e.set_property("SnapTo", snap_to.to_string());
    e.set_property("Engageable", bit(plane.engageable));
    e.set_property("Vulnerable", bit(plane.vulnerable));
    e.set_property("LimitAmmo", bit(plane.limit_ammo));
    e.set_property("AIRTBDecision", bit(plane.rtb));
    e.set_property("Renewable", bit(plane.renewable));
    e.set_property("PayloadId", plane.payload_id.to_string());
    let mask = if plane.mod_mask.is_empty() {
        "0".to_string()
    } else {
        plane.mod_mask.clone()
    };
    e.set_property("ModMask", mask);
    e.set_property("Fuel", fmt_fuel(plane.fuel));
    e.set_property("RouteTime", "0");
    e.set_property("RenewTime", plane.renew_time_s.max(0).to_string());
    e.set_property("Altitude", altitude_m.max(0).to_string());
    e.set_property("Spotter", "-1");
    e.set_property("Model", quoted(&model));
    e.set_property("Script", quoted(&script));
    e.set_property("Skin", "\"\"");
    e.set_property("BotSkin", "\"\"");
    e.set_property("AvMods", "\"\"");
    e.set_property("AvSkins", "\"\"");
    e.set_property("AvPayloads", "\"\"");
    e.set_property("AvBotSkins", "\"\"");
    e.set_property(
        "Name",
        quoted(&written_plane_name(plane, start_type, snap_to)),
    );
    e.set_property("Callsign", callsign.to_string());
    e.set_property("Callnum", "0");
    e.set_property("TCode", "\"\"");
    e.set_property("TCodeColors", "\"\"");
    e.set_property("GunLoad", format_int_array(&[0, 0, 0, 0, 0, 0]));
    e.set_property("GunBelt", format_int_array(&[0, 0, 0, 0, 0, 0]));
    e.set_property("Emblem", "0");
    e
}

fn plane_block(plane: &AirStartPlane, callsign: i32) -> Il2Entity {
    plane_entry(plane, callsign, 0, plane.altitude_m, 0)
}

/// Fuel at or above this is a long-range load. 0.95 is 95%.
const LONG_RANGE_FUEL: f64 = 0.95;

/// Plane `Name` when the user has not typed one. Snap To wins over Start Type.
/// Payload 0 is Clean, payload N is `Strike N`, and fuel at or above
/// [`LONG_RANGE_FUEL`] replaces Clean or follows the strike.
pub fn automatic_plane_name(start_type: i32, snap_to: i32, payload_id: i32, fuel: f64) -> String {
    let condition = if snap_to == 2 {
        "Parked"
    } else if snap_to == 1 {
        "On runway"
    } else if start_type == 2 || start_type == 3 {
        "Parked"
    } else if start_type == 1 {
        "At ramp"
    } else {
        "In air"
    };
    let long_range = fuel >= LONG_RANGE_FUEL;
    let load = if payload_id > 0 && long_range {
        format!("Strike {payload_id} - Long range")
    } else if payload_id > 0 {
        format!("Strike {payload_id}")
    } else if long_range {
        "Long range".to_string()
    } else {
        "Clean".to_string()
    };
    format!("{condition} - {load}")
}

/// The name written on the plane. A typed name wins. Otherwise the automatic one.
pub fn written_plane_name(plane: &AirStartPlane, start_type: i32, snap_to: i32) -> String {
    let custom = plane.name.replace('"', "").trim().to_string();
    if !custom.is_empty() {
        return custom;
    }
    automatic_plane_name(start_type, snap_to, plane.payload_id, plane.fuel)
}

fn set_pos(e: &mut Il2Entity, x: f64, y: f64, z: f64, heading: f64) {
    e.set_property("XPos", format!("{x:.3}"));
    e.set_property("YPos", format!("{y:.3}"));
    e.set_property("ZPos", format!("{z:.3}"));
    e.set_property("XOri", "0");
    e.set_property("YOri", format!("{:.3}", heading.rem_euclid(360.0)));
    e.set_property("ZOri", "0");
}

fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

fn bit(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

fn fmt_fuel(fuel: f64) -> String {
    let fuel = fuel.clamp(0.0, 1.0);
    if (fuel - fuel.round()).abs() < 1e-6 {
        format!("{:.0}", fuel.round())
    } else {
        format!("{fuel:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

/// Aircraft lists read off a base map. `air_starts` are fakefields with no
/// taxi graph. `fields` are defined airfields (a taxi graph, or the runway
/// spawn model) that already carry a `Planes` list.
#[derive(Clone, Debug, Default)]
pub struct MissionAir {
    pub air_starts: Vec<PlacedAirStart>,
    pub fields: Vec<ImportedField>,
}

/// One defined airfield and the aircraft written on it.
#[derive(Clone, Debug)]
pub struct ImportedField {
    pub group: Il2Entity,
    pub planes: Vec<AirStartPlane>,
}

/// Air starts and defined-airfield aircraft anywhere under `root`.
pub fn read_mission(root: &Il2Entity) -> MissionAir {
    let mut out = MissionAir::default();
    scan_air(root, &mut out);
    out
}

/// True when `group` is only fakefield air starts, so a base-map import
/// should not treat it as a ground pack or a reference airfield.
pub fn is_air_start_group(group: &Il2Entity) -> bool {
    let read = read_mission(group);
    !read.air_starts.is_empty() && read.fields.is_empty() && !has_ground_unit(group)
}

/// Add placed air starts to the Airfield bank. A bank entry with the same
/// name is replaced by the imported aircraft.
pub fn merge_bank(bank: &mut Vec<AirStartField>, placed: &[PlacedAirStart]) {
    for placed in placed {
        if let Some(existing) = bank
            .iter_mut()
            .find(|field| field.name == placed.field.name)
        {
            *existing = placed.field.clone();
        } else {
            bank.push(placed.field.clone());
        }
    }
}

fn scan_air(node: &Il2Entity, out: &mut MissionAir) {
    let nested: Vec<&Il2Entity> = node
        .children
        .iter()
        .filter(|child| child.block_type == "Group" && has_block(child, "Airfield"))
        .collect();
    if node.block_type == "Group" && nested.is_empty() && has_block(node, "Airfield") {
        classify_field_group(node, out);
        return;
    }
    for child in nested {
        scan_air(child, out);
    }
}

fn classify_field_group(group: &Il2Entity, out: &mut MissionAir) {
    let Some(air) = find_block(group, "Airfield") else {
        return;
    };
    if is_pure_fakefield(air) && !has_block(group, "MCU_TR_TaxiGraph") {
        if let Some(placed) = placed_from_airfield(air, group) {
            out.air_starts.push(placed);
        }
        return;
    }
    let planes = planes_of(air);
    if planes.is_empty() {
        return;
    }
    out.fields.push(ImportedField {
        group: group.clone(),
        planes,
    });
}

fn placed_from_airfield(air: &Il2Entity, group: &Il2Entity) -> Option<PlacedAirStart> {
    let (x, z) = air.pos_xz().or_else(|| group.first_xz())?;
    let country = prop_int(air, "Country").unwrap_or(COUNTRY_NATO);
    let name = useful_name(air.name())
        .or_else(|| useful_name(group.name()))
        .unwrap_or_else(|| "Air start".to_string());
    Some(PlacedAirStart {
        field: AirStartField {
            name,
            nato: crate::aircraft::country_is_nato(country) != Some(false),
            heading_deg: prop_f64(air, "YOri").unwrap_or(0.0).rem_euclid(360.0),
            planes: planes_of(air),
        },
        x,
        z,
    })
}

fn planes_of(air: &Il2Entity) -> Vec<AirStartPlane> {
    air.children
        .iter()
        .filter(|child| child.block_type == "Planes")
        .flat_map(|list| list.children.iter().filter_map(plane_from_entry))
        .collect()
}

fn plane_from_entry(entry: &Il2Entity) -> Option<AirStartPlane> {
    if entry.block_type != "Plane" {
        return None;
    }
    let type_id = plane_type_id(entry)?;
    let mut plane = default_plane(&type_id);
    if let Some(number) = prop_int(entry, "Number") {
        plane.number = number;
    }
    if let Some(level) = prop_int(entry, "AILevel") {
        plane.ai_level = level.clamp(0, 4);
    }
    if let Some(altitude) = prop_int(entry, "Altitude") {
        plane.altitude_m = altitude.max(0);
    }
    if let Some(fuel) = prop_f64(entry, "Fuel") {
        plane.fuel = fuel.clamp(0.0, 1.0);
    }
    if let Some(payload) = prop_int(entry, "PayloadId") {
        plane.payload_id = payload;
    }
    if let Some(mask) = entry.property("ModMask") {
        let mask = mask.trim().trim_matches('"');
        if !mask.is_empty() {
            plane.mod_mask = mask.to_string();
        }
    }
    if let Some(on) = prop_bit(entry, "Renewable") {
        plane.renewable = on;
    }
    if let Some(seconds) = prop_int(entry, "RenewTime") {
        plane.renew_time_s = seconds.max(0);
    }
    if let Some(on) = prop_bit(entry, "LimitAmmo") {
        plane.limit_ammo = on;
    }
    if let Some(on) = prop_bit(entry, "Engageable") {
        plane.engageable = on;
    }
    if let Some(on) = prop_bit(entry, "Vulnerable") {
        plane.vulnerable = on;
    }
    if let Some(on) = prop_bit(entry, "AIRTBDecision") {
        plane.rtb = on;
    }
    let start = prop_int(entry, "StartType").unwrap_or(0);
    let snap = prop_int(entry, "SnapTo").unwrap_or(0);
    plane.parking_snap = snap == 2 || start == 2 || start == 3;
    if let Some(raw) = entry.name() {
        let auto = automatic_plane_name(start, snap, plane.payload_id, plane.fuel);
        if !raw.is_empty() && raw != auto {
            plane.name = raw.to_string();
        }
    }
    Some(plane)
}

fn plane_type_id(entry: &Il2Entity) -> Option<String> {
    for key in ["Script", "Model"] {
        let Some(raw) = entry.property(key) else {
            continue;
        };
        let text = raw.trim().trim_matches('"').replace('/', "\\");
        let file = text.rsplit('\\').next().unwrap_or("");
        let stem = file
            .strip_suffix(".txt")
            .or_else(|| file.strip_suffix(".mgm"))
            .unwrap_or(file);
        if !stem.is_empty() {
            return Some(stem.to_string());
        }
    }
    None
}

fn is_pure_fakefield(air: &Il2Entity) -> bool {
    let blob = format!(
        "{} {}",
        air.property("Script").unwrap_or(""),
        air.property("Model").unwrap_or("")
    )
    .to_ascii_lowercase();
    blob.contains("fakefield") && !blob.contains("rnwspawn")
}

fn useful_name(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    if name.is_empty()
        || name.eq_ignore_ascii_case("airfield")
        || name.eq_ignore_ascii_case("group")
        || name.eq_ignore_ascii_case("airfield entity")
    {
        None
    } else {
        Some(name.to_string())
    }
}

fn has_block(node: &Il2Entity, block_type: &str) -> bool {
    node.block_type == block_type
        || node
            .children
            .iter()
            .any(|child| has_block(child, block_type))
}

fn find_block<'a>(node: &'a Il2Entity, block_type: &str) -> Option<&'a Il2Entity> {
    if node.block_type == block_type {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_block(child, block_type))
}

fn has_ground_unit(node: &Il2Entity) -> bool {
    matches!(node.block_type.as_str(), "Vehicle" | "Ship" | "Train")
        || node.children.iter().any(has_ground_unit)
}

fn prop_int(entity: &Il2Entity, key: &str) -> Option<i32> {
    entity.property(key)?.trim().parse().ok()
}

fn prop_f64(entity: &Il2Entity, key: &str) -> Option<f64> {
    entity.property(key)?.trim().parse().ok()
}

fn prop_bit(entity: &Il2Entity, key: &str) -> Option<bool> {
    match entity.property(key)?.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Korea aircraft the Airfield tab can place on a field or an air start.
/// Bombers and transports stay off the field.
const FLYABLE: &[&str] = &[
    "f51d", "f80c10", "f84e", "f86a5", "il10", "mig15bis", "la11", "yak9p",
];

/// Aircraft ids the bank can offer, in flyable order.
pub fn aircraft_ids() -> Vec<&'static str> {
    FLYABLE
        .iter()
        .copied()
        .filter(|id| aircraft_specs().any(|s| s.id == *id))
        .collect()
}

pub fn aircraft_label(type_id: &str) -> String {
    model_spec::spec_for(type_id)
        .map(|s| s.label.to_string())
        .unwrap_or_else(|| type_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;

    fn nato_pair() -> AirStartField {
        let mut field = default_field("AirStart", true);
        field.heading_deg = 90.0;
        field.planes.push(default_plane("f84e"));
        field.planes[1].altitude_m = 2500;
        field.planes[1].fuel = 1.0;
        field
    }

    #[test]
    fn field_matches_the_airstart_template() {
        let group = field_group(&nato_pair(), 355916.408, 331791.849, 4.013);
        let text = crate::serialize::serialize_group(&group);
        let root = parse_group_file(&text).expect("reparse");
        let air = root
            .children
            .iter()
            .find(|c| c.block_type == "Airfield")
            .expect("airfield");
        assert_eq!(air.name(), Some("AirStart"));
        assert_eq!(air.property("Country"), Some("601"));
        assert_eq!(
            air.property("Model"),
            Some(r#""graphics\airfields\fakefield.mgm""#)
        );
        assert_eq!(
            air.property("Script"),
            Some(r#""LuaScripts\WorldObjects\Airfields\fakefield.txt""#)
        );
        assert_eq!(air.property("YOri"), Some("90.000"));
        assert_eq!(air.property("MaintenanceRadius"), Some("10"));
        let planes = air
            .children
            .iter()
            .find(|c| c.block_type == "Planes")
            .expect("planes");
        assert_eq!(planes.children.len(), 2);
        assert_eq!(planes.children[0].property("Number"), Some("-1"));
        assert_eq!(planes.children[0].property("StartType"), Some("0"));
        assert_eq!(planes.children[0].property("Altitude"), Some("1500"));
        assert_eq!(planes.children[0].property("AILevel"), Some("2"));
        assert_eq!(planes.children[0].property("AIRTBDecision"), Some("1"));
        assert!(
            planes.children[0]
                .property("Script")
                .unwrap()
                .contains("f51d")
        );
        assert_eq!(planes.children[1].property("Altitude"), Some("2500"));
        assert_eq!(planes.children[1].property("Fuel"), Some("1"));
        let entity = root
            .children
            .iter()
            .find(|c| c.block_type == "MCU_TR_Entity")
            .expect("entity");
        assert_eq!(entity.property("MisObjID"), air.property("Index"));
        assert_eq!(air.property("LinkTrId"), entity.property("Index"));
        let y: f64 = entity.property("YPos").unwrap().parse().unwrap();
        assert!((y - 4.213).abs() < 0.001);
    }

    #[test]
    fn dprk_country_and_combined_group() {
        let mut a = default_field("North", false);
        a.planes[0].ai_level = 3;
        a.planes[0].renewable = false;
        let b = default_field("South", true);
        let placed = vec![
            PlacedAirStart {
                field: a,
                x: 1000.0,
                z: 2000.0,
            },
            PlacedAirStart {
                field: b,
                x: 3000.0,
                z: 4000.0,
            },
        ];
        let root = combined_group(&placed, &[10.0, 20.0]);
        assert_eq!(root.name(), Some("Air starts"));
        assert_eq!(root.count_block_type("Airfield"), 2);
        let north = root.find_by_name("North").unwrap();
        let air = north
            .children
            .iter()
            .find(|c| c.block_type == "Airfield")
            .unwrap();
        assert_eq!(air.property("Country"), Some("502"));
        let mig = air.children[0].children[0].property("Script").unwrap();
        assert!(mig.contains("mig15bis"), "{mig}");
        assert_eq!(air.children[0].children[0].property("AILevel"), Some("3"));
        assert_eq!(air.children[0].children[0].property("Renewable"), Some("0"));
        let y: f64 = air.property("YPos").unwrap().parse().unwrap();
        assert!((y - 10.0).abs() < 0.001);
    }

    #[test]
    fn aircraft_list_is_flyable_only() {
        assert_eq!(
            aircraft_ids(),
            vec![
                "f51d", "f80c10", "f84e", "f86a5", "il10", "mig15bis", "la11", "yak9p"
            ]
        );
    }

    #[test]
    fn finite_plane_count_writes_number() {
        let mut field = default_field("AirStart", true);
        field.planes[0].number = 6;
        let group = field_group(&field, 0.0, 0.0, 0.0);
        let air = group
            .children
            .iter()
            .find(|c| c.block_type == "Airfield")
            .unwrap();
        let plane = &air
            .children
            .iter()
            .find(|c| c.block_type == "Planes")
            .unwrap()
            .children[0];
        assert_eq!(plane.property("Number"), Some("6"));
    }

    #[test]
    fn parking_flag_does_not_change_an_air_start() {
        let mut field = default_field("AirStart", true);
        field.planes[0].parking_snap = true;
        let group = field_group(&field, 0.0, 0.0, 0.0);
        let air = group
            .children
            .iter()
            .find(|c| c.block_type == "Airfield")
            .unwrap();
        assert_eq!(
            air.property("Model"),
            Some(r#""graphics\airfields\fakefield.mgm""#)
        );
        assert!(!air.property("Model").unwrap().contains("rnwspawn"));
        let plane = &air
            .children
            .iter()
            .find(|c| c.block_type == "Planes")
            .unwrap()
            .children[0];
        assert_eq!(plane.property("StartType"), Some("0"));
        assert_eq!(plane.property("SnapTo"), Some("0"));
        assert_eq!(plane.property("Altitude"), Some("1500"));
        assert_eq!(plane.property("Name"), Some("\"In air - Clean\""));
    }

    #[test]
    fn air_start_name_follows_the_load_and_keeps_a_typed_name() {
        let mut field = default_field("CAP North", true);
        field.planes[0].payload_id = 2;
        field.planes[0].fuel = 1.0;
        let group = field_group(&field, 10_000.0, 10_000.0, 0.0);
        let air = group
            .children
            .iter()
            .find(|c| c.block_type == "Airfield")
            .unwrap();
        let plane = &air
            .children
            .iter()
            .find(|c| c.block_type == "Planes")
            .unwrap()
            .children[0];
        assert_eq!(
            plane.property("Name"),
            Some("\"In air - Strike 2 - Long range\"")
        );
        let read = read_mission(&group);
        assert!(read.air_starts[0].field.planes[0].name.is_empty());

        field.planes[0].name = "Dawn patrol".into();
        let named = field_group(&field, 10_000.0, 10_000.0, 0.0);
        let again = read_mission(&named);
        assert_eq!(again.air_starts[0].field.planes[0].name, "Dawn patrol");
        let rewritten = field_group(&again.air_starts[0].field, 10_000.0, 10_000.0, 0.0);
        let kept = rewritten
            .children
            .iter()
            .find(|c| c.block_type == "Airfield")
            .unwrap()
            .children
            .iter()
            .find(|c| c.block_type == "Planes")
            .unwrap()
            .children[0]
            .property("Name");
        assert_eq!(kept, Some("\"Dawn patrol\""));
    }

    #[test]
    fn read_mission_round_trips_air_starts() {
        let mut north = default_field("CAP North", false);
        north.heading_deg = 45.0;
        north.planes[0].number = 4;
        north.planes[0].altitude_m = 2200;
        north.planes[0].ai_level = 3;
        north.planes[0].renewable = false;
        north.planes[0].fuel = 0.4;
        let south = default_field("CAP South", true);
        let placed = vec![
            PlacedAirStart {
                field: north,
                x: 120_000.0,
                z: 240_000.0,
            },
            PlacedAirStart {
                field: south,
                x: 130_000.0,
                z: 250_000.0,
            },
        ];
        let group = combined_group(&placed, &[15.0, 20.0]);
        let text = crate::serialize::serialize_group(&group);
        let root = parse_group_file(&text).expect("reparse");
        assert!(is_air_start_group(&root));
        let read = read_mission(&root);
        assert!(read.fields.is_empty());
        assert_eq!(read.air_starts.len(), 2);
        let cap = &read.air_starts[0];
        assert_eq!(cap.field.name, "CAP North");
        assert!(!cap.field.nato);
        assert!((cap.x - 120_000.0).abs() < 0.01);
        assert!((cap.z - 240_000.0).abs() < 0.01);
        assert!((cap.field.heading_deg - 45.0).abs() < 0.01);
        assert_eq!(cap.field.planes[0].type_id, "mig15bis");
        assert_eq!(cap.field.planes[0].number, 4);
        assert_eq!(cap.field.planes[0].altitude_m, 2200);
        assert_eq!(cap.field.planes[0].ai_level, 3);
        assert!(!cap.field.planes[0].renewable);
        assert!((cap.field.planes[0].fuel - 0.4).abs() < 0.001);
        assert!(!cap.field.planes[0].parking_snap);
        assert_eq!(read.air_starts[1].field.name, "CAP South");
        assert!(read.air_starts[1].field.nato);
        assert_eq!(read.air_starts[1].field.planes[0].type_id, "f51d");

        let mut bank = vec![default_field("CAP North", true)];
        merge_bank(&mut bank, &read.air_starts);
        assert_eq!(bank.len(), 2);
        assert!(!bank[0].nato);
        assert_eq!(bank[0].planes[0].type_id, "mig15bis");
        merge_bank(&mut bank, &read.air_starts);
        assert_eq!(bank.len(), 2);
    }

    #[test]
    fn read_mission_keeps_defined_airfields_with_their_aircraft() {
        let mut field = default_field("ImportField", true);
        field.planes[0] = default_plane("f86a5");
        field.planes[0].parking_snap = true;
        field.planes[0].payload_id = 2;
        let mut group = field_group(&field, 1_000.0, 2_000.0, 10.0);
        let air = group
            .children
            .iter_mut()
            .find(|child| child.block_type == "Airfield")
            .unwrap();
        air.set_property("Model", r#""graphics\airfields\fakefield_rnwspawn.mgm""#);
        air.set_property(
            "Script",
            r#""LuaScripts\WorldObjects\Airfields\fakefield_rnwspawn.txt""#,
        );
        let plane = air
            .children
            .iter_mut()
            .find(|child| child.block_type == "Planes")
            .unwrap()
            .children
            .get_mut(0)
            .unwrap();
        plane.set_property("StartType", "2");
        plane.set_property("SnapTo", "2");
        plane.set_property("Altitude", "0");
        group.children.push(Il2Entity::new("MCU_TR_TaxiGraph"));

        let text = crate::serialize::serialize_group(&group);
        let root = parse_group_file(&text).expect("reparse");
        assert!(!is_air_start_group(&root));
        let read = read_mission(&root);
        assert!(read.air_starts.is_empty());
        assert_eq!(read.fields.len(), 1);
        assert_eq!(read.fields[0].planes[0].type_id, "f86a5");
        assert!(read.fields[0].planes[0].parking_snap);
        assert_eq!(read.fields[0].planes[0].payload_id, 2);
        assert_eq!(read.fields[0].planes[0].altitude_m, 0);

        let mut parked = field_group(&default_field("Suwon", false), 3.0, 4.0, 0.0);
        parked.children.push(Il2Entity::new("MCU_TR_TaxiGraph"));
        let parked = read_mission(&parked);
        assert!(parked.air_starts.is_empty());
        assert_eq!(parked.fields.len(), 1);
        assert_eq!(parked.fields[0].planes[0].type_id, "mig15bis");
    }

    #[test]
    fn changing_type_resets_loadout() {
        let mut plane = default_plane("f51d");
        plane.payload_id = 4;
        plane.mod_mask = "999".into();
        apply_type(&mut plane, "f86a5");
        assert_eq!(plane.type_id, "f86a5");
        assert_eq!(plane.payload_id, 0);
        assert_ne!(plane.mod_mask, "999");
        apply_type(&mut plane, "f86a5");
        assert_eq!(plane.payload_id, 0);
        assert!((plane.fuel - 1.0).abs() < 1e-9);
        apply_type(&mut plane, "f51d");
        assert!((plane.fuel - 0.65).abs() < 1e-9);
    }

    #[test]
    fn fuel_defaults_full_except_the_f51() {
        assert!((default_plane("f51d").fuel - 0.65).abs() < 1e-9);
        assert!((default_plane("mig15bis").fuel - 1.0).abs() < 1e-9);
        assert!((default_plane("f86a5").fuel - 1.0).abs() < 1e-9);
        assert!((default_plane("f80c10").fuel - 1.0).abs() < 1e-9);
    }
}
