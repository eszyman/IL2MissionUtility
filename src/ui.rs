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
//! * Mode enums: [`AppMode`], [`ReconSubmode`]. Map drawing types live in [`map`].
//! * Slot structs: [`BomberSlot`], [`ReconSlot`]. Map army slots live in [`map`].
//! * [`GroupGeneratorApp`] — all state plus `eframe::App::update`. Template
//!   Builder lives in [`builder`]; Map lives in [`map`]. The other modes are
//!   still methods here (`recon_*`, `fighter_*`, …).
//! * Free functions — shared widgets and `save_with_sidecars` (writes the
//!   group file plus merged translation sidecars). Map UV/world conversions
//!   and Korea map drawing live in [`map`].
//!
//! ## Conventions
//! * World coordinates are game meters: X is north (up on the map), Z is
//!   east. The map image uses UV and the screen uses `Pos2`; convert only
//!   through the functions at the bottom of this file.
//! * Drawn-mark undo/redo lives in [`map`]: `drawn_marks` is the stack of
//!   what exists; the `redo_*` stacks mirror it for Ctrl-Y. Any new drawing
//!   clears the redo stacks.
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
use crate::flights::FlightConfig;
use crate::frontlines::{timeline_index, ImportedFighterPack, MapRefGroup, Season, BATTLES};
use crate::harvest::{
    default_missions_dir, find_gen_file, harvest_file, GenWatcher, HarvestConfig,
    HarvestOutcome, DEFAULT_DB_DIR,
};
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

use crate::locale::{has_sidecars, merge_template_sidecars, write_sidecars};
use crate::mapclip::WorldAabb;
use crate::mapfighters::MapFighterLayout;
use crate::mapground::{GroundKind, MapGroundLayout};
use crate::mapshipping::MapShipLayout;
use crate::model_spec::{self, ModelClass};
use crate::pack::generate_pack;
use crate::parser::{parse_group_file, parse_il2_document};
use crate::recon::{
    allocate_copies, allocate_mix, apply_randomizer_typed, combine_placed_packs, generate_recon_ex,
    inspect_placed_pack, inspect_unit, looks_like_placed_pack, restore_always_on, wanted_winners,
    ReconBuild, ReconInput, RestoreKind, TypeMix, UnitPlanInfo, SUGGESTED_ZONE_NAMES,
};
use crate::serialize::serialize_group;
use crate::template::{
    bundled_catalog, AIR_ZONE_IN_M, AIR_ZONE_OUT_M, BringUp, CatalogUnit,
    PlaceLayout, TemplateSeat, UnitKind as CatalogKind, WAYPOINT_SPACING_M, ZoneCoalition, ZoneMix,
};
use crate::weapon_range::ArmyUnitKind;

/// Template Builder tab (`src/ui/builder.rs`). Map is `src/ui/map.rs`.
mod builder;
use builder::{default_template_seats, TemplateSnapshot, TplSelect};
mod map;
use map::{
    DrawnMark, GroundHit, KoreaMapLayer, MapAction, MapArmySlot, MapDock, MapDrawingMode,
    MapForces, ShipHit, MAP_TOOLS,
};
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

/// A destructive action waiting for its confirmation dialog.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    ResetTemplate,
    ResetFighter,
    LoadTemplate,
    LoadBaseMap,
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
    /// While set, dragging empty map does not redraw `front_aabb`, and Reset AO is off.
    /// Load base map can still restore a saved box.
    ao_locked: bool,
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
    /// Production loads the baked store. UI tests set this off so they start
    /// from an empty file.
    terrain_use_builtin: bool,
    /// Loaded on first use; `terrain_error` holds why it could not be.
    terrain_store: Option<HeightStore>,
    terrain_error: Option<String>,
    terrain_tiles: Option<Vec<(usize, usize, usize)>>,
    terrain_log: Vec<String>,
    terrain_show_coverage: bool,
    terrain_show_relief: bool,
    /// Put generated units on the measured terrain when exporting. On by
    /// default; a session can turn it off and keep authored heights.
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
    /// Order chip being dragged across the tree (seat, order index).
    tpl_order_drag: Option<(usize, usize)>,
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


fn country_short(country: i32) -> String {
    COUNTRIES
        .iter()
        .find(|(id, _)| *id == country)
        .map(|(_, l)| (*l).to_string())
        .unwrap_or_else(|| country.to_string())
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
            ao_locked: false,
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
            terrain_use_builtin: true,
            terrain_store: None,
            terrain_error: None,
            terrain_tiles: None,
            terrain_log: Vec::new(),
            terrain_show_coverage: false,
            terrain_show_relief: false,
            terrain_apply: true,
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
            tpl_order_drag: None,
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
        map::ensure_side_textures(ctx);
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


    // ── Undo, confirmations, unsaved edits (README §6.3) ──────────────────








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
