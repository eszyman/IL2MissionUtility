//! Dated Korean War front polylines for the Map slider.
//!
//! Spacing is not uniform: weekly (or tighter) while the line is racing in 1950,
//! monthly through mid-1951, then seasonal once the MLR freezes.

use super::Season;
use crate::geo::latlon_to_xz;
use crate::mapclip::prepare_front;

#[derive(Clone, Copy, Debug)]
pub struct TimelineMark {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub season: Season,
    pub title: &'static str,
    pub note: &'static str,
    pub front: &'static [(f64, f64)],
    /// Closed lat/lon ring: UN east-coast pocket (Wonsan–Hamhung–Hungnam).
    pub pocket: Option<&'static [(f64, f64)]>,
}

impl TimelineMark {
    pub fn date_label(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Landscape season to load in the mission editor.
    pub fn editor_map(self) -> &'static str {
        match self.season {
            Season::EarlySpring | Season::LateSpring => "Spring",
            Season::Summer => "Summer",
            Season::Fall => "Autumn",
            Season::Winter => "Winter",
        }
    }

    pub fn editor_hint(self) -> String {
        format!(
            "Editor map: {} {} (IL-2 landscape)",
            self.editor_map(),
            self.year
        )
    }
}

const F38: &[(f64, f64)] = &[
    (38.00, 124.90),
    (38.00, 125.60),
    (38.00, 126.20),
    (38.00, 126.80),
    (38.00, 127.40),
    (38.00, 128.00),
    (38.00, 128.55),
];

const JUN28: &[(f64, f64)] = &[
    (37.48, 126.45),
    (37.52, 126.95),
    (37.42, 127.45),
    (37.35, 128.05),
    (37.32, 128.55),
];

const JUL08: &[(f64, f64)] = &[
    (37.22, 126.45),
    (37.20, 127.00),
    (37.18, 127.55),
    (37.16, 128.10),
    (37.14, 128.55),
];

const PUSAN: &[(f64, f64)] = &[
    (37.08, 126.40),
    (37.06, 127.00),
    (37.08, 127.60),
    (37.12, 128.20),
    (37.16, 128.60),
];

const SEP22: &[(f64, f64)] = &[
    (37.40, 126.40),
    (37.48, 126.90),
    (37.35, 127.40),
    (37.20, 128.00),
    (37.14, 128.55),
];

const SEP28: &[(f64, f64)] = &[
    (37.70, 126.50),
    (37.75, 126.95),
    (37.62, 127.45),
    (37.45, 128.05),
    (37.30, 128.55),
];

const OCT10: &[(f64, f64)] = &[
    (38.15, 125.10),
    (38.20, 125.80),
    (38.10, 126.50),
    (38.05, 127.20),
    (37.95, 127.90),
    (37.85, 128.45),
];

const OCT19: &[(f64, f64)] = &[
    (39.15, 124.80),
    (39.10, 125.40),
    (39.05, 125.95),
    (39.15, 126.55),
    (38.95, 127.25),
    (38.65, 127.85),
    (38.35, 128.35),
];

const OCT26: &[(f64, f64)] = &[
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

const NOV15: &[(f64, f64)] = &[
    (39.70, 124.70),
    (39.65, 125.30),
    (39.55, 125.90),
    (39.70, 126.50),
    (40.05, 127.10),
    (40.20, 127.40),
    (39.85, 127.70),
    (39.40, 127.90),
    (39.05, 128.30),
];

const NOV27: &[(f64, f64)] = &[
    (39.20, 124.90),
    (39.10, 125.50),
    (38.90, 126.10),
    (38.70, 126.70),
    (39.40, 127.20),
    (40.15, 127.45),
    (39.70, 127.75),
    (39.20, 128.05),
];

/// 8th Army in the west; east coast is the Hungnam–Wonsan pocket, not this line.
const DEC05: &[(f64, f64)] = &[
    (38.25, 124.95),
    (38.15, 125.70),
    (38.00, 126.40),
    (37.90, 126.95),
    (38.05, 127.35),
];

const DEC15: &[(f64, f64)] = &[
    (37.85, 125.00),
    (37.75, 125.80),
    (37.65, 126.50),
    (37.55, 127.00),
    (37.70, 127.40),
];

const DEC24: &[(f64, f64)] = &[
    (37.55, 126.50),
    (37.50, 126.95),
    (37.65, 127.50),
    (37.90, 128.10),
    (38.10, 128.50),
];

const JAN04: &[(f64, f64)] = &[
    (37.35, 126.45),
    (37.40, 126.95),
    (37.32, 127.45),
    (37.45, 128.00),
    (37.55, 128.50),
];

const FEB15: &[(f64, f64)] = &[
    (37.55, 126.50),
    (37.62, 126.95),
    (37.58, 127.40),
    (37.70, 128.00),
    (37.80, 128.50),
];

const MAR14: &[(f64, f64)] = &[
    (37.72, 126.55),
    (37.85, 126.95),
    (37.98, 127.40),
    (38.08, 128.00),
    (38.12, 128.50),
];

const APR22: &[(f64, f64)] = &[
    (37.95, 126.55),
    (38.08, 126.95),
    (38.18, 127.35),
    (38.22, 127.90),
    (38.28, 128.50),
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

/// UN coastal pocket: Wonsan south, Hamhung/Hungnam north. Closed ring.
const POCKET_HUNGNAM: &[(f64, f64)] = &[
    (39.12, 127.25),
    (39.35, 127.12),
    (39.70, 127.18),
    (40.02, 127.38),
    (40.00, 127.72),
    (39.72, 127.88),
    (39.35, 127.82),
    (39.10, 127.55),
];

pub const TIMELINE: &[TimelineMark] = &[
    TimelineMark {
        year: 1950,
        month: 6,
        day: 1,
        season: Season::LateSpring,
        title: "1 June 1950 — 38th Parallel",
        note: "Pre-invasion. ROK on the parallel; NKPA assembling north of it.",
        front: F38,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 6,
        day: 25,
        season: Season::Summer,
        title: "25 June 1950 — invasion",
        note: "NKPA crosses the 38th. The line has only just begun to sag south.",
        front: F38,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 6,
        day: 28,
        season: Season::Summer,
        title: "28 June 1950 — Seoul falls",
        note: "First Battle of Seoul. Front just south of the Han.",
        front: JUN28,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 7,
        day: 8,
        season: Season::Summer,
        title: "8 July 1950 — drive south",
        note: "Osan and Pyongtaek lost. Taejon is next.",
        front: JUL08,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 7,
        day: 20,
        season: Season::Summer,
        title: "20 July 1950 — Taejon",
        note: "NKPA takes Taejon. UN falls back toward the Naktong. Most of this line is off the south edge of the Korea map.",
        front: PUSAN,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 8,
        day: 4,
        season: Season::Summer,
        title: "4 August 1950 — Pusan Perimeter",
        note: "UN holds a box around Pusan, south of this map. The drawn line is the north edge of what still fits.",
        front: PUSAN,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 8,
        day: 20,
        season: Season::Summer,
        title: "20 August 1950 — Naktong battles",
        note: "Perimeter holds. Still off the south edge of this map.",
        front: PUSAN,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 9,
        day: 15,
        season: Season::Fall,
        title: "15 September 1950 — Inchon",
        note: "X Corps lands at Inchon. The main NKPA front is still around Pusan.",
        front: PUSAN,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 9,
        day: 22,
        season: Season::Fall,
        title: "22 September 1950 — breakout",
        note: "Pusan breakout meets the Inchon force. Front jumps north through Suwon.",
        front: SEP22,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 9,
        day: 28,
        season: Season::Fall,
        title: "28 September 1950 — Seoul retaken",
        note: "Second Battle of Seoul. Front north of the Han, still short of the 38th in the east.",
        front: SEP28,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 10,
        day: 10,
        season: Season::Fall,
        title: "10 October 1950 — across the 38th",
        note: "UN crosses the parallel. Wonsan is the next east-coast prize.",
        front: OCT10,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 10,
        day: 19,
        season: Season::Fall,
        title: "19 October 1950 — Pyongyang",
        note: "ROK/US enter Pyongyang. Race for the Yalu begins.",
        front: OCT19,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 10,
        day: 26,
        season: Season::Fall,
        title: "26 October 1950 — toward the Yalu",
        note: "8th Army in the west and X Corps in the east push north. Line stays on the Korean bank of the Yalu.",
        front: OCT26,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 11,
        day: 1,
        season: Season::Fall,
        title: "1 November 1950 — Unsan",
        note: "First Chinese blow at Unsan. The UN line is still far north.",
        front: OCT26,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 11,
        day: 15,
        season: Season::Fall,
        title: "15 November 1950 — pause",
        note: "UN still north, but the PVA is across the Yalu in force.",
        front: NOV15,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 11,
        day: 27,
        season: Season::Winter,
        title: "27 November 1950 — Chosin / Kunu-ri",
        note: "PVA Second Phase Offensive. Chosin is surrounded; 8th Army starts to break in the west.",
        front: NOV27,
        pocket: None,
    },
    TimelineMark {
        year: 1950,
        month: 12,
        day: 5,
        season: Season::Winter,
        title: "5 December 1950 — Pyongyang lost, east-coast pocket",
        note: "8th Army is back toward the 38th. X Corps holds a pocket around Wonsan, Hamhung, and Hungnam.",
        front: DEC05,
        pocket: Some(POCKET_HUNGNAM),
    },
    TimelineMark {
        year: 1950,
        month: 12,
        day: 11,
        season: Season::Winter,
        title: "11 December 1950 — Hungnam evacuation",
        note: "Wonsan is gone. Hungnam is the remaining UN bubble while the sea lift runs.",
        front: DEC15,
        pocket: Some(POCKET_HUNGNAM),
    },
    TimelineMark {
        year: 1950,
        month: 12,
        day: 24,
        season: Season::Winter,
        title: "24 December 1950 — Hungnam complete",
        note: "X Corps is off the beach. One front again, south of Seoul in the west.",
        front: DEC24,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 1,
        day: 4,
        season: Season::Winter,
        title: "4 January 1951 — Seoul lost again",
        note: "Third Battle of Seoul. PVA/NKPA take the capital.",
        front: JAN04,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 1,
        day: 25,
        season: Season::Winter,
        title: "25 January 1951 — Thunderbolt",
        note: "UN probe back toward the Han.",
        front: FEB15,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 2,
        day: 20,
        season: Season::Winter,
        title: "20 February 1951 — Killer",
        note: "Ridgway’s limited offensives grind north.",
        front: FEB15,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 3,
        day: 14,
        season: Season::EarlySpring,
        title: "14 March 1951 — Seoul retaken",
        note: "Operation Ripper. UN back in Seoul.",
        front: MAR14,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 4,
        day: 22,
        season: Season::LateSpring,
        title: "22 April 1951 — Chinese spring offensive",
        note: "Imjin and Kapyong. The line bows but does not break at Seoul.",
        front: APR22,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 6,
        day: 15,
        season: Season::LateSpring,
        title: "15 June 1951 — MLR forms",
        note: "Front settles near the 38th. Kansas Line / MLR.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 9,
        day: 1,
        season: Season::Fall,
        title: "1 September 1951 — Bloody Ridge",
        note: "Hill battles on the eastern MLR. Line barely moves.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 10,
        day: 15,
        season: Season::Fall,
        title: "15 October 1951 — Heartbreak Ridge",
        note: "Punchbowl / Heartbreak. Talks at Kaesong/Panmunjom.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1951,
        month: 12,
        day: 1,
        season: Season::Winter,
        title: "1 December 1951 — stalemate",
        note: "Outpost war. Air war over MiG Alley.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1952,
        month: 4,
        day: 1,
        season: Season::LateSpring,
        title: "1 April 1952 — stalemate",
        note: "Same Kansas Line. Wonsan still under naval siege.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1952,
        month: 7,
        day: 1,
        season: Season::Summer,
        title: "1 July 1952 — outpost war",
        note: "Raids along the MLR. No major shift.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1952,
        month: 10,
        day: 14,
        season: Season::Fall,
        title: "14 October 1952 — Triangle Hill",
        note: "Shangganling. Local, costly, the MLR holds.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1953,
        month: 1,
        day: 1,
        season: Season::Winter,
        title: "1 January 1953 — outpost war",
        note: "Old Baldy and the approaches to Pork Chop.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1953,
        month: 4,
        day: 16,
        season: Season::LateSpring,
        title: "16 April 1953 — Pork Chop Hill",
        note: "Heavy outpost fights while the armistice is drafted.",
        front: MLR,
        pocket: None,
    },
    TimelineMark {
        year: 1953,
        month: 7,
        day: 27,
        season: Season::Summer,
        title: "27 July 1953 — armistice",
        note: "Ceasefire. Line of contact becomes the DMZ.",
        front: MLR,
        pocket: None,
    },
];

pub fn timeline_index(year: u16, season: Season) -> usize {
    TIMELINE
        .iter()
        .position(|m| m.year == year && m.season == season)
        .unwrap_or(0)
}

pub fn mark_for_battle(id: &str) -> usize {
    let (y, mo, d) = match id {
        "seoul1" => (1950, 6, 28),
        "inchon" => (1950, 9, 15),
        "seoul2" => (1950, 9, 28),
        "pyongyang" => (1950, 10, 19),
        "chosin" => (1950, 11, 27),
        "hungnam" => (1950, 12, 11),
        "seoul3" => (1951, 1, 4),
        "ripper" => (1951, 3, 14),
        "imjin" => (1951, 4, 22),
        "bloody" => (1951, 9, 1),
        "heartbreak" | "punchbowl" => (1951, 10, 15),
        "triangle" => (1952, 10, 14),
        "porkchop" => (1953, 4, 16),
        _ => (1951, 6, 15),
    };
    TIMELINE
        .iter()
        .position(|m| m.year == y && m.month == mo && m.day == d)
        .or_else(|| TIMELINE.iter().position(|m| m.year == y && m.month == mo))
        .unwrap_or(0)
}

pub fn front_xz(mark: &TimelineMark) -> Vec<(f64, f64)> {
    let raw: Vec<(f64, f64)> = mark
        .front
        .iter()
        .copied()
        .map(|(la, lo)| latlon_to_xz(la, lo))
        .collect();
    prepare_front(&raw)
}

pub fn pocket_xz(mark: &TimelineMark) -> Option<Vec<(f64, f64)>> {
    let ring = mark.pocket?;
    let mut xz: Vec<(f64, f64)> = ring
        .iter()
        .copied()
        .map(|(la, lo)| latlon_to_xz(la, lo))
        .collect();
    if xz.len() >= 2 && xz.first() != xz.last() {
        xz.push(xz[0]);
    }
    Some(xz)
}

/// Front at slider index `t` in `0 ..= n-1`, interpolating between adjacent marks.
pub fn preview_front_xz(t: f32) -> Vec<(f64, f64)> {
    if TIMELINE.is_empty() {
        return Vec::new();
    }
    let max = (TIMELINE.len() - 1) as f32;
    let t = t.clamp(0.0, max);
    let i0 = t.floor() as usize;
    let i1 = (i0 + 1).min(TIMELINE.len() - 1);
    let frac = (t - i0 as f32) as f64;
    let a = front_xz(&TIMELINE[i0]);
    if i0 == i1 || frac < 1e-4 {
        return a;
    }
    let b = front_xz(&TIMELINE[i1]);
    lerp_fronts(&a, &b, frac)
}

pub fn preview_pocket_xz(t: f32) -> Option<Vec<(f64, f64)>> {
    if TIMELINE.is_empty() {
        return None;
    }
    let i = t.round().clamp(0.0, (TIMELINE.len() - 1) as f32) as usize;
    pocket_xz(&TIMELINE[i])
}

fn lerp_fronts(a: &[(f64, f64)], b: &[(f64, f64)], t: f64) -> Vec<(f64, f64)> {
    let mut zs: Vec<f64> = a.iter().map(|p| p.1).chain(b.iter().map(|p| p.1)).collect();
    zs.sort_by(|x, y| x.partial_cmp(y).unwrap());
    zs.dedup_by(|x, y| (*x - *y).abs() < 200.0);
    zs.into_iter()
        .map(|z| {
            let xa = interp_x(a, z);
            let xb = interp_x(b, z);
            (xa * (1.0 - t) + xb * t, z)
        })
        .collect()
}

fn interp_x(line: &[(f64, f64)], z: f64) -> f64 {
    if line.is_empty() {
        return 0.0;
    }
    if z <= line[0].1 {
        return line[0].0;
    }
    if let Some(last) = line.last() {
        if z >= last.1 {
            return last.0;
        }
    }
    for w in line.windows(2) {
        let (x0, z0) = w[0];
        let (x1, z1) = w[1];
        if z >= z0 && z <= z1 {
            let u = (z - z0) / (z1 - z0).max(1e-6);
            return x0 + (x1 - x0) * u;
        }
    }
    line.last().map(|p| p.0).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_war_is_finer_than_stalemate() {
        let weeks_1950: usize = TIMELINE.iter().filter(|m| m.year == 1950).count();
        let marks_1952: usize = TIMELINE.iter().filter(|m| m.year == 1952).count();
        assert!(weeks_1950 >= 12, "1950 should be week/month scale, got {weeks_1950}");
        assert!(marks_1952 <= 5, "1952 should stay coarse, got {marks_1952}");
    }

    #[test]
    fn hungnam_marks_have_a_pocket() {
        let n = TIMELINE.iter().filter(|m| m.pocket.is_some()).count();
        assert!(n >= 2);
        let m = TIMELINE.iter().find(|m| m.day == 11 && m.month == 12).unwrap();
        assert!(m.pocket.is_some());
        assert!(pocket_xz(m).unwrap().len() >= 4);
        let hungnam_t = TIMELINE
            .iter()
            .position(|mark| mark.day == 11 && mark.month == 12)
            .unwrap() as f32;
        assert!(preview_pocket_xz(hungnam_t).is_some());
        assert!(m.title.contains("Hungnam"));
    }

    #[test]
    fn editor_map_winter_for_december() {
        let m = TIMELINE.iter().find(|m| m.month == 12 && m.year == 1950).unwrap();
        assert_eq!(m.editor_map(), "Winter");
        assert!(m.editor_hint().contains("Winter 1950"));
    }
}
