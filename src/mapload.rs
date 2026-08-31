//! Load Korea map catalog points from `References/` (and optional `templates/`).
//!
//! Airfields and buildings come from `MCU_Waypoint` entries in
//! `landscape_Korea_FullScene.Group` (MARKS groups) plus standalone
//! airfield `.Group` files.

use std::path::{Path, PathBuf};

use crate::ast::Il2Entity;
use crate::parser::parse_entity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    Airfield,
    Building,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapPoint {
    pub name: String,
    pub kind: PointKind,
    pub x: f64,
    pub z: f64,
}

#[derive(Clone, Debug, Default)]
pub struct MapCatalog {
    pub points: Vec<MapPoint>,
    pub sources: Vec<PathBuf>,
}

impl MapCatalog {
    pub fn airfields(&self) -> impl Iterator<Item = &MapPoint> {
        self.points.iter().filter(|p| p.kind == PointKind::Airfield)
    }

    pub fn buildings(&self) -> impl Iterator<Item = &MapPoint> {
        self.points.iter().filter(|p| p.kind == PointKind::Building)
    }
}

pub fn template_dirs() -> Vec<PathBuf> {
    ["References", "templates"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

pub fn load_catalog_from_dirs(dirs: &[PathBuf]) -> Result<MapCatalog, String> {
    let mut cat = MapCatalog::default();
    for dir in dirs {
        load_dir(dir, &mut cat)?;
    }
    if cat.points.is_empty() {
        return Err("no airfield or building points found in References/templates.".into());
    }
    Ok(cat)
}

pub fn load_catalog() -> Result<MapCatalog, String> {
    let dirs = template_dirs();
    if dirs.is_empty() {
        return Err("References/ (or templates/) folder was not found.".into());
    }
    load_catalog_from_dirs(&dirs)
}

fn load_dir(dir: &Path, cat: &mut MapCatalog) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("group"))
        })
        .collect();
    files.sort();
    for path in files {
        load_group_file(&path, cat)?;
    }
    Ok(())
}

fn load_group_file(path: &Path, cat: &mut MapCatalog) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let mut rest = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut first = true;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let (next, entity) = parse_entity(rest).map_err(|err| {
            format!(
                "{}: parse error: {err}",
                path.display()
            )
        })?;
        let is_marks = entity.name() == Some("MARKS");
        let before = cat.points.len();
        collect_points(&entity, None, cat);
        if first && cat.points.len() == before && looks_like_airfield_file(path) {
            if let Some((x, z)) = entity.first_xz() {
                cat.points.push(MapPoint {
                    name: entity
                        .name()
                        .map(str::to_string)
                        .unwrap_or_else(|| file_stem(path)),
                    kind: PointKind::Airfield,
                    x,
                    z,
                });
            }
        }
        first = false;
        rest = next;
        // FullScene scenery groups after MARKS are 3D Blocks, not catalog points.
        if is_marks {
            break;
        }
    }
    cat.sources.push(path.to_path_buf());
    Ok(())
}

fn looks_like_airfield_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_uppercase().contains("AFB"))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("point")
        .to_string()
}

fn collect_points(entity: &Il2Entity, parent: Option<&str>, cat: &mut MapCatalog) {
    let name = entity.name();
    if entity.block_type == "MCU_Waypoint" {
        if let (Some(kind), Some((x, z))) = (kind_for_parent(parent), entity.pos_xz()) {
            cat.points.push(MapPoint {
                name: name.unwrap_or("Waypoint").to_string(),
                kind,
                x,
                z,
            });
        }
    }
    let next_parent = match name {
        Some("AIRFIELDS") => Some("AIRFIELDS"),
        Some("CITIES") => Some("CITIES"),
        Some("RW_STATIONS") => Some("RW_STATIONS"),
        Some("MILITARY_CAMP") => Some("MILITARY_CAMP"),
        Some("INDUSTRIAL zones and ports") => Some("INDUSTRIAL"),
        Some("MARKS") => parent,
        _ => parent,
    };
    for child in &entity.children {
        collect_points(child, next_parent, cat);
    }
}

fn kind_for_parent(parent: Option<&str>) -> Option<PointKind> {
    match parent {
        Some("AIRFIELDS") => Some(PointKind::Airfield),
        Some("CITIES") | Some("RW_STATIONS") | Some("MILITARY_CAMP") | Some("INDUSTRIAL") => {
            Some(PointKind::Building)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k13_airfield_group_is_a_point() {
        let path = PathBuf::from("References/K13 AFB_mp.Group");
        if !path.exists() {
            return;
        }
        let mut cat = MapCatalog::default();
        load_group_file(&path, &mut cat).expect("parse K13");
        assert!(!cat.points.is_empty());
        assert!(cat.airfields().count() >= 1);
        assert!(cat.points.iter().all(|p| p.kind == PointKind::Airfield));
        assert!(cat.points.iter().any(|p| p.z > 200_000.0));
    }

    #[test]
    fn landscape_marks_include_kimpo_and_seoul() {
        let path = PathBuf::from("References/landscape_Korea_FullScene.Group");
        if !path.exists() {
            return;
        }
        let mut cat = MapCatalog::default();
        load_group_file(&path, &mut cat).expect("parse landscape");
        assert!(
            cat.airfields().any(|p| p.name.contains("Kimpo")),
            "AIRFIELDS should list K-14 Kimpo"
        );
        assert!(
            cat.buildings().any(|p| p.name.contains("Seoul")),
            "cities or rail should mention Seoul"
        );
        let dirs = template_dirs();
        assert!(dirs.iter().any(|d| d.ends_with("References")));
        let full = load_catalog_from_dirs(&dirs).expect("catalog");
        assert!(full.airfields().count() >= cat.airfields().count());
        assert!(load_catalog().is_ok());
    }
}
