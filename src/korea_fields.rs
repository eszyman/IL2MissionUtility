//! korea_fields.rs — Korea map airfields and which ones still need a harvest
//!
//! Positions are the AIRFIELDS marks in the game's
//! `landscape_Korea_FullScene.Group`. Six K-sites named in the historical
//! reference have no mark on this map, so their position is unknown.
//! A field harvested only for the other coalition is still missing.

use crate::aircraft::country_is_nato;
use crate::mapclip::{WorldAabb, in_salient_zone, on_dprk_side};

const GAME_MARK: &str = "game file landscape_Korea_FullScene.Group (AIRFIELDS mark)";
const REFERENCE: &str = "reference doc docs/historical-reference/korea-1950-53-unit-reference.md";

pub struct KoreaField {
    pub name: &'static str,
    pub x: Option<f64>,
    pub z: Option<f64>,
    pub source: &'static str,
}

/// Game marks first, then reference fields with no position on this map.
pub const KOREA_FIELDS: &[KoreaField] = &[
    field("K-6 Pyongtaek", 39060.112, 291115.556),
    field("K-13 Suwon", 70877.971, 288471.836),
    field("K-14 Kimpo", 105930.832, 269353.748),
    field("K-16 Seoul", 102342.577, 280786.354),
    field("K-17 Ongjin (Oshin)", 147349.707, 148041.221),
    field("K-18 Gangneung", 131429.000, 458460.948),
    field("K-19 Haeju (Kaishu)", 155498.162, 179697.621),
    field("K-20 Sinmak", 202915.613, 221522.660),
    field("K-21 Pyonggang", 203579.865, 312383.802),
    field("K-22 Onchon", 256164.319, 132618.786),
    field("K-23 Pyongyang", 269599.586, 179962.935),
    field("K-24 Pyongyang", 267666.612, 185400.246),
    field("K-25 Wonsan", 285708.729, 327078.357),
    field("K-26 Sondok", 349685.939, 325148.674),
    field("K-27 Yonpo", 354864.460, 330548.138),
    field("K-28 Hamhung West", 372966.373, 325911.964),
    field("K-29 Sinanju", 349455.712, 190784.979),
    field("K-30 Sinuiju", 387670.836, 63402.405),
    field("K-41 Chungju", 40347.864, 367886.126),
    field("K-44 Changhowon-ni", 60346.371, 344289.585),
    field("K-45 Yoju", 75859.965, 356657.681),
    field("K-46 Hoengsong", 94099.416, 372809.842),
    field("K-47 Chuncheon", 143270.006, 350195.941),
    field("K-49 Yangsu-ri", 102153.938, 315263.081),
    field("K-50 Sokcho-ri", 173797.543, 426956.922),
    field("K-51 Inje", 157945.027, 385157.981),
    field("K-52 Yanggu", 166303.990, 373396.595),
    field("K-53 Paengyong-do", 149529.425, 86099.291),
    field("K-54 Cho-do", 216687.650, 97287.505),
    field("K-55 Osan", 54145.557, 290489.829),
    field("Uiju", 394612.066, 71207.026),
    field("Iwon", 420324.257, 430003.851),
    field("Tatung-kao (Myaogou)", 372987.636, 40588.230),
    field("Antung (Andong)", 380805.942, 52855.816),
    field("Dapu", 438289.644, 49019.344),
    field("Kuandian", 455296.186, 80102.895),
    field("Sariwon", 227233.428, 180583.906),
    field("Kangwon", 229076.248, 342704.919),
    field("Namsi", 369008.803, 131424.156),
    KoreaField {
        name: "K-1 Pusan West",
        x: None,
        z: None,
        source: REFERENCE,
    },
    KoreaField {
        name: "K-2 Taegu",
        x: None,
        z: None,
        source: REFERENCE,
    },
    KoreaField {
        name: "K-3 Pohang",
        x: None,
        z: None,
        source: REFERENCE,
    },
    KoreaField {
        name: "K-8 Kunsan",
        x: None,
        z: None,
        source: REFERENCE,
    },
    KoreaField {
        name: "K-9 Pusan East",
        x: None,
        z: None,
        source: REFERENCE,
    },
    KoreaField {
        name: "K-10 Chinhae",
        x: None,
        z: None,
        source: REFERENCE,
    },
];

const fn field(name: &'static str, x: f64, z: f64) -> KoreaField {
    KoreaField {
        name,
        x: Some(x),
        z: Some(z),
        source: GAME_MARK,
    }
}

pub struct HarvestHit {
    pub name: String,
    pub country: i32,
}

pub struct FieldStatus {
    pub name: &'static str,
    pub coalition: &'static str,
    pub harvested: bool,
    pub source: &'static str,
}

pub struct AoHarvest {
    pub inside: Vec<FieldStatus>,
    pub outside: usize,
    pub unplaced: Vec<&'static str>,
}

/// Fields inside `area`, and whether a harvest exists for the coalition that
/// holds that point. A harvest for the other coalition does not count.
/// A field inside a salient counts either harvest, and is labeled for both.
pub fn ao_harvest(
    hits: &[HarvestHit],
    area: WorldAabb,
    front: &[(f64, f64)],
    salients: &[Vec<(f64, f64)>],
) -> AoHarvest {
    let mut inside = Vec::new();
    let mut outside = 0usize;
    let mut unplaced = Vec::new();
    for field in KOREA_FIELDS {
        let Some((x, z)) = field.x.zip(field.z) else {
            unplaced.push(field.name);
            continue;
        };
        if !area.contains(x, z) {
            outside += 1;
            continue;
        }
        let in_zone = in_salient_zone(front, salients, x, z);
        let nato = !on_dprk_side(front, salients, x, z);
        let coalition = if in_zone {
            "NATO or DPRK"
        } else if nato {
            "NATO"
        } else {
            "DPRK"
        };
        let harvested = if in_zone {
            harvested_for(field.name, hits, true) || harvested_for(field.name, hits, false)
        } else {
            harvested_for(field.name, hits, nato)
        };
        inside.push(FieldStatus {
            name: field.name,
            coalition,
            harvested,
            source: field.source,
        });
    }
    AoHarvest {
        inside,
        outside,
        unplaced,
    }
}

fn harvested_for(list_name: &str, hits: &[HarvestHit], want_nato: bool) -> bool {
    hits.iter().any(|hit| {
        names_match(list_name, &hit.name) && country_is_nato(hit.country) == Some(want_nato)
    })
}

fn names_match(list_name: &str, harvest_name: &str) -> bool {
    match (k_code(list_name), k_code(harvest_name)) {
        (Some(a), Some(b)) => a == b,
        _ => place_hit(list_name, harvest_name),
    }
}

pub fn k_code(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'k' && bytes[i + 1] == b'-' && bytes[i + 2].is_ascii_digit() {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            return Some(lower[start..i].to_string());
        }
        i += 1;
    }
    None
}

fn tokens(name: &str) -> Vec<String> {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 4 && token.bytes().any(|b| b.is_ascii_alphabetic()))
        .filter(|token| !matches!(*token, "west" | "east" | "north" | "south"))
        .map(str::to_string)
        .collect()
}

fn place_hit(list_name: &str, harvest_name: &str) -> bool {
    let harvest = tokens(harvest_name);
    tokens(list_name)
        .into_iter()
        .any(|token| harvest.iter().any(|other| other == &token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_covers_game_marks_and_reference_fields_without_a_position() {
        assert_eq!(KOREA_FIELDS.len(), 45);
        let placed = KOREA_FIELDS
            .iter()
            .filter(|field| field.x.is_some() && field.z.is_some())
            .count();
        assert_eq!(placed, 39);
        assert!(KOREA_FIELDS.iter().any(|field| {
            field.name == "K-14 Kimpo"
                && field.x == Some(105930.832)
                && field.source.contains("landscape_Korea_FullScene")
        }));
        assert!(KOREA_FIELDS.iter().any(|field| {
            field.name == "K-2 Taegu" && field.x.is_none() && field.source.contains("reference")
        }));
    }

    #[test]
    fn a_field_harvested_for_the_other_coalition_stays_missing() {
        let hits = vec![
            HarvestHit {
                name: "K-14_Kimpo_AF".into(),
                country: 601,
            },
            HarvestHit {
                name: "K-23_Pyongyang_AF".into(),
                country: 601,
            },
        ];
        let area = WorldAabb::from_corners(100_000.0, 160_000.0, 280_000.0, 280_000.0);
        let front = vec![(200_000.0, 0.0), (200_000.0, 500_000.0)];
        let report = ao_harvest(&hits, area, &front, &[]);
        let kimpo = report
            .inside
            .iter()
            .find(|row| row.name.contains("Kimpo"))
            .unwrap();
        assert_eq!(kimpo.coalition, "NATO");
        assert!(kimpo.harvested);
        let pyongyang = report
            .inside
            .iter()
            .find(|row| row.name.starts_with("K-23"))
            .unwrap();
        assert_eq!(pyongyang.coalition, "DPRK");
        assert!(!pyongyang.harvested);
        assert!(report.outside > 0);
        assert!(report.unplaced.iter().any(|name| name.contains("K-2")));
        assert!(names_match("K-14 Kimpo", "K-14_Kimpo_AF_601"));
        assert!(!names_match("K-23 Pyongyang", "K-24_Pyongyang_AF"));
        assert!(names_match("Sariwon", "Sariwon_AF"));
        assert!(!names_match("Uiju", "K-30_Sinuiju_AF"));
    }

    #[test]
    fn a_field_inside_a_salient_counts_either_harvest() {
        let hits = vec![HarvestHit {
            name: "K-23_Pyongyang_AF".into(),
            country: 601,
        }];
        let area = WorldAabb::from_corners(100_000.0, 160_000.0, 280_000.0, 280_000.0);
        let front = vec![(200_000.0, 0.0), (200_000.0, 500_000.0)];
        let pocket = vec![
            (260_000.0, 170_000.0),
            (280_000.0, 170_000.0),
            (280_000.0, 190_000.0),
            (260_000.0, 190_000.0),
            (260_000.0, 170_000.0),
        ];
        let report = ao_harvest(&hits, area, &front, &[pocket]);
        let pyongyang = report
            .inside
            .iter()
            .find(|row| row.name.starts_with("K-23"))
            .unwrap();
        assert_eq!(pyongyang.coalition, "NATO or DPRK");
        assert!(pyongyang.harvested);
    }
}
