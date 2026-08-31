//! IL-2 group language sidecars (`.eng`, `.chs`, …).
//!
//! MCU_Icon / MCU_TR_Subtitle store numeric LC indexes. The actual strings live
//! in a sidecar next to the `.Group`. Those files are UTF-16 LE with a BOM;
//! the mission editor will not invent them on resave, so generated groups must
//! carry the tables with them.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const NEWLINE: &str = "\r\n";
const UTF16_LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16_BE_BOM: [u8; 2] = [0xFE, 0xFF];
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub const LANG_EXTS: &[&str] = &["eng", "chs", "fra", "ger", "rus", "spa"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocaleTable {
    entries: BTreeMap<i32, String>,
}

impl LocaleTable {
    pub fn get(&self, id: i32) -> Option<&str> {
        self.entries.get(&id).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn max_id(&self) -> Option<i32> {
        self.entries.keys().next_back().copied()
    }

    /// Keep the first value when two templates share an LC index.
    pub fn merge(&mut self, other: LocaleTable) {
        for (id, text) in other.entries {
            self.entries.entry(id).or_insert(text);
        }
    }

    pub fn overlay(&mut self, other: LocaleTable) {
        for (id, text) in other.entries {
            self.entries.insert(id, text);
        }
    }

    pub fn insert(&mut self, id: i32, text: impl Into<String>) {
        self.entries.insert(id, text.into());
    }

    pub fn contains_text(&self, text: &str) -> bool {
        self.entries.values().any(|v| v == text)
    }
}

pub fn parse_locale(text: &str) -> LocaleTable {
    let mut entries = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let Some((id_str, rest)) = line.split_once(':') else {
            continue;
        };
        let Ok(id) = id_str.parse::<i32>() else {
            continue;
        };
        entries.insert(id, rest.to_string());
    }
    LocaleTable { entries }
}

pub fn serialize_locale(table: &LocaleTable) -> String {
    let mut out = String::new();
    for (id, text) in &table.entries {
        out.push_str(&id.to_string());
        out.push(':');
        out.push_str(text);
        out.push_str(NEWLINE);
    }
    out
}

pub fn decode_locale_bytes(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&UTF16_LE_BOM) {
        utf16_to_string(&bytes[2..], false)
    } else if bytes.starts_with(&UTF16_BE_BOM) {
        utf16_to_string(&bytes[2..], true)
    } else if bytes.starts_with(&UTF8_BOM) {
        String::from_utf8(bytes[3..].to_vec()).map_err(|err| err.to_string())
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|err| err.to_string())
    }
}

pub fn encode_locale_utf16le(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + text.len() * 2);
    out.extend_from_slice(&UTF16_LE_BOM);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn utf16_to_string(bytes: &[u8], big_endian: bool) -> Result<String, String> {
    let mut units = Vec::with_capacity(bytes.len() / 2);
    let mut chunks = bytes.chunks_exact(2);
    for chunk in chunks.by_ref() {
        let value = if big_endian {
            u16::from_be_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_le_bytes([chunk[0], chunk[1]])
        };
        units.push(value);
    }
    String::from_utf16(&units).map_err(|err| err.to_string())
}

fn read_locale_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    decode_locale_bytes(&bytes)
}

/// True if any language sidecar sits next to the `.Group`.
pub fn has_sidecars(group_path: &Path) -> bool {
    LANG_EXTS
        .iter()
        .any(|ext| group_path.with_extension(ext).is_file())
}

/// Load and merge sidecars for every template `.Group` path.
pub fn merge_template_sidecars(group_paths: &[PathBuf]) -> HashMap<String, LocaleTable> {
    let mut by_ext: HashMap<String, LocaleTable> = HashMap::new();
    for path in group_paths {
        for ext in LANG_EXTS {
            let sidecar = path.with_extension(ext);
            let Ok(text) = read_locale_file(&sidecar) else {
                continue;
            };
            by_ext
                .entry((*ext).to_string())
                .or_default()
                .merge(parse_locale(&text));
        }
    }
    by_ext
}

/// Write merged tables next to `dest_group` (`Foo.Group` → `Foo.eng`, …).
pub fn write_sidecars(
    dest_group: &Path,
    tables: &HashMap<String, LocaleTable>,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for ext in LANG_EXTS {
        let Some(table) = tables.get(*ext) else {
            continue;
        };
        if table.is_empty() {
            continue;
        }
        let path = dest_group.with_extension(ext);
        let bytes = encode_locale_utf16le(&serialize_locale(table));
        std::fs::write(&path, bytes).map_err(|err| {
            format!(
                "could not write {}: {err}",
                path.file_name().and_then(|n| n.to_str()).unwrap_or(ext)
            )
        })?;
        written.push((*ext).to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_units(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("TemplateExamples/GroundUnits")
            .join(name)
    }

    fn load_eng(name: &str) -> LocaleTable {
        let text = read_locale_file(&ground_units(name)).expect("read eng");
        parse_locale(&text)
    }

    #[test]
    fn parse_debug_eng_keeps_concentration_title() {
        let table = load_eng("Debug.eng");
        assert_eq!(table.get(13), Some("Enemy Concentration Noted"));
        assert_eq!(
            table.get(19),
            Some("Enemy Spotted!<br>Targets noted on flight plans.")
        );
        assert_eq!(table.get(3), Some(""));
        assert_eq!(table.get(20), Some("Zone in Triggered"));
        assert!(table.max_id().unwrap() >= 20);
    }

    #[test]
    fn debug_eng_is_utf16le() {
        let bytes = std::fs::read(ground_units("Debug.eng")).unwrap();
        assert!(bytes.starts_with(&UTF16_LE_BOM));
        let text = decode_locale_bytes(&bytes).unwrap();
        assert!(text.contains("Enemy Concentration Noted"));
    }

    #[test]
    fn round_trip_utf16_preserves_entries() {
        let table = load_eng("Debug.eng");
        let bytes = encode_locale_utf16le(&serialize_locale(&table));
        assert!(bytes.starts_with(&UTF16_LE_BOM));
        let again = parse_locale(&decode_locale_bytes(&bytes).unwrap());
        assert_eq!(table, again);
        assert_eq!(again.get(13), Some("Enemy Concentration Noted"));
    }

    #[test]
    fn merge_keeps_both_id_ranges() {
        let mut table = load_eng("Debug.eng");
        table.merge(load_eng("DropIns/DPRK Truck Run.eng"));
        assert_eq!(table.get(13), Some("Enemy Concentration Noted"));
        assert_eq!(table.get(533), Some("Enemy Column Noted"));
    }

    #[test]
    fn merge_first_wins_on_collision() {
        let mut table = parse_locale("13:First\n");
        table.merge(parse_locale("13:Second\n"));
        assert_eq!(table.get(13), Some("First"));
    }

    #[test]
    fn loads_debug_sidecars_from_disk() {
        let tables = merge_template_sidecars(&[ground_units("Debug.Group")]);
        let eng = tables.get("eng").expect("Debug.eng");
        assert_eq!(eng.get(13), Some("Enemy Concentration Noted"));
        assert!(tables.contains_key("chs"));
        assert!(tables.contains_key("rus"));
        assert_eq!(tables.len(), LANG_EXTS.len());
    }

    #[test]
    fn detects_whether_sidecars_exist() {
        assert!(has_sidecars(&ground_units("Debug.Group")));
        assert!(!has_sidecars(&ground_units("NoSuchTemplate.Group")));
    }
}
