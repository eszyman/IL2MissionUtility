//! Korean War map icon templates (1950–1953) for the IL-2 Korea map.
//!
//! Polylines and markers are authored in WGS84 and projected with `geo`.
//! Generation clips everything to the user-drawn world AABB.

use std::path::PathBuf;

use crate::aircraft::{aircraft_by_id, AircraftType};
use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::geo::{latlon_to_xz, on_map, MAP_MAX, MAP_MIN};
use crate::locale::{merge_template_sidecars, LocaleTable};
use crate::mapclip::{
    apply_salients, clip_ring_to_aabb, extend_front_to_aabb_ex, format_clip_preview,
    influence_minus_salients, multipolygon_rings, prepare_front, WorldAabb,
};
use crate::parser::parse_group_file;

mod timeline;
pub use timeline::{
    front_xz, mark_for_battle, preview_front_xz, timeline_index, TimelineMark, TIMELINE,
};

const LINE_STEP: f64 = 4_000.0;
const OVERLAP: f64 = 0.22;
/// End-vertex line style from `TemplateExamples/IconHelper.Group` (non-front polylines).
const LINE_POLY: i32 = 22;
/// Front polyline from `TemplateExamples/FrontLine.Group`.
const LINE_FRONT_BODY: i32 = 13;
const LINE_FRONT_END: i32 = 1;
/// Filled attack arrow from `TemplateExamples/CorrectedAttack_arrow.Group`.
const LINE_ATTACK_ARROW: i32 = 11;
/// Closed defend / assembly ring from `TemplateExamples/CorrectedDefendArea.Group`.
const LINE_DEFEND_AREA: i32 = 12;
/// IL-2 salient fill from `TemplateExamples/SalientReference.Group` (`LineType` 4).
const LINE_SALIENT: i32 = 4;
const COAL_ALL: &str = "[1, 2, 0]";
const COAL_SIDES: &str = "[1, 2]";
const COUNTRY_USA: i32 = 601;
const COUNTRY_DPRK: i32 = 503;
const NATO_R: i32 = 0;
const NATO_G: i32 = 120;
const NATO_B: i32 = 150;
const EAST_R: i32 = 155;
const EAST_G: i32 = 0;
const EAST_B: i32 = 0;
const BATTLE_RADIUS: f64 = 12_000.0;
const ARROW_LENGTH: f64 = 32_000.0;
pub const ARROW_TAIL_WIDTH: f64 = 4_200.0;
/// Include reference groups this far outside the drawn AO.
pub const PLACE_MARGIN: f64 = 10_000.0;
/// Clear this much on each side of the front between the two influence areas.
pub const AOI_GAP: f64 = 5_000.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Season {
    EarlySpring,
    LateSpring,
    Summer,
    Fall,
    Winter,
}

impl Season {
    pub const ALL: [Season; 5] = [
        Season::EarlySpring,
        Season::LateSpring,
        Season::Summer,
        Season::Fall,
        Season::Winter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Season::EarlySpring => "Early spring",
            Season::LateSpring => "Late spring",
            Season::Summer => "Summer",
            Season::Fall => "Fall",
            Season::Winter => "Winter",
        }
    }
}

pub const YEARS: [u16; 4] = [1950, 1951, 1952, 1953];

#[derive(Clone, Copy, Debug)]
pub struct Sector {
    pub id: usize,
    pub name: &'static str,
    pub hint: &'static str,
    pub x_min: f64,
    pub x_max: f64,
    pub z_min: f64,
    pub z_max: f64,
}

impl Sector {
    pub fn contains(self, x: f64, z: f64) -> bool {
        (self.x_min..=self.x_max).contains(&x) && (self.z_min..=self.z_max).contains(&z)
    }
}

/// Six overlapping map tiles: 2 north–south × 3 west–east.
pub fn sectors() -> [Sector; 6] {
    let span_x = (MAP_MAX - MAP_MIN) / 2.0;
    let span_z = (MAP_MAX - MAP_MIN) / 3.0;
    let ox = span_x * OVERLAP;
    let oz = span_z * OVERLAP;
    let names = [
        ("SW Yellow Sea south", "Ongjin, Haeju approaches, west coastal plain"),
        ("SC Seoul–Inchon", "Suwon, Inchon, Seoul, Han River"),
        ("SE East Coast south", "Sokcho, Yangyang, Punchbowl south"),
        ("NW Pyongyang", "Pyongyang, Sariwon, Yellow Sea north"),
        ("NC Iron Triangle", "Kaesong, Chorwon, Kumhwa, Pyonggang"),
        ("NE Wonsan–Chosin", "Wonsan, Hamhung, Hungnam, Chosin"),
    ];
    let mut out = [Sector {
        id: 0,
        name: "",
        hint: "",
        x_min: 0.0,
        x_max: 0.0,
        z_min: 0.0,
        z_max: 0.0,
    }; 6];
    for row in 0..2 {
        for col in 0..3 {
            let i = row * 3 + col;
            let x0 = MAP_MIN + row as f64 * span_x - if row == 0 { 0.0 } else { ox };
            let x1 = MAP_MIN + (row as f64 + 1.0) * span_x + if row == 1 { 0.0 } else { ox };
            let z0 = MAP_MIN + col as f64 * span_z - if col == 0 { 0.0 } else { oz };
            let z1 = MAP_MIN + (col as f64 + 1.0) * span_z + if col == 2 { 0.0 } else { oz };
            out[i] = Sector {
                id: i,
                name: names[i].0,
                hint: names[i].1,
                x_min: x0.max(MAP_MIN - 1.0),
                x_max: x1.min(MAP_MAX + 1.0),
                z_min: z0.max(MAP_MIN - 1.0),
                z_max: z1.min(MAP_MAX + 1.0),
            };
        }
    }
    out
}

#[derive(Clone, Copy)]
pub struct Battle {
    pub id: &'static str,
    pub name: &'static str,
    pub year: u16,
    pub season: Season,
    pub lat: f64,
    pub lon: f64,
    pub note: &'static str,
}

pub const BATTLES: &[Battle] = &[
    Battle {
        id: "seoul1",
        name: "First Battle of Seoul",
        year: 1950,
        season: Season::LateSpring,
        lat: 37.57,
        lon: 126.98,
        note: "NKPA takes Seoul, 28 June 1950",
    },
    Battle {
        id: "inchon",
        name: "Inchon landing",
        year: 1950,
        season: Season::Fall,
        lat: 37.46,
        lon: 126.63,
        note: "X Corps amphibious landing, 15 September 1950",
    },
    Battle {
        id: "seoul2",
        name: "Second Battle of Seoul",
        year: 1950,
        season: Season::Fall,
        lat: 37.57,
        lon: 126.98,
        note: "UN recaptures Seoul, 25–28 September 1950",
    },
    Battle {
        id: "pyongyang",
        name: "Capture of Pyongyang",
        year: 1950,
        season: Season::Fall,
        lat: 39.04,
        lon: 125.76,
        note: "ROK/US enter Pyongyang, 19 October 1950",
    },
    Battle {
        id: "chosin",
        name: "Chosin Reservoir",
        year: 1950,
        season: Season::Winter,
        lat: 40.49,
        lon: 127.25,
        note: "1st Marine Division breakout, November–December 1950",
    },
    Battle {
        id: "hungnam",
        name: "Hungnam evacuation",
        year: 1950,
        season: Season::Winter,
        lat: 39.83,
        lon: 127.62,
        note: "X Corps sea evacuation, December 1950",
    },
    Battle {
        id: "seoul3",
        name: "Third Battle of Seoul",
        year: 1950,
        season: Season::Winter,
        lat: 37.57,
        lon: 126.98,
        note: "Chinese take Seoul, 4 January 1951",
    },
    Battle {
        id: "ripper",
        name: "Operation Ripper / Seoul retaken",
        year: 1951,
        season: Season::EarlySpring,
        lat: 37.57,
        lon: 126.98,
        note: "UN recaptures Seoul, 14–15 March 1951",
    },
    Battle {
        id: "imjin",
        name: "Imjin River / Kapyong",
        year: 1951,
        season: Season::LateSpring,
        lat: 37.89,
        lon: 126.80,
        note: "Chinese Spring Offensive, April 1951",
    },
    Battle {
        id: "bloody",
        name: "Bloody Ridge",
        year: 1951,
        season: Season::Summer,
        lat: 38.26,
        lon: 128.10,
        note: "Hill fighting east of the Punchbowl, August–September 1951",
    },
    Battle {
        id: "heartbreak",
        name: "Heartbreak Ridge",
        year: 1951,
        season: Season::Fall,
        lat: 38.30,
        lon: 128.12,
        note: "September–October 1951",
    },
    Battle {
        id: "punchbowl",
        name: "Punchbowl",
        year: 1951,
        season: Season::Fall,
        lat: 38.28,
        lon: 128.20,
        note: "Static mountain front, 1951",
    },
    Battle {
        id: "triangle",
        name: "Triangle Hill",
        year: 1952,
        season: Season::Fall,
        lat: 38.32,
        lon: 127.28,
        note: "Shangganling, October–November 1952",
    },
    Battle {
        id: "porkchop",
        name: "Pork Chop Hill",
        year: 1953,
        season: Season::LateSpring,
        lat: 38.24,
        lon: 127.00,
        note: "April–July 1953, eve of the armistice",
    },
];

/// A user-picked `.Group` to stamp onto the map at its saved X/Z (then trimmed).
#[derive(Clone, Debug)]
pub struct MapRefGroup {
    pub path: PathBuf,
    pub entity: Il2Entity,
}

/// One NodeGates fighter pack parked on the map (one wave, or one leftover pack).
#[derive(Clone, Debug)]
pub struct MapFighterPack {
    pub root: Il2Entity,
}

/// One randomizer ship pack parked on the map.
#[derive(Clone, Debug)]
pub struct MapShipPack {
    pub root: Il2Entity,
}

/// One randomizer ground pack parked on the map (one coalition).
#[derive(Clone, Debug)]
pub struct MapGroundPack {
    pub root: Il2Entity,
}

#[derive(Clone, Debug)]
pub struct FrontOptions {
    pub year: u16,
    pub season: Season,
    pub aabb: WorldAabb,
    pub front: bool,
    pub battles: bool,
    pub buildups: bool,
    pub defenses: bool,
    pub attacks: bool,
    pub naval: bool,
    pub influence: bool,
    pub ref_groups: Vec<MapRefGroup>,
    pub battle_focus: Option<&'static str>,
    /// Dated TIMELINE slot. `None` resolves the first mark for `year`+`season`.
    pub timeline_idx: Option<usize>,
    pub custom_front: Option<Vec<(f64, f64)>>,
	pub salients: Vec<Vec<(f64, f64)>>,
    /// User-drawn attack axes `(tail, tip)` in world X/Z. Colored by the side the tail sits on.
    pub user_attacks: Vec<((f64, f64), (f64, f64))>,
    /// Linked fighter packs placed on the map (one coalition).
    pub fighter_packs: Vec<MapFighterPack>,
    /// Randomizer ship pack parked on water (one coalition).
    pub ship_packs: Vec<MapShipPack>,
    /// Randomizer ground packs parked on open terrain (one per coalition).
    pub ground_packs: Vec<MapGroundPack>,
}

impl Default for FrontOptions {
    fn default() -> Self {
        Self {
            year: 1951,
            season: Season::LateSpring,
            aabb: WorldAabb::full_map(),
            front: true,
            battles: true,
            buildups: true,
            defenses: true,
            attacks: true,
            naval: true,
            influence: true,
            ref_groups: Vec::new(),
            battle_focus: None,
            timeline_idx: None,
            custom_front: None,
            salients: Vec::new(),
            user_attacks: Vec::new(),
            fighter_packs: Vec::new(),
            ship_packs: Vec::new(),
            ground_packs: Vec::new(),
        }
    }
}

pub struct FrontPack {
    pub root: Il2Entity,
    pub locale: LocaleTable,
    pub period_label: &'static str,
    pub period_note: &'static str,
    pub aircraft: Vec<&'static AircraftType>,
    pub icon_count: usize,
    pub notes: Vec<String>,
    pub clip_preview: String,
}

#[derive(Clone, Copy)]
struct Style {
    r: i32,
    g: i32,
    b: i32,
    end_r: i32,
    end_g: i32,
    end_b: i32,
    icon_id: i32,
    body_line: i32,
    end_line: i32,
    body_coalitions: &'static str,
    end_coalitions: &'static str,
    ypos: &'static str,
}

const fn marker(r: i32, g: i32, b: i32, icon_id: i32) -> Style {
    Style {
        r,
        g,
        b,
        end_r: r,
        end_g: g,
        end_b: b,
        icon_id,
        body_line: 0,
        end_line: 0,
        body_coalitions: COAL_ALL,
        end_coalitions: COAL_ALL,
        ypos: "0.000",
    }
}

const fn poly(r: i32, g: i32, b: i32) -> Style {
    Style {
        r,
        g,
        b,
        end_r: r,
        end_g: g,
        end_b: b,
        icon_id: 0,
        body_line: 0,
        end_line: LINE_POLY,
        body_coalitions: COAL_ALL,
        end_coalitions: COAL_ALL,
        ypos: "0.000",
    }
}

const fn zone(r: i32, g: i32, b: i32, line: i32) -> Style {
    Style {
        r,
        g,
        b,
        end_r: r,
        end_g: g,
        end_b: b,
        icon_id: 0,
        body_line: line,
        end_line: line,
        body_coalitions: COAL_SIDES,
        end_coalitions: COAL_SIDES,
        ypos: "1.000",
    }
}

const FRONT: Style = Style {
    r: 255,
    g: 255,
    b: 255,
    end_r: 255,
    end_g: 0,
    end_b: 0,
    icon_id: 0,
    body_line: LINE_FRONT_BODY,
    end_line: LINE_FRONT_END,
    body_coalitions: COAL_SIDES,
    end_coalitions: COAL_ALL,
    ypos: "1.000",
};
const BATTLE: Style = marker(255, 220, 40, 501);
const NAVAL: Style = poly(30, 160, 220);

fn faction_style(eastern: bool, line: i32) -> Style {
    if eastern {
        zone(EAST_R, EAST_G, EAST_B, line)
    } else {
        zone(NATO_R, NATO_G, NATO_B, line)
    }
}

fn area_is_eastern(area: &Area) -> bool {
    let t = format!("{} {}", area.name, area.desc);
    t.contains("NKPA") || t.contains("PVA") || t.contains("Communist") || t.contains("Chinese")
}

#[derive(Clone, Copy)]
struct Area {
    name: &'static str,
    desc: &'static str,
    ring: &'static [(f64, f64)],
}

#[derive(Clone, Copy)]
struct Route {
    name: &'static str,
    desc: &'static str,
    path: &'static [(f64, f64)],
}

#[derive(Clone, Copy)]
struct Snapshot {
    year: u16,
    season: Season,
    label: &'static str,
    note: &'static str,
    front: &'static [(f64, f64)],
    buildups: &'static [Area],
    defenses: &'static [Area],
    attacks: &'static [Area],
    naval: &'static [Route],
    extra_note: Option<&'static str>,
}

const PREWAR_38: &[(f64, f64)] = &[
    (38.00, 124.90),
    (38.00, 125.60),
    (38.00, 126.20),
    (38.00, 126.80),
    (38.00, 127.40),
    (38.00, 128.00),
    (38.00, 128.55),
];

const FRONT_JUL50: &[(f64, f64)] = &[
    (37.08, 126.40),
    (37.06, 127.00),
    (37.08, 127.60),
    (37.12, 128.20),
    (37.16, 128.60),
];

const FRONT_JUN50: &[(f64, f64)] = &[
    (37.40, 126.50),
    (37.45, 126.90),
    (37.38, 127.40),
    (37.32, 128.00),
    (37.30, 128.50),
];

const FRONT_OCT50: &[(f64, f64)] = &[
    (39.95, 124.55),
    (39.88, 125.15),
    (39.80, 125.75),
    (39.95, 126.40),
    (40.20, 126.95),
    (40.32, 127.30),
    (40.10, 127.60),
    (39.55, 127.75),
    (39.15, 128.20),
];

const FRONT_DEC50: &[(f64, f64)] = &[
    (37.55, 126.50),
    (37.50, 126.95),
    (37.65, 127.50),
    (37.90, 128.10),
    (38.10, 128.50),
];

const FRONT_MAR51: &[(f64, f64)] = &[
    (37.72, 126.55),
    (37.85, 126.95),
    (37.98, 127.40),
    (38.08, 128.00),
    (38.12, 128.50),
];

const MLR: &[(f64, f64)] = &[
    (37.90, 126.62),
    (38.02, 126.95),
    (38.28, 127.22),
    (38.32, 127.48),
    (38.28, 127.85),
    (38.30, 128.20),
    (38.38, 128.55),
];

const BUILD_NKPA_38: &[Area] = &[Area {
    name: "NKPA buildup north of the 38th",
    desc: "Armor and infantry assembling for the June invasion",
    ring: &[
        (38.15, 126.40),
        (38.55, 126.40),
        (38.55, 127.40),
        (38.15, 127.40),
    ],
}];

const BUILD_CHINESE_YALU: &[Area] = &[Area {
    name: "PVA assembly, Yalu approaches",
    desc: "Chinese armies crossing into Korea, October-November 1950",
    ring: &[
        (39.85, 125.00),
        (40.22, 125.00),
        (40.22, 126.40),
        (39.85, 126.40),
    ],
}];

const BUILD_IRON: &[Area] = &[Area {
    name: "Iron Triangle buildup",
    desc: "Communist logistics hub: Chorwon, Kumhwa, Pyonggang",
    ring: &[
        (38.25, 127.10),
        (38.55, 127.10),
        (38.55, 127.55),
        (38.25, 127.55),
    ],
}];

const DEF_38_ROK: &[Area] = &[Area {
    name: "ROK 38th Parallel defenses",
    desc: "Thin ROK line on the parallel, June 1950",
    ring: &[
        (37.85, 126.50),
        (38.00, 126.50),
        (38.00, 128.20),
        (37.85, 128.20),
    ],
}];

const DEF_STALEMATE: &[Area] = &[
    Area {
        name: "UN Kansas Line",
        desc: "Main defensive belt after the 1951 spring battles",
        ring: &[
            (37.82, 126.55),
            (38.00, 126.55),
            (38.22, 127.20),
            (38.22, 128.30),
            (38.00, 128.50),
            (37.85, 128.00),
            (37.80, 127.20),
        ],
    },
    Area {
        name: "Communist MLR",
        desc: "Deep defenses north of the Punchbowl and Iron Triangle",
        ring: &[
            (38.35, 126.90),
            (38.70, 126.90),
            (38.70, 128.30),
            (38.35, 128.30),
        ],
    },
];

const ATK_FALL50: &[Area] = &[
    Area {
        name: "Inchon–Seoul attack",
        desc: "X Corps landing and drive inland, September 1950",
        ring: &[
            (37.38, 126.45),
            (37.70, 126.45),
            (37.70, 127.10),
            (37.38, 127.10),
        ],
    },
    Area {
        name: "UN drive on the Yalu",
        desc: "8th Army west, X Corps east, October 1950",
        ring: &[
            (39.20, 125.40),
            (40.50, 125.40),
            (40.50, 127.60),
            (39.20, 127.60),
        ],
    },
];

const ATK_NKPA_SOUTH: &[Area] = &[Area {
    name: "NKPA drive on Seoul",
    desc: "Invasion axis, late June 1950",
    ring: &[
        (37.50, 126.70),
        (38.05, 126.70),
        (38.05, 127.20),
        (37.50, 127.20),
    ],
}];

const ATK_PVA_SOUTH: &[Area] = &[Area {
    name: "PVA Second Phase Offensive",
    desc: "Chinese strike on 8th Army and X Corps, November 1950",
    ring: &[
        (39.80, 125.50),
        (40.50, 125.50),
        (40.50, 127.50),
        (39.80, 127.50),
    ],
}];

const ATK_UN_SPRING51: &[Area] = &[Area {
    name: "UN counteroffensive",
    desc: "Thunderbolt / Killer / Ripper, February–March 1951",
    ring: &[
        (37.35, 126.60),
        (38.05, 126.60),
        (38.05, 127.80),
        (37.35, 127.80),
    ],
}];

const NAV_YELLOW: Route = Route {
    name: "Yellow Sea UN naval route",
    desc: "Carrier and amphibious approaches off Inchon and Haeju",
    path: &[
        (37.20, 125.40),
        (37.45, 126.10),
        (37.50, 126.50),
        (37.80, 125.80),
        (38.40, 125.20),
        (38.90, 124.90),
    ],
};

const NAV_INCHON: Route = Route {
    name: "Inchon approach",
    desc: "Flying Fish Channel to Inchon tidal basin",
    path: &[(37.20, 126.20), (37.35, 126.45), (37.46, 126.60)],
};

const NAV_EAST: Route = Route {
    name: "East Sea UN naval route",
    desc: "Wonsan siege and Hungnam approaches",
    path: &[
        (37.30, 129.00),
        (38.00, 128.85),
        (38.80, 128.40),
        (39.15, 127.80),
        (39.83, 127.70),
        (40.20, 128.00),
    ],
};

const NAV_HUNGNAM: Route = Route {
    name: "Hungnam evacuation route",
    desc: "X Corps sea lift, December 1950",
    path: &[(39.83, 127.62), (39.60, 128.20), (39.20, 129.00)],
};

fn snapshots() -> Vec<Snapshot> {
    vec![
        Snapshot {
            year: 1950,
            season: Season::EarlySpring,
            label: "Spring 1950 — uneasy 38th Parallel",
            note: "Pre-invasion. ROK on the parallel; NKPA assembling north of it.",
            front: PREWAR_38,
            buildups: BUILD_NKPA_38,
            defenses: DEF_38_ROK,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1950,
            season: Season::LateSpring,
            label: "June 1950 — invasion of the South",
            note: "NKPA crosses the 38th. Seoul falls on 28 June.",
            front: FRONT_JUN50,
            buildups: BUILD_NKPA_38,
            defenses: DEF_38_ROK,
            attacks: ATK_NKPA_SOUTH,
            naval: &[NAV_YELLOW],
            extra_note: None,
        },
        Snapshot {
            year: 1950,
            season: Season::Summer,
            label: "Summer 1950 — NKPA holds the peninsula",
            note: "UN is pinned on the Pusan Perimeter, south of this map.",
            front: FRONT_JUL50,
            buildups: &[],
            defenses: &[],
            attacks: ATK_NKPA_SOUTH,
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: Some("Pusan Perimeter is off the south edge of this map."),
        },
        Snapshot {
            year: 1950,
            season: Season::Fall,
            label: "Fall 1950 — Inchon to the Yalu",
            note: "Inchon (15 Sep), Seoul retaken, Pyongyang (19 Oct), then the UN race north.",
            front: FRONT_OCT50,
            buildups: BUILD_CHINESE_YALU,
            defenses: &[],
            attacks: ATK_FALL50,
            naval: &[NAV_YELLOW, NAV_INCHON, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1950,
            season: Season::Winter,
            label: "Winter 1950–51 — Chosin and the long retreat",
            note: "Chosin breakout, Hungnam evacuation, Chinese recapture of Seoul (4 Jan).",
            front: FRONT_DEC50,
            buildups: BUILD_CHINESE_YALU,
            defenses: &[],
            attacks: ATK_PVA_SOUTH,
            naval: &[NAV_EAST, NAV_HUNGNAM, NAV_YELLOW],
            extra_note: None,
        },
        Snapshot {
            year: 1951,
            season: Season::EarlySpring,
            label: "Early spring 1951 — back through Seoul",
            note: "Operations Thunderbolt, Killer, and Ripper. Seoul retaken 14 March.",
            front: FRONT_MAR51,
            buildups: &[],
            defenses: &[],
            attacks: ATK_UN_SPRING51,
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1951,
            season: Season::LateSpring,
            label: "Late spring 1951 — Chinese offensive, then the MLR",
            note: "Imjin and Kapyong in April. Front settles near the 38th by June.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1951,
            season: Season::Summer,
            label: "Summer 1951 — Kansas Line and Bloody Ridge",
            note: "Static war begins. Hill battles on the eastern MLR.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1951,
            season: Season::Fall,
            label: "Fall 1951 — Heartbreak Ridge and the Punchbowl",
            note: "UN attacks on the eastern mountains while talks go on at Kaesong/Panmunjom.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1951,
            season: Season::Winter,
            label: "Winter 1951–52 — stalemate",
            note: "Main line of resistance frozen near the 38th. Air war over MiG Alley.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1952,
            season: Season::EarlySpring,
            label: "1952 — stalemate (early spring)",
            note: "MLR unchanged. UN air pressure on Communist rail and airfields.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1952,
            season: Season::LateSpring,
            label: "1952 — stalemate (late spring)",
            note: "Same Kansas Line. Wonsan still under naval siege.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1952,
            season: Season::Summer,
            label: "1952 — stalemate (summer)",
            note: "Outpost war along the MLR.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1952,
            season: Season::Fall,
            label: "Fall 1952 — Triangle Hill",
            note: "Shangganling / Triangle Hill, October–November 1952.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1952,
            season: Season::Winter,
            label: "Winter 1952–53 — stalemate",
            note: "Talks continue. MLR holds.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1953,
            season: Season::EarlySpring,
            label: "Early spring 1953 — outpost war",
            note: "Old Baldy and the approaches to Pork Chop Hill.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1953,
            season: Season::LateSpring,
            label: "Late spring 1953 — Pork Chop Hill",
            note: "Heavy outpost fights while the armistice is drafted.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1953,
            season: Season::Summer,
            label: "Summer 1953 — armistice",
            note: "Ceasefire 27 July 1953. Line of contact becomes the DMZ.",
            front: MLR,
            buildups: BUILD_IRON,
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: Some("Armistice 27 July 1953. Treat the front as the DMZ."),
        },
        Snapshot {
            year: 1953,
            season: Season::Fall,
            label: "Fall 1953 — DMZ",
            note: "Post-armistice. Same line of contact.",
            front: MLR,
            buildups: &[],
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
        Snapshot {
            year: 1953,
            season: Season::Winter,
            label: "Winter 1953 — DMZ",
            note: "Post-armistice. Same line of contact.",
            front: MLR,
            buildups: &[],
            defenses: DEF_STALEMATE,
            attacks: &[],
            naval: &[NAV_YELLOW, NAV_EAST],
            extra_note: None,
        },
    ]
}

pub fn snapshot_label(year: u16, season: Season) -> &'static str {
    find_snapshot(year, season).label
}

pub fn snapshot_note(year: u16, season: Season) -> &'static str {
    find_snapshot(year, season).note
}

pub fn battles_in_period(year: u16, season: Season) -> Vec<&'static Battle> {
    BATTLES
        .iter()
        .filter(|b| b.year == year && b.season == season)
        .collect()
}

pub fn suggested_aircraft(year: u16, season: Season) -> Vec<&'static AircraftType> {
    aircraft_ids(year, season)
        .iter()
        .filter_map(|id| aircraft_by_id(id))
        .collect()
}

pub fn snapshot_front_xz(year: u16, season: Season) -> Vec<(f64, f64)> {
    let raw: Vec<(f64, f64)> = find_snapshot(year, season)
        .front
        .iter()
        .copied()
        .map(|(la, lo)| latlon_to_xz(la, lo))
        .collect();
    prepare_front(&raw)
}

/// Historical overlay for the map preview (not exported).
#[derive(Clone, Debug, Default)]
pub struct TimelinePreview {
    pub battles: Vec<PreviewBattleMark>,
}

#[derive(Clone, Debug)]
pub struct PreviewBattleMark {
    pub name: &'static str,
    pub x: f64,
    pub z: f64,
}

/// Major battles for the nearest dated mark (preview only).
pub fn timeline_preview(t: f32) -> TimelinePreview {
    let max = TIMELINE.len().saturating_sub(1);
    let i = t.round().clamp(0.0, max as f32) as usize;
    let mark = TIMELINE.get(i).copied().unwrap_or(TIMELINE[0]);
    let mut battles = Vec::new();
    for b in battles_in_period(mark.year, mark.season) {
        let (x, z) = latlon_to_xz(b.lat, b.lon);
        if !on_map(x, z) {
            continue;
        }
        battles.push(PreviewBattleMark {
            name: b.name,
            x,
            z,
        });
    }
    TimelinePreview { battles }
}

fn aircraft_ids(year: u16, season: Season) -> &'static [&'static str] {
    match (year, season) {
        (1950, Season::EarlySpring | Season::LateSpring | Season::Summer) => {
            &["f80c10", "f51d", "yak9p", "la11"]
        }
        (1950, Season::Fall) => &["f80c10", "f51d", "f84e", "yak9p", "la11", "mig15bis"],
        (1950, Season::Winter) => {
            &["f80c10", "f51d", "f84e", "f86a5", "yak9p", "la11", "mig15bis"]
        }
        _ => &[
            "f86a5", "f84e", "f80c10", "f51d", "mig15bis", "la11", "yak9p",
        ],
    }
}

fn find_snapshot(year: u16, season: Season) -> Snapshot {
    snapshots()
        .into_iter()
        .find(|s| s.year == year && s.season == season)
        .unwrap_or_else(|| snapshots().into_iter().next().unwrap())
}

pub fn generate_front(opts: &FrontOptions) -> Result<FrontPack, String> {
    if !opts.front
        && !opts.battles
        && !opts.buildups
        && !opts.defenses
        && !opts.attacks
        && !opts.naval
        && !opts.influence
        && opts.ref_groups.is_empty()
        && opts.user_attacks.is_empty()
        && opts.fighter_packs.is_empty()
        && opts.ship_packs.is_empty()
        && opts.ground_packs.is_empty()
    {
        return Err("enable at least one layer.".into());
    }
    if !opts.aabb.is_valid() {
        return Err("draw a bounding box on the map.".into());
    }
    let mark = resolve_timeline_mark(opts);
    let snap = find_snapshot(mark.year, mark.season);
    let aabb = opts.aabb;
    let place = aabb.expanded(PLACE_MARGIN);
    let keep = move |x: f64, z: f64| on_map(x, z) && place.contains(x, z);

    let dated = opts.timeline_idx.is_some();
    let using_custom = opts.custom_front.is_some();
    let front_name = if using_custom {
        "Custom Front line"
    } else if dated {
        mark.title
    } else {
        snap.label
    };
    let front_desc = if using_custom {
        "User-drawn line"
    } else if dated {
        mark.note
    } else {
        snap.note
    };
    let stretch_east = !using_custom;

    let proto = icon_prototype()?;
    let mut next_id = 1i32;
    let mut locale = LocaleTable::default();
    let mut next_lc = 2i32;
    for g in &opts.ref_groups {
        next_lc = next_lc.max(max_lc(&g.entity) + 1);
    }
    let paths: Vec<PathBuf> = opts.ref_groups.iter().map(|g| g.path.clone()).collect();
    for table in merge_template_sidecars(&paths).into_values() {
        if let Some(m) = table.max_id() {
            next_lc = next_lc.max(m + 1);
        }
    }
    let mut children = Vec::new();
    let mut icon_count = 0usize;
    let mut notes = Vec::new();
    if let Some(n) = snap.extra_note {
        notes.push(n.to_string());
    }

    let mut front_runs = 0usize;
    let mut influence_rings = 0usize;

    let raw_base = opts
        .custom_front
        .clone()
        .unwrap_or_else(|| front_xz(mark));
    let extended_base = extend_front_to_aabb_ex(&raw_base, aabb, true, stretch_east);
    let dense_base = densify(&extended_base, LINE_STEP);
    let (composite_front, patches) = apply_salients(dense_base.clone(), &opts.salients);

    if opts.front {
        let flatten = !patches.iter().any(|p| p.attached);
        let (group, n, runs) = clipped_front_group_xz(
            front_name,
            front_desc,
            &composite_front,
            FRONT,
            aabb,
            false,
            flatten,
            &proto,
            &mut next_id,
            &mut locale,
            &mut next_lc,
        );
        icon_count += n;
        front_runs = runs;

        let mut main_group = group;

        for patch in &patches {
            let fill = faction_style(!patch.cuts_north, LINE_SALIENT);
            for ring in clip_ring_to_aabb(&patch.ring, aabb) {
                let (fill_icons, fn_) = polyline_xz_icons(
                    "Salient",
                    "Salient hashed fill",
                    &ring,
                    fill,
                    true,
                    &keep,
                    &proto,
                    &mut next_id,
                    &mut locale,
                    &mut next_lc,
                );
                icon_count += fn_;
                main_group.children.extend(fill_icons);
            }
        }

        if main_group.children.is_empty() {
            notes.push("Front line does not cross the selected area.".into());
        } else {
            children.push(main_group);
        }
    }

    if !opts.user_attacks.is_empty() {
        let mut g = named_group("Attack arrows", &mut next_id);
        let mut added = 0usize;
        for &(tail, tip) in &opts.user_attacks {
            let dx = tip.0 - tail.0;
            let dz = tip.1 - tail.1;
            if (dx * dx + dz * dz).sqrt() < 1.0 {
                continue;
            }
            let eastern = crate::mapclip::point_north_of_front(&dense_base, tail.0, tail.1);
            let path = attack_arrow_points(tail, tip, ARROW_TAIL_WIDTH);
            let (icons, c) = emit_attack_chain(
                "Attack",
                "User-drawn attack axis",
                &path,
                faction_style(eastern, LINE_ATTACK_ARROW),
                &proto,
                &mut next_id,
                &mut locale,
                &mut next_lc,
            );
            added += c;
            g.children.extend(icons);
        }
        icon_count += added;
        if added > 0 {
            children.push(g);
        }
    }

    if opts.battles {
        let focus = opts.battle_focus;
        let list: Vec<&Battle> = BATTLES
            .iter()
            .filter(|b| b.year == mark.year && b.season == mark.season)
            .filter(|b| focus.is_none_or(|id| b.id == id))
            .collect();
        let mut g = named_group("Battles", &mut next_id);
        let mut added = 0usize;
        for b in list {
            let (x, z) = latlon_to_xz(b.lat, b.lon);
            if !on_map(x, z) {
                notes.push(format!("{} is off this map.", b.name));
                continue;
            }
            if !keep(x, z) {
                continue;
            }
            let (lc_n, lc_d) = push_lc(&mut locale, &mut next_lc, b.name, b.note);
            g.children.push(icon(
                &proto,
                &mut next_id,
                x,
                z,
                BATTLE,
                lc_n,
                lc_d,
                vec![],
                true,
            ));
            added += 1;

            let eastern = battle_eastern_attacker(b);
            let ring = regular_ring(x, z, BATTLE_RADIUS, 6);
            let (ring_icons, rc) = emit_icon_chain(
                "Defend Area",
                b.note,
                &ring,
                faction_style(!eastern, LINE_DEFEND_AREA),
                true,
                true,
                &proto,
                &mut next_id,
                &mut locale,
                &mut next_lc,
            );
            added += rc;
            g.children.extend(ring_icons);

            let (tail, tip) = battle_arrow_ends(x, z, b);
            let path = attack_arrow_points(tail, tip, ARROW_TAIL_WIDTH);
            let (arrow_icons, ac) = emit_attack_chain(
                "Attack",
                b.note,
                &path,
                faction_style(eastern, LINE_ATTACK_ARROW),
                &proto,
                &mut next_id,
                &mut locale,
                &mut next_lc,
            );
            added += ac;
            g.children.extend(arrow_icons);
        }
        icon_count += added;
        if added > 0 {
            children.push(g);
        }
    }

    if opts.buildups {
        let (group, n) = areas_group(
            "Troop buildups",
            snap.buildups,
            LINE_DEFEND_AREA,
            &keep,
            &proto,
            &mut next_id,
            &mut locale,
            &mut next_lc,
        );
        icon_count += n;
        if n > 0 {
            children.push(group);
        }
    }
    if opts.defenses {
        let (group, n) = areas_group(
            "Defensive positions",
            snap.defenses,
            LINE_DEFEND_AREA,
            &keep,
            &proto,
            &mut next_id,
            &mut locale,
            &mut next_lc,
        );
        icon_count += n;
        if n > 0 {
            children.push(group);
        }
    }
    if opts.attacks {
        let (group, n) = attack_arrows_group(
            snap.attacks,
            &keep,
            &proto,
            &mut next_id,
            &mut locale,
            &mut next_lc,
        );
        icon_count += n;
        if n > 0 {
            children.push(group);
        }
    }
    if opts.naval {
        let mut g = named_group("Naval routes", &mut next_id);
        let mut n = 0usize;
        for route in snap.naval {
            let (chain, c) = polyline_icons(
                route.name,
                route.desc,
                route.path,
                NAVAL,
                false,
                &keep,
                &proto,
                &mut next_id,
                &mut locale,
                &mut next_lc,
            );
            n += c;
            g.children.extend(chain);
        }
        icon_count += n;
        if n > 0 {
            children.push(g);
        }
    }

    if opts.influence {
        if let Some((north_mp, south_mp)) = influence_minus_salients(
            &dense_base,
            aabb,
            AOI_GAP,
            true,
            stretch_east,
            &patches,
        ) {
            let mut g = named_group("Areas of influence", &mut next_id);
            let mut n = 0usize;
            for (label, country, mp) in [
                ("DPRK Influence Area", COUNTRY_DPRK, north_mp),
                ("USA Influence Area", COUNTRY_USA, south_mp),
            ] {
                let rings = multipolygon_rings(&mp);
                influence_rings += rings.len();
                for ring in &rings {
                    if open_ring(ring).len() < 3 {
                        continue;
                    }
                    g.children.push(influence_area(label, country, ring, &mut next_id));
                    n += 1;
                }
            }
            icon_count += n;
            if n > 0 {
                children.push(g);
            }
        }
    }

    if !opts.ref_groups.is_empty() {
        let (group, n) = place_ref_groups(&opts.ref_groups, place, &mut next_id);
        icon_count += n;
        if n > 0 {
            children.push(group);
        } else {
            notes.push("No reference groups fall in the selected area (plus 10 km).".into());
        }
    }

    if !opts.fighter_packs.is_empty() {
        let groups = stamp_rooted_packs(
            opts.fighter_packs.iter().map(|p| &p.root),
            &mut next_id,
        );
        children.extend(groups);
    }

    if !opts.ship_packs.is_empty() {
        let groups = stamp_rooted_packs(
            opts.ship_packs.iter().map(|p| &p.root),
            &mut next_id,
        );
        children.extend(groups);
    }

    if !opts.ground_packs.is_empty() {
        let groups = stamp_rooted_packs(
            opts.ground_packs.iter().map(|p| &p.root),
            &mut next_id,
        );
        children.extend(groups);
    }

    let (border, bn) = aoi_border_group(
        aabb, &proto, &mut next_id, &mut locale, &mut next_lc,
    );
    icon_count += bn;
    children.push(border);

    if children.is_empty() {
        return Err("nothing to place in the selected area.".into());
    }

    let mut root = Il2Entity::new("Group");
    root.index = Some(next_id);
    root.set_property("Index", next_id.to_string());
    if opts.custom_front.is_some() {
        root.set_name("Korea Base Map Custom");
    } else if dated {
        root.set_name(&format!("Korea Base Map {}", mark.date_label()));
    } else {
        root.set_name(&format!(
            "Korea Base Map {} {}",
            opts.year,
            ascii_label(opts.season.label())
        ));
    }
    root.set_property("Desc", "\"\"");
    root.children = children;

    Ok(FrontPack {
        root,
        locale,
        period_label: if opts.custom_front.is_some() { "Custom user drawing" } else if dated { mark.title } else { snap.label },
        period_note: if opts.custom_front.is_some() { "Generated from hand-drawn map preview line." } else if dated { mark.note } else { snap.note },
        aircraft: suggested_aircraft(mark.year, mark.season),
        icon_count,
        notes,
        clip_preview: format_clip_preview(
            aabb,
            opts.ref_groups
                .iter()
                .filter(|g| g.entity.first_xz().is_some_and(|(x, z)| keep(x, z)))
                .count(),
            front_runs,
            influence_rings,
        ),
    })
}

fn resolve_timeline_mark(opts: &FrontOptions) -> &'static TimelineMark {
    if TIMELINE.is_empty() {
        panic!("TIMELINE is empty");
    }
    match opts.timeline_idx {
        Some(idx) => &TIMELINE[idx.min(TIMELINE.len() - 1)],
        None => &TIMELINE[timeline_index(opts.year, opts.season)],
    }
}

fn icon_prototype() -> Result<Il2Entity, String> {
    let root = parse_group_file(include_str!("../TemplateExamples/IconHelper.Group"))
        .map_err(|err| format!("IconHelper.Group: {err}"))?;
    root.children
        .iter()
        .find(|c| c.block_type == "MCU_Icon")
        .cloned()
        .ok_or_else(|| "IconHelper.Group has no MCU_Icon".into())
}

fn named_group(name: &str, next_id: &mut i32) -> Il2Entity {
    let mut g = Il2Entity::new("Group");
    g.set_name(&ascii_label(name));
    g.index = Some(*next_id);
    g.set_property("Index", next_id.to_string());
    *next_id += 1;
    g.set_property("Desc", "\"\"");
    g
}

fn clipped_front_group_xz(
    line_name: &str,
    desc: &str,
    xz: &[(f64, f64)],
    style: Style,
    aabb: WorldAabb,
    stretch_east: bool,
    flatten: bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Il2Entity, usize, usize) {
    let dense = densify(xz, LINE_STEP);
    let extended = extend_front_to_aabb_ex(&dense, aabb, true, stretch_east);
    let pts = if flatten {
        densify(&prepare_front(&extended), LINE_STEP)
    } else {
        densify(&extended, LINE_STEP)
    };
    let mut g = named_group("Front line", next_id);
    if pts.len() < 2 {
        return (g, 0, 0);
    }
    let (icons, n) = emit_icon_chain(
        line_name, desc, &pts, style, true, false, proto, next_id, locale, next_lc,
    );
    g.children.extend(icons);
    (g, n, 1)
}

fn areas_group(
    group_name: &str,
    areas: &[Area],
    line: i32,
    keep: &dyn Fn(f64, f64) -> bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Il2Entity, usize) {
    let mut g = named_group(group_name, next_id);
    let mut n = 0usize;
    for area in areas {
        let style = faction_style(area_is_eastern(area), line);
        let (icons, c) = polyline_icons(
            area.name, area.desc, area.ring, style, true, keep, proto, next_id, locale, next_lc,
        );
        n += c;
        g.children.extend(icons);
    }
    (g, n)
}

fn attack_arrows_group(
    areas: &[Area],
    keep: &dyn Fn(f64, f64) -> bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Il2Entity, usize) {
    let mut g = named_group("Areas to attack", next_id);
    let mut n = 0usize;
    for area in areas {
        let xz: Vec<(f64, f64)> = area
            .ring
            .iter()
            .copied()
            .map(|(la, lo)| latlon_to_xz(la, lo))
            .collect();
        if !xz.iter().any(|&(x, z)| keep(x, z)) {
            continue;
        }
        let eastern = area_is_eastern(area);
        let path = attack_arrow_from_area(&xz, area.name, eastern);
        let (icons, c) = emit_attack_chain(
            area.name, area.desc, &path, faction_style(eastern, LINE_ATTACK_ARROW),
            proto, next_id, locale, next_lc,
        );
        n += c;
        g.children.extend(icons);
    }
    (g, n)
}

/// Arrow chain from `CorrectedAttack_arrow.Group`: tail width, shaft fade, point.
pub fn attack_arrow_points(tail: (f64, f64), tip: (f64, f64), tail_width: f64) -> Vec<(f64, f64)> {
    let dx = tip.0 - tail.0;
    let dz = tip.1 - tail.1;
    let len = (dx * dx + dz * dz).sqrt().max(1.0);
    let px = -dz / len;
    let pz = dx / len;
    let hw = tail_width * 0.5;
    let lerp = |t: f64| (tail.0 + dx * t, tail.1 + dz * t);
    let rail = |t: f64, half: f64| {
        let p = lerp(t);
        (p.0 + px * half, p.1 + pz * half)
    };
    vec![
        (tail.0 - px * hw, tail.1 - pz * hw),
        (tail.0 + px * hw, tail.1 + pz * hw),
        rail(0.33, hw * 0.85),
        rail(0.66, hw * 0.55),
        (tip.0 - px * hw * 0.12, tip.1 - pz * hw * 0.12),
        rail(0.82, hw * 1.05),
    ]
}

fn attack_arrow_from_area(ring: &[(f64, f64)], name: &str, eastern: bool) -> Vec<(f64, f64)> {
    let (cx, cz) = centroid(ring);
    let half = ARROW_LENGTH * 0.5;
    let (tail, tip) = if name.contains("Inchon") {
        ((cx, cz - half), (cx, cz + half))
    } else if eastern {
        ((cx + half, cz), (cx - half, cz))
    } else {
        ((cx - half, cz), (cx + half, cz))
    };
    attack_arrow_points(tail, tip, ARROW_TAIL_WIDTH)
}

fn battle_eastern_attacker(b: &Battle) -> bool {
    matches!(b.id, "seoul1" | "chosin" | "seoul3" | "imjin")
}

fn battle_arrow_ends(cx: f64, cz: f64, b: &Battle) -> ((f64, f64), (f64, f64)) {
    let r = BATTLE_RADIUS;
    let len = ARROW_LENGTH;
    let tip = r * 0.25;
    match b.id {
        "inchon" => ((cx, cz - r - len), (cx, cz - tip)),
        "hungnam" => ((cx, cz + r + len), (cx, cz + tip)),
        _ if battle_eastern_attacker(b) => ((cx + r + len, cz), (cx + tip, cz)),
        _ => ((cx - r - len, cz), (cx - tip, cz)),
    }
}

fn regular_ring(cx: f64, cz: f64, radius: f64, n: usize) -> Vec<(f64, f64)> {
    let n = n.max(3);
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            (cx + radius * a.cos(), cz + radius * a.sin())
        })
        .collect()
}

const AOI_BORDER_GAP: f64 = 800.0;

fn aoi_border_group(
    aabb: WorldAabb,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Il2Entity, usize) {
    let inner = zone(110, 90, 0, LINE_POLY);
    let outer = zone(200, 170, 0, LINE_POLY);
    let mut g = named_group("AO outline", next_id);
    let mut n = 0usize;
    for (label, pad, style) in [
        ("AO inner border", 0.0, inner),
        ("AO outer border", AOI_BORDER_GAP, outer),
    ] {
        let ring = aabb_rectangle(aabb, pad);
        let dense = densify(&{
            let mut r = ring.clone();
            r.push(ring[0]);
            r
        }, LINE_STEP);
        let mut pts = dense;
        if pts.len() >= 2 && pts.first() == pts.last() {
            pts.pop();
        }
        let (icons, c) = emit_icon_chain(
            label, "Stay inside this box", &pts, style, true, true, proto, next_id, locale, next_lc,
        );
        n += c;
        g.children.extend(icons);
    }
    (g, n)
}

fn aabb_rectangle(aabb: WorldAabb, pad: f64) -> Vec<(f64, f64)> {
    let a = aabb.expanded(pad);
    vec![
        (a.x_min, a.z_min),
        (a.x_min, a.z_max),
        (a.x_max, a.z_max),
        (a.x_max, a.z_min),
    ]
}

fn place_ref_groups(
    groups: &[MapRefGroup],
    place: WorldAabb,
    next_id: &mut i32,
) -> (Il2Entity, usize) {
    let mut g = named_group("Reference groups", next_id);
    let mut n = 0usize;
    for src in groups {
        let mut entity = src.entity.clone();
        strip_catalog_waypoints(&mut entity);
        prune_to_aabb(&mut entity, place);
        if !spatially_relevant(&entity, place) {
            continue;
        }
        let (clone, _) = duplicate_template(&entity, next_id);
        g.children.push(clone);
        n += 1;
    }
    (g, n)
}

fn stamp_rooted_packs<'a, I>(packs: I, next_id: &mut i32) -> Vec<Il2Entity>
where
    I: IntoIterator<Item = &'a Il2Entity>,
{
    let mut out = Vec::new();
    for root in packs {
        let (clone, _) = duplicate_template(root, next_id);
        out.push(clone);
    }
    out
}

/// Landscape MARKS use MCU_Waypoint as catalog pins. Preview them; do not stamp
/// thousands of waypoints into the generated group (the editor already has the scene).
fn strip_catalog_waypoints(entity: &mut Il2Entity) {
    entity.children.retain_mut(|child| {
        strip_catalog_waypoints(child);
        child.block_type != "MCU_Waypoint"
    });
}

fn prune_to_aabb(entity: &mut Il2Entity, aabb: WorldAabb) {
    entity.children.retain_mut(|child| {
        prune_to_aabb(child, aabb);
        spatially_relevant(child, aabb)
    });
}

fn spatially_relevant(entity: &Il2Entity, aabb: WorldAabb) -> bool {
    if let Some((x, z)) = entity.pos_xz() {
        return aabb.contains(x, z) || !entity.children.is_empty();
    }
    !entity.children.is_empty() || entity.block_type != "Group"
}

fn max_lc(entity: &Il2Entity) -> i32 {
    let mut m = 0i32;
    if let Some(v) = entity.property("LCName").and_then(|s| s.parse().ok()) {
        m = m.max(v);
    }
    if let Some(v) = entity.property("LCDesc").and_then(|s| s.parse().ok()) {
        m = m.max(v);
    }
    for child in &entity.children {
        m = m.max(max_lc(child));
    }
    m
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Airfield,
    LinkedEntity,
    Block,
}

#[derive(Clone, Copy, Debug)]
pub struct PreviewDot {
    pub x: f64,
    pub z: f64,
    pub kind: PreviewKind,
}

/// World points for the map preview. MCU_* are ignored except `MCU_TR_Entity`
/// and landscape `MCU_Waypoint` marks.
pub fn preview_dots(entity: &Il2Entity, cap: usize) -> Vec<PreviewDot> {
    let mut out = Vec::new();
    collect_preview_dots(entity, 0, &mut out, cap.saturating_mul(8).max(cap));
    if out.is_empty() {
        if let Some((x, z)) = entity.first_xz() {
            out.push(PreviewDot {
                x,
                z,
                kind: PreviewKind::Block,
            });
        }
    }
    downsample_dots(out, cap)
}

fn downsample_dots(dots: Vec<PreviewDot>, cap: usize) -> Vec<PreviewDot> {
    if dots.len() <= cap {
        return dots;
    }
    let step = dots.len() as f64 / cap as f64;
    let mut out = Vec::with_capacity(cap);
    let mut t = 0.0;
    while out.len() < cap && (t as usize) < dots.len() {
        out.push(dots[t as usize]);
        t += step;
    }
    out
}

fn collect_preview_dots(entity: &Il2Entity, group_depth: usize, out: &mut Vec<PreviewDot>, cap: usize) {
    if out.len() >= cap {
        return;
    }
    if entity.block_type == "Group" {
        for child in &entity.children {
            collect_preview_dots(child, group_depth + 1, out, cap);
        }
        return;
    }
    if entity.block_type.starts_with("MCU_")
        && entity.block_type != "MCU_TR_Entity"
        && entity.block_type != "MCU_Waypoint"
    {
        return;
    }
    if let Some((x, z)) = entity.pos_xz() {
        if let Some(kind) = classify_preview(&entity.block_type, group_depth) {
            out.push(PreviewDot { x, z, kind });
        }
    }
    for child in &entity.children {
        collect_preview_dots(child, group_depth, out, cap);
    }
}

fn classify_preview(block_type: &str, _group_depth: usize) -> Option<PreviewKind> {
    match block_type {
        "Airfield" => Some(PreviewKind::Airfield),
        "MCU_TR_Entity" => Some(PreviewKind::LinkedEntity),
        "Block" | "Ground" | "Vehicle" | "MCU_Waypoint" => Some(PreviewKind::Block),
        _ => None,
    }
}

fn polyline_icons(
    name: &str,
    desc: &str,
    latlon: &[(f64, f64)],
    style: Style,
    closed: bool,
    keep: &dyn Fn(f64, f64) -> bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Vec<Il2Entity>, usize) {
    let xz: Vec<(f64, f64)> = latlon.iter().copied().map(|(la, lo)| latlon_to_xz(la, lo)).collect();
    polyline_xz_icons(name, desc, &xz, style, closed, keep, proto, next_id, locale, next_lc)
}

fn polyline_xz_icons(
    name: &str,
    desc: &str,
    xz: &[(f64, f64)],
    style: Style,
    closed: bool,
    keep: &dyn Fn(f64, f64) -> bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Vec<Il2Entity>, usize) {
    if closed {
        if xz.len() < 3 || !xz.iter().any(|&(x, z)| keep(x, z)) {
            return (Vec::new(), 0);
        }
        let mut ring = xz.to_vec();
        if ring.first() == ring.last() {
            ring.pop();
        }
        let mut looped = ring.clone();
        looped.push(ring[0]);
        let mut dense = densify(&looped, LINE_STEP);
        if dense.len() >= 2 && dense.first() == dense.last() {
            dense.pop();
        }
        return emit_icon_chain(
            name, desc, &dense, style, true, true, proto, next_id, locale, next_lc,
        );
    }
    let dense = densify(xz, LINE_STEP);
    let runs = clip_runs(&dense, keep);
    let mut out = Vec::new();
    let mut n = 0usize;
    for (run_i, run) in runs.into_iter().enumerate() {
        let (icons, c) = emit_icon_chain(
            name, desc, &run, style, run_i == 0, false, proto, next_id, locale, next_lc,
        );
        n += c;
        out.extend(icons);
    }
    (out, n)
}

fn emit_icon_chain(
    name: &str,
    desc: &str,
    run: &[(f64, f64)],
    style: Style,
    named: bool,
    closed: bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Vec<Il2Entity>, usize) {
    emit_icon_chain_at(
        name, desc, run, style, named, closed, false, proto, next_id, locale, next_lc,
    )
}

/// Attack arrows put `Attack` on the vertex before the tip so the label sits on the head.
fn emit_attack_chain(
    name: &str,
    desc: &str,
    run: &[(f64, f64)],
    style: Style,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Vec<Il2Entity>, usize) {
    emit_icon_chain_at(
        name, desc, run, style, true, false, true, proto, next_id, locale, next_lc,
    )
}

fn emit_icon_chain_at(
    name: &str,
    desc: &str,
    run: &[(f64, f64)],
    style: Style,
    named: bool,
    closed: bool,
    name_second_last: bool,
    proto: &Il2Entity,
    next_id: &mut i32,
    locale: &mut LocaleTable,
    next_lc: &mut i32,
) -> (Vec<Il2Entity>, usize) {
    let run = dedupe_export_xz(run, closed);
    if run.len() < 2 {
        return (Vec::new(), 0);
    }
    let name_i = if !named {
        None
    } else if name_second_last {
        Some(run.len().saturating_sub(2))
    } else {
        Some(0)
    };
    let mut out = Vec::new();
    let ids: Vec<i32> = (0..run.len()).map(|i| *next_id + i as i32).collect();
    for (i, &(x, z)) in run.iter().enumerate() {
        let last = i + 1 == run.len();
        let targets = if last {
            if closed {
                vec![ids[0]]
            } else {
                vec![]
            }
        } else {
            vec![ids[i + 1]]
        };
        let (lc_n, lc_d) = if name_i == Some(i) {
            push_lc(locale, next_lc, name, desc)
        } else {
            (0, 0)
        };
        let end_style = last && !closed;
        out.push(icon(
            proto, next_id, x, z, style, lc_n, lc_d, targets, end_style,
        ));
    }
    let n = out.len();
    (out, n)
}

/// MCU_Icon XPos/ZPos are written as `{:.3}`. Consecutive vertices that round
/// to the same metre-millimetre break LineType 13 (IconId 0) in the editor.
fn export_xz_key(x: f64, z: f64) -> (i64, i64) {
    ((x * 1000.0).round() as i64, (z * 1000.0).round() as i64)
}

fn same_export_xz(a: (f64, f64), b: (f64, f64)) -> bool {
    export_xz_key(a.0, a.1) == export_xz_key(b.0, b.1)
}

fn dedupe_export_xz(run: &[(f64, f64)], closed: bool) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &p in run {
        if out.last().is_some_and(|&q| same_export_xz(p, q)) {
            continue;
        }
        out.push(p);
    }
    if closed && out.len() >= 2 && same_export_xz(out[0], *out.last().unwrap()) {
        out.pop();
    }
    out
}

fn push_lc(locale: &mut LocaleTable, next_lc: &mut i32, name: &str, desc: &str) -> (i32, i32) {
    let n = *next_lc;
    let d = *next_lc + 1;
    *next_lc += 2;
    locale.insert(n, ascii_label(name));
    locale.insert(d, ascii_label(desc));
    (n, d)
}

/// IL-2 .Group files are not UTF-8. En-dashes in Group names / LC strings
/// make the mission editor refuse the import.
fn ascii_label(s: &str) -> String {
    s.replace('\u{2013}', "-")
        .replace('\u{2014}', "-")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201c}', "\"")
        .replace('\u{201d}', "\"")
}

fn icon(
    proto: &Il2Entity,
    next_id: &mut i32,
    x: f64,
    z: f64,
    style: Style,
    lc_name: i32,
    lc_desc: i32,
    targets: Vec<i32>,
    last: bool,
) -> Il2Entity {
    let mut e = proto.clone();
    let id = *next_id;
    *next_id += 1;
    e.index = Some(id);
    e.set_property("Index", id.to_string());
    e.set_targets(targets);
    e.set_objects(vec![]);
    e.set_property("XPos", format!("{x:.3}"));
    e.set_property("YPos", style.ypos);
    e.set_property("ZPos", format!("{z:.3}"));
    e.set_property("LCName", lc_name.to_string());
    e.set_property("LCDesc", lc_desc.to_string());
    e.set_property("IconId", style.icon_id.to_string());
    if last {
        e.set_property("RColor", style.end_r.to_string());
        e.set_property("GColor", style.end_g.to_string());
        e.set_property("BColor", style.end_b.to_string());
        e.set_property("LineType", style.end_line.to_string());
        e.set_property("Coalitions", style.end_coalitions);
    } else {
        e.set_property("RColor", style.r.to_string());
        e.set_property("GColor", style.g.to_string());
        e.set_property("BColor", style.b.to_string());
        e.set_property("LineType", style.body_line.to_string());
        e.set_property("Coalitions", style.body_coalitions);
    }
    e
}

fn open_ring(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut v = pts.to_vec();
    if v.len() >= 2 && v.first() == v.last() {
        v.pop();
    }
    v
}

fn centroid(pts: &[(f64, f64)]) -> (f64, f64) {
    if pts.is_empty() {
        return (0.0, 0.0);
    }
    let n = pts.len() as f64;
    let x = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let z = pts.iter().map(|p| p.1).sum::<f64>() / n;
    (x, z)
}

fn influence_area(name: &str, country: i32, ring: &[(f64, f64)], next_id: &mut i32) -> Il2Entity {
    let open = open_ring(ring);
    let dense = densify(&open, LINE_STEP);
    let (cx, cz) = centroid(&dense);
    let mut e = Il2Entity::new("MCU_TR_InfluenceArea");
    let id = *next_id;
    *next_id += 1;
    e.index = Some(id);
    e.set_property("Index", id.to_string());
    e.set_name(name);
    e.set_property("Desc", "\"\"");
    e.set_targets(vec![]);
    e.set_objects(vec![]);
    e.set_property("XPos", format!("{cx:.3}"));
    e.set_property("YPos", "0.000");
    e.set_property("ZPos", format!("{cz:.3}"));
    e.set_property("XOri", "0");
    e.set_property("YOri", "0");
    e.set_property("ZOri", "0");
    e.set_property("Enabled", "1");
    e.set_property("Country", country.to_string());
    let mut boundary = Il2Entity::new("Boundary");
    for &(x, z) in &dense {
        let x = if country == COUNTRY_USA {
            crate::mapclip::clamp_point_south_of_yalu(x, z)
        } else {
            x
        };
        boundary
            .properties
            .push((String::new(), format!("{:.0}, {:.0}", x.round(), z.round())));
    }
    e.children.push(boundary);
    e
}

pub fn densify(pts: &[(f64, f64)], step: f64) -> Vec<(f64, f64)> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (x0, z0) = w[0];
        let (x1, z1) = w[1];
        let dx = x1 - x0;
        let dz = z1 - z0;
        let dist = (dx * dx + dz * dz).sqrt();
        if dist < 0.001 {
            continue;
        }
        if out.last().is_none_or(|&p| !same_export_xz(p, (x0, z0))) {
            out.push((x0, z0));
        }
        if dist <= step {
            continue;
        }
        let n = (dist / step).floor() as i32;
        for i in 1..n {
            let t = i as f64 / (n as f64);
            let p = (x0 + dx * t, z0 + dz * t);
            if out.last().is_none_or(|&q| !same_export_xz(q, p)) {
                out.push(p);
            }
        }
    }
    if let Some(&last) = pts.last() {
        if out.last().is_none_or(|&p| !same_export_xz(p, last)) {
            out.push(last);
        }
    }
    out
}

fn clip_runs(pts: &[(f64, f64)], keep: &dyn Fn(f64, f64) -> bool) -> Vec<Vec<(f64, f64)>> {
    let mut runs = Vec::new();
    let mut cur = Vec::new();
    for &(x, z) in pts {
        if keep(x, z) {
            cur.push((x, z));
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_group_file, parse_il2_document};
    use crate::serialize::serialize_group;

    #[test]
    fn six_sectors_overlap_and_cover_the_map() {
        let s = sectors();
        assert_eq!(s.len(), 6);
        assert!(s[0].contains(MAP_MIN + 1000.0, MAP_MIN + 1000.0));
        assert!(s[5].contains(MAP_MAX - 1000.0, MAP_MAX - 1000.0));
        let (sx, sz) = latlon_to_xz(crate::geo::REF_LAT, crate::geo::REF_LON);
        assert!(s[1].contains(sx, sz), "Seoul should sit in SC Seoul–Inchon");
        assert_eq!(s[0].id, 0);
        assert!(s[0].name.contains("Yellow Sea"));
        assert!(!s[0].hint.is_empty());
        assert!(s[0].x_max > s[3].x_min, "south/north overlap in X");
        assert!(s[0].z_max > s[1].z_min, "west/center overlap in Z");
        let (px, pz) = latlon_to_xz(39.04, 125.76);
        assert!(s.iter().any(|sec| sec.contains(px, pz)), "Pyongyang on a sector");
    }

    #[test]
    fn summer_1950_has_no_sabre_or_mig() {
        let ids: Vec<_> = suggested_aircraft(1950, Season::Summer)
            .iter()
            .map(|a| a.id)
            .collect();
        assert!(ids.contains(&"f80c10"));
        assert!(ids.contains(&"yak9p"));
        assert!(!ids.contains(&"f86a5"));
        assert!(!ids.contains(&"mig15bis"));
    }

    #[test]
    fn winter_1950_unlocks_sabre_and_mig() {
        let ids: Vec<_> = suggested_aircraft(1950, Season::Winter)
            .iter()
            .map(|a| a.id)
            .collect();
        assert!(ids.contains(&"f86a5"));
        assert!(ids.contains(&"mig15bis"));
        assert!(ids.contains(&"f80c10"));
    }

    #[test]
    fn generate_mlr_has_chained_front_and_locale() {
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::LateSpring,
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.icon_count > 10);
        assert_eq!(pack.root.name(), Some("Korea Base Map 1951 Late spring"));
        assert!(snapshot_label(1951, Season::LateSpring).contains("MLR"));
        assert!(!snapshot_note(1951, Season::LateSpring).is_empty());
        assert!(pack.root.find_by_name("Front line").is_some());
        assert!(pack.locale.contains_text("Imjin River / Kapyong"));
        let front = pack.root.find_by_name("Front line").unwrap();
        let icons: Vec<_> = front
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(icons.len() >= 2);
        assert!(!icons[0].targets.is_empty());
        assert!(icons.last().unwrap().targets.is_empty());
        assert_eq!(icons[0].property("LineType"), Some("13"));
        assert_eq!(icons[0].property("RColor"), Some("255"));
        assert_eq!(icons[0].property("GColor"), Some("255"));
        assert_eq!(icons[0].property("Coalitions"), Some("[1, 2]"));
        assert_eq!(
            icons.last().unwrap().property("LineType"),
            Some("1")
        );
        assert_eq!(icons.last().unwrap().property("GColor"), Some("0"));
        assert_eq!(icons.last().unwrap().property("BColor"), Some("0"));
        assert_eq!(
            icons.last().unwrap().property("Coalitions"),
            Some("[1, 2, 0]")
        );
        assert!(icons.iter().all(|i| i.property("Name").is_none()));
        assert_eq!(icons[0].property("LCName"), Some("2"));
        assert!(
            icons.iter().skip(1).all(|i| i.property("LCName") == Some("0")),
            "unlabeled front vertices must not steal airfield LC strings"
        );
        let usa = pack.root.find_by_name("USA Influence Area").unwrap();
        assert_eq!(usa.block_type, "MCU_TR_InfluenceArea");
        assert_eq!(usa.property("Country"), Some("601"));
        assert_eq!(usa.children[0].block_type, "Boundary");
        assert!(usa.children[0]
            .properties
            .iter()
            .any(|(k, v)| k.is_empty() && v.contains(',')));
        let dprk = pack.root.find_by_name("DPRK Influence Area").unwrap();
        assert_eq!(dprk.property("Country"), Some("503"));
        fn xz_pair(v: &str) -> Option<(f64, f64)> {
            let mut p = v.split(',');
            let x: f64 = p.next()?.trim().parse().ok()?;
            let z: f64 = p.next()?.trim().parse().ok()?;
            Some((x, z))
        }
        assert!(
            dprk.children[0].properties.iter().any(|(_, v)| {
                xz_pair(v).is_some_and(|(x, z)| {
                    crate::geo::yalu_x_at_z(z).is_some_and(|yx| x > yx + 1_000.0)
                })
            }),
            "DPRK AoI must extend north of the Yalu"
        );
        for (_, v) in &usa.children[0].properties {
            if let Some((x, z)) = xz_pair(v) {
                if let Some(yx) = crate::geo::yalu_x_at_z(z) {
                    assert!(
                        x <= yx - 1_000.0,
                        "USA AoI x={x} at z={z} must stay south of Yalu {yx}"
                    );
                }
            }
        }
        assert_eq!(
            pack.locale.get(2),
            Some("Late spring 1951 - Chinese offensive, then the MLR")
        );
        let text = serialize_group(&pack.root);
        parse_group_file(&text).expect("reparse front group");
        assert!(text.is_ascii(), "Group file must stay ASCII for the editor");
    }

    #[test]
    fn aabb_filter_drops_west_coast_when_only_northeast() {
        let mut opts = FrontOptions {
            year: 1950,
            season: Season::Fall,
            aabb: WorldAabb::from_corners(250_000.0, 280_000.0, 470_000.0, 470_000.0),
            ..FrontOptions::default()
        };
        let ne = generate_front(&opts).unwrap();
        opts.aabb = WorldAabb::full_map();
        let all = generate_front(&opts).unwrap();
        assert!(ne.icon_count < all.icon_count);
        assert!(!ne.locale.contains_text("Inchon landing"));
        assert!(all.locale.contains_text("Inchon landing"));
    }

    #[test]
    fn battle_focus_keeps_one_battle() {
        let pack = generate_front(&FrontOptions {
            year: 1950,
            season: Season::Fall,
            battle_focus: Some("inchon"),
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.locale.contains_text("Inchon landing"));
        assert!(!pack.locale.contains_text("Capture of Pyongyang"));
    }

    #[test]
    fn pusan_note_in_summer_1950() {
        let pack = generate_front(&FrontOptions {
            year: 1950,
            season: Season::Summer,
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.notes.iter().any(|n| n.contains("Pusan")));
    }

    #[test]
    fn icon_helper_line_puts_type_22_on_the_end_vertex() {
        let root = parse_group_file(include_str!("../TemplateExamples/IconHelper.Group"))
            .expect("parse IconHelper");
        let icons: Vec<_> = root
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        let start = icons
            .iter()
            .find(|i| !i.targets.is_empty())
            .expect("polyline start");
        let end_id = start.targets[0];
        let end = icons
            .iter()
            .find(|i| i.index == Some(end_id))
            .expect("polyline end");
        assert_eq!(start.property("LineType"), Some("0"));
        assert_eq!(end.property("LineType"), Some("22"));
        assert_eq!(end.property("Coalitions"), Some("[1, 2, 0]"));
        assert!(start.property("Name").is_none());
        assert!(end.property("Name").is_none());
    }

    #[test]
    fn frontline_template_uses_type_13_and_red_end() {
        let root = parse_il2_document(include_str!("../TemplateExamples/FrontLine.Group"))
            .expect("parse FrontLine");
        let icons: Vec<_> = root
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(icons.len() >= 3);
        let end = icons
            .iter()
            .find(|i| i.targets.is_empty())
            .expect("front end");
        assert_eq!(end.property("LineType"), Some("1"));
        assert_eq!(end.property("RColor"), Some("255"));
        assert_eq!(end.property("GColor"), Some("0"));
        assert_eq!(end.property("BColor"), Some("0"));
        assert_eq!(end.property("Coalitions"), Some("[1, 2, 0]"));
        let body = icons
            .iter()
            .find(|i| !i.targets.is_empty())
            .expect("front body");
        assert_eq!(body.property("LineType"), Some("13"));
        assert_eq!(body.property("GColor"), Some("255"));
        assert_eq!(body.property("Coalitions"), Some("[1, 2]"));
        assert!(icons.iter().all(|i| i.property("Name").is_none()));
    }

    #[test]
    fn aoi_template_uses_influence_area_boundaries() {
        let root = parse_il2_document(include_str!("../TemplateExamples/AoI.Group"))
            .expect("parse AoI");
        let usa = root.find_by_name("USA Influence Area").unwrap();
        assert_eq!(usa.block_type, "MCU_TR_InfluenceArea");
        assert_eq!(usa.property("Country"), Some("601"));
        assert_eq!(usa.children[0].block_type, "Boundary");
        assert!(usa.children[0].properties.len() > 10);
        let dprk = root.find_by_name("DPRK Influence Area").unwrap();
        assert_eq!(dprk.property("Country"), Some("503"));
        assert!(dprk.children[0].properties.iter().any(|(k, v)| k.is_empty() && v.contains(',')));
    }

    #[test]
    fn densify_at_4km_has_about_twice_the_8km_vertices() {
        let pts = vec![(100_000.0, 80_000.0), (100_000.0, 160_000.0)];
        let fine = densify(&pts, 4_000.0);
        let coarse = densify(&pts, 8_000.0);
        assert!(fine.len() >= coarse.len() * 2 - 2);
        assert!(fine.len() > coarse.len());
    }

    #[test]
    fn densify_drops_zero_length_segments() {
        let pts = vec![
            (125_691.419, 208_470.669),
            (125_691.419, 208_470.669),
            (126_582.868, 211_154.835),
        ];
        let out = densify(&pts, 4_000.0);
        assert!(out.len() >= 2);
        for w in out.windows(2) {
            assert!(
                !same_export_xz(w[0], w[1]),
                "densify must not keep identical neighbours"
            );
        }
    }

    fn icon_xz(e: &Il2Entity) -> (f64, f64) {
        let x: f64 = e.property("XPos").unwrap().parse().unwrap();
        let z: f64 = e.property("ZPos").unwrap().parse().unwrap();
        (x, z)
    }

    fn front_icons(pack: &FrontPack) -> Vec<&Il2Entity> {
        pack.root
            .find_by_name("Front line")
            .expect("front")
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect()
    }

    #[test]
    fn failed_type_0_fixture_has_zero_length_leading_segment() {
        let root = parse_group_file(include_str!("../TemplateExamples/Failed_type_0.Group"))
            .expect("parse Failed_type_0");
        let icons: Vec<_> = root
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(icons.len() >= 2);
        assert_eq!(icon_xz(icons[0]), icon_xz(icons[1]));
        assert_eq!(icons[0].property("LineType"), Some("13"));
    }

    #[test]
    fn custom_front_omits_duplicate_consecutive_vertices() {
        let pack = generate_front(&FrontOptions {
            custom_front: Some(vec![
                (125_691.419, 208_470.669),
                (125_691.419, 208_470.669),
                (126_582.868, 211_154.835),
                (130_000.0, 220_000.0),
                (140_000.0, 250_000.0),
            ]),
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            ..FrontOptions::default()
        })
        .unwrap();
        let icons = front_icons(&pack);
        assert!(icons.len() >= 2);
        for w in icons.windows(2) {
            assert_ne!(
                icon_xz(w[0]),
                icon_xz(w[1]),
                "LineType 13 cannot have a zero-length segment"
            );
        }
    }

    #[test]
    fn user_attack_arrows_export_line_type_11_colored_by_tail_side() {
        let front = snapshot_front_xz(1951, Season::LateSpring);
        let (fx, fz) = front[front.len() / 2];
        let north_tail = (fx + 25_000.0, fz);
        let south_tail = (fx - 25_000.0, fz);
        let pack = generate_front(&FrontOptions {
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            user_attacks: vec![
                (north_tail, (fx, fz + 12_000.0)),
                (south_tail, (fx, fz + 12_000.0)),
            ],
            ..FrontOptions::default()
        })
        .unwrap();
        let group = pack.root.find_by_name("Attack arrows").expect("attack arrows");
        let icons: Vec<_> = group
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(icons.len() >= 12);
        assert!(icons.iter().all(|c| c.property("LineType") == Some("11")));
        let red = icons.iter().any(|c| c.property("RColor") == Some("155"));
        let blue = icons.iter().any(|c| c.property("BColor") == Some("150"));
        assert!(red, "tail north of the front should be Eastern red");
        assert!(blue, "tail south of the front should be NATO blue");
        let first_arrow: Vec<_> = icons.iter().take(6).copied().collect();
        assert!(first_arrow.len() >= 6);
        assert_eq!(first_arrow[0].property("LCName"), Some("0"));
        let labeled = first_arrow[first_arrow.len() - 2];
        assert_ne!(labeled.property("LCName"), Some("0"));
        assert_eq!(icons.last().unwrap().property("LCName"), Some("0"));
        assert!(pack.locale.contains_text("Attack"));
    }

    #[test]
    fn base_map_export_skips_historical_context_layers() {
        let pack = generate_front(&FrontOptions {
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.root.find_by_name("Front line").is_some());
        assert!(pack.root.find_by_name("Areas of influence").is_some());
        assert!(pack.root.find_by_name("AO outline").is_some());
        assert!(pack.root.find_by_name("Battles").is_none());
        assert!(pack.root.find_by_name("Troop buildups").is_none());
        assert!(pack.root.find_by_name("Defensive positions").is_none());
        assert!(pack.root.find_by_name("Areas to attack").is_none());
        assert!(pack.root.find_by_name("Naval routes").is_none());
    }

    #[test]
    fn attack_arrow_has_tail_width_and_point() {
        let pts = attack_arrow_points((90_000.0, 200_000.0), (122_000.0, 200_000.0), 4_200.0);
        assert_eq!(pts.len(), 6);
        let tail = (pts[0].1 - pts[1].1).abs();
        assert!((tail - 4_200.0).abs() < 1.0);
        let tip_x = pts[4].0.max(pts[5].0);
        assert!(tip_x > pts[0].0);
    }

    #[test]
    fn seoul_box_keeps_k13_and_does_not_emit_town_icons() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("References/K13 AFB_mp.Group");
        let text = std::fs::read_to_string(&path).expect("read K13");
        let entity = parse_group_file(&text).expect("parse K13");
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::LateSpring,
            aabb: WorldAabb::from_corners(60_000.0, 270_000.0, 130_000.0, 310_000.0),
            ref_groups: vec![MapRefGroup { path, entity }],
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.root.find_by_name("K13 AFB").is_some());
        assert!(pack.root.find_by_name("Airfields").is_none());
        assert!(pack.root.find_by_name("Buildings").is_none());
        let text = serialize_group(&pack.root);
        assert!(!text.contains("IconId = 102;"), "no building town icons");
    }

    #[test]
    fn troop_buildup_uses_defend_ring_line_type() {
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::LateSpring,
            ..FrontOptions::default()
        })
        .unwrap();
        let buildups = pack.root.find_by_name("Troop buildups").expect("buildups");
        let icon = buildups
            .children
            .iter()
            .find(|c| c.block_type == "MCU_Icon")
            .expect("buildup icon");
        assert_eq!(icon.property("LineType"), Some("12"));
        let defense = pack.root.find_by_name("Defensive positions").expect("defense");
        let def_icon = defense
            .children
            .iter()
            .find(|c| c.block_type == "MCU_Icon")
            .expect("defense icon");
        assert_eq!(def_icon.property("LineType"), Some("12"));
        let last = defense
            .children
            .iter()
            .rev()
            .find(|c| c.block_type == "MCU_Icon")
            .unwrap();
        assert!(!last.targets.is_empty(), "defend ring is a closed loop");
    }

    #[test]
    fn attack_axes_use_corrected_arrow_line_type() {
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::EarlySpring,
            ..FrontOptions::default()
        })
        .unwrap();
        let attacks = pack.root.find_by_name("Areas to attack").expect("attacks");
        let attack_icon = attacks
            .children
            .iter()
            .find(|c| c.block_type == "MCU_Icon")
            .expect("attack icon");
        assert_eq!(attack_icon.property("LineType"), Some("11"));
        assert_eq!(attack_icon.property("RColor"), Some("0"));
        assert_eq!(attack_icon.property("GColor"), Some("120"));
        assert_eq!(attack_icon.property("BColor"), Some("150"));
        assert!(attacks.children.iter().filter(|c| c.block_type == "MCU_Icon").count() >= 6);
        assert!(attacks
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .all(|c| c.property("LineType") == Some("11")));
    }

    #[test]
    fn battle_gets_defend_ring_and_attack_arrow() {
        let pack = generate_front(&FrontOptions {
            year: 1950,
            season: Season::LateSpring,
            battle_focus: Some("seoul1"),
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.locale.contains_text("First Battle of Seoul"));
        assert!(pack.locale.contains_text("Defend Area"));
        assert!(pack.locale.contains_text("Attack"));
        let battles = pack.root.find_by_name("Battles").expect("battles");
        let types: Vec<_> = battles
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .filter_map(|c| c.property("LineType"))
            .collect();
        assert!(types.contains(&"11"));
        assert!(types.contains(&"12"));
        let arrow = battles
            .children
            .iter()
            .find(|c| c.property("LineType") == Some("11"))
            .unwrap();
        assert_eq!(arrow.property("RColor"), Some("155"));
        assert_eq!(arrow.property("GColor"), Some("0"));
        assert_eq!(arrow.property("BColor"), Some("0"));
    }

    #[test]
    fn aoi_double_border_uses_gold_pair() {
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::LateSpring,
            aabb: WorldAabb::from_corners(80_000.0, 200_000.0, 140_000.0, 280_000.0),
            ..FrontOptions::default()
        })
        .unwrap();
        let outline = pack.root.find_by_name("AO outline").expect("AO outline");
        let icons: Vec<_> = outline
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(icons.len() >= 8);
        assert!(icons.iter().any(|i| {
            i.property("RColor") == Some("110")
                && i.property("GColor") == Some("90")
                && i.property("BColor") == Some("0")
        }));
        assert!(icons.iter().any(|i| {
            i.property("RColor") == Some("200")
                && i.property("GColor") == Some("170")
                && i.property("BColor") == Some("0")
        }));
        let inner = icons
            .iter()
            .find(|i| i.property("RColor") == Some("110"))
            .unwrap();
        let outer = icons
            .iter()
            .find(|i| i.property("RColor") == Some("200"))
            .unwrap();
        let iz: f64 = inner.property("ZPos").unwrap().parse().unwrap();
        let oz: f64 = outer.property("ZPos").unwrap().parse().unwrap();
        assert!((oz - iz).abs() >= 700.0);
        assert!((oz - iz).abs() <= 1_200.0);
    }

    #[test]
    fn corrected_templates_use_line_types_11_and_12() {
        let attack = parse_group_file(include_str!("../TemplateExamples/CorrectedAttack_arrow.Group"))
            .expect("attack");
        let defend = parse_group_file(include_str!("../TemplateExamples/CorrectedDefendArea.Group"))
            .expect("defend");
        let a_icons: Vec<_> = attack
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        let d_icons: Vec<_> = defend
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(a_icons.len() >= 6);
        assert!(a_icons.iter().all(|i| i.property("LineType") == Some("11")));
        assert!(d_icons.len() >= 6);
        assert!(d_icons.iter().all(|i| i.property("LineType") == Some("12")));
        let end = d_icons.iter().find(|i| i.targets.contains(&50792));
        assert!(end.is_some(), "defend ring closes to the first icon");
    }

    #[test]
    fn salient_reference_uses_line_type_4() {
        let root = parse_il2_document(include_str!("../TemplateExamples/SalientReference.Group"))
            .expect("SalientReference");
        let icons: Vec<_> = root
            .children
            .iter()
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(icons.len() >= 4);
        assert!(icons.iter().all(|i| i.property("LineType") == Some("4")));
    }

    #[test]
    fn fall_1950_front_stays_south_of_the_yalu() {
        let front = snapshot_front_xz(1950, Season::Fall);
        assert!(front.len() >= 4);
        for &(x, z) in &front {
            if let Some(yx) = crate::geo::yalu_x_at_z(z) {
                assert!(
                    x <= yx - 1_000.0,
                    "front northing {x} must stay south of Yalu {yx} at z={z}"
                );
            }
        }
    }

    #[test]
    fn preview_slider_lerps_between_snapshots() {
        assert!(TIMELINE.len() >= 20);
        let a = preview_front_xz(0.0);
        let mid = preview_front_xz(0.5);
        assert!(!a.is_empty());
        assert!(!mid.is_empty());
        // Adjacent marks 0–1 share the 38th; lerp still returns a polyline.
        let later = preview_front_xz(2.0);
        let between = preview_front_xz(1.5);
        assert_ne!(a, later);
        assert!(!between.is_empty());
        assert_ne!(between, a);
        assert_ne!(between, later);
    }

    #[test]
    fn hungnam_timeline_does_not_auto_seed_a_salient() {
        let idx = TIMELINE
            .iter()
            .position(|m| m.year == 1950 && m.month == 12 && m.day == 11)
            .expect("11 Dec 1950");
        let pack = generate_front(&FrontOptions {
            year: 1950,
            season: Season::Winter,
            timeline_idx: Some(idx),
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.root.find_by_name("Hungnam-Wonsan pocket").is_none());
        let icons: Vec<_> = pack
            .root
            .children
            .iter()
            .flat_map(|c| c.children.iter())
            .filter(|c| c.block_type == "MCU_Icon")
            .collect();
        assert!(
            !icons.iter().any(|i| i.property("LineType") == Some("4")),
            "Hungnam pockets are not auto-exported; draw a salient by hand"
        );
        assert!(pack.period_label.contains("Hungnam"));
        assert_eq!(pack.root.name(), Some("Korea Base Map 1950-12-11"));
        let text = crate::serialize::serialize_group(&pack.root);
        assert!(
            text.is_ascii(),
            "en-dashes in Group names make the mission editor refuse the import"
        );
        assert!(!text.contains('\u{2013}') && !text.contains('\u{2014}'));
    }

    #[test]
    fn exported_salient_ring_is_clipped_to_aabb() {
        let aabb = WorldAabb::from_corners(80_000.0, 100_000.0, 160_000.0, 220_000.0);
        let pack = generate_front(&FrontOptions {
            aabb,
            custom_front: Some(vec![
                (110_000.0, 90_000.0),
                (110_000.0, 230_000.0),
            ]),
            salients: vec![vec![
                (110_000.0, 130_000.0),
                (250_000.0, 150_000.0),
                (250_000.0, 180_000.0),
                (110_000.0, 190_000.0),
            ]],
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            influence: false,
            ..FrontOptions::default()
        })
        .unwrap();
        let icons: Vec<_> = pack
            .root
            .children
            .iter()
            .flat_map(|c| c.children.iter())
            .filter(|c| c.block_type == "MCU_Icon")
            .filter(|c| c.property("LineType") == Some("4"))
            .collect();
        assert!(!icons.is_empty(), "salient fill should still export");
        for icon in icons {
            let (x, z) = icon_xz(icon);
            assert!(
                aabb.expanded(2.0).contains(x, z),
                "salient vertex ({x}, {z}) must stay in the AO"
            );
        }
    }

    #[test]
    fn generate_without_pocket_date_has_no_pocket_group() {
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::LateSpring,
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(pack.root.find_by_name("Hungnam-Wonsan pocket").is_none());
        let idx = TIMELINE
            .iter()
            .position(|m| m.year == 1950 && m.month == 12 && m.day == 24)
            .expect("24 Dec 1950");
        let dated = generate_front(&FrontOptions {
            timeline_idx: Some(idx),
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(dated.root.find_by_name("Hungnam-Wonsan pocket").is_none());
    }

    #[test]
    fn preview_dots_classifies_and_shows_landscape_waypoints() {
        fn positioned(block: &str, x: f64, z: f64) -> Il2Entity {
            let mut e = Il2Entity::new(block);
            e.set_property("XPos", format!("{x:.3}"));
            e.set_property("ZPos", format!("{z:.3}"));
            e
        }
        let mut root = Il2Entity::new("Group");
        root.children.push(positioned("Airfield", 1000.0, 2000.0));
        root.children.push(positioned("MCU_TR_Entity", 1100.0, 2100.0));
        root.children.push(positioned("Block", 1200.0, 2200.0));
        root.children.push(positioned("MCU_Waypoint", 1300.0, 2300.0));
        root.children.push(positioned("MCU_Timer", 1500.0, 2500.0));
        let mut nested = Il2Entity::new("Group");
        nested.children.push(positioned("Block", 1400.0, 2400.0));
        root.children.push(nested);

        let dots = preview_dots(&root, 800);
        assert_eq!(dots.len(), 5);
        assert!(dots.iter().any(|d| d.kind == PreviewKind::Airfield
            && (d.x - 1000.0).abs() < 0.1
            && (d.z - 2000.0).abs() < 0.1));
        assert!(dots.iter().any(|d| d.kind == PreviewKind::LinkedEntity
            && (d.x - 1100.0).abs() < 0.1));
        assert!(dots.iter().any(|d| d.kind == PreviewKind::Block
            && (d.x - 1200.0).abs() < 0.1));
        assert!(dots.iter().any(|d| d.kind == PreviewKind::Block
            && (d.x - 1400.0).abs() < 0.1));
        assert!(dots.iter().any(|d| d.kind == PreviewKind::Block
            && (d.x - 1300.0).abs() < 0.1));
        assert!(!dots.iter().any(|d| (d.x - 1500.0).abs() < 0.1));
    }

    #[test]
    fn map_fighters_export_packs_without_wave_icons() {
        let tpl = parse_group_file(include_str!(
            "../TemplateExamples/Eastern_Fighters_Random_3pack_V6.Group"
        ))
        .expect("parse 3pack");
        let mut root = crate::pack::generate_pack_at(
            &tpl,
            &[(110_000.0, 280_000.0), (130_000.0, 300_000.0)],
            "NATO Fighters Wave 1 pack 1",
        )
        .unwrap();
        let aabb = WorldAabb::from_corners(80_000.0, 250_000.0, 160_000.0, 330_000.0);
        let rtb = [
            crate::mapfighters::rtb_ao_point(false, 110_000.0, 280_000.0, aabb),
            crate::mapfighters::rtb_ao_point(false, 130_000.0, 300_000.0, aabb),
        ];
        crate::pack::park_rtbs(&mut root, &rtb);
        let pack = generate_front(&FrontOptions {
            year: 1951,
            season: Season::LateSpring,
            aabb,
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            fighter_packs: vec![MapFighterPack { root }],
            ..FrontOptions::default()
        })
        .unwrap();
        assert!(!pack.locale.contains_text("Wave 1"));
        let g1 = pack.root.find_by_name("Group 1").expect("Group 1");
        assert!(
            g1.children
                .iter()
                .all(|c| c.block_type != "MCU_Icon"),
            "wave icons should not be exported"
        );
        let wp = pack.root.find_by_name("RTB - 1").expect("RTB");
        let (x, z) = wp.pos_xz().unwrap();
        assert!((x - rtb[0].0).abs() < 1.0 && (z - rtb[0].1).abs() < 1.0);
        assert!(pack.root.find_by_name("NodeGates").is_some());
    }
}
