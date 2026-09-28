//! heightprobe.rs — terrain height probes: tile files out, snapped files in
//!
//! Builds Helper Input probe groups that measure the editor's terrain (one
//! `MCU_H_Input` per land node of the 100 m lattice). A full 224 × 224-node
//! tile is about 50 000 probes, and the editor spends minutes on set-to-ground
//! for a group that large, so each file holds at most `PROBE_CHUNK` probes.
//! Snapped files are read back into a `terrain::HeightStore`. Probes are
//! named `H<iiii>_<jjjj>` after their lattice node, so a snapped file needs
//! nothing else. The land test matches `tools/heightgrid/gen_land_grid.py`:
//! dry cells of `combined_terrain.bin` inside the map frame, grown 2 cells
//! into the sea. Sea snaps to exactly 0 m; only the 800 m survey pass
//! probes it on purpose, to learn where the game's water is.
//!
//! A land probe still at Y = 0 was not snapped: it is reported and ignored,
//! never stored as sea level. A file with no probe above 0 m is not merged.
//!
//! ## Public API
//! * `CONTENT_X` / `CONTENT_Z` — map area inside the image frame
//! * `tile_probe_nodes`, `tile_probe_count`, `probe_tiles`
//! * `tile_name`, `probe_name`, `parse_probe_name`
//! * `PROBE_CHUNK`, `probe_part_count`, `probe_part_name`, `probe_part`
//! * `probe_group`, `tile_probe_group`, `survey_nodes`
//! * `ingest` / `IngestReport` — merge a snapped tree into a store
//! * `run_cli` — `--probe-*` command-line mode (see `CLI_HELP`)
//!
//! ## Used by
//! * ui.rs — Map › Terrain: export survey / AO tiles, import snapped files

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::ast::Il2Entity;
use crate::geo::MAP_MAX;
use crate::parser::parse_il2_document;
use crate::terrain::{
    HeightStore, LATTICE_N, STEP_M, TILES_PER_SIDE, tile_node_range, tile_of_node,
};
use crate::watermap::TerrainMap;

/// Map content inside the image frame (the mask marks the frame as land).
pub const CONTENT_X: (f64, f64) = (30_100.0, 468_400.0);
pub const CONTENT_Z: (f64, f64) = (29_600.0, 469_500.0);
/// Probes reach this many mask cells (~110 m each) past the coastline.
const COAST_BUFFER_CELLS: usize = 2;
/// Helper Inputs per exported file. A full inland tile is 50 176 probes;
/// the editor's select-all / set-to-ground on that one group takes minutes,
/// while the same two commands on a few hundred objects stay quick.
pub const PROBE_CHUNK: usize = 256;
/// Ground objects whose snapped Y is kept by `ingest(.., learn = true)`.
const LEARN_TYPES: [&str; 4] = ["Vehicle", "Train", "Block", "Ground"];

fn in_content(x: f64, z: f64) -> bool {
    (CONTENT_X.0..=CONTENT_X.1).contains(&x) && (CONTENT_Z.0..=CONTENT_Z.1).contains(&z)
}

/// Mask cells that get probes: dry, inside the frame, plus the coast buffer.
fn probe_cells() -> Option<&'static (Vec<bool>, usize, usize)> {
    static CELLS: OnceLock<Option<(Vec<bool>, usize, usize)>> = OnceLock::new();
    CELLS
        .get_or_init(|| {
            let map = TerrainMap::builtin().ok()?;
            let (w, h) = (map.width as usize, map.height as usize);
            let mut land = vec![false; w * h];
            for y in 0..h {
                let x_world = MAP_MAX - (y as f64 + 0.5) / h as f64 * MAP_MAX;
                for x in 0..w {
                    let z_world = (x as f64 + 0.5) / w as f64 * MAP_MAX;
                    land[y * w + x] =
                        in_content(x_world, z_world) && !map.is_water_cell(x as u32, y as u32);
                }
            }
            for _ in 0..COAST_BUFFER_CELLS {
                let prev = land.clone();
                for y in 0..h {
                    for x in 0..w {
                        let k = y * w + x;
                        if !prev[k]
                            && ((y > 0 && prev[k - w])
                                || (y + 1 < h && prev[k + w])
                                || (x > 0 && prev[k - 1])
                                || (x + 1 < w && prev[k + 1]))
                        {
                            land[k] = true;
                        }
                    }
                }
            }
            Some((land, w, h))
        })
        .as_ref()
}

fn keep_node(i: usize, j: usize) -> bool {
    let Some((cells, w, h)) = probe_cells() else {
        return false;
    };
    let (xi, zj) = (i as f64 * STEP_M, j as f64 * STEP_M);
    if !in_content(xi, zj) {
        return false;
    }
    let gy = (((MAP_MAX - xi) / MAP_MAX * *h as f64) as usize).min(h - 1);
    let gx = ((zj / MAP_MAX * *w as f64) as usize).min(w - 1);
    cells[gy * w + gx]
}

/// Probe nodes of tile (ti, tj), north → south then west → east.
pub fn tile_probe_nodes(ti: usize, tj: usize) -> Vec<(usize, usize)> {
    let (i_lo, i_hi, j_lo, j_hi) = tile_node_range(ti, tj);
    (i_lo..=i_hi)
        .rev()
        .flat_map(|i| (j_lo..=j_hi).map(move |j| (i, j)))
        .filter(|&(i, j)| keep_node(i, j))
        .collect()
}

pub fn tile_probe_count(ti: usize, tj: usize) -> usize {
    let (i_lo, i_hi, j_lo, j_hi) = tile_node_range(ti, tj);
    (i_lo..=i_hi)
        .map(|i| (j_lo..=j_hi).filter(|&j| keep_node(i, j)).count())
        .sum()
}

/// Every tile with at least one probe: (row, column, probe count).
pub fn probe_tiles() -> Vec<(usize, usize, usize)> {
    (0..TILES_PER_SIDE)
        .flat_map(|ti| (0..TILES_PER_SIDE).map(move |tj| (ti, tj)))
        .map(|(ti, tj)| (ti, tj, tile_probe_count(ti, tj)))
        .filter(|&(_, _, n)| n > 0)
        .collect()
}

pub fn tile_name(ti: usize, tj: usize) -> String {
    format!("HG100_T{ti:02}_{tj:02}")
}

pub fn probe_name(i: usize, j: usize) -> String {
    format!("H{i:04}_{j:04}")
}

/// `H<iiii>_<jjjj>` → lattice node.
pub fn parse_probe_name(name: &str) -> Option<(usize, usize)> {
    let b = name.as_bytes();
    if b.len() != 10 || b[0] != b'H' || b[5] != b'_' {
        return None;
    }
    let digits = |s: &str| {
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())
            .flatten()
    };
    let (i, j): (usize, usize) = (digits(&name[1..5])?, digits(&name[6..10])?);
    (i < LATTICE_N && j < LATTICE_N).then_some((i, j))
}

/// Earlier test grids (`HG_rrr_ccc`, `HGA_…`, `HGB_…`) — read by position.
fn is_legacy_probe(name: &str) -> bool {
    ["HG_", "HGA_", "HGB_"].iter().any(|p| name.starts_with(p))
}

/// How many files `nodes` split into.
pub fn probe_part_count(n: usize) -> usize {
    n.div_ceil(PROBE_CHUNK)
}

/// File stem for one part. A set that fits in one file keeps `base`;
/// larger sets are `base_p0000`, `base_p0001`, …
pub fn probe_part_name(base: &str, part: usize, parts: usize) -> String {
    if parts <= 1 {
        base.to_string()
    } else {
        format!("{base}_p{part:04}")
    }
}

/// One slice of `nodes`, at most `PROBE_CHUNK` Helper Inputs.
pub fn probe_part(base: &str, nodes: &[(usize, usize)], part: usize) -> Result<Il2Entity, String> {
    let parts = probe_part_count(nodes.len());
    if part >= parts {
        return Err(format!("probe part {part} of {parts}"));
    }
    let start = part * PROBE_CHUNK;
    let end = (start + PROBE_CHUNK).min(nodes.len());
    Ok(probe_group(
        &probe_part_name(base, part, parts),
        &nodes[start..end],
    ))
}

/// `MCU_H_Input` at Y = 0. The editor's set-to-ground writes the snapped height
/// back into `YPos`; export of these small MCUs stays quick.
fn helper_input(name: &str, index: i32, x: f64, z: f64) -> Il2Entity {
    let mut m = Il2Entity::new("MCU_H_Input");
    m.index = Some(index);
    m.set_property("Index", index.to_string());
    m.set_name(name);
    m.set_property("Desc", "\"\"");
    m.set_targets(Vec::new());
    m.set_objects(Vec::new());
    m.set_property("XPos", format!("{x:.3}"));
    m.set_property("YPos", "0.000");
    m.set_property("ZPos", format!("{z:.3}"));
    for key in ["XOri", "YOri", "ZOri"] {
        m.set_property(key, "0");
    }
    m
}

/// A `Group` of Helper Input probes at Y = 0 on the given lattice nodes.
pub fn probe_group(name: &str, nodes: &[(usize, usize)]) -> Il2Entity {
    let mut group = Il2Entity::new("Group");
    group.set_name(name);
    group.index = Some(1);
    group.set_property("Index", "1");
    group.set_property(
        "Desc",
        "\"Terrain height probe - select all, set to ground, save\"",
    );
    for (k, &(i, j)) in nodes.iter().enumerate() {
        group.children.push(helper_input(
            &probe_name(i, j),
            k as i32 + 2,
            i as f64 * STEP_M,
            j as f64 * STEP_M,
        ));
    }
    group
}

/// Every probe of one tile in a single group, or `None` if the tile is all
/// sea / frame. Exports use `probe_part` instead, so each file stays small.
pub fn tile_probe_group(ti: usize, tj: usize) -> Option<Il2Entity> {
    let nodes = tile_probe_nodes(ti, tj);
    if nodes.is_empty() {
        return None;
    }
    Some(probe_group(&tile_name(ti, tj), &nodes))
}

/// Survey pass: every `SURVEY_STRIDE`-th lattice node (800 m), whole square.
pub const SURVEY_STRIDE: usize = 8;
pub const SURVEY_NAME: &str = "HG_PASS1_SURVEY_800m";

/// Survey nodes over the full map square — sea and frame included, so the
/// snapped file shows where the game has water (exactly 0 m) and terrain.
pub fn survey_nodes() -> Vec<(usize, usize)> {
    (0..LATTICE_N)
        .rev()
        .step_by(SURVEY_STRIDE)
        .flat_map(|i| (0..LATTICE_N).step_by(SURVEY_STRIDE).map(move |j| (i, j)))
        .collect()
}

fn is_survey_node(i: usize, j: usize) -> bool {
    i.is_multiple_of(SURVEY_STRIDE) && j.is_multiple_of(SURVEY_STRIDE)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IngestReport {
    /// Probe heights written to lattice nodes.
    pub lattice: usize,
    /// Off-lattice probe heights kept as measured points.
    pub probe_points: usize,
    /// Snapped mission objects kept as measured points (`learn`).
    pub learned: usize,
    /// Probes at exactly 0 m over water (kept: water level).
    pub water: usize,
    /// Probes at exactly 0 m on land: not snapped, ignored.
    pub unsnapped: usize,
    /// Probes with a non-zero height.
    pub above_zero: usize,
    /// Tiles that received lattice heights.
    pub tiles: BTreeSet<(usize, usize)>,
    /// False when the file looked unsnapped and nothing was merged.
    pub merged: bool,
}

impl IngestReport {
    /// Every probe still at 0 m, or most land probes at 0 m: the file was
    /// saved without running set to ground.
    pub fn looks_unsnapped(&self) -> bool {
        let probes = self.lattice + self.probe_points + self.unsnapped;
        (probes > 0 && self.above_zero == 0) || (self.unsnapped > 0 && self.unsnapped * 2 > probes)
    }
}

fn num(e: &Il2Entity, key: &str) -> Option<f64> {
    e.property(key)?.trim().parse().ok()
}

enum Write {
    Node(usize, usize, f64),
    Point(f64, f64, f64),
}

/// Merge every probe in `root` into `store`; nothing is merged if the file
/// looks unsnapped. A probe at exactly 0 m counts as water when it is a
/// survey node (the survey covers sea on purpose) or the terrain mask says
/// water; otherwise it was not snapped and is skipped. With `learn`, snapped
/// ground objects (Vehicle / Train / Block / Ground with `PinToTerrain = 1`
/// and a non-zero Y) are kept as measured points too — only use it on files
/// the user has run set to ground on.
pub fn ingest(root: &Il2Entity, store: &mut HeightStore, learn: bool) -> IngestReport {
    let mask = TerrainMap::builtin().ok();
    let mut rep = IngestReport::default();
    let mut writes = Vec::new();
    root.for_each(&mut |e| {
        let (Some(x), Some(y), Some(z)) = (num(e, "XPos"), num(e, "YPos"), num(e, "ZPos")) else {
            return;
        };
        let name = e.name().unwrap_or("");
        if parse_probe_name(name).is_none() && !is_legacy_probe(name) {
            let pinned = e.property("PinToTerrain").map(str::trim) == Some("1");
            if learn && pinned && y != 0.0 && LEARN_TYPES.contains(&e.block_type.as_str()) {
                writes.push(Write::Point(x, z, y));
                rep.learned += 1;
            }
            return;
        }
        let (fi, fj) = (x / STEP_M, z / STEP_M);
        let node = ((fi - fi.round()).abs() < 0.005
            && (fj - fj.round()).abs() < 0.005
            && fi >= 0.0
            && fj >= 0.0)
            .then(|| (fi.round() as usize, fj.round() as usize))
            .filter(|&(i, j)| i < LATTICE_N && j < LATTICE_N);
        if y == 0.0 {
            let survey = node.is_some_and(|(i, j)| is_survey_node(i, j));
            if survey || mask.is_some_and(|m| m.is_water_xz(x, z)) {
                rep.water += 1;
            } else {
                rep.unsnapped += 1;
                return;
            }
        } else {
            rep.above_zero += 1;
        }
        match node {
            Some((i, j)) => {
                writes.push(Write::Node(i, j, y));
                rep.tiles.insert(tile_of_node(i, j));
                rep.lattice += 1;
            }
            None => {
                writes.push(Write::Point(x, z, y));
                rep.probe_points += 1;
            }
        }
    });
    if rep.looks_unsnapped() {
        return rep;
    }
    for w in writes {
        match w {
            Write::Node(i, j, y) => store.set_node(i, j, y),
            Write::Point(x, z, y) => store.add_point(x, z, y),
        }
    }
    rep.merged = true;
    rep
}

pub const CLI_HELP: &str = r"Terrain height probes (the window does not open):
  --probe-survey <folder>                 write the 800 m survey pass, one file per 256 probes
  --probe-ingest [--learn] <snapped>...   merge snapped .Group/.Mission files into the store
  --probe-status                          show what the store holds
  --store <path>                          use this store instead of the default
Default store: heights baked into this build, with
%APPDATA%\IL2MissionUtility\terrain\korea_100m.hgt merged on top when that file exists.";

/// `--probe-*` command-line mode. Returns the exit code, or `None` when the
/// arguments are not a probe command (the GUI starts).
pub fn run_cli(args: &[String]) -> Option<i32> {
    let cmd = args.first()?.as_str();
    if !cmd.starts_with("--probe-") {
        return None;
    }
    let mut store_path = crate::terrain::default_store_path();
    let mut learn = false;
    let mut files = Vec::new();
    let mut rest = args[1..].iter();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--learn" => learn = true,
            "--store" => match rest.next() {
                Some(p) => store_path = p.into(),
                None => return Some(fail("--store needs a path")),
            },
            _ => files.push(a.clone()),
        }
    }
    let result = match cmd {
        "--probe-survey" => cli_survey(&files),
        "--probe-ingest" => cli_ingest(&files, &store_path, learn),
        "--probe-status" => cli_status(&store_path),
        _ => Err(CLI_HELP.to_string()),
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => fail(&e),
    })
}

fn fail(msg: &str) -> i32 {
    eprintln!("{msg}");
    1
}

fn cli_survey(files: &[String]) -> Result<(), String> {
    let [out] = files else {
        return Err(format!(
            "--probe-survey needs one output folder

{CLI_HELP}"
        ));
    };
    let dir = std::path::Path::new(out);
    std::fs::create_dir_all(dir).map_err(|e| format!("{out}: {e}"))?;
    let nodes = survey_nodes();
    let parts = probe_part_count(nodes.len());
    for part in 0..parts {
        let group = probe_part(SURVEY_NAME, &nodes, part)?;
        let name = group.name().unwrap_or(SURVEY_NAME);
        let path = dir.join(format!("{name}.Group"));
        std::fs::write(&path, crate::serialize::serialize_group(&group))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    println!(
        "{out}: {parts} files, {} Helper Inputs, 800 m apart over the whole map (sea and frame included)",
        nodes.len()
    );
    println!(
        "Import one file, select all, set to ground, save, then run --probe-ingest on the saved files."
    );
    Ok(())
}

fn cli_ingest(files: &[String], store_path: &std::path::Path, learn: bool) -> Result<(), String> {
    if files.is_empty() {
        return Err(format!(
            "--probe-ingest needs at least one snapped file

{CLI_HELP}"
        ));
    }
    let mut store = crate::terrain::open_store(store_path)?;
    let mut refused = 0;
    for path in files {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let root = parse_il2_document(&text).map_err(|e| format!("{path}: {e}"))?;
        let rep = ingest(&root, &mut store, learn);
        println!(
            "{path}: {} heights, {} water, {} extra points, {} learned, {} unsnapped",
            rep.lattice, rep.water, rep.probe_points, rep.learned, rep.unsnapped
        );
        if !rep.merged {
            refused += 1;
            println!(
                "  NOT merged: it looks unsnapped (select all, set to ground, save, then retry)"
            );
        }
    }
    store.save(store_path)?;
    cli_status(store_path)?;
    if refused > 0 {
        Err(format!("{refused} file(s) not merged"))
    } else {
        Ok(())
    }
}

fn cli_status(store_path: &std::path::Path) -> Result<(), String> {
    let store = crate::terrain::open_store(store_path)?;
    let tiles = (0..TILES_PER_SIDE)
        .flat_map(|ti| (0..TILES_PER_SIDE).map(move |tj| (ti, tj)))
        .filter(|&(ti, tj)| store.tile_measured(ti, tj) > 0)
        .count();
    println!(
        "{}: {} measured nodes in {tiles} tiles, {} extra points",
        store_path.display(),
        store.measured_nodes(),
        store.points().len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;
    use crate::terrain::KOREA_MAP_ID;

    /// Counts from the Python generator's tiles.csv (same mask and bounds).
    #[test]
    fn tile_counts_match_python_generator() {
        for (ti, tj, n) in [
            (5, 16, 18_106),
            (11, 4, 3),
            (20, 1, 16),
            (10, 10, 50_176),
            (1, 1, 21_280),
            (12, 18, 12_508),
        ] {
            assert_eq!(tile_probe_count(ti, tj), n, "tile {ti}_{tj}");
        }
        assert_eq!(tile_probe_count(0, 0), 0, "frame corner has no probes");
    }

    #[test]
    fn all_tiles_match_python_generator() {
        let tiles = probe_tiles();
        assert_eq!(tiles.len(), 310);
        assert_eq!(tiles.iter().map(|t| t.2).sum::<usize>(), 12_443_401);
    }

    #[test]
    fn probe_names_round_trip() {
        assert_eq!(probe_name(3905, 3628), "H3905_3628");
        assert_eq!(parse_probe_name("H3905_3628"), Some((3905, 3628)));
        for bad in [
            "H3905-3628",
            "H390_36280",
            "X3905_3628",
            "H9999_0001",
            "H12a4_0001",
            "HG_000_001",
        ] {
            assert_eq!(parse_probe_name(bad), None, "{bad}");
        }
        assert_eq!(tile_name(5, 16), "HG100_T05_16");
    }

    #[test]
    fn probe_group_is_a_helper_input_on_the_lattice() {
        let g = tile_probe_group(11, 4).unwrap();
        assert_eq!(g.name(), Some("HG100_T11_04"));
        assert_eq!(g.children.len(), 3);
        let v = &g.children[0];
        assert_eq!(v.block_type, "MCU_H_Input");
        assert_eq!(v.name(), Some("H2305_1098"));
        assert_eq!(v.property("XPos"), Some("230500.000"));
        assert_eq!(v.property("ZPos"), Some("109800.000"));
        assert_eq!(v.property("YPos"), Some("0.000"));
        assert_eq!(v.property("Desc"), Some("\"\""));
        assert_eq!(v.property("Targets"), Some("[]"));
        assert_eq!(v.property("Objects"), Some("[]"));
        assert_eq!(v.property("XOri"), Some("0"));
        assert_eq!(v.targets, Vec::<i32>::new());
        assert_eq!(v.objects, Vec::<i32>::new());
        assert_eq!(v.index, Some(2));
        assert!(tile_probe_group(0, 0).is_none());
    }

    #[test]
    fn large_sets_split_into_chunk_sized_files() {
        let nodes: Vec<(usize, usize)> = (0..PROBE_CHUNK + 3).map(|j| (1000, j)).collect();
        assert_eq!(probe_part_count(nodes.len()), 2);
        let a = probe_part("HG100_T01_01", &nodes, 0).unwrap();
        let b = probe_part("HG100_T01_01", &nodes, 1).unwrap();
        assert_eq!(a.children.len(), PROBE_CHUNK);
        assert_eq!(b.children.len(), 3);
        assert_eq!(a.name(), Some("HG100_T01_01_p0000"));
        assert_eq!(b.name(), Some("HG100_T01_01_p0001"));
        assert_eq!(a.children.last().unwrap().name(), Some("H1000_0255"));
        assert_eq!(b.children[0].name(), Some("H1000_0256"));
        assert!(probe_part("HG100_T01_01", &nodes, 2).is_err());
        assert_eq!(probe_part_name("HG100_T11_04", 0, 1), "HG100_T11_04");
    }

    #[test]
    fn probe_file_round_trips_through_parser() {
        let g = tile_probe_group(11, 4).unwrap();
        let text = serialize_group(&g);
        let back = parse_group_file(&text).unwrap();
        assert_eq!(back, g);
        assert_eq!(serialize_group(&back), text);
    }

    fn snapped(y_by_name: &[(&str, &str)], extra: &str) -> Il2Entity {
        let nodes = [(3905, 3628), (3905, 3629), (3906, 3628)];
        let mut text = serialize_group(&probe_group("T", &nodes));
        for (name, y) in y_by_name {
            let at = text.find(&format!("\"{name}\"")).unwrap();
            let yline = at + text[at..].find("YPos = 0.000").unwrap();
            text.replace_range(yline..yline + 12, &format!("YPos = {y}"));
        }
        let close = text.rfind('}').unwrap();
        text.insert_str(close, extra);
        parse_group_file(&text).unwrap()
    }

    #[test]
    fn ingest_stores_snapped_probes_and_skips_unsnapped() {
        let root = snapped(&[("H3905_3628", "412.250"), ("H3905_3629", "398.100")], "");
        let mut store = HeightStore::new(KOREA_MAP_ID);
        let rep = ingest(&root, &mut store, false);
        assert_eq!(rep.lattice, 2);
        assert_eq!(rep.unsnapped, 1, "H3906_3628 still at 0 m on land");
        assert!(rep.tiles.contains(&tile_of_node(3905, 3628)));
        assert_eq!(store.node(3905, 3628), Some(412.3));
        assert_eq!(store.node(3905, 3629), Some(398.1));
        assert_eq!(store.node(3906, 3628), None);
        assert!(!rep.looks_unsnapped());
    }

    #[test]
    fn ingest_flags_a_file_saved_without_snapping() {
        let root = snapped(&[], "");
        let mut store = HeightStore::new(KOREA_MAP_ID);
        let rep = ingest(&root, &mut store, false);
        assert_eq!(rep.unsnapped, 3);
        assert!(rep.looks_unsnapped());
        assert!(!rep.merged);
        assert_eq!(store.measured_nodes(), 0);
    }

    #[test]
    fn survey_covers_the_whole_square_every_800_m() {
        let nodes = survey_nodes();
        assert_eq!(nodes.len(), 625 * 625);
        assert_eq!(nodes[0], (4992, 0), "north-west corner first");
        assert_eq!(*nodes.last().unwrap(), (0, 4992));
        assert!(nodes.iter().all(|&(i, j)| is_survey_node(i, j)));
    }

    #[test]
    fn survey_zeros_are_water_but_an_all_zero_survey_is_refused() {
        // two survey nodes: one on land at 0 m (water per the survey), one snapped
        let nodes = [(3904, 3624), (3904, 3632)];
        let text = serialize_group(&probe_group(SURVEY_NAME, &nodes));
        let unsnapped = parse_group_file(&text).unwrap();
        let mut store = HeightStore::new(KOREA_MAP_ID);
        let rep = ingest(&unsnapped, &mut store, false);
        assert!(rep.looks_unsnapped() && !rep.merged, "no probe above 0 m");
        let snapped =
            parse_group_file(&text.replacen("YPos = 0.000", "YPos = 351.200", 1)).unwrap();
        let rep = ingest(&snapped, &mut store, false);
        assert!(rep.merged);
        assert_eq!((rep.above_zero, rep.water, rep.unsnapped), (1, 1, 0));
        assert_eq!(store.node(3904, 3624), Some(351.2));
        assert_eq!(store.node(3904, 3632), Some(0.0));
    }

    #[test]
    fn ingest_learns_snapped_units_and_legacy_points() {
        let extra = "  Vehicle\r\n  {\r\n    Name = \"Tank\";\r\n    XPos = 390123.500;\r\n    YPos = 211.700;\r\n    ZPos = 362456.250;\r\n    PinToTerrain = 1;\r\n  }\r\n\r\n  \r\n\
                     \x20 Vehicle\r\n  {\r\n    Name = \"HGB_001_001\";\r\n    XPos = 393550.000;\r\n    YPos = 640.500;\r\n    ZPos = 375250.000;\r\n  }\r\n\r\n  \r\n\
                     \x20 Plane\r\n  {\r\n    Name = \"Flying\";\r\n    XPos = 390000.000;\r\n    YPos = 1500.000;\r\n    ZPos = 360000.000;\r\n    PinToTerrain = 1;\r\n  }\r\n\r\n  \r\n";
        let root = snapped(&[("H3905_3628", "412.000")], extra);
        let mut store = HeightStore::new(KOREA_MAP_ID);
        let off = ingest(&root, &mut store, false);
        assert_eq!((off.learned, off.probe_points), (0, 1));
        let mut store = HeightStore::new(KOREA_MAP_ID);
        let on = ingest(&root, &mut store, true);
        assert_eq!((on.learned, on.probe_points, on.lattice), (1, 1, 1));
        assert_eq!(store.height_at(390_123.5, 362_456.25), Some(211.7));
        assert_eq!(store.points().len(), 2, "planes are never learned");
    }
}

#[cfg(test)]
mod real_files {
    use super::*;
    use crate::terrain::KOREA_MAP_ID;

    /// `IL2_SNAPPED="a.Group;b.Group" cargo test --offline real_snapped -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_snapped_files_ingest() {
        let list = std::env::var("IL2_SNAPPED").expect("set IL2_SNAPPED");
        let mut store = HeightStore::new(KOREA_MAP_ID);
        for path in list.split(';') {
            let text = std::fs::read_to_string(path).unwrap();
            let root = parse_il2_document(&text).unwrap();
            let rep = ingest(&root, &mut store, false);
            println!(
                "{path}: lattice {} points {} sea {} unsnapped {} tiles {}",
                rep.lattice,
                rep.probe_points,
                rep.water,
                rep.unsnapped,
                rep.tiles.len()
            );
        }
        println!(
            "measured nodes {}, points {}",
            store.measured_nodes(),
            store.points().len()
        );
        println!(
            "height at 392,000 / 376,000: {:?}",
            store.height_at(392_000.0, 376_000.0)
        );
    }
}
