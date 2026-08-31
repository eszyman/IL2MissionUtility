//! Aircraft types, countries, 1950s-style numbers / tail codes / callsigns.

pub struct AircraftType {
    pub id: &'static str,
    pub label: &'static str,
    pub model: &'static str,
    pub script: &'static str,
}

pub const AIRCRAFT_TYPES: &[AircraftType] = &[
    AircraftType {
        id: "mig15bis",
        label: "MiG-15bis",
        model: r"graphics\planes\mig15bis\mig15bis.mgm",
        script: r"LuaScripts\WorldObjects\Planes\mig15bis.txt",
    },
    AircraftType {
        id: "la11",
        label: "La-11",
        model: r"graphics\planes\la11\la11.mgm",
        script: r"LuaScripts\WorldObjects\Planes\la11.txt",
    },
    AircraftType {
        id: "yak9p",
        label: "Yak-9P",
        model: r"graphics\planes\yak9p\yak9p.mgm",
        script: r"LuaScripts\WorldObjects\Planes\yak9p.txt",
    },
    AircraftType {
        id: "f86a5",
        label: "F-86A-5",
        model: r"graphics\planes\f86a5\f86a5.mgm",
        script: r"LuaScripts\WorldObjects\Planes\f86a5.txt",
    },
    AircraftType {
        id: "f84e",
        label: "F-84E",
        model: r"graphics\planes\f84e\f84e.mgm",
        script: r"LuaScripts\WorldObjects\Planes\f84e.txt",
    },
    AircraftType {
        id: "f80c10",
        label: "F-80C-10",
        model: r"graphics\planes\f80c10\f80c10.mgm",
        script: r"LuaScripts\WorldObjects\Planes\f80c10.txt",
    },
    AircraftType {
        id: "f51d",
        label: "F-51D",
        model: r"graphics\planes\f51d\f51d.mgm",
        script: r"LuaScripts\WorldObjects\Planes\f51d.txt",
    },
];

pub const COUNTRIES: &[(i32, &str)] = &[
    (501, "501  USSR"),
    (502, "502  DPRK"),
    (503, "503  PRC"),
    (601, "601  USA"),
];

const FLIGHT_COLORS: &[&str] = &["Red", "Blue", "Yellow", "Green", "White", "Black"];

/// IL-2 TCode digit glyphs (from the fighter templates: 119 → `%20%22%22%2a`).
const TCODE_DIGIT: [&str; 10] = [
    "%21", "%22", "%23", "%24", "%25", "%26", "%27", "%28", "%29", "%2a",
];

/// Color index per TCode glyph. Template uses 1=white, 2=red, 4=yellow.
fn tcode_color_digit(color: &str) -> char {
    match color {
        "White" => '1',
        "Red" => '2',
        "Blue" => '3',
        "Yellow" => '4',
        "Green" => '5',
        "Black" => '0',
        _ => '1',
    }
}

/// 1950s four-ship numbering: flight 1 is 11–14, flight 2 is 21–24, …
pub fn flight_number(flight: usize, seat: usize) -> u32 {
    ((flight % 9) as u32 + 1) * 10 + (seat as u32 + 1)
}

pub fn flight_color(flight: usize) -> &'static str {
    FLIGHT_COLORS[flight % FLIGHT_COLORS.len()]
}

pub fn encode_tcode(number: u32) -> String {
    let body = number.to_string();
    let mut out = String::new();
    if body.len() < 3 {
        // 2-digit tactical numbers as in Red 12 / White 21.
        for c in body.chars() {
            out.push_str(digit_code(c));
        }
    } else {
        // 3-digit bort with a leading space, matching "honcho 119".
        out.push_str("%20");
        for c in body.chars() {
            out.push_str(digit_code(c));
        }
    }
    out
}

fn digit_code(c: char) -> &'static str {
    c.to_digit(10)
        .and_then(|d| TCODE_DIGIT.get(d as usize).copied())
        .unwrap_or("%20")
}

pub fn encode_tcode_color(color: &str, number: u32) -> String {
    let glyphs = if number >= 100 { 4 } else { number.to_string().len() };
    std::iter::repeat(tcode_color_digit(color))
        .take(glyphs)
        .collect()
}

pub fn plane_display_name(flight: usize, seat: usize) -> String {
    format!("{} {}", flight_color(flight), flight_number(flight, seat))
}

/// Radio callsign index. 12 is the MiG “Honcho” slot used in the templates.
pub fn callsign_for(country: i32, type_id: &str) -> i32 {
    match (country, type_id) {
        (501, "mig15bis") => 12,
        _ => 0,
    }
}

pub fn plane_coalitions_for_country(country: i32) -> &'static str {
    if country / 100 == 6 {
        "[1]"
    } else {
        "[2]"
    }
}

pub fn aircraft_by_id(id: &str) -> Option<&'static AircraftType> {
    AIRCRAFT_TYPES.iter().find(|a| a.id == id)
}

pub fn default_skill(id: &str) -> i32 {
    match id {
        "mig15bis" | "f86a5" => 3,
        _ => 2,
    }
}

/// Recommended skill with a small deterministic jitter (clamped 0–4).
pub fn loose_skill(recommended: i32, salt: usize) -> i32 {
    let jitter = [0, -1, 0, 1, 0, -1, 1][salt % 7];
    (recommended + jitter).clamp(0, 4)
}

/// Lead skill is never below the wingman. They may be equal.
pub fn pair_skills(recommended: i32, flight: usize, pair: usize) -> (i32, i32) {
    let lead = loose_skill(recommended, flight.wrapping_mul(8) + pair.wrapping_mul(2));
    let wing = loose_skill(recommended, flight.wrapping_mul(8) + pair.wrapping_mul(2) + 1).min(lead);
    (lead, wing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcode_matches_template_honcho_119() {
        assert_eq!(encode_tcode(119), "%20%22%22%2a");
    }

    #[test]
    fn tcode_matches_template_red_12() {
        assert_eq!(encode_tcode(12), "%22%23");
    }

    #[test]
    fn tcode_matches_template_white_21() {
        assert_eq!(encode_tcode(21), "%23%22");
    }

    #[test]
    fn four_ship_numbers() {
        assert_eq!(flight_number(0, 0), 11);
        assert_eq!(flight_number(0, 3), 14);
        assert_eq!(flight_number(1, 0), 21);
        assert_eq!(plane_display_name(0, 1), "Red 12");
    }

    #[test]
    fn lead_skill_is_never_below_wingman() {
        for rec in 0..=4 {
            for f in 0..6 {
                let (lead, wing) = pair_skills(rec, f, 0);
                assert!(lead >= wing, "rec={rec} flight={f}: lead {lead} < wing {wing}");
                assert!((0..=4).contains(&lead));
                assert!((0..=4).contains(&wing));
            }
        }
    }

    #[test]
    fn coalitions_split_east_west() {
        assert_eq!(plane_coalitions_for_country(501), "[2]");
        assert_eq!(plane_coalitions_for_country(502), "[2]");
        assert_eq!(plane_coalitions_for_country(503), "[2]");
        assert_eq!(plane_coalitions_for_country(601), "[1]");
    }
}
