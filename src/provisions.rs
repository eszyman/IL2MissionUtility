//! provisions.rs — scenery loaded for a base map, and the harvested-airfield list
//!
//! The Map tab imports a fixed set of landscape groups from a template folder
//! the user picks. Harvested airfields stay in the Airfield tab: that list is
//! what a base map places. `catalog.Group` is the harvest index, not a field,
//! so it is left out.
//!
//! ## Public API
//! * `LANDSCAPE_FILES` / `DEFAULT_TEMPLATE_DIR`
//! * `struct LoadedGroups` / `fn load_named_groups` / `fn load_group`
//! * `fn harvested_airfield_paths` / `fn load_harvested_airfields`
//! * `fn place_matching_coalition` / `fn pick_coalition_airfields`
//! * `fn guess_nato` / `fn landscape_label`
//!
//! ## Used by
//! * ui (Map, Airfield) — template folder, airbase list, base-map export

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::aircraft::{country_is_nato, country_nation};
use crate::ast::Il2Entity;
use crate::frontlines::MapRefGroup;
use crate::parser::{parse_group_file, parse_il2_document};

/// Korea install. The Map References tab starts here; Browse can change it.
pub const DEFAULT_TEMPLATE_DIR: &str = r"C:\Program Files\IL2Series\game\data\Template";

/// Scenery and blocks imported from the template folder. Airfields are not
/// in this list; they come from the harvested database folder.
pub const LANDSCAPE_FILES: &[&str] = &[
    "landscape_Korea_Tunnels.Group",
    "landscape_Korea_RailwayStations.Group",
    "landscape_Korea_Mines.Group",
    "landscape_Korea_MilitaryCamps.Group",
    "landscape_Korea_Industry_Ports.Group",
    "landscape_Korea_Dams.Group",
    "landscape_Korea_Cities.Group",
    "landscape_Korea_BridgesRoad.Group",
    "landscape_Korea_BridgesRailroad.Group",
];

pub struct LoadedGroups {
    pub groups: Vec<MapRefGroup>,
    pub missing: Vec<String>,
    pub errors: Vec<String>,
}

/// `landscape_Korea_Cities.Group` → `Cities`.
pub fn landscape_label(file_name: &str) -> String {
    file_name
        .trim_end_matches(".Group")
        .trim_end_matches(".group")
        .trim_start_matches("landscape_Korea_")
        .to_string()
}

pub fn is_landscape_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| {
            LANDSCAPE_FILES
                .iter()
                .any(|file| file.eq_ignore_ascii_case(name))
        })
}

/// NATO when the airfield's Country is in the 600s (USA 601), or the file
/// stem ends in `_601`. DPRK is the 500s (USSR 501, DPRK 502, PRC 503).
/// Anything else stays NATO.
pub fn guess_nato(root: &Il2Entity, path: &Path) -> bool {
    if let Some(country) = airfield_country(root) {
        return country_is_nato(country).unwrap_or(true);
    }
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return true;
    };
    match stem_country(stem) {
        Some(country) => country_is_nato(country).unwrap_or(true),
        None => true,
    }
}

fn airfield_country(root: &Il2Entity) -> Option<i32> {
    let mut found = None;
    root.for_each(&mut |e: &Il2Entity| {
        if e.block_type == "Airfield"
            && let Some(country) = e.property("Country").and_then(|s| s.parse().ok())
        {
            found = Some(country);
        }
    });
    found
}

pub fn load_group(path: &Path) -> Result<MapRefGroup, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
    let entity = parse_il2_document(&text).or_else(|_| parse_group_file(&text))?;
    Ok(MapRefGroup {
        path: path.to_path_buf(),
        entity,
    })
}

pub fn load_named_groups(dir: &Path, names: &[&str]) -> LoadedGroups {
    let mut groups = Vec::new();
    let mut missing = Vec::new();
    let mut errors = Vec::new();
    if !dir.is_dir() {
        missing.extend(names.iter().map(|name| (*name).to_string()));
        return LoadedGroups {
            groups,
            missing,
            errors,
        };
    }
    for name in names {
        let path = dir.join(name);
        if !path.is_file() {
            missing.push((*name).to_string());
            continue;
        }
        match load_group(&path) {
            Ok(group) => groups.push(group),
            Err(err) => errors.push(err),
        }
    }
    LoadedGroups {
        groups,
        missing,
        errors,
    }
}

/// Top-level `*.Group` files in the harvest database, excluding `catalog.Group`.
pub fn harvested_airfield_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".group") {
            continue;
        }
        if name.eq_ignore_ascii_case("catalog.group") {
            continue;
        }
        paths.push(path);
    }
    paths.sort();
    paths
}

pub fn load_harvested_airfields(dir: &Path) -> LoadedGroups {
    let mut groups = Vec::new();
    let mut errors = Vec::new();
    for path in harvested_airfield_paths(dir) {
        match load_group(&path) {
            Ok(group) => groups.push(group),
            Err(err) => errors.push(err),
        }
    }
    LoadedGroups {
        groups,
        missing: Vec::new(),
        errors,
    }
}

/// Country and field key of one harvested group.
pub struct AirfieldIdentity {
    pub key: String,
    pub country: Option<i32>,
    pub pos: Option<(f64, f64)>,
}

/// Groups kept for one coalition, plus notes for copies that were left out.
pub struct AirfieldPlacement {
    pub groups: Vec<MapRefGroup>,
    pub notes: Vec<String>,
}

/// Airfield block name, else the file stem with a trailing `_601` / `_502` removed.
pub fn airfield_identity(root: &Il2Entity, path: &Path) -> AirfieldIdentity {
    let mut country = None;
    let mut pos = None;
    let mut name = None;
    root.for_each(&mut |entity: &Il2Entity| {
        if entity.block_type != "Airfield" {
            return;
        }
        if country.is_none() {
            country = entity.property("Country").and_then(|s| s.parse().ok());
        }
        if pos.is_none() {
            pos = entity.pos_xz();
        }
        if name.is_none() && entity.name().is_some_and(useful_field_name) {
            name = entity.name().map(str::to_string);
        }
    });
    if country.is_none() {
        country = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(stem_country);
    }
    if pos.is_none() {
        pos = root.first_xz();
    }
    let key = name.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(stem_key)
            .unwrap_or_else(|| "Airfield".into())
    });
    AirfieldIdentity { key, country, pos }
}

fn useful_field_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && !name.eq_ignore_ascii_case("airfield")
        && !name.eq_ignore_ascii_case("group")
}

/// `K-13_Suwon_AF_601` → `K-13_Suwon_AF`, and the same after a `_running` tail.
fn stem_key(stem: &str) -> String {
    let stem = stem.strip_suffix("_running").unwrap_or(stem);
    match stem.rsplit_once('_') {
        Some((base, suffix))
            if suffix
                .parse::<i32>()
                .ok()
                .and_then(country_is_nato)
                .is_some() =>
        {
            base.to_string()
        }
        _ => stem.to_string(),
    }
}

fn stem_country(stem: &str) -> Option<i32> {
    let stem = stem.strip_suffix("_running").unwrap_or(stem);
    let (_, suffix) = stem.rsplit_once('_')?;
    let country = suffix.parse::<i32>().ok()?;
    country_is_nato(country)?;
    Some(country)
}

fn coalition_word(nato: bool) -> &'static str {
    if nato { "NATO" } else { "DPRK" }
}

fn nation_list(countries: &[i32]) -> String {
    let mut names: Vec<String> = countries.iter().copied().map(country_nation).collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        "other".into()
    } else {
        names.join(" and ")
    }
}

fn keep_rank(country: i32, want_nato: bool, key: &str, path: &Path) -> (i32, i32, String) {
    let rank = if want_nato {
        if country == 601 { 0 } else { 1 }
    } else {
        match country {
            502 => 0,
            503 => 1,
            501 => 2,
            _ => 3,
        }
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let canonical = if stem == format!("{key}_{country}") {
        0
    } else {
        1
    };
    (rank, canonical, stem)
}

/// Keep the files whose country belongs to `want_nato`. Two files for one
/// field yield one group. A field that exists only for the other coalition
/// is omitted, not replaced with that copy.
pub fn pick_coalition_airfields(groups: Vec<MapRefGroup>, want_nato: bool) -> AirfieldPlacement {
    let mut buckets: BTreeMap<String, Vec<MapRefGroup>> = BTreeMap::new();
    for group in groups {
        let key = airfield_identity(&group.entity, &group.path).key;
        buckets.entry(key).or_default().push(group);
    }
    let mut kept = Vec::new();
    let mut notes = Vec::new();
    for (key, copies) in buckets {
        let (matching, other): (Vec<MapRefGroup>, Vec<MapRefGroup>) =
            copies.into_iter().partition(|group| {
                airfield_identity(&group.entity, &group.path)
                    .country
                    .and_then(country_is_nato)
                    == Some(want_nato)
            });
        if matching.is_empty() {
            if !other.is_empty() {
                let countries: Vec<i32> = other
                    .iter()
                    .filter_map(|group| airfield_identity(&group.entity, &group.path).country)
                    .collect();
                notes.push(format!(
                    "{key} has no {} harvest, so the {} group was not placed.",
                    coalition_word(want_nato),
                    nation_list(&countries)
                ));
            }
            continue;
        }
        let other_countries: Vec<i32> = other
            .iter()
            .filter_map(|group| airfield_identity(&group.entity, &group.path).country)
            .collect();
        let best = matching
            .into_iter()
            .min_by_key(|group| {
                let id = airfield_identity(&group.entity, &group.path);
                keep_rank(id.country.unwrap_or(0), want_nato, &key, &group.path)
            })
            .expect("matching is not empty");
        if !other_countries.is_empty() {
            notes.push(format!(
                "{key}: placed the {} group and left out the {} copy.",
                coalition_word(want_nato),
                nation_list(&other_countries)
            ));
        }
        kept.push(best);
    }
    AirfieldPlacement {
        groups: kept,
        notes,
    }
}

/// True when two loaded groups are the same scenery file or the same airfield.
/// A second copy in a saved base map is the one to drop.
pub fn same_loaded_ref(a: &MapRefGroup, b: &MapRefGroup) -> bool {
    let an = a.entity.name().unwrap_or("").trim();
    let bn = b.entity.name().unwrap_or("").trim();
    if !an.is_empty() && an.eq_ignore_ascii_case(bn) {
        return true;
    }
    if !group_has_airfield(&a.entity) || !group_has_airfield(&b.entity) {
        return false;
    }
    airfield_identity(&a.entity, &a.path)
        .key
        .eq_ignore_ascii_case(&airfield_identity(&b.entity, &b.path).key)
}

/// `manual` with provision copies and repeated names removed. Order is kept.
pub fn without_duplicates(
    manual: Vec<MapRefGroup>,
    provisions: &[MapRefGroup],
) -> Vec<MapRefGroup> {
    let mut out = Vec::new();
    for group in manual {
        let seen = provisions
            .iter()
            .chain(out.iter())
            .any(|kept| same_loaded_ref(kept, &group));
        if !seen {
            out.push(group);
        }
    }
    out
}

pub fn group_has_airfield(root: &Il2Entity) -> bool {
    let mut found = false;
    root.for_each(&mut |entity: &Il2Entity| {
        if entity.block_type == "Airfield" {
            found = true;
        }
    });
    found
}

/// One group per field. The side comes from `nato_at` (the front). The other
/// country's file stays on disk and is not used in its place.
/// `either_at` keeps one NATO file and one DPRK file when the field sits in a salient.
pub fn place_matching_coalition(
    groups: Vec<MapRefGroup>,
    nato_at: impl Fn(f64, f64) -> bool,
    either_at: impl Fn(f64, f64) -> bool,
) -> AirfieldPlacement {
    let mut buckets: BTreeMap<String, Vec<MapRefGroup>> = BTreeMap::new();
    let mut pos: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for group in groups {
        let id = airfield_identity(&group.entity, &group.path);
        if let Some(at) = id.pos {
            pos.entry(id.key.clone()).or_insert(at);
        }
        buckets.entry(id.key).or_default().push(group);
    }
    let mut placed = AirfieldPlacement {
        groups: Vec::new(),
        notes: Vec::new(),
    };
    for (key, copies) in buckets {
        if let Some((x, z)) = pos.get(&key)
            && either_at(*x, *z)
        {
            let nato = pick_coalition_airfields(copies.clone(), true);
            let dprk = pick_coalition_airfields(copies, false);
            if nato.groups.is_empty() && dprk.groups.is_empty() {
                continue;
            }
            if nato.groups.is_empty() {
                placed.notes.push(format!(
                    "{key} is inside a salient. There is no NATO harvest, so only the DPRK group was kept."
                ));
            } else if dprk.groups.is_empty() {
                placed.notes.push(format!(
                    "{key} is inside a salient. There is no DPRK harvest, so only the NATO group was kept."
                ));
            } else {
                placed.notes.push(format!(
                    "{key} is inside a salient, so both the NATO and DPRK groups were kept."
                ));
            }
            placed.groups.extend(nato.groups);
            placed.groups.extend(dprk.groups);
            continue;
        }
        let want = if let Some((x, z)) = pos.get(&key) {
            nato_at(*x, *z)
        } else if let Some(nato) = agreed_coalition(&copies) {
            nato
        } else {
            placed.notes.push(format!(
                "{key} has both coalitions and no position, so neither group was placed."
            ));
            continue;
        };
        let pick = pick_coalition_airfields(copies, want);
        placed.groups.extend(pick.groups);
        placed.notes.extend(pick.notes);
    }
    placed
}

fn agreed_coalition(groups: &[MapRefGroup]) -> Option<bool> {
    let mut found = None;
    for group in groups {
        let Some(nato) = airfield_identity(&group.entity, &group.path)
            .country
            .and_then(country_is_nato)
        else {
            continue;
        };
        match found {
            None => found = Some(nato),
            Some(prev) if prev != nato => return None,
            Some(_) => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_list_is_the_nine_scenery_groups() {
        assert_eq!(LANDSCAPE_FILES.len(), 9);
        assert!(LANDSCAPE_FILES.iter().all(|name| {
            !name.to_ascii_lowercase().contains("airfield")
                && name.starts_with("landscape_Korea_")
                && name.ends_with(".Group")
        }));
        assert!(LANDSCAPE_FILES.contains(&"landscape_Korea_BridgesRailroad.Group"));
        assert_eq!(
            landscape_label("landscape_Korea_Industry_Ports.Group"),
            "Industry_Ports"
        );
    }

    #[test]
    fn missing_template_folder_lists_every_file() {
        let loaded = load_named_groups(Path::new("does-not-exist-template"), LANDSCAPE_FILES);
        assert!(loaded.groups.is_empty());
        assert_eq!(loaded.missing.len(), LANDSCAPE_FILES.len());
    }

    #[test]
    fn harvest_folder_skips_the_catalog() {
        let dir = std::env::temp_dir().join(format!("il2_provisions_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("catalog.Group"), "Group\n{\n}\n").unwrap();
        std::fs::write(
            dir.join("Suwon_601.Group"),
            "Group\n{\n  Name = \"Suwon\";\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "skip").unwrap();
        let paths = harvested_airfield_paths(&dir);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("Suwon_601.Group"));
        let loaded = load_harvested_airfields(&dir);
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].entity.name(), Some("Suwon"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nato_follows_country_then_the_file_suffix() {
        let mut root = Il2Entity::new("Group");
        let path = PathBuf::from("Suwon_601.Group");
        assert!(guess_nato(&root, &path));
        assert!(!guess_nato(&root, Path::new("Yonpo_502.Group")));
        assert!(!guess_nato(&root, Path::new("Pyongyang_503.Group")));
        let mut field = Il2Entity::new("Airfield");
        field.set_property("Country", "502");
        root.children.push(field);
        assert!(!guess_nato(&root, &path));
    }

    fn country_file(name: &str, country: i32) -> MapRefGroup {
        let mut root = Il2Entity::new("Group");
        root.set_name(name);
        let mut field = Il2Entity::new("Airfield");
        field.set_name(name);
        field.set_property("Country", country.to_string());
        field.set_property("XPos", "269648.000");
        field.set_property("ZPos", "180083.000");
        root.children.push(field);
        MapRefGroup {
            path: PathBuf::from(format!("{name}_{country}.Group")),
            entity: root,
        }
    }

    #[test]
    fn two_country_files_place_only_the_selected_coalition() {
        let usa = country_file("K-23_Pyongyang_AF", 601);
        let prc = country_file("K-23_Pyongyang_AF", 503);
        let dprk = country_file("K-23_Pyongyang_AF", 502);

        let running = {
            let mut extra = country_file("K-23_Pyongyang_AF", 601);
            extra.path = PathBuf::from("K-23_Pyongyang_AF_601_running.Group");
            extra
        };
        let one = pick_coalition_airfields(vec![usa.clone(), running], true);
        assert_eq!(one.groups.len(), 1);
        assert!(one.groups[0].path.ends_with("K-23_Pyongyang_AF_601.Group"));

        let nato = pick_coalition_airfields(vec![usa.clone(), prc.clone()], true);
        assert_eq!(nato.groups.len(), 1);
        assert!(nato.groups[0].path.ends_with("K-23_Pyongyang_AF_601.Group"));
        assert!(nato.notes.iter().any(|note| note.contains("PRC")));
        assert!(nato.notes.iter().all(|note| !note.contains("Eastern")));

        let east = pick_coalition_airfields(vec![usa.clone(), prc.clone()], false);
        assert_eq!(east.groups.len(), 1);
        assert!(east.groups[0].path.ends_with("K-23_Pyongyang_AF_503.Group"));
        assert!(east.notes.iter().any(|note| note.contains("USA")));

        let missing = pick_coalition_airfields(vec![usa.clone()], false);
        assert!(missing.groups.is_empty());
        assert!(
            missing
                .notes
                .iter()
                .any(|note| note.contains("no DPRK harvest")),
            "{:?}",
            missing.notes
        );

        let both_dprk = pick_coalition_airfields(vec![dprk, prc], false);
        assert_eq!(both_dprk.groups.len(), 1, "one field, one group");
        assert!(
            both_dprk.groups[0]
                .path
                .ends_with("K-23_Pyongyang_AF_502.Group"),
            "DPRK 502 is kept ahead of PRC 503"
        );

        let placed = place_matching_coalition(
            vec![usa, country_file("K-23_Pyongyang_AF", 503)],
            |x, _| x < 200_000.0,
            |_, _| false,
        );
        assert_eq!(placed.groups.len(), 1);
        assert!(
            placed.groups[0]
                .path
                .ends_with("K-23_Pyongyang_AF_503.Group")
        );
    }

    #[test]
    fn a_salient_keeps_both_coalitions() {
        let usa = country_file("K-14_Kimpo_AF", 601);
        let dprk = country_file("K-14_Kimpo_AF", 502);
        let placed = place_matching_coalition(vec![usa, dprk], |_, _| true, |_, _| true);
        assert_eq!(placed.groups.len(), 2);
        assert!(placed.notes.iter().any(|note| note.contains("both")));
    }

    #[test]
    fn a_saved_base_map_does_not_keep_a_second_copy() {
        let provision = country_file("K-23_Pyongyang_AF", 503);
        let mut tunnels = Il2Entity::new("Group");
        tunnels.set_name("TUNNELS");
        let loaded = vec![
            country_file("K-23_Pyongyang_AF", 601),
            country_file("K-23_Pyongyang_AF", 503),
            MapRefGroup {
                path: PathBuf::from("TUNNELS.Group"),
                entity: tunnels.clone(),
            },
            MapRefGroup {
                path: PathBuf::from("TUNNELS-again.Group"),
                entity: tunnels,
            },
            country_file("Hand_Added", 601),
        ];
        let kept = without_duplicates(loaded, &[provision]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].entity.name(), Some("TUNNELS"));
        assert_eq!(kept[1].entity.name(), Some("Hand_Added"));
    }
}
