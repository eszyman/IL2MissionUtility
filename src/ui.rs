//! # ui.rs — egui front-end for the IL-2 Group Generator
//!
//! The GUI entry point. Boots the eframe window, holds all UI state in
//! [`GroupGeneratorApp`], and routes the six mode tabs (Template, Army
//! Generator, Fighter Pack, Exclusive Activation, Airfield, Map) to their
//! panels. The detached Help viewport lives in [`crate::help`]; this file
//! is otherwise the only egui code.
//!
//! Presentation only: no AST parsing and no group generation live here. This
//! file talks to the rest of the crate strictly through public APIs —
//! [`crate::parser`] loads files, [`crate::template`] / [`crate::pack`] /
//! [`crate::flights`] / [`crate::bombers`] / [`crate::recon`] /
//! [`crate::frontlines`] / [`crate::airfield`] build them, and
//! [`crate::serialize`] writes them. Anything that needs the AST does so in
//! those modules, not here.
//!
//! ## Layout of this file
//! * `run()` — eframe bootstrap (window size, readable style).
//! * Mode enums: [`AppMode`], [`ReconSubmode`], [`MapDrawingMode`],
//!   [`DrawnMark`].
//! * Slot structs: [`BomberSlot`], [`ReconSlot`], [`MapArmySlot`] — a loaded
//!   file plus the per-file selections the user makes (triggers, unit kind,
//!   reposition).
//! * [`GroupGeneratorApp`] — all state plus `eframe::App::update`; one method
//!   per panel/section (`template_*`, `recon_*`, `fighter_*`, `map_*`, …).
//! * Free functions — shared widgets (order/event tree chips, tree line
//!   painting, icon buttons), map coordinate conversions (`uv_to_world`,
//!   `world_to_uv`, `world_to_pos`), drawing primitives (lines, labels,
//!   arrows, salient anchors), asset loading (SVG icons, Korea map JPEGs),
//!   and `save_with_sidecars` (writes the group file plus merged
//!   translation sidecars).
//!
//! ## Conventions
//! * World coordinates are game meters: X is north (up on the map), Z is
//!   east. The map image uses UV and the screen uses `Pos2`; convert only
//!   through the functions at the bottom of this file.
//! * Drawn-mark undo/redo: `drawn_marks` is the stack of what exists; the
//!   `redo_*` stacks mirror it for Ctrl-Y. Any new drawing clears the redo
//!   stacks.
//! * `status` (`Status`) is the single bottom line: `Info`/`Warn` are soft
//!   placement notes (orange), `Error` is a hard failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, FontFamily, FontId, Layout, Pos2, Rect, RichText,
    Sense, Stroke, TextStyle, TextureHandle, Vec2,
};

use crate::aircraft::{
    default_skill, fighter_pack_filename, linked_fighter_pack_name, AIRCRAFT_TYPES, COUNTRIES,
};
use crate::airfield::{
    clean_airfield, inspect_airfield, AirfieldInfo, EASTERN_PLANE_COALITIONS,
    WESTERN_PLANE_COALITIONS,
};
use crate::bombers::{
    extract_exclusive_plans, inspect_plan, link_bomber_plans_with, looks_like_exclusive_pack,
    BomberInput, BomberPlanInfo, SUGGESTED_END_NAMES, SUGGESTED_TRIGGER_NAMES,
};
use crate::duplicate::apply_overrides;
use crate::flights::{configure_aircraft, FlightConfig};
use crate::frontlines::{
    attack_arrow_points, battles_in_period, generate_front, inspect_base_map, looks_like_base_map,
    mark_for_battle, preview_dots, preview_front_xz, suggested_aircraft, timeline_index,
    timeline_preview, Battle, FrontOptions, ImportedFighterPack, MapFighterPack, MapGroundPack,
    MapRefGroup, MapShipPack, PreviewKind, Season, TimelineMark, ARROW_TAIL_WIDTH, BATTLES,
    PLACE_MARGIN, TIMELINE, YEARS,
};
use crate::geo::{self, MAP_MAX, MAP_MIN};
use crate::harvest::{
    default_missions_dir, find_gen_file, harvest_file, GenWatcher, HarvestConfig,
    HarvestOutcome, DEFAULT_DB_DIR,
};
use crate::heightprobe;
use crate::help::{self, HelpTopic};
use crate::shell::{self, Severity};
use crate::terrain::HeightStore;
use crate::theme::{self, c};

/// Native file dialogs. Tests swap in `ui_tests::dialog`, which answers
/// from a queue of prepared paths instead of opening a window.
#[cfg(not(test))]
mod dialog {
    pub use rfd::FileDialog;
}
#[cfg(test)]
use ui_tests::dialog;
#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;

/// Terrain lattice spacing in metres (terrain::STEP_M).
const TERRAIN_STEP: f64 = crate::terrain::STEP_M;
use crate::locale::{has_sidecars, merge_template_sidecars, write_sidecars, LANG_EXTS};
use crate::mapclip::{
    apply_salients, can_extend_salient, can_extend_west_east, clip_linestring_to_rect,
    clip_polyline_to_aabb, clip_ring_to_aabb, linestring_to_points, point_north_of_front,
    points_to_linestring, snap_to_front, stroke_self_intersects, WorldAabb, FRONT_PLACE_BAND,
};
use crate::mapfighters::{
    country_for_coalition, place_in_coalition, rtb_ao_point, FighterSpot, MapFighterLayout, MAX_PACKS,
};
use crate::mapground::{
    numbered_ground_issues, place_ground_jobs, GroundJob, GroundKind, GroundSpot, MapGroundLayout,
    GROUP_DELAY_S as GROUND_GROUP_DELAY_S, START_DELAY_S as GROUND_START_DELAY_S,
    ARTY_OBJECTIVE_RADIUS,
};
use crate::mapnet;
use crate::mapshipping::{place_ships, MapShipLayout, ShipSpot, GROUP_DELAY_S, START_DELAY_S};
use crate::model_spec::{self, ModelClass};
use crate::payloads;
use crate::placement::PlaceOpts;
use crate::pack::{builtin_template, generate_pack, generate_pack_at, group_anchor_xz, park_rtbs, zone_in_radius};
use crate::parser::{parse_group_file, parse_il2_document};
use crate::recon::{
    allocate_copies, allocate_mix, apply_randomizer_typed, combine_placed_packs, generate_recon_ex,
    inspect_army_copies, inspect_placed_pack, inspect_unit, looks_like_placed_pack,
    park_army_mixed, park_recon_copies_headed, park_recon_copies_spots,
    restore_always_on, snap_army_placed_attack_areas, snap_placed_attack_areas,
    wanted_winners, ArmyCopyInfo, ReconBuild, ReconInput, RestoreKind,
    TypeMix, UnitPlanInfo, SUGGESTED_ZONE_NAMES,
};
use crate::serialize::serialize_group;
use crate::template::{
    append_seat, apply_formation_numbers, apply_plane_start,
    apply_suggested_attack_area, bundled_catalog, AIR_START_ALTITUDE_M,
    copy_seat_attributes, flight_lead_of, formation_label, formations_for, generate_template,
    has_linked_wingmen, is_follower, lead_indexes, load_catalog, load_catalog_as_user_added,
    load_template, insert_goto_waypoint_after, merge_catalog, move_seat, next_waypoint_number,
    event_triggers_order, normalize_order_chain, order_seat_indexes, order_tree_layout,
    path_waypoint_display_m,
    place_offset, receives_orders, refresh_attack_areas_for_seat, remap_event_then,
    remap_index_vec, remap_seat_index, replace_seat_unit, set_report_following,
    used_waypoint_count, waypoint_area_m, waypoint_display_altitude, waypoint_display_priority,
    zone_defaults,
    zone_mix_for_seats, visual_range_m, near_visual_range, AIR_ZONE_IN_M, AIR_ZONE_OUT_M,
    TRAIN_ZONE_IN_M, BringUp, CatalogUnit, EntityEvent, EventHook, EventThen, FlightRole,
    OrderKind, OrderSpec, OrderTreeNode, PlaceLayout, PlaneStart, TemplateOptions, TemplateSeat,
    UnitKind as CatalogKind, ZoneCoalition, ZoneMix, DEFAULT_TIME_ON_TARGET_S, PLACEMENT_SPACING,
    WAYPOINT_SPACING_M, attack_area_range_limit, carriage_label, catalog_carriage_scripts,
    AttackAreaTarget, DEFAULT_TIMER_S, order_chip_detail, priority_label,
};
use crate::weapon_range::{self, ArmyUnitKind};

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 1280.0])
            .with_min_inner_size([1280.0, 800.0])
            .with_decorations(true),
        ..Default::default()
    };
    eframe::run_native(
        "IL-2 Group Generator",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            let mut app = GroupGeneratorApp::default();
            app.mark_saved(AppMode::Template);
            app.mark_saved(AppMode::Map);
            Ok(Box::new(app))
        }),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppMode {
    Template,
    Fighter,
    Exclusive,
    Recon,
    Airfield,
    Map,
}

/// Index of `mode` in `MODES` (the per-tab status slots follow it).
fn mode_slot(mode: AppMode) -> usize {
    MODES.iter().position(|(m, _)| *m == mode).unwrap_or(0)
}

/// Rail order (docs/ui-redesign/README.md §4). Ctrl 1–6 follow it.
const MODES: [(AppMode, &str); 6] = [
    (AppMode::Template, "Template"),
    (AppMode::Recon, "Army Generator"),
    (AppMode::Fighter, "Fighter Pack"),
    (AppMode::Exclusive, "Exclusive Activation"),
    (AppMode::Airfield, "Airfield"),
    (AppMode::Map, "Map"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReconSubmode {
    New,
    Rework,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapDrawingMode {
    None,
    BaseFront,
    Salient,
    AttackArrow,
    PlaceEastObjective,
    PlaceNatoObjective,
}

/// Map tool palette, top to bottom; keys 1–6 pick them (README §5.6).
/// (mode, icon, name, banner / status hint)
const MAP_TOOLS: [(MapDrawingMode, shell::ToolIcon, &str, &str); 6] = [
    (MapDrawingMode::None, shell::ToolIcon::Select, "Select AO / move units", "Drag a box for the AO, or drag a unit"),
    (MapDrawingMode::BaseFront, shell::ToolIcon::Front, "Draw front", "Click west to east, or drag · Esc to cancel"),
    (MapDrawingMode::Salient, shell::ToolIcon::Salient, "Salient", "Click along the front · right-click to finish · Esc to cancel"),
    (MapDrawingMode::AttackArrow, shell::ToolIcon::Arrow, "Attack arrow", "Drag from tail to tip · Esc to cancel"),
    (MapDrawingMode::PlaceEastObjective, shell::ToolIcon::Objective, "DPRK objective", "Click to place · Shift for more"),
    (MapDrawingMode::PlaceNatoObjective, shell::ToolIcon::Objective, "NATO objective", "Click to place · Shift for more"),
];

/// What Template Reset / Remove / Load can take away; restored by Ctrl Z.
struct TemplateSnapshot {
    seats: Vec<TemplateSeat>,
    select: Option<TplSelect>,
    bring_up: BringUp,
    spawn_reset: bool,
    spawn_cooldown_min: f32,
    place_layout: PlaceLayout,
    per_group: u32,
    zone_in: f32,
    zone_out: f32,
    zone_mix: Option<ZoneMix>,
    wp_spacing: f32,
    wp_speed: f32,
    wp_altitude: f32,
    wp_priority: i32,
    zone_coalition: ZoneCoalition,
    loaded_path: Option<PathBuf>,
}

/// What a Map Clear or Remove can take away (README §6.3).
struct MapForces {
    ships: Option<MapShipLayout>,
    ground_east: Option<MapGroundLayout>,
    ground_nato: Option<MapGroundLayout>,
    armies: Vec<MapArmySlot>,
    fighters: Option<MapFighterLayout>,
    imported_fighters: Vec<ImportedFighterPack>,
    east_objectives: Vec<(f64, f64)>,
    nato_objectives: Vec<(f64, f64)>,
    refs: Vec<MapRefGroup>,
    /// Drawn lines, only for Clear lines / salients / arrows; restored only when set.
    lines: Option<MapLines>,
}

/// What Clear lines / salients / arrows can take away.
struct MapLines {
    custom_front: Vec<(f64, f64)>,
    salients: Vec<Vec<(f64, f64)>>,
    attack_arrows: Vec<((f64, f64), (f64, f64))>,
    drawn_marks: Vec<DrawnMark>,
}

/// The last undoable Map action, so Ctrl Z and the status bar's Undo label
/// agree (README §6.3): a drawing (drawn-mark undo) or a Clear / Remove
/// (the `map_undo` snapshot).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MapAction {
    Drawing,
    Clear,
}

/// A destructive action waiting for its confirmation dialog.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    ResetTemplate,
    ResetFighter,
    LoadTemplate,
    LoadBaseMap,
}

/// Tabs of the Map tab's right dock.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MapDock {
    Period,
    Forces,
    References,
    Terrain,
}

impl MapDock {
    /// The dock's tab strip; References carries its count.
    fn tabs(refs: usize) -> [(MapDock, &'static str, Option<usize>); 4] {
        [
            (MapDock::Period, "Period", None),
            (MapDock::Forces, "Forces", None),
            (MapDock::References, "References", Some(refs)),
            (MapDock::Terrain, "Terrain", None),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DrawnMark {
    Salient,
    AttackArrow,
    /// One Draw-front click or drag; undo truncates `custom_front_xz` back to `prev_len`.
    Front { prev_len: usize },
}

impl DrawnMark {
    fn is_front(self) -> bool {
        matches!(self, DrawnMark::Front { .. })
    }

    /// Status-bar undo label for this drawing.
    fn label(self) -> &'static str {
        match self {
            DrawnMark::Salient => "Drew a salient",
            DrawnMark::AttackArrow => "Drew an attack arrow",
            DrawnMark::Front { .. } => "Drew front line",
        }
    }
}

#[derive(Clone)]
struct BomberSlot {
    path: PathBuf,
    root: crate::ast::Il2Entity,
    info: BomberPlanInfo,
    selected_triggers: Vec<i32>,
    selected_completion: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitKind {
    Ship,
    Armor,
    Supply,
    Artillery,
    Infantry,
    Train,
}

impl UnitKind {
    const ALL: [Self; 6] = [
        Self::Ship,
        Self::Armor,
        Self::Supply,
        Self::Artillery,
        Self::Infantry,
        Self::Train,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Ship => "Ship",
            Self::Armor => "Armor",
            Self::Supply => "Supply",
            Self::Artillery => "Artillery",
            Self::Infantry => "Infantry",
            Self::Train => "Train",
        }
    }

    fn terrain_hint(self) -> &'static str {
        match self {
            Self::Ship => "water",
            Self::Train => "railroad",
            Self::Infantry => "dry land",
            Self::Armor | Self::Supply | Self::Artillery => "open ground or road column",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Ship => 0,
            Self::Armor => 1,
            Self::Supply => 2,
            Self::Artillery => 3,
            Self::Infantry => 4,
            Self::Train => 5,
        }
    }

    fn hover(self) -> String {
        format!("{} ({})", self.label(), self.terrain_hint())
    }

    fn from_army(kind: ArmyUnitKind) -> Self {
        match kind {
            ArmyUnitKind::Ship => Self::Ship,
            ArmyUnitKind::Armor => Self::Armor,
            ArmyUnitKind::Supply => Self::Supply,
            ArmyUnitKind::Artillery | ArmyUnitKind::MobileArtillery => Self::Artillery,
            ArmyUnitKind::Infantry => Self::Infantry,
            ArmyUnitKind::Train => Self::Train,
        }
    }

    fn ground(self) -> Option<GroundKind> {
        match self {
            Self::Ship => None,
            Self::Armor => Some(GroundKind::Armor),
            Self::Supply => Some(GroundKind::Supply),
            Self::Artillery => Some(GroundKind::Artillery),
            Self::Infantry => Some(GroundKind::Infantry),
            Self::Train => Some(GroundKind::Train),
        }
    }
}

#[derive(Clone)]
struct ReconSlot {
    path: PathBuf,
    info: UnitPlanInfo,
    kind: UnitKind,
    selected_triggers: Vec<i32>,
    influence: u32,
    restore_start: String,
    /// Copies already on the map (Rework only).
    detected: Option<usize>,
    /// Packs that contributed this type, with per-file counts (Rework only).
    sources: Vec<(PathBuf, usize)>,
}

/// A .Group loaded in Map mode as an army (not a reference stamp).
#[derive(Clone)]
struct MapArmySlot {
    path: PathBuf,
    entity: crate::ast::Il2Entity,
    eastern: bool,
    reposition: bool,
    copies: Vec<ArmyCopyInfo>,
    ground: Option<MapGroundLayout>,
    ships: Option<MapShipLayout>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroundHit {
    Ag { eastern: bool, i: usize },
    Army { slot: usize, i: usize },
}

impl GroundHit {
    fn is_ag(self) -> bool {
        matches!(self, GroundHit::Ag { .. })
    }

    fn spot_i(self) -> usize {
        match self {
            GroundHit::Ag { i, .. } | GroundHit::Army { i, .. } => i,
        }
    }
}

#[derive(Clone, Copy)]
enum ShipHit {
    Ag(usize),
    Army { slot: usize, i: usize },
}

impl ShipHit {
    fn is_ag(self) -> bool {
        matches!(self, ShipHit::Ag(_))
    }
}

struct GroupGeneratorApp {
    mode: AppMode,
    custom_path: Option<PathBuf>,
    linked_groups: u32,
    flight_count: u32,
    max_in_flight: u32,
    type_enabled: Vec<bool>,
    type_skill: Vec<i32>,
    country: i32,
    cooldown: f32,
    reinforcement: f32,
    delete_orders: f32,
    altitude_min: f32,
    altitude_max: f32,
    bomber_slots: Vec<BomberSlot>,
    bomber_keep_positions: bool,
    /// Plan shown in the Exclusive Activation center.
    bomber_selected: Option<usize>,
    /// The generated Exclusive Activation pack whose plans were added (header "Editing {stem}").
    bomber_loaded_path: Option<PathBuf>,
    recon_submode: ReconSubmode,
    recon_slots: Vec<ReconSlot>,
    recon_rework: Vec<ReconSlot>,
    recon_total: u32,
    recon_percent: u32,
    recon_keep_positions: bool,
    recon_import_kind: UnitKind,
    /// Eastern (red) vs NATO (teal) icons on the Army Generator page.
    recon_eastern: bool,
    recon_group_delay_ms: u32,
    recon_start_delay_s: u32,
    recon_strip_randomizer: bool,
    airfield_path: Option<PathBuf>,
    airfield_root: Option<crate::ast::Il2Entity>,
    airfield_info: Option<AirfieldInfo>,
    airfield_western: bool,
    harvest_missions_dir: String,
    harvest_db_dir: String,
    harvest_cfg: HarvestConfig,
    /// `Some` while "Watch for new airfields" is on.
    harvest_watcher: Option<GenWatcher>,
    /// Newest first.
    harvest_log: Vec<String>,
    front_year: u16,
    front_season: Season,
    front_t: f32,
    front_aabb: WorldAabb,
    map_drag_uv: Option<Pos2>,
    map_lo_tex: Option<TextureHandle>,
    map_hi_tex: Option<TextureHandle>,
    map_rx: Option<std::sync::mpsc::Receiver<KoreaMapLayer>>,
    map_zoom: f32,
    map_pan: Pos2,
    map_refs: Vec<MapRefGroup>,
    drawing_custom_front: bool,
    custom_front_xz: Vec<(f64, f64)>,
    map_drawing_mode: MapDrawingMode,
    map_dock: MapDock,
    /// World (x, z) under the pointer on the map, for the height readout.
    map_hover_xz: Option<(f64, f64)>,
    terrain_store_path: PathBuf,
    /// Loaded on first use; `terrain_error` holds why it could not be.
    terrain_store: Option<HeightStore>,
    terrain_error: Option<String>,
    terrain_tiles: Option<Vec<(usize, usize, usize)>>,
    terrain_log: Vec<String>,
    terrain_show_coverage: bool,
    terrain_show_relief: bool,
    /// Put generated units on the measured terrain when exporting. Off by
    /// default (user decision, 2026-09-24): exports are unchanged until it is on.
    terrain_apply: bool,
    terrain_relief: Option<TextureHandle>,
    current_salient: Vec<(f64, f64)>,
    salients: Vec<Vec<(f64, f64)>>,
    attack_arrows: Vec<((f64, f64), (f64, f64))>,
    attack_drag: Option<((f64, f64), (f64, f64))>,
    drawn_marks: Vec<DrawnMark>,
	redo_marks: Vec<DrawnMark>,
    redo_salients: Vec<Vec<(f64, f64)>>,
    redo_attack_arrows: Vec<((f64, f64), (f64, f64))>,
    /// Points a front-mark undo took off `custom_front_xz`, for Ctrl Y.
    redo_fronts: Vec<Vec<(f64, f64)>>,
    /// `custom_front_xz.len()` when the current Draw-front drag started.
    front_stroke: Option<usize>,
    /// A tool was cancelled or switched: ignore the map's left button until
    /// it is released, so the rest of that drag draws nothing.
    map_void_drag: bool,
    map_fighters: Option<MapFighterLayout>,
    map_imported_fighters: Vec<ImportedFighterPack>,
    fighter_waves: u32,
    fighter_fill: bool,
    fighter_drag: Option<usize>,
    fighter_tex_east: Option<TextureHandle>,
    fighter_tex_nato: Option<TextureHandle>,
    ship_tex_east: Option<TextureHandle>,
    ship_tex_nato: Option<TextureHandle>,
    dir_tex: Option<TextureHandle>,
    map_ships: Option<MapShipLayout>,
    ship_drag: Option<ShipHit>,
    ship_heading_drag: Option<ShipHit>,
    obj_tex_east: Option<TextureHandle>,
    obj_tex_nato: Option<TextureHandle>,
    armor_tex_east: Option<TextureHandle>,
    armor_tex_nato: Option<TextureHandle>,
    supply_tex_east: Option<TextureHandle>,
    supply_tex_nato: Option<TextureHandle>,
    arty_tex_east: Option<TextureHandle>,
    arty_tex_nato: Option<TextureHandle>,
    train_tex_east: Option<TextureHandle>,
    train_tex_nato: Option<TextureHandle>,
    infantry_tex_east: Option<TextureHandle>,
    infantry_tex_nato: Option<TextureHandle>,
    east_objectives: Vec<(f64, f64)>,
    nato_objectives: Vec<(f64, f64)>,
    objective_drag: Option<(bool, usize)>,
    map_ground_east: Option<MapGroundLayout>,
    map_ground_nato: Option<MapGroundLayout>,
    map_armies: Vec<MapArmySlot>,
    ground_drag: Option<GroundHit>,
    ground_heading_drag: Option<GroundHit>,
    wp_drag: Option<(GroundHit, usize)>,
    wp_selected: Option<(GroundHit, usize)>,
    front_focus: Option<&'static str>,
    help_open: bool,
    help_topic: HelpTopic,
    status: Status,
    /// Each tab's status while another tab is shown, in `MODES` order (`set_mode` swaps).
    tab_status: [Status; 6],
    /// Status text last seen, and the tab's edit fingerprint when it appeared (`age_status`).
    status_seen: String,
    status_fp: u64,
    tpl_path: Option<PathBuf>,
    /// Group currently being edited (Load group…), not the catalog path.
    tpl_loaded_path: Option<PathBuf>,
    tpl_catalog: Vec<CatalogUnit>,
    tpl_kind: CatalogKind,
    tpl_class: Option<ModelClass>,
    /// Country filter for the model list (prototype `Country`, any kind).
    tpl_country: Option<i32>,
    tpl_add_pick: usize,
    /// Scroll the left panel to the model list on the next frame.
    tpl_show_models: bool,
    /// When true, the formation-view card shows the catalog pick (adding a unit).
    /// Otherwise it follows the highlighted seat.
    tpl_preview_from_catalog: bool,
    tpl_model_tex: HashMap<String, TextureHandle>,
    tpl_seats: Vec<TemplateSeat>,
    tpl_select: Option<TplSelect>,
    tpl_undo: shell::Undo<TemplateSnapshot>,
    /// `tpl_fingerprint` at the last Load / Generate / Reset.
    tpl_saved: String,
    recon_undo: shell::Undo<(ReconSubmode, Vec<ReconSlot>)>,
    /// The plans before a Remove, and which plan was selected.
    bomber_undo: shell::Undo<(Vec<BomberSlot>, Option<usize>)>,
    map_undo: shell::Undo<MapForces>,
    /// `drawn_marks.len()` right after the recorded Clear: marks above it were
    /// drawn later and undo first; once they are gone the Clear is next.
    map_undo_marks: usize,
    /// What Ctrl Z undoes next on Map (see `map_undo_kind`).
    map_last: MapAction,
    map_saved: String,
    confirm: Option<Confirm>,
    /// Unit card being dragged in the Units list (index at drag start).
    tpl_card_drag: Option<usize>,
    tpl_bring_up: BringUp,
    tpl_spawn_reset: bool,
    tpl_spawn_cooldown_min: f32,
    tpl_place_layout: PlaceLayout,
    tpl_per_group: u32,
    tpl_zone_in: f32,
    tpl_zone_out: f32,
    /// Last mix that wrote Zone IN / Out defaults. User edits stick until this changes.
    tpl_zone_mix: Option<ZoneMix>,
    tpl_wp_spacing: f32,
    tpl_wp_speed: f32,
    tpl_wp_altitude: f32,
    tpl_wp_priority: i32,
    tpl_zone_coalition: ZoneCoalition,
    tpl_view_zoom: f32,
    tpl_view_pan: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TplSelect {
    Seat(usize),
    Order { seat: usize, order: usize },
    Event { seat: usize, event: usize },
}

#[derive(Clone, Copy, Debug)]
enum AddTreeItem {
    Order { seat: usize, kind: OrderKind },
    Event { seat: usize, kind: EntityEvent },
}

fn order_spec_for_added_kind(unit: &CatalogUnit, kind: OrderKind, next_wp: u32) -> OrderSpec {
    let mut spec = OrderSpec::for_unit(unit);
    spec.kind = kind;
    if kind == OrderKind::GotoWaypoint {
        spec.waypoint = next_wp.max(1);
    }
    if kind == OrderKind::TimeOnTarget {
        spec.time_s = DEFAULT_TIME_ON_TARGET_S;
    }
    if kind == OrderKind::Timer {
        spec.time_s = DEFAULT_TIMER_S;
    }
    if kind == OrderKind::Cover || kind == OrderKind::ForceComplete {
        spec.priority = 2;
    }
    spec
}

fn apply_order_kind(seats: &mut [TemplateSeat], seat: usize, order: usize, k: OrderKind) {
    let unit_kind = if seats[seat].unit.is_air() {
        CatalogKind::Plane
    } else {
        CatalogKind::Vehicle
    };
    let was_goto = seats[seat].orders[order].kind == OrderKind::GotoWaypoint;
    let next_wp = next_waypoint_number(seats);
    seats[seat].orders[order].kind = k;
    if k == OrderKind::GotoWaypoint && !was_goto {
        seats[seat].orders[order].waypoint = next_wp;
    }
    if k == OrderKind::Formation {
        let presets = formations_for(unit_kind);
        let id = seats[seat].orders[order].formation_type;
        if !presets.iter().any(|p| p.id == id) {
            seats[seat].orders[order].formation_type =
                OrderSpec::for_kind(unit_kind).formation_type;
        }
    }
    if k == OrderKind::TimeOnTarget {
        seats[seat].orders[order].time_s = DEFAULT_TIME_ON_TARGET_S;
    }
    if k == OrderKind::Timer {
        seats[seat].orders[order].time_s = DEFAULT_TIMER_S;
    }
    if k == OrderKind::Cover || k == OrderKind::ForceComplete {
        seats[seat].orders[order].priority = 2;
    }
    if k == OrderKind::AttackArea {
        apply_suggested_attack_area(seats, seat, order);
    }
}

fn kinds_in_same_group(kind: OrderKind, unit_kind: CatalogKind) -> Vec<OrderKind> {
    if kind.is_report() {
        OrderKind::reports(unit_kind).collect()
    } else if kind.is_special() {
        OrderKind::specials(unit_kind).collect()
    } else {
        OrderKind::commands(unit_kind).collect()
    }
}

/// "+ Order ▾" in the order tree header: commands, specials and reports for
/// the selected unit.
fn tree_order_menu(ui: &mut egui::Ui, si: usize, unit_kind: CatalogKind, add: &mut Option<AddTreeItem>) {
    ui.menu_button("+ Order ▾", |ui| {
        let groups: [(&str, Vec<OrderKind>); 3] = [
            ("Command", OrderKind::commands(unit_kind).collect()),
            ("Special", OrderKind::specials(unit_kind).collect()),
            ("Report", OrderKind::reports(unit_kind).collect()),
        ];
        for (heading, kinds) in groups {
            if kinds.is_empty() {
                continue;
            }
            ui.label(RichText::new(heading).small().color(c::NEUTRAL_700));
            for k in kinds {
                if ui.button(k.label()).clicked() {
                    *add = Some(AddTreeItem::Order { seat: si, kind: k });
                    ui.close();
                }
            }
        }
    })
    .response
    .on_hover_text("Add a command, Time on Target / Timer / Mission Complete, or an OnReport to the selected unit")
    .on_disabled_hover_text("Select a lead or independent unit. Wingmen take no orders.");
}

/// "+ Event ▾" in the order tree header: OnEvents for the selected unit.
fn tree_event_menu(ui: &mut egui::Ui, si: usize, unit_kind: CatalogKind, add: &mut Option<AddTreeItem>) {
    ui.menu_button("+ Event ▾", |ui| {
        ui.set_max_height(280.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for k in EntityEvent::available(unit_kind) {
                if ui.button(k.label()).clicked() {
                    *add = Some(AddTreeItem::Event { seat: si, kind: *k });
                    ui.close();
                }
            }
        });
    })
    .response
    .on_hover_text("Add an OnEvent to the selected unit")
    .on_disabled_hover_text("Select a unit first.");
}

/// DPRK for the 500-series countries, NATO for the 600-series, none otherwise.
fn seat_side(country: i32) -> Option<shell::Side> {
    match country / 100 {
        5 => Some(shell::Side::Dprk),
        6 => Some(shell::Side::Nato),
        _ => None,
    }
}

fn paint_seat_marker(painter: &egui::Painter, center: Pos2, size: f32, country: i32) {
    match seat_side(country) {
        Some(side) => shell::paint_side_marker(painter, center, size, side),
        None => {
            painter.rect_filled(Rect::from_center_size(center, Vec2::splat(size * 0.7)), 1.0, c::NEUTRAL_500);
        }
    }
}

/// Condensed uppercase section heading (README §8 type scale).
fn section_heading(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).text_style(TextStyle::Name("section".into()))
}

/// Segmented control in rows of equal-width cells (README §5.1: the kind
/// selector has more options than fit one row of the 270 px panel).
fn segmented_rows<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)], per_row: usize) -> bool {
    let mut changed = false;
    let w = ui.available_width();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        for row in options.chunks(per_row.max(1)) {
            let cell = (w / row.len() as f32).floor();
            ui.horizontal(|ui| {
                for (v, label) in row {
                    let selected = *value == *v;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(cell, 28.0), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, *label));
                    if ui.is_rect_visible(rect) {
                        let visuals = ui.style().interact_selectable(&resp, selected);
                        let p = ui.painter();
                        p.rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
                        p.text(rect.center(), Align2::CENTER_CENTER, *label, FontId::proportional(13.0), visuals.text_color());
                    }
                    if resp.clicked() && *value != *v {
                        *value = *v;
                        changed = true;
                    }
                }
            });
        }
    });
    changed
}

/// IL-2 AI level names for `AILevel` 0–4, as the mission editor lists them.
fn skill_name(skill: i32) -> &'static str {
    match skill.clamp(0, 4) {
        0 => "Plain",
        1 => "Low",
        2 => "Normal",
        3 => "Veteran",
        _ => "Ace",
    }
}

/// Label column of the right-panel field grids (README §5.1).
const FIELD_LABEL_W: f32 = 96.0;

/// Two-column field grid: a 96 px label column, one label/control pair per row.
fn field_grid<R>(ui: &mut egui::Ui, id: impl std::hash::Hash, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(FIELD_LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, add)
        .inner
}

/// Label cell of a `field_grid`: wraps inside the 96 px column instead of
/// widening it.
fn field_label(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.scope(|ui| {
        ui.set_max_width(FIELD_LABEL_W);
        ui.add(egui::Label::new(text).wrap())
    })
    .inner
}

#[derive(Clone, Copy)]
enum NoteStyle {
    Plain,
    Italic,
    Warn,
}

/// Explanations shown under a `field_grid`, in small type.
fn show_field_notes(ui: &mut egui::Ui, notes: &[(String, NoteStyle)]) {
    if notes.is_empty() {
        return;
    }
    ui.add_space(4.0);
    for (text, style) in notes {
        let text = match style {
            NoteStyle::Plain => RichText::new(text).small().color(c::NEUTRAL_700),
            NoteStyle::Italic => RichText::new(text).italics().small(),
            NoteStyle::Warn => RichText::new(text).color(c::WARN_TEXT),
        };
        ui.add(egui::Label::new(text).wrap());
    }
}

/// Width left for the control column of a `field_grid`.
fn field_control_w(ui: &egui::Ui) -> f32 {
    (ui.available_width() - FIELD_LABEL_W - 8.0).max(96.0)
}

fn default_template_seats() -> Vec<TemplateSeat> {
    Vec::new()
}

fn country_short(country: i32) -> String {
    COUNTRIES
        .iter()
        .find(|(id, _)| *id == country)
        .map(|(_, l)| (*l).to_string())
        .unwrap_or_else(|| country.to_string())
}

/// Formation-view color for a country's side: DPRK, NATO, or neutral grey.
fn side_color(country: i32) -> Color32 {
    seat_side(country).map_or(c::NEUTRAL_600, shell::Side::color)
}

fn clamp_tpl_select(select: &mut Option<TplSelect>, seats: &[TemplateSeat]) {
    match *select {
        Some(TplSelect::Seat(i)) if i >= seats.len() => {
            *select = seats.last().map(|_| TplSelect::Seat(seats.len() - 1));
        }
        Some(TplSelect::Order { seat, order }) => {
            if seat >= seats.len() {
                *select = seats.last().map(|_| TplSelect::Seat(seats.len() - 1));
            } else if order >= seats[seat].orders.len() {
                *select = Some(TplSelect::Seat(seat));
            }
        }
        Some(TplSelect::Event { seat, event }) => {
            if seat >= seats.len() {
                *select = seats.last().map(|_| TplSelect::Seat(seats.len() - 1));
            } else if event >= seats[seat].events.len() {
                *select = Some(TplSelect::Seat(seat));
            }
        }
        _ => {}
    }
}

fn swap_tpl_select(select: &mut Option<TplSelect>, a: usize, b: usize) {
    let map = |i: usize| {
        if i == a {
            b
        } else if i == b {
            a
        } else {
            i
        }
    };
    match select {
        Some(TplSelect::Seat(i)) => *i = map(*i),
        Some(TplSelect::Order { seat, .. }) => *seat = map(*seat),
        Some(TplSelect::Event { seat, .. }) => *seat = map(*seat),
        None => {}
    }
}

fn order_chip_fill(
    kind: OrderKind,
    selected: bool,
    selected_fill: Color32,
    order_fill: Color32,
    report_fill: Color32,
    chain_fill: Color32,
) -> Color32 {
    if selected {
        selected_fill
    } else if kind.is_report() {
        report_fill
    } else if kind.is_special() {
        chain_fill
    } else {
        order_fill
    }
}

fn order_chip_label(oi: usize, kind: OrderKind, extra: usize) -> String {
    if extra > 0 {
        format!("{} {} +{}", oi + 1, kind.label(), extra)
    } else {
        format!("{} {}", oi + 1, kind.label())
    }
}

/// Order-tree chip: hairline border, accent border + fill when selected,
/// dashed border for events.
fn draw_tree_chip(
    ui: &mut egui::Ui,
    title: &str,
    detail: &str,
    fill: Color32,
    selected: bool,
    dashed: bool,
    size: Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title));
    let p = ui.painter();
    let (fill, stroke) = if selected {
        (c::ACCENT_200, Stroke::new(1.5_f32, c::ACCENT))
    } else if response.hovered() {
        (c::ACCENT_100, Stroke::new(1.0_f32, c::ACCENT))
    } else {
        (fill, Stroke::new(1.0_f32, c::NEUTRAL_500))
    };
    p.rect_filled(rect, 2.0, fill);
    if dashed && !selected {
        let r = rect.shrink(0.5);
        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        p.extend(egui::Shape::dashed_line(&pts, stroke, 4.0, 3.0));
    } else {
        p.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    }
    let title_font = FontId::new(13.0, if selected { theme::bold_family() } else { FontFamily::Proportional });
    let c0 = rect.center();
    let clip = p.with_clip_rect(rect.shrink(3.0));
    if detail.is_empty() {
        clip.text(c0, Align2::CENTER_CENTER, title, title_font, c::TEXT);
    } else {
        clip.text(Pos2::new(c0.x, c0.y - 7.0), Align2::CENTER_CENTER, title, title_font, c::TEXT);
        clip.text(Pos2::new(c0.x, c0.y + 8.0), Align2::CENTER_CENTER, detail, FontId::proportional(12.0), c::NEUTRAL_700);
    }
    response
}

/// The unit chip that starts each order-tree row: side marker, "{n} · {model}".
fn draw_tree_unit_chip(ui: &mut egui::Ui, n: usize, label: &str, country: i32, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(TREE_UNIT_W, TREE_CHIP_H), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, format!("{n} · {label}")));
    let p = ui.painter();
    let (fill, stroke) = if selected {
        (c::ACCENT_200, Stroke::new(1.5_f32, c::ACCENT))
    } else if response.hovered() {
        (c::ACCENT_100, Stroke::new(1.0_f32, c::ACCENT))
    } else {
        (c::NEUTRAL_100, Stroke::new(1.0_f32, c::NEUTRAL_500))
    };
    p.rect_filled(rect, 2.0, fill);
    p.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    paint_seat_marker(p, Pos2::new(rect.left() + 12.0, rect.center().y), 11.0, country);
    // Elide a long model name with "…" instead of cutting a letter in half.
    let mut job = egui::text::LayoutJob::single_section(
        format!("{n} · {label}"),
        egui::TextFormat::simple(FontId::new(13.0, theme::bold_family()), c::TEXT),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.right() - 4.0 - (rect.left() + 22.0));
    let galley = ui.fonts(|f| f.layout_job(job));
    let pos = Pos2::new(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, c::TEXT);
    let full = format!("{n} · {label}");
    response.on_hover_text(full)
}

const TREE_ARROW_SLOT: f32 = 28.0;
/// Two text lines at 13 + 12 px; README §6.6 asks for at least 28.
const TREE_CHIP_H: f32 = 36.0;
const TREE_CHIP_W: f32 = 140.0;
const TREE_UNIT_W: f32 = 98.0;
const TREE_ROW_GAP: f32 = 6.0;
const TREE_UNIT_GAP: f32 = 10.0;

fn order_chip_size(_kind: OrderKind) -> Vec2 {
    Vec2::new(TREE_CHIP_W, TREE_CHIP_H)
}

fn draw_priority_combo(ui: &mut egui::Ui, id: String, priority: &mut i32) {
    ui.label("Priority");
    egui::ComboBox::from_id_salt(id)
        .selected_text(priority_label(*priority))
        .width(100.0)
        .show_ui(ui, |ui| {
            for (v, label) in [(0, "Low"), (1, "Medium"), (2, "High")] {
                ui.selectable_value(priority, v, label);
            }
        });
}

fn draw_carriage_style(
    ui: &mut egui::Ui,
    salt: String,
    train_script: &str,
    car_script: &str,
    mask: &mut String,
) {
    let Some(slot_n) = payloads::train_carriage_slot(car_script) else {
        return;
    };
    let Some(ac) = payloads::catalog().for_script(train_script) else {
        return;
    };
    let Some(slot) = ac.mod_slots.iter().find(|s| s.number == slot_n) else {
        return;
    };
    if !payloads::slot_has_choices(slot) {
        return;
    }
    let mut bits = payloads::parse_mod_mask_for(train_script, mask);
    let choices: Vec<&payloads::ModOption> = slot
        .options
        .iter()
        .filter(|o| payloads::extra_bits(&o.binary_id) != 0)
        .collect();
    if choices.len() == 1 {
        let opt = choices[0];
        let mut on = payloads::option_selected(bits, opt);
        if ui.checkbox(&mut on, &opt.description).changed() {
            bits = payloads::set_toggle_for(train_script, bits, opt, on);
            *mask = payloads::encode_mod_mask(bits);
        }
        return;
    }
    let selected = payloads::exclusive_selection(bits, slot);
    let label = selected
        .map(|o| o.description.as_str())
        .unwrap_or("None");
    egui::ComboBox::from_id_salt(salt)
        .selected_text(label)
        .width(180.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(selected.is_none(), "None").clicked() {
                bits = payloads::clear_exclusive_for(train_script, bits, slot);
                *mask = payloads::encode_mod_mask(bits);
            }
            for opt in &choices {
                let is_on = selected.is_some_and(|s| s.binary_id == opt.binary_id);
                if ui.selectable_label(is_on, &opt.description).clicked() {
                    bits = payloads::select_exclusive_for(train_script, bits, slot, opt);
                    *mask = payloads::encode_mod_mask(bits);
                }
            }
        });
}

fn sync_train_mask_to_carriages(seat: &mut TemplateSeat) {
    let script = seat.unit.script.clone();
    let Some(ac) = payloads::catalog().for_script(&script) else {
        return;
    };
    let mut used = [false; 16];
    for car in &seat.carriages {
        if let Some(n) = payloads::train_carriage_slot(car) {
            if (n as usize) < used.len() {
                used[n as usize] = true;
            }
        }
    }
    let mut mask = payloads::parse_mod_mask_for(&script, &seat.mod_mask);
    for slot in &ac.mod_slots {
        let n = slot.number as usize;
        if n < used.len() && !used[n] && payloads::slot_has_choices(slot) {
            mask = payloads::clear_exclusive_for(&script, mask, slot);
        }
    }
    seat.mod_mask = payloads::encode_mod_mask(mask);
}

fn tree_arrow_slot(ui: &mut egui::Ui, show: bool, left: bool, enabled: bool) -> bool {
    if show {
        let mut clicked = false;
        ui.add_enabled_ui(enabled, |ui| {
            if move_col_button(ui, left).clicked() {
                clicked = true;
            }
        });
        clicked
    } else {
        ui.add_space(TREE_ARROW_SLOT);
        false
    }
}

fn draw_template_order_chip(
    ui: &mut egui::Ui,
    si: usize,
    oi: usize,
    n_orders: usize,
    kind: OrderKind,
    extra: usize,
    detail: &str,
    selected: bool,
    selected_fill: Color32,
    order_fill: Color32,
    report_fill: Color32,
    chain_fill: Color32,
    clicked: &mut Option<TplSelect>,
    remove_order: &mut Option<(usize, usize)>,
    move_order: &mut Option<(usize, usize, i32)>,
) -> Rect {
    let mut chip_rect = Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if tree_arrow_slot(ui, selected, true, oi > 0) {
            *move_order = Some((si, oi, -1));
        }
        let text = order_chip_label(oi, kind, extra);
        let fill = order_chip_fill(
            kind,
            selected,
            selected_fill,
            order_fill,
            report_fill,
            chain_fill,
        );
        let hover = if kind.is_wp_parallel() {
            "Starts with Attack / Time on Target from the waypoint, not after a delay."
        } else if kind == OrderKind::MissionComplete {
            "Pulses MISSION END. The previous hop starts this only when no event Then's it; otherwise cleanup waits for those events."
        } else if kind == OrderKind::Timer {
            "MCU_Timer pause between the previous order and the next."
        } else {
            ""
        };
        let resp = draw_tree_chip(ui, &text, detail, fill, selected, false, order_chip_size(kind));
        if !hover.is_empty() {
            resp.clone().on_hover_text(hover);
        }
        if resp.clicked() {
            *clicked = Some(TplSelect::Order { seat: si, order: oi });
        }
        chip_rect = resp.rect;
        if tree_arrow_slot(ui, selected, false, oi + 1 < n_orders) {
            *move_order = Some((si, oi, 1));
        }
        if ui.button("×").on_hover_text("Remove order").clicked() {
            *remove_order = Some((si, oi));
        }
    });
    chip_rect
}

fn draw_template_event_chip(
    ui: &mut egui::Ui,
    si: usize,
    ei: usize,
    kind: EntityEvent,
    selected: bool,
    // Selection is drawn by draw_tree_chip; kept for the shared call shape.
    _selected_fill: Color32,
    event_fill: Color32,
    clicked: &mut Option<TplSelect>,
    remove_event: &mut Option<(usize, usize)>,
) -> Rect {
    let mut chip_rect = Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(TREE_ARROW_SLOT);
        let resp = draw_tree_chip(
            ui,
            kind.label(),
            "",
            event_fill,
            selected,
            true,
            Vec2::new(TREE_CHIP_W, TREE_CHIP_H),
        );
        if resp.clicked() {
            *clicked = Some(TplSelect::Event { seat: si, event: ei });
        }
        chip_rect = resp.rect;
        ui.add_space(TREE_ARROW_SLOT);
        if ui.button("×").on_hover_text("Remove event").clicked() {
            *remove_event = Some((si, ei));
        }
    });
    chip_rect
}

fn split_trailing_events(mut laid: Vec<Vec<OrderTreeNode>>) -> (Vec<Vec<OrderTreeNode>>, Vec<OrderTreeNode>) {
    let trailing = if laid.last().is_some_and(|col| {
        !col.is_empty() && col.iter().all(|n| matches!(n, OrderTreeNode::Event(_)))
    }) {
        laid.pop().unwrap_or_default()
    } else {
        Vec::new()
    };
    (laid, trailing)
}

fn tree_line_stroke() -> Stroke {
    Stroke::new(1.0_f32, c::NEUTRAL_500)
}

fn push_polyline(shapes: &mut Vec<egui::Shape>, pts: Vec<Pos2>, stroke: Stroke) {
    if pts.len() < 2 {
        return;
    }
    shapes.push(egui::Shape::line(pts, stroke));
}

/// Leave the right side of `from`, enter the left side of `to`.
/// Different rows use an S: out the right, vertical in the gap, into the left.
fn right_into_left(from: Rect, to: Rect) -> Vec<Pos2> {
    let start = from.right_center();
    let dest = to.left_center();
    if (start.y - dest.y).abs() < 2.0 {
        return vec![start, dest];
    }
    let gap = dest.x - start.x;
    if gap > 4.0 {
        let t = if dest.y > start.y + 2.0 {
            0.62
        } else {
            0.38
        };
        let lane = start.x + gap * t;
        vec![
            start,
            Pos2::new(lane, start.y),
            Pos2::new(lane, dest.y),
            dest,
        ]
    } else {
        let stub_x = start.x.max(dest.x) + 16.0;
        vec![
            start,
            Pos2::new(stub_x, start.y),
            Pos2::new(stub_x, dest.y),
            dest,
        ]
    }
}

fn node_is_tot(node: OrderTreeNode, orders: &[OrderSpec]) -> bool {
    matches!(
        node,
        OrderTreeNode::Order(i) if orders.get(i).is_some_and(|o| o.kind == OrderKind::TimeOnTarget)
    )
}

fn node_is_attack(node: OrderTreeNode, orders: &[OrderSpec]) -> bool {
    matches!(
        node,
        OrderTreeNode::Order(i)
            if orders.get(i).is_some_and(|o| {
                matches!(o.kind, OrderKind::Attack | OrderKind::AttackArea)
            })
    )
}

fn node_is_spine_order(node: OrderTreeNode, orders: &[OrderSpec]) -> bool {
    matches!(
        node,
        OrderTreeNode::Order(i)
            if orders.get(i).is_some_and(|o| {
                o.kind == OrderKind::OnSpawned
                    || (o.kind != OrderKind::TimeOnTarget && !o.kind.is_report())
            })
    )
}

fn spine_triggered_by_event(
    col: &[(OrderTreeNode, Rect)],
    orders: &[OrderSpec],
    events: &[EventHook],
) -> bool {
    column_spine(col, orders).is_some_and(|(n, _)| {
        matches!(n, OrderTreeNode::Order(i) if event_triggers_order(events, i))
    })
}

fn column_spine(
    col: &[(OrderTreeNode, Rect)],
    orders: &[OrderSpec],
) -> Option<(OrderTreeNode, Rect)> {
    col.iter()
        .copied()
        .find(|(n, _)| node_is_spine_order(*n, orders))
}

fn event_prefix_len(col: &[OrderTreeNode]) -> usize {
    col.iter()
        .take_while(|n| matches!(n, OrderTreeNode::Event(_)))
        .count()
}

fn node_is_report(node: OrderTreeNode, orders: &[OrderSpec]) -> bool {
    matches!(
        node,
        OrderTreeNode::Order(i) if orders.get(i).is_some_and(|o| o.kind.is_report())
    )
}

fn node_is_stacked_report(node: OrderTreeNode, orders: &[OrderSpec]) -> bool {
    matches!(
        node,
        OrderTreeNode::Order(i)
            if orders.get(i).is_some_and(|o| {
                o.kind.is_report() && o.kind != OrderKind::OnSpawned
            })
    )
}

fn top_report_len(col: &[OrderTreeNode], orders: &[OrderSpec]) -> usize {
    let ev = event_prefix_len(col);
    col[ev..]
        .iter()
        .take_while(|n| node_is_stacked_report(**n, orders))
        .count()
}

fn report_then_oi(orders: &[OrderSpec], oi: usize) -> Option<usize> {
    let next = oi + 1;
    (next < orders.len() && !orders[next].kind.is_report()).then_some(next)
}

fn column_is_feeder(col: &[(OrderTreeNode, Rect)], orders: &[OrderSpec]) -> bool {
    !col.is_empty()
        && col
            .iter()
            .all(|(n, _)| matches!(n, OrderTreeNode::Event(_)) || node_is_report(*n, orders))
}

fn order_rect_in(columns: &[Vec<(OrderTreeNode, Rect)>], oi: usize) -> Option<Rect> {
    for col in columns {
        for (node, rect) in col {
            if matches!(node, OrderTreeNode::Order(i) if *i == oi) {
                return Some(*rect);
            }
        }
    }
    None
}

fn paint_feeder_line(
    shapes: &mut Vec<egui::Shape>,
    columns: &[Vec<(OrderTreeNode, Rect)>],
    col: &[(OrderTreeNode, Rect)],
    orders: &[OrderSpec],
    events: &[EventHook],
    node: OrderTreeNode,
    rect: Rect,
    stroke: Stroke,
) {
    match node {
        OrderTreeNode::Event(ei) => {
            if let Some(EventThen::Order(oi)) = events.get(ei).map(|h| h.then) {
                if let Some(target) = order_rect_in(columns, oi) {
                    push_polyline(shapes, right_into_left(rect, target), stroke);
                }
            }
        }
        OrderTreeNode::Order(oi) if node_is_report(node, orders) => {
            if let Some(toi) = report_then_oi(orders, oi) {
                let same_col = col
                    .iter()
                    .any(|(n, _)| matches!(n, OrderTreeNode::Order(i) if *i == toi));
                if !same_col {
                    if let Some(target) = order_rect_in(columns, toi) {
                        push_polyline(shapes, right_into_left(rect, target), stroke);
                    }
                }
            }
        }
        _ => {}
    }
}

fn paint_order_tree_lines(
    ui: &mut egui::Ui,
    idx: egui::layers::ShapeIdx,
    columns: &[Vec<(OrderTreeNode, Rect)>],
    orders: &[OrderSpec],
    events: &[EventHook],
) {
    let stroke = tree_line_stroke();
    let mut shapes = Vec::new();
    let spines: Vec<(usize, Rect)> = columns
        .iter()
        .enumerate()
        .filter(|(_, col)| !column_is_feeder(col, orders))
        .filter_map(|(ci, col)| column_spine(col, orders).map(|(_, r)| (ci, r)))
        .collect();
    for pair in spines.windows(2) {
        if spine_triggered_by_event(&columns[pair[1].0], orders, events) {
            continue;
        }
        push_polyline(&mut shapes, right_into_left(pair[0].1, pair[1].1), stroke);
    }
    for (ci, col) in columns.iter().enumerate() {
        if column_is_feeder(col, orders) {
            for (node, rect) in col {
                paint_feeder_line(&mut shapes, columns, col, orders, events, *node, *rect, stroke);
            }
            continue;
        }
        let Some(spine_i) = col.iter().position(|(n, _)| node_is_spine_order(*n, orders)) else {
            continue;
        };
        let (spine_node, spine_rect) = col[spine_i];
        let prev_spine = spines
            .iter()
            .rev()
            .find(|(sci, _)| *sci < ci)
            .map(|(_, r)| *r);
        let next_spine = spines.iter().find(|(sci, _)| *sci > ci).and_then(|(sci, r)| {
            if spine_triggered_by_event(&columns[*sci], orders, events) {
                None
            } else {
                Some(*r)
            }
        });
        for (row, (node, rect)) in col.iter().copied().enumerate() {
            if row == spine_i {
                continue;
            }
            if node_is_tot(node, orders) {
                let feeder = if node_is_attack(spine_node, orders) {
                    prev_spine.unwrap_or(spine_rect)
                } else {
                    spine_rect
                };
                push_polyline(&mut shapes, right_into_left(feeder, rect), stroke);
                if let Some(next) = next_spine {
                    push_polyline(&mut shapes, right_into_left(rect, next), stroke);
                }
            } else if let OrderTreeNode::Event(ei) = node {
                if let Some(EventThen::Order(oi)) = events.get(ei).map(|h| h.then) {
                    let same_col = col
                        .iter()
                        .any(|(n, _)| matches!(n, OrderTreeNode::Order(i) if *i == oi));
                    if !same_col {
                        if let Some(target) = order_rect_in(columns, oi) {
                            push_polyline(&mut shapes, right_into_left(rect, target), stroke);
                        }
                    }
                }
            } else if node_is_report(node, orders) {
                if let OrderTreeNode::Order(oi) = node {
                    if let Some(toi) = report_then_oi(orders, oi) {
                        let same_col = col
                            .iter()
                            .any(|(n, _)| matches!(n, OrderTreeNode::Order(i) if *i == toi));
                        if !same_col {
                            if let Some(target) = order_rect_in(columns, toi) {
                                push_polyline(&mut shapes, right_into_left(rect, target), stroke);
                            }
                        }
                    }
                }
            } else if let Some(next) = next_spine {
                push_polyline(&mut shapes, right_into_left(rect, next), stroke);
            }
        }
    }
    ui.painter().set(idx, egui::Shape::Vec(shapes));
}

fn draw_tree_node_chip(
    ui: &mut egui::Ui,
    si: usize,
    node: OrderTreeNode,
    unit_kind: CatalogKind,
    orders: &[OrderSpec],
    events: &[EventHook],
    select: Option<TplSelect>,
    selected_fill: Color32,
    order_fill: Color32,
    report_fill: Color32,
    chain_fill: Color32,
    event_fill: Color32,
    clicked: &mut Option<TplSelect>,
    remove_order: &mut Option<(usize, usize)>,
    remove_event: &mut Option<(usize, usize)>,
    move_order: &mut Option<(usize, usize, i32)>,
) -> Rect {
    match node {
        OrderTreeNode::Order(oi) => {
            let kind = orders[oi].kind;
            let extra = orders[oi].shared_with.len();
            let selected = matches!(
                select,
                Some(TplSelect::Order { seat, order }) if seat == si && order == oi
            );
            let detail = order_chip_detail(&orders[oi], unit_kind);
            draw_template_order_chip(
                ui,
                si,
                oi,
                orders.len(),
                kind,
                extra,
                &detail,
                selected,
                selected_fill,
                order_fill,
                report_fill,
                chain_fill,
                clicked,
                remove_order,
                move_order,
            )
        }
        OrderTreeNode::Event(ei) => {
            let selected = matches!(
                select,
                Some(TplSelect::Event { seat, event }) if seat == si && event == ei
            );
            draw_template_event_chip(
                ui,
                si,
                ei,
                events[ei].kind,
                selected,
                selected_fill,
                event_fill,
                clicked,
                remove_event,
            )
        }
    }
}

fn draw_order_tree_columns(
    ui: &mut egui::Ui,
    si: usize,
    columns: &[Vec<OrderTreeNode>],
    seats: &[TemplateSeat],
    select: Option<TplSelect>,
    selected_fill: Color32,
    order_fill: Color32,
    report_fill: Color32,
    chain_fill: Color32,
    event_fill: Color32,
    clicked: &mut Option<TplSelect>,
    remove_order: &mut Option<(usize, usize)>,
    remove_event: &mut Option<(usize, usize)>,
    move_order: &mut Option<(usize, usize, i32)>,
) {
    if columns.is_empty() {
        return;
    }
    let unit_kind = seats[si].unit.kind;
    let orders = &seats[si].orders;
    let events = &seats[si].events;
    let line_idx = ui.painter().add(egui::Shape::Noop);
    let mut geom: Vec<Vec<(OrderTreeNode, Rect)>> = Vec::new();
    let max_events = columns.iter().map(|c| event_prefix_len(c)).max().unwrap_or(0);
    let max_reports = columns
        .iter()
        .map(|c| top_report_len(c, orders))
        .max()
        .unwrap_or(0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, TREE_ROW_GAP);
        for col in columns {
            let mut col_geom = Vec::new();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = TREE_ROW_GAP;
                let events_n = event_prefix_len(col);
                let reports_n = top_report_len(col, orders);
                for _ in 0..max_events.saturating_sub(events_n) {
                    ui.add_space(TREE_CHIP_H);
                }
                for node in &col[..events_n] {
                    col_geom.push((
                        *node,
                        draw_tree_node_chip(
                            ui,
                            si,
                            *node,
                            unit_kind,
                            orders,
                            events,
                            select,
                            selected_fill,
                            order_fill,
                            report_fill,
                            chain_fill,
                            event_fill,
                            clicked,
                            remove_order,
                            remove_event,
                            move_order,
                        ),
                    ));
                }
                for _ in 0..max_reports.saturating_sub(reports_n) {
                    ui.add_space(TREE_CHIP_H);
                }
                for node in &col[events_n..] {
                    col_geom.push((
                        *node,
                        draw_tree_node_chip(
                            ui,
                            si,
                            *node,
                            unit_kind,
                            orders,
                            events,
                            select,
                            selected_fill,
                            order_fill,
                            report_fill,
                            chain_fill,
                            event_fill,
                            clicked,
                            remove_order,
                            remove_event,
                            move_order,
                        ),
                    ));
                }
            });
            geom.push(col_geom);
        }
    });
    paint_order_tree_lines(ui, line_idx, &geom, orders, events);
}

#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Info(String),
    /// Soft placement / reposition notes (orange bullets). Hard failures use [`Status::Error`].
    Warn { lead: String, items: Vec<String> },
    Error(String),
}

impl Default for GroupGeneratorApp {
    fn default() -> Self {
        let mut type_enabled = vec![false; AIRCRAFT_TYPES.len()];
        type_enabled[0] = true; // MiG-15bis
        type_enabled[1] = true; // La-11
        let type_skill = AIRCRAFT_TYPES
            .iter()
            .map(|ac| default_skill(ac.id))
            .collect();
        Self {
            mode: AppMode::Fighter,
            custom_path: None,
            linked_groups: 3,
            flight_count: 4,
            max_in_flight: 4,
            type_enabled,
            type_skill,
            country: 501,
            cooldown: 180.0,
            reinforcement: 300.0,
            delete_orders: 60.0,
            altitude_min: 1000.0,
            altitude_max: 5500.0,
            bomber_slots: Vec::new(),
            bomber_keep_positions: false,
            bomber_selected: None,
            bomber_loaded_path: None,
            recon_submode: ReconSubmode::New,
            recon_slots: Vec::new(),
            recon_rework: Vec::new(),
            recon_total: 2,
            recon_percent: 50,
            recon_keep_positions: false,
            recon_import_kind: UnitKind::Armor,
            recon_eastern: true,
            recon_group_delay_ms: 500,
            recon_start_delay_s: 0,
            recon_strip_randomizer: false,
            airfield_path: None,
            airfield_root: None,
            airfield_info: None,
            airfield_western: true,
            harvest_missions_dir: default_missions_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            harvest_db_dir: DEFAULT_DB_DIR.to_string(),
            harvest_cfg: HarvestConfig::default(),
            harvest_watcher: None,
            harvest_log: Vec::new(),
            front_year: 1951,
            front_season: Season::LateSpring,
            front_t: timeline_index(1951, Season::LateSpring) as f32,
            front_aabb: WorldAabb::full_map(),
            map_drag_uv: None,
            map_lo_tex: None,
            map_hi_tex: None,
            map_rx: None,
            map_zoom: 1.0,
            map_pan: Pos2::new(0.5, 0.5),
            map_refs: Vec::new(),
            drawing_custom_front: false,
            custom_front_xz: Vec::new(),
			map_drawing_mode: MapDrawingMode::None,
            map_dock: MapDock::Period,
            map_hover_xz: None,
            terrain_store_path: crate::terrain::default_store_path(),
            terrain_store: None,
            terrain_error: None,
            terrain_tiles: None,
            terrain_log: Vec::new(),
            terrain_show_coverage: false,
            terrain_show_relief: false,
            terrain_apply: false,
            terrain_relief: None,
            current_salient: Vec::new(),
            salients: Vec::new(),
            attack_arrows: Vec::new(),
            attack_drag: None,
            drawn_marks: Vec::new(),
			redo_marks: Vec::new(),
            redo_salients: Vec::new(),
            redo_attack_arrows: Vec::new(),
            redo_fronts: Vec::new(),
            front_stroke: None,
            map_void_drag: false,
            map_fighters: None,
            map_imported_fighters: Vec::new(),
            fighter_waves: 2,
            fighter_fill: false,
            fighter_drag: None,
            fighter_tex_east: None,
            fighter_tex_nato: None,
            ship_tex_east: None,
            ship_tex_nato: None,
            dir_tex: None,
            map_ships: None,
            ship_drag: None,
            ship_heading_drag: None,
            obj_tex_east: None,
            obj_tex_nato: None,
            armor_tex_east: None,
            armor_tex_nato: None,
            supply_tex_east: None,
            supply_tex_nato: None,
            arty_tex_east: None,
            arty_tex_nato: None,
            train_tex_east: None,
            train_tex_nato: None,
            infantry_tex_east: None,
            infantry_tex_nato: None,
            east_objectives: Vec::new(),
            nato_objectives: Vec::new(),
            objective_drag: None,
            map_ground_east: None,
            map_ground_nato: None,
            map_armies: Vec::new(),
            ground_drag: None,
            ground_heading_drag: None,
            wp_drag: None,
            wp_selected: None,
            front_focus: None,
            help_open: false,
            help_topic: HelpTopic::Overview,
            status: Status::Idle,
            tab_status: Default::default(),
            status_seen: String::new(),
            status_fp: 0,
            tpl_path: None,
            tpl_loaded_path: None,
            tpl_catalog: bundled_catalog(),
            tpl_kind: CatalogKind::Plane,
            tpl_class: None,
            tpl_country: None,
            tpl_add_pick: 0,
            tpl_show_models: false,
            tpl_preview_from_catalog: false,
            tpl_model_tex: HashMap::new(),
            tpl_seats: default_template_seats(),
            tpl_select: None,
            tpl_undo: shell::Undo::default(),
            tpl_saved: String::new(),
            recon_undo: shell::Undo::default(),
            bomber_undo: shell::Undo::default(),
            map_undo: shell::Undo::default(),
            map_undo_marks: 0,
            map_last: MapAction::Drawing,
            map_saved: String::new(),
            confirm: None,
            tpl_card_drag: None,
            tpl_bring_up: BringUp::Activate,
            tpl_spawn_reset: false,
            tpl_spawn_cooldown_min: 5.0,
            tpl_place_layout: PlaceLayout::InvertedVee,
            tpl_per_group: 4,
            tpl_zone_in: AIR_ZONE_IN_M,
            tpl_zone_out: AIR_ZONE_OUT_M,
            tpl_zone_mix: Some(ZoneMix::Air),
            tpl_wp_spacing: WAYPOINT_SPACING_M,
            tpl_wp_speed: 100.0,
            tpl_wp_altitude: 0.0,
            tpl_wp_priority: 1,
            tpl_zone_coalition: ZoneCoalition::Western,
            tpl_view_zoom: 1.0,
            tpl_view_pan: Vec2::ZERO,
        }
    }
}

impl eframe::App for GroupGeneratorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl GroupGeneratorApp {
    /// One frame of the whole app. `update` only forwards here, so the UI
    /// tests (`ui_tests.rs`) can drive frames on a bare `egui::Context`.
    fn ui(&mut self, ctx: &egui::Context) {
        ensure_side_textures(ctx);
        self.poll_harvest(ctx);
        let keys = shell::read_shortcuts(ctx, self.mode == AppMode::Map);
        self.handle_shortcuts(&keys);

        // Panels added first take the outer space: rail, status, header, then the page.
        egui::SidePanel::left("rail")
            .exact_width(shell::RAIL_W)
            .resizable(false)
            .show(ctx, |ui| {
                let mut idx = MODES.iter().position(|(m, _)| *m == self.mode).unwrap_or(0);
                let labels = MODES.map(|(_, label)| label);
                if shell::mode_rail(ui, &labels, &mut idx) {
                    self.open_help(self.page_help_topic());
                }
                self.set_mode(MODES[idx].0);
            });
        let mut undo = false;
        egui::TopBottomPanel::bottom("status")
            .exact_height(shell::STATUS_H)
            .show(ctx, |ui| undo = self.status_line(ui));
        if undo {
            self.undo_current_tab();
        }
        self.status_details(ctx);
        egui::TopBottomPanel::top("page_header")
            .exact_height(shell::HEADER_H)
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(18, 0)))
            .show(ctx, |ui| self.page_header_bar(ui));
        // Each page adds its own side and center panels; only panels scroll.
        match self.mode {
            AppMode::Template => self.template_page(ctx),
            AppMode::Recon => self.recon_page(ctx),
            AppMode::Fighter => self.fighter_page(ctx),
            AppMode::Exclusive => self.bomber_page(ctx),
            AppMode::Airfield => self.airfield_page(ctx),
            AppMode::Map => self.map_page(ctx),
        }
        self.confirm_dialogs(ctx);
        help::show_window(ctx, &mut self.help_open, &mut self.help_topic);
        self.settle_undo();
        self.age_status();
    }
}

impl GroupGeneratorApp {
    fn page_help_topic(&self) -> HelpTopic {
        match self.mode {
            AppMode::Template => HelpTopic::Template,
            AppMode::Fighter => HelpTopic::Fighter,
            AppMode::Exclusive => HelpTopic::Exclusive,
            AppMode::Recon => HelpTopic::Recon,
            AppMode::Airfield => HelpTopic::Airfield,
            AppMode::Map => HelpTopic::Front,
        }
    }

    fn open_help(&mut self, topic: HelpTopic) {
        self.help_topic = topic;
        self.help_open = true;
    }

    /// Leaving Map drops any half-drawn mark and returns to Select.
    fn set_mode(&mut self, mode: AppMode) {
        if mode == self.mode {
            return;
        }
        if self.mode == AppMode::Map {
            self.cancel_map_tool();
        }
        // Each tab keeps its own status line: park the outgoing one, restore the incoming one.
        let incoming = std::mem::take(&mut self.tab_status[mode_slot(mode)]);
        self.tab_status[mode_slot(self.mode)] = std::mem::replace(&mut self.status, incoming);
        self.mode = mode;
    }

    fn handle_shortcuts(&mut self, keys: &shell::Shortcuts) {
        // An open confirmation takes the keyboard: Esc cancels it (Enter is in the dialog).
        if self.confirm.is_some() {
            if keys.escape {
                self.confirm = None;
            }
            return;
        }
        if keys.undo {
            self.undo_current_tab();
        }
        if let Some((mode, _)) = keys.tab.and_then(|i| MODES.get(i)) {
            self.set_mode(*mode);
        }
        if keys.help {
            self.open_help(self.page_help_topic());
        }
        if keys.generate && self.primary_enabled() {
            self.primary_action();
        }
        if keys.load {
            self.load_action();
        }
        // Redo exists for Map drawings only.
        if self.mode == AppMode::Map {
            if keys.redo {
                self.redo_last_mark();
            }
            if let Some(i) = keys.map_tool {
                self.pick_map_tool(MAP_TOOLS[i].0);
            }
            if keys.escape {
                self.cancel_map_tool();
            }
        }
    }

    fn primary_enabled(&self) -> bool {
        shell::all_ok(&self.readiness_checks())
    }

    /// The smallest output each tab allows (README §10). These mirror the
    /// errors the generate functions already return; Generate stays disabled
    /// until all pass. Map has none: any period makes a valid base map.
    fn readiness_checks(&self) -> Vec<shell::Check> {
        use shell::Check;
        match self.mode {
            AppMode::Template => vec![Check::new(!self.tpl_seats.is_empty(), "At least one unit")],
            AppMode::Recon => match self.recon_submode {
                ReconSubmode::New => {
                    let mut checks = vec![Check::new(!self.recon_slots.is_empty(), "At least one template")];
                    if !self.recon_slots.is_empty() {
                        let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
                        checks.push(Check::new(
                            weights.iter().any(|w| *w > 0),
                            "Influence above 0 on at least one template",
                        ));
                        let copies = allocate_copies(&weights, self.recon_total as usize);
                        for (slot, n) in self.recon_slots.iter().zip(copies) {
                            if n > 0 {
                                checks.push(Check::new(
                                    !slot.selected_triggers.is_empty(),
                                    format!("Zone In for {}", slot.info.name),
                                ));
                            }
                        }
                    }
                    checks
                }
                ReconSubmode::Rework => {
                    let mut checks = vec![Check::new(!self.recon_rework.is_empty(), "At least one placed pack")];
                    for slot in &self.recon_rework {
                        checks.push(if self.recon_strip_randomizer {
                            Check::new(
                                !slot.restore_start.is_empty(),
                                format!("Start timer or checkzone for {}", slot.info.name),
                            )
                        } else {
                            Check::new(!slot.selected_triggers.is_empty(), format!("Zone In for {}", slot.info.name))
                        });
                    }
                    checks
                }
            },
            AppMode::Fighter => vec![Check::new(
                !self.selected_types().0.is_empty(),
                "At least one aircraft type",
            )],
            AppMode::Exclusive => {
                let mut checks = vec![Check::new(!self.bomber_slots.is_empty(), "At least one plan")];
                for (n, slot) in self.bomber_slots.iter().enumerate() {
                    checks.push(Check::new(
                        !slot.selected_triggers.is_empty(),
                        format!("Plan {}: a start checkzone", n + 1),
                    ));
                    checks.push(Check::new(
                        slot.selected_completion.is_some(),
                        format!("Plan {}: an end timer", n + 1),
                    ));
                }
                checks
            }
            AppMode::Airfield => vec![Check::new(self.airfield_info.is_some(), "An airfield file loaded")],
            AppMode::Map => Vec::new(),
        }
    }

    /// The header's primary button and Ctrl G.
    fn primary_action(&mut self) {
        let mode = self.mode;
        self.run_io(mode, |s| s.primary_action_inner());
    }

    fn primary_action_inner(&mut self) {
        match self.mode {
            AppMode::Template => self.generate_unit_template(),
            AppMode::Recon => match self.recon_submode {
                ReconSubmode::New => self.generate_recon_file(),
                ReconSubmode::Rework => self.generate_rework_file(),
            },
            AppMode::Fighter => self.generate_fighter_file(),
            AppMode::Exclusive => self.generate_bomber_file(),
            AppMode::Airfield => self.export_airfield(),
            AppMode::Map => self.generate_front_file(),
        }
    }

    /// The header's first secondary button and Ctrl O. Fighter Pack has none.
    fn load_action(&mut self) {
        match self.mode {
            AppMode::Template if self.is_dirty(AppMode::Template) => self.confirm = Some(Confirm::LoadTemplate),
            AppMode::Template => self.load_template_now(),
            AppMode::Map if self.is_dirty(AppMode::Map) => self.confirm = Some(Confirm::LoadBaseMap),
            AppMode::Map => self.run_io(AppMode::Map, |s| s.load_base_map()),
            AppMode::Recon => match self.recon_submode {
                ReconSubmode::New => self.add_recon_template(),
                ReconSubmode::Rework => self.add_placed_packs(),
            },
            AppMode::Fighter => {}
            AppMode::Exclusive => self.add_bomber_template(),
            AppMode::Airfield => self.load_airfield(),
        }
    }

    fn page_header_bar(&mut self, ui: &mut egui::Ui) {
        fn file_label(path: &Option<PathBuf>, stem_only: bool) -> Option<String> {
            let p = path.as_ref()?;
            let name = if stem_only { p.file_stem() } else { p.file_name() };
            name.and_then(|n| n.to_str()).map(str::to_owned)
        }
        let (title, file, primary) = match self.mode {
            AppMode::Template => (
                "Template Builder",
                file_label(&self.tpl_loaded_path, true).map(|n| format!("Editing {n}")),
                "Generate File",
            ),
            AppMode::Recon => ("Army Generator", None, "Generate File"),
            AppMode::Fighter => (
                "Fighter Pack",
                Some(file_label(&self.custom_path, false).unwrap_or_else(|| "Built-in logic".into())),
                "Generate File",
            ),
            AppMode::Exclusive => (
                "Exclusive Activation",
                // Only while plans from that generated pack are still listed.
                self.bomber_loaded_path
                    .as_ref()
                    .filter(|p| self.bomber_slots.iter().any(|s| &s.path == *p))
                    .and_then(|p| p.file_stem())
                    .and_then(|n| n.to_str())
                    .map(|n| format!("Editing {n}")),
                "Generate File",
            ),
            AppMode::Airfield => (
                "Airfield to Multiplayer",
                file_label(&self.airfield_path, false),
                "Generate File",
            ),
            AppMode::Map => {
                let battle = self
                    .front_focus
                    .and_then(|id| BATTLES.iter().find(|b| b.id == id))
                    .map_or("Entire front", |b| b.name);
                let period = format!("{} {} · {battle}", self.front_season.label(), self.front_year);
                ("Map", Some(period), "Generate Base Map")
            }
        };
        let mode = self.mode;
        let submode = self.recon_submode;
        let mut new_submode = submode;
        let (mut load, mut reset, mut add_folder) = (false, false, false);
        let checks = self.readiness_checks();
        let generate = shell::page_header(
            ui,
            title,
            file.as_deref(),
            primary,
            shell::all_ok(&checks),
            &checks,
            |ui| {
                if mode == AppMode::Recon {
                    shell::segmented(
                        ui,
                        &mut new_submode,
                        &[
                            (ReconSubmode::New, "New from templates"),
                            (ReconSubmode::Rework, "Rework existing"),
                        ],
                    );
                }
            },
            // Right-to-left: the button added last sits leftmost.
            |ui| match mode {
                AppMode::Template => {
                    reset = ui
                        .button("Reset")
                        .on_hover_text("Clear units and options so you can author a new template from scratch. Catalog stays.")
                        .clicked();
                    load = ui
                        .button("Load…")
                        .on_hover_text("Open a .Group to edit it here. Files this mode wrote load as-is; other layouts are rebuilt from units and orders.  (Ctrl O)")
                        .clicked();
                }
                AppMode::Recon => match submode {
                    ReconSubmode::New => {
                        add_folder = ui.button("Add folder…").clicked();
                        load = ui.button("Add templates…").on_hover_text("Ctrl O").clicked();
                    }
                    ReconSubmode::Rework => {
                        load = ui.button("Add packs…").on_hover_text("Ctrl O").clicked();
                    }
                },
                AppMode::Fighter => {
                    reset = ui
                        .button("Reset")
                        .on_hover_text("Back to the default pack, flights, types, country, altitudes and timers.")
                        .clicked();
                }
                AppMode::Exclusive => {
                    load = ui
                        .button("Add templates…")
                        .on_hover_text("Add templates or a generated Exclusive Activation pack.  (Ctrl O)")
                        .clicked();
                }
                AppMode::Airfield => {
                    load = ui.button("Load airfield…").on_hover_text("Ctrl O").clicked();
                }
                AppMode::Map => {
                    load = ui
                        .button("Load base map…")
                        .on_hover_text("Reload a previously generated Korea_BaseMap_*.Group: AO, front, attack arrows, and unit/fighter placement. Objectives are not stored in the file.  (Ctrl O)")
                        .clicked();
                }
            },
        );
        self.recon_submode = new_submode;
        if reset {
            self.confirm = match mode {
                AppMode::Fighter => Some(Confirm::ResetFighter),
                _ if self.tpl_seats.is_empty() => {
                    self.reset_template_confirmed();
                    None
                }
                _ => Some(Confirm::ResetTemplate),
            };
        }
        if add_folder {
            self.add_recon_folder();
        }
        if load {
            self.load_action();
        }
        if generate {
            self.primary_action();
        }
    }

    /// Template tab (README §5.1): add units and the unit list on the left, the
    /// formation view over the order tree in the center, the selection and
    /// settings on the right. Each panel scrolls on its own.
    fn template_page(&mut self, ctx: &egui::Context) {
        self.sync_template_zone_defaults();
        egui::SidePanel::left("tpl_left")
            .exact_width(270.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("tpl_left_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        self.template_add_units_section(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        self.template_units_section(ui);
                    });
            });
        egui::SidePanel::right("tpl_right")
            .exact_width(304.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("tpl_right_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Composite widgets (slider + value) do not wrap, so keep them narrow.
                        ui.spacing_mut().slider_width = 96.0;
                        ui.add_space(6.0);
                        self.template_selection_block(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        self.template_bring_up_section(ui);
                        self.template_placement_section(ui);
                        self.template_waypoints_section(ui);
                        self.template_catalog_section(ui);
                    });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("tpl_tree")
                    .exact_height(236.0)
                    .show_inside(ui, |ui| self.template_order_tree(ui));
                self.draw_template_schematic(ui);
            });
    }

    /// The seat the current selection belongs to (unit, order or event).
    fn selected_tpl_seat(&self) -> Option<usize> {
        match self.tpl_select {
            Some(
                TplSelect::Seat(s) | TplSelect::Order { seat: s, .. } | TplSelect::Event { seat: s, .. },
            ) if s < self.tpl_seats.len() => Some(s),
            _ => None,
        }
    }

    fn template_catalog_label(&self) -> String {
        self.tpl_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .map_or_else(|| "Built-in catalog".to_string(), str::to_owned)
    }

    fn use_builtin_catalog(&mut self) {
        self.tpl_path = None;
        self.tpl_catalog = bundled_catalog();
        self.tpl_class = None;
        self.tpl_country = None;
        self.tpl_add_pick = 0;
        self.tpl_preview_from_catalog = false;
    }

    fn template_catalog_section(&mut self, ui: &mut egui::Ui) {
        let summary = if self.tpl_path.is_some() {
            self.template_catalog_label()
        } else {
            "Built-in".to_string()
        };
        shell::settings_section(ui, "tpl_catalog", "Catalog", &summary, false, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Load catalog…").clicked() {
                    self.load_unit_catalog();
                }
                if ui.button("Add group…").clicked() {
                    self.add_user_catalog_group();
                }
                if ui.button("Use built-in catalog").clicked() {
                    self.use_builtin_catalog();
                }
            });
            let label = self
                .tpl_path
                .as_ref()
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .unwrap_or("Built-in ModelTypes + Fixed Objects");
            ui.label(RichText::new(label).italics().color(c::NEUTRAL_700));
        });
    }

    fn template_add_units_section(&mut self, ui: &mut egui::Ui) {
        let catalog = self.template_catalog_label();
        ui.horizontal(|ui| {
            ui.label(section_heading("Add units"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button(RichText::new(format!("{catalog} ▾")).small().color(c::NEUTRAL_700), |ui| {
                    if ui.button("Load catalog…").clicked() {
                        self.load_unit_catalog();
                        ui.close();
                    }
                    if ui.button("Add group…").clicked() {
                        self.add_user_catalog_group();
                        ui.close();
                    }
                    if ui.button("Use built-in catalog").clicked() {
                        self.use_builtin_catalog();
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Unit catalog: load one, add a group to it, or go back to the built-in list.");
            });
        });
        ui.add_space(4.0);
        // Seven kinds do not fit one 270 px row: two rows of equal cells.
        let kinds: Vec<(CatalogKind, &str)> = CatalogKind::ALL.iter().map(|k| (*k, k.label())).collect();
        let mut kind = self.tpl_kind;
        if segmented_rows(ui, &mut kind, &kinds, 4) {
            self.tpl_kind = kind;
            self.tpl_class = None;
            self.tpl_country = None;
            self.tpl_add_pick = 0;
            self.tpl_preview_from_catalog = true;
        }
        ui.add_space(4.0);
        self.draw_template_model_browser(ui);
    }

    /// Adds one seat of `unit` and selects it, as "Add unit" always has.
    fn template_add_model(&mut self, unit: CatalogUnit) {
        append_seat(&mut self.tpl_seats, unit, self.tpl_per_group);
        self.tpl_select = Some(TplSelect::Seat(self.tpl_seats.len() - 1));
        self.tpl_preview_from_catalog = false;
        self.sync_template_waypoint_speed();
    }

    fn template_units_section(&mut self, ui: &mut egui::Ui) {
        let n = self.tpl_seats.len();
        shell::section_title(ui, &format!("Units · {n}"), (n > 1).then_some("drag to reorder"));
        if n == 0 {
            ui.label(
                RichText::new("No units yet. Pick a model above and click + Add.")
                    .small()
                    .color(c::NEUTRAL_700),
            );
            return;
        }
        if self.tpl_card_drag.is_some_and(|d| d >= n) {
            self.tpl_card_drag = None;
        }
        let selected = self.selected_tpl_seat();
        let mut rects = Vec::with_capacity(n);
        let mut clicked = None;
        let mut remove = None;
        for si in 0..n {
            let seat = &self.tpl_seats[si];
            let (tag, accent) = match seat.role {
                FlightRole::Lead => {
                    let size = 1 + (0..n).filter(|&j| self.tpl_seats[j].role == FlightRole::Follows(si)).count();
                    (format!("Lead ×{size}"), true)
                }
                FlightRole::Follows(lead) if is_follower(&self.tpl_seats, si) => {
                    (format!("Follows {}", lead + 1), false)
                }
                _ => (String::new(), false),
            };
            let tail = if receives_orders(&self.tpl_seats, si) {
                match seat.orders.len() {
                    1 => "1 order".to_string(),
                    k => format!("{k} orders"),
                }
            } else {
                // A wingman flies its lead's orders; its own are not written.
                "lead's orders".to_string()
            };
            let country_name = country_short(seat.country);
            // "601 USA" → "USA": the code is in the Country field when it matters.
            let country_name = country_name.split_once(' ').map_or(country_name.as_str(), |(_, n)| n.trim());
            let meta = format!("{country_name} · {} · {tail}", skill_name(seat.skill));
            let title = format!("{} · {}", si + 1, seat.unit.label());
            let country = seat.country;
            let resp = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .id_salt(("tpl_card", si))
                        .sense(Sense::click_and_drag()),
                    |ui| {
                        shell::card(ui, selected == Some(si), |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                paint_seat_marker(ui.painter(), r.center(), 12.0, country);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(title).font(FontId::new(13.0, theme::bold_family())),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("×").on_hover_text("Remove unit").clicked() {
                                        remove = Some(si);
                                    }
                                    if !tag.is_empty() {
                                        shell::tag(ui, &tag, accent);
                                    }
                                });
                            });
                            ui.label(RichText::new(meta).small().color(c::NEUTRAL_700));
                        });
                    },
                )
                .response;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected == Some(si), format!("Unit card {}", si + 1)));
            if resp.clicked() {
                clicked = Some(si);
            }
            if resp.drag_started() {
                self.tpl_card_drag = Some(si);
            }
            if resp.hovered() && self.tpl_card_drag.is_none() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            rects.push(resp.rect);
            ui.add_space(4.0);
        }
        let copy = ui
            .add_enabled(selected.is_some(), egui::Button::new("Copy attributes to all"))
            .on_hover_text(
                "Copy the selected unit's country, skill, fuel, and flags to every unit, and its altitude to every plane (capped at each plane's ceiling). Payload and modifications copy only to the same aircraft type.",
            )
            .clicked();
        if selected.is_none() {
            // The reason is shown, not only on hover (README §6.5).
            shell::hint(ui, "Select a unit to copy its attributes from.", false);
        }
        if let (true, Some(from)) = (copy, selected) {
            self.record_tpl_undo(format!("Copied attributes from {}", self.tpl_seats[from].unit.label()));
            copy_seat_attributes(&mut self.tpl_seats, from);
        }

        // Drag to reorder: an accent line marks where the card will land.
        if let Some(src) = self.tpl_card_drag {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let insert = ui.ctx().pointer_interact_pos().map(|p| {
                rects.iter().position(|r| p.y < r.center().y).unwrap_or(n)
            });
            if let Some(ins) = insert {
                let y = if ins < n { rects[ins].top() - 2.0 } else { rects[n - 1].bottom() + 2.0 };
                ui.painter().line_segment(
                    [Pos2::new(rects[0].left(), y), Pos2::new(rects[0].right(), y)],
                    Stroke::new(2.0_f32, c::ACCENT),
                );
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.tpl_card_drag = None;
                if let Some(ins) = insert {
                    let dest = if ins > src { ins - 1 } else { ins };
                    self.move_tpl_seat_to(src, dest);
                }
            }
        }
        if let Some(si) = clicked {
            self.tpl_select = Some(TplSelect::Seat(si));
            self.tpl_preview_from_catalog = false;
        }
        if let Some(si) = remove {
            self.record_tpl_undo(format!("Removed {}", self.tpl_seats[si].unit.label()));
            self.remove_tpl_seat(si);
        }
    }

    /// Moves a seat one step at a time so `move_seat` keeps roles and
    /// order targets consistent.
    fn move_tpl_seat_to(&mut self, mut from: usize, to: usize) {
        while from != to {
            let dir = if to > from { 1 } else { -1 };
            match move_seat(&mut self.tpl_seats, from, dir) {
                Some(dest) => {
                    swap_tpl_select(&mut self.tpl_select, from, dest);
                    from = dest;
                }
                None => break,
            }
        }
    }

    fn remove_tpl_seat(&mut self, si: usize) {
        if si >= self.tpl_seats.len() {
            return;
        }
        self.tpl_seats.remove(si);
        for seat in &mut self.tpl_seats {
            seat.role = match seat.role {
                FlightRole::Follows(t) if t == si => FlightRole::Independent,
                FlightRole::Follows(t) if t > si => FlightRole::Follows(t - 1),
                other => other,
            };
            for order in &mut seat.orders {
                remap_seat_index(&mut order.cover_lead, si);
                remap_seat_index(&mut order.attack_seat, si);
                remap_index_vec(&mut order.shared_with, si);
            }
        }
        clamp_tpl_select(&mut self.tpl_select, &self.tpl_seats);
        self.sync_template_waypoint_speed();
    }

    fn template_order_tree(&mut self, ui: &mut egui::Ui) {
        let seat = self.selected_tpl_seat();
        let unit_kind = seat.map_or(CatalogKind::Plane, |s| self.tpl_seats[s].unit.kind);
        let can_orders = seat.is_some_and(|s| receives_orders(&self.tpl_seats, s));
        let mut add = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(section_heading("Order tree"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(seat.is_some(), |ui| {
                    tree_event_menu(ui, seat.unwrap_or(0), unit_kind, &mut add);
                });
                ui.add_enabled_ui(can_orders, |ui| {
                    tree_order_menu(ui, seat.unwrap_or(0), unit_kind, &mut add);
                });
            });
        });
        let hint_h = 24.0;
        let max_h = (ui.available_height() - hint_h).max(40.0);
        ui.scope(|ui| {
            // A solid bar (not egui's hover-only floating one) shows that a
            // long chain continues past the right edge.
            ui.spacing_mut().scroll = egui::style::ScrollStyle {
                foreground_color: true,
                dormant_handle_opacity: 0.35,
                dormant_background_opacity: 0.3,
                ..egui::style::ScrollStyle::solid()
            };
            egui::ScrollArea::both()
                .id_salt("tpl_tree_scroll")
                .auto_shrink([false, false])
                .max_height(max_h)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| self.draw_template_seat_list(ui));
        });
        if shell::hint(
            ui,
            "UNIT → OnSpawned → orders. ‹ › move the selected chip.",
            true,
        ) {
            self.open_help(HelpTopic::Template);
        }
        if let Some(item) = add {
            self.apply_tree_add(item);
        }
    }

    fn apply_tree_add(&mut self, item: AddTreeItem) {
        match item {
            AddTreeItem::Order { seat: si, kind } => {
                let unit = self.tpl_seats[si].unit.clone();
                let next_wp = next_waypoint_number(&self.tpl_seats);
                self.tpl_seats[si]
                    .orders
                    .push(order_spec_for_added_kind(&unit, kind, next_wp));
                if kind == OrderKind::GotoWaypoint {
                    if let Some(spec) = self.tpl_seats[si].orders.last_mut() {
                        spec.priority = self.tpl_wp_priority;
                    }
                }
                let oi = self.tpl_seats[si].orders.len() - 1;
                if kind == OrderKind::AttackArea {
                    apply_suggested_attack_area(&mut self.tpl_seats, si, oi);
                }
                let oi = {
                    let seat = &mut self.tpl_seats[si];
                    normalize_order_chain(&mut seat.orders, &mut seat.events, oi)
                };
                self.tpl_select = Some(TplSelect::Order { seat: si, order: oi });
            }
            AddTreeItem::Event { seat: si, kind } => {
                let mut hook = EventHook::default_for(self.tpl_seats[si].unit.kind);
                hook.kind = kind;
                self.tpl_seats[si].events.push(hook);
                let ei = self.tpl_seats[si].events.len() - 1;
                self.tpl_select = Some(TplSelect::Event { seat: si, event: ei });
            }
        }
        self.tpl_preview_from_catalog = false;
    }

    /// Right panel, top: what is selected, in large type, then its fields.
    /// With nothing selected (or a catalog model picked) it previews that model.
    fn template_selection_block(&mut self, ui: &mut egui::Ui) {
        let kicker = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text.to_uppercase()).small().color(c::NEUTRAL_700));
        };
        let title = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text).font(FontId::new(22.0, theme::heading_family())));
        };
        let seat = self.selected_tpl_seat();
        let Some(si) = seat.filter(|_| !self.tpl_preview_from_catalog) else {
            let unit = self.displayed_catalog().get(self.tpl_add_pick).cloned();
            kicker(ui, "Catalog");
            title(ui, unit.as_ref().map_or("No model", |u| u.label()));
            self.draw_model_preview(ui, unit.as_ref(), None, None, None);
            if self.tpl_seats.is_empty() {
                ui.add_space(6.0);
                if shell::hint(ui, "One proximity-triggered unit group per file.", true) {
                    self.open_help(HelpTopic::Template);
                }
            }
            return;
        };
        match self.tpl_select {
            Some(TplSelect::Order { order, .. }) if order < self.tpl_seats[si].orders.len() => {
                kicker(ui, &format!("Selected · Unit {} · Order {}", si + 1, order + 1));
                title(ui, self.tpl_seats[si].orders[order].kind.label());
            }
            Some(TplSelect::Event { event, .. }) if event < self.tpl_seats[si].events.len() => {
                kicker(ui, &format!("Selected · Unit {} · Event {}", si + 1, event + 1));
                title(ui, self.tpl_seats[si].events[event].kind.label());
            }
            _ => {
                let unit = self.tpl_seats[si].unit.clone();
                kicker(ui, &format!("Selected · Unit {}", si + 1));
                title(ui, unit.label());
                let payload = payloads::payload_preview(&unit.script, self.tpl_seats[si].payload_id);
                let mods = payloads::mods_preview(&unit.script, &self.tpl_seats[si].mod_mask);
                let status = unit.is_air().then(|| {
                    PlaneStart::from_i32(self.tpl_seats[si].start_type)
                        .preview_status(self.tpl_seats[si].altitude)
                });
                self.draw_model_preview(ui, Some(&unit), Some(&payload), Some(&mods), status.as_deref());
                ui.add_space(6.0);
            }
        }
        self.draw_template_details(ui);
    }

    fn template_bring_up_section(&mut self, ui: &mut egui::Ui) {
        let flights_need_activate = has_linked_wingmen(&self.tpl_seats);
        if flights_need_activate && self.tpl_bring_up == BringUp::Spawn {
            self.tpl_bring_up = BringUp::Activate;
        }
        let summary = match self.tpl_bring_up {
            BringUp::Activate if flights_need_activate => "Activate · required by wingmen".to_string(),
            BringUp::Activate => "Activate".to_string(),
            BringUp::Spawn if self.tpl_spawn_reset => {
                format!("Spawn · repeat {:.0} min", self.tpl_spawn_cooldown_min)
            }
            BringUp::Spawn => "Spawn · once".to_string(),
        };
        shell::settings_section(ui, "tpl_bring_up", "Activate or Spawn", &summary, false, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for mode in [BringUp::Activate, BringUp::Spawn] {
                    let spawn_locked = mode == BringUp::Spawn && flights_need_activate;
                    ui.add_enabled_ui(!spawn_locked, |ui| {
                        if ui
                            .selectable_label(self.tpl_bring_up == mode, mode.label())
                            .on_hover_text(match mode {
                                BringUp::Activate => {
                                    "Enable parked units. Required for flights with wingmen (Exclusive Activation)."
                                }
                                BringUp::Spawn => {
                                    "Spawn through a counter. Object-links the spawner to each unit entity. Best for many independent units, or the same units more than once."
                                }
                            })
                            .clicked()
                        {
                            self.tpl_bring_up = mode;
                        }
                    });
                }
            });
            if flights_need_activate {
                // The lock reason is shown, not only on hover (README §6.5).
                shell::warning(
                    ui,
                    "Spawn is off: a lead has wingmen, and a flight must be activated. Independent units in the file activate with it.",
                );
            }
            if self.tpl_bring_up == BringUp::Spawn {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.tpl_spawn_reset, "Allow multiple spawns")
                        .on_hover_text(
                            "Respawns after the cooldown once every unit is destroyed. Zone Out cleanup details are in Help › Template.",
                        );
                    if self.tpl_spawn_reset {
                        ui.label("Cooldown");
                        ui.add(
                            egui::Slider::new(&mut self.tpl_spawn_cooldown_min, 1.0..=60.0)
                                .suffix(" min")
                                .integer(),
                        );
                    }
                });
            }
        });
    }

    fn template_placement_section(&mut self, ui: &mut egui::Ui) {
        let summary = format!(
            "{} · In {:.1} km",
            self.tpl_place_layout.label(),
            self.tpl_zone_in / 1000.0
        );
        let mut help = false;
        shell::settings_section(ui, "tpl_placement", "Placement & Checkzones", &summary, true, |ui| {
            ui.spacing_mut().slider_width = 64.0;
            let visual = visual_range_m(self.template_zone_mix());
            let zone_label = |ui: &mut egui::Ui, text: &str, near: bool| {
                ui.vertical(|ui| {
                    field_label(ui, text);
                    if near {
                        ui.label(RichText::new("visual range").small().color(c::WARN_TEXT));
                    }
                });
            };
            let ctrl_w = field_control_w(ui);
            // Zone sliders show km with one decimal; the stored values stay metres.
            let km = |v: f64, _: std::ops::RangeInclusive<usize>| format!("{:.1} km", v / 1000.0);
            let parse_km = |t: &str| {
                let t = t.trim().trim_end_matches("km").trim();
                t.parse::<f64>().ok().map(|v| v * 1000.0)
            };
            field_grid(ui, "tpl_place_grid", |ui| {
                field_label(ui, "Layout");
                ui.horizontal(|ui| {
                    let prev_layout = self.tpl_place_layout;
                    let combo_w = (ctrl_w - 52.0).max(80.0);
                    ui.scope(|ui| {
                        // The combo truncates to the space it is given.
                        ui.set_max_width(combo_w);
                        egui::ComboBox::from_id_salt("tpl_place_layout")
                            .selected_text(self.tpl_place_layout.label())
                            .width(combo_w)
                            .truncate()
                            .show_ui(ui, |ui| {
                                for layout in PlaceLayout::ALL {
                                    ui.selectable_value(&mut self.tpl_place_layout, layout, layout.label());
                                }
                            });
                    });
                    if self.tpl_place_layout != prev_layout {
                        self.tpl_per_group = self.tpl_place_layout.default_per_group();
                    }
                    ui.add(egui::DragValue::new(&mut self.tpl_per_group).range(1..=8))
                        .on_hover_text("Units per group");
                });
                ui.end_row();

                field_label(ui, "Trigger coalition");
                egui::ComboBox::from_id_salt("tpl_zone_coalition")
                    .selected_text(self.tpl_zone_coalition.label())
                    .width(ctrl_w.min(160.0))
                    .show_ui(ui, |ui| {
                        for co in ZoneCoalition::ALL {
                            ui.selectable_value(&mut self.tpl_zone_coalition, co, co.label());
                        }
                    });
                ui.end_row();

                zone_label(ui, "Zone In", near_visual_range(self.tpl_zone_in, visual));
                ui.add(
                    egui::Slider::new(&mut self.tpl_zone_in, 500.0..=40_000.0)
                        .logarithmic(true)
                        .custom_formatter(km)
                        .custom_parser(parse_km),
                );
                ui.end_row();

                if self.tpl_zone_out < self.tpl_zone_in + 200.0 {
                    self.tpl_zone_out = self.tpl_zone_in + 200.0;
                }
                zone_label(ui, "Zone Out", near_visual_range(self.tpl_zone_out, visual));
                ui.add(
                    egui::Slider::new(&mut self.tpl_zone_out, 700.0..=50_000.0)
                        .logarithmic(true)
                        .custom_formatter(km)
                        .custom_parser(parse_km),
                );
                ui.end_row();
            });
            help = shell::hint(ui, "Inverted Vee is finger-four. Spacing 150 m.", true);
        });
        if help {
            self.open_help(HelpTopic::Template);
        }
    }

    fn template_waypoints_section(&mut self, ui: &mut egui::Ui) {
        let n = used_waypoint_count(&self.tpl_seats);
        let shown_alt = path_waypoint_display_m(&self.tpl_seats, self.tpl_wp_altitude);
        let summary = format!("{n} · {:.0} km/h · {shown_alt:.0} m", self.tpl_wp_speed);
        shell::settings_section(ui, "tpl_waypoints", "Waypoints", &summary, false, |ui| {
            egui::Grid::new("tpl_wp_grid")
                .num_columns(2)
                .min_col_width(FIELD_LABEL_W)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Waypoints");
                    ui.label(if n == 0 {
                        "None (add a Goto WP order)".to_string()
                    } else {
                        format!("{n} from Goto WP orders")
                    });
                    ui.end_row();

                    ui.label("Speed");
                    ui.add(egui::DragValue::new(&mut self.tpl_wp_speed).suffix(" km/h"))
                        .on_hover_text(
                            "MCU Speed in km/h. No editor limit. Defaults to 90% of the slowest unit’s cruise, rounded to 10 km/h.",
                        );
                    ui.end_row();

                    ui.label("Altitude");
                    let max_alt = self
                        .tpl_seats
                        .iter()
                        .filter(|s| s.unit.is_air())
                        .map(|s| model_spec::ceiling_m(&s.unit.script))
                        .fold(0.0_f32, f32::max);
                    let mut shown_alt = shown_alt;
                    let alt_range = if max_alt > 0.0 { 0.0..=max_alt } else { 0.0..=f32::MAX };
                    if ui
                        .add(egui::DragValue::new(&mut shown_alt).range(alt_range).suffix(" m"))
                        .on_hover_text("0 m for ground units. Aircraft follow spawn height until you enter a value.")
                        .changed()
                    {
                        self.tpl_wp_altitude = shown_alt;
                    }
                    ui.end_row();

                    let mut pri = self.tpl_wp_priority;
                    draw_priority_combo(ui, "tpl_wp_pri".into(), &mut pri);
                    ui.end_row();
                    if pri != self.tpl_wp_priority {
                        self.tpl_wp_priority = pri;
                        for seat in &mut self.tpl_seats {
                            for order in &mut seat.orders {
                                if order.kind == OrderKind::GotoWaypoint {
                                    order.priority = pri;
                                }
                            }
                        }
                    }
                });
        });
    }

    /// Order tree rows: one per unit, unit chip → OnSpawned → orders.
    fn draw_template_seat_list(&mut self, ui: &mut egui::Ui) {
        // Fills tell commands, specials and reports apart; selection is accent.
        let selected_fill = c::ACCENT_200;
        let order_fill = c::NEUTRAL_100;
        let report_fill = c::NEUTRAL_300;
        let chain_fill = c::NEUTRAL_200;
        let event_fill = c::BG;
        let mut remove_order = None;
        let mut remove_event = None;
        let mut move_order: Option<(usize, usize, i32)> = None;
        let mut move_seat_dir: Option<(usize, i32)> = None;
        let mut clicked = None;
        let mut change_unit: Option<(usize, CatalogUnit)> = None;

        if self.tpl_seats.is_empty() {
            ui.label(
                RichText::new("No units yet. Pick a model on the left and click + Add.")
                    .small()
                    .color(c::NEUTRAL_700),
            );
        }

        for si in 0..self.tpl_seats.len() {
            if si > 0 {
                ui.add_space(TREE_UNIT_GAP);
            }
            ui.horizontal_top(|ui| {
                let unit_sel = matches!(self.tpl_select, Some(TplSelect::Seat(s)) if s == si);
                let label = self.tpl_seats[si].unit.label().to_string();
                let current_script = self.tpl_seats[si].unit.script.clone();
                let kind = self.tpl_seats[si].unit.kind;
                let mut models: Vec<CatalogUnit> = self
                    .tpl_catalog
                    .iter()
                    .filter(|u| u.kind == kind)
                    .cloned()
                    .collect();
                models.sort_by(|a, b| a.label().cmp(b.label()));
                let response =
                    draw_tree_unit_chip(ui, si + 1, &label, self.tpl_seats[si].country, unit_sel)
                        .on_hover_text(format!("{label}. Click to change model (same kind)."));
                if response.clicked() {
                    clicked = Some(TplSelect::Seat(si));
                }
                egui::Popup::menu(&response).show(|ui| {
                    ui.set_max_height(280.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for unit in &models {
                            let selected = unit.script.eq_ignore_ascii_case(&current_script);
                            if ui.selectable_label(selected, unit.label()).clicked() {
                                change_unit = Some((si, unit.clone()));
                                ui.close();
                            }
                        }
                    });
                });
                let this_seat = matches!(
                    self.tpl_select,
                    Some(TplSelect::Seat(s) | TplSelect::Order { seat: s, .. } | TplSelect::Event { seat: s, .. })
                        if s == si
                );
                if this_seat {
                    ui.add_enabled_ui(si > 0, |ui| {
                        if move_row_button(ui, true).on_hover_text("Move unit up").clicked() {
                            move_seat_dir = Some((si, -1));
                        }
                    });
                    ui.add_enabled_ui(si + 1 < self.tpl_seats.len(), |ui| {
                        if move_row_button(ui, false).on_hover_text("Move unit down").clicked() {
                            move_seat_dir = Some((si, 1));
                        }
                    });
                }
                if receives_orders(&self.tpl_seats, si) {
                    let laid = order_tree_layout(
                        &self.tpl_seats[si].orders,
                        &self.tpl_seats[si].events,
                    );
                    let (columns, trailing) = split_trailing_events(laid);
                    draw_order_tree_columns(
                        ui,
                        si,
                        &columns,
                        &self.tpl_seats,
                        self.tpl_select,
                        selected_fill,
                        order_fill,
                        report_fill,
                        chain_fill,
                        event_fill,
                        &mut clicked,
                        &mut remove_order,
                        &mut remove_event,
                        &mut move_order,
                    );
                    if !trailing.is_empty() {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 8.0;
                            for node in trailing {
                                if let OrderTreeNode::Event(ei) = node {
                                    let selected = matches!(
                                        self.tpl_select,
                                        Some(TplSelect::Event { seat, event })
                                            if seat == si && event == ei
                                    );
                                    draw_template_event_chip(
                                        ui,
                                        si,
                                        ei,
                                        self.tpl_seats[si].events[ei].kind,
                                        selected,
                                        selected_fill,
                                        event_fill,
                                        &mut clicked,
                                        &mut remove_event,
                                    );
                                }
                            }
                        });
                    }
                } else {
                    ui.label(RichText::new("follows lead").small().color(c::NEUTRAL_700));
                    if !self.tpl_seats[si].events.is_empty() {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 8.0;
                            for ei in 0..self.tpl_seats[si].events.len() {
                                let selected = matches!(
                                    self.tpl_select,
                                    Some(TplSelect::Event { seat, event })
                                        if seat == si && event == ei
                                );
                                draw_template_event_chip(
                                    ui,
                                    si,
                                    ei,
                                    self.tpl_seats[si].events[ei].kind,
                                    selected,
                                    selected_fill,
                                    event_fill,
                                    &mut clicked,
                                    &mut remove_event,
                                );
                            }
                        });
                    }
                }
            });
        }

        if let Some(sel) = clicked {
            self.tpl_select = Some(sel);
            self.tpl_preview_from_catalog = false;
        }
        if let Some((si, oi)) = remove_order {
            self.remove_tpl_order(si, oi);
        }
        if let Some((si, ei)) = remove_event {
            self.remove_tpl_event(si, ei);
        }
        if let Some((si, oi, dir)) = move_order {
            let n = self.tpl_seats[si].orders.len();
            let dest = oi as i32 + dir;
            if dest >= 0 && (dest as usize) < n {
                self.tpl_seats[si].orders.swap(oi, dest as usize);
                let new_oi = {
                    let seat = &mut self.tpl_seats[si];
                    normalize_order_chain(&mut seat.orders, &mut seat.events, dest as usize)
                };
                self.tpl_select = Some(TplSelect::Order {
                    seat: si,
                    order: new_oi,
                });
            }
        }
        if let Some((si, dir)) = move_seat_dir {
            if let Some(dest) = move_seat(&mut self.tpl_seats, si, dir) {
                swap_tpl_select(&mut self.tpl_select, si, dest);
            }
        }
        if let Some((si, unit)) = change_unit {
            if si < self.tpl_seats.len() {
                replace_seat_unit(&mut self.tpl_seats[si], unit);
                if self.tpl_seats[si].unit.is_air() {
                    let ceil = model_spec::ceiling_m(&self.tpl_seats[si].unit.script);
                    self.tpl_seats[si].altitude = self.tpl_seats[si].altitude.min(ceil);
                    self.tpl_seats[si].start_type = PlaneStart::stored_for_altitude(
                        self.tpl_seats[si].start_type,
                        self.tpl_seats[si].altitude,
                    );
                }
                refresh_attack_areas_for_seat(&mut self.tpl_seats, si);
                self.sync_template_waypoint_speed();
                self.tpl_select = Some(TplSelect::Seat(si));
                self.tpl_preview_from_catalog = false;
            }
        }
    }

    fn draw_train_carriages(&mut self, ui: &mut egui::Ui, si: usize) {
        ui.add_space(4.0);
        ui.label(RichText::new("Carriages").strong());
        ui.label(
            RichText::new(
                "The locomotive is the selected train. Add, remove, or reorder cars. Tender first is typical. Carriage styles (Hospital, boxcars, wagons, platforms) are also on Modifications above.",
            )
            .italics()
            .small(),
        );
        let mut remove_at: Option<usize> = None;
        let mut move_at: Option<(usize, i32)> = None;
        let n = self.tpl_seats[si].carriages.len();
        if n == 0 {
            ui.label(
                RichText::new("Locomotive only — add cars below.")
                    .italics()
                    .small(),
            );
        }
        for ci in 0..n {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{}.", ci + 1));
                ui.label(carriage_label(&self.tpl_seats[si].carriages[ci]));
                let train_script = self.tpl_seats[si].unit.script.clone();
                let car_script = self.tpl_seats[si].carriages[ci].clone();
                draw_carriage_style(
                    ui,
                    format!("tpl_car_mod_{si}_{ci}"),
                    &train_script,
                    &car_script,
                    &mut self.tpl_seats[si].mod_mask,
                );
                ui.add_enabled_ui(ci > 0, |ui| {
                    if move_row_button(ui, true).on_hover_text("Move carriage up").clicked() {
                        move_at = Some((ci, -1));
                    }
                });
                ui.add_enabled_ui(ci + 1 < n, |ui| {
                    if move_row_button(ui, false).on_hover_text("Move carriage down").clicked() {
                        move_at = Some((ci, 1));
                    }
                });
                if ui.button("×").on_hover_text("Remove carriage").clicked() {
                    remove_at = Some(ci);
                }
            });
        }
        if let Some((ci, dir)) = move_at {
            let dest = if dir < 0 { ci - 1 } else { ci + 1 };
            if dest < self.tpl_seats[si].carriages.len() {
                self.tpl_seats[si].carriages.swap(ci, dest);
            }
        }
        if let Some(ci) = remove_at {
            if ci < self.tpl_seats[si].carriages.len() {
                self.tpl_seats[si].carriages.remove(ci);
                sync_train_mask_to_carriages(&mut self.tpl_seats[si]);
            }
        }
        let mut choices = self.tpl_seats[si].unit.prototype_carriages();
        for extra in catalog_carriage_scripts(&self.tpl_catalog) {
            if !choices.iter().any(|s| s.eq_ignore_ascii_case(&extra)) {
                choices.push(extra);
            }
        }
        if !choices.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Add");
                egui::ComboBox::from_id_salt(format!("tpl_add_car_{si}"))
                    .selected_text("carriage…")
                    .width(240.0)
                    .height(280.0)
                    .show_ui(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                for script in &choices {
                                    if ui
                                        .selectable_label(false, carriage_label(script))
                                        .clicked()
                                    {
                                        self.tpl_seats[si].carriages.push(script.clone());
                                    }
                                }
                            });
                    });
            });
        }
    }

    /// Selection fields (README §5.1): one label / control pair per row in a
    /// 96 px label grid; explanations follow the grid in small type.
    fn draw_template_details(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        match self.tpl_select {
            Some(TplSelect::Seat(si)) if si < self.tpl_seats.len() => self.draw_template_seat_fields(ui, si),
            Some(TplSelect::Order { seat, order })
                if seat < self.tpl_seats.len() && order < self.tpl_seats[seat].orders.len() =>
            {
                self.draw_template_order_fields(ui, seat, order);
            }
            Some(TplSelect::Event { seat, event })
                if seat < self.tpl_seats.len() && event < self.tpl_seats[seat].events.len() =>
            {
                self.draw_template_event_fields(ui, seat, event);
            }
            _ => {
                ui.label(
                    RichText::new("Select a unit card, an order chip, or a unit in the formation view.")
                        .color(c::NEUTRAL_700),
                );
            }
        }
    }

    fn draw_template_seat_fields(&mut self, ui: &mut egui::Ui, si: usize) {
        let ctrl_w = field_control_w(ui);
        let mut moved = false;
        let mut payload_note = None;
        field_grid(ui, ("tpl_seat_fields", si), |ui| {
            field_label(ui, "Position");
            ui.horizontal(|ui| {
                ui.label(format!("{} · {}", self.tpl_seats[si].unit.kind.label(), si + 1));
                ui.add_enabled_ui(si > 0, |ui| {
                    if move_row_button(ui, true).on_hover_text("Move unit up").clicked() {
                        if let Some(dest) = move_seat(&mut self.tpl_seats, si, -1) {
                            swap_tpl_select(&mut self.tpl_select, si, dest);
                        }
                        moved = true;
                    }
                });
                ui.add_enabled_ui(si + 1 < self.tpl_seats.len(), |ui| {
                    if move_row_button(ui, false).on_hover_text("Move unit down").clicked() {
                        if let Some(dest) = move_seat(&mut self.tpl_seats, si, 1) {
                            swap_tpl_select(&mut self.tpl_select, si, dest);
                        }
                        moved = true;
                    }
                });
            });
            ui.end_row();
            if moved {
                // `si` now names another seat: draw the rest next frame.
                return;
            }

            field_label(ui, "Role");
            let follow_choices: Vec<(usize, String)> = lead_indexes(&self.tpl_seats)
                .into_iter()
                .filter(|&i| i != si)
                .map(|i| (i, format!("Follows seat {} ({})", i + 1, self.tpl_seats[i].unit.label())))
                .collect();
            let current = match self.tpl_seats[si].role {
                FlightRole::Independent => "Independent".to_string(),
                FlightRole::Lead => "Lead".to_string(),
                FlightRole::Follows(i) => format!("Follows seat {}", i + 1),
            };
            egui::ComboBox::from_id_salt(format!("tpl_role_{si}"))
                .selected_text(current)
                .width(ctrl_w)
                .truncate()
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.tpl_seats[si].role == FlightRole::Independent, "Independent")
                        .clicked()
                    {
                        let was_lead = self.tpl_seats[si].role == FlightRole::Lead;
                        self.tpl_seats[si].role = FlightRole::Independent;
                        if was_lead {
                            for (j, seat) in self.tpl_seats.iter_mut().enumerate() {
                                if j != si && seat.role == FlightRole::Follows(si) {
                                    seat.role = FlightRole::Independent;
                                }
                            }
                        }
                    }
                    if ui
                        .selectable_label(self.tpl_seats[si].role == FlightRole::Lead, "Lead")
                        .on_hover_text("Wingmen can follow this unit. Orders go only to the lead.")
                        .clicked()
                    {
                        self.tpl_seats[si].role = FlightRole::Lead;
                        self.tpl_seats[si].number_in_formation = 0;
                        let count = if self.tpl_seats[si].formation_count == 0 {
                            self.tpl_per_group
                        } else {
                            self.tpl_seats[si].formation_count
                        };
                        apply_formation_numbers(&mut self.tpl_seats, si, count);
                    }
                    for (i, text) in &follow_choices {
                        if ui
                            .selectable_label(self.tpl_seats[si].role == FlightRole::Follows(*i), text)
                            .clicked()
                        {
                            self.tpl_seats[si].role = FlightRole::Follows(*i);
                            let n = self
                                .tpl_seats
                                .iter()
                                .enumerate()
                                .filter(|(j, s)| *j != si && s.role == FlightRole::Follows(*i))
                                .count() as i32
                                + 1;
                            self.tpl_seats[si].number_in_formation = n;
                        }
                    }
                });
            ui.end_row();

            if self.tpl_seats[si].role == FlightRole::Lead {
                field_label(ui, "In formation");
                let mut count = if self.tpl_seats[si].formation_count == 0 {
                    self.tpl_per_group
                } else {
                    self.tpl_seats[si].formation_count.min(self.tpl_per_group)
                };
                if ui
                    .add(egui::DragValue::new(&mut count).range(1..=self.tpl_per_group))
                    .on_hover_text("Number in this flight (0 = lead). Mods the per-group count.")
                    .changed()
                {
                    apply_formation_numbers(&mut self.tpl_seats, si, count);
                }
                ui.end_row();
            }

            field_label(ui, "Country");
            egui::ComboBox::from_id_salt(format!("tpl_seat_country_{si}"))
                .selected_text(
                    COUNTRIES
                        .iter()
                        .find(|(id, _)| *id == self.tpl_seats[si].country)
                        .map(|(_, l)| *l)
                        .unwrap_or("?"),
                )
                .width(ctrl_w)
                .truncate()
                .show_ui(ui, |ui| {
                    for (id, label) in COUNTRIES {
                        ui.selectable_value(&mut self.tpl_seats[si].country, *id, *label);
                    }
                });
            ui.end_row();

            field_label(ui, "Skill");
            ui.add(
                egui::Slider::new(&mut self.tpl_seats[si].skill, 0..=4)
                    .custom_formatter(|v, _| format!("{v:.0} {}", skill_name(v as i32)))
                    .custom_parser(|t| t.split_whitespace().next().and_then(|n| n.parse::<f64>().ok())),
            )
            .on_hover_text("AILevel 0–4: Plain, Low, Normal, High, Ace");
            ui.end_row();

            if self.tpl_seats[si].unit.is_air() {
                field_label(ui, "Start");
                let airborne = self.tpl_seats[si].altitude > 0.0;
                let shown = if airborne {
                    PlaneStart::Air
                } else {
                    PlaneStart::from_i32(self.tpl_seats[si].start_type)
                };
                let mut choice = shown;
                egui::ComboBox::from_id_salt(format!("tpl_seat_start_{si}"))
                    .selected_text(shown.label())
                    .width(ctrl_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for start in std::iter::once(PlaneStart::Air).chain(PlaneStart::GROUND) {
                            ui.selectable_value(&mut choice, start, start.label());
                        }
                    })
                    .response
                    .on_hover_text(format!(
                        "Airstart puts this plane at {AIR_START_ALTITUDE_M:.0} m, the same height for every aircraft. The slider changes this plane. Running, Warm, and Cold stay on the ground."
                    ));
                if choice != shown {
                    apply_plane_start(&mut self.tpl_seats[si], choice);
                }
                ui.end_row();
                if self.tpl_seats[si].altitude > 0.0 {
                    field_label(ui, "Altitude");
                    let ceiling = model_spec::ceiling_m(&self.tpl_seats[si].unit.script);
                    ui.add(
                        egui::Slider::new(&mut self.tpl_seats[si].altitude, 0.0..=ceiling)
                            .suffix(" m")
                            .integer(),
                    );
                    if self.tpl_seats[si].altitude <= 0.0 {
                        self.tpl_seats[si].altitude = 0.0;
                    }
                    self.tpl_seats[si].start_type = PlaneStart::stored_for_altitude(
                        self.tpl_seats[si].start_type,
                        self.tpl_seats[si].altitude,
                    );
                    ui.end_row();
                }
            }

            field_label(ui, "Number in formation");
            ui.add(egui::DragValue::new(&mut self.tpl_seats[si].number_in_formation).range(0..=8));
            ui.end_row();

            field_label(ui, "Fuel");
            ui.add(egui::DragValue::new(&mut self.tpl_seats[si].fuel).range(0.0..=1.0).speed(0.05));
            ui.end_row();

            payload_note = self.draw_seat_payload_mods(ui, si, ctrl_w);

            field_label(ui, "Flags");
            ui.vertical(|ui| {
                ui.checkbox(&mut self.tpl_seats[si].vulnerable, "Vulnerable");
                ui.checkbox(&mut self.tpl_seats[si].engageable, "Engageable");
                ui.checkbox(&mut self.tpl_seats[si].limit_ammo, "Limit ammo");
                if self.tpl_seats[si].unit.is_air() {
                    ui.checkbox(&mut self.tpl_seats[si].ai_rtb, "AI RTB");
                }
            });
            ui.end_row();
        });
        if moved {
            return;
        }
        if let Some(note) = payload_note {
            ui.add(egui::Label::new(RichText::new(note).small().weak()).wrap());
        }
        if is_follower(&self.tpl_seats, si) {
            ui.label(
                RichText::new("This unit follows its lead (entity target link). Orders are given to the lead only.")
                    .italics()
                    .small(),
            );
        }
        ui.label(RichText::new(self.tpl_seats[si].unit.script.as_str()).italics().small());
        if self.tpl_seats[si].unit.is_train() {
            self.draw_train_carriages(ui, si);
        }
    }

    fn draw_template_order_fields(&mut self, ui: &mut egui::Ui, seat: usize, order: usize) {
        let ctrl_w = field_control_w(ui);
        let unit_kind = if self.tpl_seats[seat].unit.is_air() { CatalogKind::Plane } else { CatalogKind::Vehicle };
        field_grid(ui, ("tpl_order_kind", seat, order), |ui| {
            field_label(ui, "Kind");
            let kind = self.tpl_seats[seat].orders[order].kind;
            egui::ComboBox::from_id_salt(format!("tpl_ord_kind_{seat}_{order}"))
                .selected_text(kind.label())
                .width(ctrl_w)
                .truncate()
                .show_ui(ui, |ui| {
                    for k in kinds_in_same_group(kind, unit_kind) {
                        if ui.selectable_label(self.tpl_seats[seat].orders[order].kind == k, k.label()).clicked() {
                            apply_order_kind(&mut self.tpl_seats, seat, order, k);
                        }
                    }
                });
            ui.end_row();
        });
        let order = {
            let s = &mut self.tpl_seats[seat];
            normalize_order_chain(&mut s.orders, &mut s.events, order)
        };
        self.tpl_select = Some(TplSelect::Order { seat, order });
        if order >= self.tpl_seats[seat].orders.len() {
            return;
        }

        // Explanations under the fields: (text, style).
        let mut notes: Vec<(String, NoteStyle)> = Vec::new();
        let kind = self.tpl_seats[seat].orders[order].kind;
        field_grid(ui, ("tpl_order_fields", seat, order), |ui| match kind {
            OrderKind::Attack => {
                notes.push((
                    "MCU_CMD_AttackTarget: Objects = this unit (or its lead), Targets = the unit to attack.".into(),
                    NoteStyle::Plain,
                ));
                let others = order_seat_indexes(&self.tpl_seats)
                    .into_iter()
                    .filter(|&i| i != seat)
                    .collect::<Vec<_>>();
                field_label(ui, "Target");
                if others.is_empty() {
                    ui.label(RichText::new("Add another unit to attack.").italics());
                } else {
                    let current = self.tpl_seats[seat].orders[order]
                        .attack_seat
                        .and_then(|i| self.tpl_seats.get(i).map(|s| format!("Seat {} ({})", i + 1, s.unit.label())))
                        .unwrap_or_else(|| "Choose unit…".into());
                    egui::ComboBox::from_id_salt(format!("tpl_atk_{seat}_{order}"))
                        .selected_text(current)
                        .width(ctrl_w)
                        .truncate()
                        .show_ui(ui, |ui| {
                            for i in &others {
                                let text = format!("Seat {} ({})", i + 1, self.tpl_seats[*i].unit.label());
                                ui.selectable_value(&mut self.tpl_seats[seat].orders[order].attack_seat, Some(*i), text);
                            }
                        });
                }
                ui.end_row();
                field_label(ui, "Group");
                ui.checkbox(&mut self.tpl_seats[seat].orders[order].attack_group, "Attack group");
                ui.end_row();
                draw_priority_combo(ui, format!("tpl_atk_pri_{seat}_{order}"), &mut self.tpl_seats[seat].orders[order].priority);
                ui.end_row();
            }
            OrderKind::GotoWaypoint => {
                notes.push((
                    "Pulses waypoint MCU WP n (there is no Goto command). On arrival that WP pulses the next order's timer — AttackArea, Time on Target, or the next Goto WP — not the MCU itself. The next waypoint is reached through that timer, not a WP n → WP n+1 MCU link.".into(),
                    NoteStyle::Plain,
                ));
                notes.push((
                    "Bombers: put Time on Target after the attack. The IP waypoint pulses the attack timer and the TOT timer; when TOT expires the chain continues. Use Mission Complete at the end to pulse MISSION END (Force Complete, RTB, Land cleanup).".into(),
                    NoteStyle::Italic,
                ));
                field_label(ui, "Waypoint");
                ui.horizontal(|ui| {
                    let max_wp = used_waypoint_count(&self.tpl_seats).max(1);
                    let wp = self.tpl_seats[seat].orders[order].waypoint.clamp(1, max_wp);
                    self.tpl_seats[seat].orders[order].waypoint = wp;
                    egui::ComboBox::from_id_salt(format!("tpl_goto_{seat}_{order}"))
                        .selected_text(format!("WP {wp}"))
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for n in 1..=max_wp {
                                ui.selectable_value(&mut self.tpl_seats[seat].orders[order].waypoint, n, format!("WP {n}"));
                            }
                        });
                    if ui.button("New").on_hover_text("Add another waypoint after this hop").clicked() {
                        let idx = insert_goto_waypoint_after(&mut self.tpl_seats, seat, order);
                        if let Some(spec) = self.tpl_seats[seat].orders.get_mut(idx) {
                            spec.priority = self.tpl_wp_priority;
                        }
                        self.tpl_select = Some(TplSelect::Order { seat, order: idx });
                    }
                });
                ui.end_row();
                draw_priority_combo(ui, format!("tpl_goto_pri_{seat}_{order}"), &mut self.tpl_seats[seat].orders[order].priority);
                ui.end_row();
                if self.tpl_seats[seat].unit.is_air() {
                    field_label(ui, "Altitude");
                    let hop_ceiling = model_spec::ceiling_m(&self.tpl_seats[seat].unit.script);
                    let inherited = path_waypoint_display_m(&self.tpl_seats, self.tpl_wp_altitude);
                    let mut shown = if self.tpl_seats[seat].orders[order].altitude > 0.0 {
                        self.tpl_seats[seat].orders[order].altitude
                    } else {
                        inherited
                    };
                    if ui
                        .add(egui::DragValue::new(&mut shown).range(0.0..=hop_ceiling).suffix(" m"))
                        .on_hover_text("Follows the Waypoints altitude until you enter a value.")
                        .changed()
                    {
                        self.tpl_seats[seat].orders[order].altitude =
                            if (shown - inherited).abs() < 0.5 { 0.0 } else { shown };
                    }
                    ui.end_row();
                }
                notes.push(("Click a WP diamond on the diagram to pick it.".into(), NoteStyle::Italic));
            }
            OrderKind::TimeOnTarget => {
                notes.push((
                    "Timer pulsed from the waypoint before Attack / AttackArea (not the previous delay). When it expires, the next order in the chain fires. Use this so the flight is updated after hanging on the target.".into(),
                    NoteStyle::Plain,
                ));
                field_label(ui, "Time on target");
                ui.add(egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].time_s).range(0.0..=3600.0).suffix(" s"));
                ui.end_row();
            }
            OrderKind::MissionComplete => {
                notes.push((
                    "Pulses MISSION END: Force Complete, then deactivate / delete. The previous order (or Time on Target) starts this timer only when no event Then's it. When events Then this chip, cleanup waits for one of those events. Put Land or Force Complete before this if the flight should receive those commands first.".into(),
                    NoteStyle::Plain,
                ));
            }
            OrderKind::Timer => {
                notes.push((
                    "MCU_Timer pause in the order chain. The previous order pulses this timer; when it expires the next order runs.".into(),
                    NoteStyle::Plain,
                ));
                field_label(ui, "Pause");
                ui.add(egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].time_s).range(0.0..=3600.0).suffix(" s"));
                ui.end_row();
            }
            OrderKind::ForceComplete => {
                notes.push((
                    "MCU_CMD_ForceComplete on this unit (or shared Objects). Mission Complete pulses the shared MISSION END hub instead.".into(),
                    NoteStyle::Plain,
                ));
                draw_priority_combo(ui, format!("tpl_fc_pri_{seat}_{order}"), &mut self.tpl_seats[seat].orders[order].priority);
                ui.end_row();
            }
            OrderKind::RtbOnZoneOut => {
                notes.push((
                    "On Zone Out this unit (or its lead) flies to an RTB waypoint. Deactivate waits 1 minute. Each placement group and coalition gets its own RTB. No RTB waypoint is written unless at least one unit has this order.".into(),
                    NoteStyle::Plain,
                ));
            }
            OrderKind::AttackArea => {
                let ground = self.tpl_seats[seat].orders[order].attack_ground
                    || self.tpl_seats[seat].orders[order].attack_g_targets;
                field_label(ui, "Attack");
                let current = self.tpl_seats[seat].orders[order].attack_area_target();
                egui::ComboBox::from_id_salt(format!("tpl_aa_tgt_{seat}_{order}"))
                    .selected_text(current.label())
                    .width(ctrl_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for t in AttackAreaTarget::ALL {
                            if ui.selectable_label(current == t, t.label()).clicked() {
                                self.tpl_seats[seat].orders[order].set_attack_area_target(t);
                            }
                        }
                    });
                ui.end_row();
                draw_priority_combo(ui, format!("tpl_aa_pri_{seat}_{order}"), &mut self.tpl_seats[seat].orders[order].priority);
                ui.end_row();
                field_label(ui, "Area");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].attack_area).range(100.0..=20_000.0).suffix(" m"));
                    if ui
                        .button("Match range")
                        .on_hover_text("Set the area to this unit’s (and Also-apply) system range, capped at 3 km.")
                        .clicked()
                    {
                        apply_suggested_attack_area(&mut self.tpl_seats, seat, order);
                    }
                });
                ui.end_row();
                field_label(ui, "Time");
                ui.add(egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].time_s).range(0.0..=3600.0).suffix(" s"));
                ui.end_row();
                let area = self.tpl_seats[seat].orders[order].attack_area;
                if let Some(limit) = attack_area_range_limit(&self.tpl_seats, seat, order) {
                    if f64::from(area) > limit + 0.5 {
                        notes.push((
                            format!(
                                "Area {:.0} m is larger than this unit’s {:.1} km range. The far edge of the bubble is out of reach.",
                                area,
                                limit / 1000.0
                            ),
                            NoteStyle::Warn,
                        ));
                    } else {
                        notes.push((
                            format!(
                                "This system reaches {:.1} km. On the map the group parks within that of a hashed objective, and this MCU (ground / ground targets) is moved onto that objective.",
                                limit / 1000.0
                            ),
                            NoteStyle::Italic,
                        ));
                    }
                } else if ground {
                    notes.push((
                        "Ground / ground-target AttackArea sits on the group origin here. Generate on the Map moves it onto the hashed objective, or across the front along the unit's heading when none is marked.".into(),
                        NoteStyle::Italic,
                    ));
                }
                notes.push((
                    "MCU Time is how long AttackArea runs. Time on Target in the chain is a separate timer from the waypoint, for leaving the target.".into(),
                    NoteStyle::Italic,
                ));
            }
            OrderKind::Formation => {
                field_label(ui, "Formation");
                let current = formation_label(self.tpl_seats[seat].orders[order].formation_type, unit_kind);
                egui::ComboBox::from_id_salt(format!("tpl_form_{seat}_{order}"))
                    .selected_text(current)
                    .width(ctrl_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for preset in formations_for(unit_kind) {
                            ui.selectable_value(&mut self.tpl_seats[seat].orders[order].formation_type, preset.id, preset.label);
                        }
                    });
                ui.end_row();
            }
            OrderKind::Behaviour => {
                field_label(ui, "Filter");
                ui.add(egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].behaviour_filter).range(0..=32));
                ui.end_row();
            }
            OrderKind::Flare => {
                field_label(ui, "Color");
                ui.add(egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].flare_color).range(0..=4));
                ui.end_row();
            }
            OrderKind::Effect => {
                field_label(ui, "Effect");
                ui.checkbox(&mut self.tpl_seats[seat].orders[order].effect_start, "Start (off = stop)");
                ui.end_row();
            }
            OrderKind::Cover => {
                notes.push((
                    "Cover another unit. If this seat is a flight lead with wingmen, CoverGroup is set and the order goes to the lead (lead-to-lead). Independent units cover the chosen unit directly.".into(),
                    NoteStyle::Plain,
                ));
                let others = order_seat_indexes(&self.tpl_seats)
                    .into_iter()
                    .filter(|&i| i != seat)
                    .collect::<Vec<_>>();
                field_label(ui, "Cover");
                if others.is_empty() {
                    ui.label(RichText::new("Add another unit to cover.").italics());
                } else {
                    let current = self.tpl_seats[seat].orders[order]
                        .cover_lead
                        .and_then(|i| self.tpl_seats.get(i).map(|s| format!("Seat {} ({})", i + 1, s.unit.label())))
                        .unwrap_or_else(|| "Choose unit…".into());
                    egui::ComboBox::from_id_salt(format!("tpl_cover_{seat}_{order}"))
                        .selected_text(current)
                        .width(ctrl_w)
                        .truncate()
                        .show_ui(ui, |ui| {
                            for i in &others {
                                let text = format!("Seat {} ({})", i + 1, self.tpl_seats[*i].unit.label());
                                ui.selectable_value(&mut self.tpl_seats[seat].orders[order].cover_lead, Some(*i), text);
                            }
                        });
                }
                ui.end_row();
                draw_priority_combo(ui, format!("tpl_cover_pri_{seat}_{order}"), &mut self.tpl_seats[seat].orders[order].priority);
                ui.end_row();
            }
            OrderKind::Land => {
                draw_priority_combo(ui, format!("tpl_land_pri_{seat}_{order}"), &mut self.tpl_seats[seat].orders[order].priority);
                ui.end_row();
            }
            OrderKind::TakeOff => {
                notes.push((
                    "MCU_CMD_TakeOff. Put OnTookOff after this so the next order waits until airborne.".into(),
                    NoteStyle::Plain,
                ));
            }
            OrderKind::OnSpawned
            | OrderKind::OnTargetAttacked
            | OrderKind::OnAreaAttacked
            | OrderKind::OnTookOff
            | OrderKind::OnLanded => {
                let hint = match kind {
                    OrderKind::OnSpawned => {
                        "OnSpawned: CmdId = spawner, TarId = this timer, then the next order. Use Spawn Units."
                    }
                    OrderKind::OnTargetAttacked => {
                        "OnTargetAttacked: waits until Attack finishes, then this timer, then the next order."
                    }
                    OrderKind::OnAreaAttacked => {
                        "OnAreaAttacked: waits until AttackArea Time runs out, then this timer, then the next order."
                    }
                    OrderKind::OnTookOff => "OnTookOff: waits until Take Off finishes, then this timer, then the next order.",
                    OrderKind::OnLanded => "OnLanded: waits until Land finishes, then this timer, then the next order.",
                    _ => "",
                };
                notes.push((hint.into(), NoteStyle::Plain));
                if kind == OrderKind::OnSpawned && self.tpl_bring_up != BringUp::Spawn {
                    notes.push((
                        "Spawn Units is off: this timer still starts the chain from AFTER BRING UP.".into(),
                        NoteStyle::Italic,
                    ));
                }
                field_label(ui, "Timer");
                ui.add(
                    egui::DragValue::new(&mut self.tpl_seats[seat].orders[order].delay_s)
                        .range(0.0..=60.0)
                        .suffix(" s")
                        .speed(0.1),
                );
                ui.end_row();
                let then_label = self.tpl_seats[seat]
                    .orders
                    .get(order + 1)
                    .filter(|o| !o.kind.is_report())
                    .map(|o| o.kind.label())
                    .unwrap_or("Choose next order…");
                field_label(ui, "Then");
                egui::ComboBox::from_id_salt(format!("tpl_rep_then_{seat}_{order}"))
                    .selected_text(then_label)
                    .width(ctrl_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for k in OrderKind::following(unit_kind) {
                            let selected = self.tpl_seats[seat].orders.get(order + 1).is_some_and(|o| o.kind == k);
                            if ui.selectable_label(selected, k.label()).clicked() {
                                set_report_following(&mut self.tpl_seats[seat].orders, order, k, unit_kind);
                            }
                        }
                    });
                ui.end_row();
            }
        });
        show_field_notes(ui, &notes);
        // Fields above may have inserted an order and moved the selection.
        if order >= self.tpl_seats[seat].orders.len() {
            return;
        }
        if self.tpl_seats[seat].orders[order].kind.has_command_mcu() {
            ui.add_space(4.0);
            ui.label(RichText::new("Also apply to").strong());
            ui.label(
                RichText::new(
                    "Checked units share this command (one MCU, multiple Objects). Leave unchecked for a private order.",
                )
                .italics()
                .small(),
            );
            let others: Vec<(usize, String)> = (0..self.tpl_seats.len())
                .filter(|&i| i != seat)
                .map(|i| (i, format!("Seat {} ({})", i + 1, self.tpl_seats[i].unit.label())))
                .collect();
            if others.is_empty() {
                ui.label(RichText::new("Add another unit to share this order.").italics());
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (i, label) in &others {
                        let mut on = self.tpl_seats[seat].orders[order].shared_with.contains(i);
                        if ui.checkbox(&mut on, label).changed() {
                            let list = &mut self.tpl_seats[seat].orders[order].shared_with;
                            if on {
                                if !list.contains(i) {
                                    list.push(*i);
                                }
                            } else {
                                list.retain(|s| s != i);
                            }
                        }
                    }
                });
            }
        }
        ui.add_space(6.0);
        if ui.button("Remove order").on_hover_text("Ctrl Z undoes it").clicked() {
            self.remove_tpl_order(seat, order);
        }
    }

    fn draw_template_event_fields(&mut self, ui: &mut egui::Ui, seat: usize, event: usize) {
        let ctrl_w = field_control_w(ui);
        let unit_kind = self.tpl_seats[seat].unit.kind;
        let chain_si = if receives_orders(&self.tpl_seats, seat) { seat } else { flight_lead_of(&self.tpl_seats, seat) };
        let then_orders = self.tpl_seats[chain_si].orders.clone();
        field_grid(ui, ("tpl_event_fields", seat, event), |ui| {
            field_label(ui, "Event");
            let kind = self.tpl_seats[seat].events[event].kind;
            egui::ComboBox::from_id_salt(format!("tpl_evt_kind_{seat}_{event}"))
                .selected_text(kind.label())
                .width(ctrl_w)
                .truncate()
                .show_ui(ui, |ui| {
                    for k in EntityEvent::available(unit_kind) {
                        ui.selectable_value(&mut self.tpl_seats[seat].events[event].kind, *k, k.label());
                    }
                });
            ui.end_row();

            field_label(ui, "Then");
            let current = self.tpl_seats[seat].events[event].then.label(&then_orders);
            egui::ComboBox::from_id_salt(format!("tpl_evt_then_{seat}_{event}"))
                .selected_text(current)
                .width(ctrl_w)
                .truncate()
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.tpl_seats[seat].events[event].then, EventThen::ForceComplete, "Force Complete");
                    for (i, o) in then_orders.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.tpl_seats[seat].events[event].then,
                            EventThen::Order(i),
                            format!("{} {}", i + 1, o.kind.label()),
                        );
                    }
                });
            ui.end_row();
        });
        show_field_notes(
            ui,
            &[("OnEvent (TarId only). Links this unit to Force Complete or an order timer.".into(), NoteStyle::Plain)],
        );
        ui.add_space(6.0);
        if ui.button("Remove event").on_hover_text("Ctrl Z undoes it").clicked() {
            self.remove_tpl_event(seat, event);
        }
    }

    /// Removes one order (undoable) and selects its unit, like the tree's ×.
    fn remove_tpl_order(&mut self, si: usize, oi: usize) {
        if si < self.tpl_seats.len() && oi < self.tpl_seats[si].orders.len() {
            self.record_tpl_undo(format!("Removed {}", self.tpl_seats[si].orders[oi].kind.label()));
            self.tpl_seats[si].orders.remove(oi);
            for hook in &mut self.tpl_seats[si].events {
                remap_event_then(&mut hook.then, oi);
            }
            self.tpl_select = Some(TplSelect::Seat(si));
        }
    }

    /// Removes one event (undoable) and selects its unit, like the tree's ×.
    fn remove_tpl_event(&mut self, si: usize, ei: usize) {
        if si < self.tpl_seats.len() && ei < self.tpl_seats[si].events.len() {
            self.record_tpl_undo(format!("Removed {}", self.tpl_seats[si].events[ei].kind.label()));
            self.tpl_seats[si].events.remove(ei);
            self.tpl_select = Some(TplSelect::Seat(si));
        }
    }

    /// Filter combo, then one 28 px row per model: click a row to preview it,
    /// click its "+ Add" (or double-click the row) to add a unit.
    fn draw_template_model_browser(&mut self, ui: &mut egui::Ui) {
        let full_w = ui.available_width();
        // Class and country filters, each shown when the kind has that data.
        // `displayed_catalog` applies both, so the list always matches them.
        let classes = self.classes_for_kind(self.tpl_kind);
        if self.tpl_class.is_some_and(|c| !classes.contains(&c)) {
            self.tpl_class = None;
        }
        let countries = self.countries_for_kind(self.tpl_kind);
        if self.tpl_country.is_some_and(|c| !countries.contains(&c)) {
            self.tpl_country = None;
        }
        // A filter with a single choice filters nothing: show it from two up.
        let (classes, countries) = (
            if classes.len() > 1 { classes } else { Vec::new() },
            if countries.len() > 1 { countries } else { Vec::new() },
        );
        let shown = usize::from(!classes.is_empty()) + usize::from(!countries.is_empty());
        let combo_w = if shown == 2 { (full_w - 8.0 - 6.0) / 2.0 } else { full_w - 8.0 };
        let mut filter_changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if !classes.is_empty() {
                let current = self.tpl_class.map_or("All types", |c| c.label());
                egui::ComboBox::from_id_salt("tpl_class_filter")
                    .selected_text(current)
                    .width(combo_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.tpl_class.is_none(), "All types").clicked() {
                            self.tpl_class = None;
                            filter_changed = true;
                        }
                        for class in &classes {
                            if ui.selectable_label(self.tpl_class == Some(*class), class.label()).clicked() {
                                self.tpl_class = Some(*class);
                                filter_changed = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Filter the models by type");
            }
            if !countries.is_empty() {
                let current = self.tpl_country.map_or_else(|| "All countries".to_string(), country_short);
                egui::ComboBox::from_id_salt("tpl_country_filter")
                    .selected_text(current)
                    .width(combo_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.tpl_country.is_none(), "All countries").clicked() {
                            self.tpl_country = None;
                            filter_changed = true;
                        }
                        for country in &countries {
                            if ui
                                .selectable_label(self.tpl_country == Some(*country), country_short(*country))
                                .clicked()
                            {
                                self.tpl_country = Some(*country);
                                filter_changed = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Filter the models by the prototype's country");
            }
        });
        if filter_changed {
            self.tpl_add_pick = 0;
            self.tpl_preview_from_catalog = true;
        }

        let models = self.displayed_catalog();
        if models.is_empty() {
            let resp = ui.label(RichText::new("No models of this kind in the catalog yet.").color(c::NEUTRAL_700));
            if std::mem::take(&mut self.tpl_show_models) {
                resp.scroll_to_me(Some(Align::Center));
            }
            return;
        }
        if self.tpl_add_pick >= models.len() {
            self.tpl_add_pick = 0;
        }

        ui.add_space(4.0);
        let mut pick = None;
        let mut add = None;
        let show_models = std::mem::take(&mut self.tpl_show_models);
        let list = egui::ScrollArea::vertical()
            .id_salt("tpl_model_list")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let w = ui.available_width();
                for (i, unit) in models.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
                    let sel = i == self.tpl_add_pick;
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, sel, format!("Model {}", unit.label())));
                    let add_rect = Rect::from_min_max(Pos2::new(rect.right() - 58.0, rect.top()), rect.max);
                    let over_add = resp.hover_pos().is_some_and(|p| add_rect.contains(p));
                    let p = ui.painter();
                    if sel {
                        p.rect_filled(rect, 0.0, c::ACCENT_100);
                    } else if resp.hovered() {
                        p.rect_filled(rect, 0.0, c::NEUTRAL_100);
                    }
                    p.with_clip_rect(rect.shrink2(Vec2::new(0.0, 1.0)).with_max_x(add_rect.left())).text(
                        rect.left_center() + Vec2::new(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        unit.label(),
                        FontId::proportional(13.0),
                        c::TEXT,
                    );
                    let (add_font, add_color) = if sel || over_add {
                        (FontId::new(13.0, theme::bold_family()), c::ACCENT_700)
                    } else {
                        (FontId::proportional(13.0), c::NEUTRAL_600)
                    };
                    p.text(rect.right_center() - Vec2::new(8.0, 0.0), Align2::RIGHT_CENTER, "+ Add", add_font, add_color);
                    if over_add {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if sel && show_models {
                        resp.scroll_to_me(Some(Align::Center));
                    }
                    let resp = resp.on_hover_text(format!("{}. Double-click or + Add to add a unit.", unit.label()));
                    if resp.double_clicked() || (resp.clicked() && over_add) {
                        add = Some(i);
                    } else if resp.clicked() {
                        pick = Some(i);
                    }
                }
            });
        if show_models {
            ui.scroll_to_rect(list.inner_rect, Some(Align::Min));
        }
        if let Some(i) = pick {
            self.tpl_add_pick = i;
            self.tpl_preview_from_catalog = true;
        }
        if let Some(i) = add {
            self.tpl_add_pick = i;
            self.template_add_model(models[i].clone());
        }

    }

    /// Payload and Modifications rows of the seat `field_grid`. Returns the
    /// payload description, which the caller shows under the grid.
    fn draw_seat_payload_mods(&mut self, ui: &mut egui::Ui, si: usize, ctrl_w: f32) -> Option<String> {
        let script = self.tpl_seats[si].unit.script.clone();
        let loadout = payloads::catalog().for_script(&script);
        let has_payloads = loadout.is_some_and(|a| a.has_payloads());
        let has_mods = loadout.is_some_and(|a| a.has_mods());
        let mut note = None;

        if has_payloads {
            field_label(ui, "Payload");
            let current = self.tpl_seats[si].payload_id;
            let selected_text = payloads::payload_preview(&script, current);
            egui::ComboBox::from_id_salt(format!("tpl_payload_{si}"))
                .selected_text(selected_text)
                .width(ctrl_w)
                .truncate()
                .show_ui(ui, |ui| {
                    let ac = payloads::catalog().for_script(&script).unwrap();
                    for p in &ac.payloads {
                        let summary = format!("{}  {}", p.id, p.summary());
                        let desc = p.description(payloads::catalog());
                        if ui
                            .selectable_label(current == p.id, summary)
                            .on_hover_text(desc)
                            .clicked()
                        {
                            self.tpl_seats[si].payload_id = p.id;
                        }
                    }
                });
            ui.end_row();
            if let Some(ac) = payloads::catalog().for_script(&script) {
                if let Some(p) = ac.payload(self.tpl_seats[si].payload_id) {
                    note = Some(p.description(payloads::catalog()));
                }
            }
        } else if self.tpl_seats[si].unit.is_air() && !has_mods {
            field_label(ui, "Payload");
            ui.add(egui::DragValue::new(&mut self.tpl_seats[si].payload_id).range(0..=99));
            ui.end_row();
        }

        if has_mods {
            let is_train = self.tpl_seats[si].unit.is_train();
            field_label(ui, "Modifications");
            let preview = payloads::mods_preview(&script, &self.tpl_seats[si].mod_mask);
            let label = if preview == "—" {
                "Select…".to_string()
            } else {
                preview
            };
            ui.menu_button(label, |ui| {
                ui.set_min_width(240.0);
                ui.label(
                    RichText::new(if is_train {
                        "Stock is none (ModMask 0). Hospital is on or off. Boxcars, wagons, and platforms each pick one style."
                    } else if payloads::empty_mod_mask(&script) == 0 {
                        "Stock is none (ModMask 0). Equipment, cargo, and trailer slots each pick one option."
                    } else {
                        "More than one mod may be selected when the aircraft has several slots."
                    })
                    .small()
                    .weak(),
                );
                let ac = payloads::catalog().for_script(&script).unwrap();
                let mut mask = payloads::parse_mod_mask_for(&script, &self.tpl_seats[si].mod_mask);
                let mut changed = false;
                let mut shown = 0usize;
                for slot in &ac.mod_slots {
                    if !payloads::slot_has_choices(slot) {
                        continue;
                    }
                    if shown > 0 {
                        ui.separator();
                    }
                    shown += 1;
                    let exclusive = payloads::slot_choice_count(slot) > 1;
                    if exclusive {
                        let title = payloads::mod_slot_title(&script, slot.number);
                        ui.label(RichText::new(title).small().weak());
                        let has_none = slot
                            .options
                            .iter()
                            .any(|o| payloads::extra_bits(&o.binary_id) == 0);
                        if !has_none {
                            let none_on = payloads::exclusive_selection(mask, slot).is_none();
                            if ui.radio(none_on, "None").clicked() {
                                mask = payloads::clear_exclusive_for(&script, mask, slot);
                                changed = true;
                            }
                        }
                        for opt in &slot.options {
                            if payloads::extra_bits(&opt.binary_id) == 0 {
                                continue;
                            }
                            let selected = payloads::exclusive_selection(mask, slot)
                                .is_some_and(|s| s.binary_id == opt.binary_id);
                            if ui.radio(selected, &opt.description).clicked() {
                                mask = payloads::select_exclusive_for(&script, mask, slot, opt);
                                changed = true;
                            }
                        }
                    } else if let Some(opt) = slot.options.iter().find(|o| {
                        payloads::extra_bits(&o.binary_id) != 0
                    }) {
                        let mut on = payloads::option_selected(mask, opt);
                        if ui.checkbox(&mut on, &opt.description).changed() {
                            mask = payloads::set_toggle_for(&script, mask, opt, on);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.tpl_seats[si].mod_mask = payloads::encode_mod_mask(mask);
                }
            });
            ui.end_row();
        }
        note
    }

    /// Model picture and specs. The caller draws the name (selection title).
    fn draw_model_preview(
        &mut self,
        ui: &mut egui::Ui,
        unit: Option<&CatalogUnit>,
        payload_line: Option<&str>,
        mods_line: Option<&str>,
        status_line: Option<&str>,
    ) {
        let Some(unit) = unit else {
            ui.label(RichText::new("Select a model.").color(c::NEUTRAL_700));
            return;
        };
        let ctx = ui.ctx().clone();
        let tex = self.model_texture(&ctx, &unit.script);
        let size = tex.size_vec2();
        let max_w = ui.available_width().max(1.0);
        let scale = (max_w / size.x).min(88.0 / size.y).min(1.0);
        ui.add(egui::Image::new((tex.id(), size * scale)));
        let class = model_spec::class_for(&unit.script);
        let cruise = model_spec::spec_for(&unit.script)
            .map(|s| s.cruise_line())
            .unwrap_or_else(|| model_spec::format_cruise(None));
        ui.add_space(4.0);
        ui.add(egui::Label::new(format!("Type: {}", class.label())).wrap());
        ui.add(egui::Label::new(format!("Cruise speed: {cruise}")).wrap());
        if let Some(status) = status_line {
            ui.add(egui::Label::new(RichText::new(status).strong()).wrap());
        }
        if let Some(spec) = model_spec::spec_for(&unit.script) {
            if spec.ceiling_m > 0.0 {
                let ft = spec.ceiling_m * 3.280_84;
                ui.add(
                    egui::Label::new(format!(
                        "Ceiling: {:.0} m / {:.0} ft",
                        spec.ceiling_m, ft
                    ))
                    .wrap(),
                );
            }
            if !spec.notes.is_empty() {
                ui.add(egui::Label::new(RichText::new(spec.notes).small().color(c::NEUTRAL_700)).wrap());
            }
        }
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new("Skins: —").small()).wrap());
        let payload = payload_line.unwrap_or("—");
        ui.add(
            egui::Label::new(RichText::new(format!("Payload: {payload}")).small()).wrap(),
        );
        let mods = mods_line.unwrap_or("—");
        ui.add(
            egui::Label::new(RichText::new(format!("Modifications: {mods}")).small()).wrap(),
        );
    }

    fn model_texture(&mut self, ctx: &egui::Context, script: &str) -> TextureHandle {
        let id = if model_spec::class_for(script) == ModelClass::Infantry {
            "infantry".to_string()
        } else {
            model_spec::script_id(script)
        };
        if let Some(tex) = self.tpl_model_tex.get(&id) {
            return tex.clone();
        }
        let img = load_model_png(model_spec::png_for_script(script));
        let tex = ctx.load_texture(
            format!("tpl_model_{id}"),
            img,
            egui::TextureOptions::LINEAR,
        );
        self.tpl_model_tex.insert(id, tex.clone());
        tex
    }

    fn classes_for_kind(&self, kind: CatalogKind) -> Vec<ModelClass> {
        model_spec::classes_in(
            self.tpl_catalog
                .iter()
                .filter(|u| u.kind == kind)
                .map(|u| u.script.as_str()),
        )
    }

    /// Countries on the catalog prototypes of `kind`, sorted.
    fn countries_for_kind(&self, kind: CatalogKind) -> Vec<i32> {
        let mut ids: Vec<i32> = self
            .tpl_catalog
            .iter()
            .filter(|u| u.kind == kind)
            .map(|u| u.country())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Models of the current kind that pass both the class and the country filter.
    fn displayed_catalog(&self) -> Vec<CatalogUnit> {
        let mut models: Vec<CatalogUnit> = self
            .tpl_catalog
            .iter()
            .filter(|u| u.kind == self.tpl_kind)
            .filter(|u| self.tpl_class.is_none_or(|c| model_spec::class_for(&u.script) == c))
            .filter(|u| self.tpl_country.is_none_or(|c| u.country() == c))
            .cloned()
            .collect();
        models.sort_by(|a, b| a.label().cmp(b.label()));
        models
    }

    fn load_unit_catalog(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(err) => {
                self.status = Status::Error(format!("Could not read catalog: {err}"));
                return;
            }
        };
        let root = match parse_group_file(&text).or_else(|_| parse_il2_document(&text)) {
            Ok(r) => r,
            Err(err) => {
                self.status = Status::Error(format!("Catalog parse failed: {err}"));
                return;
            }
        };
        let cat = load_catalog(&root);
        if cat.is_empty() {
            self.status = Status::Error(
                "Catalog has no Plane / Vehicle / Infantry / Train / Ship / Fixed prototypes. Use subgroups named Planes, Vehicles, Infantry, Trains, Ships, Fixed Units / Fixed Objects, or User Added.".into(),
            );
            return;
        }
        let n = cat.len();
        self.tpl_catalog = cat;
        self.tpl_path = Some(path);
        self.tpl_class = None;
        self.tpl_country = None;
        self.tpl_add_pick = 0;
        self.tpl_preview_from_catalog = true;
        self.status = Status::Info(format!("Loaded {n} prototype(s) from the catalog."));
    }

    fn add_user_catalog_group(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(err) => {
                self.status = Status::Error(format!("Could not read group: {err}"));
                return;
            }
        };
        let root = match parse_group_file(&text).or_else(|_| parse_il2_document(&text)) {
            Ok(r) => r,
            Err(err) => {
                self.status = Status::Error(format!("Group parse failed: {err}"));
                return;
            }
        };
        let extra = load_catalog_as_user_added(&root);
        if extra.is_empty() {
            self.status = Status::Error(
                "That group has no Plane / Vehicle / Infantry / Train / Ship / Fixed prototypes to add.".into(),
            );
            return;
        }
        let n = extra.len();
        merge_catalog(&mut self.tpl_catalog, extra);
        self.tpl_kind = CatalogKind::UserAdded;
        self.tpl_class = None;
        self.tpl_country = None;
        self.tpl_add_pick = 0;
        self.tpl_preview_from_catalog = true;
        self.tpl_path = Some(path);
        self.status = Status::Info(format!("Appended {n} prototype(s) to User Added."));
    }

    fn sync_template_waypoint_speed(&mut self) {
        self.tpl_wp_speed = model_spec::suggested_waypoint_speed_kmh(
            self.tpl_seats.iter().map(|s| s.unit.script.as_str()),
        );
    }

    fn template_zone_mix(&self) -> ZoneMix {
        zone_mix_for_seats(&self.tpl_seats)
            .or(self.tpl_zone_mix)
            .unwrap_or(ZoneMix::Air)
    }

    fn sync_template_zone_defaults(&mut self) {
        let Some(mix) = zone_mix_for_seats(&self.tpl_seats) else {
            return;
        };
        if self.tpl_zone_mix == Some(mix) {
            return;
        }
        let (zone_in, zone_out) = zone_defaults(mix);
        self.tpl_zone_in = zone_in;
        self.tpl_zone_out = zone_out;
        self.tpl_zone_mix = Some(mix);
    }

    fn generate_unit_template(&mut self) {
        if self.tpl_zone_out < self.tpl_zone_in + 200.0 {
            self.tpl_zone_out = self.tpl_zone_in + 200.0;
        }
        let opts = TemplateOptions {
            name: self
                .tpl_loaded_path
                .as_ref()
                .or(self.tpl_path.as_ref())
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .unwrap_or("Unit Template")
                .to_string(),
            zone_in: self.tpl_zone_in,
            zone_out: self.tpl_zone_out,
            spacing: PLACEMENT_SPACING,
            seats: self.tpl_seats.clone(),
            place_layout: self.tpl_place_layout,
            per_group: self.tpl_per_group,
            bring_up: self.tpl_bring_up,
            allow_multiple_spawns: self.tpl_spawn_reset,
            spawn_cooldown_min: self.tpl_spawn_cooldown_min,
            waypoint_count: used_waypoint_count(&self.tpl_seats),
            waypoint_spacing: WAYPOINT_SPACING_M,
            waypoint_speed: self.tpl_wp_speed,
            waypoint_altitude: self.tpl_wp_altitude,
            waypoint_priority: self.tpl_wp_priority,
            zone_coalition: self.tpl_zone_coalition,
        };
        let mut pack = match generate_template(&opts) {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let terrain_note = self.apply_terrain(&mut pack);
        let text = serialize_group(&pack);
        let default_name = self
            .tpl_loaded_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Unit_Template.Group");
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        let locale = self
            .tpl_loaded_path
            .as_ref()
            .or(self.tpl_path.as_ref())
            .map(|p| vec![p.clone()])
            .unwrap_or_default();
        self.status = save_with_sidecars(
            &save_path,
            &text,
            &locale,
            &format!(
                "Wrote {} units ({}) with Zone In / Zone Out and MISSION END cleanup",
                opts.seats.len(),
                opts.bring_up.label()
            ),
        );
        self.add_terrain_note(terrain_note);
    }

    fn reset_template_builder(&mut self) {
        self.tpl_seats = default_template_seats();
        self.tpl_select = None;
        self.tpl_preview_from_catalog = false;
        self.tpl_bring_up = BringUp::Activate;
        self.tpl_spawn_reset = false;
        self.tpl_spawn_cooldown_min = 5.0;
        self.tpl_place_layout = PlaceLayout::InvertedVee;
        self.tpl_per_group = 4;
        self.tpl_zone_in = AIR_ZONE_IN_M;
        self.tpl_zone_out = AIR_ZONE_OUT_M;
        self.tpl_zone_mix = Some(ZoneMix::Air);
        self.tpl_wp_spacing = WAYPOINT_SPACING_M;
        self.tpl_wp_speed = 100.0;
        self.tpl_wp_altitude = 0.0;
        self.tpl_wp_priority = 1;
        self.tpl_zone_coalition = ZoneCoalition::Western;
        self.tpl_view_zoom = 1.0;
        self.tpl_view_pan = Vec2::ZERO;
        self.tpl_loaded_path = None;
        self.status = Status::Info(
            "Template cleared — add units to start a new group.".into(),
        );
    }

    fn load_template_group(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(err) => {
                self.status = Status::Error(format!("Could not read group: {err}"));
                return;
            }
        };
        let root = match parse_group_file(&text).or_else(|_| parse_il2_document(&text)) {
            Ok(r) => r,
            Err(err) => {
                self.status = Status::Error(format!("Group parse failed: {err}"));
                return;
            }
        };
        let loaded = match load_template(&root, &self.tpl_catalog) {
            Ok(l) => l,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        for seat in &loaded.options.seats {
            if !self
                .tpl_catalog
                .iter()
                .any(|u| u.script.eq_ignore_ascii_case(&seat.unit.script))
            {
                merge_catalog(&mut self.tpl_catalog, vec![seat.unit.clone()]);
            }
        }
        let opts = loaded.options;
        self.tpl_seats = opts.seats;
        self.tpl_select = None;
        self.tpl_preview_from_catalog = false;
        self.tpl_bring_up = opts.bring_up;
        self.tpl_spawn_reset = opts.allow_multiple_spawns;
        self.tpl_spawn_cooldown_min = opts.spawn_cooldown_min;
        self.tpl_place_layout = opts.place_layout;
        self.tpl_per_group = opts.per_group;
        self.tpl_zone_in = opts.zone_in;
        self.tpl_zone_out = opts.zone_out;
        self.tpl_zone_mix = zone_mix_for_seats(&self.tpl_seats);
        self.tpl_wp_speed = opts.waypoint_speed;
        self.tpl_wp_altitude = opts.waypoint_altitude;
        self.tpl_wp_priority = opts.waypoint_priority;
        self.tpl_wp_spacing = WAYPOINT_SPACING_M;
        self.tpl_zone_coalition = opts.zone_coalition;
        self.tpl_view_zoom = 1.0;
        self.tpl_view_pan = Vec2::ZERO;
        self.tpl_loaded_path = Some(path.clone());
        clamp_tpl_select(&mut self.tpl_select, &self.tpl_seats);
        let n = self.tpl_seats.len();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("group");
        if loaded.warnings.is_empty() {
            self.status = Status::Info(format!("Loaded {n} unit(s) from {name} for editing."));
        } else {
            let lead = if loaded.native_format {
                format!("Loaded {n} unit(s) from {name} with corrections:")
            } else {
                format!("Rebuilt {name} from units and orders ({n} unit(s)):")
            };
            self.status = Status::Warn {
                lead,
                items: loaded.warnings,
            };
        }
    }

    /// Formation view: fills the center above the order tree (README §5.1).
    fn draw_template_schematic(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Formation view"));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, c::BG);
        let grid = Stroke::new(1.0_f32, c::NEUTRAL_200);
        let mut gx = rect.left() + 32.0;
        while gx < rect.right() {
            painter.line_segment([Pos2::new(gx, rect.top()), Pos2::new(gx, rect.bottom())], grid);
            gx += 32.0;
        }
        let mut gy = rect.top() + 32.0;
        while gy < rect.bottom() {
            painter.line_segment([Pos2::new(rect.left(), gy), Pos2::new(rect.right(), gy)], grid);
            gy += 32.0;
        }

        let per = self.tpl_per_group.max(1) as usize;
        let spacing = PLACEMENT_SPACING as f64;
        let n = self.tpl_seats.len();
        let mut world: Vec<(f64, f64)> = Vec::new();
        for i in 0..n {
            world.push(place_offset(self.tpl_place_layout, i, per, spacing));
        }
        let (min_x, max_x, min_z, max_z) = if world.is_empty() {
            (-150.0, 150.0, -150.0, 150.0)
        } else {
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_z = f64::MAX;
            let mut max_z = f64::MIN;
            for &(dx, dz) in &world {
                min_x = min_x.min(dx);
                max_x = max_x.max(dx);
                min_z = min_z.min(dz);
                max_z = max_z.max(dz);
            }
            let pad = spacing.max(80.0);
            (min_x - pad, max_x + pad, min_z - pad, max_z + pad)
        };
        let span = (max_x - min_x).max(max_z - min_z).max(120.0);
        let fit_scale = (rect.width().min(rect.height()) * 0.72) / span as f32;
        let mid_x = (min_x + max_x) * 0.5;
        let mid_z = (min_z + max_z) * 0.5;
        let c = rect.center();
        self.tpl_view_zoom = self.tpl_view_zoom.clamp(0.04, 12.0);

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.5 {
                let factor = if scroll > 0.0 { 1.15 } else { 1.0 / 1.15 };
                let old = self.tpl_view_zoom;
                let new = (old * factor).clamp(0.04, 12.0);
                if let Some(hover) = response.hover_pos() {
                    let old_scale = fit_scale * old;
                    let dx = mid_x + self.tpl_view_pan.x as f64
                        - (hover.y - c.y) as f64 / old_scale as f64;
                    let dz = mid_z + self.tpl_view_pan.y as f64
                        + (hover.x - c.x) as f64 / old_scale as f64;
                    let new_scale = fit_scale * new;
                    self.tpl_view_pan.x =
                        (dx - mid_x) as f32 - (c.y - hover.y) / new_scale;
                    self.tpl_view_pan.y =
                        (dz - mid_z) as f32 - (hover.x - c.x) / new_scale;
                }
                self.tpl_view_zoom = new;
            }
            if scroll.abs() > 0.0 {
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = Vec2::ZERO;
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.events.retain(|e| {
                        !matches!(e, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_))
                    });
                });
            }
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let d = response.drag_delta();
            let scale = fit_scale * self.tpl_view_zoom;
            if scale > 1e-6 {
                self.tpl_view_pan.x += d.y / scale;
                self.tpl_view_pan.y -= d.x / scale;
            }
        }

        let scale = fit_scale * self.tpl_view_zoom;
        let pan = self.tpl_view_pan;
        let to_screen = |dx: f64, dz: f64| {
            Pos2::new(
                c.x + ((dz - mid_z) as f32 - pan.y) * scale,
                c.y - ((dx - mid_x) as f32 - pan.x) * scale,
            )
        };

        let selected_seat = match self.tpl_select {
            Some(
                TplSelect::Seat(s)
                | TplSelect::Order { seat: s, .. }
                | TplSelect::Event { seat: s, .. },
            ) => Some(s),
            None => None,
        };

        let selected_wp = match self.tpl_select {
            Some(TplSelect::Order { seat, order })
                if seat < self.tpl_seats.len()
                    && order < self.tpl_seats[seat].orders.len()
                    && self.tpl_seats[seat].orders[order].kind == OrderKind::GotoWaypoint =>
            {
                Some(self.tpl_seats[seat].orders[order].waypoint)
            }
            _ => None,
        };
        let selected_area = match self.tpl_select {
            Some(TplSelect::Order { seat, order })
                if seat < self.tpl_seats.len()
                    && order < self.tpl_seats[seat].orders.len()
                    && self.tpl_seats[seat].orders[order].kind == OrderKind::AttackArea =>
            {
                Some(self.tpl_seats[seat].orders[order].attack_area)
            }
            _ => None,
        };

        let origin = to_screen(0.0, 0.0);
        let visual = visual_range_m(self.template_zone_mix());
        let vis_on_in = (self.tpl_zone_in - visual).abs() < 50.0;
        let vis_on_out = (self.tpl_zone_out - visual).abs() < 50.0;
        // Zone In solid ACCENT_700, Zone Out dashed ACCENT, labels above each circle.
        painter.circle_stroke(
            origin,
            (self.tpl_zone_in * scale).max(2.0),
            Stroke::new(1.5_f32, c::ACCENT_700),
        );
        shell::dashed_circle(
            &painter,
            origin,
            (self.tpl_zone_out * scale).max(2.0),
            Stroke::new(1.5_f32, c::ACCENT),
        );
        if !vis_on_in && !vis_on_out {
            painter.circle_stroke(
                origin,
                (visual * scale).max(2.0),
                Stroke::new(1.0_f32, c::WARN),
            );
            painter.text(
                to_screen(0.0, visual as f64),
                Align2::LEFT_CENTER,
                " visual",
                FontId::proportional(12.0),
                c::WARN_TEXT,
            );
        }
        painter.text(
            to_screen(self.tpl_zone_in as f64, 0.0) - Vec2::new(0.0, 4.0),
            Align2::CENTER_BOTTOM,
            format!(
                "Zone In {:.1} km{}",
                self.tpl_zone_in / 1000.0,
                if vis_on_in { " · visual" } else { "" }
            ),
            FontId::proportional(12.0),
            c::ACCENT_700,
        );
        painter.text(
            to_screen(self.tpl_zone_out as f64, 0.0) - Vec2::new(0.0, 4.0),
            Align2::CENTER_BOTTOM,
            format!(
                "Zone Out {:.1} km{}",
                self.tpl_zone_out / 1000.0,
                if vis_on_out { " · visual" } else { "" }
            ),
            FontId::proportional(12.0),
            c::ACCENT_700,
        );
        if let Some(area) = selected_area {
            painter.circle_stroke(
                origin,
                (area * scale).max(2.0),
                Stroke::new(1.4_f32, c::WARN),
            );
        }

        let wp_space = self.tpl_wp_spacing as f64;
        let mut wp_pts: Vec<(u32, Pos2)> = Vec::new();
        let wp_count = used_waypoint_count(&self.tpl_seats);
        if wp_count > 0 {
            let mut path = vec![origin];
            for w in 0..wp_count {
                let p = to_screen((w as f64 + 1.0) * wp_space, 0.0);
                path.push(p);
                wp_pts.push((w + 1, p));
            }
            for pair in path.windows(2) {
                painter.line_segment(
                    [pair[0], pair[1]],
                    Stroke::new(1.4_f32, c::ACCENT_500),
                );
            }
            for &(num, p) in &wp_pts {
                let sel = selected_wp == Some(num);
                let r = if sel { 8.0 } else { 6.5 };
                let color = if sel { c::ACCENT_800 } else { c::ACCENT_500 };
                let dia = vec![
                    Pos2::new(p.x, p.y - r),
                    Pos2::new(p.x + r, p.y),
                    Pos2::new(p.x, p.y + r),
                    Pos2::new(p.x - r, p.y),
                ];
                painter.add(egui::Shape::convex_polygon(dia, color, Stroke::NONE));
                painter.text(
                    p + Vec2::new(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("WP {num}"),
                    FontId::proportional(12.0),
                    c::NEUTRAL_800,
                );
            }
        }

        let groups = n.div_ceil(per).max(1);
        for g in 0..groups {
            let start = g * per;
            let end = (start + per).min(n);
            if start >= end {
                continue;
            }
            let mut g_min = Pos2::new(f32::MAX, f32::MAX);
            let mut g_max = Pos2::new(f32::MIN, f32::MIN);
            for i in start..end {
                let p = to_screen(world[i].0, world[i].1);
                g_min.x = g_min.x.min(p.x);
                g_min.y = g_min.y.min(p.y);
                g_max.x = g_max.x.max(p.x);
                g_max.y = g_max.y.max(p.y);
            }
            let box_rect = Rect::from_min_max(g_min, g_max).expand(22.0);
            painter.rect_stroke(
                box_rect,
                6.0,
                Stroke::new(1.0_f32, c::NEUTRAL_300),
                egui::StrokeKind::Outside,
            );
        }

        for (i, seat) in self.tpl_seats.iter().enumerate() {
            if let FlightRole::Follows(lead) = seat.role {
                if lead < world.len() {
                    painter.line_segment(
                        [
                            to_screen(world[lead].0, world[lead].1),
                            to_screen(world[i].0, world[i].1),
                        ],
                        Stroke::new(1.2_f32, c::NEUTRAL_500),
                    );
                }
            }
        }

        let mut points = Vec::new();
        for i in 0..n {
            points.push(to_screen(world[i].0, world[i].1));
        }

        let hover_wp = response.hover_pos().and_then(|pos| {
            let mut best: Option<(u32, f32)> = None;
            for &(num, p) in &wp_pts {
                let d = p.distance(pos);
                if d < 18.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((num, d));
                }
            }
            best.map(|(n, _)| n)
        });
        let hover = response.hover_pos().and_then(|pos| {
            let mut best: Option<(usize, f32)> = None;
            for (i, p) in points.iter().enumerate() {
                let d = p.distance(pos);
                if d < 22.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((i, d));
                }
            }
            best.map(|(i, _)| i)
        });
        if hover.is_some() || hover_wp.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        for (i, p) in points.iter().enumerate() {
            let selected = selected_seat == Some(i);
            let hovered = hover == Some(i);
            let lead_here = self.tpl_seats[i].role == FlightRole::Lead;
            let base = side_color(self.tpl_seats[i].country);
            let color = if selected {
                c::ACCENT
            } else if hovered {
                Color32::from_rgb(
                    base.r().saturating_add(70),
                    base.g().saturating_add(70),
                    base.b().saturating_add(70),
                )
            } else if lead_here {
                Color32::from_rgb(
                    base.r().saturating_add(35),
                    base.g().saturating_add(35),
                    base.b().saturating_add(35),
                )
            } else if is_follower(&self.tpl_seats, i) {
                Color32::from_rgb(
                    base.r().saturating_sub(30).max(20),
                    base.g().saturating_sub(30).max(20),
                    base.b().saturating_sub(30).max(20),
                )
            } else {
                base
            };
            let r = if selected || hovered { 11.0 } else { 9.0 };
            match self.tpl_seats[i].unit.kind {
                CatalogKind::Plane => {
                    let tri = vec![
                        Pos2::new(p.x, p.y - r),
                        Pos2::new(p.x - r * 0.75, p.y + r * 0.65),
                        Pos2::new(p.x + r * 0.75, p.y + r * 0.65),
                    ];
                    painter.add(egui::Shape::convex_polygon(tri, color, Stroke::NONE));
                }
                CatalogKind::Ship => {
                    let dia = vec![
                        Pos2::new(p.x, p.y - r),
                        Pos2::new(p.x + r, p.y),
                        Pos2::new(p.x, p.y + r),
                        Pos2::new(p.x - r, p.y),
                    ];
                    painter.add(egui::Shape::convex_polygon(dia, color, Stroke::NONE));
                }
                CatalogKind::Train => {
                    painter.rect_filled(
                        Rect::from_center_size(*p, Vec2::new(r * 2.2, r * 1.1)),
                        2.0,
                        color,
                    );
                }
                CatalogKind::Vehicle | CatalogKind::Infantry | CatalogKind::Fixed => {
                    painter.rect_filled(
                        Rect::from_center_size(*p, Vec2::splat(r * 1.6)),
                        2.0,
                        color,
                    );
                }
                CatalogKind::UserAdded => {
                    painter.circle_filled(*p, r * 0.85, color);
                    painter.rect_stroke(
                        Rect::from_center_size(*p, Vec2::splat(r * 1.8)),
                        1.0,
                        Stroke::new(1.2_f32, color),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            if selected {
                painter.circle_stroke(*p, r + 5.0, Stroke::new(1.5_f32, c::ACCENT));
            }
            let role = match self.tpl_seats[i].role {
                FlightRole::Lead => "Lead",
                FlightRole::Follows(_) if is_follower(&self.tpl_seats, i) => "Wing",
                _ => "Solo",
            };
            let label = format!("{} {role}", i + 1);
            painter.text(
                *p + Vec2::new(0.0, r + 4.0),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(12.0),
                c::NEUTRAL_800,
            );
        }

        if let Some(i) = hover {
            let seat = &self.tpl_seats[i];
            let alt = if seat.unit.is_air() {
                format!(
                    " · {}",
                    PlaneStart::from_i32(seat.start_type).preview_status(seat.altitude)
                )
            } else {
                String::new()
            };
            let text = format!(
                "Seat {} · {} · {}{} · form {}",
                i + 1,
                seat.unit.label(),
                country_short(seat.country),
                alt,
                seat.number_in_formation
            );
            painter.text(rect.min + Vec2::new(14.0, 54.0), Align2::LEFT_TOP, text, FontId::proportional(13.0), c::TEXT);
        } else if let Some(num) = hover_wp {
            painter.text(
                rect.min + Vec2::new(14.0, 54.0),
                Align2::LEFT_TOP,
                format!(
                    "WP {num} · {:.0} m north of origin · {:.0} m alt · {} · Area {} m",
                    (num as f32) * self.tpl_wp_spacing,
                    waypoint_display_altitude(&self.tpl_seats, num, self.tpl_wp_altitude),
                    priority_label(waypoint_display_priority(
                        &self.tpl_seats,
                        num,
                        self.tpl_wp_priority,
                    )),
                    waypoint_area_m(&self.tpl_seats)
                ),
                FontId::proportional(13.0),
                c::TEXT,
            );
        }

        painter.text(
            rect.min + Vec2::new(14.0, 12.0),
            Align2::LEFT_TOP,
            "FORMATION VIEW",
            FontId::new(14.0, theme::heading_family()),
            c::TEXT,
        );
        painter.text(
            rect.min + Vec2::new(14.0, 32.0),
            Align2::LEFT_TOP,
            format!(
                "{} · {} / group · 150 m   Zone In {:.1} km{} · Out {:.1} km{}",
                self.tpl_place_layout.label(),
                self.tpl_per_group,
                self.tpl_zone_in / 1000.0,
                if near_visual_range(self.tpl_zone_in, visual_range_m(self.template_zone_mix())) {
                    " · visual"
                } else {
                    ""
                },
                self.tpl_zone_out / 1000.0,
                if near_visual_range(self.tpl_zone_out, visual_range_m(self.template_zone_mix())) {
                    " · visual"
                } else {
                    ""
                },
            ),
            FontId::proportional(12.0),
            c::NEUTRAL_700,
        );
        painter.text(
            Pos2::new(rect.center().x, rect.min.y + 12.0),
            Align2::CENTER_TOP,
            "N",
            FontId::new(13.0, theme::bold_family()),
            c::NEUTRAL_700,
        );
        if n == 0 {
            self.template_empty_state(ui, rect);
        }
        painter.text(
            rect.left_bottom() + Vec2::new(14.0, -14.0),
            Align2::LEFT_BOTTOM,
            "Scroll to zoom · right-drag to pan",
            FontId::proportional(12.0),
            c::NEUTRAL_700,
        );
        // Zoom group, bottom-right. Added after the canvas so it sits on top.
        let zoom_rect = Rect::from_min_max(
            rect.right_bottom() - Vec2::new(230.0, 44.0),
            rect.right_bottom() - Vec2::new(12.0, 10.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(zoom_rect)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui.button("Fit").on_hover_text("Fit the formation and reset the pan").clicked() {
                    self.tpl_view_zoom = 1.0;
                    self.tpl_view_pan = Vec2::ZERO;
                }
                if ui.button("+").on_hover_text("Zoom in").clicked() {
                    self.tpl_view_zoom = (self.tpl_view_zoom * 1.25).min(12.0);
                }
                ui.label(RichText::new(format!("{:.0}%", self.tpl_view_zoom * 100.0)).monospace());
                if ui.button("−").on_hover_text("Zoom out").clicked() {
                    self.tpl_view_zoom = (self.tpl_view_zoom / 1.25).max(0.04);
                }
            },
        );

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let mut best: Option<(usize, f32)> = None;
                for (i, p) in points.iter().enumerate() {
                    let d = p.distance(pos);
                    if d < 22.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                        best = Some((i, d));
                    }
                }
                if let Some((i, _)) = best {
                    self.tpl_select = Some(TplSelect::Seat(i));
                    self.tpl_preview_from_catalog = false;
                } else {
                    let mut hit_wp = None;
                    for &(num, p) in &wp_pts {
                        if p.distance(pos) < 18.0 {
                            hit_wp = Some(num);
                            break;
                        }
                    }
                    if let Some(num) = hit_wp {
                        if let Some(TplSelect::Order { seat, order }) = self.tpl_select {
                            if seat < self.tpl_seats.len()
                                && order < self.tpl_seats[seat].orders.len()
                                && self.tpl_seats[seat].orders[order].kind
                                    == OrderKind::GotoWaypoint
                            {
                                self.tpl_seats[seat].orders[order].waypoint = num;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Empty formation view: names the first step and offers its button
    /// (README §4). Adds the model picked in the list, or, when the kind has
    /// no models, scrolls the left panel to the list.
    fn template_empty_state(&mut self, ui: &mut egui::Ui, canvas: Rect) {
        let pick = self.displayed_catalog().get(self.tpl_add_pick).cloned();
        let area = Rect::from_center_size(canvas.center() + Vec2::new(0.0, 60.0), Vec2::new(canvas.width().min(420.0), 84.0));
        ui.painter().rect_filled(area, 2.0, c::BG);
        ui.painter().rect_stroke(area, 2.0, Stroke::new(1.0_f32, c::DIVIDER), egui::StrokeKind::Inside);
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(area.shrink(8.0)).layout(Layout::top_down(Align::Center)),
            |ui| {
                ui.label(RichText::new("Pick a model on the left, then + Add to begin.").color(c::NEUTRAL_800));
                ui.add_space(6.0);
                match pick {
                    Some(unit) => {
                        if ui
                            .button(format!("+ Add {}", unit.label()))
                            .on_hover_text("Add the model selected in the list on the left")
                            .clicked()
                        {
                            self.template_add_model(unit);
                        }
                    }
                    None => {
                        if ui.button("Show model list").on_hover_text("Scroll the left panel to the models").clicked() {
                            self.tpl_show_models = true;
                            ui.ctx().request_repaint();
                        }
                    }
                }
            },
        );
    }

    // ── Undo, confirmations, unsaved edits (README §6.3) ──────────────────

    fn tpl_snapshot(&self) -> TemplateSnapshot {
        TemplateSnapshot {
            seats: self.tpl_seats.clone(),
            select: self.tpl_select,
            bring_up: self.tpl_bring_up,
            spawn_reset: self.tpl_spawn_reset,
            spawn_cooldown_min: self.tpl_spawn_cooldown_min,
            place_layout: self.tpl_place_layout,
            per_group: self.tpl_per_group,
            zone_in: self.tpl_zone_in,
            zone_out: self.tpl_zone_out,
            zone_mix: self.tpl_zone_mix,
            wp_spacing: self.tpl_wp_spacing,
            wp_speed: self.tpl_wp_speed,
            wp_altitude: self.tpl_wp_altitude,
            wp_priority: self.tpl_wp_priority,
            zone_coalition: self.tpl_zone_coalition,
            loaded_path: self.tpl_loaded_path.clone(),
        }
    }

    fn tpl_restore(&mut self, s: TemplateSnapshot) {
        self.tpl_seats = s.seats;
        self.tpl_select = s.select;
        self.tpl_bring_up = s.bring_up;
        self.tpl_spawn_reset = s.spawn_reset;
        self.tpl_spawn_cooldown_min = s.spawn_cooldown_min;
        self.tpl_place_layout = s.place_layout;
        self.tpl_per_group = s.per_group;
        self.tpl_zone_in = s.zone_in;
        self.tpl_zone_out = s.zone_out;
        self.tpl_zone_mix = s.zone_mix;
        self.tpl_wp_spacing = s.wp_spacing;
        self.tpl_wp_speed = s.wp_speed;
        self.tpl_wp_altitude = s.wp_altitude;
        self.tpl_wp_priority = s.wp_priority;
        self.tpl_zone_coalition = s.zone_coalition;
        self.tpl_loaded_path = s.loaded_path;
        clamp_tpl_select(&mut self.tpl_select, &self.tpl_seats);
    }

    fn map_forces(&self) -> MapForces {
        MapForces {
            ships: self.map_ships.clone(),
            ground_east: self.map_ground_east.clone(),
            ground_nato: self.map_ground_nato.clone(),
            armies: self.map_armies.clone(),
            fighters: self.map_fighters.clone(),
            imported_fighters: self.map_imported_fighters.clone(),
            east_objectives: self.east_objectives.clone(),
            nato_objectives: self.nato_objectives.clone(),
            refs: self.map_refs.clone(),
            lines: None,
        }
    }

    fn restore_map_forces(&mut self, f: MapForces) {
        if let Some(lines) = f.lines {
            self.custom_front_xz = lines.custom_front;
            self.salients = lines.salients;
            self.attack_arrows = lines.attack_arrows;
            self.drawn_marks = lines.drawn_marks;
            self.current_salient.clear();
            self.attack_drag = None;
            self.front_stroke = None;
            self.clear_redo_stack();
        }
        self.map_ships = f.ships;
        self.map_ground_east = f.ground_east;
        self.map_ground_nato = f.ground_nato;
        self.map_armies = f.armies;
        self.map_fighters = f.fighters;
        self.map_imported_fighters = f.imported_fighters;
        self.east_objectives = f.east_objectives;
        self.nato_objectives = f.nato_objectives;
        self.map_refs = f.refs;
        self.ship_drag = None;
        self.ship_heading_drag = None;
        self.ground_drag = None;
        self.ground_heading_drag = None;
        self.wp_drag = None;
        self.wp_selected = None;
        self.fighter_drag = None;
        self.objective_drag = None;
        self.reaim_map_ground();
    }

    /// Call before a Map Clear / Remove so Ctrl Z can bring the forces back.
    fn record_map_undo(&mut self, label: String) {
        self.map_undo.record(label, self.map_forces());
        self.map_undo_marks = self.drawn_marks.len();
        self.map_last = MapAction::Clear;
    }

    /// Clear lines / salients / arrows (Period tab), undoable with Ctrl Z.
    fn clear_map_lines(&mut self, front: bool, salients: bool, arrows: bool) {
        let n_front = usize::from(front && !self.custom_front_xz.is_empty());
        let n_sal = if salients { self.salients.len() } else { 0 };
        let n_arr = if arrows { self.attack_arrows.len() } else { 0 };
        let mut parts = Vec::new();
        if n_front > 0 {
            parts.push("the drawn front".to_string());
        }
        if n_sal > 0 {
            parts.push(count_noun(n_sal, "salient", "salients"));
        }
        if n_arr > 0 {
            parts.push(count_noun(n_arr, "arrow", "arrows"));
        }
        let what = match parts.len() {
            0 => "a drawing in progress".to_string(),
            1 => parts.remove(0),
            _ => {
                let last = parts.pop().unwrap_or_default();
                format!("{} and {last}", parts.join(", "))
            }
        };
        let mut snapshot = self.map_forces();
        snapshot.lines = Some(MapLines {
            custom_front: self.custom_front_xz.clone(),
            salients: self.salients.clone(),
            attack_arrows: self.attack_arrows.clone(),
            drawn_marks: self.drawn_marks.clone(),
        });
        self.map_undo.record(format!("Cleared {what}"), snapshot);
        self.map_last = MapAction::Clear;
        if front {
            self.custom_front_xz.clear();
            self.front_stroke = None;
            self.drawn_marks.retain(|m| !m.is_front());
        }
        if salients {
            self.salients.clear();
            self.current_salient.clear();
            self.drawn_marks.retain(|m| *m != DrawnMark::Salient);
        }
        if arrows {
            self.attack_arrows.clear();
            self.attack_drag = None;
            self.drawn_marks.retain(|m| *m != DrawnMark::AttackArrow);
        }
        self.clear_redo_stack();
        self.map_undo_marks = self.drawn_marks.len();
    }

    /// What Ctrl Z undoes next on Map: the last action kind, falling back to
    /// the other kind when nothing of the last kind is left.
    fn map_undo_kind(&self) -> Option<MapAction> {
        let drawing = self.map_has_marks();
        let clear = self.map_undo.label().is_some();
        match self.map_last {
            MapAction::Drawing if drawing => Some(MapAction::Drawing),
            MapAction::Clear if clear => Some(MapAction::Clear),
            _ if drawing => Some(MapAction::Drawing),
            _ if clear => Some(MapAction::Clear),
            _ => None,
        }
    }

    /// Status-bar label for what Ctrl Z undoes next on Map.
    fn map_undo_label(&self) -> Option<&str> {
        match self.map_undo_kind()? {
            MapAction::Clear => self.map_undo.label(),
            MapAction::Drawing if self.map_stroke_in_progress() => Some("Unfinished drawing"),
            MapAction::Drawing => self.drawn_marks.last().map(|m| m.label()),
        }
    }

    /// Info and error messages describe the last action. Once the tab is
    /// edited after one appeared, it gives way to the tab's idle hint, so the
    /// status bar never reports something stale. Warnings stay until replaced.
    fn age_status(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.mode as u8, self.tab_edit_text()).hash(&mut h);
        let fp = h.finish();
        let text = self.status_text();
        if text != self.status_seen {
            self.status_seen = text;
            self.status_fp = fp;
        } else if fp != self.status_fp && matches!(self.status, Status::Info(_) | Status::Error(_)) {
            self.status = Status::Idle;
            self.status_seen = self.status_text();
            self.status_fp = fp;
        }
    }

    /// The status as one string (`age_status` and tests read it).
    fn status_text(&self) -> String {
        match &self.status {
            Status::Idle => String::new(),
            Status::Info(m) | Status::Error(m) => m.clone(),
            Status::Warn { lead, items } => format!("{lead}{}", items.join("|")),
        }
    }

    /// Everything a user can edit on the current tab, as text (selection excluded).
    fn tab_edit_text(&self) -> String {
        match self.mode {
            AppMode::Template => self.tpl_fingerprint(),
            AppMode::Map => self.map_fingerprint(),
            AppMode::Recon => {
                let slots = match self.recon_submode {
                    ReconSubmode::New => &self.recon_slots,
                    ReconSubmode::Rework => &self.recon_rework,
                };
                format!(
                    "{}{:?}{:?}",
                    self.recon_submode == ReconSubmode::New,
                    slots
                        .iter()
                        .map(|s| (&s.path, s.kind, &s.selected_triggers, s.influence, &s.restore_start))
                        .collect::<Vec<_>>(),
                    (self.recon_total, self.recon_percent, self.recon_strip_randomizer, self.recon_keep_positions, self.recon_eastern)
                )
            }
            AppMode::Exclusive => format!(
                "{:?}{}",
                self.bomber_slots
                    .iter()
                    .map(|s| (&s.path, &s.selected_triggers, s.selected_completion))
                    .collect::<Vec<_>>(),
                self.bomber_keep_positions
            ),
            AppMode::Fighter => format!(
                "{:?}",
                (
                    (self.linked_groups, self.flight_count, self.max_in_flight, self.country),
                    (&self.type_enabled, &self.type_skill, &self.custom_path),
                    (self.cooldown, self.reinforcement, self.delete_orders, self.altitude_min, self.altitude_max),
                )
            ),
            AppMode::Airfield => format!("{:?}{}", self.airfield_path, self.airfield_western),
        }
    }

    /// Drop the current tab's undo snapshot once the tab is edited after it
    /// (shell::Undo::settle). Only cheap fields go into the fingerprint.
    fn settle_undo(&mut self) {
        use std::hash::{Hash, Hasher};
        fn hash(text: String) -> u64 {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            h.finish()
        }
        match self.mode {
            AppMode::Template => {
                let fp = hash(self.tpl_fingerprint());
                self.tpl_undo.settle(fp);
            }
            AppMode::Recon => {
                let slots = match self.recon_submode {
                    ReconSubmode::New => &self.recon_slots,
                    ReconSubmode::Rework => &self.recon_rework,
                };
                let fp = hash(format!(
                    "{}{:?}",
                    self.recon_submode == ReconSubmode::New,
                    slots
                        .iter()
                        .map(|s| (&s.path, s.kind, &s.selected_triggers, s.influence, &s.restore_start))
                        .collect::<Vec<_>>()
                ));
                self.recon_undo.settle(fp);
            }
            AppMode::Exclusive => {
                let fp = hash(format!(
                    "{:?}",
                    self.bomber_slots
                        .iter()
                        .map(|s| (&s.path, &s.selected_triggers, s.selected_completion))
                        .collect::<Vec<_>>()
                ));
                self.bomber_undo.settle(fp);
            }
            AppMode::Map => {
                // Lines count too when the snapshot holds them (a line Clear).
                // Drawings with their own undo rebase it instead (`note_map_drawing`).
                let lines = self.map_undo.peek().is_some_and(|f| f.lines.is_some());
                let fp = hash(format!(
                    "{:?}",
                    (
                        (&self.map_ships, &self.map_ground_east, &self.map_ground_nato, &self.map_fighters),
                        (&self.east_objectives, &self.nato_objectives, self.map_armies.len(), self.map_refs.len()),
                        self.map_imported_fighters.len(),
                        lines.then_some((&self.custom_front_xz, &self.salients, &self.attack_arrows)),
                    )
                ));
                self.map_undo.settle(fp);
            }
            AppMode::Fighter | AppMode::Airfield => {}
        }
    }

    fn record_tpl_undo(&mut self, label: String) {
        self.tpl_undo.record(label, self.tpl_snapshot());
    }

    /// The undo label shown in the status bar for the current tab.
    fn undo_label(&self) -> Option<&str> {
        match self.mode {
            AppMode::Template => self.tpl_undo.label(),
            AppMode::Recon => self.recon_undo.label(),
            AppMode::Exclusive => self.bomber_undo.label(),
            AppMode::Map => self.map_undo_label(),
            AppMode::Fighter | AppMode::Airfield => None,
        }
    }

    /// Ctrl Z, the status bar's Undo and the Map palette's Undo. On Map it
    /// undoes the last action: a drawing (the drawn-mark undo) or a Clear.
    fn undo_current_tab(&mut self) {
        let label = self.undo_label().map(str::to_owned);
        match self.mode {
            AppMode::Template => {
                if let Some(s) = self.tpl_undo.take() {
                    self.tpl_restore(s);
                    self.sync_template_waypoint_speed();
                }
            }
            AppMode::Recon => {
                if let Some((sub, slots)) = self.recon_undo.take() {
                    match sub {
                        ReconSubmode::New => self.recon_slots = slots,
                        ReconSubmode::Rework => self.recon_rework = slots,
                    }
                }
            }
            AppMode::Exclusive => {
                if let Some((slots, selected)) = self.bomber_undo.take() {
                    self.bomber_slots = slots;
                    // Undo selects the plan that came back, not its neighbour.
                    self.bomber_selected = selected;
                }
            }
            AppMode::Map => match self.map_undo_kind() {
                Some(MapAction::Clear) => {
                    if let Some(f) = self.map_undo.take() {
                        self.restore_map_forces(f);
                    }
                    self.map_last = MapAction::Drawing;
                }
                Some(MapAction::Drawing) => self.remove_last_mark(),
                None => return,
            },
            AppMode::Fighter | AppMode::Airfield => return,
        }
        if let Some(label) = label {
            self.status = Status::Info(format!("Undone: {label}."));
        }
    }

    /// Everything the Template's Generate reads, as text. Equal text means no edits.
    fn tpl_fingerprint(&self) -> String {
        format!(
            "{:?}",
            (
                &self.tpl_seats,
                self.tpl_bring_up,
                self.tpl_spawn_reset,
                self.tpl_spawn_cooldown_min,
                self.tpl_place_layout,
                self.tpl_per_group,
                self.tpl_zone_in,
                self.tpl_zone_out,
                self.tpl_wp_speed,
                self.tpl_wp_altitude,
                self.tpl_wp_priority,
                self.tpl_zone_coalition,
            )
        )
    }

    fn map_fingerprint(&self) -> String {
        let a = self.front_aabb;
        format!(
            "{:?}",
            (
                (&self.map_ships, &self.map_ground_east, &self.map_ground_nato, &self.map_fighters),
                (&self.east_objectives, &self.nato_objectives, self.map_armies.len(), self.map_refs.len()),
                (&self.custom_front_xz, &self.salients, &self.attack_arrows),
                (a.x_min, a.x_max, a.z_min, a.z_max, self.front_t),
            )
        )
    }

    /// The current state counts as saved (startup, Load, Generate, Reset).
    fn mark_saved(&mut self, mode: AppMode) {
        match mode {
            AppMode::Template => self.tpl_saved = self.tpl_fingerprint(),
            AppMode::Map => self.map_saved = self.map_fingerprint(),
            _ => {}
        }
    }

    fn is_dirty(&self, mode: AppMode) -> bool {
        match mode {
            AppMode::Template => !self.tpl_seats.is_empty() && self.tpl_fingerprint() != self.tpl_saved,
            AppMode::Map => self.map_fingerprint() != self.map_saved,
            _ => false,
        }
    }

    /// Runs a Load / Generate and marks the tab saved if it reported success.
    /// The status is cleared first, so the signal is "`f` reported a result
    /// that is not an error" — a second identical Generate counts too. A
    /// cancelled dialog reports nothing and keeps the previous status.
    fn run_io(&mut self, mode: AppMode, f: impl FnOnce(&mut Self)) {
        let before = std::mem::take(&mut self.status);
        f(self);
        match self.status {
            Status::Idle => self.status = before,
            Status::Error(_) => {}
            Status::Info(_) | Status::Warn { .. } => self.mark_saved(mode),
        }
    }

    fn confirm_dialogs(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.confirm else {
            return;
        };
        let (title, body, button) = match kind {
            Confirm::ResetTemplate => {
                let orders: usize = self.tpl_seats.iter().map(|s| s.orders.len()).sum();
                (
                    "Reset the template?",
                    format!(
                        "This clears {} units, {orders} orders and the placement settings. The catalog stays. You can undo with Ctrl Z.",
                        self.tpl_seats.len()
                    ),
                    "Reset",
                )
            }
            Confirm::ResetFighter => (
                "Reset Fighter Pack?",
                "This sets linked groups, flights, aircraft types and skills, country, altitudes and timers back to their defaults.".to_string(),
                "Reset",
            ),
            Confirm::LoadTemplate => (
                "Replace the template?",
                "The template has edits that are not in a generated file. Loading replaces them. You can undo with Ctrl Z.".to_string(),
                "Load…",
            ),
            Confirm::LoadBaseMap => (
                "Replace the map?",
                "The map has changes that are not in a generated base map. Loading replaces the AO, front, arrows and placed forces.".to_string(),
                "Load…",
            ),
        };
        let mut open = true;
        if shell::confirm_dialog(ctx, &mut open, title, &body, button) {
            match kind {
                Confirm::ResetTemplate => self.reset_template_confirmed(),
                Confirm::ResetFighter => self.reset_fighter_pack(),
                Confirm::LoadTemplate => self.load_template_now(),
                Confirm::LoadBaseMap => self.run_io(AppMode::Map, |s| s.load_base_map()),
            }
        }
        if !open {
            self.confirm = None;
        }
    }

    fn reset_template_confirmed(&mut self) {
        self.record_tpl_undo("Reset the template".into());
        self.reset_template_builder();
        self.mark_saved(AppMode::Template);
    }

    /// Load… on Template. Recorded for undo only when something was loaded.
    fn load_template_now(&mut self) {
        let before = self.tpl_snapshot();
        let fp = self.tpl_fingerprint();
        self.run_io(AppMode::Template, |s| s.load_template_group());
        if self.tpl_fingerprint() != fp {
            self.tpl_undo.record("Replaced by Load", before);
        }
    }

    // ── Map › Terrain: measured ground heights (terrain.rs, heightprobe.rs) ──

    /// The height store, loaded from disk the first time it is needed.
    fn terrain_store(&mut self) -> Option<&HeightStore> {
        if self.terrain_store.is_none() && self.terrain_error.is_none() {
            match HeightStore::load(&self.terrain_store_path, crate::terrain::KOREA_MAP_ID) {
                Ok(s) => self.terrain_store = Some(s),
                Err(e) => self.terrain_error = Some(e),
            }
        }
        self.terrain_store.as_ref()
    }

    /// (tile row, tile col, land probes) for every tile with probes; cached.
    fn terrain_tiles(&mut self) -> &[(usize, usize, usize)] {
        self.terrain_tiles.get_or_insert_with(heightprobe::probe_tiles)
    }

    /// Tiles with probes that overlap the AO.
    fn terrain_ao_tiles(&mut self) -> Vec<(usize, usize, usize)> {
        let ao = self.front_aabb;
        self.terrain_tiles()
            .iter()
            .copied()
            .filter(|&(ti, tj, _)| {
                let (i_lo, i_hi, j_lo, j_hi) = crate::terrain::tile_node_range(ti, tj);
                let (x0, x1) = (i_lo as f64 * TERRAIN_STEP, i_hi as f64 * TERRAIN_STEP);
                let (z0, z1) = (j_lo as f64 * TERRAIN_STEP, j_hi as f64 * TERRAIN_STEP);
                x1 >= ao.x_min && x0 <= ao.x_max && z1 >= ao.z_min && z0 <= ao.z_max
            })
            .collect()
    }

    /// "Ground 412 m · 100 m grid" for the point under the pointer, while a
    /// terrain layer is on.
    fn terrain_readout(&mut self) -> Option<String> {
        if !(self.terrain_show_coverage || self.terrain_show_relief) {
            return None;
        }
        let (x, z) = self.map_hover_xz?;
        Some(match self.terrain_store().and_then(|s| s.lookup(x, z)) {
            Some(h) if h.spacing_m == 0.0 => format!("Ground {:.0} m · measured point", h.y),
            Some(h) => format!("Ground {:.0} m · {:.0} m grid", h.y, h.spacing_m),
            None => "Ground not measured".to_string(),
        })
    }

    fn map_terrain_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        shell::section_title(ui, "Height store", None);
        let path = self.terrain_store_path.display().to_string();
        ui.add(egui::Label::new(RichText::new(path).small().monospace().color(c::NEUTRAL_700)).truncate());
        let total_probes: usize = self.terrain_tiles().iter().map(|t| t.2).sum();
        let tiles = self.terrain_tiles().to_vec();
        if let Some(err) = self.terrain_error.clone() {
            shell::warning(ui, &err);
        } else if let Some(store) = self.terrain_store() {
            let measured = store.measured_nodes();
            let tiles_done = tiles.iter().filter(|&&(ti, tj, _)| store.tile_measured(ti, tj) > 0).count();
            let points = store.points().len();
            let pct = if total_probes > 0 { measured as f64 * 100.0 / total_probes as f64 } else { 0.0 };
            ui.label(format!(
                "{} of {} land nodes measured ({pct:.1}%)",
                group_digits(measured as f64),
                group_digits(total_probes as f64)
            ));
            ui.label(
                RichText::new(format!(
                    "{tiles_done} of {} tiles started · {} extra points",
                    tiles.len(),
                    group_digits(points as f64)
                ))
                    .small()
                    .color(c::NEUTRAL_700),
            );
        }
        if ui.button("Reload").on_hover_text("Read the store from disk again").clicked() {
            self.terrain_store = None;
            self.terrain_error = None;
            self.terrain_relief = None;
        }
        ui.checkbox(&mut self.terrain_apply, "Apply terrain heights on export").on_hover_text(
            "Ground units and their waypoints go to the measured ground + margin, parked planes just above it, ships to sea level. Units already on the ground keep their height.",
        );
        shell::hint(
            ui,
            "Used by Template, Exclusive, Army, Map and Fighter Pack exports (not Airfield). Where the terrain is not measured, units keep their height and the export lists them.",
            false,
        );
        ui.add_space(6.0);
        ui.separator();

        shell::section_title(ui, "Measure", Some("export probes"));
        if ui
            .button("Export survey pass…")
            .on_hover_text("390 625 probe points, 800 m apart over the whole map (sea and frame included)")
            .clicked()
        {
            self.terrain_export_survey();
        }
        let ao_tiles = self.terrain_ao_tiles();
        let ao_probes: usize = ao_tiles.iter().map(|t| t.2).sum();
        ui.add_enabled_ui(!ao_tiles.is_empty(), |ui| {
            if ui
                .button(format!("Export AO tiles ({})…", ao_tiles.len()))
                .on_hover_text("One .Group per 224 × 224-node tile the AO touches, 100 m apart")
                .clicked()
            {
                self.terrain_export_tiles(&ao_tiles);
            }
        });
        shell::hint(
            ui,
            &format!(
                "The AO touches {} tiles, {} probes. Import a file in the editor, select all, set to ground, save, then Import snapped.",
                ao_tiles.len(),
                group_digits(ao_probes as f64)
            ),
            false,
        );
        ui.add_space(6.0);
        ui.separator();

        shell::section_title(ui, "Import", Some("snapped files"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Import snapped…")
                .on_hover_text("Merge probe files you ran set to ground on into the store")
                .clicked()
            {
                self.terrain_import(false);
            }
            if ui
                .button("Learn from mission…")
                .on_hover_text("Also keep the snapped height of pinned ground units in a mission you ran set to ground on")
                .clicked()
            {
                self.terrain_import(true);
            }
        });
        for line in self.terrain_log.iter().take(8) {
            ui.add(egui::Label::new(RichText::new(line).small().monospace()).wrap());
        }
        ui.add_space(6.0);
        ui.separator();

        shell::section_title(ui, "Map layers", None);
        ui.checkbox(&mut self.terrain_show_coverage, "Coverage")
            .on_hover_text("Tiles shaded by how much of their land is measured");
        if ui
            .checkbox(&mut self.terrain_show_relief, "Relief")
            .on_hover_text("Measured heights: color by height, with hillshade")
            .changed()
            && self.terrain_show_relief
        {
            self.terrain_relief = None;
        }
        shell::hint(ui, "With a layer on, the map shows the ground height under the pointer.", false);
        ui.add_space(6.0);
        if shell::hint(ui, "How to measure heights, step by step.", true) {
            self.open_help(HelpTopic::Front);
        }
    }

    /// Put an export's units on the measured terrain (terrain_apply.rs).
    /// Returns the status note, or `None` when switched off or nothing applied.
    fn apply_terrain(&mut self, root: &mut crate::ast::Il2Entity) -> Option<(String, bool)> {
        if !self.terrain_apply {
            return None;
        }
        let store = self.terrain_store()?;
        let rep = crate::terrain_apply::apply_terrain_heights(root, store);
        let note = rep.summary();
        (!note.is_empty()).then_some((note, !rep.unmeasured.is_empty()))
    }

    /// Add the terrain note to the export's status; units left unmeasured
    /// turn it into a warning.
    fn add_terrain_note(&mut self, note: Option<(String, bool)>) {
        let Some((note, warn)) = note else { return };
        self.status = match std::mem::replace(&mut self.status, Status::Idle) {
            Status::Info(msg) if warn => Status::Warn { lead: msg, items: vec![note] },
            Status::Info(msg) => Status::Info(format!("{msg} {note}")),
            Status::Warn { lead, mut items } => {
                items.push(note);
                Status::Warn { lead, items }
            }
            other => other,
        };
    }

    fn terrain_export_survey(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name("HG800_survey.Group")
            .save_file()
        else {
            return;
        };
        match heightprobe::survey_group().and_then(|g| {
            let n = g.children.len();
            std::fs::write(&path, serialize_group(&g)).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(n)
        }) {
            Ok(n) => {
                self.status = Status::Info(format!(
                    "Wrote {n} survey probes to {}. Import it, select all, set to ground, save, then Import snapped.",
                    path.display()
                ));
            }
            Err(e) => self.status = Status::Error(e),
        }
    }

    fn terrain_export_tiles(&mut self, tiles: &[(usize, usize, usize)]) {
        let Some(dir) = dialog::FileDialog::new().pick_folder() else {
            return;
        };
        let mut written = 0;
        let mut probes = 0;
        for &(ti, tj, _) in tiles {
            match heightprobe::tile_probe_group(ti, tj) {
                Ok(Some(g)) => {
                    let path = dir.join(format!("{}.Group", heightprobe::tile_name(ti, tj)));
                    if let Err(e) = std::fs::write(&path, serialize_group(&g)) {
                        self.status = Status::Error(format!("{}: {e}", path.display()));
                        return;
                    }
                    written += 1;
                    probes += g.children.len();
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = Status::Error(e);
                    return;
                }
            }
        }
        self.status = Status::Info(format!(
            "Wrote {written} probe tiles ({probes} probe points) to {}.",
            dir.display()
        ));
    }

    /// Merge snapped files into the store and save it (the CLI's --probe-ingest).
    fn terrain_import(&mut self, learn: bool) {
        let Some(paths) = dialog::FileDialog::new()
            .add_filter("Snapped group or mission", &["Group", "group", "Mission", "mission"])
            .pick_files()
        else {
            return;
        };
        self.terrain_import_paths(&paths, learn);
    }

    fn terrain_import_paths(&mut self, paths: &[PathBuf], learn: bool) {
        if self.terrain_store().is_none() {
            self.status = Status::Error(self.terrain_error.clone().unwrap_or_else(|| "No height store.".into()));
            return;
        }
        let mut store = self.terrain_store.take().unwrap_or_else(|| HeightStore::new(crate::terrain::KOREA_MAP_ID));
        let mut refused = Vec::new();
        let mut merged = 0;
        let mut lines = Vec::new();
        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
            let root = match std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| parse_il2_document(&t).map_err(|e| e.to_string()))
            {
                Ok(r) => r,
                Err(e) => {
                    lines.push(format!("{name}: failed, {e}"));
                    refused.push(name);
                    continue;
                }
            };
            let rep = heightprobe::ingest(&root, &mut store, learn);
            lines.push(format!(
                "{name}: {} heights, {} water, {} extra, {} learned, {} unsnapped{}",
                rep.lattice,
                rep.water,
                rep.probe_points,
                rep.learned,
                rep.unsnapped,
                if rep.merged { "" } else { " — not merged (looks unsnapped)" }
            ));
            if rep.merged {
                merged += 1;
            } else {
                refused.push(name);
            }
        }
        let saved = store.save(&self.terrain_store_path);
        self.terrain_store = Some(store);
        self.terrain_relief = None;
        for line in lines.into_iter().rev() {
            self.terrain_log.insert(0, line);
        }
        self.terrain_log.truncate(30);
        self.status = match saved {
            Err(e) => Status::Error(format!("Heights merged but not saved: {e}")),
            Ok(()) if refused.is_empty() => Status::Info(format!("Merged {merged} snapped file(s) into the height store.")),
            Ok(()) => Status::Warn {
                lead: format!("Merged {merged} file(s); {} not merged:", refused.len()),
                items: refused
                    .into_iter()
                    .map(|n| format!("{n}: select all, set to ground, save, then import again"))
                    .collect(),
            },
        };
    }

    /// Relief texture over the whole map at 800 m: hypsometric color plus a
    /// hillshade from the north-west; unmeasured ground stays transparent.
    fn terrain_relief_texture(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
        if let Some(t) = &self.terrain_relief {
            return Some(t.clone());
        }
        let store = self.terrain_store()?;
        const N: usize = 624; // 800 m per pixel over the 499.2 km map
        let mut h = vec![f32::NAN; N * N];
        for py in 0..N {
            for px in 0..N {
                let uv = Pos2::new((px as f32 + 0.5) / N as f32, (py as f32 + 0.5) / N as f32);
                let (x, z) = uv_to_world(uv);
                if let Some(y) = store.height_at(x, z) {
                    h[py * N + px] = y as f32;
                }
            }
        }
        let mut img = ColorImage::new([N, N], vec![Color32::TRANSPARENT; N * N]);
        for py in 0..N {
            for px in 0..N {
                let y = h[py * N + px];
                if y.is_nan() {
                    continue;
                }
                let at = |dx: isize, dy: isize| {
                    let (qx, qy) = (px as isize + dx, py as isize + dy);
                    if qx < 0 || qy < 0 || qx >= N as isize || qy >= N as isize {
                        return y;
                    }
                    let v = h[qy as usize * N + qx as usize];
                    if v.is_nan() { y } else { v }
                };
                let (gx, gy) = ((at(1, 0) - at(-1, 0)) / 1600.0, (at(0, 1) - at(0, -1)) / 1600.0);
                let shade = (0.75 - (gx + gy) * 1.2).clamp(0.35, 1.15);
                let base = relief_color(y);
                let c = |v: u8| ((v as f32 * shade).min(255.0)) as u8;
                img.pixels[py * N + px] = Color32::from_rgba_unmultiplied(c(base.r()), c(base.g()), c(base.b()), 170);
            }
        }
        let tex = ctx.load_texture("terrain_relief", img, egui::TextureOptions::LINEAR);
        self.terrain_relief = Some(tex.clone());
        Some(tex)
    }

    /// Terrain layers over the map (drawn under units and the AO box).
    fn draw_terrain_layers(&mut self, ctx: &egui::Context, painter: &egui::Painter, map_rect: Rect) {
        if self.terrain_show_relief {
            if let Some(tex) = self.terrain_relief_texture(ctx) {
                painter.image(tex.id(), map_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        }
        if self.terrain_show_coverage {
            let tiles = self.terrain_tiles().to_vec();
            let Some(store) = self.terrain_store() else { return };
            for (ti, tj, probes) in tiles {
                let (i_lo, i_hi, j_lo, j_hi) = crate::terrain::tile_node_range(ti, tj);
                let r = Rect::from_two_pos(
                    world_to_pos(map_rect, i_lo as f64 * TERRAIN_STEP, j_lo as f64 * TERRAIN_STEP),
                    world_to_pos(map_rect, (i_hi + 1) as f64 * TERRAIN_STEP, (j_hi + 1) as f64 * TERRAIN_STEP),
                );
                let frac = (store.tile_measured(ti, tj) as f32 / probes.max(1) as f32).min(1.0);
                if frac > 0.0 {
                    painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0x41, 0x61, 0x80, (40.0 + 110.0 * frac) as u8));
                }
                painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0x2c, 0x45, 0x5d, 90)), egui::StrokeKind::Inside);
            }
        }
    }

    // ── Army Generator (README §5.2) ───────────────────────────────────────

    fn recon_page(&mut self, ctx: &egui::Context) {
        self.ensure_map_assets(ctx);
        side_panel(ctx, "recon_left", true, 330.0, |ui| self.recon_templates_panel(ui));
        side_panel(ctx, "recon_right", false, 300.0, |ui| self.recon_settings_panel(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            let rework = self.recon_submode == ReconSubmode::Rework;
            let empty = if rework { self.recon_rework.is_empty() } else { self.recon_slots.is_empty() };
            if !empty {
                // COPY MIX is its own block at the bottom, sized to its rows.
                let max_h = (ui.available_height() * 0.45).max(120.0);
                egui::TopBottomPanel::bottom("recon_mix")
                    .resizable(false)
                    .frame(egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 0, top: 8, bottom: 4 }))
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("recon_mix_scroll")
                            .max_height(max_h)
                            // Without this the area stops at egui's 64 px
                            // default and hides the table rows.
                            .min_scrolled_height(max_h)
                            .auto_shrink([false, true])
                            .show(ui, |ui| self.recon_copy_mix(ui));
                    });
            }
            egui::ScrollArea::vertical()
                .id_salt("recon_center")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    self.recon_center(ui);
                });
        });
    }

    fn recon_templates_panel(&mut self, ui: &mut egui::Ui) {
        let rework = self.recon_submode == ReconSubmode::Rework;
        let n = if rework { self.recon_rework.len() } else { self.recon_slots.len() };
        shell::section_title(
            ui,
            &format!("Templates · {n}"),
            (!rework && n > 0).then_some("click a type to change it"),
        );
        if n == 0 {
            shell::hint(
                ui,
                if rework {
                    "Add the Random Ground Units packs this utility exported with Add packs… (Ctrl O)."
                } else {
                    "Add ground-unit .Group templates, or a folder of them, with Add templates… (Ctrl O)."
                },
                false,
            );
            return;
        }
        let side = shell::Side::from_eastern(self.recon_eastern);
        let removed = if rework {
            let label = (!self.recon_strip_randomizer).then_some("Activate %");
            recon_slot_list(ui, &mut self.recon_rework, label, None, side)
        } else {
            let icons = self.type_icons(self.recon_eastern);
            recon_slot_list(ui, &mut self.recon_slots, Some("Influence"), Some(icons), side)
        };
        if let Some(i) = removed {
            let (sub, slots) = if rework {
                (ReconSubmode::Rework, &mut self.recon_rework)
            } else {
                (ReconSubmode::New, &mut self.recon_slots)
            };
            self.recon_undo.record(format!("Removed {}", slots[i].info.name), (sub, slots.clone()));
            slots.remove(i);
        }
    }

    fn recon_center(&mut self, ui: &mut egui::Ui) {
        let rework = self.recon_submode == ReconSubmode::Rework;
        let sentence = if rework {
            "Reworks packs this utility exported: copies stay where they are and get a new activate mix."
        } else {
            "Copies templates onto a 10 km parking grid and picks which copies activate each mission."
        };
        if shell::hint(ui, sentence, true) {
            self.open_help(HelpTopic::Recon);
        }
        ui.add_space(8.0);
        let empty = if rework { self.recon_rework.is_empty() } else { self.recon_slots.is_empty() };
        if empty {
            let label = if rework { "Add packs…" } else { "Add templates…" };
            if empty_state(ui, &format!("{label} to begin."), label) {
                self.load_action();
            }
            return;
        }
        if !rework {
            self.recon_parking_grid(ui);
        }
    }

    /// Schematic of the parking grid: one square block per template, sized
    /// to its copies (2×2 for 4, 3×3 for 9), with a side marker per copy.
    fn recon_parking_grid(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "Parking grid · 10 km cells from 40000, 40000", None);
        if self.recon_keep_positions {
            shell::hint(ui, "Keep loaded positions is on: copies stay where they were authored.", false);
            return;
        }
        let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
        let mix = allocate_mix(&weights, self.recon_total as usize, self.recon_percent);
        let side = shell::Side::from_eastern(self.recon_eastern);
        const CELL: f32 = 18.0;
        const BLOCK_W: f32 = 132.0;
        let blocks: Vec<(String, usize)> = self
            .recon_slots
            .iter()
            .zip(&mix)
            .filter(|(_, m)| m.copies > 0)
            .map(|(slot, m)| (slot.info.name.clone(), m.copies))
            .collect();
        let per_row = ((ui.available_width() / BLOCK_W).floor() as usize).max(1);
        let rows = blocks.len().div_ceil(per_row).max(1);
        let max_side = blocks
            .iter()
            .map(|(_, n)| (*n as f32).sqrt().ceil() as usize)
            .max()
            .unwrap_or(1);
        let row_h = max_side as f32 * CELL + 30.0;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), rows as f32 * row_h),
            Sense::hover(),
        );
        let p = ui.painter_at(rect);
        for (bi, (name, copies)) in blocks.iter().enumerate() {
            let side_n = (*copies as f32).sqrt().ceil() as usize;
            let origin = rect.min
                + Vec2::new((bi % per_row) as f32 * BLOCK_W, (bi / per_row) as f32 * row_h);
            for k in 0..side_n * side_n {
                let cell = Rect::from_min_size(
                    origin + Vec2::new((k % side_n) as f32 * CELL, (k / side_n) as f32 * CELL),
                    Vec2::splat(CELL),
                );
                p.rect_stroke(cell, 0.0, Stroke::new(1.0_f32, c::NEUTRAL_400), egui::StrokeKind::Inside);
                if k < *copies {
                    shell::paint_side_marker(&p, cell.center(), 10.0, side);
                }
            }
            p.with_clip_rect(Rect::from_min_size(
                origin + Vec2::new(0.0, side_n as f32 * CELL + 4.0),
                Vec2::new(BLOCK_W - 8.0, 20.0),
            ))
            .text(
                origin + Vec2::new(0.0, side_n as f32 * CELL + 4.0),
                Align2::LEFT_TOP,
                format!("{name} · {copies}"),
                FontId::proportional(12.0),
                c::NEUTRAL_800,
            );
        }
    }

    fn recon_copy_mix(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "Copy mix", None);
        let rework = self.recon_submode == ReconSubmode::Rework;
        let strip = self.recon_strip_randomizer;
        let verb = if strip { "spawn" } else { "activate" };
        let mut placed = 0usize;
        let mut live = 0usize;
        egui::Grid::new("recon_mix")
            .num_columns(if rework { 3 } else { 4 })
            .striped(true)
            .spacing([18.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Template").font(FontId::new(12.0, theme::bold_family())));
                if rework {
                    ui.label(RichText::new("On map").font(FontId::new(12.0, theme::bold_family())));
                } else {
                    ui.label(RichText::new("Type").font(FontId::new(12.0, theme::bold_family())));
                    ui.label(RichText::new("Placed").font(FontId::new(12.0, theme::bold_family())));
                }
                ui.label(RichText::new(if strip { "Spawn" } else { "Activate" }).font(FontId::new(12.0, theme::bold_family())));
                ui.end_row();
                if rework {
                    for slot in &self.recon_rework {
                        let n = slot.detected.unwrap_or(0);
                        let mix = TypeMix::from_copies(n, slot.influence.clamp(1, 100));
                        let act = if strip { mix.copies } else { mix.activate };
                        placed += mix.copies;
                        live += act;
                        ui.label(&slot.info.name);
                        ui.label(mix.copies.to_string());
                        ui.label(act.to_string());
                        ui.end_row();
                    }
                } else {
                    let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
                    let mix = allocate_mix(&weights, self.recon_total as usize, self.recon_percent);
                    for (slot, m) in self.recon_slots.iter().zip(mix.iter()) {
                        let act = if strip { m.copies } else { m.activate };
                        placed += m.copies;
                        live += act;
                        ui.label(&slot.info.name);
                        ui.label(slot.kind.label());
                        ui.label(m.copies.to_string());
                        ui.label(act.to_string());
                        ui.end_row();
                    }
                }
            });
        ui.add_space(6.0);
        if placed > 0 {
            ui.label(
                RichText::new(format!("{live} of {placed} copies will {verb}."))
                    .font(FontId::new(13.0, theme::bold_family()))
                    .color(c::ACCENT_800),
            );
        }
        if !strip {
            let text = if rework {
                "Copies stay where they are. Each type's Activate % runs its own waterfall."
            } else {
                "Activate % applies per type, not to the pack total."
            };
            if shell::hint(ui, text, true) {
                self.open_help(HelpTopic::Recon);
            }
        }
    }

    fn recon_settings_panel(&mut self, ui: &mut egui::Ui) {
        let rework = self.recon_submode == ReconSubmode::Rework;
        shell::section_title(ui, "Army", None);
        shell::segmented(ui, &mut self.recon_eastern, &[(true, "DPRK"), (false, "NATO")]);
        shell::hint(ui, "Sets the icons on this page. Map › Place picks the side.", false);
        if !rework {
            ui.add_space(4.0);
            ui.label("Import new templates as");
            self.unit_kind_picker(ui);
        }
        ui.add_space(6.0);
        ui.separator();
        shell::section_title(ui, "Copies", None);
        if rework {
            ui.checkbox(&mut self.recon_strip_randomizer, "Spawn all copies (no randomizer)");
            if self.recon_strip_randomizer {
                shell::hint(ui, "Every copy starts. Pick what Mission Begin fires for each pack below.", false);
            }
            let detected_total: usize = self.recon_rework.iter().filter_map(|s| s.detected).sum();
            ui.label(format!("{detected_total} groups detected on the map"));
            if !self.recon_strip_randomizer {
                ui.label("Activate ratio");
                ui.horizontal(|ui| {
                    let changed = ui
                        .add(egui::Slider::new(&mut self.recon_percent, 1..=100).show_value(false).trailing_fill(true))
                        .changed();
                    let typed = ui
                        .add(egui::DragValue::new(&mut self.recon_percent).range(1..=100).speed(0.2).suffix("%"))
                        .changed();
                    if changed || typed {
                        for slot in &mut self.recon_rework {
                            slot.influence = self.recon_percent;
                        }
                    }
                });
                shell::hint(ui, "Sets every pack's Activate %. Adjust one pack on its card.", false);
            }
        } else {
            labeled_slider(ui, "Templates to create", &mut self.recon_total, 1..=64);
            ui.checkbox(&mut self.recon_strip_randomizer, "Spawn all copies (no randomizer)");
            if self.recon_strip_randomizer {
                shell::hint(ui, "Every copy keeps its Mission Begin chain and starts.", false);
            } else {
                labeled_slider_suffix(ui, "Activate ratio", &mut self.recon_percent, 1..=100, "%");
                if shell::hint(ui, "Share of each type's copies that start.", true) {
                    self.open_help(HelpTopic::Recon);
                }
            }
            ui.checkbox(&mut self.recon_keep_positions, "Keep loaded positions");
        }
        recon_dserver_note(ui);
        ui.add_space(4.0);
        ui.separator();
        let summary = format!(
            "Start {} s · {} ms apart",
            self.recon_start_delay_s, self.recon_group_delay_ms
        );
        shell::settings_section(ui, "recon_timing", "Timing", &summary, false, |ui| {
            let strip = self.recon_strip_randomizer;
            if strip {
                // Disabled, not hidden: the values come back with the randomizer.
                shell::hint(ui, "Timing applies only with the randomizer.", false);
            }
            ui.add_enabled_ui(!strip, |ui| self.recon_timing_sliders(ui));
        });
        if rework && self.recon_strip_randomizer && !self.recon_rework.is_empty() {
            shell::section_title(ui, "Reconnect Mission Begin", None);
            let detected_total: usize = self.recon_rework.iter().filter_map(|s| s.detected).sum();
            for (i, slot) in self.recon_rework.iter_mut().enumerate() {
                if slot.restore_start.is_empty() {
                    if let Some(c) = slot.info.suggested_restore() {
                        slot.restore_start = c.name.clone();
                    }
                }
                let selected = slot.restore_start.clone();
                ui.label(RichText::new(&slot.info.name).font(FontId::new(13.0, theme::bold_family())));
                egui::ComboBox::from_id_salt(format!("restore_start_{i}"))
                    .selected_text(if selected.is_empty() {
                        "Select a timer or checkzone".to_string()
                    } else {
                        selected
                    })
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for choice in &slot.info.restore_starts {
                            let kind = match choice.kind {
                                RestoreKind::Timer => "Timer",
                                RestoreKind::CheckZone => "CheckZone",
                            };
                            let rec = if choice.recommended { "  (recommended)" } else { "" };
                            let label = format!("{}  [{kind}]{rec}", choice.name);
                            if ui.selectable_label(slot.restore_start == choice.name, label).clicked() {
                                slot.restore_start = choice.name.clone();
                            }
                        }
                    });
                if let Some(c) = slot.info.restore_starts.iter().find(|c| c.name == slot.restore_start) {
                    ui.label(RichText::new(&c.hint).small().color(c::WARN_TEXT));
                }
                ui.add_space(4.0);
            }
            ui.label(
                RichText::new(format!("All {detected_total} copies will start (no randomizer)."))
                    .font(FontId::new(13.0, theme::bold_family()))
                    .color(c::ACCENT_800),
            );
        }
    }

    fn recon_timing_sliders(&mut self, ui: &mut egui::Ui) {
        labeled_slider(ui, "Start delay (s)", &mut self.recon_start_delay_s, 0..=180);
        shell::hint(ui, "Wait after Mission Begin, so several Army Generator packs do not all fire at t=0.", false);
        labeled_slider(ui, "Delay between groups (ms)", &mut self.recon_group_delay_ms, 0..=5000);
        shell::hint(ui, "Each following type waits this long so MCU load does not spike.", false);
    }

    // ── Fighter Pack (README §5.3) ─────────────────────────────────────────

    fn fighter_page(&mut self, ctx: &egui::Context) {
        side_panel(ctx, "fighter_left", true, 270.0, |ui| self.fighter_types_panel(ui));
        side_panel(ctx, "fighter_right", false, 304.0, |ui| self.fighter_settings_panel(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            paint_blueprint_grid(ui.painter(), ui.max_rect());
            egui::ScrollArea::vertical()
                .id_salt("fighter_center")
                .auto_shrink([false, false])
                .show(ui, |ui| self.fighter_preview(ui));
        });
    }

    fn fighter_flight_config(&self) -> FlightConfig {
        let (type_ids, type_skills) = self.selected_types();
        FlightConfig {
            flight_count: self.flight_count,
            max_in_flight: self.max_in_flight,
            type_ids,
            type_skills,
            country: self.country,
            cooldown: self.cooldown,
            reinforcement: self.reinforcement,
            delete_orders: self.delete_orders,
            altitude_min: self.altitude_min,
            altitude_max: self.altitude_max,
        }
    }

    fn fighter_types_panel(&mut self, ui: &mut egui::Ui) {
        let n = self.type_enabled.iter().filter(|e| **e).count();
        shell::section_title(ui, &format!("Aircraft types · {n} selected"), None);
        shell::hint(ui, "Flights cycle through the selected types. Lead skill ≥ wingman.", false);
        ui.add_space(4.0);
        ui.spacing_mut().slider_width = 90.0;
        for (i, ac) in AIRCRAFT_TYPES.iter().enumerate() {
            ui.scope(|ui| {
                if !self.type_enabled[i] {
                    ui.set_opacity(0.55);
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.type_enabled[i], ac.label);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled(
                            self.type_enabled[i],
                            egui::Slider::new(&mut self.type_skill[i], 0..=4)
                                .integer()
                                .trailing_fill(true),
                        )
                        .on_hover_text("Skill 0–4 for this type's leads");
                    });
                });
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Skill 0–4").small().color(c::NEUTRAL_700));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if shell::link(ui, "Select all").clicked() {
                    self.type_enabled.iter_mut().for_each(|e| *e = true);
                }
            });
        });
        if n == 0 {
            shell::warning(ui, "Select at least one aircraft type.");
        }
    }

    fn fighter_preview(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        shell::section_title(ui, "Pack preview", None);
        // No types: generation would refuse, so do not preview the fallback type.
        if self.selected_types().0.is_empty() {
            ui.label("Select at least one aircraft type to preview the pack.");
            return;
        }
        let flights = crate::flights::preview_flights(&self.fighter_flight_config());
        let total: usize = flights.iter().map(|f| f.seats.len()).sum();
        let groups = self.linked_groups.max(1) as usize;
        let custom = self.custom_path.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str());
        ui.label(Self::fighter_pack_sentence(custom, groups));
        ui.add_space(12.0);
        Self::fighter_group_cards(ui, groups, total, self.country);
        ui.add_space(12.0);

        let sizes: Vec<String> = flights.iter().map(|f| f.seats.len().to_string()).collect();
        shell::blueprint(ui, c::DIVIDER, |ui| {
            shell::section_title(
                ui,
                "Group 1 · flights",
                Some(&format!("{total} aircraft · sizes {}", sizes.join("/"))),
            );
            // Stripes are painted by hand across the full table width; the
            // grid's own stripes stop at its last column.
            let table_w = ui.available_width();
            let row_gap = 6.0;
            egui::Grid::new("fighter_flights")
                .num_columns(5)
                .spacing([22.0, row_gap])
                .show(ui, |ui| {
                    for h in ["Flight", "Type", "Aircraft", "Role", "Altitude"] {
                        ui.label(RichText::new(h).font(FontId::new(12.0, theme::bold_family())));
                    }
                    ui.end_row();
                    let left = ui.max_rect().left();
                    let stripe = ui.visuals().faint_bg_color;
                    for (f, fl) in flights.iter().enumerate() {
                        let (low, high) = Self::flight_element_altitudes(&fl.seats);
                        let slot = (f % 2 == 0).then(|| ui.painter().add(egui::Shape::Noop));
                        let top = ui.cursor().top();
                        Self::fighter_flight_row(ui, f, fl, low, high);
                        if let Some(slot) = slot {
                            let bottom = ui.cursor().top() - row_gap;
                            let rect = egui::Rect::from_min_max(
                                Pos2::new(left - 2.0, top - row_gap / 2.0),
                                Pos2::new(left + table_w, bottom + row_gap / 2.0),
                            );
                            ui.painter().set(slot, egui::Shape::rect_filled(rect, 0.0, stripe));
                        }
                    }
                });
            ui.add_space(10.0);
            Self::fighter_altitude_strip(ui, &flights, self.altitude_min, self.altitude_max);
        });
    }

    /// One row of the Group 1 flights table (ends the grid row).
    fn fighter_flight_row(ui: &mut egui::Ui, f: usize, fl: &crate::flights::FlightPreview, low: f64, high: Option<f64>) {
        ui.label(crate::aircraft::plane_display_name(f, 0));
        ui.label(fl.type_label);
        ui.label(fl.seats.len().to_string());
        ui.label(Self::flight_role_text(&fl.seats));
        ui.label(RichText::new(Self::altitude_text(low, high)).monospace());
        ui.end_row();
    }

    /// "Group logic is built in. 3 linked groups, chained through NodeGates,
    /// parked on a 10 km grid from 40000, 40000." — `generate_pack` parks
    /// each group with `placement::move_to_grid` (square grid from MAP_MIN).
    fn fighter_pack_sentence(custom: Option<&str>, groups: usize) -> String {
        let logic = match custom {
            Some(file) => format!("Group logic from {file}."),
            None => "Group logic is built in.".to_string(),
        };
        let (x, z) = crate::placement::grid_xz(0, groups);
        let parking = if groups <= 1 {
            format!("One group, parked at {x:.0}, {z:.0}.")
        } else {
            format!(
                "{groups} linked groups, chained through NodeGates, parked on a {:.0} km grid from {x:.0}, {z:.0}.",
                crate::placement::GRID_STEP / 1000.0
            )
        };
        format!("{logic} {parking}")
    }

    /// Group cards and NodeGate links, broken into rows so a link always
    /// travels with the card after it (a row never ends on a link).
    fn fighter_group_cards(ui: &mut egui::Ui, groups: usize, total: usize, country: i32) {
        let width = ui.available_width();
        for (r, row) in Self::pack_card_rows(groups, width).into_iter().enumerate() {
            let n = row.len() as f32;
            let links = if r == 0 { n - 1.0 } else { n };
            let row_w = n * PACK_CARD_W + links * PACK_LINK_W;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(((width - row_w) / 2.0).max(0.0));
                for g in row {
                    if g > 0 {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(PACK_LINK_W, PACK_CARD_H), Sense::hover());
                        let y = rect.center().y;
                        let p = ui.painter();
                        p.line_segment(
                            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                            Stroke::new(1.0_f32, c::NEUTRAL_500),
                        );
                        p.text(
                            Pos2::new(rect.center().x, y + 4.0),
                            Align2::CENTER_TOP,
                            "NodeGate",
                            FontId::proportional(12.0),
                            c::NEUTRAL_700,
                        );
                    }
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(PACK_CARD_W, PACK_CARD_H), Sense::hover());
                    let title = format!("Group {}", g + 1);
                    let note = format!("{total} aircraft");
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{title} · {note}"))
                    });
                    let p = ui.painter();
                    let border = if g == 0 { c::ACCENT } else { c::DIVIDER };
                    p.rect_filled(rect, 0.0, c::BG);
                    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
                    shell::corner_marks(p, rect, c::MARK);
                    let left = rect.left() + 10.0;
                    paint_seat_marker(p, Pos2::new(left + 6.0, rect.top() + 18.0), 12.0, country);
                    p.text(
                        Pos2::new(left + 18.0, rect.top() + 18.0),
                        Align2::LEFT_CENTER,
                        title,
                        FontId::new(13.0, theme::bold_family()),
                        c::TEXT,
                    );
                    p.text(
                        Pos2::new(left, rect.top() + 36.0),
                        Align2::LEFT_CENTER,
                        note,
                        FontId::proportional(12.0),
                        c::NEUTRAL_700,
                    );
                }
            });
            ui.add_space(8.0);
        }
    }

    /// Which groups share a row of `width`. The first row starts with a card;
    /// every later row starts with the link to its first card.
    fn pack_card_rows(groups: usize, width: f32) -> Vec<std::ops::Range<usize>> {
        let unit = PACK_CARD_W + PACK_LINK_W;
        let first = 1 + ((width - PACK_CARD_W).max(0.0) / unit).floor() as usize;
        let rest = ((width / unit).floor() as usize).max(1);
        let mut rows = Vec::new();
        let (mut start, mut cap) = (0, first);
        while start < groups {
            let end = (start + cap).min(groups);
            rows.push(start..end);
            start = end;
            cap = rest;
        }
        rows
    }

    /// Low and high element altitudes of a flight: its AttackArea leads.
    /// A complete 4-ship has a high pair; wingmen stack 25–50 m on their lead
    /// and are not shown.
    fn flight_element_altitudes(seats: &[(f64, bool)]) -> (f64, Option<f64>) {
        if seats.is_empty() {
            return (0.0, None);
        }
        let leads = seats.iter().filter(|s| s.1).map(|s| s.0);
        let low = leads.clone().fold(f64::MAX, f64::min);
        let high = leads.fold(f64::MIN, f64::max);
        (low, (high - low >= 1.0).then_some(high))
    }

    /// "1 800 / 3 800 m" or "5 200 m".
    fn altitude_text(low: f64, high: Option<f64>) -> String {
        match high {
            Some(high) => format!("{} / {} m", group_digits(low), group_digits(high)),
            None => format!("{} m", group_digits(low)),
        }
    }

    /// Role column: leads fly AttackArea, wingmen Cover their lead.
    fn flight_role_text(seats: &[(f64, bool)]) -> String {
        let leads = seats.iter().filter(|s| s.1).count();
        let covers = seats.len() - leads;
        match (leads, covers) {
            (1, 0) => "AttackArea".into(),
            (1, 1) => "AttackArea + Cover".into(),
            (l, 0) => format!("{l} AttackArea"),
            (l, c) => format!("{l} AttackArea · {c} Cover"),
        }
    }

    /// The altitude band with one tick per flight (at its low element), the
    /// range at the ends and flight names below. A name that would touch the
    /// one drawn before it is skipped; its tick still names it on hover.
    fn fighter_altitude_strip(ui: &mut egui::Ui, flights: &[crate::flights::FlightPreview], min: f32, max: f32) {
        let lo = min.min(max) as f64;
        let hi = (min.max(max) as f64).max(lo + 1.0);
        let font = FontId::proportional(12.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::hover());
        let p = ui.painter().clone();
        let lo_g = p.layout_no_wrap(format!("{} m", group_digits(lo)), font.clone(), c::NEUTRAL_700);
        let hi_g = p.layout_no_wrap(format!("{} m", group_digits(hi)), font.clone(), c::NEUTRAL_700);
        let y = rect.top() + 10.0;
        let bar = Rect::from_min_max(
            Pos2::new(rect.left() + lo_g.size().x + 10.0, y - 4.0),
            Pos2::new(rect.right() - hi_g.size().x - 10.0, y + 4.0),
        );
        p.galley(Pos2::new(rect.left(), y - lo_g.size().y / 2.0), lo_g, c::NEUTRAL_700);
        p.galley(Pos2::new(rect.right() - hi_g.size().x, y - hi_g.size().y / 2.0), hi_g, c::NEUTRAL_700);
        p.rect_filled(bar, 0.0, c::ACCENT_200);
        p.rect_stroke(bar, 0.0, Stroke::new(1.0_f32, c::DIVIDER), egui::StrokeKind::Inside);
        let x_of = |alt: f64| bar.left() + ((alt - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * bar.width();

        let ticks: Vec<(f32, String, String)> = flights
            .iter()
            .enumerate()
            .map(|(f, fl)| {
                let (low, high) = Self::flight_element_altitudes(&fl.seats);
                let name = crate::aircraft::plane_display_name(f, 0);
                let hover = format!("{name} · {} · {}", fl.type_label, Self::altitude_text(low, high));
                (x_of(low), name, hover)
            })
            .collect();
        let galleys: Vec<_> = ticks
            .iter()
            .map(|(_, name, _)| p.layout_no_wrap(name.clone(), font.clone(), c::NEUTRAL_700))
            .collect();
        let lefts: Vec<f32> = ticks
            .iter()
            .zip(&galleys)
            .map(|((x, _, _), g)| (x - g.size().x / 2.0).clamp(rect.left(), (rect.right() - g.size().x).max(rect.left())))
            .collect();
        let widths: Vec<f32> = galleys.iter().map(|g| g.size().x).collect();
        let shown = strip_labels_shown(&lefts, &widths, 6.0);
        for (f, ((x, name, _), galley)) in ticks.iter().zip(galleys).enumerate() {
            p.line_segment(
                [Pos2::new(*x, bar.top() - 3.0), Pos2::new(*x, bar.bottom() + 3.0)],
                Stroke::new(2.0_f32, c::ACCENT_800),
            );
            if shown[f] {
                p.galley(Pos2::new(lefts[f], bar.bottom() + 6.0), galley, c::NEUTRAL_700);
            }
            // Hover names every flight on this tick, so a skipped label is still identifiable.
            let hover: Vec<&str> = ticks
                .iter()
                .filter(|(other, _, _)| (other - x).abs() < 3.0)
                .map(|(_, _, h)| h.as_str())
                .collect();
            let hit = Rect::from_center_size(Pos2::new(*x, bar.center().y), Vec2::new(9.0, 22.0));
            let resp = ui.interact(hit, ui.id().with(("altitude_tick", f)), Sense::hover());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Altitude tick {name}")));
            resp.on_hover_text(hover.join("\n"));
        }
    }

    fn fighter_settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().slider_width = 110.0;
        shell::section_title(ui, "Pack", None);
        labeled_slider(ui, "Linked groups", &mut self.linked_groups, 1..=10);
        labeled_slider(ui, "Flights", &mut self.flight_count, 1..=10);
        labeled_slider(ui, "Max in a flight", &mut self.max_in_flight, 1..=8);
        if shell::hint(ui, "Each flight is one randomizer slot.", true) {
            self.open_help(HelpTopic::Fighter);
        }
        ui.add_space(4.0);
        ui.separator();

        shell::section_title(ui, "Country", None);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            paint_seat_marker(ui.painter(), r.center(), 12.0, self.country);
            let selected = COUNTRIES
                .iter()
                .find(|(id, _)| *id == self.country)
                .map(|(_, label)| *label)
                .unwrap_or("501  USSR");
            egui::ComboBox::from_id_salt("country")
                .selected_text(selected)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (id, label) in COUNTRIES {
                        ui.selectable_value(&mut self.country, *id, *label);
                    }
                });
        });
        shell::hint(
            ui,
            if self.country / 100 == 6 {
                "Zone In / Zone Out trigger on DPRK [1]."
            } else {
                "Zone In / Zone Out trigger on NATO [2]."
            },
            false,
        );
        ui.add_space(4.0);
        ui.separator();

        let summary = format!("{:.0}–{:.0} m", self.altitude_min, self.altitude_max);
        let mut help = false;
        shell::settings_section(ui, "fighter_alt", "Altitude range", &summary, true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Min");
                ui.add(egui::DragValue::new(&mut self.altitude_min).range(100.0..=9000.0).speed(25.0).suffix(" m"));
                ui.label("Max");
                ui.add(egui::DragValue::new(&mut self.altitude_max).range(100.0..=9000.0).speed(25.0).suffix(" m"));
            });
            help = shell::hint(ui, "Complete 4-ships split 2 low / 2 high.", true);
        });
        if self.altitude_min > self.altitude_max {
            std::mem::swap(&mut self.altitude_min, &mut self.altitude_max);
        }
        if help {
            self.open_help(HelpTopic::Fighter);
        }
        let summary = format!(
            "Cooldown {:.0} s · Delete {:.0} s",
            self.cooldown, self.delete_orders
        );
        shell::settings_section(ui, "fighter_timers", "Timers", &summary, false, |ui| {
            egui::Grid::new("fighter_timer_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Cooldown");
                ui.add(egui::DragValue::new(&mut self.cooldown).range(0.0..=1800.0).speed(1.0).suffix(" s"));
                ui.end_row();
                ui.label("Delete orders");
                ui.add(egui::DragValue::new(&mut self.delete_orders).range(0.0..=600.0).speed(1.0).suffix(" s"));
                ui.end_row();
            });
            shell::hint(
                ui,
                "Reinforcement timer is off, so a spawn cannot start another flight during cleanup.",
                false,
            );
        });
        let custom = self
            .custom_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned);
        let summary = custom.clone().unwrap_or_else(|| "Built-in · experimental".into());
        shell::settings_section(ui, "fighter_custom", "Custom pack template", &summary, false, |ui| {
            shell::hint(ui, "Experimental. Leave unloaded to use the built-in logic.", false);
            ui.horizontal(|ui| {
                if ui.button("Load…").clicked() {
                    self.pick_template();
                }
                if self.custom_path.is_some() && ui.button("Use built-in").clicked() {
                    self.custom_path = None;
                    self.status = Status::Info("Using built-in logic.".into());
                }
            });
            ui.label(RichText::new(custom.as_deref().unwrap_or("Built-in")).italics().color(c::NEUTRAL_700));
        });
    }

    /// Header Reset on Fighter Pack: back to the startup settings.
    fn reset_fighter_pack(&mut self) {
        let d = Self::default();
        self.custom_path = d.custom_path;
        self.linked_groups = d.linked_groups;
        self.flight_count = d.flight_count;
        self.max_in_flight = d.max_in_flight;
        self.type_enabled = d.type_enabled;
        self.type_skill = d.type_skill;
        self.country = d.country;
        self.cooldown = d.cooldown;
        self.reinforcement = d.reinforcement;
        self.delete_orders = d.delete_orders;
        self.altitude_min = d.altitude_min;
        self.altitude_max = d.altitude_max;
        self.status = Status::Info("Fighter Pack reset to the default settings.".into());
    }

    // ── Exclusive Activation (README §5.4) ─────────────────────────────────

    fn bomber_page(&mut self, ctx: &egui::Context) {
        if let Some(i) = self.bomber_selected {
            if i >= self.bomber_slots.len() {
                self.bomber_selected = self.bomber_slots.len().checked_sub(1);
            }
        } else if !self.bomber_slots.is_empty() {
            self.bomber_selected = Some(0);
        }
        side_panel(ctx, "bomber_left", true, 290.0, |ui| self.bomber_plans_panel(ui));
        side_panel(ctx, "bomber_right", false, 290.0, |ui| self.bomber_settings_panel(ui));
        center_panel(ctx, "bomber_center", |ui| self.bomber_center(ui));
    }

    fn bomber_plans_panel(&mut self, ui: &mut egui::Ui) {
        let n = self.bomber_slots.len();
        shell::section_title(ui, &format!("Plans · {n}"), Some("one active at a time"));
        if n == 0 {
            shell::hint(ui, "Add templates… (Ctrl O) to list plans here.", false);
            return;
        }
        let mut clicked = None;
        for i in 0..n {
            let slot = &self.bomber_slots[i];
            let file = slot.path.file_name().and_then(|f| f.to_str()).unwrap_or("file").to_string();
            let title = format!("{} · {}", i + 1, slot.info.name);
            let meta = format!("{file} · {} units", slot.info.unit_count);
            let tags = plan_tags(slot);
            let selected = self.bomber_selected == Some(i);
            let resp = ui
                .scope_builder(
                    egui::UiBuilder::new().id_salt(("bomber_card", i)).sense(Sense::click()),
                    |ui| {
                        let hovered = ui.response().hovered();
                        plan_card(ui, selected, hovered, |ui| {
                            ui.horizontal(|ui| {
                                // Tags first (right to left), so a long name truncates instead of pushing them out.
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    for (text, accent) in tags.iter().rev() {
                                        shell::tag(ui, text, *accent);
                                    }
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(title).font(FontId::new(13.0, theme::bold_family())),
                                            )
                                            .truncate(),
                                        );
                                    });
                                });
                            });
                            ui.label(RichText::new(meta).small().color(c::NEUTRAL_700));
                        });
                    },
                )
                .response;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, self.bomber_selected == Some(i), format!("Plan card {}", i + 1)));
            if resp.clicked() {
                clicked = Some(i);
            }
            ui.add_space(4.0);
        }
        if clicked.is_some() {
            self.bomber_selected = clicked;
        }
    }

    fn bomber_center(&mut self, ui: &mut egui::Ui) {
        if self.bomber_slots.is_empty() {
            if empty_state(ui, "Add templates… to begin.", "Add templates…") {
                self.add_bomber_template();
            }
            return;
        }
        let i = self.bomber_selected.unwrap_or(0).min(self.bomber_slots.len() - 1);
        let mut remove = false;
        let mut duplicate = false;
        {
            let slot = &self.bomber_slots[i];
            let file = slot.path.file_name().and_then(|f| f.to_str()).unwrap_or("file").to_string();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Plan {}", i + 1)).small().color(c::NEUTRAL_700));
                    ui.label(RichText::new(&slot.info.name).font(FontId::new(24.0, theme::heading_family())));
                    ui.label(
                        RichText::new(format!("{file} · {} units", slot.info.unit_count))
                            .small()
                            .color(c::NEUTRAL_700),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    remove = ui.button("Remove").clicked();
                    duplicate = ui.button("Add again").on_hover_text("Add this template again as another plan").clicked();
                });
            });
            show_missing_locale_hint(ui, &slot.path);
        }
        ui.add_space(8.0);

        let slot = &mut self.bomber_slots[i];
        let zones = slot.info.checkzones.clone();
        let suggested_triggers = slot.info.suggested_triggers.clone();
        let timers = slot.info.timers.clone();
        let suggested_end = slot.info.suggested_completion;
        shell::blueprint(ui, c::DIVIDER, |ui| {
            shell::section_title(ui, "Start checkzones", Some("opens this plan, closes the others"));
            if zones.len() > 1 {
                shell::warning(ui, "Several checkzones found. Pick the ones this plan enables and disables.");
            }
            for zone in &zones {
                let mut on = slot.selected_triggers.contains(&zone.index);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, &zone.name).changed() {
                        if on {
                            if !slot.selected_triggers.contains(&zone.index) {
                                slot.selected_triggers.push(zone.index);
                            }
                        } else {
                            slot.selected_triggers.retain(|id| *id != zone.index);
                        }
                    }
                    if suggested_triggers.contains(&zone.index) {
                        ui.label(RichText::new("(suggested)").small().color(c::NEUTRAL_700))
                            .on_hover_text(format!("Picked because it is named {SUGGESTED_TRIGGER_NAMES}."));
                    }
                });
            }
            if slot.selected_triggers.is_empty() {
                shell::warning(ui, "Select at least one checkzone.");
            }
            for id in &slot.selected_triggers {
                if let Some(msg) = slot.info.trigger_warnings.get(id) {
                    shell::warning(ui, msg);
                }
            }
        });
        ui.add_space(10.0);
        let end_missing = slot.selected_completion.is_none();
        shell::blueprint(ui, if end_missing { c::WARN } else { c::DIVIDER }, |ui| {
            shell::section_title(ui, "End timer", Some("finishes the plan"));
            if end_missing {
                shell::warning(
                    ui,
                    "No end timer was detected. Select the MCU_Timer that finishes the plan. It should target a Deactivate and/or Delete MCU that lists the units.",
                );
            }
            let selected_label = slot
                .selected_completion
                .and_then(|id| timers.iter().find(|t| t.index == id).map(|t| t.name.clone()))
                .unwrap_or_else(|| "Select end timer…".into());
            egui::ComboBox::from_id_salt(format!("bomber-end-{i}"))
                .selected_text(selected_label)
                .width(280.0)
                .show_ui(ui, |ui| {
                    for timer in &timers {
                        let text = if suggested_end == Some(timer.index) {
                            format!("{}  (suggested)", timer.name)
                        } else {
                            timer.name.clone()
                        };
                        ui.selectable_value(&mut slot.selected_completion, Some(timer.index), text);
                    }
                })
                .response
                .on_hover_text(format!("A timer named {SUGGESTED_END_NAMES} is picked automatically."));
            if let Some(id) = slot.selected_completion {
                if let Some(msg) = slot.info.cleanup_warnings.get(&id) {
                    shell::warning(ui, msg);
                }
            }
        });
        ui.add_space(12.0);

        shell::section_title(ui, "Sequence", None);
        let mut pick = None;
        for (j, s) in self.bomber_slots.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
            let sel = j == i;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, sel, format!("Sequence {}", j + 1)));
            let (fill, stroke) = if sel {
                (c::ACCENT_100, Stroke::new(1.5_f32, c::ACCENT))
            } else {
                (c::NEUTRAL_100, Stroke::new(1.0_f32, c::DIVIDER))
            };
            let p = ui.painter();
            p.rect_filled(rect, 0.0, fill);
            p.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
            p.text(
                rect.left_center() + Vec2::new(10.0, 0.0),
                Align2::LEFT_CENTER,
                format!("Plan {}", j + 1),
                FontId::new(13.0, if sel { theme::bold_family() } else { FontFamily::Proportional }),
                c::TEXT,
            );
            if resp.on_hover_text(format!("{} · {}", j + 1, s.info.name)).clicked() {
                pick = Some(j);
            }
            ui.add_space(4.0);
        }
        shell::hint(ui, "A start zone opens one plan and closes the rest until its end timer fires.", false);
        if let Some(j) = pick {
            self.bomber_selected = Some(j);
        }
        if remove {
            self.bomber_undo
                .record(
                    format!("Removed plan {}", self.bomber_slots[i].info.name),
                    (self.bomber_slots.clone(), self.bomber_selected),
                );
            self.bomber_slots.remove(i);
            self.bomber_selected = if self.bomber_slots.is_empty() { None } else { Some(i.min(self.bomber_slots.len() - 1)) };
        } else if duplicate {
            let s = &self.bomber_slots[i];
            let copy = BomberSlot {
                path: s.path.clone(),
                root: s.root.clone(),
                info: s.info.clone(),
                selected_triggers: s.selected_triggers.clone(),
                selected_completion: s.selected_completion,
            };
            self.bomber_slots.push(copy);
        }
    }

    fn bomber_settings_panel(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "Output", None);
        ui.checkbox(&mut self.bomber_keep_positions, "Export in place");
        shell::hint(
            ui,
            "Off: new templates park on a 10 km grid from 40000, 40000. On: groups stay where they are.",
            false,
        );
        ui.add_space(4.0);
        ui.separator();
        shell::section_title(ui, "Naming", None);
        if shell::hint(
            ui,
            "Name start zones and the end timer as listed in Help so they are found automatically.",
            true,
        ) {
            self.open_help(HelpTopic::Exclusive);
        }
        ui.add_space(4.0);
        ui.separator();
        shell::section_title(ui, "Use it for", None);
        shell::hint(
            ui,
            "Groups with heavy logic where only one should run at a time, such as preplanned bomber flights.",
            false,
        );
    }

    // ── Airfield (README §5.5) ─────────────────────────────────────────────

    fn airfield_page(&mut self, ctx: &egui::Context) {
        side_panel(ctx, "airfield_left", true, 290.0, |ui| self.airfield_left_panel(ui));
        side_panel(ctx, "airfield_right", false, 270.0, |ui| self.airfield_right_panel(ui));
        center_panel(ctx, "airfield_center", |ui| self.airfield_center(ui));
    }

    fn airfield_left_panel(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "Get the file", None);
        numbered_step(ui, 1, "In game, open a Freeflight mission and take off from the airfield.");
        numbered_step(
            ui,
            2,
            "Open /missions/_gen.mission in the mission editor. Select the field, then File › Save Selection to File.",
        );
        numbered_step(ui, 3, "Load that file here.");
        if shell::link(ui, "Help ›").clicked() {
            self.open_help(HelpTopic::Airfield);
        }
        ui.add_space(6.0);
        ui.separator();
        shell::section_title(ui, "Friendly plane coalition", None);
        shell::segmented(ui, &mut self.airfield_western, &[(true, "NATO [2]"), (false, "DPRK [1]")]);
        shell::hint(ui, "USA airfields use NATO.", false);
        ui.add_space(6.0);
        ui.separator();
        self.harvest_section(ui);
    }

    fn airfield_center(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "What Generate will change", None);
        ui.label("Strips the player and SP logic, then retargets the checkzones that were linked to the player.");
        ui.add_space(10.0);
        let Some(info) = &self.airfield_info else {
            if empty_state(ui, "Load an airfield group exported from _gen.mission.", "Load airfield…") {
                self.load_airfield();
            }
            self.harvest_log_panel(ui);
            return;
        };
        let side = if self.airfield_western { "NATO [2]" } else { "DPRK [1]" };
        ui.columns(2, |cols| {
            shell::blueprint(&mut cols[0], c::DIVIDER, |ui| {
                shell::section_title(ui, "Removed", None);
                if info.player_planes.is_empty() {
                    shell::warning(ui, "No player aircraft found; this file may already be cleaned.");
                } else {
                    for p in &info.player_planes {
                        fact_row(ui, &format!("Player · {}", p.name), &country_name(p.country));
                    }
                }
                if info.has_autoremove {
                    fact_row(ui, "AutoRemove subgroup", "");
                }
                fact_row(ui, "Player / SP graph objects", &info.strip_count.to_string());
            });
            shell::blueprint(&mut cols[1], c::ACCENT, |ui| {
                let n = info.unlink_zones.len();
                let note = format!("{n} checkzone{}", if n == 1 { "" } else { "s" });
                shell::section_title(ui, &format!("Relinked to {side}"), Some(&note));
                if info.unlink_zones.is_empty() {
                    ui.label("No checkzones are linked to the player.");
                }
                for name in &info.unlink_zones {
                    fact_row(ui, name, "");
                }
            });
        });
        ui.add_space(10.0);
        shell::blueprint(ui, c::DIVIDER, |ui| {
            shell::section_title(ui, "Kept", None);
            ui.columns(4, |cols| {
                for (col, (n, label)) in cols.iter_mut().zip([
                    (info.vehicle_count, "vehicles / ships"),
                    (info.ai_plane_count, "AI aircraft"),
                    (info.block_count, "blocks"),
                    (info.checkzone_count, "checkzones"),
                ]) {
                    col.label(RichText::new(n.to_string()).font(FontId::new(24.0, theme::heading_family())));
                    col.label(RichText::new(label).small().color(c::NEUTRAL_700));
                }
            });
        });
        ui.add_space(10.0);
        shell::warning(ui, "Still required after export: add planes to fly and set the starting location.");
        self.harvest_log_panel(ui);
    }

    fn airfield_right_panel(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "Airfield", None);
        let Some(info) = &self.airfield_info else {
            shell::hint(ui, "Load an airfield to see it here.", false);
            return;
        };
        egui::Grid::new("airfield_facts").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("Name").color(c::NEUTRAL_700));
            ui.label(RichText::new(&info.name).font(FontId::new(13.0, theme::bold_family())));
            ui.end_row();
            ui.label(RichText::new("Layout").color(c::NEUTRAL_700));
            ui.label(if info.in_group { "Inside a Group" } else { "Blocks at the root" });
            ui.end_row();
            if let Some((x, z)) = info.origin_xz {
                ui.label(RichText::new("Origin").color(c::NEUTRAL_700));
                ui.label(RichText::new(format!("{}, {}", group_digits(x), group_digits(z))).monospace());
                ui.end_row();
            }
        });
    }

    /// Map tab (README §5.6): tool palette, the map with a front-date strip,
    /// and a right dock with Period / Forces / References.
    fn map_page(&mut self, ctx: &egui::Context) {
        self.ensure_map_assets(ctx);
        egui::SidePanel::left("map_tools")
            .exact_width(shell::TOOL_PALETTE_W)
            .resizable(false)
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(10, 8)))
            .show(ctx, |ui| self.map_tool_palette(ui));
        egui::SidePanel::right("map_dock")
            .exact_width(304.0)
            .resizable(false)
            .show(ctx, |ui| self.map_dock_panel(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("map_date")
                    .exact_height(64.0)
                    .show_inside(ui, |ui| self.map_date_strip(ui));
                self.handle_map_timeline_keys(ui);
                self.draw_korea_map(ui);
            });
    }

    /// Esc on Map: drop a half-drawn mark (salient, arrow, or the points of a
    /// front drag still under way) or WP pick, and return to Select. A
    /// finished front stays.
    fn cancel_map_tool(&mut self) {
        self.cancel_map_stroke();
        self.wp_selected = None;
        self.wp_drag = None;
        // The rest of a cancelled drag must not move the AO from a stale origin.
        self.map_drag_uv = None;
        self.map_drawing_mode = MapDrawingMode::None;
    }

    /// Drop whatever is half drawn. Returns true if there was something.
    fn cancel_map_stroke(&mut self) -> bool {
        let had = self.map_stroke_in_progress();
        self.map_void_drag = true;
        self.current_salient.clear();
        self.attack_drag = None;
        if let Some(prev_len) = self.front_stroke.take() {
            self.custom_front_xz.truncate(prev_len);
            self.map_undo.rebase();
        }
        had
    }

    fn map_stroke_in_progress(&self) -> bool {
        !self.current_salient.is_empty() || self.attack_drag.is_some() || self.front_stroke.is_some()
    }

    /// Picking a tool (palette, keys 1–6, the Forces objective buttons).
    /// Switching to another tool drops a half-drawn mark, as Esc does.
    fn pick_map_tool(&mut self, mode: MapDrawingMode) {
        if mode != self.map_drawing_mode {
            self.cancel_map_stroke();
        }
        self.map_drawing_mode = mode;
    }

    /// Anything Ctrl Z can take back as a drawing: finished marks (front
    /// strokes, salients, arrows) or one still being drawn.
    fn map_has_marks(&self) -> bool {
        !self.drawn_marks.is_empty()
            || !self.salients.is_empty()
            || !self.attack_arrows.is_empty()
            || self.map_stroke_in_progress()
    }

    /// A drawing was finished: it goes on the mark stack and becomes the
    /// last action (Ctrl Z undoes it before any earlier Clear).
    fn note_map_drawing(&mut self, mark: DrawnMark) {
        self.drawn_marks.push(mark);
        self.clear_redo_stack();
        self.map_last = MapAction::Drawing;
        self.map_undo.rebase();
    }

    /// Front strokes lose their undo when the front is replaced (date slider,
    /// timeline snap): their stored lengths no longer describe it.
    fn forget_front_marks(&mut self) {
        self.front_stroke = None;
        self.drawn_marks.retain(|m| !m.is_front());
        self.redo_marks.retain(|m| !m.is_front());
        self.redo_fronts.clear();
    }

    /// "Tool: Salient · right-click to finish · Esc to cancel" while a tool is active.
    fn map_tool_status(&self) -> Option<String> {
        if let Some((hit, wi)) = self.wp_selected {
            return Some(format!(
                "Placing unit {} WP{} · click the map · right-click or Esc to cancel",
                hit.spot_i() + 1,
                wi + 1
            ));
        }
        MAP_TOOLS
            .iter()
            .find(|t| t.0 == self.map_drawing_mode && t.0 != MapDrawingMode::None)
            .map(|t| format!("Tool: {} · {}", t.2, t.3))
    }

    /// Mockup 2f: tools 1–6 with painted icons and their key in the corner,
    /// a divider between the drawing tools and the objective tools, then
    /// Undo / Redo at the bottom.
    fn map_tool_palette(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (i, (mode, icon, name, _)) in MAP_TOOLS.iter().enumerate() {
            if i == 4 {
                let (r, _) = ui.allocate_exact_size(Vec2::new(shell::TOOL_SIZE, 9.0), Sense::hover());
                ui.painter().hline(
                    (r.center().x - 14.0)..=(r.center().x + 14.0),
                    r.center().y,
                    Stroke::new(1.0_f32, c::DIVIDER),
                );
            }
            let tint = match mode {
                MapDrawingMode::PlaceEastObjective => Some(c::DPRK),
                MapDrawingMode::PlaceNatoObjective => Some(c::NATO),
                _ => None,
            };
            let key = (i + 1).to_string();
            let active = self.map_drawing_mode == *mode;
            if shell::tool_icon_button(ui, *icon, tint, name, &key, true, active).clicked() {
                self.pick_map_tool(*mode);
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let can_redo = !self.redo_marks.is_empty();
            let undo_label = self.map_undo_label().map(str::to_owned);
            // Disabled tools fade to 40 % (README §5.6).
            let mut redo = false;
            let mut undo = false;
            ui.scope(|ui| {
                if !can_redo {
                    ui.disable();
                    ui.set_opacity(0.4);
                }
                redo = shell::tool_icon_button(ui, shell::ToolIcon::Redo, None, "Redo drawing", "Ctrl Y", false, false)
                    .clicked();
            });
            ui.scope(|ui| {
                if undo_label.is_none() {
                    ui.disable();
                    ui.set_opacity(0.4);
                }
                let tip = undo_label.as_deref().map_or("Undo".to_string(), |l| format!("Undo: {l}"));
                undo = shell::tool_icon_button(ui, shell::ToolIcon::Undo, None, &tip, "Ctrl Z", false, false)
                    .clicked();
            });
            if redo {
                self.redo_last_mark();
            }
            if undo {
                self.undo_current_tab();
            }
        });
    }

    fn map_dock_panel(&mut self, ui: &mut egui::Ui) {
        shell::dock_tabs(ui, &mut self.map_dock, &MapDock::tabs(self.map_refs.len()));
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("map_dock_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().slider_width = 110.0;
                match self.map_dock {
                    MapDock::Period => self.map_period_tab(ui),
                    MapDock::Forces => self.map_forces_tab(ui),
                    MapDock::References => self.map_references_tab(ui),
                    MapDock::Terrain => self.map_terrain_tab(ui),
                }
            });
    }

    fn map_period_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("front_year")
                .selected_text(self.front_year.to_string())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for y in YEARS {
                        if ui.selectable_label(self.front_year == y, y.to_string()).clicked() {
                            self.front_year = y;
                            self.front_focus = None;
                            self.snap_timeline(timeline_index(self.front_year, self.front_season));
                        }
                    }
                })
                .response
                .on_hover_text("Year");
            egui::ComboBox::from_id_salt("front_season")
                .selected_text(self.front_season.label())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for s in Season::ALL {
                        if ui.selectable_label(self.front_season == s, s.label()).clicked() {
                            self.front_season = s;
                            self.front_focus = None;
                            self.snap_timeline(timeline_index(self.front_year, self.front_season));
                        }
                    }
                })
                .response
                .on_hover_text("Season");
        });
        ui.add_space(8.0);
        let mark = self.current_mark();
        shell::blueprint(ui, c::DIVIDER, |ui| {
            ui.label(RichText::new(mark.title).font(FontId::new(16.0, theme::heading_family())));
            ui.label(mark.note);
            ui.label(
                RichText::new(mark.editor_hint())
                    .font(FontId::new(13.0, theme::bold_family()))
                    .color(c::ACCENT_700),
            );
        });
        ui.add_space(10.0);

        ui.label(RichText::new("Battle focus").font(FontId::new(13.0, theme::bold_family())));
        let period_battles = battles_in_period(self.front_year, self.front_season);
        let selected_battle = self.front_focus.and_then(|id| BATTLES.iter().find(|b| b.id == id));
        let focus_text = selected_battle.map_or("Entire front (this period)", |b| b.name);
        egui::ComboBox::from_id_salt("front_battle")
            .selected_text(focus_text)
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.front_focus.is_none(), "Entire front (this period)")
                    .clicked()
                {
                    self.front_focus = None;
                }
                ui.separator();
                ui.label(RichText::new("This period").small().color(c::NEUTRAL_700));
                for b in &period_battles {
                    if ui.selectable_label(self.front_focus == Some(b.id), b.name).clicked() {
                        self.focus_battle(b);
                    }
                }
                ui.separator();
                ui.label(RichText::new("Jump to any battle").small().color(c::NEUTRAL_700));
                for b in BATTLES {
                    let label = format!("{}  ({} {})", b.name, b.season.label(), b.year);
                    if ui.selectable_label(self.front_focus == Some(b.id), label).clicked() {
                        self.focus_battle(b);
                    }
                }
            });
        if let Some(b) = selected_battle {
            ui.label(RichText::new(b.note).small().color(c::NEUTRAL_700));
        }
        ui.add_space(10.0);

        ui.label(RichText::new("Suggested aircraft").font(FontId::new(13.0, theme::bold_family())));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for a in suggested_aircraft(self.front_year, self.front_season) {
                shell::tag(ui, a.label, false);
            }
        });
        ui.add_space(10.0);
        ui.separator();

        shell::section_title(ui, "Drawn marks", Some("tools 2–4"));
        ui.horizontal_wrapped(|ui| {
            // add_enabled keeps each button one unit, so the row wraps whole
            // buttons instead of wrapping "Clear arrows" onto two lines.
            let button = |text: &str| egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend);
            let has_custom = !self.custom_front_xz.is_empty() || self.map_has_marks();
            if ui
                .add_enabled(has_custom, button("Clear lines"))
                .on_hover_text("Clear the drawn front, salients and arrows. Ctrl Z brings them back.")
                .clicked()
            {
                self.clear_map_lines(true, true, true);
            }
            let has_salients = !self.salients.is_empty() || !self.current_salient.is_empty();
            if ui
                .add_enabled(has_salients, button("Clear salients"))
                .on_hover_text("Ctrl Z brings them back.")
                .clicked()
            {
                self.clear_map_lines(false, true, false);
            }
            let has_arrows = !self.attack_arrows.is_empty() || self.attack_drag.is_some();
            if ui
                .add_enabled(has_arrows, button("Clear arrows"))
                .on_hover_text("Ctrl Z brings them back.")
                .clicked()
            {
                self.clear_map_lines(false, false, true);
            }
        });
        ui.add_space(6.0);
        if shell::hint(
            ui,
            "Drag a box on the map to set the AO; the period front and areas of influence are clipped to it.",
            true,
        ) {
            self.open_help(HelpTopic::Front);
        }
    }

    fn map_forces_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        shell::section_title(ui, "Fighters", Some("from Fighter Pack"));
        ui.horizontal(|ui| {
            if ui.button("Place DPRK").on_hover_text("Place DPRK fighters in their coalition zone").clicked() {
                self.place_map_fighters(true);
            }
            if ui.button("Place NATO").on_hover_text("Place NATO fighters in their coalition zone").clicked() {
                self.place_map_fighters(false);
            }
        });
        labeled_slider(ui, "Groups at once", &mut self.fighter_waves, 1..=6);
        ui.checkbox(&mut self.fighter_fill, format!("Fill AO (up to {MAX_PACKS} at once)"))
            .on_hover_text("Fill the AO at Zone In spacing");
        ui.horizontal(|ui| {
            if let Some(layout) = &self.map_fighters {
                let n = layout.spots.len();
                let packs = layout
                    .spots
                    .iter()
                    .map(|s| s.pack)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                let side = shell::Side::from_eastern(layout.eastern);
                shell::side_marker(ui, side, 12.0);
                ui.label(format!("{}: {n} groups in {packs} packs", side.label()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let has_fighters = self.map_fighters.as_ref().is_some_and(|l| !l.spots.is_empty());
                ui.add_enabled_ui(has_fighters, |ui| {
                    if ui.button("Clear").on_hover_text("Clear placed fighters. Ctrl Z brings them back.").clicked() {
                        let n = self.map_fighters.as_ref().map_or(0, |l| l.spots.len());
                        self.record_map_undo(format!("Cleared {n} fighter groups"));
                        self.map_fighters = None;
                        self.map_imported_fighters.clear();
                        self.fighter_drag = None;
                    }
                });
            });
        });
        ui.add_space(6.0);
        ui.separator();

        shell::section_title(ui, "Objectives", Some("one click each · Shift for more"));
        ui.horizontal(|ui| {
            let e_active = self.map_drawing_mode == MapDrawingMode::PlaceEastObjective;
            let n_active = self.map_drawing_mode == MapDrawingMode::PlaceNatoObjective;
            if ui
                .selectable_label(e_active, format!("DPRK · {}", self.east_objectives.len()))
                .on_hover_text("DPRK objective tool (5): click the map to place one")
                .clicked()
            {
                self.pick_map_tool(MapDrawingMode::PlaceEastObjective);
            }
            if ui
                .selectable_label(n_active, format!("NATO · {}", self.nato_objectives.len()))
                .on_hover_text("NATO objective tool (6): click the map to place one")
                .clicked()
            {
                self.pick_map_tool(MapDrawingMode::PlaceNatoObjective);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let has_objs = !self.east_objectives.is_empty() || !self.nato_objectives.is_empty();
                ui.add_enabled_ui(has_objs, |ui| {
                    if ui.button("Clear").on_hover_text("Clear all objectives. Ctrl Z brings them back.").clicked() {
                        let n = self.east_objectives.len() + self.nato_objectives.len();
                        self.record_map_undo(format!("Cleared {n} objectives"));
                        self.east_objectives.clear();
                        self.nato_objectives.clear();
                        self.objective_drag = None;
                        self.reaim_map_ground();
                    }
                });
            });
        });
        shell::hint(ui, "Preview markers that aim placed units. Right-click one to remove it. Not exported.", false);
        ui.add_space(6.0);
        ui.separator();

        shell::section_title(ui, "Units", Some("from Army Generator"));
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.recon_keep_positions, |ui| {
                if ui
                    .button("Place DPRK")
                    .on_hover_text("Place Army Generator units along the front as DPRK")
                    .on_disabled_hover_text("Keep loaded positions is on in Army Generator.")
                    .clicked()
                {
                    self.place_map_units(true);
                }
                if ui
                    .button("Place NATO")
                    .on_hover_text("Place Army Generator units along the front as NATO")
                    .on_disabled_hover_text("Keep loaded positions is on in Army Generator.")
                    .clicked()
                {
                    self.place_map_units(false);
                }
            });
        });
        if self.recon_keep_positions {
            shell::hint(ui, "Place is off: Keep loaded positions is on in Army Generator.", false);
        }
        ui.horizontal(|ui| {
            if ui
                .button("Load DPRK…")
                .on_hover_text("Load a .Group as the DPRK army. Reposition along the front, or keep authored positions.")
                .clicked()
            {
                self.load_map_armies(true);
            }
            if ui
                .button("Load NATO…")
                .on_hover_text("Load a .Group as the NATO army. Reposition along the front, or keep authored positions.")
                .clicked()
            {
                self.load_map_armies(false);
            }
        });
        let army_count = |eastern: bool| -> usize {
            self.map_armies
                .iter()
                .filter(|a| a.eastern == eastern)
                .map(|a| {
                    a.ground.as_ref().map_or(0, |g| g.spots.len()) + a.ships.as_ref().map_or(0, |s| s.spots.len())
                })
                .sum()
        };
        let e_n = self.map_ground_east.as_ref().map_or(0, |l| l.spots.len()) + army_count(true);
        let n_n = self.map_ground_nato.as_ref().map_or(0, |l| l.spots.len()) + army_count(false);
        let ships = self.map_ships.as_ref().map_or(0, |l| l.spots.len());
        ui.horizontal(|ui| {
            let mut parts = Vec::new();
            if e_n > 0 {
                parts.push(format!("DPRK {e_n}"));
            }
            if n_n > 0 {
                parts.push(format!("NATO {n_n}"));
            }
            if ships > 0 {
                parts.push(format!("Ships {ships}"));
            }
            if !parts.is_empty() {
                ui.label(parts.join(" · "));
            } else if self.recon_keep_positions {
                shell::warning(ui, "Keep loaded positions is on");
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let has_units = self.map_ships.is_some()
                    || self.map_ground_east.is_some()
                    || self.map_ground_nato.is_some()
                    || !self.map_armies.is_empty();
                ui.add_enabled_ui(has_units, |ui| {
                    if ui.button("Clear").on_hover_text("Clear placed and loaded units. Ctrl Z brings them back.").clicked() {
                        self.record_map_undo(format!("Cleared {} units", e_n + n_n + ships));
                        self.map_ships = None;
                        self.map_ground_east = None;
                        self.map_ground_nato = None;
                        self.map_armies.clear();
                        self.ship_drag = None;
                        self.ship_heading_drag = None;
                        self.ground_drag = None;
                        self.ground_heading_drag = None;
                        self.wp_drag = None;
                        self.wp_selected = None;
                    }
                });
            });
        });
        if !self.map_armies.is_empty() {
            ui.add_space(4.0);
            let mut remove_at: Option<usize> = None;
            let mut toggle_at: Option<usize> = None;
            for (i, slot) in self.map_armies.iter().enumerate() {
                let name = slot.path.file_stem().and_then(|s| s.to_str()).unwrap_or("group");
                let side = shell::Side::from_eastern(slot.eastern);
                shell::card(ui, false, |ui| {
                    ui.horizontal(|ui| {
                        shell::side_marker(ui, side, 12.0);
                        ui.add(egui::Label::new(RichText::new(name).font(FontId::new(13.0, theme::bold_family()))).truncate());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Remove").clicked() {
                                remove_at = Some(i);
                            }
                        });
                    });
                    ui.label(RichText::new(army_mix_label(&slot.copies)).small().color(c::NEUTRAL_700));
                    let mut repo = slot.reposition;
                    if ui
                        .checkbox(&mut repo, "Reposition")
                        .on_hover_text("Park along the front using detected unit types. Off keeps the group's authored X/Z.")
                        .changed()
                    {
                        toggle_at = Some(i);
                    }
                });
                ui.add_space(4.0);
            }
            if let Some(i) = toggle_at {
                self.map_armies[i].reposition = !self.map_armies[i].reposition;
                self.refresh_army_slot(i);
            }
            if let Some(i) = remove_at {
                let name = self.map_armies[i].path.file_stem().and_then(|s| s.to_str()).unwrap_or("army").to_owned();
                self.record_map_undo(format!("Removed {name}"));
                self.map_armies.remove(i);
                self.ship_drag = None;
                self.ship_heading_drag = None;
                self.ground_drag = None;
                self.ground_heading_drag = None;
                self.wp_drag = None;
                self.wp_selected = None;
            }
        }
    }

    fn map_references_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        if ui.button("Add reference groups…").clicked() {
            self.add_map_refs();
        }
        shell::hint(
            ui,
            "Airfields, blocks and entities are stamped at their saved spots, trimmed to the AO plus 10 km. Landscape MCU_Waypoint marks show as nested dots but are not exported.",
            false,
        );
        ui.add_space(4.0);
        let mut remove_at: Option<usize> = None;
        for (i, g) in self.map_refs.iter().enumerate() {
            let label = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or("group");
            let xz = g
                .entity
                .first_xz()
                .map(|(x, z)| format!("X {x:.0}  Z {z:.0}"))
                .unwrap_or_default();
            shell::card(ui, false, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(label).font(FontId::new(13.0, theme::bold_family()))).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Remove").clicked() {
                            remove_at = Some(i);
                        }
                    });
                });
                if !xz.is_empty() {
                    ui.label(RichText::new(xz).monospace().color(c::NEUTRAL_700));
                }
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove_at {
            let name = self.map_refs[i].path.file_stem().and_then(|s| s.to_str()).unwrap_or("group").to_owned();
            self.record_map_undo(format!("Removed {name}"));
            self.map_refs.remove(i);
        }
        if self.map_refs.is_empty() {
            ui.label(RichText::new("No reference groups loaded.").color(c::NEUTRAL_700));
        }
    }

    /// 64 px strip under the map: the front-date slider with year ticks.
    fn map_date_strip(&mut self, ui: &mut egui::Ui) {
        let n_slots = TIMELINE.len().max(1);
        ui.add_space(6.0);
        let mut rail = Rect::NOTHING;
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(section_heading("Front date")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("step").small().color(c::NEUTRAL_700));
                shell::kbd(ui, "→");
                shell::kbd(ui, "←");
                ui.add_space(8.0);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().slider_width = ui.available_width();
                    let slider = egui::Slider::new(&mut self.front_t, 0.0..=(n_slots - 1) as f32)
                        .show_value(false);
                    let resp = ui.add(slider).on_hover_text("Front date. ← → step one mark; Home / End jump to the ends.");
                    rail = resp.rect;
                    if resp.changed() {
                        self.custom_front_xz.clear();
                        self.forget_front_marks();
                        let i = self.front_t.round().clamp(0.0, (n_slots - 1) as f32) as usize;
                        self.apply_timeline_mark(i, true);
                    }
                });
            });
        });
        // Year ticks under the rail; egui insets the rail by the handle radius.
        let inset = rail.height() / 2.5;
        let (x0, x1) = (rail.left() + inset, rail.right() - inset);
        let painter = ui.painter();
        for year in YEARS {
            let Some(i) = TIMELINE.iter().position(|m| m.year == year) else {
                continue;
            };
            let x = x0 + (x1 - x0) * i as f32 / (n_slots - 1).max(1) as f32;
            painter.line_segment(
                [Pos2::new(x, rail.bottom()), Pos2::new(x, rail.bottom() + 4.0)],
                Stroke::new(1.0_f32, c::NEUTRAL_500),
            );
            painter.text(
                Pos2::new(x, rail.bottom() + 5.0),
                Align2::CENTER_TOP,
                year.to_string(),
                FontId::monospace(12.0),
                c::NEUTRAL_700,
            );
        }
    }

    /// Overlays on the map: AO readout, legend chip, zoom group (README §5.6).
    fn map_overlays(&mut self, ui: &mut egui::Ui, area: Rect) {
        let ao = format!(
            "AO  X {:.0}–{:.0}  Z {:.0}–{:.0}",
            self.front_aabb.x_min, self.front_aabb.x_max, self.front_aabb.z_min, self.front_aabb.z_max
        );
        let painter = ui.painter_at(area);
        let hover = ui.input(|i| i.pointer.hover_pos());
        // The readouts sit over the map; with the pointer on one they fade to
        // 25 % so the map under them stays visible (and clickable: they are
        // painted, not widgets).
        let readout = |text: String, min: Pos2| -> Rect {
            let g = painter.layout_no_wrap(text, FontId::monospace(12.0), c::TEXT);
            let r = Rect::from_min_size(min, g.size() + Vec2::new(16.0, 10.0));
            let a = if hover.is_some_and(|p| r.contains(p)) { 0.25 } else { 1.0 };
            painter.rect_filled(r, 0.0, c::BG.gamma_multiply(a));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, c::DIVIDER.gamma_multiply(a)), egui::StrokeKind::Inside);
            painter.galley_with_override_text_color(r.min + Vec2::new(8.0, 5.0), g, c::TEXT.gamma_multiply(a));
            r
        };
        // 52 px down so it clears the tool banner.
        let ao_rect = readout(ao, area.min + Vec2::new(12.0, 52.0));
        if let Some(text) = self.terrain_readout() {
            readout(text, ao_rect.left_bottom() + Vec2::new(0.0, 4.0));
        }

        let chip = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new()
                .fill(c::BG)
                .stroke(Stroke::new(1.0_f32, c::DIVIDER))
                .inner_margin(egui::Margin::symmetric(8, 2))
                .show(ui, |ui| ui.horizontal(|ui| add(ui)));
        };
        let legend_rect = Rect::from_min_max(
            area.left_bottom() + Vec2::new(12.0, -46.0),
            area.left_bottom() + Vec2::new(area.width() * 0.6, -10.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(legend_rect).layout(Layout::left_to_right(Align::Max)),
            |ui| {
                chip(ui, &mut |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    legend_line(ui, c::FRONT, false, "Front");
                    legend_side(ui, shell::Side::Dprk, "DPRK");
                    legend_side(ui, shell::Side::Nato, "NATO");
                    legend_line(ui, c::ACCENT_800, true, "AO");
                    ui.menu_button("All layers ▾", |ui| self.draw_map_legend(ui))
                        .response
                        .on_hover_text("Every map layer and its color");
                });
            },
        );
        let zoom_rect = Rect::from_min_max(
            area.right_bottom() - Vec2::new(380.0, 46.0),
            area.right_bottom() - Vec2::new(12.0, 10.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(zoom_rect).layout(Layout::right_to_left(Align::Max)),
            |ui| {
                // Right-to-left: added in reverse so it reads − % + Reset view Reset AO.
                chip(ui, &mut |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.button("Reset AO").on_hover_text("Reset the AO box to the full map").clicked() {
                        self.front_aabb = WorldAabb::full_map();
                        self.map_drag_uv = None;
                    }
                    if ui.button("Reset view").on_hover_text("Reset map zoom and pan").clicked() {
                        self.map_zoom = 1.0;
                        self.map_pan = Pos2::new(0.5, 0.5);
                    }
                    if ui.button("+").on_hover_text("Zoom in").clicked() {
                        self.bump_map_zoom(1.25, None);
                    }
                    ui.label(RichText::new(format!("{:.0}%", self.map_zoom * 100.0)).monospace());
                    if ui.button("−").on_hover_text("Zoom out").clicked() {
                        self.bump_map_zoom(1.0 / 1.25, None);
                    }
                });
            },
        );
    }

    fn ensure_map_assets(&mut self, ctx: &egui::Context) {
        if self.fighter_tex_east.is_none() {
            self.fighter_tex_east = Some(ctx.load_texture(
                "eastern_fighter",
                recolor_to_side(fighter_svg_north(true, 128), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.fighter_tex_nato.is_none() {
            self.fighter_tex_nato = Some(ctx.load_texture(
                "nato_fighter",
                recolor_to_side(fighter_svg_north(false, 128), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.ship_tex_east.is_none() {
            self.ship_tex_east = Some(ctx.load_texture(
                "eastern_shipping",
                load_side_svg(include_bytes!("../assets/EasternShipping.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.ship_tex_nato.is_none() {
            self.ship_tex_nato = Some(ctx.load_texture(
                "nato_shipping",
                load_side_svg(include_bytes!("../assets/NatoShiping.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.dir_tex.is_none() {
            self.dir_tex = Some(ctx.load_texture(
                "heading_marker",
                load_fighter_svg(include_bytes!("../assets/direction.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.obj_tex_east.is_none() {
            self.obj_tex_east = Some(ctx.load_texture(
                "eastern_objective",
                load_side_svg(include_bytes!("../assets/EasternObjective.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.obj_tex_nato.is_none() {
            self.obj_tex_nato = Some(ctx.load_texture(
                "nato_objective",
                load_side_svg(include_bytes!("../assets/NatoObjective.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.armor_tex_east.is_none() {
            self.armor_tex_east = Some(ctx.load_texture(
                "eastern_armor",
                load_side_svg(include_bytes!("../assets/EasternArmor.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.armor_tex_nato.is_none() {
            self.armor_tex_nato = Some(ctx.load_texture(
                "nato_armor",
                load_side_svg(include_bytes!("../assets/NatoArmor.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.supply_tex_east.is_none() {
            self.supply_tex_east = Some(ctx.load_texture(
                "eastern_supply",
                load_side_svg(include_bytes!("../assets/EasternSupply.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.supply_tex_nato.is_none() {
            self.supply_tex_nato = Some(ctx.load_texture(
                "nato_supply",
                load_side_svg(include_bytes!("../assets/NatoSupply.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.arty_tex_east.is_none() {
            self.arty_tex_east = Some(ctx.load_texture(
                "eastern_arty",
                load_side_svg(include_bytes!("../assets/EasternArty.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.arty_tex_nato.is_none() {
            self.arty_tex_nato = Some(ctx.load_texture(
                "nato_arty",
                load_side_svg(include_bytes!("../assets/NatoArty.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.train_tex_east.is_none() {
            self.train_tex_east = Some(ctx.load_texture(
                "eastern_train",
                load_side_svg(include_bytes!("../assets/EasternTrain.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.train_tex_nato.is_none() {
            self.train_tex_nato = Some(ctx.load_texture(
                "nato_train",
                load_side_svg(include_bytes!("../assets/NatoTrain.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.infantry_tex_east.is_none() {
            self.infantry_tex_east = Some(ctx.load_texture(
                "eastern_infantry",
                load_side_svg(include_bytes!("../assets/EasternInfantry.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.infantry_tex_nato.is_none() {
            self.infantry_tex_nato = Some(ctx.load_texture(
                "nato_infantry",
                load_side_svg(include_bytes!("../assets/NatoInfantry.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.map_rx.is_some() {
            let mut disconnected = false;
            while let Some(rx) = self.map_rx.as_ref() {
                match rx.try_recv() {
                    Ok(KoreaMapLayer::Overview(image)) => {
                        self.map_lo_tex = Some(ctx.load_texture(
                            "korea_map_lo",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    Ok(KoreaMapLayer::Detail(image)) => {
                        self.map_hi_tex = Some(ctx.load_texture(
                            "korea_map_hi",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            if disconnected || self.map_hi_tex.is_some() {
                self.map_rx = None;
            }
        } else if self.map_lo_tex.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.map_rx = Some(rx);
            let ctx_clone = ctx.clone();
            std::thread::spawn(move || {
                let lo = load_korea_jpeg(
                    include_bytes!("../assets/DD052_en_map_01_LowQ.jpg"),
                    "assets/DD052_en_map_01_LowQ.jpg",
                );
                let _ = tx.send(KoreaMapLayer::Overview(lo));
                ctx_clone.request_repaint();
                let hi = load_korea_jpeg(
                    include_bytes!("../assets/DD052_en_map_01.jpg"),
                    "assets/DD052_en_map_01.jpg",
                );
                let _ = tx.send(KoreaMapLayer::Detail(hi));
                ctx_clone.request_repaint();
            });
        }
    }

    fn bump_map_zoom(&mut self, factor: f32, toward_uv: Option<Pos2>) {
        let old = self.map_zoom;
        self.map_zoom = (self.map_zoom * factor).clamp(1.0, 12.0);
        if let Some(uv) = toward_uv {
            if old > 1.0 || self.map_zoom > 1.0 {
                let t = 1.0 - old / self.map_zoom;
                self.map_pan = Pos2::new(
                    self.map_pan.x + (uv.x - self.map_pan.x) * t,
                    self.map_pan.y + (uv.y - self.map_pan.y) * t,
                );
            }
        }
        self.clamp_map_pan();
    }

    fn clamp_map_pan(&mut self) {
        let half = 0.5 / self.map_zoom;
        self.map_pan.x = self.map_pan.x.clamp(half, 1.0 - half);
        self.map_pan.y = self.map_pan.y.clamp(half, 1.0 - half);
    }

    fn map_view_uv(&self) -> Rect {
        let half = 0.5 / self.map_zoom.max(1.0);
        Rect::from_center_size(self.map_pan, Vec2::new(half * 2.0, half * 2.0))
    }

    fn draw_korea_map(&mut self, ui: &mut egui::Ui) {
        let Some(lo) = self.map_lo_tex.clone() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(" Loading map...").italics());
            });
            return;
        };
        // The map fills the center, letterboxed on NEUTRAL_200.
        let area = ui.available_rect_before_wrap();
        ui.painter().rect_filled(area, 0.0, c::NEUTRAL_200);
        let size = lo.size_vec2();
        let scale = ((area.width() - 16.0) / size.x)
            .min((area.height() - 16.0) / size.y)
            .max(0.01);
        let img_size = Vec2::new(size.x * scale, size.y * scale);
        let rect = Rect::from_center_size(area.center(), img_size);
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Korea map"));
        let view = self.map_view_uv();
        let tex = if self.map_zoom > 1.0 {
            self.map_hi_tex.clone().unwrap_or(lo)
        } else {
            lo
        };
        ui.put(rect, egui::Image::new((tex.id(), img_size)).uv(view));

        let map_rect = map_screen_rect(rect, view);

        // Zoom / Pan Logic
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.5 {
                let factor = if scroll > 0.0 { 1.15 } else { 1.0 / 1.15 };
                let toward = response.hover_pos().map(|p| pos_to_uv(map_rect, p));
                self.bump_map_zoom(factor, toward);
            }
            if scroll.abs() > 0.0 {
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = Vec2::ZERO;
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.events.retain(|e| !matches!(e, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_)));
                });
            }
        }
        let mut heading_drag_now = false;
        if self.map_drawing_mode == MapDrawingMode::None {
            if response.drag_started_by(egui::PointerButton::Secondary) {
                if let Some(pointer) = response.interact_pointer_pos() {
                    if let Some(hit) = self.hit_ship_spot(map_rect, pointer).filter(|h| self.unit_hit_unlocked(h.is_ag())) {
                        self.ship_heading_drag = Some(hit);
                        self.ground_heading_drag = None;
                    } else if let Some(hit) = self.hit_ground_spot(map_rect, pointer).filter(|h| self.unit_hit_unlocked(h.is_ag())) {
                        self.ground_heading_drag = Some(hit);
                        self.ship_heading_drag = None;
                    }
                }
            }
            if self.ship_heading_drag.is_some()
                && (response.dragged_by(egui::PointerButton::Secondary)
                    || response.drag_started_by(egui::PointerButton::Secondary))
            {
                heading_drag_now = true;
                if let (Some(hit), Some(pointer)) =
                    (self.ship_heading_drag, response.interact_pointer_pos())
                {
                    if let Some(spot) = self.ship_spot_mut(hit) {
                        let ship_pos = world_to_pos(map_rect, spot.x, spot.z);
                        if pointer.distance(ship_pos) >= 6.0 {
                            let (hx, hz) = uv_to_world(pos_to_uv(map_rect, pointer));
                            let dx = hx - spot.x;
                            let dz = hz - spot.z;
                            spot.heading_deg = dz.atan2(dx).to_degrees().rem_euclid(360.0);
                        }
                    }
                }
            }
            if self.ground_heading_drag.is_some()
                && (response.dragged_by(egui::PointerButton::Secondary)
                    || response.drag_started_by(egui::PointerButton::Secondary))
            {
                heading_drag_now = true;
                if let (Some(hit), Some(pointer)) =
                    (self.ground_heading_drag, response.interact_pointer_pos())
                {
                    if let Some(spot) = self.ground_spot_mut(hit) {
                        let unit_pos = world_to_pos(map_rect, spot.x, spot.z);
                        if pointer.distance(unit_pos) >= 6.0 {
                            let (hx, hz) = uv_to_world(pos_to_uv(map_rect, pointer));
                            let dx = hx - spot.x;
                            let dz = hz - spot.z;
                            let requested = dz.atan2(dx).to_degrees().rem_euclid(360.0);
                            if let Some(net) = spot.network.as_mut() {
                                let pose = mapnet::align_heading_to_path(net, requested);
                                spot.apply_sampled(pose);
                            } else {
                                spot.heading_deg = requested;
                                if let Ok(terrain) = crate::watermap::WaterMap::builtin() {
                                    spot.layout_offroad_waypoints(terrain);
                                }
                            }
                        }
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.ship_heading_drag = None;
            self.ground_heading_drag = None;
        }

        let pan_with_secondary = !heading_drag_now
            && (self.map_drawing_mode != MapDrawingMode::Salient
                || !response.clicked_by(egui::PointerButton::Secondary));
        if pan_with_secondary
            && (response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle))
        {
            let delta = response.drag_delta();
            let view_now = self.map_view_uv();
            self.map_pan.x -= delta.x / rect.width() * view_now.width();
            self.map_pan.y -= delta.y / rect.height() * view_now.height();
            self.clamp_map_pan();
        }

        let raw_base = if self.custom_front_xz.is_empty() {
            preview_front_xz(self.front_t)
        } else {
            self.custom_front_xz.clone()
        };
        let full_dense = crate::frontlines::densify(&raw_base, 4000.0);
        let (snap_front, _) = apply_salients(full_dense.clone(), &self.salients);
        let snap_line = {
            let clipped = clip_polyline_to_aabb(&snap_front, self.front_aabb);
            if clipped.len() >= 2 {
                clipped
            } else {
                snap_front.clone()
            }
        };

        let hover_pos = response.hover_pos().or(response.interact_pointer_pos());
        let hover_xz = hover_pos.map(|p| uv_to_world(pos_to_uv(map_rect, p)));
        self.map_hover_xz = response.hover_pos().map(|p| uv_to_world(pos_to_uv(map_rect, p)));
        let end_snap = if self.current_salient.is_empty() {
            None
        } else {
            hover_xz
                .or_else(|| self.current_salient.last().copied())
                .and_then(|p| snap_to_front(&snap_line, p))
        };

        // After Esc or a tool switch, the drag that was under way ends unused.
        if self.map_void_drag && !ui.input(|i| i.pointer.primary_down()) {
            self.map_void_drag = false;
        }
        if self.map_void_drag {
            // Swallow the rest of the cancelled drag.
        } else if let Some(pos) = response.interact_pointer_pos() {
            let uv = pos_to_uv(map_rect, pos);
            let (x, z) = uv_to_world(uv);

            match self.map_drawing_mode {
                MapDrawingMode::BaseFront => {
                    // Each click, or each whole drag, is one undoable front stroke.
                    let dragging = response.dragged_by(egui::PointerButton::Primary);
                    if dragging || response.clicked_by(egui::PointerButton::Primary) {
                        let before = self.custom_front_xz.len();
                        if dragging && self.front_stroke.is_none() {
                            self.front_stroke = Some(before);
                        }
                        if can_extend_west_east(&self.custom_front_xz, (x, z), 2500.0) {
                            self.custom_front_xz.push((x, z));
                            self.map_undo.rebase();
                            if !dragging {
                                self.note_map_drawing(DrawnMark::Front { prev_len: before });
                            }
                        }
                    }
                }
                MapDrawingMode::Salient => {
                    if response.clicked_by(egui::PointerButton::Secondary) {
                        self.commit_current_salient(&snap_line);
                    } else {
                        let clicking_end = response.clicked_by(egui::PointerButton::Primary)
                            && self.current_salient.len() >= 2
                            && end_snap.is_some_and(|e| near_map_dot(map_rect, e, pos, 14.0));
                        if clicking_end {
                            self.commit_current_salient(&snap_line);
                        } else if response.dragged_by(egui::PointerButton::Primary)
                            || response.clicked_by(egui::PointerButton::Primary)
                        {
                            let p = if self.current_salient.is_empty() {
                                self.front_aabb.clamp_point(
                                    snap_to_front(&snap_line, (x, z)).unwrap_or((x, z)),
                                )
                            } else {
                                self.front_aabb.clamp_point((x, z))
                            };
                            if can_extend_salient(&self.current_salient, p, 2500.0) {
                                self.current_salient.push(p);
                            }
                        }
                    }
                }
                MapDrawingMode::AttackArrow => {
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        self.attack_drag = Some(((x, z), (x, z)));
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some((_, tip)) = &mut self.attack_drag {
                            *tip = (x, z);
                        }
                    }
                    if response.drag_stopped() {
                        if let Some((tail, tip)) = self.attack_drag.take() {
                            let dx = tip.0 - tail.0;
                            let dz = tip.1 - tail.1;
                            if (dx * dx + dz * dz).sqrt() >= 2_500.0 {
                                self.attack_arrows.push((tail, tip));
                                self.note_map_drawing(DrawnMark::AttackArrow);
                            }
                        }
                    }
                }
                MapDrawingMode::PlaceEastObjective | MapDrawingMode::PlaceNatoObjective => {
                    let eastern = self.map_drawing_mode == MapDrawingMode::PlaceEastObjective;
                    if response.clicked_by(egui::PointerButton::Primary) {
                        let x = x.clamp(MAP_MIN, MAP_MAX);
                        let z = z.clamp(MAP_MIN, MAP_MAX);
                        if eastern {
                            self.east_objectives.push((x, z));
                        } else {
                            self.nato_objectives.push((x, z));
                        }
                        self.reaim_map_ground();
                        if !ui.input(|i| i.modifiers.shift) {
                            self.map_drawing_mode = MapDrawingMode::None;
                        }
                    }
                    if response.clicked_by(egui::PointerButton::Secondary) {
                        if self.remove_nearest_objective(eastern, map_rect, pos) {
                            self.reaim_map_ground();
                        }
                    }
                }
                MapDrawingMode::None => {
                    if response.clicked_by(egui::PointerButton::Secondary) && self.wp_selected.is_some()
                    {
                        self.wp_selected = None;
                        self.wp_drag = None;
                    }
                    if response.clicked_by(egui::PointerButton::Primary) {
                        if let Some(hit) = self
                            .hit_waypoint(map_rect, pos)
                            .filter(|(h, _)| self.unit_hit_unlocked(h.is_ag()))
                        {
                            self.wp_selected = Some(hit);
                            self.wp_drag = None;
                            self.ground_drag = None;
                            self.fighter_drag = None;
                            self.ship_drag = None;
                            self.objective_drag = None;
                            self.map_drag_uv = None;
                        } else if self.wp_selected.is_some()
                            && self.hit_ground_spot(map_rect, pos).is_none()
                            && self.hit_ship_spot(map_rect, pos).is_none()
                            && self.hit_fighter_spot(map_rect, pos).is_none()
                            && self.hit_objective(map_rect, pos).is_none()
                        {
                            if let Some((hit, wi)) = self.wp_selected {
                                self.snap_ground_wp(hit, wi, x, z);
                            }
                            self.wp_selected = None;
                            self.wp_drag = None;
                        }
                    }
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        if let Some(i) = self.hit_fighter_spot(map_rect, pos) {
                            self.fighter_drag = Some(i);
                            self.ship_drag = None;
                            self.ground_drag = None;
                            self.wp_drag = None;
                            self.wp_selected = None;
                            self.objective_drag = None;
                            self.map_drag_uv = None;
                        } else if let Some(hit) = self.hit_ship_spot(map_rect, pos).filter(|h| self.unit_hit_unlocked(h.is_ag())) {
                            self.ship_drag = Some(hit);
                            self.fighter_drag = None;
                            self.ground_drag = None;
                            self.wp_drag = None;
                            self.wp_selected = None;
                            self.objective_drag = None;
                            self.map_drag_uv = None;
                        } else if let Some(hit) = self.hit_waypoint(map_rect, pos).filter(|(h, _)| self.unit_hit_unlocked(h.is_ag())) {
                            self.wp_selected = Some(hit);
                            self.wp_drag = Some(hit);
                            self.ground_drag = None;
                            self.fighter_drag = None;
                            self.ship_drag = None;
                            self.objective_drag = None;
                            self.map_drag_uv = None;
                        } else if let Some(hit) = self.hit_ground_spot(map_rect, pos).filter(|h| self.unit_hit_unlocked(h.is_ag())) {
                            self.ground_drag = Some(hit);
                            self.wp_drag = None;
                            self.wp_selected = None;
                            self.fighter_drag = None;
                            self.ship_drag = None;
                            self.objective_drag = None;
                            self.map_drag_uv = None;
                        } else if let Some(hit) = self.hit_objective(map_rect, pos) {
                            self.objective_drag = Some(hit);
                            self.fighter_drag = None;
                            self.ship_drag = None;
                            self.ground_drag = None;
                            self.wp_drag = None;
                            self.wp_selected = None;
                            self.map_drag_uv = None;
                        } else if let Some(sel) = self.wp_selected {
                            self.wp_drag = Some(sel);
                            self.fighter_drag = None;
                            self.ship_drag = None;
                            self.ground_drag = None;
                            self.objective_drag = None;
                            self.map_drag_uv = None;
                        } else {
                            self.fighter_drag = None;
                            self.ship_drag = None;
                            self.ground_drag = None;
                            self.wp_drag = None;
                            self.objective_drag = None;
                            self.map_drag_uv = Some(uv);
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some(i) = self.fighter_drag {
                            if let Some(layout) = &mut self.map_fighters {
                                if let Some(spot) = layout.spots.get_mut(i) {
                                    spot.x = x;
                                    spot.z = z;
                                }
                            }
                        } else if let Some(hit) = self.ship_drag {
                            if let Some(spot) = self.ship_spot_mut(hit) {
                                spot.x = x;
                                spot.z = z;
                            }
                        } else if let Some((hit, wi)) = self.wp_drag {
                            self.snap_ground_wp(hit, wi, x, z);
                        } else if let Some(hit) = self.ground_drag {
                            if let Some(spot) = self.ground_spot_mut(hit) {
                                if let Some(net) = spot.network.as_mut() {
                                    let keep = spot.heading_deg;
                                    let pose = mapnet::snap_lead_to_pointer(net, x, z, keep);
                                    spot.apply_sampled(pose);
                                } else {
                                    let dx = x - spot.x;
                                    let dz = z - spot.z;
                                    spot.x = x;
                                    spot.z = z;
                                    for wp in &mut spot.waypoints {
                                        wp.0 += dx;
                                        wp.1 += dz;
                                    }
                                    if let Ok(terrain) = crate::watermap::WaterMap::builtin() {
                                        spot.refresh_path_water_issue(terrain);
                                    }
                                }
                            }
                        } else if let Some((eastern, i)) = self.objective_drag {
                            let list = if eastern {
                                &mut self.east_objectives
                            } else {
                                &mut self.nato_objectives
                            };
                            if let Some(p) = list.get_mut(i) {
                                *p = (x.clamp(MAP_MIN, MAP_MAX), z.clamp(MAP_MIN, MAP_MAX));
                            }
                            self.reaim_map_ground();
                        } else if let Some(origin) = self.map_drag_uv {
                            self.front_aabb = aabb_from_uv(origin, uv);
                        }
                    }
                    if response.drag_stopped() {
                        self.fighter_drag = None;
                        self.ship_drag = None;
                        self.ground_drag = None;
                        self.wp_drag = None;
                        self.objective_drag = None;
                    }
                }
            }
        } else if self.map_drawing_mode == MapDrawingMode::Salient
            && response.clicked_by(egui::PointerButton::Secondary)
        {
            self.commit_current_salient(&snap_line);
        }
        // A Draw-front drag ends: its points become one mark.
        if let Some(prev_len) = self.front_stroke {
            if !response.dragged_by(egui::PointerButton::Primary) {
                self.front_stroke = None;
                if self.custom_front_xz.len() > prev_len {
                    self.note_map_drawing(DrawnMark::Front { prev_len });
                }
            }
        }

        let (composite_front, patches) = apply_salients(full_dense.clone(), &self.salients);
        let painter = ui.painter_at(rect);

        let overlay = timeline_preview(self.front_t);

        draw_reference_overlays(&painter, map_rect);
        self.draw_terrain_layers(ui.ctx(), &painter, map_rect);

        for battle in &overlay.battles {
            let pos = world_to_pos(map_rect, battle.x, battle.z);
            painter.circle_filled(pos, 4.0_f32, Color32::from_rgb(240, 220, 80));
            painter.circle_stroke(pos, 4.0_f32, Stroke::new(1.0_f32, Color32::from_rgb(20, 20, 24)));
            draw_map_label(
                &painter,
                pos + Vec2::new(6.0, -6.0),
                battle.name,
                Color32::from_rgb(240, 220, 80),
                Align2::LEFT_BOTTOM,
            );
        }

        if composite_front.len() >= 2 {
            draw_front_inside_outside(&painter, map_rect, &composite_front, self.front_aabb);
            let salient_stroke = Stroke::new(1.5_f32, SALIENT_OUTLINE);
            for patch in &patches {
                for ring in clip_ring_to_aabb(&patch.ring, self.front_aabb) {
                    draw_dashed_world_line(&painter, map_rect, &ring, salient_stroke);
                }
            }
        }

        if !self.current_salient.is_empty() {
            let sketch = Stroke::new(2.0_f32, Color32::from_rgb(255, 220, 80));
            for w in self.current_salient.windows(2) {
                painter.line_segment(
                    [world_to_pos(map_rect, w[0].0, w[0].1), world_to_pos(map_rect, w[1].0, w[1].1)],
                    sketch,
                );
            }
            if let (Some(&last), Some(hover)) = (self.current_salient.last(), hover_xz) {
                let hover_clamped = self.front_aabb.clamp_point(hover);
                painter.line_segment(
                    [world_to_pos(map_rect, last.0, last.1), world_to_pos(map_rect, hover_clamped.0, hover_clamped.1)],
                    Stroke::new(1.4_f32, Color32::from_rgb(255, 230, 140)),
                );
                if let Some(end) = end_snap {
                    painter.line_segment(
                        [world_to_pos(map_rect, hover_clamped.0, hover_clamped.1), world_to_pos(map_rect, end.0, end.1)],
                        Stroke::new(1.2_f32, Color32::from_rgb(180, 130, 40)),
                    );
                }
            }
        }

        if self.map_drawing_mode == MapDrawingMode::Salient {
            if self.current_salient.is_empty() {
                let idle_line = {
                    let clipped = clip_polyline_to_aabb(&composite_front, self.front_aabb);
                    if clipped.len() >= 2 {
                        clipped
                    } else {
                        composite_front.clone()
                    }
                };
                if let Some(start) = hover_xz.and_then(|p| snap_to_front(&idle_line, p)) {
                    draw_salient_anchor(&painter, map_rect, start, false, hover_pos);
                }
            } else {
                if let Some(&start) = self.current_salient.first() {
                    draw_salient_anchor(&painter, map_rect, start, false, None);
                }
                if let Some(end) = end_snap {
                    draw_salient_anchor(&painter, map_rect, end, true, hover_pos);
                }
            }
        }

        let arrow_front = if composite_front.len() >= 2 {
            &composite_front
        } else {
            &full_dense
        };
        for &(tail, tip) in &self.attack_arrows {
            let color = faction_map_color(point_north_of_front(arrow_front, tail.0, tail.1));
            let path = attack_arrow_points(tail, tip, ARROW_TAIL_WIDTH);
            draw_preview_arrow(&painter, map_rect, &path, color);
            draw_attack_shaft(&painter, map_rect, tail, tip, color);
        }
        if let Some((tail, tip)) = self.attack_drag {
            let color = faction_map_color(point_north_of_front(arrow_front, tail.0, tail.1));
            draw_attack_shaft(&painter, map_rect, tail, tip, color);
        }

        // 7. Draw Reference Groups
        let place = self.front_aabb.expanded(PLACE_MARGIN);
        for g in &self.map_refs {
            for dot in preview_dots(&g.entity, 2500) {
                if dot.kind == PreviewKind::Airfield { continue; }
                let in_box = self.front_aabb.contains(dot.x, dot.z);
                if !place.contains(dot.x, dot.z) { continue; }
                let (color, radius) = preview_dot_style(dot.kind, in_box);
                painter.circle_filled(world_to_pos(map_rect, dot.x, dot.z), radius, color);
            }
        }
        for g in &self.map_refs {
            for dot in preview_dots(&g.entity, 2500) {
                if dot.kind != PreviewKind::Airfield { continue; }
                let in_box = self.front_aabb.contains(dot.x, dot.z);
                if !place.contains(dot.x, dot.z) { continue; }
                let (color, radius) = preview_dot_style(dot.kind, in_box);
                painter.circle_filled(world_to_pos(map_rect, dot.x, dot.z), radius, color);
            }
        }

        self.draw_map_networks(&painter, map_rect);
        self.draw_map_fighters(&painter, map_rect);
        self.draw_map_ships(&painter, map_rect);
        self.draw_map_ground(&painter, map_rect);
        self.draw_map_objectives(&painter, map_rect);

        // 8. Draw AABB Box
        // AO: dashed ACCENT_800 outline over a ~7 % accent wash (mockup 2f).
        let box_rect = aabb_to_screen(map_rect, self.front_aabb);
        painter.rect_filled(box_rect, 0.0, AO_FILL);
        let corners = [
            box_rect.left_top(),
            box_rect.right_top(),
            box_rect.right_bottom(),
            box_rect.left_bottom(),
            box_rect.left_top(),
        ];
        painter.extend(egui::Shape::dashed_line(&corners, Stroke::new(1.5_f32, c::ACCENT_800), 6.0, 4.0));

        // Tool banner at the top-center (only while a tool or a WP pick is active).
        let banner = if let Some(t) = MAP_TOOLS
            .iter()
            .find(|t| t.0 == self.map_drawing_mode && t.0 != MapDrawingMode::None)
        {
            Some((t.2.to_string(), t.3.to_string()))
        } else if let Some((hit, wi)) = self.wp_selected {
            let on_network = self.ground_spot_mut(hit).is_some_and(|s| s.network.is_some());
            let hint = if on_network {
                "Click a road or railroad, any branch · right-click to cancel"
            } else {
                "Click dry land toward the objective · right-click to cancel"
            };
            Some((format!("Place unit {} WP{}", hit.spot_i() + 1, wi + 1), hint.to_string()))
        } else {
            None
        };
        if let Some((name, hint)) = banner {
            shell::map_tool_banner(&ui.painter_at(area), area, &name, &hint);
        }
        if self.map_drawing_mode != MapDrawingMode::None && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        self.map_overlays(ui, area);
    }

    fn hit_fighter_spot(&self, map_rect: Rect, pointer: Pos2) -> Option<usize> {
        let layout = self.map_fighters.as_ref()?;
        let mut best = None;
        let mut best_d = 18.0_f32;
        for (i, s) in layout.spots.iter().enumerate() {
            let d = world_to_pos(map_rect, s.x, s.z).distance(pointer);
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
        best
    }

    fn unit_hit_unlocked(&self, from_ag: bool) -> bool {
        !from_ag || !self.recon_keep_positions
    }

    fn ship_spot_mut(&mut self, hit: ShipHit) -> Option<&mut ShipSpot> {
        match hit {
            ShipHit::Ag(i) => self.map_ships.as_mut()?.spots.get_mut(i),
            ShipHit::Army { slot, i } => {
                self.map_armies.get_mut(slot)?.ships.as_mut()?.spots.get_mut(i)
            }
        }
    }

    fn ground_spot_mut(&mut self, hit: GroundHit) -> Option<&mut GroundSpot> {
        match hit {
            GroundHit::Ag { eastern, i } => {
                let layout = if eastern {
                    self.map_ground_east.as_mut()
                } else {
                    self.map_ground_nato.as_mut()
                };
                layout?.spots.get_mut(i)
            }
            GroundHit::Army { slot, i } => {
                self.map_armies.get_mut(slot)?.ground.as_mut()?.spots.get_mut(i)
            }
        }
    }

    fn snap_ground_wp(&mut self, hit: GroundHit, wi: usize, x: f64, z: f64) {
        if let Some(spot) = self.ground_spot_mut(hit) {
            if let Some(net) = spot.network.as_mut() {
                mapnet::snap_waypoint_to_pointer(net, wi, x, z);
            } else if wi < spot.waypoints.len() {
                spot.waypoints[wi] = (x, z);
                if let Ok(terrain) = crate::watermap::WaterMap::builtin() {
                    spot.refresh_path_water_issue(terrain);
                }
            }
        }
    }

    fn hit_ship_spot(&self, map_rect: Rect, pointer: Pos2) -> Option<ShipHit> {
        let mut best = None;
        let mut best_d = 18.0_f32;
        if let Some(layout) = &self.map_ships {
            for (i, s) in layout.spots.iter().enumerate() {
                let d = world_to_pos(map_rect, s.x, s.z).distance(pointer);
                if d < best_d {
                    best_d = d;
                    best = Some(ShipHit::Ag(i));
                }
            }
        }
        for (slot, army) in self.map_armies.iter().enumerate() {
            let Some(layout) = &army.ships else { continue };
            for (i, s) in layout.spots.iter().enumerate() {
                let d = world_to_pos(map_rect, s.x, s.z).distance(pointer);
                if d < best_d {
                    best_d = d;
                    best = Some(ShipHit::Army { slot, i });
                }
            }
        }
        best
    }

    fn draw_map_fighters(&self, painter: &egui::Painter, map_rect: Rect) {
        let Some(layout) = &self.map_fighters else {
            return;
        };
        let tex = if layout.eastern {
            self.fighter_tex_east.as_ref()
        } else {
            self.fighter_tex_nato.as_ref()
        };
        let size = Vec2::splat(26.0);
        // Fighters face their side's way (README §8): DPRK south, NATO north.
        // Fighter spots carry no heading of their own, so nothing overrides it.
        let facing = shell::Side::from_eastern(layout.eastern).facing_rad();
        for spot in &layout.spots {
            let pos = world_to_pos(map_rect, spot.x, spot.z);
            if let Some(tex) = tex {
                paint_rotated_image(painter, tex, pos, size, facing, Color32::WHITE);
            } else {
                painter.circle_filled(pos, 8.0, faction_map_color(layout.eastern));
            }
            draw_map_label(
                painter,
                pos + Vec2::new(-13.0, 13.0),
                &spot.wave.to_string(),
                Color32::WHITE,
                Align2::LEFT_BOTTOM,
            );
        }
    }

    fn draw_map_ships(&self, painter: &egui::Painter, map_rect: Rect) {
        if let Some(layout) = &self.map_ships {
            self.paint_ship_layout(painter, map_rect, layout);
        }
        for army in &self.map_armies {
            if let Some(layout) = &army.ships {
                self.paint_ship_layout(painter, map_rect, layout);
            }
        }
    }

    fn paint_ship_layout(&self, painter: &egui::Painter, map_rect: Rect, layout: &MapShipLayout) {
        let tex = self.unit_tex(layout.eastern, UnitKind::Ship);
        let size = Vec2::splat(26.0);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for (i, spot) in layout.spots.iter().enumerate() {
            let pos = world_to_pos(map_rect, spot.x, spot.z);
            let tint = if spot.in_ao {
                Color32::WHITE
            } else {
                Color32::from_rgb(255, 220, 140)
            };
            if let Some(tex) = tex {
                painter.image(tex.id(), Rect::from_center_size(pos, size), uv, tint);
            } else {
                painter.circle_filled(pos, 8.0, faction_map_color(layout.eastern));
            }
            let label = if spot.in_ao {
                (i + 1).to_string()
            } else {
                format!("{}!", i + 1)
            };
            draw_map_label(
                painter,
                pos + Vec2::new(-13.0, 13.0),
                &label,
                if spot.in_ao { Color32::WHITE } else { STATUS_WARN },
                Align2::LEFT_BOTTOM,
            );
            if let Some(dir) = self.dir_tex.as_ref() {
                let heading = spot.heading_deg.to_radians() as f32;
                let arrow_size = Vec2::new(10.0, 13.0);
                let offset = 13.0 + arrow_size.y * 0.5 + 3.0;
                let dir_vec = Vec2::new(heading.sin(), -heading.cos());
                paint_rotated_image(
                    painter,
                    dir,
                    pos + dir_vec * offset,
                    arrow_size,
                    heading,
                    tint,
                );
            }
        }
    }

    fn hit_ground_spot(&self, map_rect: Rect, pointer: Pos2) -> Option<GroundHit> {
        let mut best = None;
        let mut best_d = 18.0_f32;
        for (eastern, layout) in [
            (true, self.map_ground_east.as_ref()),
            (false, self.map_ground_nato.as_ref()),
        ] {
            let Some(layout) = layout else { continue };
            for (i, s) in layout.spots.iter().enumerate() {
                let d = world_to_pos(map_rect, s.x, s.z).distance(pointer);
                if d < best_d {
                    best_d = d;
                    best = Some(GroundHit::Ag { eastern, i });
                }
            }
        }
        for (slot, army) in self.map_armies.iter().enumerate() {
            let Some(layout) = &army.ground else { continue };
            for (i, s) in layout.spots.iter().enumerate() {
                let d = world_to_pos(map_rect, s.x, s.z).distance(pointer);
                if d < best_d {
                    best_d = d;
                    best = Some(GroundHit::Army { slot, i });
                }
            }
        }
        best
    }

    fn hit_waypoint(&self, map_rect: Rect, pointer: Pos2) -> Option<(GroundHit, usize)> {
        let mut best = None;
        let mut best_d = 12.0_f32;
        let consider = |layout: &MapGroundLayout, hit: GroundHit, best: &mut Option<(GroundHit, usize)>, best_d: &mut f32| {
            for (i, s) in layout.spots.iter().enumerate() {
                for (wi, &(x, z)) in s.path_waypoints().iter().enumerate() {
                    let d = world_to_pos(map_rect, x, z).distance(pointer);
                    if d < *best_d {
                        *best_d = d;
                        *best = Some((match hit {
                            GroundHit::Ag { eastern, .. } => GroundHit::Ag { eastern, i },
                            GroundHit::Army { slot, .. } => GroundHit::Army { slot, i },
                        }, wi));
                    }
                }
            }
        };
        if let Some(layout) = &self.map_ground_east {
            consider(layout, GroundHit::Ag { eastern: true, i: 0 }, &mut best, &mut best_d);
        }
        if let Some(layout) = &self.map_ground_nato {
            consider(layout, GroundHit::Ag { eastern: false, i: 0 }, &mut best, &mut best_d);
        }
        for (slot, army) in self.map_armies.iter().enumerate() {
            if let Some(layout) = &army.ground {
                consider(layout, GroundHit::Army { slot, i: 0 }, &mut best, &mut best_d);
            }
        }
        best
    }

    fn hit_objective(&self, map_rect: Rect, pointer: Pos2) -> Option<(bool, usize)> {
        let mut best = None;
        let mut best_d = 18.0_f32;
        for (eastern, list) in [
            (true, self.east_objectives.as_slice()),
            (false, self.nato_objectives.as_slice()),
        ] {
            for (i, &(x, z)) in list.iter().enumerate() {
                let d = world_to_pos(map_rect, x, z).distance(pointer);
                if d < best_d {
                    best_d = d;
                    best = Some((eastern, i));
                }
            }
        }
        best
    }

    fn remove_nearest_objective(&mut self, eastern: bool, map_rect: Rect, pointer: Pos2) -> bool {
        let list = if eastern {
            &mut self.east_objectives
        } else {
            &mut self.nato_objectives
        };
        let mut best = None;
        let mut best_d = 18.0_f32;
        for (i, &(x, z)) in list.iter().enumerate() {
            let d = world_to_pos(map_rect, x, z).distance(pointer);
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
        if let Some(i) = best {
            list.remove(i);
            true
        } else {
            false
        }
    }

    fn reaim_map_ground(&mut self) {
        let front = self.map_front_xz();
        if let Some(layout) = &mut self.map_ground_east {
            layout.aim_at_objectives_or_front(&self.east_objectives, &front);
        }
        if let Some(layout) = &mut self.map_ground_nato {
            layout.aim_at_objectives_or_front(&self.nato_objectives, &front);
        }
        let east_obj = self.east_objectives.clone();
        let nato_obj = self.nato_objectives.clone();
        for army in &mut self.map_armies {
            if let Some(layout) = &mut army.ground {
                let objs = if army.eastern {
                    east_obj.as_slice()
                } else {
                    nato_obj.as_slice()
                };
                layout.aim_at_objectives_or_front(objs, &front);
            }
        }
        self.relayout_offroad_waypoints();
    }

    fn relayout_offroad_waypoints(&mut self) {
        let Ok(terrain) = crate::watermap::WaterMap::builtin() else {
            return;
        };
        if let Some(layout) = &mut self.map_ground_east {
            layout.layout_offroad_waypoints(terrain);
        }
        if let Some(layout) = &mut self.map_ground_nato {
            layout.layout_offroad_waypoints(terrain);
        }
        for army in &mut self.map_armies {
            if let Some(layout) = &mut army.ground {
                layout.layout_offroad_waypoints(terrain);
            }
        }
    }

    fn map_front_xz(&self) -> Vec<(f64, f64)> {
        if self.custom_front_xz.is_empty() {
            preview_front_xz(self.front_t)
        } else {
            self.custom_front_xz.clone()
        }
    }

    fn type_icons(&self, eastern: bool) -> [Option<TextureHandle>; 6] {
        if eastern {
            [
                self.ship_tex_east.clone(),
                self.armor_tex_east.clone(),
                self.supply_tex_east.clone(),
                self.arty_tex_east.clone(),
                self.infantry_tex_east.clone(),
                self.train_tex_east.clone(),
            ]
        } else {
            [
                self.ship_tex_nato.clone(),
                self.armor_tex_nato.clone(),
                self.supply_tex_nato.clone(),
                self.arty_tex_nato.clone(),
                self.infantry_tex_nato.clone(),
                self.train_tex_nato.clone(),
            ]
        }
    }

    fn unit_tex(&self, eastern: bool, kind: UnitKind) -> Option<&TextureHandle> {
        match (eastern, kind) {
            (true, UnitKind::Ship) => self.ship_tex_east.as_ref(),
            (false, UnitKind::Ship) => self.ship_tex_nato.as_ref(),
            (true, UnitKind::Armor) => self.armor_tex_east.as_ref(),
            (false, UnitKind::Armor) => self.armor_tex_nato.as_ref(),
            (true, UnitKind::Supply) => self.supply_tex_east.as_ref(),
            (false, UnitKind::Supply) => self.supply_tex_nato.as_ref(),
            (true, UnitKind::Artillery) => self.arty_tex_east.as_ref(),
            (false, UnitKind::Artillery) => self.arty_tex_nato.as_ref(),
            (true, UnitKind::Infantry) => self.infantry_tex_east.as_ref(),
            (false, UnitKind::Infantry) => self.infantry_tex_nato.as_ref(),
            (true, UnitKind::Train) => self.train_tex_east.as_ref(),
            (false, UnitKind::Train) => self.train_tex_nato.as_ref(),
        }
    }

    /// "Import new templates as": one row of six 36 × 28 type buttons.
    fn unit_kind_picker(&mut self, ui: &mut egui::Ui) {
        let icons = self.type_icons(self.recon_eastern);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for kind in UnitKind::ALL {
                if unit_kind_icon_button(
                    ui,
                    icons[kind.index()].as_ref(),
                    kind.label(),
                    &kind.hover(),
                    self.recon_import_kind == kind,
                ) {
                    self.recon_import_kind = kind;
                }
            }
        });
    }

    fn units_locked(&self) -> bool {
        self.recon_keep_positions
    }

    fn recon_slots_of(&self, kind: UnitKind) -> Vec<&ReconSlot> {
        self.recon_slots.iter().filter(|s| s.kind == kind).collect()
    }

    fn recon_ground_jobs(&self) -> Vec<GroundJob> {
        let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
        let copies = allocate_copies(&weights, self.recon_total as usize);
        let mut jobs = Vec::new();
        for (gkind, ukind) in [
            (GroundKind::Armor, UnitKind::Armor),
            (GroundKind::Supply, UnitKind::Supply),
            (GroundKind::Artillery, UnitKind::Artillery),
            (GroundKind::Infantry, UnitKind::Infantry),
            (GroundKind::Train, UnitKind::Train),
        ] {
            for (slot, n) in self.recon_slots.iter().zip(copies.iter()) {
                if slot.kind != ukind || *n == 0 {
                    continue;
                }
                let route = if gkind == GroundKind::Train {
                    match slot.info.route.clone() {
                        Some(r) if r.rail => Some(r),
                        _ => Some(crate::mapnet::RouteLayout {
                            rail: true,
                            behind: Vec::new(),
                            wp_ahead: Vec::new(),
                            zone_in_m: TRAIN_ZONE_IN_M as f64,
                        }),
                    }
                } else {
                    slot.info.route.clone()
                };
                let kind = gkind;
                let range_m = if route.is_some() {
                    None
                } else {
                    slot.info.weapon_range_m.or(match gkind {
                        GroundKind::Artillery => Some(ARTY_OBJECTIVE_RADIUS),
                        GroundKind::Armor => Some(crate::weapon_range::UNKNOWN_ARMOR_M),
                        GroundKind::Supply | GroundKind::Train | GroundKind::Infantry => None,
                    })
                };
                let wp_ahead = if route.is_some() {
                    Vec::new()
                } else {
                    slot.info.wp_ahead.clone()
                };
                jobs.extend(std::iter::repeat(GroundJob {
                    kind,
                    range_m,
                    route,
                    wp_ahead,
                }).take(*n));
            }
        }
        jobs
    }

    fn recon_copy_split(&self) -> (usize, [usize; 5]) {
        let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
        let copies = allocate_copies(&weights, self.recon_total as usize);
        let mut ships = 0usize;
        let mut ground = [0usize; 5];
        for (slot, n) in self.recon_slots.iter().zip(copies) {
            match slot.kind {
                UnitKind::Ship => ships += n,
                UnitKind::Armor => ground[0] += n,
                UnitKind::Supply => ground[1] += n,
                UnitKind::Artillery => ground[2] += n,
                UnitKind::Infantry => ground[3] += n,
                UnitKind::Train => ground[4] += n,
            }
        }
        (ships, ground)
    }

    fn unit_mix_summary(&self) -> String {
        if self.recon_slots.is_empty() {
            return "Units are specified on the Army Generator page.".into();
        }
        let (ships, ground) = self.recon_copy_split();
        let pct = self.recon_percent;
        let mut parts = Vec::new();
        if ships > 0 {
            parts.push(if self.recon_strip_randomizer {
                format!("Ship {ships} (all spawn)")
            } else {
                format!(
                    "Ship {ships} (activate {})",
                    wanted_winners(ships, pct)
                )
            });
        }
        for (kind, n) in [
            (GroundKind::Armor, ground[0]),
            (GroundKind::Supply, ground[1]),
            (GroundKind::Artillery, ground[2]),
            (GroundKind::Infantry, ground[3]),
            (GroundKind::Train, ground[4]),
        ] {
            if n > 0 {
                parts.push(if self.recon_strip_randomizer {
                    format!("{} {n} (all spawn)", kind.label())
                } else {
                    format!(
                        "{} {n} (activate {})",
                        kind.label(),
                        wanted_winners(n, pct)
                    )
                });
            }
        }
        if parts.is_empty() {
            "0 groups — set influence on Army Generator templates.".into()
        } else {
            format!("{} — specified on Army Generator", parts.join(", "))
        }
    }

    fn placement_seed() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xA5A5_5A5A)
    }

    fn ground_tex(&self, eastern: bool, kind: GroundKind) -> Option<&TextureHandle> {
        let unit = match kind {
            GroundKind::Armor => UnitKind::Armor,
            GroundKind::Supply => UnitKind::Supply,
            GroundKind::Artillery => UnitKind::Artillery,
            GroundKind::Infantry => UnitKind::Infantry,
            GroundKind::Train => UnitKind::Train,
        };
        self.unit_tex(eastern, unit)
    }

    fn draw_map_objectives(&self, painter: &egui::Painter, map_rect: Rect) {
        let size = Vec2::splat(22.0);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for (eastern, list) in [
            (true, self.east_objectives.as_slice()),
            (false, self.nato_objectives.as_slice()),
        ] {
            let tex = if eastern {
                self.obj_tex_east.as_ref()
            } else {
                self.obj_tex_nato.as_ref()
            };
            for &(x, z) in list {
                let pos = world_to_pos(map_rect, x, z);
                if let Some(tex) = tex {
                    painter.image(tex.id(), Rect::from_center_size(pos, size), uv, Color32::WHITE);
                } else {
                    painter.circle_filled(pos, 7.0, faction_map_color(eastern));
                    painter.circle_stroke(
                        pos,
                        7.0,
                        Stroke::new(1.5_f32, Color32::from_rgb(255, 230, 80)),
                    );
                }
            }
        }
    }

    fn draw_map_ground(&self, painter: &egui::Painter, map_rect: Rect) {
        if let Some(layout) = self.map_ground_east.as_ref() {
            self.paint_ground_layout(
                painter,
                map_rect,
                layout,
                GroundHit::Ag {
                    eastern: true,
                    i: 0,
                },
            );
        }
        if let Some(layout) = self.map_ground_nato.as_ref() {
            self.paint_ground_layout(
                painter,
                map_rect,
                layout,
                GroundHit::Ag {
                    eastern: false,
                    i: 0,
                },
            );
        }
        for (slot, army) in self.map_armies.iter().enumerate() {
            if let Some(layout) = &army.ground {
                self.paint_ground_layout(
                    painter,
                    map_rect,
                    layout,
                    GroundHit::Army { slot, i: 0 },
                );
            }
        }
    }

    fn paint_ground_layout(
        &self,
        painter: &egui::Painter,
        map_rect: Rect,
        layout: &MapGroundLayout,
        origin: GroundHit,
    ) {
        let size = Vec2::splat(26.0);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let hit_for = |i: usize| match origin {
            GroundHit::Ag { eastern, .. } => GroundHit::Ag { eastern, i },
            GroundHit::Army { slot, .. } => GroundHit::Army { slot, i },
        };
        for (i, spot) in layout.spots.iter().enumerate() {
                let wps = spot.path_waypoints();
                if !wps.is_empty() {
                    let wp_color = match &spot.network {
                        Some(net) if net.rail => Color32::from_rgb(30, 30, 32),
                        Some(_) => Color32::from_rgb(140, 28, 28),
                        None => Color32::from_rgb(90, 70, 40),
                    };
                    for (wi, &(wx, wz)) in wps.iter().enumerate() {
                        let wpos = world_to_pos(map_rect, wx, wz);
                        let selected = self.wp_selected == Some((hit_for(i), wi));
                        let r = if selected { 7.0 } else { 5.0 };
                        painter.circle_filled(wpos, r, wp_color);
                        painter.circle_stroke(
                            wpos,
                            r,
                            Stroke::new(
                                if selected { 2.0_f32 } else { 1.2_f32 },
                                if selected {
                                    Color32::from_rgb(255, 255, 120)
                                } else {
                                    Color32::from_rgb(255, 230, 180)
                                },
                            ),
                        );
                        draw_map_label(
                            painter,
                            wpos + Vec2::new(7.0, -6.0),
                            &format!("{} WP{}", i + 1, wi + 1),
                            Color32::from_rgb(255, 230, 180),
                            Align2::LEFT_BOTTOM,
                        );
                    }
                }
                let pos = world_to_pos(map_rect, spot.x, spot.z);
                let problem = !spot.in_ao || spot.issue.is_some();
                let tint = if spot.in_ao {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(255, 220, 140)
                };
                if let Some(tex) = self.ground_tex(layout.eastern, spot.kind) {
                    painter.image(tex.id(), Rect::from_center_size(pos, size), uv, tint);
                } else {
                    painter.circle_filled(pos, 8.0, faction_map_color(layout.eastern));
                    if spot.kind == GroundKind::Train || spot.network.as_ref().is_some_and(|n| n.rail) {
                        painter.circle_stroke(pos, 8.0, Stroke::new(2.0_f32, Color32::from_rgb(20, 20, 24)));
                    }
                }
                let label = if problem {
                    format!("{}!", i + 1)
                } else {
                    (i + 1).to_string()
                };
                draw_map_label(
                    painter,
                    pos + Vec2::new(-13.0, 13.0),
                    &label,
                    if problem { STATUS_WARN } else { Color32::WHITE },
                    Align2::LEFT_BOTTOM,
                );
                if let Some(dir) = self.dir_tex.as_ref() {
                    let heading = spot.heading_deg.to_radians() as f32;
                    let arrow_size = Vec2::new(10.0, 13.0);
                    let offset = 13.0 + arrow_size.y * 0.5 + 3.0;
                    let dir_vec = Vec2::new(heading.sin(), -heading.cos());
                    paint_rotated_image(
                        painter,
                        dir,
                        pos + dir_vec * offset,
                        arrow_size,
                        heading,
                        tint,
                    );
                }
            }
    }

    fn draw_map_networks(&self, painter: &egui::Painter, map_rect: Rect) {
        draw_network_lines(
            painter,
            map_rect,
            mapnet::roads(),
            Stroke::new(1.15_f32, ROAD_LINE),
        );
        draw_network_lines(
            painter,
            map_rect,
            mapnet::railroads(),
            Stroke::new(1.35_f32, RAIL_LINE),
        );
    }

    fn place_map_units(&mut self, eastern: bool) {
        if self.recon_keep_positions {
            self.status = Status::Error(
                "Keep loaded positions is on — units stay where the templates were authored. Turn that off on Army Generator to place along the front."
                    .into(),
            );
            return;
        }
        if self.recon_slots.is_empty() {
            self.status = Status::Error(
                "Specify unit types on the Army Generator page before placing.".into(),
            );
            return;
        }
        let (ship_n, _) = self.recon_copy_split();
        let ground_jobs = self.recon_ground_jobs();
        if ship_n == 0 && ground_jobs.is_empty() {
            self.status = Status::Error(
                "Set influence above 0 on at least one Army Generator template.".into(),
            );
            return;
        }
        let objectives = if eastern {
            self.east_objectives.as_slice()
        } else {
            self.nato_objectives.as_slice()
        };
        let side = if eastern { "DPRK" } else { "NATO" };
        let mut warnings = Vec::new();
        let terrain = match crate::watermap::WaterMap::builtin() {
            Ok(w) => w,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let raw_base = if self.custom_front_xz.is_empty() {
            preview_front_xz(self.front_t)
        } else {
            self.custom_front_xz.clone()
        };
        let seed = Self::placement_seed();
        let occupied = self.occupied_unit_xz();
        let opts = PlaceOpts {
            front_band: Some(FRONT_PLACE_BAND),
            favor: objectives,
            seed,
            occupied: &occupied,
        };
        let mut notes = Vec::new();
        let stretch_east = self.custom_front_xz.is_empty();

        if ship_n > 0 {
            match place_ships(
                eastern,
                ship_n,
                &raw_base,
                self.front_aabb,
                &[],
                stretch_east,
                &terrain,
                opts,
            ) {
                Ok(mut layout) => {
                    let n = layout.spots.len();
                    let outside = layout.spots.iter().filter(|s| !s.in_ao).count();
                    if objectives.is_empty() {
                        layout.randomize_headings(seed);
                        notes.push(format!(
                            "{n} {side} ship groups (random heading; mark an objective to aim them)."
                        ));
                        warnings.push(format!(
                            "{n} {side} ships have random heading — mark an objective to aim them."
                        ));
                    } else {
                        layout.aim_at_hashed_objectives(objectives, seed);
                        notes.push(format!("{n} {side} ship groups facing a hashed objective."));
                    }
                    if outside > 0 {
                        warnings.push(format!("{outside} ships parked outside the AO."));
                    }
                    if let Some(old) = self.map_ships.as_mut() {
                        old.spots.extend(layout.spots);
                    } else {
                        self.map_ships = Some(layout);
                    }
                    self.ship_drag = None;
                    self.ship_heading_drag = None;
                }
                Err(err) => {
                    self.status = Status::Error(err);
                    return;
                }
            }
        }

        let ground_occupied = self.occupied_unit_xz();
        let ground_opts = PlaceOpts {
            front_band: Some(FRONT_PLACE_BAND),
            favor: objectives,
            seed,
            occupied: &ground_occupied,
        };

        if !ground_jobs.is_empty() {
            match place_ground_jobs(
                eastern,
                &ground_jobs,
                &raw_base,
                self.front_aabb,
                &[],
                stretch_east,
                &terrain,
                ground_opts,
            ) {
                Ok(layout) => {
                    warnings.extend(skip_only_warnings(&layout.warnings));
                    let n = layout.spots.len();
                    let open_n = layout.spots.iter().filter(|s| !s.on_network()).count();
                    let net_n = n.saturating_sub(open_n);
                    if net_n > 0 {
                        notes.push(format!(
                            "{net_n} {side} train/column groups on roads or rails (random direction)."
                        ));
                    }
                    if open_n > 0 && objectives.is_empty() {
                        notes.push(format!(
                            "{open_n} {side} ground groups facing the front — mark a {side} objective to aim them at a target."
                        ));
                    } else if open_n > 0 {
                        notes.push(format!(
                            "{open_n} {side} ground groups facing their hashed objective."
                        ));
                    }
                    let mut ranges: Vec<f64> = ground_jobs
                        .iter()
                        .filter_map(|j| j.range_m)
                        .collect();
                    ranges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    ranges.dedup();
                    if !ranges.is_empty() && !objectives.is_empty() {
                        let txt = ranges
                            .iter()
                            .map(|r| format!("{:.1} km", r / 1000.0))
                            .collect::<Vec<_>>()
                            .join(" / ");
                        notes.push(format!(
                            "Groups sit within system range ({txt}) of that objective when open ground exists. Ground AttackArea is on the objective."
                        ));
                    }
                    if eastern {
                        if let Some(old) = self.map_ground_east.as_mut() {
                            old.spots.extend(layout.spots);
                            old.warnings.extend(skip_only_warnings(&layout.warnings));
                        } else {
                            self.map_ground_east = Some(layout);
                        }
                    } else if let Some(old) = self.map_ground_nato.as_mut() {
                        old.spots.extend(layout.spots);
                        old.warnings.extend(skip_only_warnings(&layout.warnings));
                    } else {
                        self.map_ground_nato = Some(layout);
                    }
                    let merged = if eastern {
                        self.map_ground_east.as_ref()
                    } else {
                        self.map_ground_nato.as_ref()
                    };
                    if let Some(full) = merged {
                        warnings.extend(numbered_ground_issues(&full.spots));
                    }
                    self.ground_drag = None;
                    self.ground_heading_drag = None;
                    self.wp_drag = None;
                    self.wp_selected = None;
                }
                Err(err) => {
                    self.status = Status::Error(err);
                    return;
                }
            }
        }

        notes.push("Left-drag to move, right-drag to set heading. Click a WP, then click a road, rail, or dry land to place it.".into());
        self.wp_drag = None;
        self.wp_selected = None;
        self.set_place_status(notes, warnings);
    }

    fn occupied_unit_xz(&self) -> Vec<(f64, f64)> {
        self.occupied_unit_xz_except(None)
    }

    fn occupied_unit_xz_except(&self, skip_army: Option<usize>) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        if let Some(s) = &self.map_ships {
            out.extend(s.spots.iter().map(|s| (s.x, s.z)));
        }
        for g in [self.map_ground_east.as_ref(), self.map_ground_nato.as_ref()]
            .into_iter()
            .flatten()
        {
            out.extend(g.spots.iter().map(|s| (s.x, s.z)));
        }
        for (i, a) in self.map_armies.iter().enumerate() {
            if skip_army == Some(i) {
                continue;
            }
            if let Some(s) = &a.ships {
                out.extend(s.spots.iter().map(|s| (s.x, s.z)));
            }
            if let Some(g) = &a.ground {
                out.extend(g.spots.iter().map(|s| (s.x, s.z)));
            }
        }
        out
    }

    fn set_place_status(&mut self, notes: Vec<String>, warnings: Vec<String>) {
        let lead = notes.join(" ");
        if warnings.is_empty() {
            self.status = Status::Info(lead);
        } else {
            self.status = Status::Warn {
                lead,
                items: warnings,
            };
        }
    }

    fn load_map_armies(&mut self, eastern: bool) {
        let Some(paths) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        let mut added = 0usize;
        let mut errors = Vec::new();
        for path in paths {
            if self.map_armies.iter().any(|a| a.path == path) {
                continue;
            }
            if self.map_refs.iter().any(|g| g.path == path) {
                errors.push(format!(
                    "{} is already a reference group — remove it there first to load it as an army.",
                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("group")
                ));
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => match parse_il2_document(&text).or_else(|_| parse_group_file(&text)) {
                    Ok(entity) => {
                        let copies = inspect_army_copies(&entity);
                        if copies.is_empty() {
                            errors.push(format!(
                                "{} has no units to place.",
                                path.file_stem().and_then(|s| s.to_str()).unwrap_or("group")
                            ));
                            continue;
                        }
                        if looks_like_base_map(&entity) {
                            errors.push(format!(
                                "{} is a Korea base map — use Load Base Map to restore the AO, front, and unit placement.",
                                path.file_stem().and_then(|s| s.to_str()).unwrap_or("group")
                            ));
                            continue;
                        }
                        self.map_armies.push(MapArmySlot {
                            path,
                            entity,
                            eastern,
                            reposition: true,
                            copies,
                            ground: None,
                            ships: None,
                        });
                        let idx = self.map_armies.len() - 1;
                        self.refresh_army_slot(idx);
                        added += 1;
                    }
                    Err(err) => errors.push(err),
                },
                Err(err) => errors.push(format!("Could not read {}: {err}", path.display())),
            }
        }
        if added == 0 && !errors.is_empty() {
            self.status = Status::Error(errors.join(" "));
        } else if !errors.is_empty() {
            let mut items = errors;
            if added > 0 {
                items.insert(0, format!("Loaded {added} army group(s)."));
            }
            self.status = Status::Warn {
                lead: String::new(),
                items,
            };
        } else if added == 0 {
            self.status = Status::Info("No new army groups added.".into());
        }
    }

    fn refresh_army_slot(&mut self, idx: usize) {
        if idx >= self.map_armies.len() {
            return;
        }
        if self.map_armies[idx].reposition {
            self.place_army_slot(idx);
        } else {
            self.keep_army_slot_positions(idx);
        }
    }

    fn keep_army_slot_positions(&mut self, idx: usize) {
        let Some(slot) = self.map_armies.get(idx) else {
            return;
        };
        let eastern = slot.eastern;
        let aabb = self.front_aabb;
        let mut ship_spots = Vec::new();
        let mut ground_spots = Vec::new();
        for copy in &slot.copies {
            let in_ao = aabb.contains(copy.x, copy.z);
            match copy.kind {
                ArmyUnitKind::Ship => ship_spots.push(ShipSpot {
                    x: copy.x,
                    z: copy.z,
                    in_ao,
                    heading_deg: copy.heading,
                }),
                kind => {
                    if let Some(gkind) = UnitKind::from_army(kind).ground() {
                        let mut spot = GroundSpot::at(
                            copy.x,
                            copy.z,
                            in_ao,
                            copy.heading,
                            gkind,
                            None,
                        );
                        spot.network = copy.network.clone();
                        spot.wp_ahead = copy.wp_ahead.clone();
                        spot.waypoints = if let Some(net) = &copy.network {
                            net.waypoints.clone()
                        } else {
                            copy.waypoints.clone()
                        };
                        ground_spots.push(spot);
                    }
                }
            }
        }
        let side = if eastern { "DPRK" } else { "NATO" };
        let n = slot.copies.len();
        if let Some(slot) = self.map_armies.get_mut(idx) {
            slot.ships = if ship_spots.is_empty() {
                None
            } else {
                Some(MapShipLayout {
                    eastern,
                    spots: ship_spots,
                })
            };
            slot.ground = if ground_spots.is_empty() {
                None
            } else {
                Some(MapGroundLayout {
                    eastern,
                    spots: ground_spots,
                    warnings: Vec::new(),
                })
            };
        }
        self.status = Status::Info(format!(
            "{n} {side} groups kept at authored positions. Tick Reposition to park along the front."
        ));
    }

    fn place_army_slot(&mut self, idx: usize) {
        let Some(slot) = self.map_armies.get(idx) else {
            return;
        };
        let eastern = slot.eastern;
        let copies = slot.copies.clone();
        let side = if eastern { "DPRK" } else { "NATO" };
        let objectives = if eastern {
            self.east_objectives.clone()
        } else {
            self.nato_objectives.clone()
        };
        let ship_n = copies
            .iter()
            .filter(|c| c.kind == ArmyUnitKind::Ship)
            .count();
        let ground_jobs: Vec<GroundJob> = copies
            .iter()
            .filter_map(|c| {
                let gkind = UnitKind::from_army(c.kind).ground()?;
                let route = c.route.clone();
                let range_m = if route.is_some() {
                    None
                } else {
                    c.range_m.or(match gkind {
                        GroundKind::Artillery => Some(ARTY_OBJECTIVE_RADIUS),
                        GroundKind::Armor => Some(weapon_range::UNKNOWN_ARMOR_M),
                        GroundKind::Supply | GroundKind::Train | GroundKind::Infantry => None,
                    })
                };
                let wp_ahead = if route.is_some() {
                    Vec::new()
                } else {
                    c.wp_ahead.clone()
                };
                Some(GroundJob {
                    kind: gkind,
                    range_m,
                    route,
                    wp_ahead,
                })
            })
            .collect();
        if ship_n == 0 && ground_jobs.is_empty() {
            self.status = Status::Error(format!("No units to place in that {side} group."));
            return;
        }
        let mut warnings = Vec::new();
        let terrain = match crate::watermap::WaterMap::builtin() {
            Ok(w) => w,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let raw_base = if self.custom_front_xz.is_empty() {
            preview_front_xz(self.front_t)
        } else {
            self.custom_front_xz.clone()
        };
        let seed = Self::placement_seed();
        let stretch_east = self.custom_front_xz.is_empty();
        let mut occupied = self.occupied_unit_xz_except(Some(idx));
        let mut notes = Vec::new();
        let mut placed_ships: Vec<ShipSpot> = Vec::new();
        let mut placed_ground: Vec<GroundSpot> = Vec::new();

        if ship_n > 0 {
            let opts = PlaceOpts {
                front_band: Some(FRONT_PLACE_BAND),
                favor: &objectives,
                seed,
                occupied: &occupied,
            };
            match place_ships(
                eastern,
                ship_n,
                &raw_base,
                self.front_aabb,
                &[],
                stretch_east,
                &terrain,
                opts,
            ) {
                Ok(mut layout) => {
                    layout.eastern = eastern;
                    if objectives.is_empty() {
                        layout.randomize_headings(seed);
                        warnings.push(format!(
                            "{} {side} ships have random heading — mark an objective to aim them.",
                            layout.spots.len()
                        ));
                    } else {
                        layout.aim_at_hashed_objectives(&objectives, seed);
                    }
                    let outside = layout.spots.iter().filter(|s| !s.in_ao).count();
                    if outside > 0 {
                        warnings.push(format!("{outside} ships parked outside the AO."));
                    }
                    notes.push(format!("{} {side} ship groups.", layout.spots.len()));
                    occupied.extend(layout.spots.iter().map(|s| (s.x, s.z)));
                    placed_ships = layout.spots;
                }
                Err(err) => {
                    self.status = Status::Error(err);
                    return;
                }
            }
        }

        if !ground_jobs.is_empty() {
            let opts = PlaceOpts {
                front_band: Some(FRONT_PLACE_BAND),
                favor: &objectives,
                seed,
                occupied: &occupied,
            };
            match place_ground_jobs(
                eastern,
                &ground_jobs,
                &raw_base,
                self.front_aabb,
                &[],
                stretch_east,
                &terrain,
                opts,
            ) {
                Ok(mut layout) => {
                    layout.eastern = eastern;
                    warnings.extend(skip_only_warnings(&layout.warnings));
                    notes.push(format!("{} {side} ground groups.", layout.spots.len()));
                    warnings.extend(numbered_ground_issues(&layout.spots));
                    placed_ground = layout.spots;
                }
                Err(err) => {
                    self.status = Status::Error(err);
                    return;
                }
            }
        }

        if let Some(slot) = self.map_armies.get_mut(idx) {
            slot.ships = if placed_ships.is_empty() {
                None
            } else {
                Some(MapShipLayout {
                    eastern,
                    spots: placed_ships,
                })
            };
            slot.ground = if placed_ground.is_empty() {
                None
            } else {
                Some(MapGroundLayout {
                    eastern,
                    spots: placed_ground,
                    warnings: warnings.clone(),
                })
            };
        }
        notes.push("Left-drag to move, right-drag to set heading. Click a WP, then click a road, rail, or dry land to place it.".into());
        self.ground_drag = None;
        self.ground_heading_drag = None;
        self.wp_drag = None;
        self.wp_selected = None;
        self.set_place_status(notes, warnings);
    }

    fn build_loaded_army_packs(&self) -> Result<(Vec<MapShipPack>, Vec<MapGroundPack>), String> {
        let mut ships = Vec::new();
        let mut ground = Vec::new();
        for slot in &self.map_armies {
            let ship_poses: Vec<(f64, f64, f64)> = slot
                .ships
                .as_ref()
                .map(|s| {
                    s.spots
                        .iter()
                        .map(|p| (p.x, p.z, p.heading_deg))
                        .collect()
                })
                .unwrap_or_default();
            let ground_spots = slot
                .ground
                .as_ref()
                .map(|g| g.spots.as_slice())
                .unwrap_or(&[]);
            if ship_poses.is_empty() && ground_spots.is_empty() {
                continue;
            }
            let mut root = slot.entity.clone();
            park_army_mixed(&mut root, &slot.copies, &ship_poses, ground_spots);
            if slot.reposition {
                snap_army_placed_attack_areas(
                    &mut root,
                    &slot.copies,
                    ground_spots,
                    &self.map_front_xz(),
                    slot.eastern,
                );
            }
            let country = country_for_coalition(slot.eastern, self.country);
            apply_overrides(&mut root, "", country);
            // Written into the generated .Group as a group name: keep "Eastern".
            let side = if slot.eastern { "Eastern" } else { "NATO" };
            let name = slot
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Army");
            root.set_name(&format!("{side} {name}"));
            let ships_only = slot.copies.iter().all(|c| c.kind == ArmyUnitKind::Ship);
            if ships_only {
                ships.push(MapShipPack { root });
            } else {
                ground.push(MapGroundPack { root });
            }
        }
        Ok((ships, ground))
    }

    fn build_map_ship_packs(&self) -> Result<Vec<MapShipPack>, String> {
        if self.recon_keep_positions {
            return Ok(Vec::new());
        }
        let Some(layout) = &self.map_ships else {
            return Ok(Vec::new());
        };
        if layout.spots.is_empty() {
            return Ok(Vec::new());
        }
        let slots = self.recon_slots_of(UnitKind::Ship);
        if slots.is_empty() {
            return Err("specify at least one Ship template on Army Generator before generating.".into());
        }
        let weights: Vec<u32> = slots.iter().map(|s| s.influence.max(1)).collect();
        let copies = allocate_copies(&weights, layout.spots.len());
        let mut inputs = Vec::new();
        for (slot, n) in slots.iter().zip(copies.iter()) {
            if *n == 0 {
                continue;
            }
            if slot.selected_triggers.is_empty() {
                return Err(format!("Select a Zone In for {}.", slot.info.name));
            }
            let text = std::fs::read_to_string(&slot.path)
                .map_err(|err| format!("Could not read template: {err}"))?;
            let root = parse_group_file(&text).map_err(|err| format!("Parse failed: {err}"))?;
            inputs.push(ReconInput {
                label: slot.info.name.clone(),
                trigger_zone_ids: slot.selected_triggers.clone(),
                copies: *n,
                root,
            });
        }
        if inputs.is_empty() {
            return Err("shipping templates produced no copies.".into());
        }
        let mut root = generate_recon_ex(
            &inputs,
            ReconBuild {
                activate_percent: self.recon_percent,
                keep_positions: true,
                start_delay_s: START_DELAY_S,
                group_delay_s: GROUP_DELAY_S,
                spawn_all: self.recon_strip_randomizer,
            },
        )?;
        let spots: Vec<(f64, f64)> = layout.spots.iter().map(|s| (s.x, s.z)).collect();
        let headings: Vec<f64> = layout.spots.iter().map(|s| s.heading_deg).collect();
        park_recon_copies_headed(&mut root, &spots, &headings);
        let country = country_for_coalition(layout.eastern, self.country);
        apply_overrides(&mut root, "", country);
        // Written into the generated .Group as a group name: keep "Eastern".
        let side = if layout.eastern { "Eastern" } else { "NATO" };
        root.set_name(&format!("{side} Shipping"));
        Ok(vec![MapShipPack { root }])
    }

    fn build_map_ground_packs(&self) -> Result<Vec<MapGroundPack>, String> {
        if self.recon_keep_positions {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for layout in [self.map_ground_east.as_ref(), self.map_ground_nato.as_ref()]
            .into_iter()
            .flatten()
        {
            if layout.spots.is_empty() {
                continue;
            }
            out.push(self.build_one_ground_pack(layout)?);
        }
        Ok(out)
    }

    fn build_one_ground_pack(&self, layout: &MapGroundLayout) -> Result<MapGroundPack, String> {
        let kinds = [
            (GroundKind::Armor, UnitKind::Armor),
            (GroundKind::Supply, UnitKind::Supply),
            (GroundKind::Artillery, UnitKind::Artillery),
            (GroundKind::Infantry, UnitKind::Infantry),
            (GroundKind::Train, UnitKind::Train),
        ];
        let mut inputs = Vec::new();
        for (gkind, ukind) in kinds {
            let n = layout.spots.iter().filter(|s| s.kind == gkind).count();
            if n == 0 {
                continue;
            }
            let slots = self.recon_slots_of(ukind);
            if slots.is_empty() {
                return Err(format!(
                    "specify at least one {} template on Army Generator before generating.",
                    gkind.label()
                ));
            }
            let weights: Vec<u32> = slots.iter().map(|s| s.influence.max(1)).collect();
            let copies = allocate_copies(&weights, n);
            for (slot, c) in slots.iter().zip(copies.iter()) {
                if *c == 0 {
                    continue;
                }
                if slot.selected_triggers.is_empty() {
                    return Err(format!("Select a Zone In for {}.", slot.info.name));
                }
                let text = std::fs::read_to_string(&slot.path)
                    .map_err(|err| format!("Could not read template: {err}"))?;
                let root = parse_group_file(&text).map_err(|err| format!("Parse failed: {err}"))?;
                inputs.push(ReconInput {
                    label: slot.info.name.clone(),
                    trigger_zone_ids: slot.selected_triggers.clone(),
                    copies: *c,
                    root,
                });
            }
        }
        if inputs.is_empty() {
            return Err("ground templates produced no copies.".into());
        }
        let mut root = generate_recon_ex(
            &inputs,
            ReconBuild {
                activate_percent: self.recon_percent,
                keep_positions: true,
                start_delay_s: GROUND_START_DELAY_S,
                group_delay_s: GROUND_GROUP_DELAY_S,
                spawn_all: self.recon_strip_randomizer,
            },
        )?;
        park_recon_copies_spots(&mut root, &layout.spots);
        snap_placed_attack_areas(
            &mut root,
            &layout.spots,
            &self.map_front_xz(),
            layout.eastern,
        );
        let country = country_for_coalition(layout.eastern, self.country);
        apply_overrides(&mut root, "", country);
        // Written into the generated .Group as a group name: keep "Eastern".
        let side = if layout.eastern { "Eastern" } else { "NATO" };
        root.set_name(&format!("{side} Ground"));
        Ok(MapGroundPack { root })
    }

    fn place_map_fighters(&mut self, eastern: bool) {
        let (types, _) = self.selected_types();
        if types.is_empty() {
            self.status = Status::Error(
                "Select at least one aircraft type on Fighter Pack.".into(),
            );
            return;
        }
        let template = match self.load_fighter_template() {
            Ok(t) => t,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let raw_base = if self.custom_front_xz.is_empty() {
            preview_front_xz(self.front_t)
        } else {
            self.custom_front_xz.clone()
        };
        match place_in_coalition(
            eastern,
            &raw_base,
            self.front_aabb,
            &self.salients,
            self.custom_front_xz.is_empty(),
            zone_in_radius(&template),
            self.fighter_waves as usize,
            self.linked_groups as usize,
            self.fighter_fill,
        ) {
            Ok(layout) => {
                let n = layout.spots.len();
                let packs = layout
                    .spots
                    .iter()
                    .map(|s| s.pack)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                let side = if eastern { "DPRK" } else { "NATO" };
                // One fighter layout at a time: placing again replaces it, undoably.
                if self.map_fighters.is_some() || !self.map_imported_fighters.is_empty() {
                    self.record_map_undo("Replaced the placed fighters".into());
                }
                self.status = Status::Info(format!(
                    "{side}: {n} groups in {packs} packs. Drag icons with Select AO / move units (1) to fine-tune."
                ));
                self.map_fighters = Some(layout);
                self.map_imported_fighters.clear();
                self.fighter_drag = None;
            }
            Err(err) => {
                self.status = Status::Error(err);
            }
        }
    }

    fn load_fighter_template(&self) -> Result<crate::ast::Il2Entity, String> {
        match &self.custom_path {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .map_err(|err| format!("Could not read template: {err}"))?;
                parse_group_file(&text).map_err(|err| format!("Parse failed: {err}"))
            }
            None => builtin_template().map_err(|err| format!("Built-in template failed: {err}")),
        }
    }

    fn configured_fighter_root(&self, country: i32) -> Result<crate::ast::Il2Entity, String> {
        let (types, skills) = self.selected_types();
        if types.is_empty() {
            return Err("Select at least one aircraft type on Fighter Pack.".into());
        }
        let mut root = self.load_fighter_template()?;
        let cfg = FlightConfig {
            flight_count: self.flight_count,
            max_in_flight: self.max_in_flight,
            type_ids: types,
            type_skills: skills,
            country,
            cooldown: self.cooldown,
            reinforcement: self.reinforcement,
            delete_orders: self.delete_orders,
            altitude_min: self.altitude_min,
            altitude_max: self.altitude_max,
        };
        configure_aircraft(&mut root, &cfg)?;
        Ok(root)
    }

    fn build_map_fighter_packs(&self) -> Result<Vec<MapFighterPack>, String> {
        if !self.map_imported_fighters.is_empty() {
            return self.stamp_imported_fighters();
        }
        let Some(layout) = &self.map_fighters else {
            return Ok(Vec::new());
        };
        if layout.spots.is_empty() {
            return Ok(Vec::new());
        }
        let country = country_for_coalition(layout.eastern, self.country);
        let template = self.configured_fighter_root(country)?;
        let mut by_pack: std::collections::BTreeMap<u32, Vec<&crate::mapfighters::FighterSpot>> =
            std::collections::BTreeMap::new();
        for s in &layout.spots {
            by_pack.entry(s.pack).or_default().push(s);
        }
        // Written into the generated .Group as a group name: keep "Eastern".
        let side = if layout.eastern { "Eastern" } else { "NATO" };
        let mut out = Vec::new();
        for (pack_id, mut members) in by_pack {
            members.sort_by_key(|s| s.slot);
            let positions: Vec<(f64, f64)> = members.iter().map(|s| (s.x, s.z)).collect();
            let wave = members[0].wave;
            let name = format!("{side} Fighters Wave {wave} pack {}", pack_id + 1);
            let mut root = generate_pack_at(&template, &positions, &name)?;
            let rtbs: Vec<(f64, f64)> = positions
                .iter()
                .map(|&(x, z)| rtb_ao_point(layout.eastern, x, z, self.front_aabb))
                .collect();
            park_rtbs(&mut root, &rtbs);
            apply_overrides(&mut root, "", country);
            out.push(MapFighterPack { root });
        }
        Ok(out)
    }

    fn current_mark(&self) -> TimelineMark {
        let max = TIMELINE.len().saturating_sub(1);
        let i = self.front_t.round().clamp(0.0, max as f32) as usize;
        TIMELINE.get(i).copied().unwrap_or(TIMELINE[0])
    }

    fn snap_timeline(&mut self, idx: usize) {
        self.custom_front_xz.clear();
        self.salients.clear();
        self.current_salient.clear();
        self.drawn_marks.retain(|m| *m != DrawnMark::Salient);
        self.forget_front_marks();
        self.apply_timeline_mark(idx, true);
        self.front_t = idx.min(TIMELINE.len().saturating_sub(1)) as f32;
    }

    fn commit_current_salient(&mut self, front: &[(f64, f64)]) {
        if self.current_salient.len() < 2 {
            self.current_salient.clear();
            return;
        }
        if let Some(&last) = self.current_salient.last() {
            if let Some(end) = snap_to_front(front, last) {
                if let Some(p) = self.current_salient.last_mut() {
                    *p = self.front_aabb.clamp_point(end);
                }
            }
        }
        for p in &mut self.current_salient {
            *p = self.front_aabb.clamp_point(*p);
        }
        let cropped = clip_polyline_to_aabb(&self.current_salient, self.front_aabb);
        if cropped.len() >= 2 {
            self.current_salient = cropped;
        }
        if self.current_salient.len() < 2 || stroke_self_intersects(&self.current_salient) {
            self.current_salient.clear();
            return;
        }
        self.salients.push(std::mem::take(&mut self.current_salient));
        self.note_map_drawing(DrawnMark::Salient);
    }

    /// Undo the last drawing: first a stroke still in progress, then the
    /// newest finished mark (front stroke, salient or arrow).
    fn remove_last_mark(&mut self) {
        if self.cancel_map_stroke() {
            return;
        }
        if let Some(mark) = self.drawn_marks.pop() {
            match mark {
                DrawnMark::Salient => {
                    if let Some(s) = self.salients.pop() {
                        self.redo_salients.push(s);
                        self.redo_marks.push(mark);
                    }
                }
                DrawnMark::AttackArrow => {
                    if let Some(a) = self.attack_arrows.pop() {
                        self.redo_attack_arrows.push(a);
                        self.redo_marks.push(mark);
                    }
                }
                DrawnMark::Front { prev_len } => {
                    let cut = prev_len.min(self.custom_front_xz.len());
                    self.redo_fronts.push(self.custom_front_xz.split_off(cut));
                    self.redo_marks.push(mark);
                }
            }
            self.map_undo.rebase();
        }
        // Back below the marks drawn after the last Clear: that Clear is next.
        if self.map_undo.label().is_some() && self.drawn_marks.len() <= self.map_undo_marks {
            self.map_last = MapAction::Clear;
        }
    }

    fn redo_last_mark(&mut self) {
        if let Some(mark) = self.redo_marks.pop() {
            match mark {
                DrawnMark::Salient => {
                    if let Some(s) = self.redo_salients.pop() {
                        self.salients.push(s);
                        self.drawn_marks.push(mark);
                    }
                }
                DrawnMark::AttackArrow => {
                    if let Some(a) = self.redo_attack_arrows.pop() {
                        self.attack_arrows.push(a);
                        self.drawn_marks.push(mark);
                    }
                }
                DrawnMark::Front { .. } => {
                    if let Some(points) = self.redo_fronts.pop() {
                        self.custom_front_xz.extend(points);
                        self.drawn_marks.push(mark);
                    }
                }
            }
            self.map_last = MapAction::Drawing;
            self.map_undo.rebase();
        }
    }

    fn clear_redo_stack(&mut self) {
        self.redo_marks.clear();
        self.redo_salients.clear();
        self.redo_attack_arrows.clear();
        self.redo_fronts.clear();
    }

    fn apply_timeline_mark(&mut self, idx: usize, clear_focus: bool) {
        if let Some(m) = TIMELINE.get(idx.min(TIMELINE.len().saturating_sub(1))) {
            self.front_year = m.year;
            self.front_season = m.season;
        }
        if clear_focus {
            self.front_focus = None;
        }
    }

    fn handle_map_timeline_keys(&mut self, ui: &egui::Ui) {
        if ui.ctx().wants_keyboard_input() {
            return;
        }
        let n = TIMELINE.len().max(1);
        let max = n - 1;
        let cur = self.front_t.round().clamp(0.0, max as f32) as usize;
        let mut next = None;
        ui.ctx().input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                next = Some(cur.saturating_sub(1));
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                next = Some((cur + 1).min(max));
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                next = Some(0);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::End) {
                next = Some(max);
            }
        });
        if let Some(idx) = next {
            self.snap_timeline(idx);
        }
    }

    /// "All layers ▾": every map layer, drawn the way the map draws it.
    fn draw_map_legend(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Legend").strong());
        ui.set_min_width(220.0);
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(320.0);
            legend_line(ui, c::FRONT, false, "Front line");
            legend_line(ui, SALIENT_OUTLINE, true, "Salient");
            legend_line(ui, c::ACCENT_800, true, "AO box");
            legend_side(ui, shell::Side::Dprk, "DPRK");
            legend_side(ui, shell::Side::Nato, "NATO");
            legend_line(ui, ROAD_LINE, false, "Road");
            legend_line(ui, RAIL_LINE, false, "Railroad");
            legend_swatch(ui, Color32::from_rgb(240, 220, 80), "Major battle");
            legend_swatch(ui, Color32::from_rgb(30, 90, 220), "Airfield");
            legend_swatch(ui, Color32::from_rgb(255, 140, 40), "Linked entity");
            legend_swatch(ui, BLOCK_DOT, "Block");
        });
    }

    fn focus_battle(&mut self, battle: &Battle) {
        self.front_focus = Some(battle.id);
        let (x, z) = crate::geo::latlon_to_xz(battle.lat, battle.lon);
        let pad = 40_000.0;
        self.front_aabb = WorldAabb::from_corners(x - pad, z - pad, x + pad, z + pad);
        let idx = mark_for_battle(battle.id);
        self.front_t = idx.min(TIMELINE.len().saturating_sub(1)) as f32;
        self.apply_timeline_mark(idx, false);
        self.front_focus = Some(battle.id);
    }

    /// Returns true when the status bar's Undo is clicked.
    fn status_line(&self, ui: &mut egui::Ui) -> bool {
        // An active map tool owns the status line until it is put down (README §6.2).
        if self.mode == AppMode::Map {
            if let Some(msg) = self.map_tool_status() {
                return shell::status_bar(ui, Severity::Info, &msg, self.undo_label());
            }
        }
        let (severity, msg) = match &self.status {
            Status::Idle => match self.idle_problem() {
                Some(problem) => (Severity::Warn, problem),
                None => (Severity::Info, self.idle_hint().to_owned()),
            },
            Status::Info(msg) => (Severity::Info, msg.clone()),
            Status::Warn { lead, items } => {
                let msg = if lead.is_empty() {
                    items.first().cloned().unwrap_or_default()
                } else {
                    lead.clone()
                };
                (Severity::Warn, msg)
            }
            Status::Error(msg) => (Severity::Error, msg.clone()),
        };
        shell::status_bar(ui, severity, &msg, self.undo_label())
    }

    /// What the one-line status bar cannot hold: warning bullets and the rest
    /// of a multi-line message. Shown above the status bar while it lasts.
    fn status_details(&self, ctx: &egui::Context) {
        let lines: Vec<&str> = match &self.status {
            Status::Idle => return,
            Status::Warn { lead, items } => items
                .iter()
                .skip(usize::from(lead.is_empty()))
                .map(String::as_str)
                .collect(),
            Status::Info(msg) | Status::Error(msg) => msg.lines().skip(1).collect(),
        };
        if lines.is_empty() {
            return;
        }
        let warn = matches!(self.status, Status::Warn { .. });
        egui::TopBottomPanel::bottom("status_details")
            .resizable(false)
            .max_height(150.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for line in lines {
                        if warn {
                            ui.label(RichText::new(format!("• {line}")).color(c::WARN_TEXT));
                        } else {
                            ui.label(line);
                        }
                    }
                });
            });
    }

    fn idle_hint(&self) -> &'static str {
        match self.mode {
            AppMode::Template => "Add units, choose Activate or Spawn, then generate a proximity-triggered group.",
            AppMode::Fighter => "Ready — no template file required.",
            AppMode::Exclusive => {
                "Add templates or a generated Exclusive Activation pack, then generate."
            }
            AppMode::Recon => match self.recon_submode {
                ReconSubmode::New => {
                    "Add ground-unit templates, set the total and ratio, then generate."
                }
                ReconSubmode::Rework => {
                    "Add exported Random Ground Units packs, then generate a new file."
                }
            },
            AppMode::Airfield => {
                "Load a Freeflight airfield from _gen.mission, then generate the cleaned group."
            }
            AppMode::Map => {
                "Draw a box on the Korea map, then generate icons for that area."
            },
        }
    }

    /// With nothing else to report, the idle status names the first thing
    /// that blocks Generate on this tab (Exclusive Activation plans).
    fn idle_problem(&self) -> Option<String> {
        match self.mode {
            AppMode::Exclusive => exclusive_first_problem(&self.bomber_slots),
            _ => None,
        }
    }

    fn selected_types(&self) -> (Vec<String>, Vec<i32>) {
        AIRCRAFT_TYPES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.type_enabled[*i])
            .map(|(i, ac)| (ac.id.to_string(), self.type_skill[i]))
            .unzip()
    }

    fn pick_template(&mut self) {
        let picked = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_file();
        let Some(path) = picked else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse_group_file(&text) {
                Ok(entity) => {
                    if entity.find_by_name("Group 1").is_none()
                        || entity.find_by_name("NodeGates").is_none()
                    {
                        self.status = Status::Error(
                            "File is not a linked fighter pack (needs Group 1 and NodeGates)."
                                .into(),
                        );
                        return;
                    }
                    self.status = Status::Info(format!(
                        "Using custom template {}.",
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                    ));
                    self.custom_path = Some(path);
                    let _ = entity;
                }
                Err(err) => {
                    self.status = Status::Error(format!("Parse failed: {err}"));
                }
            },
            Err(err) => {
                self.status = Status::Error(format!("Could not read file: {err}"));
            }
        }
    }

    fn add_bomber_template(&mut self) {
        let Some(paths) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        let before = self.bomber_slots.len();
        let mut last_err = None;
        let mut loaded_pack = false;
        for path in paths {
            match self.add_bomber_from_path(path.clone()) {
                Ok(from_pack) => {
                    if from_pack {
                        loaded_pack = true;
                        self.bomber_loaded_path = Some(path);
                    }
                }
                Err(err) => last_err = Some(err),
            }
        }
        let added = self.bomber_slots.len().saturating_sub(before);
        if loaded_pack {
            self.bomber_keep_positions = true;
        }
        if added == 0 {
            self.status = Status::Error(
                last_err.unwrap_or_else(|| "No Exclusive Activation templates were added.".into()),
            );
        } else if loaded_pack {
            self.status = Status::Info(format!(
                "Loaded {added} plan(s) from Exclusive Activation. Positions are kept for in-place export. Add another template if you need a new plan."
            ));
        } else if let Some(err) = last_err {
            self.status = Status::Info(format!(
                "Added {added} plan(s). Some files were skipped: {err}"
            ));
        } else {
            self.status = Status::Info(format!("Added {added} plan(s)."));
        }
    }

    fn add_bomber_from_path(&mut self, path: PathBuf) -> Result<bool, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("Could not read file: {err}"))?;
        let entity = parse_group_file(&text)
            .or_else(|_| parse_il2_document(&text))
            .map_err(|err| format!("Parse failed: {err}"))?;
        if looks_like_exclusive_pack(&entity) {
            let plans = extract_exclusive_plans(&entity)?;
            for plan in plans {
                self.push_bomber_slot(path.clone(), plan)?;
            }
            return Ok(true);
        }
        self.push_bomber_slot(path, entity)?;
        Ok(false)
    }

    fn push_bomber_slot(
        &mut self,
        path: PathBuf,
        root: crate::ast::Il2Entity,
    ) -> Result<(), String> {
        let info = inspect_plan(&root)?;
        self.bomber_slots.push(BomberSlot {
            selected_triggers: info.suggested_triggers.clone(),
            selected_completion: info.suggested_completion,
            info,
            path,
            root,
        });
        Ok(())
    }

    fn generate_bomber_file(&mut self) {
        if self.bomber_slots.is_empty() {
            self.status = Status::Error("Add at least one template.".into());
            return;
        }

        let mut inputs = Vec::new();
        let mut locale_paths = Vec::new();
        for (n, slot) in self.bomber_slots.iter().enumerate() {
            if slot.selected_triggers.is_empty() {
                self.status = Status::Error(format!(
                    "Plan {} needs at least one checkzone selected.",
                    n + 1
                ));
                return;
            }
            let Some(end_id) = slot.selected_completion else {
                self.status = Status::Error(format!(
                    "Plan {} needs an end timer selected.",
                    n + 1
                ));
                return;
            };
            locale_paths.push(slot.path.clone());
            inputs.push(BomberInput {
                label: slot.info.name.clone(),
                source_key: slot.path.to_string_lossy().to_string(),
                trigger_zone_ids: slot.selected_triggers.clone(),
                completion_timer_id: end_id,
                root: slot.root.clone(),
            });
        }

        let mut generated = match link_bomber_plans_with(&inputs, self.bomber_keep_positions) {
            Ok(g) => g,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let terrain_note = self.apply_terrain(&mut generated);
        let text = serialize_group(&generated);
        let suggested = format!(
            "Exclusive_Activation_{}plan.Group",
            self.bomber_slots.len()
        );
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };

        let n = self.bomber_slots.len();
        let place = if self.bomber_keep_positions {
            "in place"
        } else {
            "on the parking grid"
        };
        let summary = format!(
            "Wrote {n} exclusive plan{} {place}",
            if n == 1 { "" } else { "s" }
        );
        self.status = save_with_sidecars(&save_path, &text, &locale_paths, &summary);
        if !matches!(self.status, Status::Error(_)) {
            self.bomber_loaded_path = None;
        }
        self.add_terrain_note(terrain_note);
    }

    fn add_recon_from_path(&mut self, path: PathBuf) -> Result<(), String> {
        if self.recon_slots.iter().any(|s| s.path == path) {
            return Ok(());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("Could not read file: {err}"))?;
        let entity = parse_group_file(&text).map_err(|err| format!("Parse failed: {err}"))?;
        if looks_like_placed_pack(&entity) {
            return Err(
                "that file is a placed pack — open Rework Existing and use Add pack…".into(),
            );
        }
        let info = inspect_unit(&entity)?;
        let kind = if entity.count_block_type("Train") > 0
            || info.route.as_ref().is_some_and(|r| r.rail)
        {
            UnitKind::Train
        } else if crate::weapon_range::group_is_infantry(&entity) {
            UnitKind::Infantry
        } else {
            self.recon_import_kind
        };
        self.recon_slots.push(ReconSlot {
            selected_triggers: info.suggested_triggers.clone(),
            restore_start: info
                .suggested_restore()
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            info,
            kind,
            influence: 10,
            detected: None,
            sources: Vec::new(),
            path,
        });
        Ok(())
    }

    fn add_recon_paths(&mut self, paths: Vec<PathBuf>) {
        let before = self.recon_slots.len();
        let mut last_err = None;
        for path in paths {
            if let Err(err) = self.add_recon_from_path(path) {
                last_err = Some(err);
            }
        }
        let added = self.recon_slots.len().saturating_sub(before);
        if added == 0 {
            self.status = Status::Error(
                last_err.unwrap_or_else(|| "No ground-unit .Group files were added.".into()),
            );
        } else if let Some(err) = last_err {
            self.status = Status::Info(format!(
                "Added {added} template(s). Some files were skipped: {err}"
            ));
        } else {
            self.status = Status::Info(format!("Added {added} ground-unit template(s)."));
        }
    }

    fn add_recon_template(&mut self) {
        let Some(paths) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        self.add_recon_paths(paths);
    }

    fn add_recon_folder(&mut self) {
        let Some(dir) = dialog::FileDialog::new().pick_folder() else {
            return;
        };
        let paths = group_files_in_dir(&dir);
        if paths.is_empty() {
            self.status = Status::Error(format!(
                "No .Group files in {}.",
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("that folder")
            ));
            return;
        }
        self.add_recon_paths(paths);
    }

    fn generate_recon_file(&mut self) {
        if self.recon_slots.is_empty() {
            self.status = Status::Error("Add at least one unit template.".into());
            return;
        }
        let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
        if weights.iter().all(|w| *w == 0) {
            self.status = Status::Error("Set influence above 0 on at least one template.".into());
            return;
        }
        let copies = allocate_copies(&weights, self.recon_total as usize);

        let mut inputs = Vec::new();
        let mut locale_paths = Vec::new();
        for (slot, n) in self.recon_slots.iter().zip(copies.iter()) {
            if *n == 0 {
                continue;
            }
            if slot.selected_triggers.is_empty() {
                self.status = Status::Error(format!(
                    "Select a Zone In for {}.",
                    slot.info.name
                ));
                return;
            }
            match std::fs::read_to_string(&slot.path) {
                Ok(text) => match parse_group_file(&text) {
                    Ok(root) => {
                        locale_paths.push(slot.path.clone());
                        inputs.push(ReconInput {
                            label: slot.info.name.clone(),
                            trigger_zone_ids: slot.selected_triggers.clone(),
                            copies: *n,
                            root,
                        });
                    }
                    Err(err) => {
                        self.status = Status::Error(format!("Parse failed: {err}"));
                        return;
                    }
                },
                Err(err) => {
                    self.status = Status::Error(format!("Could not read template: {err}"));
                    return;
                }
            }
        }

        let mut generated = match generate_recon_ex(
            &inputs,
            ReconBuild {
                activate_percent: self.recon_percent,
                keep_positions: self.recon_keep_positions,
                start_delay_s: self.recon_start_delay_s as f64,
                group_delay_s: self.recon_group_delay_ms as f64 / 1000.0,
                spawn_all: self.recon_strip_randomizer,
            },
        ) {
            Ok(g) => g,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let terrain_note = self.apply_terrain(&mut generated);
        let text = serialize_group(&generated);
        let mix = allocate_mix(&weights, self.recon_total as usize, self.recon_percent);
        let live: usize = mix.iter().map(|m| m.activate).sum();
        let suggested = if self.recon_strip_randomizer {
            format!("Army_{}.Group", self.recon_total)
        } else {
            format!(
                "Random_Ground_Units_{}of{}.Group",
                live, self.recon_total
            )
        };
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };
        let delay_note = recon_delay_note(self.recon_start_delay_s, self.recon_group_delay_ms);
        let summary = if self.recon_strip_randomizer {
            format!("Wrote {} placements (all spawn{delay_note})", self.recon_total)
        } else {
            format!(
                "Wrote {} placements (exactly {} live{delay_note})",
                self.recon_total, live
            )
        };
        self.status = save_with_sidecars(&save_path, &text, &locale_paths, &summary);
        self.add_terrain_note(terrain_note);
    }

    fn add_placed_from_path(&mut self, path: PathBuf) -> Result<(), String> {
        let root = load_group(&path)?;
        let info = inspect_placed_pack(&root)?;
        drop_rework_path(&mut self.recon_rework, &path);
        for ty in info.types {
            let mut unit = ty.unit;
            unit.name = ty.name.clone();
            if let Some(existing) = self
                .recon_rework
                .iter_mut()
                .find(|s| s.info.name == ty.name)
            {
                existing.sources.push((path.clone(), ty.copy_count));
                existing.detected = Some(existing.sources.iter().map(|(_, n)| *n).sum());
            } else {
                self.recon_rework.push(ReconSlot {
                    selected_triggers: unit.suggested_triggers.clone(),
                    restore_start: unit
                        .suggested_restore()
                        .map(|c| c.name.clone())
                        .unwrap_or_default(),
                    info: unit,
                    kind: UnitKind::Armor,
                    influence: self.recon_percent,
                    detected: Some(ty.copy_count),
                    sources: vec![(path.clone(), ty.copy_count)],
                    path: path.clone(),
                });
            }
        }
        Ok(())
    }

    fn add_placed_packs(&mut self) {
        let Some(paths) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        let mut errors = Vec::new();
        let mut added = 0usize;
        for path in paths {
            match self.add_placed_from_path(path.clone()) {
                Ok(()) => added += 1,
                Err(err) => {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file");
                    errors.push(format!("{name}: {err}"));
                }
            }
        }
        if added == 0 {
            self.status = Status::Error(
                errors
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| "No placed pack was added.".into()),
            );
            return;
        }
        let types = self.recon_rework.len();
        let copies: usize = self.recon_rework.iter().filter_map(|s| s.detected).sum();
        let extra = if errors.is_empty() {
            String::new()
        } else {
            format!(" Some files were skipped: {}", errors.join("; "))
        };
        self.status = Status::Info(format!(
            "Now {types} unit type(s), {copies} groups on the map.{extra}"
        ));
    }

    fn generate_rework_file(&mut self) {
        if self.recon_rework.is_empty() {
            self.status = Status::Error("Add a placed pack first.".into());
            return;
        }
        for slot in &self.recon_rework {
            if self.recon_strip_randomizer {
                if slot.restore_start.is_empty() {
                    self.status = Status::Error(format!(
                        "Select a start timer or checkzone for {}.",
                        slot.info.name
                    ));
                    return;
                }
            } else if slot.selected_triggers.is_empty() {
                self.status = Status::Error(format!("Select a Zone In for {}.", slot.info.name));
                return;
            }
        }
        let mut paths = Vec::new();
        for slot in &self.recon_rework {
            for (path, _) in &slot.sources {
                if !paths.iter().any(|p| p == path) {
                    paths.push(path.clone());
                }
            }
            if slot.sources.is_empty() && !paths.iter().any(|p| p == &slot.path) {
                paths.push(slot.path.clone());
            }
        }
        let mut roots = Vec::new();
        for path in &paths {
            match load_group(path) {
                Ok(root) => roots.push(root),
                Err(err) => {
                    self.status = Status::Error(err);
                    return;
                }
            }
        }
        let keep_types: Vec<String> = self
            .recon_rework
            .iter()
            .map(|s| s.info.name.clone())
            .collect();
        let mut combined = match combine_placed_packs(&roots, &keep_types) {
            Ok(g) => g,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        if self.recon_strip_randomizer {
            let type_starts: Vec<(String, String)> = self
                .recon_rework
                .iter()
                .map(|s| (s.info.name.clone(), s.restore_start.clone()))
                .collect();
            let n = match restore_always_on(&mut combined, &type_starts) {
                Ok(n) => n,
                Err(err) => {
                    self.status = Status::Error(err);
                    return;
                }
            };
            let terrain_note = self.apply_terrain(&mut combined);
            let text = serialize_group(&combined);
            let suggested = format!("Ground_Units_{n}_always_on.Group");
            let Some(save_path) = dialog::FileDialog::new()
                .add_filter("IL-2 Group", &["Group"])
                .set_file_name(&suggested)
                .save_file()
            else {
                return;
            };
            self.status = save_with_sidecars(
                &save_path,
                &text,
                &paths,
                &format!(
                    "Removed randomizer from {n} groups. Mission Begin fires the selected start MCU on each copy."
                ),
            );
            self.add_terrain_note(terrain_note);
            return;
        }
        let type_percents: Vec<(String, u32)> = self
            .recon_rework
            .iter()
            .map(|s| (s.info.name.clone(), s.influence.clamp(1, 100)))
            .collect();
        let start_s = self.recon_start_delay_s as f64;
        let delay_s = self.recon_group_delay_ms as f64 / 1000.0;
        let n = match apply_randomizer_typed(&mut combined, &type_percents, start_s, delay_s) {
            Ok(n) => n,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let live: usize = self
            .recon_rework
            .iter()
            .map(|s| wanted_winners(s.detected.unwrap_or(0), s.influence.clamp(1, 100)))
            .sum();
        let terrain_note = self.apply_terrain(&mut combined);
        let text = serialize_group(&combined);
        let suggested = format!("Random_Ground_Units_{live}of{n}.Group");
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };
        let delay_note = recon_delay_note(self.recon_start_delay_s, self.recon_group_delay_ms);
        self.status = save_with_sidecars(
            &save_path,
            &text,
            &paths,
            &format!("Reworked {n} groups (exactly {live} live{delay_note})"),
        );
        self.add_terrain_note(terrain_note);
    }

    fn load_airfield(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group / mission", &["Group", "group", "Mission", "mission"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(err) => {
                self.status = Status::Error(format!("Could not read file: {err}"));
                return;
            }
        };
        match parse_il2_document(&text) {
            Ok(root) => {
                let info = inspect_airfield(&root);
                let players = info.player_planes.len();
                let zones = info.unlink_zones.len();
                self.status = if players == 0 {
                    Status::Info(format!(
                        "Loaded {} — no player aircraft found.",
                        info.name
                    ))
                } else {
                    Status::Info(format!(
                        "Loaded {}: {players} player aircraft, {zones} checkzones to unlink.",
                        info.name
                    ))
                };
                self.airfield_info = Some(info);
                self.airfield_root = Some(root);
                self.airfield_path = Some(path);
            }
            Err(err) => {
                self.status = Status::Error(format!("Parse failed: {err}"));
            }
        }
    }

    /// Airfield tab, left panel: the automatic database harvest (watch the
    /// game's _gen.mission and file every start airfield).
    fn harvest_section(&mut self, ui: &mut egui::Ui) {
        shell::section_title(ui, "Harvest automatically", Some("many airfields"));
        shell::hint(
            ui,
            "Instead of the steps above: watch, then start a Freeflight from each airfield in turn. Each new _gen.mission is cut, cleaned for multiplayer and filed in the database.",
            false,
        );
        ui.add_space(4.0);
        let mut watching = self.harvest_watcher.is_some();
        if ui
            .checkbox(&mut watching, "Watch for new airfields")
            .on_hover_text("Polls the Missions folder for a rewritten _gen.mission, on any tab.")
            .changed()
        {
            if watching {
                self.start_harvest_watch();
            } else {
                self.harvest_watcher = None;
            }
        }
        if let Some(w) = &self.harvest_watcher {
            ui.label(RichText::new(format!("Watching {}", w.dir().display())).small().color(c::ACCENT_700));
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Harvest current").on_hover_text("Harvest the _gen.mission in the Missions folder now").clicked() {
                self.harvest_current_gen();
            }
            if ui.button("Harvest a file…").on_hover_text("Harvest any .Mission file").clicked() {
                if let Some(path) = dialog::FileDialog::new()
                    .add_filter("IL-2 mission", &["Mission", "mission"])
                    .pick_file()
                {
                    self.run_harvest(&path);
                }
            }
        });
        ui.add_space(4.0);
        let folder_row = |ui: &mut egui::Ui, label: &str, value: &mut String| {
            ui.label(RichText::new(label).small().color(c::NEUTRAL_700));
            // Right to left: the button takes its real width, the path the rest,
            // so the row never overflows the 290 px panel.
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 28.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui.button("Browse…").clicked() {
                        if let Some(dir) = dialog::FileDialog::new().pick_folder() {
                            *value = dir.display().to_string();
                        }
                    }
                    ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width()));
                },
            );
        };
        folder_row(ui, "Game Missions folder", &mut self.harvest_missions_dir);
        folder_row(ui, "Database folder", &mut self.harvest_db_dir);
        ui.horizontal(|ui| {
            ui.label("Radius");
            ui.add(
                egui::DragValue::new(&mut self.harvest_cfg.radius_m)
                    .range(1000.0..=10000.0)
                    .speed(50.0)
                    .suffix(" m"),
            )
            .on_hover_text("Objects this close to the field are cut out with it; logic further out is kept through links.");
            ui.checkbox(&mut self.harvest_cfg.keep_ai_planes, "Keep AI planes");
        });
    }

    /// Airfield tab, center: what the last harvests did (newest first).
    fn harvest_log_panel(&mut self, ui: &mut egui::Ui) {
        if self.harvest_log.is_empty() {
            return;
        }
        ui.add_space(12.0);
        shell::blueprint(ui, c::DIVIDER, |ui| {
            shell::section_title(ui, "Database harvest", Some("newest first"));
            for line in self.harvest_log.iter().take(12) {
                ui.add(egui::Label::new(RichText::new(line).monospace()).wrap());
            }
        });
    }

    fn harvest_dir(&self) -> PathBuf {
        PathBuf::from(self.harvest_missions_dir.trim())
    }

    fn start_harvest_watch(&mut self) {
        let dir = self.harvest_dir();
        if !dir.is_dir() {
            self.status = Status::Error(format!("Missions folder not found: {}", dir.display()));
            return;
        }
        self.harvest_watcher = Some(GenWatcher::new(dir));
        self.status = Status::Info(
            "Watching for _gen.mission. Start a Freeflight from the next airfield.".into(),
        );
    }

    fn harvest_current_gen(&mut self) {
        let dir = self.harvest_dir();
        let Some(path) = find_gen_file(&dir) else {
            self.status = Status::Error(format!("No _gen.mission in {}", dir.display()));
            return;
        };
        self.run_harvest(&path);
        if let Some(w) = &mut self.harvest_watcher {
            w.acknowledge_current();
        }
    }

    /// Poll the watcher each frame; restart it if the folder field changed.
    fn poll_harvest(&mut self, ctx: &egui::Context) {
        let dir = self.harvest_dir();
        let Some(w) = &mut self.harvest_watcher else {
            return;
        };
        if w.dir() != dir.as_path() {
            *w = GenWatcher::new(dir);
        }
        if let Some(path) = w.poll(std::time::Instant::now()) {
            // The watcher runs on every tab; it reports on the Airfield tab's status.
            self.with_tab_status(AppMode::Airfield, |s| s.run_harvest(&path));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// Runs `f` with `mode`'s status as `self.status`, so what it reports
    /// lands on that tab even when another tab is shown.
    fn with_tab_status(&mut self, mode: AppMode, f: impl FnOnce(&mut Self)) {
        if mode == self.mode {
            f(self);
            return;
        }
        let slot = mode_slot(mode);
        let shown = std::mem::replace(&mut self.status, std::mem::take(&mut self.tab_status[slot]));
        f(self);
        self.tab_status[slot] = std::mem::replace(&mut self.status, shown);
    }

    fn run_harvest(&mut self, source: &Path) {
        let db = PathBuf::from(self.harvest_db_dir.trim());
        if let Err(err) = std::fs::create_dir_all(&db) {
            self.status = Status::Error(format!("Could not create {}: {err}", db.display()));
            return;
        }
        match harvest_file(source, &db, &self.harvest_cfg) {
            Ok(out) => {
                self.status = Status::Info(format!(
                    "Harvested {} airfield(s) into {}.",
                    out.airfields.len(),
                    db.display()
                ));
                for line in harvest_log_lines(&out).into_iter().rev() {
                    self.harvest_log.insert(0, line);
                }
            }
            Err(err) => {
                self.harvest_log.insert(0, format!("FAILED: {err}"));
                self.status = Status::Error(format!("Harvest failed: {err}"));
            }
        }
        self.harvest_log.truncate(50);
    }

    fn export_airfield(&mut self) {
        let Some(root) = self.airfield_root.clone() else {
            self.status = Status::Error("Load an airfield first.".into());
            return;
        };
        let mut cleaned = root;
        let coalitions = if self.airfield_western {
            WESTERN_PLANE_COALITIONS
        } else {
            EASTERN_PLANE_COALITIONS
        };
        let report = match clean_airfield(&mut cleaned, coalitions) {
            Ok(r) => r,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        if cleaned.block_type == "Group" {
            if matches!(cleaned.name(), Some("Group") | Some("Airfield") | None) {
                if let Some(stem) = self
                    .airfield_path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                {
                    cleaned.set_name(stem);
                }
            }
        }
        let text = serialize_group(&cleaned);
        let suggested = self
            .airfield_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .map(|stem| format!("{stem}_mp.Group"))
            .unwrap_or_else(|| "Airfield_mp.Group".into());
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };
        let locale = self
            .airfield_path
            .as_ref()
            .map(|p| vec![p.clone()])
            .unwrap_or_default();
        let summary = format!(
            "Exported airfield (stripped {} objects, unlinked {} checkzones to {})",
            report.stripped, report.unlinked_checkzones, report.plane_coalitions
        );
        self.status = save_with_sidecars(&save_path, &text, &locale, &summary);
    }

    fn generate_front_file(&mut self) {
        let fighter_packs = match self.build_map_fighter_packs() {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let mut ship_packs = match self.build_map_ship_packs() {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let mut ground_packs = match self.build_map_ground_packs() {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        match self.build_loaded_army_packs() {
            Ok((loaded_ships, loaded_ground)) => {
                ship_packs.extend(loaded_ships);
                ground_packs.extend(loaded_ground);
            }
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let opts = FrontOptions {
            year: self.front_year,
            season: self.front_season,
            aabb: self.front_aabb,
            front: true,
            battles: false,
            buildups: false,
            defenses: false,
            attacks: false,
            naval: false,
            influence: true,
            ref_groups: self.map_refs.clone(),
            battle_focus: self.front_focus,
            timeline_idx: Some(self.front_t.round() as usize),
            custom_front: if self.custom_front_xz.is_empty() {
                None
            } else {
                Some(self.custom_front_xz.clone())
            },
			salients: self.salients.clone(),
            user_attacks: self.attack_arrows.clone(),
            fighter_packs,
            ship_packs,
            ground_packs,
        };
        let mut pack = match generate_front(&opts) {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let terrain_note = self.apply_terrain(&mut pack.root);
        let text = serialize_group(&pack.root);
        let suggested = format!("Korea_BaseMap_{}.Group", self.current_mark().date_label());
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };
        if let Err(err) = std::fs::write(&save_path, text) {
            self.status = Status::Error(format!("Could not write file: {err}"));
            return;
        }
        let mut paths: Vec<PathBuf> = self.map_refs.iter().map(|g| g.path.clone()).collect();
        paths.extend(self.recon_slots.iter().map(|s| s.path.clone()));
        paths.extend(self.map_armies.iter().map(|a| a.path.clone()));
        let mut tables = merge_template_sidecars(&paths);
        for ext in LANG_EXTS {
            tables
                .entry((*ext).to_string())
                .or_default()
                .overlay(pack.locale.clone());
        }
        let file = save_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        match write_sidecars(&save_path, &tables) {
            Ok(exts) => {
                let extra = if pack.notes.is_empty() {
                    String::new()
                } else {
                    format!(" {}", pack.notes.join(" "))
                };
                self.status = Status::Info(format!(
                    "Wrote base map ({} objects, {}) plus {} to {file}.{extra} {} Aircraft: {}. {}",
                    pack.icon_count,
                    pack.period_label,
                    if exts.is_empty() {
                        "no language files".into()
                    } else {
                        exts.join("/")
                    },
                    pack.period_note,
                    pack.aircraft.iter().map(|a| a.label).collect::<Vec<_>>().join(", "),
                    pack.clip_preview.replace('\n', " "),
                ));
            }
            Err(err) => {
                self.status = Status::Error(format!("Wrote the group, but language files failed: {err}"));
            }
        }
        self.add_terrain_note(terrain_note);
    }

    fn stamp_imported_fighters(&self) -> Result<Vec<MapFighterPack>, String> {
        let layout = self.map_fighters.as_ref();
        let mut out = Vec::new();
        for (pack_i, src) in self.map_imported_fighters.iter().enumerate() {
            let mut root = src.root.clone();
            if let Some(layout) = layout {
                let mut members: Vec<&FighterSpot> = layout
                    .spots
                    .iter()
                    .filter(|s| s.pack == pack_i as u32)
                    .collect();
                members.sort_by_key(|s| s.slot);
                let mut gi = 0usize;
                for child in &mut root.children {
                    if child.block_type != "Group" {
                        continue;
                    }
                    if !child.name().is_some_and(|n| n.starts_with("Group ")) {
                        continue;
                    }
                    if let Some(spot) = members.get(gi) {
                        let from = group_anchor_xz(child);
                        crate::placement::move_anchor_to(child, from, (spot.x, spot.z));
                    }
                    gi += 1;
                }
            }
            out.push(MapFighterPack { root });
        }
        Ok(out)
    }

    fn load_base_map(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(err) => {
                self.status = Status::Error(format!("Could not read {}: {err}", path.display()));
                return;
            }
        };
        let entity = match parse_group_file(&text).or_else(|_| parse_il2_document(&text)) {
            Ok(e) => e,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let imported = match inspect_base_map(&entity) {
            Ok(m) => m,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        if let Some(aabb) = imported.aabb {
            self.front_aabb = aabb;
        }
        self.custom_front_xz = imported.front;
        self.salients = imported.salients;
        self.current_salient.clear();
        self.attack_arrows = imported.attack_arrows;
        self.attack_drag = None;
        self.front_stroke = None;
        self.drawn_marks.clear();
        self.clear_redo_stack();
        for _ in &self.salients {
            self.drawn_marks.push(DrawnMark::Salient);
        }
        for _ in &self.attack_arrows {
            self.drawn_marks.push(DrawnMark::AttackArrow);
        }
        if let Some(y) = imported.year {
            self.front_year = y;
        }
        if let Some(s) = imported.season {
            self.front_season = s;
        }
        if let Some(idx) = imported.timeline_idx {
            self.front_t = idx.min(TIMELINE.len().saturating_sub(1)) as f32;
        }
        self.east_objectives.clear();
        self.nato_objectives.clear();
        self.objective_drag = None;

        self.map_armies.clear();
        self.map_ships = None;
        self.map_ground_east = None;
        self.map_ground_nato = None;
        self.ship_drag = None;
        self.ship_heading_drag = None;
        self.ground_drag = None;
        self.ground_heading_drag = None;
        self.wp_drag = None;
        self.wp_selected = None;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("BaseMap");
        for army in imported.armies {
            let copies = inspect_army_copies(&army.entity);
            if copies.is_empty() {
                continue;
            }
            let slot_path = path.with_file_name(format!("{stem}-{}.Group", army.name));
            self.map_armies.push(MapArmySlot {
                path: slot_path,
                entity: army.entity,
                eastern: army.eastern,
                reposition: false,
                copies,
                ground: None,
                ships: None,
            });
            let idx = self.map_armies.len() - 1;
            self.refresh_army_slot(idx);
        }

        self.map_imported_fighters = imported.fighters;
        if self.map_imported_fighters.is_empty() {
            self.map_fighters = None;
        } else {
            let eastern = self.map_imported_fighters[0].eastern;
            let mut spots = Vec::new();
            for (pack_i, pack) in self.map_imported_fighters.iter().enumerate() {
                for (slot_i, &(x, z)) in pack.spots.iter().enumerate() {
                    spots.push(FighterSpot {
                        x,
                        z,
                        wave: pack.wave,
                        pack: pack_i as u32,
                        slot: (slot_i + 1) as u32,
                    });
                }
            }
            self.map_fighters = Some(MapFighterLayout { eastern, spots });
        }
        self.fighter_drag = None;
        self.map_refs = imported.refs;

        let mut parts = Vec::new();
        if imported.aabb.is_some() {
            parts.push("AO".into());
        }
        if self.custom_front_xz.len() >= 2 {
            parts.push("front".into());
        }
        if !self.attack_arrows.is_empty() {
            parts.push(format!("{} attack arrow(s)", self.attack_arrows.len()));
        }
        if !self.salients.is_empty() {
            parts.push(format!("{} salient(s)", self.salients.len()));
        }
        let units = self
            .map_armies
            .iter()
            .map(|a| a.copies.len())
            .sum::<usize>();
        if units > 0 {
            parts.push(format!("{units} unit group(s) at exported positions"));
        }
        let fighters = self
            .map_fighters
            .as_ref()
            .map(|l| l.spots.len())
            .unwrap_or(0);
        if fighters > 0 {
            parts.push(format!("{fighters} fighter group(s)"));
        }
        if !self.map_refs.is_empty() {
            parts.push(format!("{} reference group(s)", self.map_refs.len()));
        }
        let extra = if imported.notes.is_empty() {
            String::new()
        } else {
            format!(" {}", imported.notes.join(" "))
        };
        self.status = Status::Info(format!(
            "Loaded base map ({}). Objectives are preview-only and were not in the file.{extra}",
            if parts.is_empty() {
                "no layers".into()
            } else {
                parts.join(", ")
            }
        ));
    }

    fn add_map_refs(&mut self) {
        let Some(paths) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        let mut added = 0usize;
        let mut errors = Vec::new();
        for path in paths {
            if self.map_refs.iter().any(|g| g.path == path) {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => match parse_il2_document(&text) {
                    Ok(entity) => {
                        self.map_refs.push(MapRefGroup { path, entity });
                        added += 1;
                    }
                    Err(err) => errors.push(err),
                },
                Err(err) => errors.push(format!("Could not read {}: {err}", path.display())),
            }
        }
        if !errors.is_empty() {
            self.status = Status::Error(format!(
                "Added {added} group(s). Some files were skipped: {}",
                errors.join("; ")
            ));
        } else if added == 0 {
            self.status = Status::Info("No new groups added.".into());
                } else {
                    self.status = Status::Info(format!(
                        "Added {added} reference group(s). Landscape MARKS (MCU_Waypoint) show as nested dots on the preview and are not written into the generated group."
                    ));
                }
    }

    fn generate_fighter_file(&mut self) {
        let root = match self.configured_fighter_root(self.country) {
            Ok(e) => e,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };

        let mut generated = match generate_pack(&root, self.linked_groups as usize) {
            Ok(g) => g,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        apply_overrides(&mut generated, "", self.country);
        generated.set_name(&linked_fighter_pack_name(
            self.country,
            self.linked_groups as usize,
        ));
        let terrain_note = self.apply_terrain(&mut generated);
        let text = serialize_group(&generated);

        let suggested = fighter_pack_filename(self.country, self.linked_groups as usize);
        let Some(save_path) = dialog::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };

        match std::fs::write(&save_path, text) {
            Ok(()) => {
                self.status = Status::Info(format!(
                    "Wrote a {}-pack, {} flights of {} ({}) to {}.",
                    self.linked_groups,
                    self.flight_count,
                    self.max_in_flight,
                    COUNTRIES
                        .iter()
                        .find(|(id, _)| *id == self.country)
                        .map(|(_, l)| *l)
                        .unwrap_or("?"),
                    save_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                ));
            }
            Err(err) => {
                self.status = Status::Error(format!("Could not write file: {err}"));
            }
        }
        self.add_terrain_note(terrain_note);
    }
}

fn preview_dot_style(kind: PreviewKind, in_box: bool) -> (Color32, f32) {
    match kind {
        PreviewKind::Airfield => {
            let color = if in_box {
                Color32::from_rgb(30, 90, 220) // Changed to Blue
            } else {
                Color32::from_rgba_unmultiplied(30, 90, 220, 90)
            };
            (color, if in_box { 3.2 } else { 2.2 })
        }
        PreviewKind::LinkedEntity => {
            let color = if in_box {
                Color32::from_rgb(255, 140, 40) // Remains Orange
            } else {
                Color32::from_rgba_unmultiplied(255, 140, 40, 90)
            };
            (color, if in_box { 2.6 } else { 1.8 })
        }
        PreviewKind::Block => {
            let color = if in_box {
                BLOCK_DOT
            } else {
                BLOCK_DOT.gamma_multiply(0.35)
            };
            (color, if in_box { 2.4 } else { 1.7 })
        }
    }
}

fn move_row_button(ui: &mut egui::Ui, up: bool) -> egui::Response {
    let size = Vec2::splat(28.0); // minimum target (README §6.6)
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), if up { "Move up" } else { "Move down" }));
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect.shrink(0.5),
        2.0,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let h = 4.5_f32;
    let w = 4.5_f32;
    let pts = if up {
        vec![
            Pos2::new(c.x, c.y - h),
            Pos2::new(c.x - w, c.y + h * 0.55),
            Pos2::new(c.x + w, c.y + h * 0.55),
        ]
    } else {
        vec![
            Pos2::new(c.x, c.y + h),
            Pos2::new(c.x - w, c.y - h * 0.55),
            Pos2::new(c.x + w, c.y - h * 0.55),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, visuals.text_color(), Stroke::NONE));
    response
}

fn move_col_button(ui: &mut egui::Ui, left: bool) -> egui::Response {
    let size = Vec2::splat(28.0); // minimum target (README §6.6)
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), if left { "Move left" } else { "Move right" }));
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect.shrink(0.5),
        2.0,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let h = 4.5_f32;
    let w = 4.5_f32;
    let pts = if left {
        vec![
            Pos2::new(c.x - h, c.y),
            Pos2::new(c.x + h * 0.55, c.y - w),
            Pos2::new(c.x + h * 0.55, c.y + w),
        ]
    } else {
        vec![
            Pos2::new(c.x + h, c.y),
            Pos2::new(c.x - h * 0.55, c.y - w),
            Pos2::new(c.x - h * 0.55, c.y + w),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, visuals.text_color(), Stroke::NONE));
    response
}

fn skip_only_warnings(warnings: &[String]) -> Vec<String> {
    warnings
        .iter()
        .filter(|w| w.contains("skipped"))
        .cloned()
        .collect()
}

/// "1 salient" / "3 salients".
fn count_noun(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Legend entry for a dot layer: a small filled square.
fn legend_swatch(ui: &mut egui::Ui, color: Color32, label: &str) {
    legend_entry(ui, label, |p, r| {
        p.rect_filled(r.shrink(2.0), 1.0, color);
    });
}

/// Legend entry for a line layer: a 14 px line, solid or dashed.
fn legend_line(ui: &mut egui::Ui, color: Color32, dashed: bool, label: &str) {
    legend_entry(ui, label, |p, r| {
        let (a, b) = (r.left_center(), r.right_center());
        let stroke = Stroke::new(2.0_f32, color);
        if dashed {
            p.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.5_f32, color), 4.0, 3.0));
        } else {
            p.line_segment([a, b], stroke);
        }
    });
}

/// Legend entry for a side: its marker, turned the side's way.
fn legend_side(ui: &mut egui::Ui, side: shell::Side, label: &str) {
    legend_entry(ui, label, |p, r| shell::paint_side_marker(p, r.center(), 12.0, side));
}

fn legend_entry(ui: &mut egui::Ui, label: &str, paint: impl FnOnce(&egui::Painter, Rect)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
        paint(ui.painter(), rect);
        ui.label(RichText::new(label).small().color(c::TEXT));
    });
}

fn fighter_icon_button(ui: &mut egui::Ui, tex: Option<&TextureHandle>, fallback: &str) -> bool {
    let hover = format!("Place {fallback} fighters in their coalition zone");
    if let Some(tex) = tex {
        ui.add(egui::ImageButton::new((tex.id(), Vec2::splat(26.0))))
            .on_hover_text(hover)
            .clicked()
    } else {
        ui.button(fallback).on_hover_text(hover).clicked()
    }
}

/// Army Generator type button (README §5.2): 36 × 28, the type texture in
/// the middle, the type name on hover and in the accessibility label.
fn unit_kind_icon_button(
    ui: &mut egui::Ui,
    tex: Option<&TextureHandle>,
    fallback: &str,
    hover: &str,
    selected: bool,
) -> bool {
    const SIZE: Vec2 = Vec2::new(36.0, 28.0);
    let Some(tex) = tex else {
        return ui
            .add(egui::Button::selectable(selected, fallback).min_size(SIZE))
            .on_hover_text(hover)
            .clicked();
    };
    let (rect, resp) = ui.allocate_exact_size(SIZE, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, fallback));
    if ui.is_rect_visible(rect) {
        let (fill, border) = if selected {
            (c::ACCENT_200, c::ACCENT)
        } else if resp.hovered() {
            (c::ACCENT_100, c::ACCENT)
        } else {
            (Color32::TRANSPARENT, c::DIVIDER)
        };
        let p = ui.painter();
        p.rect_filled(rect, 2.0, fill);
        p.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
        egui::Image::new((tex.id(), Vec2::splat(20.0))).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(20.0)));
    }
    resp.on_hover_text(hover).clicked()
}

fn map_icon_button(
    ui: &mut egui::Ui,
    tex: Option<&TextureHandle>,
    fallback: &str,
    hover: &str,
) -> bool {
    if let Some(tex) = tex {
        ui.add(egui::ImageButton::new((tex.id(), Vec2::splat(26.0))))
            .on_hover_text(hover)
            .clicked()
    } else {
        ui.button(fallback).on_hover_text(hover).clicked()
    }
}

fn paint_rotated_image(
    painter: &egui::Painter,
    tex: &TextureHandle,
    center: Pos2,
    size: Vec2,
    angle_rad: f32,
    tint: Color32,
) {
    let (s, c) = angle_rad.sin_cos();
    let hx = size.x * 0.5;
    let hy = size.y * 0.5;
    let corners = [
        (Vec2::new(-hx, -hy), Pos2::new(0.0, 0.0)),
        (Vec2::new(hx, -hy), Pos2::new(1.0, 0.0)),
        (Vec2::new(hx, hy), Pos2::new(1.0, 1.0)),
        (Vec2::new(-hx, hy), Pos2::new(0.0, 1.0)),
    ];
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (off, uv) in corners {
        let rot = Vec2::new(off.x * c - off.y * s, off.x * s + off.y * c);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + rot,
            uv,
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

fn load_fighter_svg(bytes: &[u8]) -> ColorImage {
    rasterize_svg(bytes, 128)
}

/// A one-colour side icon (`assets/Eastern*.svg` / `Nato*.svg`) recoloured
/// on screen to its side token (README §8). The SVG colours themselves, and
/// every colour written into generated files, stay as they are.
fn load_side_svg(bytes: &[u8], eastern: bool) -> ColorImage {
    recolor_to_side(rasterize_svg(bytes, 128), eastern)
}

fn recolor_to_side(mut img: ColorImage, eastern: bool) -> ColorImage {
    let color = shell::Side::from_eastern(eastern).color();
    for p in &mut img.pixels {
        let a = p.a() as f32 / 255.0;
        let ch = |v: u8| (v as f32 * a).round() as u8;
        *p = Color32::from_rgba_premultiplied(ch(color.r()), ch(color.g()), ch(color.b()), p.a());
    }
    img
}

/// The fighter SVGs are drawn diagonally: `EasternFighter.svg` points
/// north-west and `NatoFighter.svg` north-east. Turning them by these angles
/// (degrees, clockwise) when rasterizing makes both textures point north, the
/// orientation `shell::Side::facing_rad` assumes.
const FIGHTER_SVG_TO_NORTH_DEG: [f32; 2] = [45.0, -45.0];

/// A fighter icon rasterized pointing north, in its authored colour.
fn fighter_svg_north(eastern: bool, target: u32) -> ColorImage {
    if eastern {
        rasterize_svg_turned(include_bytes!("../assets/EasternFighter.svg"), target, FIGHTER_SVG_TO_NORTH_DEG[0])
    } else {
        rasterize_svg_turned(include_bytes!("../assets/NatoFighter.svg"), target, FIGHTER_SVG_TO_NORTH_DEG[1])
    }
}

/// Registers the side-marker silhouettes once (`shell::paint_side_marker`).
/// 64 px with mipmaps stays crisp from 10 to 18 px, also on HiDPI screens.
fn ensure_side_textures(ctx: &egui::Context) {
    if shell::side_textures(ctx).is_none() {
        shell::register_side_textures(ctx, side_marker_image(true), side_marker_image(false));
    }
}

/// The list-size side marker: the fighter without its ring, cropped to the
/// plane, so the silhouette and its facing still read at 12–18 px.
fn side_marker_image(eastern: bool) -> ColorImage {
    let svg: &[u8] = if eastern {
        include_bytes!("../assets/EasternFighter.svg")
    } else {
        include_bytes!("../assets/NatoFighter.svg")
    };
    let text = String::from_utf8_lossy(svg);
    let no_ring = match text.find("<circle") {
        Some(start) => match text[start..].find("/>") {
            Some(end) => format!("{}{}", &text[..start], &text[start + end + 2..]),
            None => text.into_owned(),
        },
        None => text.into_owned(),
    };
    let deg = FIGHTER_SVG_TO_NORTH_DEG[if eastern { 0 } else { 1 }];
    crop_to_content(rasterize_svg_turned(no_ring.as_bytes(), 96, deg))
}

/// Crops an image to its opaque pixels, centred in a square with a 1 px margin.
fn crop_to_content(img: ColorImage) -> ColorImage {
    let [w, h] = img.size;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if img.pixels[y * w + x].a() > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 || y1 < y0 {
        return img;
    }
    let side = (x1 - x0).max(y1 - y0) + 3;
    let (ox, oy) = ((x0 + x1) / 2, (y0 + y1) / 2);
    let mut out = ColorImage::new([side, side], vec![Color32::TRANSPARENT; side * side]);
    for y in 0..side {
        for x in 0..side {
            let sx = ox as isize + x as isize - side as isize / 2;
            let sy = oy as isize + y as isize - side as isize / 2;
            if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                out.pixels[y * side + x] = img.pixels[sy as usize * w + sx as usize];
            }
        }
    }
    out
}

fn rasterize_svg(bytes: &[u8], target: u32) -> ColorImage {
    rasterize_svg_turned(bytes, target, 0.0)
}

/// Rasterizes an SVG to `target` px on its longer side, turned `deg`
/// degrees clockwise about its centre (the icons are round, so nothing clips).
fn rasterize_svg_turned(bytes: &[u8], target: u32, deg: f32) -> ColorImage {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .expect("fighter SVG in assets/ is not valid");
    let size = tree.size().to_int_size();
    let src_w = size.width().max(1);
    let src_h = size.height().max(1);
    let scale = target as f32 / src_w.max(src_h) as f32;
    let w = ((src_w as f32) * scale).round().max(1.0) as u32;
    let h = ((src_h as f32) * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).expect("fighter SVG pixmap");
    let transform = resvg::tiny_skia::Transform::from_scale(
        w as f32 / tree.size().width(),
        h as f32 / tree.size().height(),
    )
    .post_rotate_at(deg, w as f32 / 2.0, h as f32 / 2.0);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    ColorImage::from_rgba_premultiplied([w as usize, h as usize], pixmap.data())
}

enum KoreaMapLayer {
    Overview(ColorImage),
    Detail(ColorImage),
}

fn load_korea_jpeg(bytes: &[u8], path: &str) -> ColorImage {
    let img = image::load_from_memory(bytes)
        .unwrap_or_else(|_| panic!("{path} is not a valid JPEG"))
        .to_rgba8();
    let (w, h) = img.dimensions();
    ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw())
}

fn load_model_png(bytes: &[u8]) -> ColorImage {
    decode_png(bytes)
        .or_else(|| decode_png(model_spec::PLACEHOLDER_PNG))
        .unwrap_or_else(|| ColorImage::from_rgba_unmultiplied([1, 1], &[160, 160, 160, 255]))
}

fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        img.as_raw(),
    ))
}

/// Hypsometric ramp for the relief layer (metres → color).
fn relief_color(y: f32) -> Color32 {
    const STOPS: [(f32, [f32; 3]); 7] = [
        (0.5, [120.0, 160.0, 200.0]),
        (1.0, [92.0, 140.0, 90.0]),
        (150.0, [140.0, 170.0, 100.0]),
        (400.0, [205.0, 190.0, 130.0]),
        (800.0, [170.0, 120.0, 80.0]),
        (1200.0, [140.0, 110.0, 100.0]),
        (1800.0, [235.0, 235.0, 235.0]),
    ];
    if y < STOPS[0].0 {
        let [r, g, b] = STOPS[0].1;
        return Color32::from_rgb(r as u8, g as u8, b as u8);
    }
    for w in STOPS.windows(2) {
        let ((y0, a), (y1, b)) = (w[0], w[1]);
        if y <= y1 {
            let t = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
            let m = |i: usize| (a[i] + (b[i] - a[i]) * t) as u8;
            return Color32::from_rgb(m(0), m(1), m(2));
        }
    }
    let [r, g, b] = STOPS[STOPS.len() - 1].1;
    Color32::from_rgb(r as u8, g as u8, b as u8)
}

fn map_screen_rect(widget: Rect, view: Rect) -> Rect {
    let w = widget.width() / view.width().max(1e-6);
    let h = widget.height() / view.height().max(1e-6);
    Rect::from_min_size(
        Pos2::new(
            widget.left() - view.min.x * w,
            widget.top() - view.min.y * h,
        ),
        Vec2::new(w, h),
    )
}

fn pos_to_uv(rect: Rect, pos: Pos2) -> Pos2 {
    let u = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let v = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    Pos2::new(u, v)
}

/*fn uv_to_world(uv: Pos2) -> (f64, f64) {
    let x = MAP_MAX + (MAP_MIN - MAP_MAX) * uv.y as f64;
    let z = MAP_MIN + (MAP_MAX - MAP_MIN) * uv.x as f64;
    (x, z)
}

fn world_to_uv(x: f64, z: f64) -> Pos2 {
    let span = MAP_MAX - MAP_MIN;
    let u = ((z - MAP_MIN) / span).clamp(0.0, 1.0) as f32;
    let v = ((MAP_MAX - x) / span).clamp(0.0, 1.0) as f32;
    Pos2::new(u, v)
}*/

// Manually tune these four values to perfectly align the image to the world.
// Find a reference point near each edge of the map, note its physical game coordinate,
// and adjust these bounds until the overlay snaps perfectly into place.
const IMG_TOP_X: f64 = 499_200.0;    // Physical North coordinate of the image's top edge
const IMG_BOTTOM_X: f64 = 000.0;  // Physical South coordinate of the image's bottom edge
const IMG_LEFT_Z: f64 = 000.0;    // Physical West coordinate of the image's left edge
const IMG_RIGHT_Z: f64 = 499_200.0;  // Physical East coordinate of the image's right edge

fn uv_to_world(uv: Pos2) -> (f64, f64) {
    let x = MAP_MAX + (MAP_MIN - MAP_MAX) * uv.y as f64;
    let z = MAP_MIN + (MAP_MAX - MAP_MIN) * uv.x as f64;
    (x, z)
}

fn world_to_uv(x: f64, z: f64) -> Pos2 {
    let span = MAP_MAX - MAP_MIN;
    let u = ((z - MAP_MIN) / span) as f32; // Clamping removed
    let v = ((MAP_MAX - x) / span) as f32; // Clamping removed
    Pos2::new(u, v)
}

fn world_to_pos(rect: Rect, x: f64, z: f64) -> Pos2 {
    let uv = world_to_uv(x, z);
    Pos2::new(
        rect.left() + uv.x * rect.width(),
        rect.top() + uv.y * rect.height(),
    )
}

fn draw_front_inside_outside(
    painter: &egui::Painter,
    rect: Rect,
    pts: &[(f64, f64)],
    aabb: WorldAabb,
) {
    let outside = Stroke::new(1.15_f32, c::FRONT);
    let inside = Stroke::new(2.5_f32, c::FRONT);
    draw_world_line(painter, rect, pts, outside);
    let clipped = clip_linestring_to_rect(&points_to_linestring(pts), &aabb.as_rect());
    for run in &clipped {
        draw_world_line(painter, rect, &linestring_to_points(run), inside);
    }
}

fn near_map_dot(rect: Rect, world: (f64, f64), pointer: Pos2, radius_px: f32) -> bool {
    world_to_pos(rect, world.0, world.1).distance(pointer) <= radius_px
}

fn draw_salient_anchor(
    painter: &egui::Painter,
    rect: Rect,
    world: (f64, f64),
    finish: bool,
    hover: Option<Pos2>,
) {
    let pos = world_to_pos(rect, world.0, world.1);
    let hot = hover.is_some_and(|h| h.distance(pos) <= 14.0);
    let (fill, radius) = if finish {
        (
            Color32::from_rgb(110, 60, 8),
            if hot { 8.0_f32 } else { 6.5_f32 },
        )
    } else {
        (Color32::from_rgb(255, 230, 150), 6.0_f32)
    };
    painter.circle_filled(pos, radius + 1.4, Color32::from_rgb(20, 20, 24));
    painter.circle_filled(pos, radius, fill);
    if finish {
        painter.circle_stroke(
            pos,
            radius,
            Stroke::new(1.4_f32, Color32::from_rgb(70, 35, 0)),
        );
    }
}

fn aabb_from_uv(a: Pos2, b: Pos2) -> WorldAabb {
    let (x0, z0) = uv_to_world(a);
    let (x1, z1) = uv_to_world(b);
    WorldAabb::from_corners(x0, z0, x1, z1)
}

fn aabb_to_screen(rect: Rect, aabb: WorldAabb) -> Rect {
    let a = world_to_pos(rect, aabb.x_max, aabb.z_min);
    let b = world_to_pos(rect, aabb.x_min, aabb.z_max);
    Rect::from_two_pos(a, b)
}

fn draw_reference_overlays(painter: &egui::Painter, rect: Rect) {
    let parallel = geo::parallel_38_xz();
    draw_dashed_world_line(
        painter,
        rect,
        &parallel,
        Stroke::new(1.6_f32, Color32::from_rgb(240, 230, 170)),
    );
    if let Some(&(x, z)) = parallel.first() {
        draw_map_label(
            painter,
            world_to_pos(rect, x, z) + Vec2::new(4.0, -8.0),
            "38th parallel",
            Color32::from_rgb(240, 230, 170),
            Align2::LEFT_BOTTOM,
        );
    }

    for water in crate::geo::MAJOR_WATERWAYS {
        let pos = world_to_pos(rect, water.x, water.z);
        draw_map_label(
            painter,
            pos,
            water.name,
            Color32::from_rgb(120, 200, 230),
            Align2::CENTER_CENTER,
        );
    }

    for (city, x, z) in geo::cities_on_map() {
        let pos = world_to_pos(rect, x, z);
        let color = if city.dprk {
            Color32::from_rgb(230, 150, 150)
        } else {
            Color32::from_rgb(170, 200, 240)
        };
        painter.circle_filled(pos, 3.0_f32, color);
        painter.circle_stroke(pos, 3.0_f32, Stroke::new(1.0_f32, Color32::from_rgb(20, 20, 24)));
        let (align, offset) = if city.label_left {
            (Align2::RIGHT_CENTER, Vec2::new(-5.0, 0.0))
        } else {
            (Align2::LEFT_CENTER, Vec2::new(5.0, 0.0))
        };
        draw_map_label(painter, pos + offset, city.name, color, align);
    }
}

const STATUS_WARN: Color32 = Color32::from_rgb(220, 140, 40);

/// Map line colours shared by the map and its legend (on screen only).
const SALIENT_OUTLINE: Color32 = Color32::from_rgb(180, 180, 80);
/// 50 % dark red / near-black, premultiplied (roads.svg / railroads.svg overlays).
const ROAD_LINE: Color32 = Color32::from_rgba_premultiplied(55, 9, 9, 128);
const RAIL_LINE: Color32 = Color32::from_rgba_premultiplied(8, 8, 9, 128);

/// AO box wash: ACCENT (#5980a6) at ~7 %, premultiplied.
const AO_FILL: Color32 = Color32::from_rgba_premultiplied(6, 9, 12, 18);

/// Reference-group blocks on the map and in the legend: a neutral token, so
/// they no longer share the old AO yellow.
const BLOCK_DOT: Color32 = c::NEUTRAL_800;

/// On-screen side colour (tokens). The colours written into the base map
/// (`RColor` / `GColor` / `BColor` in frontlines.rs) are separate and unchanged.
fn faction_map_color(eastern: bool) -> Color32 {
    if eastern {
        c::DPRK
    } else {
        c::NATO
    }
}

fn draw_world_line(painter: &egui::Painter, rect: Rect, pts: &[(f64, f64)], stroke: Stroke) {
    for w in pts.windows(2) {
        painter.line_segment(
            [world_to_pos(rect, w[0].0, w[0].1), world_to_pos(rect, w[1].0, w[1].1)],
            stroke,
        );
    }
}

fn draw_network_lines(
    painter: &egui::Painter,
    rect: Rect,
    net: &mapnet::Network,
    stroke: Stroke,
) {
    let pad = 8.0_f32;
    let vis = rect.expand(pad);
    for line in &net.lines {
        let mut last: Option<Pos2> = None;
        for &(x, z) in &line.pts {
            let p = world_to_pos(rect, x, z);
            if let Some(prev) = last {
                if vis.intersects(Rect::from_two_pos(prev, p))
                    && prev.distance(p) >= 0.6
                {
                    painter.line_segment([prev, p], stroke);
                }
            }
            last = Some(p);
        }
    }
}

fn draw_preview_arrow(painter: &egui::Painter, rect: Rect, pts: &[(f64, f64)], color: Color32) {
    draw_world_line(painter, rect, pts, Stroke::new(2.2_f32, color));
}

fn draw_attack_shaft(
    painter: &egui::Painter,
    rect: Rect,
    tail: (f64, f64),
    tip: (f64, f64),
    color: Color32,
) {
    let stroke = Stroke::new(2.6_f32, color);
    painter.line_segment(
        [world_to_pos(rect, tail.0, tail.1), world_to_pos(rect, tip.0, tip.1)],
        stroke,
    );
    let dx = tip.0 - tail.0;
    let dz = tip.1 - tail.1;
    let len = (dx * dx + dz * dz).sqrt().max(1.0);
    let ux = dx / len;
    let uz = dz / len;
    let px = -uz;
    let pz = ux;
    let head = (len * 0.18).clamp(2_000.0, 8_000.0);
    let left = (
        tip.0 - ux * head + px * head * 0.42,
        tip.1 - uz * head + pz * head * 0.42,
    );
    let right = (
        tip.0 - ux * head - px * head * 0.42,
        tip.1 - uz * head - pz * head * 0.42,
    );
    painter.line_segment(
        [world_to_pos(rect, tip.0, tip.1), world_to_pos(rect, left.0, left.1)],
        stroke,
    );
    painter.line_segment(
        [world_to_pos(rect, tip.0, tip.1), world_to_pos(rect, right.0, right.1)],
        stroke,
    );
}

fn draw_dashed_world_line(painter: &egui::Painter, rect: Rect, pts: &[(f64, f64)], stroke: Stroke) {
    for (i, w) in pts.windows(2).enumerate() {
        if i % 2 == 0 {
            painter.line_segment(
                [world_to_pos(rect, w[0].0, w[0].1), world_to_pos(rect, w[1].0, w[1].1)],
                stroke,
            );
        }
    }
}

fn draw_map_label(painter: &egui::Painter, pos: Pos2, text: &str, color: Color32, align: Align2) {
    let font = FontId::new(12.0, FontFamily::Proportional); // 12 px minimum (README §6.6)
    painter.text(
        pos + Vec2::new(0.6, 0.6),
        align,
        text,
        font.clone(),
        Color32::from_rgb(16, 16, 20),
    );
    painter.text(pos, align, text, font, color);
}

fn group_files_in_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("group"))
        })
        .collect();
    paths.sort();
    paths
}

fn army_mix_label(copies: &[ArmyCopyInfo]) -> String {
    let mut counts = [0usize; 7];
    for c in copies {
        let i = match c.kind {
            ArmyUnitKind::Ship => 0,
            ArmyUnitKind::Armor => 1,
            ArmyUnitKind::Supply => 2,
            ArmyUnitKind::Artillery => 3,
            ArmyUnitKind::Infantry => 4,
            ArmyUnitKind::Train => 5,
            ArmyUnitKind::MobileArtillery => 6,
        };
        counts[i] += 1;
    }
    let mut parts = Vec::new();
    for (kind, n) in [
        (ArmyUnitKind::Ship, counts[0]),
        (ArmyUnitKind::Armor, counts[1]),
        (ArmyUnitKind::Supply, counts[2]),
        (ArmyUnitKind::Artillery, counts[3]),
        (ArmyUnitKind::Infantry, counts[4]),
        (ArmyUnitKind::Train, counts[5]),
        (ArmyUnitKind::MobileArtillery, counts[6]),
    ] {
        if n > 0 {
            parts.push(format!("{}×{n}", kind.label()));
        }
    }
    if parts.is_empty() {
        "empty".into()
    } else {
        parts.join(", ")
    }
}

fn load_group(path: &Path) -> Result<crate::ast::Il2Entity, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Could not read file: {err}"))?;
    parse_group_file(&text).map_err(|err| {
        // A flat "Save Selection to File" export has many root blocks and no Group.
        match parse_il2_document(&text) {
            Ok(root) if root.children.len() > 1 && root.name() == Some("Airfield") => format!(
                "{} holds {} separate blocks, not one Group. Group them in the mission editor, then save the group and add that file.",
                path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                root.children.len()
            ),
            _ => format!("Parse failed: {err}"),
        }
    })
}

fn drop_rework_path(slots: &mut Vec<ReconSlot>, path: &Path) {
    for slot in slots.iter_mut() {
        slot.sources.retain(|(p, _)| p != path);
        slot.detected = Some(slot.sources.iter().map(|(_, n)| *n).sum());
        if let Some((first, _)) = slot.sources.first() {
            slot.path = first.clone();
        }
    }
    slots.retain(|s| !s.sources.is_empty());
}

/// A fixed-width side panel whose contents scroll on their own (README §6.1).
/// Fighter Pack preview: group card and NodeGate link sizes.
const PACK_CARD_W: f32 = 120.0;
const PACK_CARD_H: f32 = 52.0;
const PACK_LINK_W: f32 = 70.0;

/// Whole numbers with a space every three digits: 227880 → "227 880".
fn group_digits(value: f64) -> String {
    let n = value.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

/// Which labels to draw along a strip: taken left to right, a label is
/// skipped when it would come within `gap` of the last drawn one.
fn strip_labels_shown(lefts: &[f32], widths: &[f32], gap: f32) -> Vec<bool> {
    let mut order: Vec<usize> = (0..lefts.len()).collect();
    order.sort_by(|a, b| lefts[*a].total_cmp(&lefts[*b]));
    let mut shown = vec![false; lefts.len()];
    let mut last_right = f32::NEG_INFINITY;
    for i in order {
        if lefts[i] >= last_right + gap {
            shown[i] = true;
            last_right = lefts[i] + widths[i];
        }
    }
    shown
}

/// Exclusive Activation card tags: what a plan lacks, else "⚠ Check" when a
/// selected zone or the end timer has a wiring warning, else "Ready".
fn plan_tags(slot: &BomberSlot) -> Vec<(&'static str, bool)> {
    let mut tags = Vec::new();
    if slot.selected_triggers.is_empty() {
        tags.push(("⚠ Checkzone", false));
    }
    if slot.selected_completion.is_none() {
        tags.push(("⚠ End timer", false));
    }
    if tags.is_empty() {
        let warned = slot.selected_triggers.iter().any(|id| slot.info.trigger_warnings.contains_key(id))
            || slot
                .selected_completion
                .is_some_and(|id| slot.info.cleanup_warnings.contains_key(&id));
        tags.push(if warned { ("⚠ Check", false) } else { ("Ready", true) });
    }
    tags
}

/// The first plan that cannot generate, as the idle status names it.
fn exclusive_first_problem(slots: &[BomberSlot]) -> Option<String> {
    slots.iter().enumerate().find_map(|(n, slot)| {
        if slot.selected_triggers.is_empty() {
            Some(format!("Plan {} has no start checkzone", n + 1))
        } else if slot.selected_completion.is_none() {
            Some(format!("Plan {} has no end timer", n + 1))
        } else {
            None
        }
    })
}

/// A plan card: `shell::card` plus the hover state (ACCENT_100 fill, ACCENT border).
fn plan_card<R>(ui: &mut egui::Ui, selected: bool, hovered: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (stroke, fill) = if selected || hovered {
        (Stroke::new(1.0_f32, c::ACCENT), c::ACCENT_100)
    } else {
        (Stroke::new(1.0_f32, c::DIVIDER), Color32::TRANSPARENT)
    };
    let inner = egui::Frame::new()
        .stroke(stroke)
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(11, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
    if selected {
        shell::corner_marks(ui.painter(), inner.response.rect, c::MARK);
    }
    inner.inner
}

/// Country name without its code: 601 → "USA".
fn country_name(country: i32) -> String {
    COUNTRIES
        .iter()
        .find(|(id, _)| *id == country)
        .and_then(|(_, label)| label.split_whitespace().last())
        .map_or_else(|| format!("country {country}"), str::to_owned)
}

/// A label on the left, a monospace value on the right, a hairline below.
fn fact_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let resp = ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.label(label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).monospace().color(c::NEUTRAL_800));
        });
    });
    let r = resp.response.rect;
    ui.painter().hline(r.x_range(), r.bottom() + 1.0, Stroke::new(1.0_f32, c::DIVIDER));
}

fn side_panel(ctx: &egui::Context, id: &str, left: bool, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let panel = if left {
        egui::SidePanel::left(id.to_owned())
    } else {
        egui::SidePanel::right(id.to_owned())
    };
    panel.exact_width(width).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                add(ui);
            });
    });
}

/// The center panel of a tab, scrolling on its own.
fn center_panel(ctx: &egui::Context, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                add(ui);
            });
    });
}

/// Empty center: names the first step and offers its button (README §4).
/// Returns true when the button is clicked.
fn empty_state(ui: &mut egui::Ui, text: &str, button: &str) -> bool {
    let mut clicked = false;
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).color(c::NEUTRAL_800));
        ui.add_space(8.0);
        clicked = ui.button(button).on_hover_text("Ctrl O").clicked();
    });
    clicked
}

/// "1  In game, …" with a 22 px boxed number.
fn numbered_step(ui: &mut egui::Ui, n: u32, text: &str) {
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
        ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0_f32, c::NEUTRAL_500), egui::StrokeKind::Inside);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            n.to_string(),
            FontId::monospace(12.0),
            c::TEXT,
        );
        ui.add(egui::Label::new(text).wrap());
    });
    ui.add_space(4.0);
}

/// 32 px blueprint grid behind a canvas-like center.
fn paint_blueprint_grid(painter: &egui::Painter, rect: Rect) {
    let grid = Stroke::new(1.0_f32, c::NEUTRAL_200);
    let mut x = rect.left() + 32.0;
    while x < rect.right() {
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], grid);
        x += 32.0;
    }
    let mut y = rect.top() + 32.0;
    while y < rect.bottom() {
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], grid);
        y += 32.0;
    }
}

/// One card per Army Generator template (README §5.2). Returns the index
/// whose Remove was clicked; the caller records undo and removes it.
fn recon_slot_list(
    ui: &mut egui::Ui,
    slots: &mut [ReconSlot],
    influence_label: Option<&str>,
    kind_icons: Option<[Option<TextureHandle>; 6]>,
    side: shell::Side,
) -> Option<usize> {
    let mut remove = None;
    for i in 0..slots.len() {
        shell::card(ui, false, |ui| {
            ui.horizontal(|ui| {
                shell::side_marker(ui, side, 12.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} · {}", i + 1, slots[i].info.name))
                            .font(FontId::new(13.0, theme::bold_family())),
                    )
                    .truncate(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if shell::link(ui, "Remove").clicked() {
                        remove = Some(i);
                    }
                });
            });
            let file = slots[i].path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
            let file = if slots[i].sources.len() > 1 {
                format!("{} + {} more", file, slots[i].sources.len() - 1)
            } else {
                file
            };
            let mut meta = format!("{file} · {}", recon_counts_line(&slots[i].info));
            if let Some(n) = slots[i].detected {
                meta.push_str(&format!(" · {n} on the map"));
            }
            ui.label(RichText::new(meta).small().color(c::NEUTRAL_700));
            if let Some(icons) = &kind_icons {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for k in UnitKind::ALL {
                        if unit_kind_icon_button(
                            ui,
                            icons[k.index()].as_ref(),
                            k.label(),
                            &k.hover(),
                            slots[i].kind == k,
                        ) {
                            slots[i].kind = k;
                        }
                    }
                });
            }
            show_missing_locale_hint(ui, &slots[i].path);
            if let Some(label) = influence_label {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(egui::Slider::new(&mut slots[i].influence, 0..=100).trailing_fill(true));
                });
            }
            let zones = slots[i].info.checkzones.clone();
            let suggested = slots[i].info.suggested_triggers.clone();
            if !zones.is_empty() {
                // The checkzone *name* the scan matches is literal; Help lists it.
                ui.label(RichText::new("Zone In checkzones").small().color(c::NEUTRAL_700)).on_hover_text(format!(
                    "Checkzones named {SUGGESTED_ZONE_NAMES} are marked (suggested). Help › Army Generator lists the MCUs a template needs."
                ));
            }
            for zone in &zones {
                let mut on = slots[i].selected_triggers.contains(&zone.index);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, &zone.name).changed() {
                        if on {
                            if !slots[i].selected_triggers.contains(&zone.index) {
                                slots[i].selected_triggers.push(zone.index);
                            }
                        } else {
                            slots[i].selected_triggers.retain(|id| *id != zone.index);
                        }
                    }
                    if suggested.contains(&zone.index) {
                        ui.label(RichText::new("(suggested)").small().color(c::NEUTRAL_700));
                    }
                });
            }
            if slots[i].selected_triggers.is_empty() {
                shell::warning(ui, "Select at least one Zone In.");
            }
        });
        ui.add_space(4.0);
    }
    remove
}

/// "6 vehicles", "1 train, 2 blocks": what a template holds, zero counts left out.
fn recon_counts_line(info: &UnitPlanInfo) -> String {
    let vehicles = info.vehicle_count.saturating_sub(info.ship_count + info.train_count);
    let parts: Vec<String> = [
        (vehicles, "vehicle", "vehicles"),
        (info.ship_count, "ship", "ships"),
        (info.train_count, "train", "trains"),
        (info.block_count, "block", "blocks"),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
    .collect();
    if parts.is_empty() { "no units".to_string() } else { parts.join(", ") }
}

fn recon_dserver_note(ui: &mut egui::Ui) {
    shell::warning(ui, "DServer: keep under 30 random units per mission. Packs cap at 64 copies.");
}

fn recon_delay_note(start_s: u32, group_ms: u32) -> String {
    let mut parts = Vec::new();
    if start_s > 0 {
        parts.push(format!("{start_s} s start delay"));
    }
    if group_ms > 0 {
        parts.push(format!("{group_ms} ms between groups"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(", {}", parts.join(", "))
    }
}

fn labeled_slider(ui: &mut egui::Ui, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>) {
    labeled_slider_suffix(ui, label, value, range, "");
}

/// `labeled_slider` whose value box carries a unit, e.g. "60%".
fn labeled_slider_suffix(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(value, range.clone())
                .show_value(false)
                .trailing_fill(true),
        );
        ui.add(egui::DragValue::new(value).range(range).speed(0.2).suffix(suffix));
    });
}

fn show_missing_locale_hint(ui: &mut egui::Ui, group_path: &Path) {
    if has_sidecars(group_path) {
        return;
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "No translation files (.eng, …) next to this group. Re-export from the editor to create them.",
        )
        .color(c::WARN_TEXT),
    );
}

fn harvest_log_lines(out: &HarvestOutcome) -> Vec<String> {
    let mut lines: Vec<String> = out
        .airfields
        .iter()
        .map(|af| {
            let r = &af.record;
            let mut line = format!(
                "{} ({}) -> {}: {} vehicles, {} blocks, {} logic, {} taxi nodes",
                r.name,
                country_short(r.country),
                r.file,
                r.vehicles,
                r.blocks,
                r.logic,
                r.taxi_nodes
            );
            if af.cleaned.stripped > 0 {
                line.push_str(&format!("; stripped {} player/SP objects", af.cleaned.stripped));
            }
            if af.ai_planes_removed > 0 {
                line.push_str(&format!("; removed {} AI planes", af.ai_planes_removed));
            }
            if af.via_links > 0 {
                line.push_str(&format!("; {} logic nodes beyond the radius kept via links", af.via_links));
            }
            line
        })
        .collect();
    if out.models_added > 0 {
        lines.push(format!("  {} new model(s) added to models.tsv", out.models_added));
    }
    let raw = out.archived.file_name().and_then(|n| n.to_str()).unwrap_or("");
    lines.push(format!("  raw mission archived as raw/{raw}"));
    lines
}

fn save_with_sidecars(
    save_path: &Path,
    text: &str,
    locale_paths: &[PathBuf],
    summary: &str,
) -> Status {
    if let Err(err) = std::fs::write(save_path, text) {
        return Status::Error(format!("Could not write file: {err}"));
    }
    let file = save_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let missing = locale_paths.iter().filter(|p| !has_sidecars(p)).count();
    let sidecars = merge_template_sidecars(locale_paths);
    match write_sidecars(save_path, &sidecars) {
        Ok(exts) if !exts.is_empty() => {
            let extra = if missing > 0 {
                format!(
                    "; {} template(s) had no translation files — re-export from the editor if icons need labels",
                    missing
                )
            } else {
                String::new()
            };
            Status::Info(format!(
                "{summary} plus {} to {file}{extra}.",
                exts.join("/")
            ))
        }
        Ok(_) => {
            let extra = if missing > 0 {
                " No translation files were next to the templates — re-export from the editor if icons need labels."
            } else {
                ""
            };
            Status::Info(format!("{summary} to {file}.{extra}"))
        }
        Err(err) => Status::Error(format!("Wrote the group, but language files failed: {err}")),
    }
}