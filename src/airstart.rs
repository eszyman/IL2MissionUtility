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
//! * `COUNTRY_NATO` (601) / `COUNTRY_DPRK` (502)
//!
//! ## Used by
//! * ui.rs (Airfield) — the bank and its export
//! * ui/map.rs — place, move, turn, and stamp into the base map

use crate::ast::{format_int_array, Il2Entity};
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
        if self.nato {
            "NATO"
        } else {
            "DPRK"
        }
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
        fuel: 0.65,
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

/// Reset payload and mods when the aircraft type changes.
pub fn apply_type(plane: &mut AirStartPlane, type_id: &str) {
    if plane.type_id == type_id {
        return;
    }
    let fresh = default_plane(type_id);
    plane.type_id = fresh.type_id;
    plane.payload_id = fresh.payload_id;
    plane.mod_mask = fresh.mod_mask;
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

fn append_field(parent: &mut Il2Entity, field: &AirStartField, x: f64, y: f64, z: f64, next_id: &mut i32) {
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
    e.set_property("Name", "\"\"");
    e.set_property("Skin", "\"\"");
    e.set_property("BotSkin", "\"\"");
    e.set_property("AvMods", "\"\"");
    e.set_property("AvSkins", "\"\"");
    e.set_property("AvPayloads", "\"\"");
    e.set_property("AvBotSkins", "\"\"");
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
    if on {
        "1"
    } else {
        "0"
    }
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

/// Flyable Korea aircraft. AI-only types (F-84, bombers, transports) stay off the field.
const FLYABLE: &[&str] = &["f51d", "f80c10", "f86a5", "il10", "mig15bis", "la11", "yak9p"];

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
        assert_eq!(air.property("Model"), Some(r#""graphics\airfields\fakefield.mgm""#));
        assert_eq!(air.property("Script"), Some(r#""LuaScripts\WorldObjects\Airfields\fakefield.txt""#));
        assert_eq!(air.property("YOri"), Some("90.000"));
        assert_eq!(air.property("MaintenanceRadius"), Some("10"));
        let planes = air.children.iter().find(|c| c.block_type == "Planes").expect("planes");
        assert_eq!(planes.children.len(), 2);
        assert_eq!(planes.children[0].property("Number"), Some("-1"));
        assert_eq!(planes.children[0].property("StartType"), Some("0"));
        assert_eq!(planes.children[0].property("Altitude"), Some("1500"));
        assert_eq!(planes.children[0].property("AILevel"), Some("2"));
        assert_eq!(planes.children[0].property("AIRTBDecision"), Some("1"));
        assert!(planes.children[0].property("Script").unwrap().contains("f51d"));
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
            PlacedAirStart { field: a, x: 1000.0, z: 2000.0 },
            PlacedAirStart { field: b, x: 3000.0, z: 4000.0 },
        ];
        let root = combined_group(&placed, &[10.0, 20.0]);
        assert_eq!(root.name(), Some("Air starts"));
        assert_eq!(root.count_block_type("Airfield"), 2);
        let north = root.find_by_name("North").unwrap();
        let air = north.children.iter().find(|c| c.block_type == "Airfield").unwrap();
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
            vec!["f51d", "f80c10", "f86a5", "il10", "mig15bis", "la11", "yak9p"]
        );
    }

    #[test]
    fn finite_plane_count_writes_number() {
        let mut field = default_field("AirStart", true);
        field.planes[0].number = 6;
        let group = field_group(&field, 0.0, 0.0, 0.0);
        let air = group.children.iter().find(|c| c.block_type == "Airfield").unwrap();
        let plane = &air.children.iter().find(|c| c.block_type == "Planes").unwrap().children[0];
        assert_eq!(plane.property("Number"), Some("6"));
    }

    #[test]
    fn parking_flag_does_not_change_an_air_start() {
        let mut field = default_field("AirStart", true);
        field.planes[0].parking_snap = true;
        let group = field_group(&field, 0.0, 0.0, 0.0);
        let air = group.children.iter().find(|c| c.block_type == "Airfield").unwrap();
        assert_eq!(air.property("Model"), Some(r#""graphics\airfields\fakefield.mgm""#));
        assert!(!air.property("Model").unwrap().contains("rnwspawn"));
        let plane = &air.children.iter().find(|c| c.block_type == "Planes").unwrap().children[0];
        assert_eq!(plane.property("StartType"), Some("0"));
        assert_eq!(plane.property("SnapTo"), Some("0"));
        assert_eq!(plane.property("Altitude"), Some("1500"));
        assert_eq!(plane.property("Name"), Some("\"\""));
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
    }
}
