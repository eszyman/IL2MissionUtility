//! payloads.rs — aircraft payload and modification tables
//!
//! Parsed once from `assets/Payloads.txt` (`OnceLock`). A UI overlay
//! keyed by script filename, not Group AST parsing — the seat only
//! stores `payload_id` / `mod_mask`; this module owns the catalog and
//! the ModMask bit helpers. Updating the text file and rebuilding picks
//! up new aircraft. It does not write Plane properties (`template` does).
//!
//! Aircraft `ModMask` keeps bit 0 set (`1`, `11`, `10101`, …). Train
//! and vehicle masks start at `0` (stock / None) and must not force that bit.
//!
//! ## Public API
//! * `fn catalog` / `struct PayloadCatalog` (`for_script`, munition lookup)
//! * `PayloadOption` / `ModSlot` / `ModOption` / `Munition` / `AircraftLoadout`
//! * `fn payload_preview` / `fn mods_preview` — one-line UI text
//! * `fn parse_mod_mask` / `fn encode_mod_mask` / `fn parse_mod_mask_for`
//! * `fn select_exclusive` / `fn set_toggle` / `fn option_selected`
//! * `fn is_train_script` / `fn is_vehicle_script` / `fn empty_mod_mask`
//! * `fn train_carriage_slot` / `fn mod_slot_title` / `fn slot_choice_count`
//!
//! ## Used by
//! * ui.rs (Template) — payload combo, mod checkboxes, preview lines

use std::sync::OnceLock;

use crate::model_spec::script_id;

const SOURCE: &str = include_str!("../assets/Payloads.txt");

static CATALOG: OnceLock<PayloadCatalog> = OnceLock::new();

pub fn catalog() -> &'static PayloadCatalog {
    CATALOG.get_or_init(|| parse_payloads(SOURCE))
}

#[derive(Clone, Debug, Default)]
pub struct PayloadCatalog {
    pub munitions: Vec<Munition>,
    pub aircraft: Vec<AircraftLoadout>,
}

#[derive(Clone, Debug)]
pub struct Munition {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub mass: String,
}

#[derive(Clone, Debug, Default)]
pub struct AircraftLoadout {
    pub name: String,
    pub payloads: Vec<PayloadOption>,
    pub mod_slots: Vec<ModSlot>,
}

#[derive(Clone, Debug)]
pub struct PayloadOption {
    pub id: i32,
    pub columns: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct ModSlot {
    pub number: u32,
    pub options: Vec<ModOption>,
}

#[derive(Clone, Debug)]
pub struct ModOption {
    pub binary_id: String,
    pub description: String,
}

impl PayloadCatalog {
    pub fn for_script(&self, script: &str) -> Option<&AircraftLoadout> {
        if is_train_script(script) {
            return self
                .aircraft
                .iter()
                .find(|ac| normalize_key(&ac.name) == "trains");
        }
        let id = script_id(script);
        self.aircraft
            .iter()
            .filter_map(|ac| match_score(&ac.name, &id).map(|score| (score, ac)))
            .max_by_key(|(score, ac)| (*score, normalize_key(&ac.name).len()))
            .map(|(_, ac)| ac)
    }

    pub fn munition_for_cell(&self, cell: &str) -> Option<&Munition> {
        let stripped = strip_quantity(cell);
        if stripped.is_empty() || is_blank_ordnance(stripped) {
            return None;
        }
        let needle = normalize_key(stripped);
        if needle.is_empty() {
            return None;
        }
        self.munitions
            .iter()
            .filter(|m| {
                let n = normalize_key(&m.name);
                !n.is_empty() && (needle == n || needle.starts_with(&n) || n.starts_with(&needle))
            })
            .max_by_key(|m| normalize_key(&m.name).len())
    }
}

impl PayloadOption {
    pub fn summary(&self) -> String {
        let parts: Vec<&str> = self
            .columns
            .iter()
            .map(|(_, v)| v.as_str())
            .filter(|v| !is_blank_ordnance(v) && !is_none_word(v))
            .collect();
        if parts.is_empty() {
            "Clean".into()
        } else {
            parts.join(" · ")
        }
    }

    pub fn description(&self, cat: &PayloadCatalog) -> String {
        let mut lines = Vec::new();
        for (header, value) in &self.columns {
            if is_blank_ordnance(value) {
                continue;
            }
            if is_none_word(value) {
                continue;
            }
            if let Some(mun) = cat.munition_for_cell(value) {
                let mut line = format!("{value} — {}", mun.description);
                if !mun.kind.is_empty() {
                    line.push_str(" (");
                    line.push_str(&mun.kind);
                    line.push(')');
                }
                if !mun.mass.is_empty() {
                    line.push_str(". ");
                    line.push_str(&mun.mass);
                }
                lines.push(line);
            } else {
                lines.push(format!("{header}: {value}"));
            }
        }
        if lines.is_empty() {
            "Clean — no external stores.".into()
        } else {
            lines.join("\n")
        }
    }
}

impl AircraftLoadout {
    pub fn payload(&self, id: i32) -> Option<&PayloadOption> {
        self.payloads.iter().find(|p| p.id == id)
    }

    pub fn has_payloads(&self) -> bool {
        !self.payloads.is_empty()
    }

    pub fn has_mods(&self) -> bool {
        self.mod_slots.iter().any(|s| !s.options.is_empty())
    }
}

/// True when this loadout carries a bomb. Rockets, fuel, and a clean jet are not bombs.
pub fn payload_has_bombs(script: &str, payload_id: i32) -> bool {
    let cat = catalog();
    let Some(ac) = cat.for_script(script) else {
        return false;
    };
    let Some(payload) = ac.payload(payload_id) else {
        return false;
    };
    payload
        .columns
        .iter()
        .any(|(_, value)| cell_ordnance(cat, value) == Some("bomb"))
}

/// First payload that carries a bomb, or a rocket when the aircraft has no bombs.
/// Fuel tanks, flares, and a clean jet are skipped.
pub fn strike_payload_id(script: &str) -> Option<i32> {
    let cat = catalog();
    let ac = cat.for_script(script)?;
    let class_of = |p: &PayloadOption| -> Option<&'static str> {
        p.columns
            .iter()
            .find_map(|(_, value)| cell_ordnance(cat, value))
    };
    ac.payloads
        .iter()
        .find(|p| class_of(p) == Some("bomb"))
        .or_else(|| ac.payloads.iter().find(|p| class_of(p) == Some("rocket")))
        .map(|p| p.id)
}

fn cell_ordnance(cat: &PayloadCatalog, value: &str) -> Option<&'static str> {
    if is_blank_ordnance(value) || is_none_word(value) {
        return None;
    }
    if let Some(mun) = cat.munition_for_cell(value) {
        return ordnance_class(&mun.kind);
    }
    let s = value.to_ascii_lowercase();
    if s.contains("bomb") || s.contains("fab-") || s.contains("napalm") {
        Some("bomb")
    } else if s.contains("rocket") || s.contains("hvar") || s.contains("rs-") {
        Some("rocket")
    } else {
        None
    }
}

fn ordnance_class(kind: &str) -> Option<&'static str> {
    let k = kind.to_ascii_lowercase();
    if k.contains("fuel") || k.contains("illumination") || k.contains("flare") {
        return None;
    }
    if k.contains("bomb") {
        Some("bomb")
    } else if k.contains("rocket") {
        Some("rocket")
    } else {
        None
    }
}

pub fn payload_preview(script: &str, payload_id: i32) -> String {
    match catalog().for_script(script) {
        Some(ac) if ac.has_payloads() => ac
            .payload(payload_id)
            .map(|p| p.summary())
            .unwrap_or_else(|| format!("ID {payload_id}")),
        _ => {
            if payload_id == 0 {
                "—".into()
            } else {
                format!("ID {payload_id}")
            }
        }
    }
}

pub fn is_train_script(script: &str) -> bool {
    script_path_contains(script, "\\trains\\")
}

pub fn is_vehicle_script(script: &str) -> bool {
    script_path_contains(script, "\\vehicles\\")
}

fn script_path_contains(script: &str, needle: &str) -> bool {
    script
        .replace('/', "\\")
        .to_ascii_lowercase()
        .contains(needle)
}

fn loadout_is_zero_base(ac: &AircraftLoadout) -> bool {
    ac.mod_slots.iter().any(|s| {
        s.options.iter().any(|o| {
            o.binary_id.trim() == "0"
                || o.description.eq_ignore_ascii_case("none") && extra_bits(&o.binary_id) == 0
        })
    })
}

/// Aircraft empty mask is `1` (bit 0). Trains and vehicles empty/stock is `0`.
pub fn empty_mod_mask(script: &str) -> u64 {
    if is_train_script(script) || is_vehicle_script(script) {
        return 0;
    }
    if catalog()
        .for_script(script)
        .is_some_and(loadout_is_zero_base)
    {
        0
    } else {
        1
    }
}

pub fn default_mod_mask_str(script: &str) -> String {
    encode_mod_mask(empty_mod_mask(script))
}

/// Carriage script → train ModMask slot from `Payloads.txt` (Hospital,
/// boxcars, wagons, platforms). Other cars have no style bits.
pub fn train_carriage_slot(script: &str) -> Option<u32> {
    match script_id(script).as_str() {
        "carpassenger" => Some(1),
        "carbox" => Some(2),
        "cargondola" => Some(3),
        "carplatform" => Some(4),
        _ => None,
    }
}

pub fn slot_has_choices(slot: &ModSlot) -> bool {
    slot.options.iter().any(|o| extra_bits(&o.binary_id) != 0)
}

/// Train ModMask slot titles from `Payloads.txt`.
pub fn train_slot_title(number: u32) -> Option<&'static str> {
    match number {
        1 => Some("Hospital"),
        2 => Some("Boxcars"),
        3 => Some("Wagons"),
        4 => Some("Platforms"),
        _ => None,
    }
}

fn vehicle_slot_title(number: u32) -> Option<&'static str> {
    match number {
        1 => Some("Equipment"),
        2 => Some("Cargo"),
        3 => Some("Trailer canvas"),
        4 => Some("Trailer"),
        _ => None,
    }
}

pub fn mod_slot_title(script: &str, number: u32) -> String {
    let named = if is_train_script(script) {
        train_slot_title(number)
    } else if is_vehicle_script(script) {
        vehicle_slot_title(number)
    } else {
        None
    };
    named
        .map(|t| t.to_string())
        .unwrap_or_else(|| format!("Slot {number}"))
}

pub fn slot_choice_count(slot: &ModSlot) -> usize {
    slot.options
        .iter()
        .filter(|o| extra_bits(&o.binary_id) != 0)
        .count()
}

pub fn mods_preview(script: &str, mod_mask: &str) -> String {
    let Some(ac) = catalog().for_script(script) else {
        return if is_default_mask_for(script, mod_mask) {
            "—".into()
        } else {
            mod_mask.to_string()
        };
    };
    if !ac.has_mods() {
        return "—".into();
    }
    let bits = parse_mod_mask_for(script, mod_mask);
    let mut labels = Vec::new();
    for slot in &ac.mod_slots {
        for opt in &slot.options {
            let extra = extra_bits(&opt.binary_id);
            if extra == 0 {
                continue;
            }
            if bits & extra == extra {
                labels.push(opt.description.as_str());
            }
        }
    }
    if labels.is_empty() {
        "—".into()
    } else {
        labels.join(", ")
    }
}

pub fn parse_mod_mask(s: &str) -> u64 {
    parse_mod_mask_or(s, 1)
}

pub fn parse_mod_mask_for(script: &str, s: &str) -> u64 {
    parse_mod_mask_or(s, empty_mod_mask(script))
}

fn parse_mod_mask_or(s: &str, empty: u64) -> u64 {
    let s = s.trim().trim_matches('"');
    if s.is_empty() {
        return empty;
    }
    u64::from_str_radix(s, 2).unwrap_or(empty)
}

pub fn encode_mod_mask(bits: u64) -> String {
    format!("{bits:b}")
}

pub fn extra_bits(binary_id: &str) -> u64 {
    parse_mod_mask(binary_id) & !1
}

pub fn slot_bits(slot: &ModSlot) -> u64 {
    slot.options
        .iter()
        .fold(0u64, |acc, o| acc | extra_bits(&o.binary_id))
}

/// Select `option` in an exclusive slot, clearing the other options in that slot.
pub fn select_exclusive(mask: u64, slot: &ModSlot, option: &ModOption) -> u64 {
    select_exclusive_empty(mask, slot, option, 1)
}

pub fn select_exclusive_for(script: &str, mask: u64, slot: &ModSlot, option: &ModOption) -> u64 {
    select_exclusive_empty(mask, slot, option, empty_mod_mask(script))
}

fn select_exclusive_empty(mask: u64, slot: &ModSlot, option: &ModOption, empty: u64) -> u64 {
    (mask & !slot_bits(slot)) | extra_bits(&option.binary_id) | empty
}

pub fn clear_exclusive_for(script: &str, mask: u64, slot: &ModSlot) -> u64 {
    (mask & !slot_bits(slot)) | empty_mod_mask(script)
}

pub fn set_toggle(mask: u64, option: &ModOption, on: bool) -> u64 {
    set_toggle_empty(mask, option, on, 1)
}

pub fn set_toggle_for(script: &str, mask: u64, option: &ModOption, on: bool) -> u64 {
    set_toggle_empty(mask, option, on, empty_mod_mask(script))
}

fn set_toggle_empty(mask: u64, option: &ModOption, on: bool, empty: u64) -> u64 {
    let extra = extra_bits(&option.binary_id);
    if extra == 0 {
        return empty;
    }
    if on {
        mask | extra | empty
    } else {
        (mask & !extra) | empty
    }
}

pub fn option_selected(mask: u64, option: &ModOption) -> bool {
    let extra = extra_bits(&option.binary_id);
    if extra == 0 {
        false
    } else {
        mask & extra == extra
    }
}

/// Exclusive slot: which option is active. Empty (binary `1` for aircraft,
/// `0` for trains / vehicles) when no extra bits are set. Does not pick
/// the first option as a stand-in for “none”.
pub fn exclusive_selection<'a>(mask: u64, slot: &'a ModSlot) -> Option<&'a ModOption> {
    let current = mask & slot_bits(slot);
    if current == 0 {
        return slot.options.iter().find(|o| extra_bits(&o.binary_id) == 0);
    }
    slot.options
        .iter()
        .find(|o| extra_bits(&o.binary_id) == current)
        .or_else(|| {
            slot.options
                .iter()
                .filter(|o| {
                    let e = extra_bits(&o.binary_id);
                    e != 0 && current & e == e
                })
                .max_by_key(|o| extra_bits(&o.binary_id).count_ones())
        })
}

fn is_default_mask_for(script: &str, s: &str) -> bool {
    let t = s.trim();
    if empty_mod_mask(script) == 0 {
        t.is_empty() || t == "0"
    } else {
        t.is_empty() || t == "1"
    }
}

fn is_blank_ordnance(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == "—" || t == "-" || t == "–"
}

fn is_none_word(s: &str) -> bool {
    let t = s.trim();
    t.eq_ignore_ascii_case("none") || t.eq_ignore_ascii_case("none (clean)")
}

fn strip_quantity(s: &str) -> &str {
    let s = s.trim();
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return s;
    }
    let rest = s[digits..].trim_start();
    let mut chars = rest.chars();
    match chars.next() {
        Some('×' | 'x' | 'X') => chars.as_str().trim_start(),
        _ => s,
    }
}

fn normalize_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn looks_like_mod_row(fields: &[String]) -> bool {
    if fields.len() < 2 {
        return false;
    }
    if fields[0].parse::<u32>().is_err() {
        return false;
    }
    let binary = fields[1].as_str();
    !binary.is_empty() && binary.chars().all(|c| c == '0' || c == '1')
}

/// Higher is a better header→script match. Exact beats prefix; prefix of a
/// longer script (GMC-CCKW vs gmc-cckw-refueler) loses to a token match
/// that covers more of the id (GMC-Refueler).
fn match_score(header: &str, script_id: &str) -> Option<u32> {
    let h = normalize_key(header);
    if h.is_empty() {
        return None;
    }
    let covered = if script_id == h {
        h.len()
    } else if script_id.starts_with(&h) {
        // F-51 → f51d (short series suffix). Reject vehicle variants
        // (studebakerus6 → studebakerus6-bm13 / refueler / tanker).
        let leftover = script_id.len() - h.len();
        if leftover <= 2 {
            h.len()
        } else {
            header_token_coverage(header, script_id)?
        }
    } else if script_id.contains(&h) || h.contains(script_id) {
        h.len().min(script_id.len())
    } else {
        header_token_coverage(header, script_id)?
    };
    let denom = script_id.len().max(1);
    Some((covered * 1000 / denom * 100 + covered) as u32)
}

fn header_token_coverage(header: &str, script_id: &str) -> Option<usize> {
    let tokens: Vec<String> = header
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(normalize_key)
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() >= 2 && tokens.iter().all(|t| script_id.contains(t)) {
        Some(tokens.iter().map(|t| t.len()).sum())
    } else {
        None
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                fields.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

fn is_title_line(line: &str) -> bool {
    let fields = parse_csv_line(line);
    fields.len() <= 1 && !line.trim().is_empty()
}

fn parse_payloads(src: &str) -> PayloadCatalog {
    let mut cat = PayloadCatalog::default();
    let mut pending_title: Option<String> = None;
    let mut mode = Mode::None;
    let mut payload_headers: Vec<String> = Vec::new();

    for raw in src.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if is_title_line(line) {
            pending_title = Some(line.to_string());
            mode = Mode::None;
            continue;
        }
        let fields = parse_csv_line(line);
        let head = fields.first().map(|s| s.as_str()).unwrap_or("");
        if head.eq_ignore_ascii_case("Munition") {
            mode = Mode::Munitions;
            pending_title = None;
            continue;
        }
        if head.eq_ignore_ascii_case("Payload ID") {
            let name = pending_title.take().unwrap_or_default();
            if name.is_empty() || is_section_banner(&name) {
                mode = Mode::None;
                continue;
            }
            payload_headers = fields.iter().skip(1).cloned().collect();
            let idx = ensure_aircraft(&mut cat, &name);
            mode = Mode::Payloads(idx);
            continue;
        }
        if head.eq_ignore_ascii_case("Modification") {
            let name = pending_title.take().unwrap_or_default();
            if name.is_empty() || is_section_banner(&name) {
                mode = Mode::None;
                continue;
            }
            let idx = ensure_aircraft(&mut cat, &name);
            mode = Mode::Mods(idx);
            continue;
        }
        if matches!(mode, Mode::None) {
            if let Some(name) = pending_title.as_deref() {
                if !name.is_empty() && !is_section_banner(name) && looks_like_mod_row(&fields) {
                    let name = pending_title.take().unwrap();
                    let idx = ensure_aircraft(&mut cat, &name);
                    mode = Mode::Mods(idx);
                }
            }
        }
        match mode {
            Mode::Munitions => {
                if fields.len() >= 2 && !fields[0].is_empty() {
                    cat.munitions.push(Munition {
                        name: fields[0].clone(),
                        kind: fields.get(1).cloned().unwrap_or_default(),
                        description: fields.get(2).cloned().unwrap_or_default(),
                        mass: fields.get(3).cloned().unwrap_or_default(),
                    });
                }
            }
            Mode::Payloads(idx) => {
                if let Some(id) = fields.first().and_then(|s| s.parse::<i32>().ok()) {
                    let mut columns = Vec::new();
                    for (i, header) in payload_headers.iter().enumerate() {
                        let value = fields.get(i + 1).cloned().unwrap_or_default();
                        columns.push((header.clone(), value));
                    }
                    cat.aircraft[idx]
                        .payloads
                        .push(PayloadOption { id, columns });
                }
            }
            Mode::Mods(idx) => {
                if let (Some(slot_n), Some(binary)) = (fields.first(), fields.get(1)) {
                    if let Ok(number) = slot_n.parse::<u32>() {
                        let description = fields.get(2).cloned().unwrap_or_default();
                        let ac = &mut cat.aircraft[idx];
                        if let Some(slot) = ac.mod_slots.iter_mut().find(|s| s.number == number) {
                            slot.options.push(ModOption {
                                binary_id: binary.clone(),
                                description,
                            });
                        } else {
                            ac.mod_slots.push(ModSlot {
                                number,
                                options: vec![ModOption {
                                    binary_id: binary.clone(),
                                    description,
                                }],
                            });
                        }
                    }
                }
            }
            Mode::None => {}
        }
    }
    coalesce_related_aircraft(&mut cat);
    cat
}

fn is_section_banner(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("payload") && !n.contains("id") || n == "soviet payloads"
}

/// `F-51` payload table and `F-51D` mod table are the same aircraft.
fn related_aircraft_names(a: &str, b: &str) -> bool {
    let ka = normalize_key(a);
    let kb = normalize_key(b);
    if ka.is_empty() || kb.is_empty() {
        return false;
    }
    ka == kb || ka.starts_with(&kb) || kb.starts_with(&ka)
}

fn merge_loadout(into: &mut AircraftLoadout, from: AircraftLoadout) {
    if normalize_key(&from.name).len() > normalize_key(&into.name).len() {
        into.name = from.name;
    }
    for p in from.payloads {
        if !into.payloads.iter().any(|e| e.id == p.id) {
            into.payloads.push(p);
        }
    }
    for slot in from.mod_slots {
        if let Some(existing) = into.mod_slots.iter_mut().find(|s| s.number == slot.number) {
            for opt in slot.options {
                if !existing
                    .options
                    .iter()
                    .any(|o| o.binary_id == opt.binary_id)
                {
                    existing.options.push(opt);
                }
            }
        } else {
            into.mod_slots.push(slot);
        }
    }
}

fn coalesce_related_aircraft(cat: &mut PayloadCatalog) {
    let mut i = 0;
    while i < cat.aircraft.len() {
        let mut j = i + 1;
        while j < cat.aircraft.len() {
            if related_aircraft_names(&cat.aircraft[i].name, &cat.aircraft[j].name) {
                let other = cat.aircraft.remove(j);
                merge_loadout(&mut cat.aircraft[i], other);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

fn ensure_aircraft(cat: &mut PayloadCatalog, name: &str) -> usize {
    if let Some(i) = cat
        .aircraft
        .iter()
        .position(|a| related_aircraft_names(&a.name, name))
    {
        if normalize_key(name).len() > normalize_key(&cat.aircraft[i].name).len() {
            cat.aircraft[i].name = name.to_string();
        }
        i
    } else {
        cat.aircraft.push(AircraftLoadout {
            name: name.to_string(),
            payloads: Vec::new(),
            mod_slots: Vec::new(),
        });
        cat.aircraft.len() - 1
    }
}

#[derive(Clone, Copy)]
enum Mode {
    None,
    Munitions,
    Payloads(usize),
    Mods(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_western_and_soviet_payloads() {
        let cat = catalog();
        let f80 = cat.for_script("f80c10.txt").expect("F-80C-10");
        assert!(f80.payloads.len() >= 73);
        assert!(
            f80.payload(72)
                .unwrap()
                .summary()
                .contains("AN-M26A1"),
            "payload 72 is the tip-tank flare load"
        );
        let p5 = f80.payload(5).expect("payload 5");
        let summary = p5.summary();
        assert!(summary.contains("Drop Tank 165"), "{summary}");
        assert!(summary.contains("AN-M64A1"), "{summary}");

        let mig = cat
            .for_script(r"LuaScripts\WorldObjects\Planes\mig15bis.txt")
            .unwrap();
        assert_eq!(mig.payload(0).unwrap().summary(), "Clean");
        assert!(mig.payload(1).unwrap().summary().contains("250"));

        let f51 = cat.for_script("f51d.txt").expect("F-51 matches F-51D");
        assert!(f51.payloads.len() > 20);
        assert!(f51.has_mods(), "F-51D mods merge with F-51 payloads");
        assert!(f51.mod_slots.len() >= 5);
        assert!(f51.mod_slots.iter().any(|s| {
            s.options
                .iter()
                .any(|o| o.description.contains("Gunsite") || o.description.contains("Gunsight"))
        }));

        let b29 = cat
            .for_script("simpleb29.txt")
            .expect("B-29 matches simpleb29");
        assert!(b29.payload(1).unwrap().summary().contains("AN-M64A1"));
    }

    #[test]
    fn unknown_aircraft_has_no_table() {
        assert!(catalog().for_script("t34-85.txt").is_none());
        assert!(catalog().for_script("la11.txt").is_none());
    }

    #[test]
    fn payload_has_bombs_reads_the_selected_loadout() {
        let script = r"LuaScripts\WorldObjects\Planes\f51d.txt";
        let bomb = strike_payload_id(script).expect("F-51D has a bomb loadout");
        assert!(payload_has_bombs(script, bomb));
        let clean = catalog()
            .for_script(script)
            .unwrap()
            .payloads
            .iter()
            .find(|p| !payload_has_bombs(script, p.id))
            .expect("F-51D has a loadout with no bombs");
        assert!(!payload_has_bombs(script, clean.id));
        assert!(!payload_has_bombs("no-such-plane", 0));
    }

    #[test]
    fn strike_payload_prefers_a_bomb() {
        let script = r"LuaScripts\WorldObjects\Planes\f51d.txt";
        let id = strike_payload_id(script).expect("F-51D has a bomb loadout");
        let summary = payload_preview(script, id);
        assert!(
            summary.to_ascii_lowercase().contains("bomb")
                || summary.contains("AN-M")
                || summary.contains("500"),
            "{summary}"
        );
        assert!(strike_payload_id("no-such-plane").is_none());
    }

    #[test]
    fn munition_description_from_payload_cell() {
        let cat = catalog();
        let mun = cat
            .munition_for_cell("2× AN-M64A1 500 lb HE")
            .expect("500 lb");
        assert!(mun.description.to_ascii_lowercase().contains("500"));
        let hvar = cat.munition_for_cell("4× HVAR 5\"").expect("HVAR");
        assert!(hvar.kind.to_ascii_lowercase().contains("rocket"));
        let fab = cat.munition_for_cell("2× FAB-100sc").expect("FAB-100");
        assert!(!fab.description.is_empty());
    }

    #[test]
    fn csv_quoted_comma_and_inch_mark() {
        let fields = parse_csv_line(
            r#""HVAR 5""",HE Rocket,"High Velocity Aircraft Rocket, High Explosive",mass"#,
        );
        assert_eq!(fields[0], r#"HVAR 5""#);
        assert_eq!(fields[1], "HE Rocket");
        assert!(fields[2].contains("High Explosive"));
    }

    #[test]
    fn mod_mask_binary_logic() {
        assert_eq!(encode_mod_mask(1), "1");
        assert_eq!(encode_mod_mask(0b11), "11");
        assert_eq!(encode_mod_mask(0b101), "101");
        assert_eq!(encode_mod_mask(0b1001), "1001");
        assert_eq!(encode_mod_mask(0b10001), "10001");
        let two_and_four = parse_mod_mask("101") | parse_mod_mask("10001");
        assert_eq!(encode_mod_mask(two_and_four), "10101");
        assert_eq!(parse_mod_mask("1"), 1);
    }

    #[test]
    fn c47_mods_are_one_exclusive_slot() {
        let ac = catalog().for_script("c47b.txt").expect("C-47B");
        assert_eq!(ac.mod_slots.len(), 1);
        assert!(ac.payloads.is_empty());
        let slot = &ac.mod_slots[0];
        assert!(slot.options.iter().any(|o| o.description == "Cargo"));
        assert!(slot.options.iter().any(|o| o.description == "Ambulance"));
        let cargo = slot
            .options
            .iter()
            .find(|o| o.description == "Cargo")
            .unwrap();
        let mask = select_exclusive(1, slot, cargo);
        assert_eq!(encode_mod_mask(mask), "11");
        assert_eq!(
            exclusive_selection(mask, slot).map(|o| o.description.as_str()),
            Some("Cargo")
        );
        assert_eq!(mods_preview("c47b.txt", "11"), "Cargo");
        assert_eq!(mods_preview("c47b.txt", "1"), "—");
    }

    #[test]
    fn yak9p_mods_combine_across_slots() {
        let ac = catalog().for_script("yak9p.txt").expect("Yak-9P");
        assert_eq!(ac.mod_slots.len(), 4);
        let mut mask = 1u64;
        for slot in &ac.mod_slots {
            if slot.number == 2 || slot.number == 4 {
                mask = set_toggle(mask, &slot.options[0], true);
            }
        }
        assert_eq!(encode_mod_mask(mask), "10101");
        let preview = mods_preview("yak9p.txt", "10101");
        assert!(preview.contains("Gunsight"), "{preview}");
        assert!(preview.contains("Aerobatic"), "{preview}");
        assert!(!preview.contains("Horizon"), "{preview}");
    }

    #[test]
    fn li2t_and_il10_are_listed() {
        assert!(catalog().for_script("li2t.txt").unwrap().has_mods());
        assert!(catalog().for_script("il10.txt").unwrap().has_payloads());
        assert!(catalog().for_script("tu2.txt").unwrap().has_payloads());
        assert!(catalog().for_script("f84e.txt").unwrap().payloads.len() > 40);
        assert!(catalog().for_script("f86a5.txt").unwrap().payloads.len() >= 12);
    }

    #[test]
    fn f51d_has_payloads_and_modifications() {
        let ac = catalog().for_script("f51d.txt").expect("F-51D");
        assert!(ac.has_payloads());
        assert!(ac.has_mods());
        assert!(ac.payload(11).unwrap().summary().contains("HVAR"));
        let preview = mods_preview("f51d.txt", "10101");
        assert!(preview.contains("Radar"), "{preview}");
        assert!(
            preview.contains("Fuel") || preview.contains("150"),
            "{preview}"
        );
        assert_eq!(payload_preview("f51d.txt", 0), "Clean");
    }

    #[test]
    fn both_train_types_share_the_trains_mod_table() {
        let east = catalog()
            .for_script(r"LuaScripts\WorldObjects\Trains\type475-1.txt")
            .expect("type475-1");
        let west = catalog()
            .for_script(r"LuaScripts\WorldObjects\Trains\usatc-s160.txt")
            .expect("usatc-s160");
        assert!(east.has_mods());
        assert_eq!(east.mod_slots.len(), west.mod_slots.len());
        let platforms = east
            .mod_slots
            .iter()
            .find(|s| s.number == 4)
            .expect("platform slot");
        assert!(
            platforms
                .options
                .iter()
                .any(|o| o.description.contains("T-34-85"))
        );
        assert_eq!(train_carriage_slot("carbox.txt"), Some(2));
        assert_eq!(train_carriage_slot("carplatform.txt"), Some(4));
        assert_eq!(train_carriage_slot("type475-1-tender.txt"), None);
        assert_eq!(train_slot_title(2), Some("Boxcars"));
        assert_eq!(train_slot_title(4), Some("Platforms"));
        assert!(slot_choice_count(platforms) > 1);
    }

    #[test]
    fn train_mod_mask_stays_zero_until_a_carriage_style_is_chosen() {
        let script = r"LuaScripts\WorldObjects\Trains\type475-1.txt";
        assert_eq!(empty_mod_mask(script), 0);
        assert_eq!(default_mod_mask_str(script), "0");
        assert_eq!(parse_mod_mask_for(script, ""), 0);
        assert_eq!(parse_mod_mask_for(script, "0"), 0);
        assert_eq!(mods_preview(script, "0"), "—");
        let ac = catalog().for_script(script).unwrap();
        let boxcars = ac.mod_slots.iter().find(|s| s.number == 2).unwrap();
        let heated = boxcars
            .options
            .iter()
            .find(|o| o.description.contains("Heated"))
            .unwrap();
        let mask = select_exclusive_for(script, 0, boxcars, heated);
        assert_eq!(encode_mod_mask(mask), "100");
        assert_eq!(mask & 1, 0, "train masks must not force aircraft bit 0");
        let cleared = clear_exclusive_for(script, mask, boxcars);
        assert_eq!(cleared, 0);
        let hospital = ac
            .mod_slots
            .iter()
            .find(|s| s.number == 1)
            .unwrap()
            .options
            .first()
            .unwrap();
        let on = set_toggle_for(script, 0, hospital, true);
        assert_eq!(encode_mod_mask(on), "10");
        assert_eq!(set_toggle_for(script, on, hospital, false), 0);
    }

    #[test]
    fn vehicle_mod_tables_default_to_zero_and_match_scripts() {
        let gaz = catalog()
            .for_script(r"LuaScripts\WorldObjects\vehicles\gaz63.txt")
            .expect("GAZ-63");
        assert!(gaz.has_mods());
        assert!(
            gaz.mod_slots
                .iter()
                .any(|s| s.options.iter().any(|o| o.description == "Canvas top"))
        );
        assert!(gaz.mod_slots.iter().any(|s| {
            s.options
                .iter()
                .any(|o| o.description.contains("Generator"))
        }));
        let script = r"LuaScripts\WorldObjects\vehicles\gaz63.txt";
        assert_eq!(empty_mod_mask(script), 0);
        assert_eq!(default_mod_mask_str(script), "0");
        assert_eq!(mods_preview(script, "0"), "—");
        let cargo = gaz.mod_slots.iter().find(|s| s.number == 2).unwrap();
        let soldiers = cargo
            .options
            .iter()
            .find(|o| o.description == "Soldiers")
            .unwrap();
        let mask = select_exclusive_for(script, 0, cargo, soldiers);
        assert_eq!(encode_mod_mask(mask), "100");
        assert_eq!(mask & 1, 0, "vehicle masks must not force aircraft bit 0");

        let truck = catalog()
            .for_script(r"LuaScripts\WorldObjects\vehicles\gmc-cckw.txt")
            .expect("GMC-CCKW");
        let tanker = catalog()
            .for_script(r"LuaScripts\WorldObjects\vehicles\gmc-cckw-refueler.txt")
            .expect("GMC-Refueler");
        assert_ne!(truck.name, tanker.name);
        assert!(
            truck
                .mod_slots
                .iter()
                .any(|s| s.options.iter().any(|o| o.description == "Canvas top"))
        );
        assert!(
            !tanker
                .mod_slots
                .iter()
                .any(|s| s.options.iter().any(|o| o.description == "Canvas top"))
        );

        let jeep = catalog().for_script("willysmb.txt").expect("Willys MB");
        assert!(jeep.has_mods());
        let dodge = catalog().for_script("dodgewc52.txt").expect("Dodge WC52");
        assert!(dodge.mod_slots.iter().any(|s| {
            s.options
                .iter()
                .any(|o| o.description.contains("Canvas Frame"))
        }));
        assert!(catalog().for_script("u7144.txt").unwrap().has_mods());
        assert!(catalog().for_script("ba64b.txt").unwrap().has_mods());
        assert!(
            catalog()
                .for_script("m3a1-halftrack.txt")
                .unwrap()
                .has_mods()
        );
        let stude = catalog()
            .for_script(r"LuaScripts\WorldObjects\vehicles\studebakerus6.txt")
            .expect("StudebakerUS6");
        assert!(stude.has_mods());
        assert!(
            stude
                .mod_slots
                .iter()
                .any(|s| { s.options.iter().any(|o| o.description.contains("ZPU")) })
        );
        assert!(
            stude
                .mod_slots
                .iter()
                .any(|s| { s.options.iter().any(|o| o.description.contains("P20")) })
        );
        assert!(
            catalog()
                .for_script(r"LuaScripts\WorldObjects\vehicles\studebakerus6-bm13.txt")
                .is_none(),
            "BM-13 must not inherit cargo-truck mods"
        );
        assert!(
            catalog()
                .for_script(r"LuaScripts\WorldObjects\vehicles\studebakerus6-refueler.txt")
                .is_none()
        );
        assert_eq!(
            empty_mod_mask(r"LuaScripts\WorldObjects\vehicles\studebakerus6.txt"),
            0
        );
        assert_eq!(mod_slot_title(script, 2), "Cargo");
        assert_eq!(empty_mod_mask("gaz63.txt"), 0);
        assert_eq!(empty_mod_mask("f51d.txt"), 1);
    }
}
