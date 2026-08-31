//! Random ground-unit recon placements.
//!
//! Clones each unit template into its own 10 km square (from 40000, 40000)
//! so copies are easy to sort by type, then place by hand. At mission begin
//! an aircraft-style mutex waterfall keeps exactly K copies (from the activate
//! ratio) by firing the template's Mission Begin targets (`ENABLE / PULSE IN`
//! → Zone IN). A win deactivates the remaining Outs in that chain so a later
//! 100% timer cannot also fire. Losers are deleted so unused vehicles and
//! blocks do not stay in the mission.
//!
//! Clone Mission Begins are disconnected: IL-2 often fires them even when
//! Enabled = 0, which is enough to break the HUD and End Mission. Subtitle
//! MCUs are left as the template authored them.
//!
//! After hand-placement, Rework Existing loads the exported pack, keeps
//! copies where they sit, and rebuilds the randomizer. An optional start
//! delay holds the first type chain after Mission Begin (so several Random
//! Units groups in one mission can be staggered). A delay between types
//! (default 500 ms) then spaces those chains so they do not all fire together.

use crate::ast::Il2Entity;
use crate::duplicate::duplicate_template;
use crate::mapground::GroundSpot;
use crate::mapnet;
use crate::placement;

/// Same 500 ms stagger as the fighter-pack randomizer. 100 ms is too tight.
const WATERFALL_STEP_S: f64 = 0.5;
/// Local MCU icon stagger only — copies themselves use `placement` grid.
const MCU_STAGGER_COLS: usize = 10;

pub const SUGGESTED_ZONE_NAMES: &str = "Zone IN";
pub const RANDOMIZER_NAME: &str = "Recon Randomizer";
pub const DELAY_MCU_NAME: &str = "Randomizer:DELAY";

/// How to build a random-units pack.
#[derive(Debug, Clone, Copy)]
pub struct ReconBuild {
    pub activate_percent: u32,
    pub keep_positions: bool,
    /// Seconds after Mission Begin before the first type chain starts.
    pub start_delay_s: f64,
    /// Seconds between unit-type chains so they do not all fire together.
    pub group_delay_s: f64,
    /// Skip Recon Randomizer so every copy's Mission Begin fires.
    pub spawn_all: bool,
}

impl Default for ReconBuild {
    fn default() -> Self {
        Self {
            activate_percent: 50,
            keep_positions: false,
            start_delay_s: 0.0,
            group_delay_s: 0.5,
            spawn_all: false,
        }
    }
}

const WORLD_TYPES: &[&str] = &[
    "Vehicle",
    "Ship",
    "Plane",
    "Block",
    "MCU_TR_Entity",
];

#[derive(Debug, Clone)]
pub struct McuChoice {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct UnitPlanInfo {
    pub name: String,
    pub vehicle_count: usize,
    pub block_count: usize,
    pub checkzones: Vec<McuChoice>,
    pub suggested_triggers: Vec<i32>,
    /// Timers and checkzones Mission Begin can fire after the randomizer is removed.
    pub restore_starts: Vec<RestoreChoice>,
    /// Longest known gun/MG in the template, if any.
    pub weapon_range_m: Option<f64>,
    /// Train, or a perfect vehicle column (road). `None` = open-ground placement.
    pub route: Option<crate::mapnet::RouteLayout>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreKind {
    Timer,
    CheckZone,
}

#[derive(Clone, Debug)]
pub struct RestoreChoice {
    pub name: String,
    pub kind: RestoreKind,
    pub recommended: bool,
    pub hint: String,
}

impl UnitPlanInfo {
    pub fn suggested_restore(&self) -> Option<&RestoreChoice> {
        self.restore_starts
            .iter()
            .find(|c| c.recommended)
            .or(self.restore_starts.first())
    }
}

#[derive(Debug, Clone)]
pub struct ReconInput {
    pub label: String,
    pub root: Il2Entity,
    pub trigger_zone_ids: Vec<i32>,
    pub copies: usize,
}

/// Spread `total` copies across templates by relative influence weights.
pub fn allocate_copies(weights: &[u32], total: usize) -> Vec<usize> {
    let n = weights.len();
    if n == 0 || total == 0 {
        return vec![0; n];
    }
    let sum: u32 = weights.iter().sum();
    if sum == 0 {
        return vec![0; n];
    }
    let mut out = vec![0usize; n];
    let mut frac: Vec<(usize, f64)> = Vec::with_capacity(n);
    for (i, w) in weights.iter().enumerate() {
        let exact = total as f64 * *w as f64 / sum as f64;
        out[i] = exact.floor() as usize;
        frac.push((i, exact.fract()));
    }
    let mut rem = total.saturating_sub(out.iter().sum());
    frac.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (i, _) in frac {
        if rem == 0 {
            break;
        }
        out[i] += 1;
        rem -= 1;
    }
    out
}

/// How many copies must win. 2 at 50% → 1; 100 at 20% → 20. Never zero.
pub fn wanted_winners(total: usize, percent: u32) -> usize {
    if total == 0 {
        return 0;
    }
    ((total * percent as usize + 50) / 100).clamp(1, total)
}

/// Copies created for a type, and how many of those win that type's waterfall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeMix {
    pub copies: usize,
    pub activate: usize,
}

impl TypeMix {
    pub fn from_copies(copies: usize, activate_percent: u32) -> Self {
        Self {
            copies,
            activate: wanted_winners(copies, activate_percent),
        }
    }
}

/// Influence splits `total` copies; activate % is applied **per type**, not on the pack sum.
pub fn allocate_mix(weights: &[u32], total: usize, activate_percent: u32) -> Vec<TypeMix> {
    allocate_copies(weights, total)
        .into_iter()
        .map(|n| TypeMix::from_copies(n, activate_percent))
        .collect()
}

#[derive(Debug, Clone)]
pub struct PlacedTypeInfo {
    pub name: String,
    pub copy_count: usize,
    pub unit: UnitPlanInfo,
}

#[derive(Debug, Clone)]
pub struct PlacedPackInfo {
    pub name: String,
    pub copy_count: usize,
    pub types: Vec<PlacedTypeInfo>,
    pub has_randomizer: bool,
}

/// Strip generator `[3]` or editor `*3*` suffixes so copies of one template group together.
pub fn copy_type_name(name: &str) -> String {
    strip_index_suffix(name.trim()).trim_end().to_string()
}

fn strip_index_suffix(name: &str) -> &str {
    if let Some(stripped) = strip_star_index(name) {
        return stripped;
    }
    if let Some(stripped) = strip_bracket_index(name) {
        return stripped;
    }
    name
}

fn strip_star_index(name: &str) -> Option<&str> {
    if !name.ends_with('*') {
        return None;
    }
    let inner = &name[..name.len() - 1];
    let star = inner.rfind('*')?;
    let digits = &inner[star + 1..];
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(&inner[..star])
}

fn strip_bracket_index(name: &str) -> Option<&str> {
    let open = name.rfind('[')?;
    let close = name.rfind(']')?;
    if close != name.len() - 1 || close <= open + 1 {
        return None;
    }
    if !name[open + 1..close].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(&name[..open])
}

fn is_placed_copy(entity: &Il2Entity) -> bool {
    entity.block_type == "Group" && entity.name() != Some(RANDOMIZER_NAME)
}

fn top_level_copies(root: &Il2Entity) -> Vec<&Il2Entity> {
    root.children.iter().filter(|c| is_placed_copy(c)).collect()
}

fn pack_root(root: &Il2Entity) -> &Il2Entity {
    if looks_like_pack_here(root) {
        return root;
    }
    let groups: Vec<&Il2Entity> = root
        .children
        .iter()
        .filter(|c| c.block_type == "Group")
        .collect();
    if groups.len() == 1 {
        return pack_root(groups[0]);
    }
    root
}

fn pack_root_mut(root: &mut Il2Entity) -> &mut Il2Entity {
    if looks_like_pack_here(root) {
        return root;
    }
    let idx = {
        let groups: Vec<usize> = root
            .children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.block_type == "Group")
            .map(|(i, _)| i)
            .collect();
        if groups.len() == 1 {
            Some(groups[0])
        } else {
            None
        }
    };
    match idx {
        Some(i) => pack_root_mut(&mut root.children[i]),
        None => root,
    }
}

fn looks_like_pack_here(root: &Il2Entity) -> bool {
    if root
        .name()
        .is_some_and(|n| n.starts_with("Random Ground Units"))
    {
        return true;
    }
    if root.children.iter().any(|c| c.name() == Some(RANDOMIZER_NAME)) {
        return true;
    }
    let copies = top_level_copies(root);
    let numbered = copies
        .iter()
        .filter(|c| {
            c.name()
                .is_some_and(|n| strip_star_index(n).is_some() || strip_bracket_index(n).is_some())
        })
        .count();
    if numbered >= 2 {
        return true;
    }
    if copies.len() >= 2 {
        let mut seen: Vec<(String, usize)> = Vec::new();
        for c in &copies {
            let ty = copy_type_name(c.name().unwrap_or("Unit"));
            if let Some((_, n)) = seen.iter_mut().find(|(name, _)| *name == ty) {
                *n += 1;
                if *n >= 2 {
                    return true;
                }
            } else {
                seen.push((ty, 1));
            }
        }
    }
    false
}

/// True for a Random Ground Units pack, including editor-exported `*N*` names.
pub fn looks_like_placed_pack(root: &Il2Entity) -> bool {
    looks_like_pack_here(pack_root(root))
}

/// Top-level numbered copies in a Random Ground Units pack.
pub fn placed_copy_count(root: &Il2Entity) -> usize {
    top_level_copies(pack_root(root)).len()
}

pub fn inspect_placed_pack(root: &Il2Entity) -> Result<PlacedPackInfo, String> {
    let root = pack_root(root);
    if !looks_like_pack_here(root) {
        return Err(
            "not a Random Ground Units pack — export the generated group from the editor".into(),
        );
    }
    let copies = top_level_copies(root);
    if copies.is_empty() {
        return Err("that pack has no unit groups to adjust".into());
    }
    let mut types: Vec<PlacedTypeInfo> = Vec::new();
    for copy in &copies {
        let name = copy_type_name(copy.name().unwrap_or("Unit"));
        if let Some(existing) = types.iter_mut().find(|t| t.name == name) {
            existing.copy_count += 1;
        } else {
            types.push(PlacedTypeInfo {
                name: name.clone(),
                copy_count: 1,
                unit: inspect_unit(copy).unwrap_or_else(|_| fallback_unit_info(copy, &name)),
            });
        }
    }
    Ok(PlacedPackInfo {
        name: root.name().unwrap_or("Placed pack").to_string(),
        copy_count: copies.len(),
        has_randomizer: root
            .children
            .iter()
            .any(|c| c.name() == Some(RANDOMIZER_NAME)),
        types,
    })
}

/// Merge placed copies from several packs into one group. Indexes are unique.
/// Types not in `keep_types` are dropped. Existing randomizers are not copied.
pub fn combine_placed_packs(
    roots: &[Il2Entity],
    keep_types: &[String],
) -> Result<Il2Entity, String> {
    if roots.is_empty() {
        return Err("load at least one placed pack".into());
    }
    let mut copies = Vec::new();
    let mut next_id = 1i32;
    for root in roots {
        for copy in top_level_copies(pack_root(root)) {
            let ty = copy_type_name(copy.name().unwrap_or("Unit"));
            if !keep_types.iter().any(|k| k == &ty) {
                continue;
            }
            let (cloned, _) = duplicate_template(copy, &mut next_id);
            copies.push(cloned);
        }
    }
    if copies.is_empty() {
        return Err("no unit groups to keep from the loaded packs".into());
    }
    let n = copies.len();
    let mut out = Il2Entity::new("Group");
    out.index = Some(next_id);
    out.set_property("Index", next_id.to_string());
    out.set_name(&format!("Random Ground Units {n}"));
    out.set_property("Desc", "\"\"");
    out.children = copies;
    Ok(out)
}

/// Start time for each listed group: `start`, `start+gap`, `start+2·gap`, …
pub fn group_start_delays(n_groups: usize, gap_s: f64) -> Vec<f64> {
    staggered_start_delays(n_groups, 0.0, gap_s)
}

fn staggered_start_delays(n_groups: usize, start_s: f64, gap_s: f64) -> Vec<f64> {
    (0..n_groups)
        .map(|i| (((start_s + i as f64 * gap_s) * 1000.0).round()) / 1000.0)
        .collect()
}

/// Equal-probability waterfall: 2 choices → 50 / 100. Same as `flights`.
fn random_pct(index: usize, n: usize) -> i32 {
    let remaining = n.saturating_sub(index).max(1);
    ((100 + remaining / 2) / remaining).clamp(1, 100) as i32
}

fn partition_sizes(n: usize, k: usize) -> Vec<usize> {
    if n == 0 || k == 0 {
        return Vec::new();
    }
    let k = k.min(n);
    let base = n / k;
    let extra = n % k;
    (0..k)
        .map(|i| if i < extra { base + 1 } else { base })
        .collect()
}

pub fn inspect_unit(root: &Il2Entity) -> Result<UnitPlanInfo, String> {
    let checkzones = collect_checkzones(root);
    if checkzones.is_empty() {
        return Err("template has no MCU_CheckZone (need Zone IN)".into());
    }
    Ok(UnitPlanInfo {
        name: root.name().unwrap_or("Ground unit").to_string(),
        vehicle_count: root.count_block_type("Vehicle")
            + root.count_block_type("Ship")
            + root.count_block_type("Train"),
        block_count: root.count_block_type("Block") + root.count_block_type("Ground"),
        suggested_triggers: checkzones
            .iter()
            .filter(|z| is_zone_in(&z.name))
            .map(|z| z.index)
            .collect(),
        checkzones,
        restore_starts: restore_start_choices(root),
        weapon_range_m: crate::weapon_range::group_weapon_range(root),
        route: mapnet::inspect_route(root),
    })
}

fn fallback_unit_info(root: &Il2Entity, name: &str) -> UnitPlanInfo {
    let checkzones = collect_checkzones(root);
    UnitPlanInfo {
        name: name.to_string(),
        vehicle_count: root.count_block_type("Vehicle")
            + root.count_block_type("Ship")
            + root.count_block_type("Train"),
        block_count: root.count_block_type("Block") + root.count_block_type("Ground"),
        suggested_triggers: checkzones
            .iter()
            .filter(|z| is_zone_in(&z.name))
            .map(|z| z.index)
            .collect(),
        checkzones,
        restore_starts: restore_start_choices(root),
        weapon_range_m: crate::weapon_range::group_weapon_range(root),
        route: mapnet::inspect_route(root),
    }
}

/// Build N clones. Exactly `wanted_winners` fire their Mission Begin chain;
/// the rest are deleted. The pick is an aircraft mutex waterfall.
pub fn generate_recon(plans: &[ReconInput], activate_percent: u32) -> Result<Il2Entity, String> {
    generate_recon_ex(
        plans,
        ReconBuild {
            activate_percent,
            ..ReconBuild::default()
        },
    )
}

pub fn generate_recon_ex(plans: &[ReconInput], build: ReconBuild) -> Result<Il2Entity, String> {
    if plans.is_empty() {
        return Err("add at least one ground-unit template".into());
    }
    let percent = build.activate_percent.clamp(1, 100);
    let total: usize = plans.iter().map(|p| p.copies).sum();
    if total == 0 {
        return Err("set influence so at least one copy is created".into());
    }
    for (i, plan) in plans.iter().enumerate() {
        if plan.copies == 0 {
            continue;
        }
        if plan.trigger_zone_ids.is_empty() {
            return Err(format!("plan {} needs a Zone IN selected", i + 1));
        }
        for id in &plan.trigger_zone_ids {
            if find_index(&plan.root, *id).is_none() {
                return Err(format!("plan {} checkzone {id} was not found", i + 1));
            }
        }
    }

    let mut next_id = plans
        .iter()
        .map(|p| p.root.max_index())
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    let mut copies = Vec::with_capacity(total);
    let mut win_targets: Vec<Vec<i32>> = Vec::with_capacity(total);
    let mut delete_objects: Vec<Vec<i32>> = Vec::with_capacity(total);
    let mut slot = 0usize;
    let square_counts: Vec<usize> = plans
        .iter()
        .filter(|p| p.copies > 0)
        .map(|p| p.copies)
        .collect();
    let mut origins = placement::template_square_origins(&square_counts).into_iter();

    for plan in plans {
        if plan.copies == 0 {
            continue;
        }
        let (ox, oz) = origins.next().unwrap_or((placement::MAP_MIN, placement::MAP_MIN));
        for local in 0..plan.copies {
            let (mut clone, id_map) = duplicate_template(&plan.root, &mut next_id);
            let mut start = mission_begin_targets(&clone);
            if start.is_empty() {
                if let Some(id) = clone.find_by_name("ENABLE / PULSE IN").and_then(|e| e.index) {
                    start.push(id);
                }
            }
            if start.is_empty() {
                start = plan
                    .trigger_zone_ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied())
                    .collect();
            }
            if start.is_empty() {
                return Err(format!(
                    "{} has no Mission Begin targets or ENABLE / PULSE IN to fire on a win",
                    plan.label
                ));
            }
            if !build.spawn_all {
                silence_clone_starts(&mut clone);
            }
            if !build.keep_positions {
                placement::move_to_grid_at(&mut clone, local, plan.copies, ox, oz);
            }
            let base = plan.root.name().unwrap_or(plan.label.as_str());
            clone.set_name(&format!("{} [{}]", base, slot + 1));
            win_targets.push(start);
            delete_objects.push(world_object_ids(&clone));
            copies.push(clone);
            slot += 1;
        }
    }

    if build.spawn_all {
        let mut out = Il2Entity::new("Group");
        out.index = Some(next_id);
        out.set_property("Index", next_id.to_string());
        out.set_name(&format!("Army {total}"));
        out.set_property("Desc", "\"\"");
        out.children = copies;
        return Ok(out);
    }

    let proto_timer = find_block(&plans[0].root, "MCU_Timer")
        .cloned()
        .ok_or("template has no MCU_Timer")?;
    let proto_deact = find_block(&plans[0].root, "MCU_Deactivate")
        .cloned()
        .or_else(|| {
            plans
                .iter()
                .find_map(|p| find_block(&p.root, "MCU_Deactivate"))
                .cloned()
        });
    let proto_begin = find_block(&plans[0].root, "MCU_TR_MissionBegin")
        .cloned()
        .or_else(|| {
            plans
                .iter()
                .find_map(|p| find_block(&p.root, "MCU_TR_MissionBegin"))
                .cloned()
        });
    let proto_delete = find_block(&plans[0].root, "MCU_Delete")
        .cloned()
        .or_else(|| {
            plans
                .iter()
                .find_map(|p| find_block(&p.root, "MCU_Delete"))
                .cloned()
        });

    let type_sizes: Vec<usize> = plans
        .iter()
        .filter(|p| p.copies > 0)
        .map(|p| p.copies)
        .collect();
    let chains = chains_from_block_sizes(
        &type_sizes,
        percent,
        build.start_delay_s,
        build.group_delay_s,
    );
    let randomizer = build_randomizer(
        &copies,
        &win_targets,
        &delete_objects,
        &chains,
        &proto_timer,
        proto_deact.as_ref(),
        proto_begin.as_ref(),
        proto_delete.as_ref(),
        &mut next_id,
    )?;

    let mut out = Il2Entity::new("Group");
    out.index = Some(next_id);
    out.set_property("Index", next_id.to_string());
    out.set_name(&format!("Random Ground Units {total}"));
    out.set_property("Desc", "\"\"");
    out.children = copies;
    out.children.push(randomizer);
    Ok(out)
}

/// Move each placed copy so its first X/Z sits on `spots[i]`. The randomizer group is skipped.
pub fn park_recon_copies(root: &mut Il2Entity, spots: &[(f64, f64)]) {
    park_recon_copies_headed(root, spots, &[]);
}

/// Rotate each placed copy by `headings[i]` (degrees, 0 = north), then park it on `spots[i]`.
pub fn park_recon_copies_headed(root: &mut Il2Entity, spots: &[(f64, f64)], headings: &[f64]) {
    let mut i = 0usize;
    for child in &mut root.children {
        if !is_placed_copy(child) {
            continue;
        }
        if i >= spots.len() {
            break;
        }
        if let Some(&heading) = headings.get(i) {
            crate::placement::apply_group_heading(child, heading);
        }
        let from = child.first_xz().unwrap_or((0.0, 0.0));
        crate::placement::move_anchor_to(child, from, spots[i]);
        i += 1;
    }
}

fn park_one_ground_copy(child: &mut Il2Entity, spot: &GroundSpot) {
    if let Some(net) = &spot.network {
        mapnet::park_route_copy(
            child,
            (spot.x, spot.z),
            spot.heading_deg,
            &net.unit_xz,
            &net.waypoints,
        );
        return;
    }
    crate::placement::apply_group_heading(child, spot.heading_deg);
    let from = child.first_xz().unwrap_or((0.0, 0.0));
    crate::placement::move_anchor_to(child, from, (spot.x, spot.z));
}

/// Park each placed copy from a map preview, following roads/rails when set.
pub fn park_recon_copies_spots(root: &mut Il2Entity, spots: &[GroundSpot]) {
    let mut i = 0usize;
    for child in &mut root.children {
        if !is_placed_copy(child) {
            continue;
        }
        if i >= spots.len() {
            break;
        }
        park_one_ground_copy(child, &spots[i]);
        i += 1;
    }
}

/// After parking, move each copy's ground AttackArea MCU onto `objectives[i]`.
pub fn snap_copy_attack_areas(root: &mut Il2Entity, objectives: &[Option<(f64, f64)>]) {
    let mut i = 0usize;
    for child in &mut root.children {
        if !is_placed_copy(child) {
            continue;
        }
        if i >= objectives.len() {
            break;
        }
        if let Some((x, z)) = objectives[i] {
            crate::weapon_range::snap_ground_attack_areas(child, x, z);
        }
        i += 1;
    }
}

/// One unit group inside a loaded army (a template, or one numbered copy).
#[derive(Clone, Debug)]
pub struct ArmyCopyInfo {
    pub kind: crate::weapon_range::ArmyUnitKind,
    pub range_m: Option<f64>,
    pub x: f64,
    pub z: f64,
    pub route: Option<crate::mapnet::RouteLayout>,
}

fn army_copy_refs(root: &Il2Entity) -> Vec<&Il2Entity> {
    let packed = pack_root(root);
    let copies = top_level_copies(packed);
    if looks_like_pack_here(packed) && !copies.is_empty() {
        copies
    } else {
        vec![packed]
    }
}

/// Classify each copy in a template or Random Ground Units pack.
pub fn inspect_army_copies(root: &Il2Entity) -> Vec<ArmyCopyInfo> {
    army_copy_refs(root)
        .into_iter()
        .map(|copy| {
            let (x, z) = copy.first_xz().unwrap_or((0.0, 0.0));
            ArmyCopyInfo {
                kind: crate::weapon_range::classify_army_unit(copy),
                range_m: crate::weapon_range::group_weapon_range(copy),
                x,
                z,
                route: mapnet::inspect_route(copy),
            }
        })
        .collect()
}

/// Park a loaded army (numbered copies, or the template itself) onto `spots`.
pub fn park_army_group(root: &mut Il2Entity, spots: &[(f64, f64)], headings: &[f64]) {
    let packed = pack_root_mut(root);
    let copy_count = top_level_copies(packed).len();
    if looks_like_pack_here(packed) && copy_count > 0 {
        park_recon_copies_headed(packed, spots, headings);
        return;
    }
    if let Some(&heading) = headings.first() {
        crate::placement::apply_group_heading(packed, heading);
    }
    let from = packed.first_xz().unwrap_or((0.0, 0.0));
    if let Some(&to) = spots.first() {
        crate::placement::move_anchor_to(packed, from, to);
    }
}

/// Park a loaded army using map preview spots (road/rail aware).
pub fn park_army_group_spots(root: &mut Il2Entity, spots: &[GroundSpot]) {
    let packed = pack_root_mut(root);
    let copy_count = top_level_copies(packed).len();
    if looks_like_pack_here(packed) && copy_count > 0 {
        park_recon_copies_spots(packed, spots);
        return;
    }
    if let Some(spot) = spots.first() {
        park_one_ground_copy(packed, spot);
    }
}

/// Park mixed ship + ground copies from a loaded army preview.
pub fn park_army_mixed(
    root: &mut Il2Entity,
    copies: &[ArmyCopyInfo],
    ships: &[(f64, f64, f64)],
    ground: &[GroundSpot],
) {
    let packed = pack_root_mut(root);
    let mut si = 0usize;
    let mut gi = 0usize;
    let mut apply = |child: &mut Il2Entity, kind: crate::weapon_range::ArmyUnitKind| {
        if kind == crate::weapon_range::ArmyUnitKind::Ship {
            if let Some(&(x, z, heading)) = ships.get(si) {
                crate::placement::apply_group_heading(child, heading);
                let from = child.first_xz().unwrap_or((0.0, 0.0));
                crate::placement::move_anchor_to(child, from, (x, z));
                si += 1;
            }
        } else if let Some(spot) = ground.get(gi) {
            park_one_ground_copy(child, spot);
            gi += 1;
        }
    };
    if looks_like_pack_here(packed) && top_level_copies(packed).len() > 0 {
        let mut ci = 0usize;
        for child in &mut packed.children {
            if !is_placed_copy(child) {
                continue;
            }
            if let Some(copy) = copies.get(ci) {
                apply(child, copy.kind);
            }
            ci += 1;
        }
        return;
    }
    if let Some(copy) = copies.first() {
        apply(packed, copy.kind);
    }
}

/// Snap ground AttackArea MCUs for a pack or a single template.
pub fn snap_army_attack_areas(root: &mut Il2Entity, objectives: &[Option<(f64, f64)>]) {
    let packed = pack_root_mut(root);
    let copy_count = top_level_copies(packed).len();
    if looks_like_pack_here(packed) && copy_count > 0 {
        snap_copy_attack_areas(packed, objectives);
        return;
    }
    if let Some(Some((x, z))) = objectives.first().copied() {
        crate::weapon_range::snap_ground_attack_areas(packed, x, z);
    }
}

fn win_targets_for_copy(copy: &Il2Entity) -> Vec<i32> {
    let mut ids = mission_begin_targets(copy);
    if ids.is_empty() {
        if let Some(id) = copy.find_by_name("ENABLE / PULSE IN").and_then(|e| e.index) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        copy.for_each(&mut |e| {
            if e.block_type == "MCU_CheckZone" {
                if let Some(name) = e.name() {
                    if is_zone_in(name) {
                        if let Some(id) = e.index {
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
        });
    }
    ids
}

fn groups_from_percent(n: usize, percent: u32) -> Vec<Vec<usize>> {
    let wanted = wanted_winners(n, percent.clamp(1, 100));
    let sizes = partition_sizes(n, wanted);
    let mut groups = Vec::with_capacity(sizes.len());
    let mut slot = 0usize;
    for sz in sizes {
        groups.push((slot..slot + sz).collect::<Vec<usize>>());
        slot += sz;
    }
    groups
}

struct RandomizerChain {
    groups: Vec<Vec<usize>>,
    delay_s: f64,
}

fn remap_groups(local: Vec<Vec<usize>>, offset: usize) -> Vec<Vec<usize>> {
    local
        .into_iter()
        .map(|g| g.into_iter().map(|i| i + offset).collect())
        .collect()
}

fn chains_from_block_sizes(
    sizes: &[usize],
    percent: u32,
    start_s: f64,
    gap_s: f64,
) -> Vec<RandomizerChain> {
    let delays = staggered_start_delays(sizes.len(), start_s, gap_s);
    let mut chains = Vec::with_capacity(sizes.len());
    let mut slot = 0usize;
    for (i, &n) in sizes.iter().enumerate() {
        if n == 0 {
            continue;
        }
        chains.push(RandomizerChain {
            groups: remap_groups(groups_from_percent(n, percent), slot),
            delay_s: delays.get(i).copied().unwrap_or(0.0),
        });
        slot += n;
    }
    chains
}

fn chains_from_type_percents(
    copies: &[Il2Entity],
    type_percents: &[(String, u32)],
    start_s: f64,
    gap_s: f64,
) -> Vec<RandomizerChain> {
    let mut used = vec![false; copies.len()];
    let mut chains = Vec::new();
    let mut type_i = 0usize;
    let delays = staggered_start_delays(type_percents.len() + 1, start_s, gap_s);
    for (type_name, pct) in type_percents {
        let idxs: Vec<usize> = copies
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                !used[*i] && copy_type_name(c.name().unwrap_or("")) == *type_name
            })
            .map(|(i, _)| i)
            .collect();
        for &i in &idxs {
            used[i] = true;
        }
        if idxs.is_empty() {
            continue;
        }
        let wanted = wanted_winners(idxs.len(), (*pct).clamp(1, 100));
        let sizes = partition_sizes(idxs.len(), wanted);
        let mut groups = Vec::new();
        let mut slot = 0usize;
        for sz in sizes {
            groups.push(idxs[slot..slot + sz].to_vec());
            slot += sz;
        }
        chains.push(RandomizerChain {
            groups,
            delay_s: delays.get(type_i).copied().unwrap_or(0.0),
        });
        type_i += 1;
    }
    let leftover: Vec<usize> = used
        .iter()
        .enumerate()
        .filter(|(_, taken)| !**taken)
        .map(|(i, _)| i)
        .collect();
    if !leftover.is_empty() {
        let local = groups_from_percent(leftover.len(), 50);
        let groups = local
            .into_iter()
            .map(|g| g.into_iter().map(|j| leftover[j]).collect())
            .collect();
        let delays = staggered_start_delays(type_i + 1, start_s, gap_s);
        chains.push(RandomizerChain {
            groups,
            delay_s: delays.get(type_i).copied().unwrap_or(0.0),
        });
    }
    chains
}

fn build_randomizer(
    copies: &[Il2Entity],
    win_targets: &[Vec<i32>],
    delete_objects: &[Vec<i32>],
    chains: &[RandomizerChain],
    proto_timer: &Il2Entity,
    proto_deact: Option<&Il2Entity>,
    proto_begin: Option<&Il2Entity>,
    proto_delete: Option<&Il2Entity>,
    next_id: &mut i32,
) -> Result<Il2Entity, String> {
    let total = copies.len();
    if total == 0 {
        return Err("no placed unit groups to randomize".into());
    }
    let groups: Vec<Vec<usize>> = chains.iter().flat_map(|c| c.groups.clone()).collect();
    if groups.is_empty() {
        return Err("no randomizer groups to build".into());
    }
    let max_group = groups.iter().map(|g| g.len()).max().unwrap_or(1);
    let delete_s = WATERFALL_STEP_S * (max_group as f64 + 1.0);

    let (gx, gy, gz) = copies
        .first()
        .and_then(first_pos)
        .unwrap_or((0.0, 0.0, 0.0));

    let mut begin = match proto_begin {
        Some(proto) => clone_named(proto, next_id, "Recon: Mission Begin"),
        None => synthesize_mcu("MCU_TR_MissionBegin", next_id, "Recon: Mission Begin"),
    };
    begin.set_property("Enabled", "1");
    begin.set_objects(vec![]);
    set_pos(&mut begin, gx, gy, gz);

    let named_inputs = chains.len() > 1;
    let mut inputs = Vec::with_capacity(chains.len());
    let mut delays: Vec<Option<Il2Entity>> = Vec::with_capacity(chains.len());
    for (ti, chain) in chains.iter().enumerate() {
        let input_name = if named_inputs {
            format!("Randomizer:INPUT {}", ti + 1)
        } else {
            "Randomizer:INPUT".to_string()
        };
        let mut input = clone_named(proto_timer, next_id, &input_name);
        input.set_property("Time", "0");
        input.set_property("Random", "100");
        input.set_objects(vec![]);
        set_pos(&mut input, gx, gy, gz);
        let delay = if chain.delay_s > 0.001 {
            let mut d = clone_named(
                proto_timer,
                next_id,
                &format!("{DELAY_MCU_NAME} {}", ti + 1),
            );
            d.set_property("Time", format_time(chain.delay_s));
            d.set_property("Random", "100");
            d.set_objects(vec![]);
            set_pos(&mut d, gx, gy, gz);
            Some(d)
        } else {
            None
        };
        inputs.push(input);
        delays.push(delay);
    }

    let mut randoms: Vec<Option<Il2Entity>> = (0..total).map(|_| None).collect();
    let mut outs: Vec<Option<Il2Entity>> = (0..total).map(|_| None).collect();
    let mut deletes: Vec<Option<Il2Entity>> = (0..total).map(|_| None).collect();
    let mut keeps: Vec<Option<Il2Entity>> = (0..total).map(|_| None).collect();
    let mut dumpers: Vec<Option<Il2Entity>> = (0..total).map(|_| None).collect();
    let mut closers = Vec::new();

    for group in groups.iter() {
        for (local, &i) in group.iter().enumerate() {
            let dx = (i % MCU_STAGGER_COLS) as f64 * 40.0;
            let dz = (i / MCU_STAGGER_COLS) as f64 * 40.0;
            let n = i + 1;
            let g = group.len();
            let pct = random_pct(local, g);
            let t = WATERFALL_STEP_S * (local + 1) as f64;
            let ms = (t * 1000.0).round() as i32;

            let mut rnd = clone_named(
                proto_timer,
                next_id,
                &format!("Random {}:{}% {ms}ms", n, pct),
            );
            rnd.set_property("Time", format_time(t));
            rnd.set_property("Random", pct.to_string());
            rnd.set_objects(vec![]);
            set_pos(&mut rnd, gx + dx, gy, gz + dz);
            randoms[i] = Some(rnd);

            let mut out_tmr = clone_named(proto_timer, next_id, &format!("Out {n}"));
            out_tmr.set_property("Time", "0");
            out_tmr.set_property("Random", "100");
            out_tmr.set_objects(vec![]);
            set_pos(&mut out_tmr, gx + dx, gy, gz + dz + 30.0);
            outs[i] = Some(out_tmr);

            if local + 1 < g {
                let mut closer = match proto_deact {
                    Some(proto) => clone_named(proto, next_id, "Close_Remaining_Output(s)"),
                    None => synthesize_mcu(
                        "MCU_Deactivate",
                        next_id,
                        "Close_Remaining_Output(s)",
                    ),
                };
                closer.set_objects(vec![]);
                set_pos(&mut closer, gx + dx, gy, gz + dz + 45.0);
                closers.push(closer);
            }

            let mut delete_tmr = clone_named(
                proto_timer,
                next_id,
                &format!("Delete unused {n}"),
            );
            delete_tmr.set_property("Time", format_time(delete_s));
            delete_tmr.set_property("Random", "100");
            delete_tmr.set_objects(vec![]);
            set_pos(&mut delete_tmr, gx + dx, gy, gz + dz + 60.0);
            deletes[i] = Some(delete_tmr);

            let mut keep = match proto_deact {
                Some(proto) => clone_named(proto, next_id, &format!("Keep {n}")),
                None => synthesize_mcu("MCU_Deactivate", next_id, &format!("Keep {n}")),
            };
            keep.set_objects(vec![]);
            set_pos(&mut keep, gx + dx, gy, gz + dz + 75.0);
            keeps[i] = Some(keep);

            let mut dump = match proto_delete {
                Some(proto) => clone_named(proto, next_id, &format!("Dump unused {n}")),
                None => synthesize_mcu("MCU_Delete", next_id, &format!("Dump unused {n}")),
            };
            dump.set_targets(vec![]);
            dump.set_objects(delete_objects[i].clone());
            set_pos(&mut dump, gx + dx, gy, gz + dz + 90.0);
            dumpers[i] = Some(dump);
        }
    }

    let unwrap_mcu = |slots: Vec<Option<Il2Entity>>, kind: &str| -> Result<Vec<Il2Entity>, String> {
        slots
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                e.ok_or_else(|| format!("copy {i} was not assigned a {kind} slot"))
            })
            .collect()
    };
    let mut randoms = unwrap_mcu(randoms, "random")?;
    let mut outs = unwrap_mcu(outs, "out")?;
    let mut deletes = unwrap_mcu(deletes, "delete")?;
    let mut keeps = unwrap_mcu(keeps, "keep")?;
    let dumpers = unwrap_mcu(dumpers, "dump")?;

    let random_ids: Vec<i32> = randoms.iter().filter_map(|e| e.index).collect();
    let out_ids: Vec<i32> = outs.iter().filter_map(|e| e.index).collect();
    let delete_ids: Vec<i32> = deletes.iter().filter_map(|e| e.index).collect();
    let keep_ids: Vec<i32> = keeps.iter().filter_map(|e| e.index).collect();
    let dump_ids: Vec<i32> = dumpers.iter().filter_map(|e| e.index).collect();
    let closer_ids: Vec<i32> = closers.iter().filter_map(|e| e.index).collect();

    let mut closer_i = 0usize;
    for group in &groups {
        let g = group.len();
        for local in 0..g {
            let i = group[local];
            let mut out_targets = win_targets[i].clone();
            out_targets.push(keep_ids[i]);
            outs[i].set_targets(out_targets);
            keeps[i].set_targets(vec![delete_ids[i]]);
            deletes[i].set_targets(vec![dump_ids[i]]);
            if local + 1 == g {
                randoms[i].set_targets(vec![out_ids[i]]);
            } else {
                randoms[i].set_targets(vec![out_ids[i], closer_ids[closer_i]]);
                let rest: Vec<i32> = group[local + 1..].iter().map(|&j| out_ids[j]).collect();
                closers[closer_i].set_targets(rest);
                closer_i += 1;
            }
        }
    }

    let mut begin_targets = Vec::new();
    for (ti, chain) in chains.iter().enumerate() {
        let copy_is: Vec<usize> = chain.groups.iter().flatten().copied().collect();
        let mut input_targets: Vec<i32> = copy_is.iter().map(|&i| random_ids[i]).collect();
        input_targets.extend(copy_is.iter().map(|&i| delete_ids[i]));
        inputs[ti].set_targets(input_targets);
        if let Some(ref mut delay_mcu) = delays[ti] {
            begin_targets.push(delay_mcu.index.unwrap());
            delay_mcu.set_targets(vec![inputs[ti].index.unwrap()]);
        } else {
            begin_targets.push(inputs[ti].index.unwrap());
        }
    }
    begin.set_targets(begin_targets);

    let mut randomizer = Il2Entity::new("Group");
    randomizer.index = Some(*next_id);
    randomizer.set_property("Index", next_id.to_string());
    *next_id += 1;
    randomizer.set_name(RANDOMIZER_NAME);
    randomizer.set_property("Desc", "\"\"");
    randomizer.children.push(begin);
    for delay_mcu in delays.into_iter().flatten() {
        randomizer.children.push(delay_mcu);
    }
    randomizer.children.extend(inputs);
    randomizer.children.extend(randoms);
    randomizer.children.extend(outs);
    randomizer.children.extend(closers);
    randomizer.children.extend(deletes);
    randomizer.children.extend(keeps);
    randomizer.children.extend(dumpers);
    Ok(randomizer)
}

/// Remove every `Recon Randomizer` group. Placed unit copies stay where they are.
pub fn strip_randomizer(root: &mut Il2Entity) -> usize {
    let before = root.children.len();
    root.children
        .retain(|c| c.name() != Some(RANDOMIZER_NAME));
    let mut n = before - root.children.len();
    for child in &mut root.children {
        n += strip_randomizer(child);
    }
    n
}

/// MCU names Mission Begin can fire after the randomizer is stripped.
/// Timers that target a Closer checkzone (and `ENABLE / PULSE IN`) are recommended.
pub fn restore_start_choices(root: &Il2Entity) -> Vec<RestoreChoice> {
    let mut by_id: Vec<(i32, String, String, Vec<i32>, Option<i32>)> = Vec::new();
    root.for_each(&mut |e| {
        let Some(index) = e.index else {
            return;
        };
        if e.block_type != "MCU_Timer" && e.block_type != "MCU_CheckZone" {
            return;
        }
        let name = e.name().unwrap_or("").trim().to_string();
        if name.is_empty() {
            return;
        }
        let closer = e
            .property("Closer")
            .and_then(|v| v.parse::<i32>().ok());
        by_id.push((index, e.block_type.clone(), name, e.targets.clone(), closer));
    });
    let closer_ids: std::collections::HashSet<i32> = by_id
        .iter()
        .filter(|(_, kind, _, _, closer)| kind == "MCU_CheckZone" && *closer == Some(1))
        .map(|(id, _, _, _, _)| *id)
        .collect();
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for (_id, kind, name, targets, closer) in &by_id {
        if seen.iter().any(|n| n == name) {
            continue;
        }
        seen.push(name.clone());
        if *kind == "MCU_Timer" {
            let pulse = name.eq_ignore_ascii_case("enable / pulse in");
            let linked: Vec<&str> = by_id
                .iter()
                .filter(|(zid, k, _, _, _)| {
                    *k == "MCU_CheckZone" && closer_ids.contains(zid) && targets.contains(zid)
                })
                .map(|(_, _, n, _, _)| n.as_str())
                .collect();
            let recommended = pulse || !linked.is_empty();
            let hint = if pulse && !linked.is_empty() {
                format!("usual start — timer targets Closer checkzone \"{}\"", linked[0])
            } else if pulse {
                "usual start timer (ENABLE / PULSE IN)".into()
            } else if !linked.is_empty() {
                format!("timer targets Closer checkzone \"{}\"", linked[0])
            } else {
                "timer".into()
            };
            out.push(RestoreChoice {
                name: name.clone(),
                kind: RestoreKind::Timer,
                recommended,
                hint,
            });
        } else {
            let is_closer = *closer == Some(1);
            let hint = if is_closer {
                "Closer checkzone — prefer a timer that targets this zone".into()
            } else {
                "checkzone (not Closer)".into()
            };
            out.push(RestoreChoice {
                name: name.clone(),
                kind: RestoreKind::CheckZone,
                recommended: false,
                hint,
            });
        }
    }
    out.sort_by(|a, b| {
        b.recommended
            .cmp(&a.recommended)
            .then_with(|| {
                (a.kind == RestoreKind::CheckZone).cmp(&(b.kind == RestoreKind::CheckZone))
            })
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Drop the randomizer and point each copy's Mission Begin at the chosen start MCU (by name).
pub fn restore_always_on(
    root: &mut Il2Entity,
    type_starts: &[(String, String)],
) -> Result<usize, String> {
    strip_randomizer(root);
    let pack = pack_root_mut(root);
    let mut next_id = pack.max_index().saturating_add(1);
    let mut n = 0usize;
    let mut missing = Vec::new();
    for copy in pack.children.iter_mut().filter(|c| is_placed_copy(c)) {
        let ty = copy_type_name(copy.name().unwrap_or("Unit"));
        let Some((_, start_name)) = type_starts.iter().find(|(t, _)| *t == ty) else {
            missing.push(format!("{ty} has no start MCU selected"));
            continue;
        };
        match restore_copy_start(copy, start_name, &mut next_id) {
            Ok(()) => n += 1,
            Err(err) => missing.push(err),
        }
    }
    if n == 0 {
        return Err(if missing.is_empty() {
            "no unit groups to restore".into()
        } else {
            missing.join("; ")
        });
    }
    pack.set_name(&format!("Ground Units {n}"));
    if !missing.is_empty() {
        return Err(format!(
            "restored {n} copies, but some failed: {}",
            missing.join("; ")
        ));
    }
    Ok(n)
}

fn restore_copy_start(
    copy: &mut Il2Entity,
    start_name: &str,
    next_id: &mut i32,
) -> Result<(), String> {
    let mut start_id = None;
    copy.for_each(&mut |e| {
        if start_id.is_none() && e.name() == Some(start_name) {
            start_id = e.index;
        }
    });
    let id = start_id.ok_or_else(|| {
        format!(
            "{} has no MCU named \"{start_name}\"",
            copy.name().unwrap_or("copy")
        )
    })?;
    let mut begins = 0usize;
    copy.for_each_mut(&mut |e| {
        if e.block_type == "MCU_TR_MissionBegin" {
            e.set_property("Enabled", "1");
            e.set_targets(vec![id]);
            begins += 1;
        }
    });
    if begins == 0 {
        let mut begin = synthesize_mcu("MCU_TR_MissionBegin", next_id, "Translator Mission Begin");
        begin.set_targets(vec![id]);
        if let Some((x, y, z)) = first_pos(copy) {
            set_pos(&mut begin, x, y, z);
        }
        copy.children.push(begin);
    }
    Ok(())
}

/// Rebuild the selector on an already-placed pack. Does not move copies.
pub fn apply_randomizer(
    root: &mut Il2Entity,
    percent: u32,
    group_delay_s: f64,
) -> Result<usize, String> {
    apply_randomizer_inner(root, None, percent, 0.0, group_delay_s)
}

/// Same as apply, but each unit type keeps its own activate ratio.
pub fn apply_randomizer_typed(
    root: &mut Il2Entity,
    type_percents: &[(String, u32)],
    start_delay_s: f64,
    group_delay_s: f64,
) -> Result<usize, String> {
    apply_randomizer_inner(root, Some(type_percents), 50, start_delay_s, group_delay_s)
}

fn apply_randomizer_inner(
    root: &mut Il2Entity,
    type_percents: Option<&[(String, u32)]>,
    percent: u32,
    start_delay_s: f64,
    group_delay_s: f64,
) -> Result<usize, String> {
    strip_randomizer(root);
    let pack = pack_root_mut(root);
    let copy_idx: Vec<usize> = pack
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| is_placed_copy(c))
        .map(|(i, _)| i)
        .collect();
    if copy_idx.is_empty() {
        return Err(
            "no placed unit groups found — load a Random Ground Units file exported from the editor"
                .into(),
        );
    }
    let owned: Vec<Il2Entity> = copy_idx.iter().map(|&i| pack.children[i].clone()).collect();
    let win_targets: Vec<Vec<i32>> = owned.iter().map(win_targets_for_copy).collect();
    if let Some(i) = win_targets.iter().position(|t| t.is_empty()) {
        return Err(format!(
            "{} has no ENABLE / PULSE IN or Zone IN to fire on a win",
            owned[i].name().unwrap_or("copy")
        ));
    }
    let delete_objects: Vec<Vec<i32>> = owned.iter().map(world_object_ids).collect();
    let proto_timer = owned
        .iter()
        .find_map(|c| find_block(c, "MCU_Timer"))
        .cloned()
        .ok_or("placed groups have no MCU_Timer to clone for the randomizer")?;
    let proto_deact = owned
        .iter()
        .find_map(|c| find_block(c, "MCU_Deactivate"))
        .cloned();
    let proto_begin = owned
        .iter()
        .find_map(|c| find_block(c, "MCU_TR_MissionBegin"))
        .cloned();
    let proto_delete = owned
        .iter()
        .find_map(|c| find_block(c, "MCU_Delete"))
        .cloned();
    let chains = match type_percents {
        Some(types) => chains_from_type_percents(&owned, types, start_delay_s, group_delay_s),
        None => chains_from_block_sizes(&[owned.len()], percent, start_delay_s, group_delay_s),
    };
    let mut next_id = pack.max_index().saturating_add(1);
    let randomizer = build_randomizer(
        &owned,
        &win_targets,
        &delete_objects,
        &chains,
        &proto_timer,
        proto_deact.as_ref(),
        proto_begin.as_ref(),
        proto_delete.as_ref(),
        &mut next_id,
    )?;
    let n = copy_idx.len();
    pack.children.push(randomizer);
    Ok(n)
}

fn is_zone_in(name: &str) -> bool {
    name.eq_ignore_ascii_case("zone in")
}

fn collect_checkzones(root: &Il2Entity) -> Vec<McuChoice> {
    let mut out = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_CheckZone" {
            if let Some(index) = e.index {
                out.push(McuChoice {
                    index,
                    name: e.name().unwrap_or("(unnamed)").to_string(),
                });
            }
        }
    });
    out
}

fn mission_begin_targets(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        if e.block_type == "MCU_TR_MissionBegin" {
            for t in &e.targets {
                if !ids.contains(t) {
                    ids.push(*t);
                }
            }
        }
    });
    ids
}

/// IL-2 still runs Mission Begin when Enabled = 0. Empty the target list.
fn silence_clone_starts(root: &mut Il2Entity) {
    root.for_each_mut(&mut |e| {
        if e.block_type == "MCU_TR_MissionBegin" {
            e.set_property("Enabled", "0");
            e.set_targets(vec![]);
        }
    });
}

fn world_object_ids(root: &Il2Entity) -> Vec<i32> {
    let mut ids = Vec::new();
    root.for_each(&mut |e| {
        if WORLD_TYPES.contains(&e.block_type.as_str()) {
            if let Some(id) = e.index {
                ids.push(id);
            }
        }
    });
    ids
}

fn find_index<'a>(root: &'a Il2Entity, index: i32) -> Option<&'a Il2Entity> {
    if root.index == Some(index) {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_index(c, index))
}

fn find_block<'a>(root: &'a Il2Entity, block_type: &str) -> Option<&'a Il2Entity> {
    if root.block_type == block_type {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_block(c, block_type))
}

fn first_pos(root: &Il2Entity) -> Option<(f64, f64, f64)> {
    if let (Some(x), Some(y), Some(z)) = (
        root.property("XPos").and_then(|v| v.parse().ok()),
        root.property("YPos").and_then(|v| v.parse().ok()),
        root.property("ZPos").and_then(|v| v.parse().ok()),
    ) {
        return Some((x, y, z));
    }
    root.children.iter().find_map(first_pos)
}

fn clone_named(proto: &Il2Entity, next_id: &mut i32, name: &str) -> Il2Entity {
    let (mut cloned, _) = duplicate_template(proto, next_id);
    cloned.set_name(name);
    cloned
}

fn synthesize_mcu(block_type: &str, next_id: &mut i32, name: &str) -> Il2Entity {
    let mut e = Il2Entity::new(block_type);
    e.index = Some(*next_id);
    e.set_property("Index", next_id.to_string());
    *next_id += 1;
    e.set_name(name);
    e.set_property("Desc", "\"\"");
    e.set_targets(vec![]);
    e.set_objects(vec![]);
    e.set_property("XPos", "0");
    e.set_property("YPos", "0");
    e.set_property("ZPos", "0");
    e.set_property("XOri", "0");
    e.set_property("YOri", "0");
    e.set_property("ZOri", "0");
    if block_type == "MCU_TR_MissionBegin" {
        e.set_property("Enabled", "1");
    }
    e
}

fn set_pos(entity: &mut Il2Entity, x: f64, y: f64, z: f64) {
    entity.set_property("XPos", format!("{x:.3}"));
    entity.set_property("YPos", format!("{y:.3}"));
    entity.set_property("ZPos", format!("{z:.3}"));
}

fn format_time(s: f64) -> String {
    if (s - s.round()).abs() < 1e-9 {
        format!("{:.0}", s)
    } else {
        format!("{:.1}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_group_file;
    use crate::serialize::serialize_group;
    use std::collections::HashSet;

    fn truck() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group"
        ))
        .expect("truck")
    }

    fn bm13() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/DropIns/DPRK MM13 Rocket Arty.Group"
        ))
        .expect("bm13")
    }

    fn input(key: &str, root: Il2Entity, copies: usize) -> ReconInput {
        let info = inspect_unit(&root).expect("inspect");
        ReconInput {
            label: key.into(),
            trigger_zone_ids: info.suggested_triggers.clone(),
            copies,
            root,
        }
    }

    fn debug_group() -> Il2Entity {
        parse_group_file(include_str!(
            "../TemplateExamples/GroundUnits/Debug.Group"
        ))
        .expect("debug")
    }

    #[test]
    fn allocate_splits_by_weight() {
        assert_eq!(allocate_copies(&[1, 1, 1, 1, 1], 100), vec![20, 20, 20, 20, 20]);
        assert_eq!(allocate_copies(&[50, 30, 20], 10), vec![5, 3, 2]);
        assert_eq!(allocate_copies(&[1, 0], 8), vec![8, 0]);
        let mix = allocate_mix(&[50, 50], 10, 50);
        assert_eq!(
            mix,
            vec![
                TypeMix {
                    copies: 5,
                    activate: 3
                },
                TypeMix {
                    copies: 5,
                    activate: 3
                }
            ]
        );
        assert_eq!(mix.iter().map(|m| m.activate).sum::<usize>(), 6);
        assert_ne!(wanted_winners(10, 50), 6);
    }

    #[test]
    fn inspect_truck_suggests_zone_in() {
        let info = inspect_unit(&truck()).unwrap();
        assert_eq!(info.suggested_triggers.len(), 1);
        let zone = info
            .checkzones
            .iter()
            .find(|z| z.index == info.suggested_triggers[0])
            .unwrap();
        assert!(is_zone_in(&zone.name));
        assert!(info.checkzones.iter().any(|z| z.name == "Zone OUT"));
        assert!(info.vehicle_count >= 5);
    }

    #[test]
    fn inspect_bm13_suggests_zone_in() {
        let info = inspect_unit(&bm13()).unwrap();
        assert!(!info.suggested_triggers.is_empty());
        let zone = info
            .checkzones
            .iter()
            .find(|z| z.index == info.suggested_triggers[0])
            .unwrap();
        assert!(is_zone_in(&zone.name));
    }

    #[test]
    fn wanted_and_waterfall_percents() {
        assert_eq!(wanted_winners(2, 50), 1);
        assert_eq!(wanted_winners(2, 100), 2);
        assert_eq!(wanted_winners(100, 20), 20);
        assert_eq!(wanted_winners(2, 20), 1);
        assert_eq!(random_pct(0, 1), 100);
        assert_eq!(random_pct(0, 2), 50);
        assert_eq!(random_pct(1, 2), 100);
        assert_eq!(random_pct(0, 4), 25);
        assert_eq!(random_pct(3, 4), 100);
        assert_eq!(partition_sizes(2, 1), vec![2]);
        assert_eq!(partition_sizes(2, 2), vec![1, 1]);
        assert_eq!(partition_sizes(10, 2), vec![5, 5]);
        assert_eq!(partition_sizes(3, 2), vec![2, 1]);
    }

    #[test]
    fn two_copies_fifty_percent_mutex_waterfall() {
        let out = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        let rnd = out.find_by_name("Recon Randomizer").unwrap();
        let begin = rnd.find_by_name("Recon: Mission Begin").unwrap();
        let input = rnd.find_by_name("Randomizer:INPUT").unwrap();
        assert_eq!(begin.targets, vec![input.index.unwrap()]);
        let r1 = rnd.find_by_name("Random 1:50% 500ms").unwrap();
        let r2 = rnd.find_by_name("Random 2:100% 1000ms").unwrap();
        let out1 = rnd.find_by_name("Out 1").unwrap();
        let out2 = rnd.find_by_name("Out 2").unwrap();
        assert_eq!(r1.property("Random"), Some("50"));
        assert_eq!(r1.property("Time"), Some("0.5"));
        assert_eq!(r2.property("Random"), Some("100"));
        assert_eq!(r2.property("Time"), Some("1"));
        assert!(input.targets.contains(&r1.index.unwrap()));
        assert!(input.targets.contains(&r2.index.unwrap()));
        assert!(r1.targets.contains(&out1.index.unwrap()));
        assert_eq!(r2.targets, vec![out2.index.unwrap()]);
        let closer_id = r1
            .targets
            .iter()
            .copied()
            .find(|id| *id != out1.index.unwrap())
            .expect("first random must close remaining outs");
        let closer = find_index(rnd, closer_id).unwrap();
        assert_eq!(closer.block_type, "MCU_Deactivate");
        assert_eq!(closer.targets, vec![out2.index.unwrap()]);
        let keep = rnd.find_by_name("Keep 1").unwrap().index.unwrap();
        let pulse = out
            .find_by_name("DPRK Truck Run [1]")
            .unwrap()
            .find_by_name("ENABLE / PULSE IN")
            .unwrap();
        assert!(out1.targets.contains(&keep));
        assert!(out1.targets.contains(&pulse.index.unwrap()));
        assert!(!r1.targets.contains(&keep), "win goes through Out, then Close remaining");
        assert!(rnd.find_by_name("Random 1:50% 500ms").is_some());
        assert!(rnd.find_by_name("Pick 1:50%").is_none());
    }

    #[test]
    fn ten_copies_chain_and_silence_mission_begin() {
        let out = generate_recon(&[input("truck", truck(), 10)], 20).unwrap();
        assert_eq!(out.name(), Some("Random Ground Units 10"));
        let rnd = out.find_by_name("Recon Randomizer").unwrap();
        let begin = rnd.find_by_name("Recon: Mission Begin").unwrap();
        assert_eq!(begin.property("Enabled"), Some("1"));
        assert_eq!(
            begin.targets.len(),
            1,
            "Mission Begin must not fan out to every timer"
        );
        let r1 = rnd.find_by_name("Random 1:20% 500ms").unwrap();
        assert_eq!(r1.property("Random"), Some("20"));
        let last = rnd.find_by_name("Random 5:100% 2500ms").unwrap();
        assert_eq!(last.property("Random"), Some("100"));
        let r6 = rnd.find_by_name("Random 6:20% 500ms").unwrap();
        assert_eq!(r6.property("Random"), Some("20"));
        assert!(rnd.find_by_name("1 Arm Zone IN").is_none());
        assert!(rnd.find_by_name("1 Disarm Zones").is_none());
        let dump = rnd.find_by_name("Dump unused 1").unwrap();
        assert!(dump.objects.len() >= 5);
        let clone1 = out.find_by_name("DPRK Truck Run [1]").unwrap();
        let pulse = clone1.find_by_name("ENABLE / PULSE IN").unwrap();
        let mut begins = 0;
        clone1.for_each(&mut |e| {
            if e.block_type == "MCU_TR_MissionBegin" {
                begins += 1;
                assert_eq!(e.property("Enabled"), Some("0"));
                assert!(e.targets.is_empty(), "clone Mission Begin must not fire");
            }
        });
        assert!(begins >= 1);
        let out1 = rnd.find_by_name("Out 1").unwrap();
        let keep = rnd.find_by_name("Keep 1").unwrap().index.unwrap();
        assert!(out1.targets.contains(&keep));
        assert!(
            out1.targets.contains(&pulse.index.unwrap()),
            "win must fire ENABLE / PULSE IN, not MCU_Activate Zone IN"
        );
        let closer_id = r1
            .targets
            .iter()
            .copied()
            .find(|id| *id != out1.index.unwrap())
            .unwrap();
        let closer = find_index(rnd, closer_id).unwrap();
        assert!(closer.targets.contains(&rnd.find_by_name("Out 5").unwrap().index.unwrap()));
        assert_eq!(last.targets, vec![rnd.find_by_name("Out 5").unwrap().index.unwrap()]);
    }

    #[test]
    fn debug_group_two_copies_fire_original_begin_chain() {
        let out = generate_recon(&[input("debug", debug_group(), 2)], 100).unwrap();
        assert_eq!(out.name(), Some("Random Ground Units 2"));
        let rnd = out.find_by_name("Recon Randomizer").unwrap();
        let r1 = rnd.find_by_name("Random 1:100% 500ms").unwrap();
        assert_eq!(r1.property("Random"), Some("100"));
        let out1 = rnd.find_by_name("Out 1").unwrap();
        let clone1 = out
            .find_by_name("DPRK Forces - Static Truck Line [1]")
            .unwrap();
        let pulse = clone1.find_by_name("ENABLE / PULSE IN").unwrap();
        assert!(
            out1.targets.contains(&pulse.index.unwrap()),
            "win must fire ENABLE / PULSE IN like the editor Mission Begin"
        );
        let keep = rnd.find_by_name("Keep 1").unwrap().index.unwrap();
        assert!(out1.targets.contains(&keep));
        assert_eq!(
            out1.targets.len(),
            3,
            "Debug.Group has two Mission Begins (Recc + ENABLE / PULSE IN) plus Keep"
        );
        assert_eq!(r1.targets, vec![out1.index.unwrap()]);
        let r2 = rnd.find_by_name("Random 2:100% 500ms").unwrap();
        assert_eq!(r2.property("Random"), Some("100"));
        let mut subtitles = 0;
        clone1.for_each(&mut |e| {
            if e.block_type == "MCU_TR_Subtitle" {
                subtitles += 1;
            }
            if e.block_type == "MCU_TR_MissionBegin" {
                assert!(e.targets.is_empty());
            }
        });
        assert!(subtitles >= 1);
        assert!(out
            .find_by_name("DPRK Forces - Static Truck Line [2]")
            .is_some());
        let text = serialize_group(&out);
        parse_group_file(&text).expect("reparse debug");
    }

    #[test]
    fn mixed_templates_unique_indexes() {
        let out = generate_recon(
            &[input("truck", truck(), 3), input("bm13", bm13(), 2)],
            20,
        )
        .unwrap();
        assert!(out.find_by_name("DPRK Truck Run [1]").is_some());
        assert!(out.find_by_name("Russian BM13 Artillery [4]").is_some());
        let mut ids = Vec::new();
        out.collect_indexes(&mut ids);
        let set: HashSet<i32> = ids.iter().copied().collect();
        assert_eq!(ids.len(), set.len());
        let text = serialize_group(&out);
        parse_group_file(&text).expect("reparse");
    }

    #[test]
    fn copies_are_offset_on_a_grid() {
        let out = generate_recon(&[input("truck", truck(), 2)], 20).unwrap();
        let a = out.find_by_name("DPRK Truck Run [1]").unwrap();
        let b = out.find_by_name("DPRK Truck Run [2]").unwrap();
        let (x1, _, z1) = first_pos(a).unwrap();
        let (x2, _, z2) = first_pos(b).unwrap();
        assert!((x1 - crate::placement::MAP_MIN).abs() < 1.0);
        assert!((z1 - crate::placement::MAP_MIN).abs() < 1.0);
        let dist = ((x2 - x1).powi(2) + (z2 - z1).powi(2)).sqrt();
        assert!((dist - crate::placement::GRID_STEP).abs() < 1.0);
    }

    #[test]
    fn each_template_gets_its_own_square() {
        let out = generate_recon(
            &[input("truck", truck(), 4), input("bm13", bm13(), 1)],
            20,
        )
        .unwrap();
        let t1 = first_pos(out.find_by_name("DPRK Truck Run [1]").unwrap()).unwrap();
        let arty = first_pos(out.find_by_name("Russian BM13 Artillery [5]").unwrap()).unwrap();
        let origins = crate::placement::template_square_origins(&[4, 1]);
        assert!((t1.0 - origins[0].0).abs() < 1.0);
        assert!((t1.2 - origins[0].1).abs() < 1.0);
        assert!((arty.0 - origins[1].0).abs() < 1.0);
        assert!((arty.2 - origins[1].1).abs() < 1.0);
        assert!(arty.0 - t1.0 > crate::placement::GRID_STEP);
    }

    #[test]
    fn keep_positions_leaves_template_coords() {
        let tmpl = first_pos(&truck()).unwrap();
        let out = generate_recon_ex(
            &[input("truck", truck(), 2)],
            ReconBuild {
                activate_percent: 50,
                keep_positions: true,
                start_delay_s: 0.0,
                group_delay_s: 0.0,
                spawn_all: false,
            },
        )
        .unwrap();
        let a = first_pos(out.find_by_name("DPRK Truck Run [1]").unwrap()).unwrap();
        let b = first_pos(out.find_by_name("DPRK Truck Run [2]").unwrap()).unwrap();
        assert!((a.0 - tmpl.0).abs() < 1.0);
        assert!((a.2 - tmpl.2).abs() < 1.0);
        assert!((b.0 - tmpl.0).abs() < 1.0);
        assert!((b.2 - tmpl.2).abs() < 1.0);
        assert!((a.0 - crate::placement::MAP_MIN).abs() > 1000.0);
    }

    #[test]
    fn group_delay_staggers_second_template() {
        let out = generate_recon_ex(
            &[input("truck", truck(), 1), input("bm13", bm13(), 1)],
            ReconBuild {
                activate_percent: 100,
                keep_positions: false,
                start_delay_s: 0.0,
                group_delay_s: 0.5,
                spawn_all: false,
            },
        )
        .unwrap();
        let rnd = out.find_by_name(RANDOMIZER_NAME).unwrap();
        let begin = rnd.find_by_name("Recon: Mission Begin").unwrap();
        let input1 = rnd.find_by_name("Randomizer:INPUT 1").unwrap();
        let input2 = rnd.find_by_name("Randomizer:INPUT 2").unwrap();
        let delay = rnd.find_by_name(&format!("{DELAY_MCU_NAME} 2")).unwrap();
        assert_eq!(delay.property("Time"), Some("0.5"));
        assert_eq!(delay.property("Random"), Some("100"));
        assert!(begin.targets.contains(&input1.index.unwrap()));
        assert!(begin.targets.contains(&delay.index.unwrap()));
        assert_eq!(delay.targets, vec![input2.index.unwrap()]);
        assert!(rnd.find_by_name("Randomizer:INPUT").is_none());
    }

    #[test]
    fn strip_keeps_copies_and_drops_randomizer() {
        let mut pack = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        assert!(pack.find_by_name(RANDOMIZER_NAME).is_some());
        assert_eq!(strip_randomizer(&mut pack), 1);
        assert!(pack.find_by_name(RANDOMIZER_NAME).is_none());
        assert!(pack.find_by_name("DPRK Truck Run [1]").is_some());
        assert!(pack.find_by_name("DPRK Truck Run [2]").is_some());
        assert_eq!(strip_randomizer(&mut pack), 0);
    }

    #[test]
    fn restore_choices_prefer_timers_that_target_closer_zones() {
        let choices = restore_start_choices(&truck());
        assert!(
            choices.iter().any(|c| c.name == "ENABLE / PULSE IN" && c.recommended),
            "ENABLE / PULSE IN should be recommended"
        );
        assert!(
            choices.iter().any(|c| c.name == "Trigger Timer" && c.recommended),
            "timer that targets a Closer checkzone should be recommended"
        );
        let zone = choices.iter().find(|c| c.name == "Zone IN").unwrap();
        assert!(!zone.recommended);
        assert_eq!(zone.kind, RestoreKind::CheckZone);
        assert!(zone.hint.contains("Closer"));
    }

    #[test]
    fn restore_always_on_reconnects_silenced_mission_begin() {
        let mut pack = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        let n = restore_always_on(
            &mut pack,
            &[("DPRK Truck Run".into(), "ENABLE / PULSE IN".into())],
        )
        .unwrap();
        assert_eq!(n, 2);
        assert!(pack.find_by_name(RANDOMIZER_NAME).is_none());
        assert_eq!(pack.name(), Some("Ground Units 2"));
        let copy = pack.find_by_name("DPRK Truck Run [1]").unwrap();
        let pulse = copy.find_by_name("ENABLE / PULSE IN").unwrap().index.unwrap();
        let mut begins = 0;
        copy.for_each(&mut |e| {
            if e.block_type == "MCU_TR_MissionBegin" {
                begins += 1;
                assert_eq!(e.property("Enabled"), Some("1"));
                assert_eq!(e.targets, vec![pulse]);
            }
        });
        assert!(begins >= 1);
        let text = serialize_group(&pack);
        parse_group_file(&text).expect("reparse restored pack");
    }

    #[test]
    fn apply_rebuilds_selector_without_moving_copies() {
        let mut pack = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        let before = first_pos(pack.find_by_name("DPRK Truck Run [1]").unwrap()).unwrap();
        strip_randomizer(&mut pack);
        let n = apply_randomizer(&mut pack, 100, 0.5).unwrap();
        assert_eq!(n, 2);
        let after = first_pos(pack.find_by_name("DPRK Truck Run [1]").unwrap()).unwrap();
        assert!((before.0 - after.0).abs() < 0.01);
        assert!((before.2 - after.2).abs() < 0.01);
        let rnd = pack.find_by_name(RANDOMIZER_NAME).unwrap();
        let begin = rnd.find_by_name("Recon: Mission Begin").unwrap();
        let input = rnd.find_by_name("Randomizer:INPUT").unwrap();
        assert_eq!(begin.targets, vec![input.index.unwrap()]);
        assert!(rnd.find_by_name(&format!("{DELAY_MCU_NAME} 1")).is_none());
        assert!(rnd.find_by_name("Random 1:100% 500ms").is_some());
        assert!(rnd.find_by_name("Random 2:100% 500ms").is_some());
        let pulse = pack
            .find_by_name("DPRK Truck Run [1]")
            .unwrap()
            .find_by_name("ENABLE / PULSE IN")
            .unwrap();
        let out1 = rnd.find_by_name("Out 1").unwrap();
        assert!(out1.targets.contains(&pulse.index.unwrap()));
        let text = serialize_group(&pack);
        parse_group_file(&text).expect("reparse applied pack");
    }

    #[test]
    fn inspect_placed_pack_accepts_generated_and_rejects_template() {
        let pack = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        let info = inspect_placed_pack(&pack).unwrap();
        assert_eq!(info.copy_count, 2);
        assert!(info.name.contains("Random Ground Units"));
        assert_eq!(info.types.len(), 1);
        assert_eq!(info.types[0].name, "DPRK Truck Run");
        assert_eq!(info.types[0].copy_count, 2);
        assert!(inspect_placed_pack(&truck()).is_err());
        let mut stripped = pack.clone();
        strip_randomizer(&mut stripped);
        assert_eq!(inspect_placed_pack(&stripped).unwrap().copy_count, 2);
    }

    #[test]
    fn copy_type_name_strips_generator_and_editor_suffixes() {
        assert_eq!(copy_type_name("DPRK Truck Run [1]"), "DPRK Truck Run");
        assert_eq!(copy_type_name("DPRK Truck Run *53*"), "DPRK Truck Run");
        assert_eq!(
            copy_type_name("DPRK Forces - Static Truck Line"),
            "DPRK Forces - Static Truck Line"
        );
        assert_eq!(
            copy_type_name("Russian BM13 Artillery *10*"),
            "Russian BM13 Artillery"
        );
    }

    #[test]
    fn inspect_biggroup_export_groups_types() {
        let pack = parse_group_file(include_str!("../TemplateExamples/BigGroup.Group"))
            .expect("BigGroup");
        let info = inspect_placed_pack(&pack).unwrap();
        assert_eq!(info.copy_count, 60);
        assert_eq!(placed_copy_count(&pack), 60);
        assert_eq!(info.name, "Random Ground Units 60");
        assert!(info.has_randomizer);
        assert!(info.types.iter().all(|t| !t.unit.checkzones.is_empty()));
        assert!(info
            .types
            .iter()
            .all(|t| !t.unit.suggested_triggers.is_empty()));
        let by_name = |n: &str| {
            info.types
                .iter()
                .find(|t| t.name == n)
                .unwrap_or_else(|| panic!("missing {n}"))
                .copy_count
        };
        assert_eq!(by_name("DPRK ML20 Artillery"), 9);
        assert_eq!(by_name("Russian BM13 Artillery"), 9);
        assert_eq!(by_name("Larger Shipos"), 6);
        assert_eq!(by_name("DPRK Forces - Static Truck Line"), 11);
        assert_eq!(by_name("DPRK Tank Company"), 6);
        assert_eq!(by_name("DPRK Tank Platoon"), 11);
        assert_eq!(by_name("DPRK Truck Run"), 8);
        assert_eq!(info.types.len(), 7);
    }

    #[test]
    fn combine_packs_merges_copies_and_skips_dropped_types() {
        let trucks = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        let mixed = generate_recon(
            &[input("truck", truck(), 1), input("bm13", bm13(), 2)],
            50,
        )
        .unwrap();
        let keep = vec!["DPRK Truck Run".into(), "Russian BM13 Artillery".into()];
        let out = combine_placed_packs(&[trucks, mixed.clone()], &keep).unwrap();
        assert_eq!(placed_copy_count(&out), 5);
        let info = inspect_placed_pack(&out).unwrap();
        assert_eq!(info.types.iter().find(|t| t.name == "DPRK Truck Run").unwrap().copy_count, 3);
        assert_eq!(
            info.types
                .iter()
                .find(|t| t.name == "Russian BM13 Artillery")
                .unwrap()
                .copy_count,
            2
        );
        assert!(out.find_by_name(RANDOMIZER_NAME).is_none());
        let mut ids = Vec::new();
        out.for_each(&mut |e| {
            if let Some(i) = e.index {
                ids.push(i);
            }
        });
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len());

        let trucks_only = combine_placed_packs(&[mixed], &["DPRK Truck Run".into()]).unwrap();
        assert_eq!(placed_copy_count(&trucks_only), 1);
        assert!(trucks_only.find_by_name("Russian BM13 Artillery [2]").is_none());
        assert!(trucks_only.find_by_name("Russian BM13 Artillery [3]").is_none());
    }

    #[test]
    fn apply_typed_keeps_per_type_activate_ratio() {
        let mut pack = generate_recon(
            &[input("truck", truck(), 2), input("bm13", bm13(), 2)],
            50,
        )
        .unwrap();
        let types = vec![
            ("DPRK Truck Run".into(), 50_u32),
            ("Russian BM13 Artillery".into(), 100_u32),
        ];
        assert_eq!(apply_randomizer_typed(&mut pack, &types, 0.0, 0.0).unwrap(), 4);
        let rnd = pack.find_by_name(RANDOMIZER_NAME).unwrap();
        assert!(rnd.find_by_name("Random 1:50% 500ms").is_some());
        assert!(rnd.find_by_name("Random 3:100% 500ms").is_some());
        assert!(rnd.find_by_name("Random 4:100% 500ms").is_some());
        let (x1, _, _) = first_pos(pack.find_by_name("DPRK Truck Run [1]").unwrap()).unwrap();
        assert!((x1 - crate::placement::MAP_MIN).abs() < 1.0);
    }

    #[test]
    fn group_start_delays_are_even_gaps() {
        assert_eq!(group_start_delays(0, 0.5), Vec::<f64>::new());
        assert_eq!(group_start_delays(1, 0.5), vec![0.0]);
        assert_eq!(group_start_delays(3, 0.5), vec![0.0, 0.5, 1.0]);
        assert_eq!(group_start_delays(2, 0.5)[1], 0.5);
        assert_eq!(staggered_start_delays(2, 2.0, 0.5), vec![2.0, 2.5]);
    }

    #[test]
    fn start_delay_holds_first_type_after_mission_begin() {
        let out = generate_recon_ex(
            &[input("truck", truck(), 1), input("bm13", bm13(), 1)],
            ReconBuild {
                activate_percent: 100,
                keep_positions: false,
                start_delay_s: 2.0,
                group_delay_s: 0.5,
                spawn_all: false,
            },
        )
        .unwrap();
        let rnd = out.find_by_name(RANDOMIZER_NAME).unwrap();
        let begin = rnd.find_by_name("Recon: Mission Begin").unwrap();
        let delay1 = rnd.find_by_name(&format!("{DELAY_MCU_NAME} 1")).unwrap();
        let delay2 = rnd.find_by_name(&format!("{DELAY_MCU_NAME} 2")).unwrap();
        let input1 = rnd.find_by_name("Randomizer:INPUT 1").unwrap();
        let input2 = rnd.find_by_name("Randomizer:INPUT 2").unwrap();
        assert_eq!(delay1.property("Time"), Some("2"));
        assert_eq!(delay2.property("Time"), Some("2.5"));
        assert!(begin.targets.contains(&delay1.index.unwrap()));
        assert!(begin.targets.contains(&delay2.index.unwrap()));
        assert!(!begin.targets.contains(&input1.index.unwrap()));
        assert_eq!(delay1.targets, vec![input1.index.unwrap()]);
        assert_eq!(delay2.targets, vec![input2.index.unwrap()]);
    }

    #[test]
    fn spawn_all_skips_randomizer_and_keeps_mission_begin() {
        let out = generate_recon_ex(
            &[input("truck", truck(), 2)],
            ReconBuild {
                activate_percent: 50,
                keep_positions: false,
                start_delay_s: 0.0,
                group_delay_s: 0.5,
                spawn_all: true,
            },
        )
        .unwrap();
        assert_eq!(out.name(), Some("Army 2"));
        assert!(out.find_by_name(RANDOMIZER_NAME).is_none());
        let clone1 = out.find_by_name("DPRK Truck Run [1]").unwrap();
        let mut begins = 0;
        clone1.for_each(&mut |e| {
            if e.block_type == "MCU_TR_MissionBegin" {
                begins += 1;
                assert_ne!(e.property("Enabled"), Some("0"));
                assert!(
                    !e.targets.is_empty(),
                    "spawn-all must leave Mission Begin connected so every copy starts"
                );
            }
        });
        assert!(begins >= 1);
        assert!(out.find_by_name("DPRK Truck Run [2]").is_some());
    }

    #[test]
    fn inspect_army_copies_classifies_template_and_pack() {
        let truck_info = inspect_army_copies(&truck());
        assert_eq!(truck_info.len(), 1);
        assert_eq!(truck_info[0].kind, crate::weapon_range::ArmyUnitKind::Supply);
        assert!(truck_info[0].route.as_ref().is_some_and(|r| !r.rail));

        let arty_info = inspect_army_copies(&bm13());
        assert_eq!(arty_info.len(), 1);
        assert_eq!(arty_info[0].kind, crate::weapon_range::ArmyUnitKind::Artillery);

        let pack = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        let copies = inspect_army_copies(&pack);
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|c| c.kind == crate::weapon_range::ArmyUnitKind::Supply));
    }

    #[test]
    fn park_army_group_moves_template_and_pack_copies() {
        let mut tmpl = truck();
        park_army_group(&mut tmpl, &[(100_000.0, 200_000.0)], &[90.0]);
        let (x, z) = tmpl.first_xz().unwrap();
        assert!((x - 100_000.0).abs() < 1.0);
        assert!((z - 200_000.0).abs() < 1.0);

        let mut pack = generate_recon(&[input("truck", truck(), 2)], 50).unwrap();
        park_army_group(
            &mut pack,
            &[(110_000.0, 210_000.0), (120_000.0, 220_000.0)],
            &[0.0, 0.0],
        );
        snap_army_attack_areas(
            &mut pack,
            &[Some((1.0, 2.0)), Some((3.0, 4.0))],
        );
        let a = pack.find_by_name("DPRK Truck Run [1]").unwrap().first_xz().unwrap();
        let b = pack.find_by_name("DPRK Truck Run [2]").unwrap().first_xz().unwrap();
        assert!((a.0 - 110_000.0).abs() < 1.0);
        assert!((b.0 - 120_000.0).abs() < 1.0);
    }
}
