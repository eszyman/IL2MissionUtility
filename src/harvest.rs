//! harvest.rs — Task Editor `_gen.mission` → airfield database
//!
//! Freeflight writes the whole generated mission to `data/Missions/_gen.mission`
//! whenever you start a flight from an airfield. This module cuts the start
//! airfield out of that file (no mission editor step), cleans it with
//! `airfield::clean_airfield`, and files it into a database folder:
//!
//! ```text
//! <db>/raw/<utc stamp>_gen.Mission   untouched copy + language files
//! <db>/<Airfield>_<country>.Group     cleaned airfield + language files
//! <db>/catalog.Group                  one AirfieldRecord per field/country
//! <db>/models.tsv                     every Model/Script the game used (+ raw file)
//! ```
//!
//! ## Cutting one airfield out of a mission
//! 1. Every indexed object within `radius_m` of the `Airfield` block is
//!    kept, unless another `Airfield` is closer (nearest-field split).
//! 2. Logic reachable from those objects through links (`Targets`,
//!    `Objects`, `OnEvents`, …) is pulled in up to `link_reach_m` away:
//!    K13's approach icons sit 13–15 km out. World objects (anything with a
//!    `Model`) and entities are never pulled in by links.
//! 3. Links to anything left behind are scrubbed.
//! 4. The player plane and its SP logic go (`clean_airfield`); remaining AI
//!    planes go too unless `keep_ai_planes` (`strip_ai_planes`).
//!
//! The raw file is archived before parsing, so a parse failure loses
//! nothing and later extractors can mine the archive.
//!
//! ## Runway axis
//! A taxi node with `Runway = 0` is on the runway. On the K13–K15 exports
//! those are the Type 1 centerline points. A runway node that also has
//! `RunwayEnd = 0` is a threshold; the other runway nodes omit `RunwayEnd`.
//! `AxisHeading` / `AxisLength` are still the principal axis of every
//! `MCU_TR_TaxiGraph` node (grid north, 0 = +X), so the figure follows that
//! runway and also takes in the taxiways. Older maps keep an
//! `Airfield { Chart { Point } }` in airfield-local coordinates instead;
//! those points are counted in `TaxiNodes` but give no axis.
//!
//! ## Public API
//! * `GEN_FILE`, `DEFAULT_RADIUS_M`, `DEFAULT_LINK_REACH_M`, `DEFAULT_DB_DIR`
//! * `struct HarvestConfig`, `struct AirfieldRecord`, `struct HarvestedAirfield`,
//!   `struct HarvestOutcome`
//! * `fn default_missions_dir` / `fn find_gen_file`
//! * `fn harvest_root` — cut + clean airfields from a parsed mission (pure)
//! * `fn harvest_file` — archive, harvest, write group/catalog/models (I/O)
//! * `fn place_field_spawn` — move the fakefield to the hold-short and write planes
//! * `struct GenWatcher` — polls `_gen.mission` and reports stable rewrites
//!
//! ## Used by
//! * ui.rs (Airfield) — watcher toggle, Harvest now, Harvest a file, field-spawn export

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::airstart::{self, AirStartPlane};
use crate::airfield::{
    clean_airfield, node_link_ids, scrub_deleted, strip_ai_planes, CleanReport,
    EASTERN_PLANE_COALITIONS, WESTERN_PLANE_COALITIONS,
};
use crate::ast::Il2Entity;
use crate::locale::{merge_template_sidecars, write_sidecars, LocaleTable};
use crate::parser::{parse_group_file, parse_il2_document};
use crate::serialize::serialize_group;

pub const GEN_FILE: &str = "_gen.mission";
pub const DEFAULT_RADIUS_M: f64 = 4000.0;
pub const DEFAULT_LINK_REACH_M: f64 = 20000.0;
/// Korea install. The Airfield tab starts here; the folder fields can still be changed.
pub const DEFAULT_MISSIONS_DIR: &str = r"C:\Program Files\IL2Series\game\data\Missions";
pub const DEFAULT_DB_DIR: &str = r"C:\Program Files\IL2Series\game\data\Template\MP Airfields";

const CATALOG_FILE: &str = "catalog.Group";
const MODELS_FILE: &str = "models.tsv";
const RAW_DIR: &str = "raw";
const RECORD_BLOCK: &str = "AirfieldRecord";
/// `_gen.mission` must stop changing for this long before it is read.
const STABLE_FOR: Duration = Duration::from_millis(1500);
/// Sidecars archived with the raw mission (`pol` is not in `LANG_EXTS`).
const RAW_SIDECAR_EXTS: &[&str] = &["eng", "chs", "fra", "ger", "rus", "spa", "pol"];

const STEAM_MISSIONS: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\IL-2 Sturmovik Battle of Stalingrad\data\Missions";
const STANDALONE_MISSIONS: &str =
    r"C:\Program Files\IL-2 Sturmovik Great Battles\data\Missions";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HarvestConfig {
    pub radius_m: f64,
    pub link_reach_m: f64,
    pub keep_ai_planes: bool,
}

impl Default for HarvestConfig {
    fn default() -> Self {
        Self {
            radius_m: DEFAULT_RADIUS_M,
            link_reach_m: DEFAULT_LINK_REACH_M,
            keep_ai_planes: false,
        }
    }
}

/// One catalog row. Written as an `AirfieldRecord` block in `catalog.Group`.
#[derive(Debug, Clone, PartialEq)]
pub struct AirfieldRecord {
    pub name: String,
    pub file: String,
    pub country: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Farthest kept world object from the airfield block (m).
    pub footprint_m: f64,
    pub taxi_nodes: usize,
    pub axis_heading_deg: Option<f64>,
    pub axis_length_m: Option<f64>,
    pub vehicles: usize,
    pub blocks: usize,
    pub logic: usize,
    pub map: String,
    pub date: String,
    pub source: String,
    pub captured: String,
}

#[derive(Debug, Clone)]
pub struct HarvestedAirfield {
    pub group: Il2Entity,
    pub record: AirfieldRecord,
    pub cleaned: CleanReport,
    pub ai_planes_removed: usize,
    /// Logic nodes kept only because a link reached them from inside the radius.
    pub via_links: usize,
}

#[derive(Debug, Clone)]
pub struct HarvestOutcome {
    pub archived: PathBuf,
    pub airfields: Vec<HarvestedAirfield>,
    pub models_added: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct MissionHeader {
    map: String,
    date: String,
}

#[derive(Debug, Clone)]
struct Site {
    index: i32,
    name: String,
    country: i32,
    x: f64,
    y: f64,
    z: f64,
}

pub fn default_missions_dir() -> Option<PathBuf> {
    [DEFAULT_MISSIONS_DIR, STEAM_MISSIONS, STANDALONE_MISSIONS]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
}

/// `_gen.mission` in `dir`, matched case-insensitively.
pub fn find_gen_file(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(GEN_FILE))
        })
}

/// Parse `.Mission` text. `#` comment lines and the `Options` header are
/// dropped: `WindLayers` rows (`0 : 143 : 1.5;`) are not `.Group` syntax.
/// The header's `GuiMap` and `Date` are returned for the catalog.
fn parse_mission_with_header(text: &str) -> Result<(Il2Entity, MissionHeader), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let body: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\r\n");
    let (body, options) = cut_top_level_block(&body, "Options");
    let header = options.map(|o| read_header(&o)).unwrap_or_default();
    let root = parse_il2_document(&body).map_err(|err| format!("mission parse failed: {err}"))?;
    Ok((root, header))
}

/// Remove the first top-level `name { … }` block. Returns the rest and the block text.
fn cut_top_level_block(text: &str, name: &str) -> (String, Option<String>) {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_quote = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if in_quote {
            in_quote = c != b'"';
            i += 1;
            continue;
        }
        match c {
            b'"' => in_quote = true,
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && bytes[i..].starts_with(name.as_bytes()) => {
                let at_word = i == 0 || !is_ident_byte(bytes[i - 1]);
                let after = &text[i + name.len()..];
                let ends_word = after.bytes().next().is_none_or(|b| !is_ident_byte(b));
                if at_word && ends_word && after.trim_start().starts_with('{') {
                    if let Some(end) = matching_brace(text, i + name.len()) {
                        let block = text[i..=end].to_string();
                        let rest = format!("{}{}", &text[..i], &text[end + 1..]);
                        return (rest, Some(block));
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    (text.to_string(), None)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Index of the `}` closing the first `{` at or after `from`.
fn matching_brace(text: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_quote = false;
    for (off, c) in text[from..].bytes().enumerate() {
        if in_quote {
            in_quote = c != b'"';
            continue;
        }
        match c {
            b'"' => in_quote = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(from + off);
                }
            }
            _ => {}
        }
    }
    None
}

fn read_header(options: &str) -> MissionHeader {
    let mut header = MissionHeader::default();
    for line in options.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(';').trim().trim_matches('"');
        match key.trim() {
            "GuiMap" => header.map = value.to_string(),
            "Date" => header.date = value.to_string(),
            _ => {}
        }
    }
    header
}

/// Cut and clean airfields from a parsed mission. With a player plane only
/// the start field (nearest the player) is taken; otherwise every field.
pub fn harvest_root(root: &Il2Entity, cfg: &HarvestConfig) -> Vec<HarvestedAirfield> {
    let sites = airfield_sites(root);
    let wanted: Vec<&Site> = match player_xz(root) {
        Some((px, pz)) => sites
            .iter()
            .min_by(|a, b| dist2(a.x, a.z, px, pz).total_cmp(&dist2(b.x, b.z, px, pz)))
            .into_iter()
            .collect(),
        None => sites.iter().collect(),
    };
    wanted
        .into_iter()
        .map(|site| harvest_site(root, site, &sites, cfg))
        .collect()
}

fn harvest_site(
    root: &Il2Entity,
    site: &Site,
    sites: &[Site],
    cfg: &HarvestConfig,
) -> HarvestedAirfield {
    let (mut group, via_links) = extract_site(root, site, sites, cfg);
    let coalitions = if site.country / 100 == 5 {
        EASTERN_PLANE_COALITIONS
    } else {
        WESTERN_PLANE_COALITIONS
    };
    let cleaned = clean_airfield(&mut group, coalitions).unwrap_or(CleanReport {
        stripped: 0,
        unlinked_checkzones: 0,
        plane_coalitions: coalitions.to_string(),
    });
    let mut ai_planes_removed = 0;
    if !cfg.keep_ai_planes {
        ai_planes_removed = group.count_block_type("Plane");
        if ai_planes_removed > 0 {
            let _ = strip_ai_planes(&mut group, coalitions);
        }
    }
    let record = describe(&group, site);
    HarvestedAirfield {
        group,
        record,
        cleaned,
        ai_planes_removed,
        via_links,
    }
}

fn airfield_sites(root: &Il2Entity) -> Vec<Site> {
    let mut sites = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type != "Airfield" {
            return;
        }
        let (Some(index), Some((x, z))) = (e.index, e.pos_xz()) else {
            return;
        };
        sites.push(Site {
            index,
            name: e.name().unwrap_or("Airfield").to_string(),
            country: int_prop(e, "Country").unwrap_or(0),
            x,
            y: float_prop(e, "YPos").unwrap_or(0.0),
            z,
        });
    });
    sites
}

fn player_xz(root: &Il2Entity) -> Option<(f64, f64)> {
    let mut found = None;
    root.for_each(&mut |e| {
        if found.is_none() && e.block_type == "Plane" && e.property("AILevel") == Some("0") {
            found = e.pos_xz();
        }
    });
    found
}

/// Clone the site's objects into a `Group` named after the airfield.
fn extract_site(
    root: &Il2Entity,
    site: &Site,
    sites: &[Site],
    cfg: &HarvestConfig,
) -> (Il2Entity, usize) {
    let by_index = index_map(root);
    let in_zone = |x: f64, z: f64| {
        let d = dist2(x, z, site.x, site.z);
        d <= cfg.radius_m * cfg.radius_m
            && sites
                .iter()
                .filter(|s| s.index != site.index)
                .all(|s| d <= dist2(x, z, s.x, s.z))
    };

    let mut keep: HashSet<i32> = HashSet::new();
    for (&id, e) in &by_index {
        if e.block_type == "Group" || (e.block_type == "Airfield" && id != site.index) {
            continue;
        }
        if e.pos_xz().is_some_and(|(x, z)| in_zone(x, z)) {
            keep.insert(id);
        }
    }

    let mut via_links = 0usize;
    let reach2 = cfg.link_reach_m * cfg.link_reach_m;
    let mut queue: Vec<i32> = keep.iter().copied().collect();
    while let Some(id) = queue.pop() {
        let Some(node) = by_index.get(&id) else {
            continue;
        };
        for next in node_link_ids(node) {
            if keep.contains(&next) {
                continue;
            }
            let Some(n) = by_index.get(&next) else {
                continue;
            };
            if n.block_type == "Group" || is_world_object(n) || n.block_type == "MCU_TR_Entity" {
                continue;
            }
            if n
                .pos_xz()
                .is_some_and(|(x, z)| dist2(x, z, site.x, site.z) > reach2)
            {
                continue;
            }
            keep.insert(next);
            queue.push(next);
            via_links += 1;
        }
    }

    // Entities follow their object: keep the kept objects' entities, drop orphans.
    let world_kept: Vec<i32> = keep
        .iter()
        .copied()
        .filter(|id| by_index.get(id).is_some_and(|e| is_world_object(e)))
        .collect();
    for id in world_kept {
        if let Some(link) = by_index.get(&id).and_then(|e| int_prop(e, "LinkTrId")) {
            if link > 0 && by_index.contains_key(&link) {
                keep.insert(link);
            }
        }
    }
    keep.retain(|id| {
        let Some(e) = by_index.get(id) else {
            return false;
        };
        if e.block_type != "MCU_TR_Entity" {
            return true;
        }
        match int_prop(e, "MisObjID") {
            Some(owner) if owner > 0 => keep_contains_owner(&by_index, owner, site, &in_zone),
            _ => true,
        }
    });

    let mut group = Il2Entity::new("Group");
    group.set_name(&site.name);
    group.set_property("Desc", "\"\"");
    group.children = filter_children(&root.children, &keep, &|x, z| in_zone(x, z));

    let dropped: HashSet<i32> = by_index.keys().copied().filter(|id| !keep.contains(id)).collect();
    scrub_deleted(&mut group, &dropped);
    (group, via_links)
}

/// An entity survives when its owner is inside the zone (owners are never
/// pulled in by links, so the zone test is the whole rule).
fn keep_contains_owner(
    by_index: &HashMap<i32, &Il2Entity>,
    owner: i32,
    site: &Site,
    in_zone: &dyn Fn(f64, f64) -> bool,
) -> bool {
    let Some(obj) = by_index.get(&owner) else {
        return false;
    };
    if obj.block_type == "Airfield" {
        return obj.index == Some(site.index);
    }
    obj.pos_xz().is_some_and(|(x, z)| in_zone(x, z))
}

fn filter_children(
    children: &[Il2Entity],
    keep: &HashSet<i32>,
    in_zone: &dyn Fn(f64, f64) -> bool,
) -> Vec<Il2Entity> {
    let mut out = Vec::new();
    for child in children {
        if child.block_type == "Group" {
            let inner = filter_children(&child.children, keep, in_zone);
            if !inner.is_empty() {
                let mut g = child.clone();
                g.children = inner;
                out.push(g);
            }
        } else if let Some(id) = child.index {
            if keep.contains(&id) {
                out.push(child.clone());
            }
        } else if child.pos_xz().is_some_and(|(x, z)| in_zone(x, z)) {
            out.push(child.clone());
        }
    }
    out
}

fn is_world_object(e: &Il2Entity) -> bool {
    e.property("Model").is_some()
}

fn index_map(root: &Il2Entity) -> HashMap<i32, &Il2Entity> {
    let mut map = HashMap::new();
    fill_index_map(root, &mut map);
    map
}

fn fill_index_map<'a>(e: &'a Il2Entity, map: &mut HashMap<i32, &'a Il2Entity>) {
    if let Some(id) = e.index {
        map.entry(id).or_insert(e);
    }
    for child in &e.children {
        fill_index_map(child, map);
    }
}

fn describe(group: &Il2Entity, site: &Site) -> AirfieldRecord {
    let mut footprint2: f64 = 0.0;
    let (mut vehicles, mut blocks, mut logic) = (0usize, 0usize, 0usize);
    let mut nodes: Vec<(f64, f64)> = Vec::new();
    let mut chart_points = 0usize;
    group.for_each(&mut |e| {
        match e.block_type.as_str() {
            "Vehicle" | "Ship" => vehicles += 1,
            "Block" | "Ground" | "Bridge" => blocks += 1,
            t if t.starts_with("MCU_") => logic += 1,
            _ => {}
        }
        if is_world_object(e) {
            if let Some((x, z)) = e.pos_xz() {
                footprint2 = footprint2.max(dist2(x, z, site.x, site.z));
            }
        }
        if e.block_type == "MCU_TR_TaxiGraph" {
            collect_taxi_nodes(e, &mut nodes);
        }
        if e.block_type == "Airfield" {
            chart_points += e
                .children
                .iter()
                .filter(|c| c.block_type == "Chart")
                .map(|c| c.count_block_type("Point"))
                .sum::<usize>();
        }
    });
    let axis = principal_axis(&nodes);
    AirfieldRecord {
        name: site.name.clone(),
        file: airfield_file_name(&site.name, site.country),
        country: site.country,
        x: site.x,
        y: site.y,
        z: site.z,
        footprint_m: footprint2.sqrt(),
        taxi_nodes: nodes.len() + chart_points,
        axis_heading_deg: axis.map(|a| a.0),
        axis_length_m: axis.map(|a| a.1),
        vehicles,
        blocks,
        logic,
        map: String::new(),
        date: String::new(),
        source: String::new(),
        captured: String::new(),
    }
}

fn collect_taxi_nodes(e: &Il2Entity, out: &mut Vec<(f64, f64)>) {
    for child in &e.children {
        if child.block_type == "Node" {
            if let (Some(x), Some(z)) = (float_prop(child, "X"), float_prop(child, "Z")) {
                out.push((x, z));
            }
        }
        collect_taxi_nodes(child, out);
    }
}

/// Principal axis of `points`: grid heading in [0, 180) (0 = +X, 90 = +Z)
/// and the length of the points projected onto it.
fn principal_axis(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mz = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut sxx, mut szz, mut sxz) = (0.0, 0.0, 0.0);
    for (x, z) in points {
        let (dx, dz) = (x - mx, z - mz);
        sxx += dx * dx;
        szz += dz * dz;
        sxz += dx * dz;
    }
    let theta = 0.5 * (2.0 * sxz).atan2(sxx - szz);
    let (c, s) = (theta.cos(), theta.sin());
    let proj = points.iter().map(|(x, z)| (x - mx) * c + (z - mz) * s);
    let (lo, hi) = proj.fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    let heading = theta.to_degrees().rem_euclid(180.0);
    Some((heading, hi - lo))
}

pub fn airfield_file_name(name: &str, country: i32) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}_{country}.Group")
}

/// 80 m runway-spawn line, centered on the fakefield. Confirmed in
/// `Graphics1.gtp` (`/graphics/airfields/fakefield_rnwspawn.mgm`).
const RUNWAY_SPAWN_MODEL: &str = r"graphics\airfields\fakefield_rnwspawn.mgm";
/// Matching script. Confirmed in `Scripts.gtp` and in
/// `Pa38_Road_to_Manchuria_11.Mission`.
const RUNWAY_SPAWN_SCRIPT: &str = r"LuaScripts\WorldObjects\Airfields\fakefield_rnwspawn.txt";

/// IL-2 editor Start Type, from the integer switch in `IL2Editor.exe`:
/// 0 In Air, 1 Engine On, 2 Engine Off, 3 Engine Cold.
const START_ENGINE_ON: i32 = 1;
const START_ENGINE_OFF: i32 = 2;

/// IL-2 editor Snap To, from the integer switch in `IL2Editor.exe`:
/// 0 None, 1 Runway, 2 Parking. Parking is 2. Shipped missions that write
/// `SnapTo = 1` are snapping to the runway, not to a parking spot.
const SNAP_PARKING: i32 = 2;
const SNAP_RUNWAY: i32 = 1;
/// Engine Cold. The editor's fourth Start Type. Not written by the default
/// field-spawn choices; the plane name still recognizes it.
const START_ENGINE_COLD: i32 = 3;
/// Fuel at or above this is a long-range load. The field-spawn fuel slider
/// stores a percent, so 0.95 is 95% and 1.0 is full.
const LONG_RANGE_FUEL: f64 = 0.95;

/// Where `place_field_spawn` put the fakefield.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnSpot {
    pub x: f64,
    pub z: f64,
    pub heading_deg: f64,
}

/// A few metres either side of the threshold still counts as just short of it.
/// Farther onto the runway is down the strip. Farther the other way is the overrun.
const THRESHOLD_SLACK_M: f64 = 12.0;

/// Move the fakefield and its entity, and write `planes` onto the field.
///
/// `takeoff_heading` is the into-wind heading in degrees (0 = north, `X`
/// north, `Z` east). Pass it when the mission wind is known. `None` uses
/// the base end: the `RunwayEnd` threshold closer to the buildings.
///
/// The fakefield stays on the taxi node just short of that threshold, off
/// the centerline, facing the runway. It is not slid along the ramp and it
/// is not moved onto a Type 3 pad. Engine running (`parking_snap` false)
/// writes `StartType` 1 and `SnapTo` 0 on the 80 m runway-spawn model.
/// Engine off, parking writes `StartType` 2 and `SnapTo` 2 and leaves the
/// field on that same hold-short. A field with no Type 3 nodes still
/// exports. `country`, when set, is written on the airfield (NATO 601 / DPRK 502).
pub fn place_field_spawn(
    group: &mut Il2Entity,
    planes: &[AirStartPlane],
    takeoff_heading: Option<f64>,
    country: Option<i32>,
) -> Result<SpawnSpot, String> {
    if planes.is_empty() {
        return Err("Add an aircraft before placing the field.".into());
    }
    let air = find_one_airfield(group)?;
    let nodes = taxi_nodes_for(group, air.entity_id)?;
    let ends = threshold_indexes(&nodes)?;
    let departure = departure_index(group, air.index, &nodes, ends, takeoff_heading)?;
    let far = 1 - departure;
    let depart = &nodes[ends[departure]];
    let far_node = &nodes[ends[far]];
    let spot_i = hold_short(&nodes, depart, far_node)?;
    let spot = SpawnSpot {
        x: nodes[spot_i].x,
        z: nodes[spot_i].z,
        heading_deg: heading_toward_runway(&nodes[spot_i], depart, far_node),
    };
    write_spawn(group, air.index, air.entity_id, &spot, planes, country)?;
    Ok(spot)
}

struct AirRef {
    index: i32,
    entity_id: i32,
}

struct TaxiNode {
    x: f64,
    z: f64,
    runway: bool,
    end: bool,
}

fn find_one_airfield(group: &Il2Entity) -> Result<AirRef, String> {
    let mut found: Vec<AirRef> = Vec::new();
    group.for_each(&mut |e| {
        if e.block_type != "Airfield" {
            return;
        }
        let Some(index) = e.index else { return };
        let Some(entity_id) = int_prop(e, "LinkTrId") else { return };
        found.push(AirRef { index, entity_id });
    });
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err("This file has no airfield.".into()),
        _ => Err("This file has more than one airfield.".into()),
    }
}

fn taxi_nodes_for(group: &Il2Entity, entity_id: i32) -> Result<Vec<TaxiNode>, String> {
    let mut graphs = Vec::new();
    collect_graphs(group, &mut graphs);
    let graph = graphs
        .iter()
        .find(|g| g.objects.contains(&entity_id))
        .copied()
        .or_else(|| (graphs.len() == 1).then(|| graphs[0]));
    let Some(graph) = graph else {
        return Err("The airfield has no taxi graph.".into());
    };
    let mut nodes = Vec::new();
    collect_spawn_nodes(graph, &mut nodes);
    if nodes.is_empty() {
        return Err("The taxi graph has no nodes.".into());
    }
    Ok(nodes)
}

fn collect_graphs<'a>(e: &'a Il2Entity, out: &mut Vec<&'a Il2Entity>) {
    if e.block_type == "MCU_TR_TaxiGraph" {
        out.push(e);
    }
    for child in &e.children {
        collect_graphs(child, out);
    }
}

fn collect_spawn_nodes(e: &Il2Entity, out: &mut Vec<TaxiNode>) {
    for child in &e.children {
        if child.block_type == "Node" {
            if let (Some(x), Some(z)) = (float_prop(child, "X"), float_prop(child, "Z")) {
                let mut runway = false;
                let mut end = false;
                for data in &child.children {
                    if data.block_type != "Data" {
                        continue;
                    }
                    runway |= int_prop(data, "Runway") == Some(0);
                    end |= int_prop(data, "RunwayEnd") == Some(0);
                }
                out.push(TaxiNode { x, z, runway, end });
            }
        } else {
            collect_spawn_nodes(child, out);
        }
    }
}

fn threshold_indexes(nodes: &[TaxiNode]) -> Result<[usize; 2], String> {
    let ends: Vec<usize> = nodes.iter().enumerate().filter(|(_, n)| n.end).map(|(i, _)| i).collect();
    if ends.len() != 2 {
        return Err(format!(
            "This airfield has {} runway ends; a spawn needs two.",
            ends.len()
        ));
    }
    Ok([ends[0], ends[1]])
}

fn departure_index(
    group: &Il2Entity,
    air_index: i32,
    nodes: &[TaxiNode],
    ends: [usize; 2],
    takeoff_heading: Option<f64>,
) -> Result<usize, String> {
    let headings = [
        heading_between(&nodes[ends[0]], &nodes[ends[1]]),
        heading_between(&nodes[ends[1]], &nodes[ends[0]]),
    ];
    if let Some(want) = takeoff_heading {
        let d0 = heading_delta(headings[0], want);
        let d1 = heading_delta(headings[1], want);
        if (d0 - d1).abs() > 1e-6 {
            return Ok(if d0 < d1 { 0 } else { 1 });
        }
    }
    let (bx, bz) = building_centroid(group, air_index)?;
    let d0 = dist2(nodes[ends[0]].x, nodes[ends[0]].z, bx, bz);
    let d1 = dist2(nodes[ends[1]].x, nodes[ends[1]].z, bx, bz);
    Ok(if d0 <= d1 { 0 } else { 1 })
}

fn building_centroid(group: &Il2Entity, skip_airfield: i32) -> Result<(f64, f64), String> {
    let mut n = 0.0;
    let mut sx = 0.0;
    let mut sz = 0.0;
    group.for_each(&mut |e| {
        if e.index == Some(skip_airfield) {
            return;
        }
        if e.block_type != "Block" && e.block_type != "Ground" {
            return;
        }
        if let Some((x, z)) = e.pos_xz() {
            sx += x;
            sz += z;
            n += 1.0;
        }
    });
    if n == 0.0 {
        return Err("No buildings to choose a runway end.".into());
    }
    Ok((sx / n, sz / n))
}

fn hold_short(nodes: &[TaxiNode], departure: &TaxiNode, far: &TaxiNode) -> Result<usize, String> {
    let (ux, uz) = unit_toward(departure, far)?;
    let mut best: Option<(f64, usize)> = None;
    for (i, node) in nodes.iter().enumerate() {
        if node.runway {
            continue;
        }
        let along = (node.x - departure.x) * ux + (node.z - departure.z) * uz;
        if along.abs() > THRESHOLD_SLACK_M {
            continue;
        }
        let d = dist2(node.x, node.z, departure.x, departure.z);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i).ok_or_else(|| {
        "No taxi node sits just short of the runway threshold.".into()
    })
}

/// Heading from `spawn` toward the nearest point on the runway segment.
/// That is perpendicular to the centerline: the nose points at the strip,
/// not down it. X is north, Z is east, 0 faces north.
fn heading_toward_runway(spawn: &TaxiNode, departure: &TaxiNode, far: &TaxiNode) -> f64 {
    let Ok((ux, uz)) = unit_toward(departure, far) else {
        return 0.0;
    };
    let along = (spawn.x - departure.x) * ux + (spawn.z - departure.z) * uz;
    let len = ((far.x - departure.x).powi(2) + (far.z - departure.z).powi(2)).sqrt();
    let t = along.clamp(0.0, len);
    let cx = departure.x + ux * t;
    let cz = departure.z + uz * t;
    let dx = cx - spawn.x;
    let dz = cz - spawn.z;
    if dx * dx + dz * dz < 0.25 {
        return heading_between(departure, far);
    }
    dz.atan2(dx).to_degrees().rem_euclid(360.0)
}

fn unit_toward(from: &TaxiNode, to: &TaxiNode) -> Result<(f64, f64), String> {
    let dx = to.x - from.x;
    let dz = to.z - from.z;
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1.0 {
        return Err("The runway ends are in the same place.".into());
    }
    Ok((dx / len, dz / len))
}

fn heading_between(from: &TaxiNode, to: &TaxiNode) -> f64 {
    (to.z - from.z).atan2(to.x - from.x).to_degrees().rem_euclid(360.0)
}

fn heading_delta(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn write_spawn(
    group: &mut Il2Entity,
    air_index: i32,
    entity_id: i32,
    spot: &SpawnSpot,
    planes: &[AirStartPlane],
    country: Option<i32>,
) -> Result<(), String> {
    let x = format!("{:.3}", spot.x);
    let z = format!("{:.3}", spot.z);
    let heading = format!("{:.3}", spot.heading_deg.rem_euclid(360.0));
    let mut wrote_air = false;
    let mut wrote_entity = false;
    group.for_each_mut(&mut |e| {
        let is_air = e.block_type == "Airfield" && e.index == Some(air_index);
        let is_entity = e.block_type == "MCU_TR_Entity" && e.index == Some(entity_id);
        if !is_air && !is_entity {
            return;
        }
        e.set_property("XPos", x.clone());
        e.set_property("ZPos", z.clone());
        e.set_property("YOri", heading.clone());
        if is_air {
            if let Some(country) = country {
                e.set_property("Country", country.to_string());
            }
            wrote_air = true;
            e.set_property("Model", format!("\"{RUNWAY_SPAWN_MODEL}\""));
            e.set_property("Script", format!("\"{RUNWAY_SPAWN_SCRIPT}\""));
            e.children.retain(|c| c.block_type != "Planes");
            let mut list = Il2Entity::new("Planes");
            for (i, plane) in planes.iter().enumerate() {
                let (start_type, snap) = if plane.parking_snap {
                    (START_ENGINE_OFF, SNAP_PARKING)
                } else {
                    (START_ENGINE_ON, 0)
                };
                let mut entry = airstart::plane_entry(plane, 36 + i as i32, start_type, 0, snap);
                let name = field_spawn_name(start_type, snap, plane.payload_id, plane.fuel);
                entry.set_property("Name", format!("\"{name}\""));
                list.children.push(entry);
            }
            e.children.push(list);
        }
        if is_entity {
            wrote_entity = true;
        }
    });
    if !wrote_air || !wrote_entity {
        return Err("The airfield entity link is broken.".into());
    }
    Ok(())
}

/// Plane `Name` Erik reads in the editor. Snap To wins over Start Type.
/// Load follows: payload 0 is Clean, payload N is `Strike N`, and fuel at or
/// above [`LONG_RANGE_FUEL`] replaces Clean or is appended after the strike.
fn field_spawn_name(start_type: i32, snap_to: i32, payload_id: i32, fuel: f64) -> String {
    let condition = if snap_to == SNAP_PARKING {
        "Parked"
    } else if snap_to == SNAP_RUNWAY {
        "On runway"
    } else if start_type == START_ENGINE_OFF || start_type == START_ENGINE_COLD {
        "Parked"
    } else if start_type == START_ENGINE_ON {
        "At ramp"
    } else {
        "In air"
    };
    let long_range = fuel >= LONG_RANGE_FUEL;
    let load = if payload_id > 0 && long_range {
        format!("Strike {payload_id} - Long range")
    } else if payload_id > 0 {
        format!("Strike {payload_id}")
    } else if long_range {
        "Long range".to_string()
    } else {
        "Clean".to_string()
    };
    format!("{condition} - {load}")
}

/// Archive `source`, harvest it, and write groups, catalog and model list into `db`.
pub fn harvest_file(
    source: &Path,
    db: &Path,
    cfg: &HarvestConfig,
) -> Result<HarvestOutcome, String> {
    let bytes = std::fs::read(source).map_err(|err| format!("{}: {err}", source.display()))?;
    let stamp = utc_stamp(SystemTime::now());
    let archived = archive_raw(source, &bytes, db, &stamp.file)?;
    let text = String::from_utf8_lossy(&bytes);
    let (root, header) = parse_mission_with_header(&text)?;

    let mut airfields = harvest_root(&root, cfg);
    if airfields.is_empty() {
        return Err(format!(
            "no Airfield block in {} (raw copy kept at {})",
            source.display(),
            archived.display()
        ));
    }
    let source_rel = format!(
        "{RAW_DIR}/{}",
        archived.file_name().and_then(|n| n.to_str()).unwrap_or("")
    );
    let tables = merge_template_sidecars(&[source.to_path_buf()]);
    for af in &mut airfields {
        af.record.map = header.map.clone();
        af.record.date = header.date.clone();
        af.record.source = source_rel.clone();
        af.record.captured = stamp.display.clone();
        let path = db.join(&af.record.file);
        std::fs::write(&path, serialize_group(&af.group))
            .map_err(|err| format!("{}: {err}", path.display()))?;
        write_sidecars(&path, &used_tables(&af.group, &tables))?;
    }
    let records: Vec<AirfieldRecord> = airfields.iter().map(|a| a.record.clone()).collect();
    upsert_catalog(&db.join(CATALOG_FILE), &records)?;
    let raw_name = archived.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let models_added = append_models(&db.join(MODELS_FILE), &root, raw_name)?;
    Ok(HarvestOutcome {
        archived,
        airfields,
        models_added,
    })
}

fn archive_raw(source: &Path, bytes: &[u8], db: &Path, stamp: &str) -> Result<PathBuf, String> {
    let raw_dir = db.join(RAW_DIR);
    std::fs::create_dir_all(&raw_dir).map_err(|err| format!("{}: {err}", raw_dir.display()))?;
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("mission");
    let dest = raw_dir.join(format!("{stamp}{stem}.Mission"));
    std::fs::write(&dest, bytes).map_err(|err| format!("{}: {err}", dest.display()))?;
    for ext in RAW_SIDECAR_EXTS {
        let side = source.with_extension(ext);
        if side.is_file() {
            let _ = std::fs::copy(&side, dest.with_extension(ext));
        }
    }
    Ok(dest)
}

/// Language tables trimmed to the LC ids the harvested group uses.
fn used_tables(
    group: &Il2Entity,
    tables: &HashMap<String, LocaleTable>,
) -> HashMap<String, LocaleTable> {
    let mut ids = HashSet::new();
    group.for_each(&mut |e| {
        for (k, v) in &e.properties {
            if k.starts_with("LC") {
                if let Ok(id) = v.parse::<i32>() {
                    ids.insert(id);
                }
            }
        }
    });
    tables
        .iter()
        .map(|(ext, table)| {
            let mut out = LocaleTable::default();
            for &id in &ids {
                if let Some(text) = table.get(id) {
                    out.insert(id, text);
                }
            }
            (ext.clone(), out)
        })
        .collect()
}

fn record_entity(r: &AirfieldRecord) -> Il2Entity {
    let mut e = Il2Entity::new(RECORD_BLOCK);
    let q = |s: &str| format!("\"{}\"", s.replace('"', "'"));
    e.set_property("Name", q(&r.name));
    e.set_property("File", q(&r.file));
    e.set_property("Country", r.country.to_string());
    e.set_property("XPos", format!("{:.3}", r.x));
    e.set_property("YPos", format!("{:.3}", r.y));
    e.set_property("ZPos", format!("{:.3}", r.z));
    e.set_property("Footprint", format!("{:.0}", r.footprint_m));
    e.set_property("TaxiNodes", r.taxi_nodes.to_string());
    if let (Some(h), Some(l)) = (r.axis_heading_deg, r.axis_length_m) {
        e.set_property("AxisHeading", format!("{h:.1}"));
        e.set_property("AxisLength", format!("{l:.0}"));
    }
    e.set_property("Vehicles", r.vehicles.to_string());
    e.set_property("Blocks", r.blocks.to_string());
    e.set_property("Logic", r.logic.to_string());
    e.set_property("Map", q(&r.map));
    e.set_property("Date", q(&r.date));
    e.set_property("Source", q(&r.source));
    e.set_property("Captured", q(&r.captured));
    e
}

fn record_from_entity(e: &Il2Entity) -> Option<AirfieldRecord> {
    let text = |k: &str| e.property(k).unwrap_or("").trim_matches('"').to_string();
    Some(AirfieldRecord {
        name: e.name()?.to_string(),
        file: text("File"),
        country: int_prop(e, "Country").unwrap_or(0),
        x: float_prop(e, "XPos").unwrap_or(0.0),
        y: float_prop(e, "YPos").unwrap_or(0.0),
        z: float_prop(e, "ZPos").unwrap_or(0.0),
        footprint_m: float_prop(e, "Footprint").unwrap_or(0.0),
        taxi_nodes: int_prop(e, "TaxiNodes").unwrap_or(0).max(0) as usize,
        axis_heading_deg: float_prop(e, "AxisHeading"),
        axis_length_m: float_prop(e, "AxisLength"),
        vehicles: int_prop(e, "Vehicles").unwrap_or(0).max(0) as usize,
        blocks: int_prop(e, "Blocks").unwrap_or(0).max(0) as usize,
        logic: int_prop(e, "Logic").unwrap_or(0).max(0) as usize,
        map: text("Map"),
        date: text("Date"),
        source: text("Source"),
        captured: text("Captured"),
    })
}

/// Records in `catalog.Group` (empty when the file does not exist yet).
pub fn read_catalog(path: &Path) -> Result<Vec<AirfieldRecord>, String> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let root = parse_group_file(&text).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(root
        .children
        .iter()
        .filter(|c| c.block_type == RECORD_BLOCK)
        .filter_map(record_from_entity)
        .collect())
}

/// Replace records with the same name and country; keep the rest sorted by name.
fn upsert_catalog(path: &Path, fresh: &[AirfieldRecord]) -> Result<(), String> {
    let mut records = read_catalog(path)?;
    for r in fresh {
        records.retain(|old| !(old.name == r.name && old.country == r.country));
        records.push(r.clone());
    }
    records.sort_by(|a, b| a.name.cmp(&b.name).then(a.country.cmp(&b.country)));
    let mut root = Il2Entity::new("Group");
    root.set_name("AirfieldDB");
    root.set_property("Desc", "\"Harvested by IL-2 Mission Utility; not an editor group\"");
    root.children = records.iter().map(record_entity).collect();
    std::fs::write(path, serialize_group(&root)).map_err(|err| format!("{}: {err}", path.display()))
}

/// Append Model/Script pairs not yet in `models.tsv`. Returns how many were new.
fn append_models(path: &Path, root: &Il2Entity, seen_at: &str) -> Result<usize, String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut known: HashSet<(String, String)> = existing
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut cols = l.split('\t');
            Some((cols.next()?.to_string(), cols.next()?.to_string()))
        })
        .collect();
    let mut out = if existing.is_empty() {
        "Type\tScript\tModel\tFirstSeenIn\r\n".to_string()
    } else {
        existing
    };
    let mut added = 0usize;
    root.for_each(&mut |e| {
        let (Some(model), Some(script)) = (e.property("Model"), e.property("Script")) else {
            return;
        };
        let script = script.trim_matches('"').to_string();
        if known.insert((e.block_type.clone(), script.clone())) {
            out.push_str(&format!(
                "{}\t{script}\t{}\t{seen_at}\r\n",
                e.block_type,
                model.trim_matches('"')
            ));
            added += 1;
        }
    });
    if added > 0 {
        std::fs::write(path, out).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(added)
}

struct Stamp {
    /// `2026-09-24_153012Z_` — file-name prefix.
    file: String,
    /// `2026-09-24 15:30:12 UTC`
    display: String,
}

fn utc_stamp(t: SystemTime) -> Stamp {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, rem % 3600 / 60, rem % 60);
    Stamp {
        file: format!("{y:04}-{m:02}-{d:02}_{hh:02}{mm:02}{ss:02}Z"),
        display: format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC"),
    }
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn dist2(ax: f64, az: f64, bx: f64, bz: f64) -> f64 {
    (ax - bx).powi(2) + (az - bz).powi(2)
}

fn int_prop(e: &Il2Entity, key: &str) -> Option<i32> {
    e.property(key)?.trim().parse().ok()
}

fn float_prop(e: &Il2Entity, key: &str) -> Option<f64> {
    e.property(key)?.trim().parse().ok()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FileSig {
    modified: SystemTime,
    len: u64,
}

fn file_sig(path: &Path) -> Option<FileSig> {
    let meta = std::fs::metadata(path).ok()?;
    Some(FileSig {
        modified: meta.modified().ok()?,
        len: meta.len(),
    })
}

/// Polls a Missions folder for `_gen.mission` rewrites. A file that already
/// exists when the watcher starts is ignored (use Harvest now for it).
#[derive(Debug, Clone)]
pub struct GenWatcher {
    dir: PathBuf,
    done: Option<FileSig>,
    pending: Option<(FileSig, Instant)>,
}

impl GenWatcher {
    pub fn new(dir: PathBuf) -> Self {
        let done = find_gen_file(&dir).and_then(|p| file_sig(&p));
        Self {
            dir,
            done,
            pending: None,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Treat the current `_gen.mission` as handled (after a manual harvest).
    pub fn acknowledge_current(&mut self) {
        self.done = find_gen_file(&self.dir).and_then(|p| file_sig(&p));
        self.pending = None;
    }

    /// The `_gen.mission` path once a new version has been stable for
    /// `STABLE_FOR`; otherwise `None`.
    pub fn poll(&mut self, now: Instant) -> Option<PathBuf> {
        let path = find_gen_file(&self.dir)?;
        let sig = file_sig(&path)?;
        if self.done == Some(sig) {
            self.pending = None;
            return None;
        }
        match self.pending {
            Some((p, since)) if p == sig => {
                if now.duration_since(since) >= STABLE_FOR {
                    self.done = Some(sig);
                    self.pending = None;
                    return Some(path);
                }
            }
            _ => self.pending = Some((sig, now)),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_il2_document;

    fn k13() -> Il2Entity {
        parse_group_file(include_str!("../References/K13 AFB_mp.Group")).expect("K13")
    }

    fn seoul_flat() -> Il2Entity {
        parse_il2_document(include_str!("../TemplateExamples/Seoul AFB.Group")).expect("Seoul")
    }

    /// A fake `_gen.mission`: comment, Options with WindLayers, then objects.
    fn mission_text(body: &str) -> String {
        format!(
            "# Mission File Version = 1.0;\r\n\r\nOptions\r\n{{\r\n  LCName = 0;\r\n  Time = 7:56:34;\r\n  Date = 29.7.1951;\r\n  GuiMap = \"korea-summer\";\r\n  WindLayers\r\n  {{\r\n    0 :     143 :     1.5;\r\n    500 :     0 :     1;\r\n  }}\r\n  Countries\r\n  {{\r\n    Country\r\n    {{\r\n      ID = 601;\r\n      Coalition = 2;\r\n    }}\r\n  }}\r\n}}\r\n\r\n{body}\r\n\r\n# end of file"
        )
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "il2_harvest_{tag}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn block(kind: &str, index: i32, x: f64, z: f64, extra: &str) -> String {
        format!(
            "{kind}\r\n{{\r\n  Index = {index};\r\n  Name = \"{kind}{index}\";\r\n  XPos = {x:.3};\r\n  YPos = 10.000;\r\n  ZPos = {z:.3};\r\n{extra}}}\r\n"
        )
    }

    #[test]
    fn parse_mission_drops_comments_and_options() {
        let text = mission_text(&serialize_group(&k13()));
        let (root, header) = parse_mission_with_header(&text).expect("parse");
        assert_eq!(header.map, "korea-summer");
        assert_eq!(header.date, "29.7.1951");
        assert_eq!(root.name(), Some("K13 AFB"));
        assert!(root.find_by_name("K-13_Suwon_AF").is_some());
    }

    /// A Freeflight file can put a literal quote in a tail code (`""`).
    /// That plane sits ahead of the start airfield, so the cut has to read past it.
    #[test]
    fn escaped_quote_in_player_plane_still_harvests_airfield() {
        let body = "\
Plane\r\n{\r\n  Name = \"\";\r\n  Index = 1;\r\n  XPos = 10.000;\r\n  YPos = 0.000;\r\n  ZPos = 10.000;\r\n  AILevel = 0;\r\n  TCode = \"   \"\"&\";\r\n}\r\n\
Airfield\r\n{\r\n  Name = \"K-27_Yonpo_AF\";\r\n  Index = 2;\r\n  XPos = 0.000;\r\n  YPos = 0.000;\r\n  ZPos = 0.000;\r\n  Country = 601;\r\n}\r\n";
        let (root, _) = parse_mission_with_header(&mission_text(body)).expect("parse");
        let got = harvest_root(&root, &HarvestConfig::default());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].record.name, "K-27_Yonpo_AF");
        assert_eq!(got[0].record.country, 601);
    }

    #[test]
    fn cut_block_ignores_names_inside_quotes_and_words() {
        let text = "Group\r\n{\r\n  Name = \"Options {\";\r\n}\r\nOptionsX\r\n{\r\n}\r\nOptions\r\n{\r\n  A { B = 1; }\r\n}\r\nTail\r\n{\r\n}";
        let (rest, block) = cut_top_level_block(text, "Options");
        let block = block.expect("Options block");
        assert!(block.starts_with("Options"));
        assert!(block.ends_with('}'));
        assert!(rest.contains("OptionsX"));
        assert!(rest.contains("Tail"));
        assert!(rest.contains("\"Options {\""));
    }

    #[test]
    fn k13_harvests_whole_package() {
        let root = k13();
        let before = root.count_block_type("MCU_Timer");
        let got = harvest_root(&root, &HarvestConfig::default());
        assert_eq!(got.len(), 1);
        let af = &got[0];
        assert_eq!(af.record.name, "K-13_Suwon_AF");
        assert_eq!(af.record.country, 601);
        assert_eq!(af.record.file, "K-13_Suwon_AF_601.Group");
        assert_eq!(af.group.count_block_type("MCU_Timer"), before);
        assert_eq!(af.group.count_block_type("MCU_TR_TaxiGraph"), 1);
        assert_eq!(af.record.taxi_nodes, 205);
        assert!(af.group.count_block_type("MCU_Icon") >= 30, "icons via links");
        let len = af.record.axis_length_m.unwrap();
        assert!((1500.0..4000.0).contains(&len), "axis length {len}");
        let text = serialize_group(&af.group);
        parse_group_file(&text).expect("reparse harvested group");
    }

    #[test]
    fn seoul_freeflight_drops_player_and_keeps_field() {
        let root = seoul_flat();
        let got = harvest_root(&root, &HarvestConfig::default());
        assert_eq!(got.len(), 1);
        let af = &got[0];
        let mut players = 0;
        af.group.for_each(&mut |e| {
            if e.block_type == "Plane" {
                players += 1;
            }
        });
        assert_eq!(players, 0);
        assert!(af.cleaned.stripped > 10);
        assert!(af.group.find_by_name("VERTICAL_SearchLightArea").is_some());
        assert_eq!(af.group.count_block_type("MCU_TR_TaxiGraph"), 1);
    }

    #[test]
    fn nearest_field_split_and_player_picks_start_field() {
        let a = block("Airfield", 1, 0.0, 0.0, "  Country = 601;\r\n");
        let b = block("Airfield", 2, 3000.0, 0.0, "  Country = 501;\r\n");
        let near_a = block("Block", 3, 1000.0, 0.0, "  Model = \"a.mgm\";\r\n  Script = \"a.txt\";\r\n");
        let near_b = block("Block", 4, 2000.0, 0.0, "  Model = \"b.mgm\";\r\n  Script = \"b.txt\";\r\n");
        let timer = block("MCU_Timer", 5, 100.0, 0.0, "  Targets = [4,6];\r\n  Objects = [];\r\n");
        let far_icon = block("MCU_Icon", 6, 14000.0, 0.0, "  Targets = [];\r\n  Objects = [];\r\n");
        let body = [a, b, near_a, near_b, timer, far_icon].join("\r\n");
        let root = parse_mission_with_header(&mission_text(&body)).map(|(r, _)| r).unwrap();

        let all = harvest_root(&root, &HarvestConfig::default());
        assert_eq!(all.len(), 2, "no player: every field");
        let fa = all.iter().find(|h| h.record.country == 601).unwrap();
        assert!(fa.group.find_by_name("Block3").is_some());
        assert!(fa.group.find_by_name("Block4").is_none(), "closer to field 2");
        assert!(fa.group.find_by_name("MCU_Icon6").is_some(), "pulled in by link");
        assert_eq!(fa.via_links, 1);
        let t = fa.group.find_by_name("MCU_Timer5").unwrap();
        assert_eq!(t.targets, vec![6], "link to the other field scrubbed");

        let player = block("Plane", 7, 2900.0, 0.0, "  AILevel = 0;\r\n  Country = 501;\r\n  Model = \"p.mgm\";\r\n  Script = \"p.txt\";\r\n");
        let body = format!("{body}\r\n{player}");
        let root = parse_mission_with_header(&mission_text(&body)).map(|(r, _)| r).unwrap();
        let start = harvest_root(&root, &HarvestConfig::default());
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].record.country, 501);
        assert_eq!(start[0].group.count_block_type("Plane"), 0);
    }

    #[test]
    fn ai_planes_stripped_unless_kept() {
        let a = block("Airfield", 1, 0.0, 0.0, "  Country = 601;\r\n");
        let ai = block("Plane", 2, 200.0, 0.0, "  AILevel = 2;\r\n  LinkTrId = 3;\r\n  Model = \"p.mgm\";\r\n  Script = \"p.txt\";\r\n");
        let ent = block("MCU_TR_Entity", 3, 200.0, 0.0, "  Targets = [];\r\n  Objects = [];\r\n  MisObjID = 2;\r\n");
        let body = [a, ai, ent].join("\r\n");
        let root = parse_mission_with_header(&mission_text(&body)).map(|(r, _)| r).unwrap();
        let stripped = harvest_root(&root, &HarvestConfig::default());
        assert_eq!(stripped[0].ai_planes_removed, 1);
        assert_eq!(stripped[0].group.count_block_type("Plane"), 0);
        assert_eq!(stripped[0].group.count_block_type("MCU_TR_Entity"), 0);
        let cfg = HarvestConfig {
            keep_ai_planes: true,
            ..HarvestConfig::default()
        };
        let kept = harvest_root(&root, &cfg);
        assert_eq!(kept[0].group.count_block_type("Plane"), 1);
        assert_eq!(kept[0].group.count_block_type("MCU_TR_Entity"), 1);
    }

    #[test]
    fn principal_axis_of_a_line() {
        let pts: Vec<(f64, f64)> = (0..=10).map(|i| (i as f64 * 100.0, i as f64 * 100.0)).collect();
        let (h, l) = principal_axis(&pts).unwrap();
        assert!((h - 45.0).abs() < 1e-6, "heading {h}");
        assert!((l - 1000.0 * 2f64.sqrt()).abs() < 1e-6, "length {l}");
        let (h, _) = principal_axis(&[(0.0, 0.0), (0.0, 500.0)]).unwrap();
        assert!((h - 90.0).abs() < 1e-6);
    }

    #[test]
    fn utc_stamp_formats_known_instant() {
        let t = UNIX_EPOCH + Duration::from_secs(1_790_263_812); // 2026-09-24 15:30:12 UTC
        let s = utc_stamp(t);
        assert_eq!(s.display, "2026-09-24 15:30:12 UTC");
        assert_eq!(s.file, "2026-09-24_153012Z");
    }

    #[test]
    fn harvest_file_writes_db_and_upserts_catalog() {
        let game = scratch_dir("game");
        let db = scratch_dir("db");
        let gen_path = game.join("_gen.Mission");
        std::fs::write(&gen_path, mission_text(&serialize_group(&k13()))).unwrap();
        let eng = std::fs::read("References/K13 AFB_mp.eng").unwrap();
        std::fs::write(game.join("_gen.eng"), eng).unwrap();

        let out = harvest_file(&gen_path, &db, &HarvestConfig::default()).unwrap();
        assert!(out.archived.is_file());
        assert!(out.archived.with_extension("eng").is_file());
        assert!(db.join("K-13_Suwon_AF_601.Group").is_file());
        assert!(db.join("K-13_Suwon_AF_601.eng").is_file());
        assert!(out.models_added > 10);
        let cat = read_catalog(&db.join(CATALOG_FILE)).unwrap();
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].map, "korea-summer");
        assert_eq!(cat[0].taxi_nodes, 205);
        assert!(cat[0].source.starts_with("raw/"));

        let again = harvest_file(&gen_path, &db, &HarvestConfig::default()).unwrap();
        assert_eq!(again.models_added, 0);
        assert_eq!(read_catalog(&db.join(CATALOG_FILE)).unwrap().len(), 1, "upsert, not append");

        let _ = std::fs::remove_dir_all(&game);
        let _ = std::fs::remove_dir_all(&db);
    }

    #[test]
    fn watcher_waits_for_stable_rewrite() {
        let dir = scratch_dir("watch");
        let gen_path = dir.join("_gen.mission");
        std::fs::write(&gen_path, "old").unwrap();
        let mut w = GenWatcher::new(dir.clone());
        let t0 = Instant::now();
        assert_eq!(w.poll(t0), None, "existing file ignored");

        std::fs::write(&gen_path, "new mission text").unwrap();
        assert_eq!(w.poll(t0), None, "first sighting only arms");
        assert_eq!(w.poll(t0 + Duration::from_millis(500)), None, "not stable yet");
        assert_eq!(w.poll(t0 + STABLE_FOR), Some(gen_path.clone()));
        assert_eq!(w.poll(t0 + STABLE_FOR * 2), None, "reported once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn korea_folders_are_the_defaults() {
        assert_eq!(
            DEFAULT_MISSIONS_DIR,
            r"C:\Program Files\IL2Series\game\data\Missions"
        );
        assert_eq!(
            DEFAULT_DB_DIR,
            r"C:\Program Files\IL2Series\game\data\Template\MP Airfields"
        );
        let _ = default_missions_dir();
    }

    fn taxi_xz(root: &Il2Entity) -> Vec<(String, String)> {
        let mut out = Vec::new();
        root.for_each(&mut |e| {
            if e.block_type == "Node" {
                if let (Some(x), Some(z)) = (e.property("X"), e.property("Z")) {
                    out.push((x.to_string(), z.to_string()));
                }
            }
        });
        out
    }

    fn find_block<'a>(e: &'a Il2Entity, pred: &impl Fn(&Il2Entity) -> bool) -> Option<&'a Il2Entity> {
        if pred(e) {
            return Some(e);
        }
        e.children.iter().find_map(|c| find_block(c, pred))
    }

    fn field_and_entity(root: &Il2Entity) -> (&Il2Entity, &Il2Entity) {
        let air = find_block(root, &|e| e.block_type == "Airfield").expect("airfield");
        let link = int_prop(air, "LinkTrId").unwrap();
        let entity = find_block(root, &|e| e.block_type == "MCU_TR_Entity" && e.index == Some(link)).expect("entity");
        (air, entity)
    }

    fn planes_of(air: &Il2Entity) -> &Il2Entity {
        air.children.iter().find(|c| c.block_type == "Planes").expect("planes")
    }

    #[test]
    fn k13_engine_running_holds_short_of_the_base_end() {
        let mut root = k13();
        let nodes_before = taxi_xz(&root);
        let (air0, ent0) = field_and_entity(&root);
        let air_y = air0.property("YPos").unwrap().to_string();
        let ent_y = ent0.property("YPos").unwrap().to_string();
        let link = air0.property("LinkTrId").unwrap().to_string();
        let mis = ent0.property("MisObjID").unwrap().to_string();
        let mut plane = crate::airstart::default_plane("f51d");
        plane.number = 6;
        plane.fuel = 0.4;
        let planes = vec![plane];
        let spot = place_field_spawn(&mut root, &planes, None, Some(601)).unwrap();
        assert!(
            (spot.x - 72007.346).abs() < 0.001 && (spot.z - 287796.226).abs() < 0.001,
            "{spot:?}"
        );
        assert!((spot.heading_deg - 234.0).abs() < 0.05, "heading {}", spot.heading_deg);
        assert!((heading_delta(spot.heading_deg, 144.0) - 90.0).abs() < 1.0);
        let (air, ent) = field_and_entity(&root);
        for obj in [air, ent] {
            let x: f64 = obj.property("XPos").unwrap().parse().unwrap();
            let z: f64 = obj.property("ZPos").unwrap().parse().unwrap();
            let h: f64 = obj.property("YOri").unwrap().parse().unwrap();
            assert!((x - spot.x).abs() < 0.001 && (z - spot.z).abs() < 0.001);
            assert!((h - spot.heading_deg).abs() < 0.001);
        }
        assert_eq!(air.property("YPos"), Some(air_y.as_str()));
        assert_eq!(ent.property("YPos"), Some(ent_y.as_str()));
        assert_eq!(air.property("LinkTrId"), Some(link.as_str()));
        assert_eq!(ent.property("MisObjID"), Some(mis.as_str()));
        assert_eq!(ent.index, int_prop(air, "LinkTrId"));
        assert_eq!(air.index, int_prop(ent, "MisObjID"));
        assert_eq!(taxi_xz(&root), nodes_before);
        let plane = &planes_of(air).children[0];
        assert_eq!(plane.property("StartType"), Some("1"));
        assert_eq!(plane.property("SnapTo"), Some("0"));
        assert_eq!(plane.property("Altitude"), Some("0"));
        assert_eq!(
            air.property("Model"),
            Some(r#""graphics\airfields\fakefield_rnwspawn.mgm""#)
        );
        assert_eq!(
            air.property("Script"),
            Some(r#""LuaScripts\WorldObjects\Airfields\fakefield_rnwspawn.txt""#)
        );
        assert_eq!(plane.property("Number"), Some("6"));
        assert_eq!(plane.property("Fuel"), Some("0.4"));
        assert_eq!(plane.property("Name"), Some("\"At ramp - Clean\""));
        assert!(plane.property("Script").unwrap().contains("f51d"));
        assert_eq!(air.property("Country"), Some("601"));
        let northwest = dist2(spot.x, spot.z, 71948.326, 287712.612).sqrt();
        let southeast = dist2(spot.x, spot.z, 69788.467, 289281.835).sqrt();
        assert!(northwest < southeast, "spawn should be the northwest hold-short");
        assert!(northwest < 120.0, "hold-short {northwest:.0} m from the threshold");
        let nose = spot.heading_deg.to_radians();
        let toward = nose.cos() * (71948.326 - spot.x) + nose.sin() * (287712.612 - spot.z);
        assert!(toward > 0.0, "nose faces the runway");
    }

    #[test]
    fn preferred_heading_picks_the_into_wind_threshold() {
        let planes = vec![crate::airstart::default_plane("f51d")];
        let mut base = k13();
        let mut wind = k13();
        let base_spot = place_field_spawn(&mut base, &planes, None, None).unwrap();
        let wind_spot = place_field_spawn(&mut wind, &planes, Some(324.0), None).unwrap();
        assert!((heading_delta(wind_spot.heading_deg, 324.0) - 90.0).abs() < 15.0, "{}", wind_spot.heading_deg);
        let moved = dist2(base_spot.x, base_spot.z, wind_spot.x, wind_spot.z).sqrt();
        assert!(moved > 1000.0, "into-wind end is the other threshold, moved {moved:.0} m");
        let to_se = dist2(wind_spot.x, wind_spot.z, 69788.467, 289281.835).sqrt();
        assert!(to_se < 120.0, "wind spawn {to_se:.0} m from the southeast threshold");
    }

    #[test]
    fn k14_engine_running_stays_off_the_midfield_ramp() {
        let mut root = parse_group_file(include_str!("../References/K14 AFB_mp.Group")).unwrap();
        let before = taxi_xz(&root);
        let planes = vec![crate::airstart::default_plane("f86a5")];
        let spot = place_field_spawn(&mut root, &planes, None, None).unwrap();
        assert!(
            (spot.x - 106612.739).abs() < 0.001 && (spot.z - 268571.103).abs() < 0.001,
            "{spot:?}"
        );
        assert!((heading_delta(spot.heading_deg, 135.0) - 90.0).abs() < 15.0, "{}", spot.heading_deg);
        let to_threshold = dist2(spot.x, spot.z, 106649.0, 268611.0).sqrt();
        let to_ramp = dist2(spot.x, spot.z, 106299.0, 269360.0).sqrt();
        assert!(to_threshold < 80.0, "hold-short {to_threshold:.0} m from the northwest end");
        assert!(to_ramp > 400.0, "engine running is {to_ramp:.0} m from the midfield ramp");
        assert_eq!(taxi_xz(&root), before);
        let (air, _) = field_and_entity(&root);
        assert_eq!(planes_of(air).children[0].property("StartType"), Some("1"));
    }

    #[test]
    fn parking_snap_stays_on_the_hold_short() {
        let running = vec![crate::airstart::default_plane("f51d")];
        let mut parked_plane = crate::airstart::default_plane("mig15bis");
        parked_plane.parking_snap = true;
        let parked = vec![parked_plane];

        let mut hold = k13();
        let hold_spot = place_field_spawn(&mut hold, &running, None, None).unwrap();
        let mut root = k13();
        let spot = place_field_spawn(&mut root, &parked, None, None).unwrap();
        assert!((spot.x - hold_spot.x).abs() < 0.001 && (spot.z - hold_spot.z).abs() < 0.001);
        assert!((spot.x - 72007.346).abs() < 0.001 && (spot.z - 287796.226).abs() < 0.001, "{spot:?}");
        assert!((spot.heading_deg - hold_spot.heading_deg).abs() < 0.001);
        let (air, ent) = field_and_entity(&root);
        let x: f64 = air.property("XPos").unwrap().parse().unwrap();
        let z: f64 = air.property("ZPos").unwrap().parse().unwrap();
        let h: f64 = air.property("YOri").unwrap().parse().unwrap();
        assert!((x - spot.x).abs() < 0.001 && (z - spot.z).abs() < 0.001);
        assert!((h - spot.heading_deg).abs() < 0.001);
        let ex: f64 = ent.property("XPos").unwrap().parse().unwrap();
        let ez: f64 = ent.property("ZPos").unwrap().parse().unwrap();
        assert!((ex - spot.x).abs() < 0.001 && (ez - spot.z).abs() < 0.001);
        let plane = &planes_of(air).children[0];
        assert_eq!(plane.property("StartType"), Some("2"));
        assert_eq!(plane.property("SnapTo"), Some("2"));
        assert_eq!(plane.property("Altitude"), Some("0"));
        assert_eq!(plane.property("Name"), Some("\"Parked - Clean\""));
        assert_eq!(
            air.property("Model"),
            Some(r#""graphics\airfields\fakefield_rnwspawn.mgm""#)
        );

        let bare = "\
Group\r\n{\r\nAirfield\r\n{\r\nIndex = 1;\r\nLinkTrId = 2;\r\nXPos = 0;\r\nYPos = 1;\r\nZPos = 0;\r\nModel = \"graphics\\\\airfields\\\\fakefield.mgm\";\r\n}\r\nMCU_TR_Entity\r\n{\r\nIndex = 2;\r\nMisObjID = 1;\r\nXPos = 0;\r\nYPos = 1.2;\r\nZPos = 0;\r\n}\r\nMCU_TR_TaxiGraph\r\n{\r\nIndex = 3;\r\nObjects = [2];\r\nXPos = 0;\r\nZPos = 0;\r\nShape\r\n{\r\nNode\r\n{\r\nX = 0;\r\nZ = 0;\r\nData\r\n{\r\nRunway = 0;\r\nRunwayEnd = 0;\r\n}\r\n}\r\nNode\r\n{\r\nX = 1000;\r\nZ = 0;\r\nData\r\n{\r\nRunway = 0;\r\nRunwayEnd = 0;\r\n}\r\n}\r\nNode\r\n{\r\nX = 4;\r\nZ = 16;\r\n}\r\n}\r\n}\r\nBlock\r\n{\r\nIndex = 4;\r\nXPos = 10;\r\nZPos = 10;\r\nModel = \"m\";\r\n}\r\n}\r\n";
        let mut bare = parse_group_file(bare).unwrap();
        let spot = place_field_spawn(&mut bare, &parked, None, None).unwrap();
        assert!((spot.x - 4.0).abs() < 0.001 && (spot.z - 16.0).abs() < 0.001, "{spot:?}");
        let (air, _) = field_and_entity(&bare);
        assert_eq!(planes_of(air).children[0].property("SnapTo"), Some("2"));
        assert_eq!(planes_of(air).children[0].property("StartType"), Some("2"));
    }

    #[test]
    fn field_spawn_name_states_the_condition_and_the_load() {
        assert_eq!(field_spawn_name(1, 0, 0, 0.65), "At ramp - Clean");
        assert_eq!(field_spawn_name(1, 0, 0, 0.94), "At ramp - Clean");
        assert_eq!(field_spawn_name(2, 2, 0, 0.65), "Parked - Clean");
        assert_eq!(field_spawn_name(2, 0, 0, 0.5), "Parked - Clean");
        assert_eq!(field_spawn_name(3, 0, 0, 0.5), "Parked - Clean");
        assert_eq!(field_spawn_name(1, 2, 0, 0.5), "Parked - Clean");
        assert_eq!(field_spawn_name(1, 1, 0, 0.5), "On runway - Clean");
        assert_eq!(field_spawn_name(2, 1, 0, 0.5), "On runway - Clean");
        assert_eq!(field_spawn_name(0, 0, 0, 0.5), "In air - Clean");
        assert_eq!(field_spawn_name(1, 0, 2, 0.5), "At ramp - Strike 2");
        assert_eq!(field_spawn_name(1, 1, 2, 0.4), "On runway - Strike 2");
        assert_eq!(field_spawn_name(1, 0, 0, 0.95), "At ramp - Long range");
        assert_eq!(field_spawn_name(1, 0, 0, 1.0), "At ramp - Long range");
        assert_eq!(field_spawn_name(2, 2, 0, 1.0), "Parked - Long range");
        assert_eq!(field_spawn_name(1, 0, 2, 0.95), "At ramp - Strike 2 - Long range");

        let mut strike = crate::airstart::default_plane("f51d");
        strike.payload_id = 2;
        strike.fuel = 0.4;
        let mut long_range = crate::airstart::default_plane("f51d");
        long_range.fuel = 0.95;
        long_range.parking_snap = true;
        let mut both = crate::airstart::default_plane("f51d");
        both.payload_id = 2;
        both.fuel = 1.0;
        let mut root = k13();
        place_field_spawn(&mut root, &[strike, long_range, both], None, None).unwrap();
        let (air, _) = field_and_entity(&root);
        let names: Vec<_> = planes_of(air)
            .children
            .iter()
            .map(|p| p.property("Name").unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "\"At ramp - Strike 2\"".to_string(),
                "\"Parked - Long range\"".to_string(),
                "\"At ramp - Strike 2 - Long range\"".to_string(),
            ]
        );
    }

    /// Smoke test on a real game mission: `IL2_MISSION=<path> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn real_mission_from_env() {
        let path = std::env::var("IL2_MISSION").expect("set IL2_MISSION");
        let bytes = std::fs::read(&path).unwrap();
        let (root, _) =
            parse_mission_with_header(&String::from_utf8_lossy(&bytes)).expect("parse real mission");
        let got = harvest_root(&root, &HarvestConfig::default());
        for af in &got {
            eprintln!(
                "{} c{} nodes={} logic={} veh={} blocks={} via_links={} axis={:?}/{:?}",
                af.record.name,
                af.record.country,
                af.record.taxi_nodes,
                af.record.logic,
                af.record.vehicles,
                af.record.blocks,
                af.via_links,
                af.record.axis_heading_deg,
                af.record.axis_length_m
            );
            parse_group_file(&serialize_group(&af.group)).expect("reparse");
        }
    }
}
