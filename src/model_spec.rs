//! Per-model type, cruise speed, ceiling, notes, and preview image lookup.
//!
//! The AST stays schema-agnostic; this table is a UI overlay keyed by the
//! script filename (type-id), not by parsing extra Group keys.

include!(concat!(env!("OUT_DIR"), "/model_images.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelClass {
    Fighter,
    FighterBomber,
    Attack,
    Bomber,
    Transport,
    Armor,
    TankDestroyer,
    Spg,
    LightAaa,
    Truck,
    Jeep,
    Tractor,
    Infantry,
    RocketArtillery,
    LightFlak,
    HeavyFlak,
    Artillery,
    Mortar,
    MachineGun,
    Radar,
    Airfield,
    Dummy,
    CargoShip,
    Destroyer,
    TorpedoBoat,
    LandingCraft,
    Gunboat,
    SmallBoat,
    Locomotive,
    Unknown,
}

impl ModelClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fighter => "Fighter",
            Self::FighterBomber => "Fighter-bomber",
            Self::Attack => "Attack",
            Self::Bomber => "Bomber",
            Self::Transport => "Transport",
            Self::Armor => "Armor",
            Self::TankDestroyer => "Tank destroyer",
            Self::Spg => "SPG",
            Self::LightAaa => "Light AAA",
            Self::Truck => "Truck",
            Self::Jeep => "Jeep",
            Self::Tractor => "Tractor",
            Self::Infantry => "Infantry",
            Self::RocketArtillery => "Rocket artillery",
            Self::LightFlak => "Light flak",
            Self::HeavyFlak => "Heavy flak",
            Self::Artillery => "Artillery",
            Self::Mortar => "Mortar",
            Self::MachineGun => "Machine gun",
            Self::Radar => "Radar",
            Self::Airfield => "Airfield",
            Self::Dummy => "Dummy",
            Self::CargoShip => "Cargo",
            Self::Destroyer => "Destroyer",
            Self::TorpedoBoat => "Torpedo boat",
            Self::LandingCraft => "Landing craft",
            Self::Gunboat => "Gunboat",
            Self::SmallBoat => "Small boat",
            Self::Locomotive => "Locomotive",
            Self::Unknown => "Unknown",
        }
    }

    fn sort_key(self) -> u8 {
        match self {
            Self::Fighter => 0,
            Self::FighterBomber => 1,
            Self::Attack => 2,
            Self::Bomber => 3,
            Self::Transport => 4,
            Self::Armor => 5,
            Self::TankDestroyer => 6,
            Self::Spg => 7,
            Self::LightAaa => 8,
            Self::RocketArtillery => 9,
            Self::Truck => 10,
            Self::Jeep => 11,
            Self::Tractor => 12,
            Self::Infantry => 13,
            Self::LightFlak => 14,
            Self::HeavyFlak => 15,
            Self::Artillery => 16,
            Self::Mortar => 17,
            Self::MachineGun => 18,
            Self::Radar => 19,
            Self::Airfield => 20,
            Self::Dummy => 21,
            Self::CargoShip => 22,
            Self::Destroyer => 23,
            Self::TorpedoBoat => 24,
            Self::LandingCraft => 25,
            Self::Gunboat => 26,
            Self::SmallBoat => 27,
            Self::Locomotive => 28,
            Self::Unknown => 29,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub class: ModelClass,
    /// Typical cruise speed. `Some(0.0)` is stationary (fixed guns, flags).
    pub cruise_kmh: Option<f32>,
    /// Service ceiling in meters. 0 = not applicable (ground/ship units).
    pub ceiling_m: f32,
    /// Free-form notes shown in the UI. Empty string = no notes yet.
    pub notes: &'static str,
}

impl ModelSpec {
    pub fn cruise_line(self) -> String {
        format_cruise(self.cruise_kmh)
    }
}

const SPECS: &[ModelSpec] = &[
    // Planes
    spec("b29", "B-29 Superfortress", ModelClass::Bomber, 350.0, 10668.0, ""),
    spec("simpleb29", "B-29 (simple)", ModelClass::Bomber, 350.0, 10668.0, ""),
    spec("c47b", "C-47B Skytrain", ModelClass::Transport, 257.0, 6096.0, ""),
    spec("f51d", "F-51D Mustang", ModelClass::Fighter, 583.0, 11278.0, ""),
    spec("f80c10", "F-80C-10 Shooting Star", ModelClass::Fighter, 732.0, 13716.0, ""),
    spec("f84e", "F-84E Thunderjet", ModelClass::FighterBomber, 770.0, 12192.0, ""),
    spec("f86a5", "F-86A-5 Sabre", ModelClass::Fighter, 800.0, 15240.0, ""),
    spec("il10", "Il-10", ModelClass::Attack, 436.0, 6950.0, ""),
    spec("la11", "La-11", ModelClass::Fighter, 500.0, 10150.0, ""),
    spec("li2t", "Li-2T", ModelClass::Transport, 240.0, 6800.0, ""),
    spec("mig15bis", "MiG-15bis", ModelClass::Fighter, 850.0, 15000.0, ""),
    spec("tu2", "Tu-2", ModelClass::Bomber, 465.0, 10500.0, ""),
    spec("yak9p", "Yak-9P", ModelClass::Fighter, 554.0, 10600.0, ""),
    // Vehicles
    spec("ba64b", "BA-64B", ModelClass::Armor, 80.0, 0.0, ""),
    spec("dodgewc52", "Dodge WC-52", ModelClass::Jeep, 80.0, 0.0, ""),
    spec("dodgewc54", "Dodge WC-54", ModelClass::Truck, 80.0, 0.0, ""),
    spec("gaz55", "GAZ-55", ModelClass::Truck, 70.0, 0.0, ""),
    spec("gaz63", "GAZ-63", ModelClass::Truck, 65.0, 0.0, ""),
    spec("gaz67b", "GAZ-67B", ModelClass::Jeep, 90.0, 0.0, ""),
    spec("gmc-cckw", "GMC CCKW", ModelClass::Truck, 72.0, 0.0, ""),
    spec("gmc-cckw-refueler", "GMC CCKW Refueler", ModelClass::Truck, 72.0, 0.0, ""),
    spec("is2", "IS-2", ModelClass::Armor, 37.0, 0.0, ""),
    spec("isu122", "ISU-122", ModelClass::TankDestroyer, 35.0, 0.0, ""),
    spec("m16-mgmc", "M16 MGMC", ModelClass::LightAaa, 64.0, 0.0, ""),
    spec("m19", "M19", ModelClass::LightAaa, 56.0, 0.0, ""),
    spec("m3a1-halftrack", "M3A1 Halftrack", ModelClass::Armor, 72.0, 0.0, ""),
    spec("m40", "M40", ModelClass::Spg, 38.0, 0.0, ""),
    spec("m46", "M46 Patton", ModelClass::Armor, 48.0, 0.0, ""),
    spec("m4a3e8", "M4A3E8 Sherman", ModelClass::Armor, 42.0, 0.0, ""),
    spec("m6-tractor", "M6 Tractor", ModelClass::Tractor, 48.0, 0.0, ""),
    spec("m7b1", "M7B1 Priest", ModelClass::Spg, 40.0, 0.0, ""),
    spec("squad-mg-1950-dprk", "DPRK MG squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-mg-1950-prc", "PRC MG squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-mg-1950-usa", "USA MG squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-rifle-1950-dprk", "DPRK Rifle squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-rifle-1950-prc", "PRC Rifle squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-rifle-1950-usa", "USA Rifle squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-smg-1950-dprk", "DPRK SMG squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-smg-1950-prc", "PRC SMG squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("squad-smg-1950-usa", "USA SMG squad", ModelClass::Infantry, 5.0, 0.0, ""),
    spec("studebakerus6", "Studebaker US6", ModelClass::Truck, 70.0, 0.0, ""),
    spec("studebakerus6-bm13", "BM-13 Katyusha", ModelClass::RocketArtillery, 70.0, 0.0, ""),
    spec("studebakerus6-refueler", "Studebaker US6 Refueler", ModelClass::Truck, 70.0, 0.0, ""),
    spec("studebakerus6-tanker", "Studebaker US6 Tanker", ModelClass::Truck, 70.0, 0.0, ""),
    spec("su76m", "SU-76M", ModelClass::TankDestroyer, 45.0, 0.0, ""),
    spec("t34-85", "T-34-85", ModelClass::Armor, 55.0, 0.0, ""),
    spec("u7144", "U-7144", ModelClass::Truck, 60.0, 0.0, ""),
    spec("willysmb", "Willys MB", ModelClass::Jeep, 105.0, 0.0, "Grandpa used to build these in Toledo"),
    // Trains
    spec("type475-1", "Type 475-1", ModelClass::Locomotive, 50.0, 0.0, ""),
    spec("usatc-s160", "USATC S160", ModelClass::Locomotive, 65.0, 0.0, ""),
    // Ships
    spec("cargoship1", "Cargo ship", ModelClass::CargoShip, 22.0, 0.0, ""),
    spec("cargoship2", "Cargo ship (2)", ModelClass::CargoShip, 22.0, 0.0, ""),
    spec("g5", "G-5", ModelClass::TorpedoBoat, 98.0, 0.0, ""),
    spec("gleaves", "Gleaves-class", ModelClass::Destroyer, 69.0, 0.0, ""),
    spec("lci", "LCI", ModelClass::LandingCraft, 26.0, 0.0, ""),
    spec("lcvp", "LCVP", ModelClass::LandingCraft, 22.0, 0.0, ""),
    spec("libertyship", "Liberty ship", ModelClass::CargoShip, 21.0, 0.0, ""),
    spec("lst", "LST", ModelClass::LandingCraft, 20.0, 0.0, ""),
    spec("pinnace", "Pinnace", ModelClass::SmallBoat, 15.0, 0.0, ""),
    spec("pinnace-gunboat", "Pinnace gunboat", ModelClass::Gunboat, 15.0, 0.0, ""),
    spec("pinnace-landingcraft", "Pinnace landing craft", ModelClass::LandingCraft, 15.0, 0.0, ""),
    spec("sampan", "Sampan", ModelClass::SmallBoat, 10.0, 0.0, ""),
    spec("sampan-landingcraft", "Sampan landing craft", ModelClass::LandingCraft, 10.0, 0.0, ""),
    spec("seiner", "Seiner", ModelClass::SmallBoat, 15.0, 0.0, ""),
    spec("seiner-gunboat", "Seiner gunboat", ModelClass::Gunboat, 15.0, 0.0, ""),
    spec("tankership", "Tanker", ModelClass::CargoShip, 22.0, 0.0, ""),
    // Fixed — stationary
    spec("61k", "61-K 37 mm", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("72k", "72-K 25 mm", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("boforsl60", "Bofors L60", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("dshk-aa", "DShK AA", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("m1919-aa", "M1919 AA", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("m2cal50-aa", "M2 .50 AA", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("sg-aa", "SG AA", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("zpuvz53", "ZPU / VZ-53", ModelClass::LightFlak, 0.0, 0.0, ""),
    spec("ks12", "KS-12 85 mm", ModelClass::HeavyFlak, 0.0, 0.0, ""),
    spec("ks19", "KS-19 100 mm", ModelClass::HeavyFlak, 0.0, 0.0, ""),
    spec("m1aaa120", "M1 120 mm AA", ModelClass::HeavyFlak, 0.0, 0.0, ""),
    spec("m2aaa90", "M2 90 mm AA", ModelClass::HeavyFlak, 0.0, 0.0, ""),
    spec("s60", "S-60 57 mm", ModelClass::HeavyFlak, 0.0, 0.0, ""),
    spec("m1a1-155mm", "M1A1 155 mm", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("m2-8inch", "M2 8-inch", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("m2a1-105mm", "M2A1 105 mm", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("m2-longtom", "M2 Long Tom", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("m30", "M-30 122 mm", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("ml20", "ML-20 152 mm", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("zis3", "ZiS-3 76 mm", ModelClass::Artillery, 0.0, 0.0, ""),
    spec("bm37", "BM-37 mortar", ModelClass::Mortar, 0.0, 0.0, ""),
    spec("m29-mortar", "M29 mortar", ModelClass::Mortar, 0.0, 0.0, ""),
    spec("m30-mortar", "M30 mortar", ModelClass::Mortar, 0.0, 0.0, ""),
    spec("pm43", "PM-43 mortar", ModelClass::Mortar, 0.0, 0.0, ""),
    spec("dshk", "DShK", ModelClass::MachineGun, 0.0, 0.0, ""),
    spec("m1919", "M1919", ModelClass::MachineGun, 0.0, 0.0, ""),
    spec("m2cal50", "M2 .50", ModelClass::MachineGun, 0.0, 0.0, ""),
    spec("sg", "SG", ModelClass::MachineGun, 0.0, 0.0, ""),
    spec("ancps1", "AN/CPS-1", ModelClass::Radar, 0.0, 0.0, ""),
    spec("ancps4", "AN/CPS-4", ModelClass::Radar, 0.0, 0.0, ""),
    spec("ancps6b", "AN/CPS-6B", ModelClass::Radar, 0.0, 0.0, ""),
    spec("ancps6b-control", "AN/CPS-6B control", ModelClass::Radar, 0.0, 0.0, ""),
    spec("ancps6b-generator", "AN/CPS-6B generator", ModelClass::Radar, 0.0, 0.0, ""),
    spec("antps1c", "AN/TPS-1C", ModelClass::Radar, 0.0, 0.0, ""),
    spec("ge1942a", "GE-1942A", ModelClass::Radar, 0.0, 0.0, ""),
    spec("ge1942a-generator", "GE-1942A generator", ModelClass::Radar, 0.0, 0.0, ""),
    spec("p20", "P-20", ModelClass::Radar, 0.0, 0.0, ""),
    spec("p8", "P-8", ModelClass::Radar, 0.0, 0.0, ""),
    spec("p8-interrogator", "P-8 interrogator", ModelClass::Radar, 0.0, 0.0, ""),
    spec("rp15-1", "RP-15-1", ModelClass::Radar, 0.0, 0.0, ""),
    spec("t66", "T-66", ModelClass::Mortar, 0.0, 0.0, ""),
    spec("ge1942a-landinglight", "GE-1942A landing light", ModelClass::Airfield, 0.0, 0.0, ""),
    spec("flagstock-1950", "Flagstaff", ModelClass::Airfield, 0.0, 0.0, ""),
    spec("flagstock-small-1950", "Flagstaff (small)", ModelClass::Airfield, 0.0, 0.0, ""),
    spec("ndb", "NDB", ModelClass::Airfield, 0.0, 0.0, ""),
    spec("windsock", "Windsock", ModelClass::Airfield, 0.0, 0.0, ""),
    spec("windsock-small", "Windsock (small)", ModelClass::Airfield, 0.0, 0.0, ""),
    spec("fake_object", "Fake object", ModelClass::Dummy, 0.0, 0.0, ""),
    spec("fake_object_primary", "Fake object (primary)", ModelClass::Dummy, 0.0, 0.0, ""),
];

const fn spec(
    id: &'static str,
    label: &'static str,
    class: ModelClass,
    cruise_kmh: f32,
    ceiling_m: f32,
    notes: &'static str,
) -> ModelSpec {
    ModelSpec {
        id,
        label,
        class,
        cruise_kmh: Some(cruise_kmh),
        ceiling_m,
        notes,
    }
}

pub fn script_id(script: &str) -> String {
    script
        .trim_matches('"')
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(script)
        .trim_end_matches(".txt")
        .trim_matches('"')
        .to_ascii_lowercase()
}

pub fn spec_for(script: &str) -> Option<&'static ModelSpec> {
    let id = script_id(script);
    SPECS.iter().find(|s| s.id == id)
}

pub fn class_for(script: &str) -> ModelClass {
    spec_for(script).map(|s| s.class).unwrap_or(ModelClass::Unknown)
}

/// Classes present in `scripts`, display order (Fighter before Transport, etc.).
pub fn classes_in<'a>(scripts: impl IntoIterator<Item = &'a str>) -> Vec<ModelClass> {
    let mut out = Vec::new();
    for script in scripts {
        let class = class_for(script);
        if !out.contains(&class) {
            out.push(class);
        }
    }
    out.sort_by_key(|c| c.sort_key());
    out
}

pub fn format_cruise(kmh: Option<f32>) -> String {
    match kmh {
        None => "—".into(),
        Some(v) if v <= 0.0 => "stationary".into(),
        Some(v) => {
            let mph = v / 1.609_344;
            format!("{v:.0} km/h / {mph:.0} mph")
        }
    }
}

pub fn png_for_script(script: &str) -> &'static [u8] {
    model_png(&script_id(script))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_id_strips_path_and_extension() {
        assert_eq!(
            script_id(r"LuaScripts\WorldObjects\Planes\mig15bis.txt"),
            "mig15bis"
        );
        assert_eq!(script_id("\"f51d.txt\""), "f51d");
    }

    #[test]
    fn known_types_match_examples() {
        assert_eq!(class_for(r"Planes\mig15bis.txt"), ModelClass::Fighter);
        assert_eq!(class_for(r"Planes\b29.txt"), ModelClass::Bomber);
        assert_eq!(class_for(r"Planes\il10.txt"), ModelClass::Attack);
        assert_eq!(class_for(r"vehicles\t34-85.txt"), ModelClass::Armor);
        assert_eq!(
            class_for(r"fixedobjects\boforsl60.txt"),
            ModelClass::LightFlak
        );
        assert_eq!(class_for(r"fixedobjects\ks19.txt"), ModelClass::HeavyFlak);
        assert_eq!(class_for("unknown-thing.txt"), ModelClass::Unknown);
    }

    #[test]
    fn cruise_shows_metric_and_imperial() {
        let line = spec_for("f51d").unwrap().cruise_line();
        assert!(line.contains("km/h"), "{line}");
        assert!(line.contains("mph"), "{line}");
        assert!(line.starts_with("583"), "{line}");
        assert_eq!(format_cruise(Some(0.0)), "stationary");
        assert_eq!(format_cruise(None), "—");
    }

    #[test]
    fn ceiling_values_for_aircraft() {
        assert_eq!(spec_for("mig15bis").unwrap().ceiling_m, 15000.0);
        assert_eq!(spec_for("f86a5").unwrap().ceiling_m, 15240.0);
        assert_eq!(spec_for("b29").unwrap().ceiling_m, 10668.0);
        // Non-aircraft should be 0.0
        assert_eq!(spec_for("t34-85").unwrap().ceiling_m, 0.0);
        assert_eq!(spec_for("boforsl60").unwrap().ceiling_m, 0.0);
    }

    #[test]
    fn notes_are_optional() {
        assert_eq!(
            spec_for("willysmb").unwrap().notes,
            "Grandpa used to build these in Toledo"
        );
        assert!(spec_for("mig15bis").unwrap().notes.is_empty());
    }

    #[test]
    fn spec_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in SPECS {
            assert!(seen.insert(s.id), "duplicate spec id {}", s.id);
        }
    }

    #[test]
    fn classes_in_keeps_display_order() {
        let classes = classes_in(["b29.txt", "mig15bis.txt", "c47b.txt"]);
        assert_eq!(
            classes,
            vec![ModelClass::Fighter, ModelClass::Bomber, ModelClass::Transport]
        );
    }

    #[test]
    fn placeholder_png_decodes() {
        let img = image::load_from_memory(PLACEHOLDER_PNG).expect("placeholder.png");
        assert!(img.width() > 0 && img.height() > 0);
    }

    #[test]
    fn known_model_png_is_nonempty() {
        assert!(!png_for_script("mig15bis.txt").is_empty());
        assert!(!model_png("no-such-model").is_empty());
    }
}
