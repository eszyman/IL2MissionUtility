//! Korea-war max / max-effective ranges for catalog systems (metres).
//! Used to park groups within reach of a map objective and to size ground
//! AttackArea engage radii.

use crate::ast::Il2Entity;

/// Exact script type-id (filename without `.txt`) → metres.
/// High end of a published band (e.g. Sherman 1500–2000 → 2000).
const RANGES: &[(&str, f64)] = &[
    // Artillery (max range)
    ("m2a1-105mm", 11_430.0),
    ("m101", 11_430.0),
    ("m114", 14_600.0),
    ("m2-8inch", 16_800.0),
    ("m115", 16_800.0),
    ("m2-longtom", 23_500.0),
    ("m1a1-155mm", 23_500.0),
    ("studebakerus6-bm13", 8_470.0),
    ("bm13", 8_470.0),
    ("bm12", 8_000.0),
    ("zis3", 13_290.0),
    ("ml20", 17_230.0),
    ("m30", 11_800.0),
    // Armor (max effective)
    ("m4a3e8", 2_000.0),
    ("sherman", 2_000.0),
    ("m46", 2_000.0),
    ("m26", 2_000.0),
    ("patton", 2_000.0),
    ("centurion", 3_000.0),
    ("t34-85", 1_500.0),
    ("t-34-85", 1_500.0),
    // Machine guns (max effective)
    ("m1919", 1_400.0),
    ("m1917", 1_400.0),
    ("squad-mg-1950-usa", 1_400.0),
    ("m2cal50", 2_000.0),
    ("dshk", 2_000.0),
    ("squad-mg-1950-dprk", 1_000.0),
    ("squad-mg-1950-prc", 1_000.0),
    ("sg-aa", 1_000.0),
    ("sg-43", 1_000.0),
    ("sg", 1_000.0),
];

/// Fallback when the template is artillery but no script matched.
pub const UNKNOWN_ARTILLERY_M: f64 = 15_000.0;
/// Fallback when the template is armor but no script matched.
pub const UNKNOWN_ARMOR_M: f64 = 2_000.0;
/// Known guns at or above this are artillery; shorter is armor / MG.
pub const ARTILLERY_RANGE_MIN_M: f64 = 4_500.0;

/// Map / Army Generator unit class inferred from the group, not the UI icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmyUnitKind {
    Ship,
    Armor,
    Supply,
    Artillery,
    Train,
    MobileArtillery,
}

impl ArmyUnitKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ship => "Ship",
            Self::Armor => "Armor",
            Self::Supply => "Supply",
            Self::Artillery => "Artillery",
            Self::Train => "Train",
            Self::MobileArtillery => "Mobile artillery",
        }
    }
}

fn type_id(script: &str) -> String {
    script
        .trim_matches('"')
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(script)
        .trim_end_matches(".txt")
        .trim_matches('"')
        .to_ascii_lowercase()
}

/// Max range for a single Script / Model path, if known.
pub fn range_for_script(script: &str) -> Option<f64> {
    let key = type_id(script);
    if key.contains("mortar") {
        return None;
    }
    let mut best: Option<(usize, f64)> = None;
    for &(id, metres) in RANGES {
        if key == id {
            return Some(metres);
        }
        if key.contains(id) {
            let take = best.map(|(len, _)| id.len() > len).unwrap_or(true);
            if take {
                best = Some((id.len(), metres));
            }
        }
    }
    best.map(|(_, r)| r)
}

/// Longest known system in this group (a battery plus AAA uses the guns).
pub fn group_weapon_range(root: &Il2Entity) -> Option<f64> {
    let mut max = None;
    root.for_each(&mut |e| {
        if let Some(script) = e.property("Script") {
            if let Some(r) = range_for_script(script) {
                max = Some(max.map(|m: f64| m.max(r)).unwrap_or(r));
            }
        }
    });
    max
}

/// Engage radius around the target: never larger than the default 3 km bubble,
/// but MGs/armor shrink to their effective range.
pub fn attack_area_radius_m(script: &str, default_m: f32) -> f32 {
    range_for_script(script)
        .map(|r| (r as f32).min(default_m))
        .unwrap_or(default_m)
}

/// Suggested AttackArea for one or more assigned units: shortest known
/// system range, capped at `default_m`. Unknown scripts keep the default.
pub fn suggested_attack_area_m<'a>(
    scripts: impl IntoIterator<Item = &'a str>,
    default_m: f32,
) -> f32 {
    let mut area = default_m;
    for script in scripts {
        area = area.min(attack_area_radius_m(script, default_m));
    }
    area
}

/// Shortest known weapon range among `scripts`.
pub fn shortest_range_m<'a>(scripts: impl IntoIterator<Item = &'a str>) -> Option<f64> {
    let mut min = None;
    for script in scripts {
        if let Some(r) = range_for_script(script) {
            min = Some(min.map(|m: f64| m.min(r)).unwrap_or(r));
        }
    }
    min
}

/// If `area_m` is larger than a known assigned weapon, return that range.
pub fn area_exceeds_range<'a>(
    scripts: impl IntoIterator<Item = &'a str>,
    area_m: f32,
) -> Option<f64> {
    shortest_range_m(scripts).filter(|&r| f64::from(area_m) > r + 0.5)
}

/// MCU_CMD_AttackArea that hunts ground or ground targets (not air-only AAA).
pub fn is_ground_attack_area(entity: &Il2Entity) -> bool {
    if entity.block_type != "MCU_CMD_AttackArea" {
        return false;
    }
    let on = |key: &str| {
        entity
            .property(key)
            .is_some_and(|v| v.trim_matches('"') == "1")
    };
    on("AttackGround") || on("AttackGTargets")
}

/// True if any descendant is a ground / ground-target AttackArea.
pub fn group_has_ground_attack_area(root: &Il2Entity) -> bool {
    let mut found = false;
    root.for_each(&mut |e| {
        if is_ground_attack_area(e) {
            found = true;
        }
    });
    found
}

/// Infer Ship / Armor / Supply / Artillery / Train from objects, scripts, and AttackArea.
///
/// A ground-target AttackArea plus a long (or unknown) gun is artillery.
/// Known short-range guns are armor even when the area hunts air only.
/// A long gun without a ground area (a Katyusha in a truck column) stays supply.
/// A perfect column of artillery is mobile artillery (parked on roads).
/// `Train` objects always classify as trains (parked on rails).
pub fn classify_army_unit(root: &Il2Entity) -> ArmyUnitKind {
    if root.count_block_type("Ship") > 0 {
        return ArmyUnitKind::Ship;
    }
    if root.count_block_type("Train") > 0 {
        return ArmyUnitKind::Train;
    }
    let range = group_weapon_range(root);
    let hunts_ground = group_has_ground_attack_area(root);
    let long = range.is_some_and(|r| r >= ARTILLERY_RANGE_MIN_M);
    let kind = if hunts_ground && (long || range.is_none()) {
        ArmyUnitKind::Artillery
    } else if range.is_some_and(|r| r < ARTILLERY_RANGE_MIN_M) {
        ArmyUnitKind::Armor
    } else {
        ArmyUnitKind::Supply
    };
    if kind == ArmyUnitKind::Artillery {
        if let Some(route) = crate::mapnet::inspect_route(root) {
            if !route.rail {
                return ArmyUnitKind::MobileArtillery;
            }
        }
    }
    kind
}

/// Move ground / ground-target AttackArea MCUs onto `xz`. Air-only areas stay.
pub fn snap_ground_attack_areas(entity: &mut Il2Entity, x: f64, z: f64) {
    entity.for_each_mut(&mut |e| {
        if is_ground_attack_area(e) {
            e.set_property("XPos", format!("{x:.3}"));
            e.set_property("ZPos", format!("{z:.3}"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Il2Entity;

    #[test]
    fn known_scripts_match_published_ranges() {
        assert_eq!(
            range_for_script(r#"LuaScripts\WorldObjects\fixedobjects\ml20.txt"#),
            Some(17_230.0)
        );
        assert_eq!(
            range_for_script(r#""LuaScripts\WorldObjects\vehicles\studebakerus6-bm13.txt""#),
            Some(8_470.0)
        );
        assert_eq!(
            range_for_script("LuaScripts\\WorldObjects\\fixedobjects\\m2a1-105mm.txt"),
            Some(11_430.0)
        );
        assert_eq!(
            range_for_script("fixedobjects\\m30.txt"),
            Some(11_800.0)
        );
        assert_eq!(
            range_for_script("fixedobjects\\m30-mortar.txt"),
            None
        );
        assert_eq!(
            range_for_script("vehicles\\t34-85.txt"),
            Some(1_500.0)
        );
        assert_eq!(
            range_for_script("fixedobjects\\dshk.txt"),
            Some(2_000.0)
        );
        assert_eq!(
            range_for_script("vehicles\\studebakerus6.txt"),
            None
        );
    }

    #[test]
    fn group_takes_longest_gun() {
        let mut root = Il2Entity::new("Group");
        let mut gun = Il2Entity::new("Vehicle");
        gun.set_property("Script", r#"LuaScripts\WorldObjects\fixedobjects\ml20.txt"#);
        let mut mg = Il2Entity::new("Vehicle");
        mg.set_property("Script", r#"LuaScripts\WorldObjects\fixedobjects\dshk.txt"#);
        root.children.push(gun);
        root.children.push(mg);
        assert_eq!(group_weapon_range(&root), Some(17_230.0));
    }

    #[test]
    fn mg_attack_area_shrinks_to_effective_range() {
        assert_eq!(attack_area_radius_m("fixedobjects\\dshk.txt", 3000.0), 2000.0);
        assert_eq!(attack_area_radius_m("fixedobjects\\ml20.txt", 3000.0), 3000.0);
        assert_eq!(
            attack_area_radius_m("fixedobjects\\squad-mg-1950-dprk.txt", 3000.0),
            1000.0
        );
        assert_eq!(
            suggested_attack_area_m(
                [
                    "vehicles\\t34-85.txt",
                    "fixedobjects\\squad-mg-1950-dprk.txt"
                ],
                3000.0
            ),
            1000.0
        );
        assert_eq!(
            area_exceeds_range(["fixedobjects\\squad-mg-1950-dprk.txt"], 3000.0),
            Some(1000.0)
        );
        assert!(area_exceeds_range(["fixedobjects\\squad-mg-1950-dprk.txt"], 1000.0).is_none());
    }

    fn group_with(script: Option<&str>, block: &str, ground_area: bool) -> Il2Entity {
        let mut root = Il2Entity::new("Group");
        root.set_name("Unit");
        let mut obj = Il2Entity::new(block);
        if let Some(script) = script {
            obj.set_property("Script", script);
        }
        obj.set_property("XPos", "10.000");
        obj.set_property("ZPos", "20.000");
        root.children.push(obj);
        if ground_area {
            let mut area = Il2Entity::new("MCU_CMD_AttackArea");
            area.set_property("AttackGround", "1");
            area.set_property("AttackGTargets", "0");
            root.children.push(area);
        }
        root
    }

    #[test]
    fn classify_uses_ships_range_and_ground_attack_area() {
        let ship = group_with(
            Some(r#"LuaScripts\WorldObjects\Ships\seiner-gunboat.txt"#),
            "Ship",
            false,
        );
        assert_eq!(classify_army_unit(&ship), ArmyUnitKind::Ship);

        let tank = group_with(
            Some(r#"LuaScripts\WorldObjects\vehicles\t34-85.txt"#),
            "Vehicle",
            false,
        );
        assert_eq!(classify_army_unit(&tank), ArmyUnitKind::Armor);

        let tank_hunting = group_with(
            Some(r#"LuaScripts\WorldObjects\vehicles\t34-85.txt"#),
            "Vehicle",
            true,
        );
        assert_eq!(classify_army_unit(&tank_hunting), ArmyUnitKind::Armor);

        let ml20 = group_with(
            Some(r#"LuaScripts\WorldObjects\fixedobjects\ml20.txt"#),
            "Vehicle",
            true,
        );
        assert_eq!(classify_army_unit(&ml20), ArmyUnitKind::Artillery);

        let cargo_katyusha = group_with(
            Some(r#"LuaScripts\WorldObjects\vehicles\studebakerus6-bm13.txt"#),
            "Vehicle",
            false,
        );
        assert_eq!(classify_army_unit(&cargo_katyusha), ArmyUnitKind::Supply);

        let battery = group_with(
            Some(r#"LuaScripts\WorldObjects\vehicles\studebakerus6-bm13.txt"#),
            "Vehicle",
            true,
        );
        assert_eq!(classify_army_unit(&battery), ArmyUnitKind::Artillery);

        let truck = group_with(
            Some(r#"LuaScripts\WorldObjects\vehicles\gaz63.txt"#),
            "Vehicle",
            false,
        );
        assert_eq!(classify_army_unit(&truck), ArmyUnitKind::Supply);

        let unknown_ground = group_with(None, "Vehicle", true);
        assert_eq!(classify_army_unit(&unknown_ground), ArmyUnitKind::Artillery);

        let train = group_with(None, "Train", false);
        assert_eq!(classify_army_unit(&train), ArmyUnitKind::Train);
    }

    #[test]
    fn snap_moves_ground_not_air_attack_area() {
        let mut root = Il2Entity::new("Group");
        let mut ground = Il2Entity::new("MCU_CMD_AttackArea");
        ground.set_property("AttackGround", "1");
        ground.set_property("AttackAir", "0");
        ground.set_property("XPos", "1.000");
        ground.set_property("ZPos", "2.000");
        let mut air = Il2Entity::new("MCU_CMD_AttackArea");
        air.set_property("AttackGround", "0");
        air.set_property("AttackAir", "1");
        air.set_property("XPos", "3.000");
        air.set_property("ZPos", "4.000");
        root.children.push(ground);
        root.children.push(air);
        snap_ground_attack_areas(&mut root, 100_000.0, 200_000.0);
        assert_eq!(root.children[0].property("XPos"), Some("100000.000"));
        assert_eq!(root.children[0].property("ZPos"), Some("200000.000"));
        assert_eq!(root.children[1].property("XPos"), Some("3.000"));
    }
}
