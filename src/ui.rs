//! Native egui front-end. Talks to pack / flights / serialize only through
//! their public APIs — no AST parsing lives here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, FontFamily, FontId, Layout, Pos2, Rect, RichText,
    Sense, Stroke, TextStyle, TextureHandle, Vec2,
};

use crate::aircraft::{default_skill, AIRCRAFT_TYPES, COUNTRIES};
use crate::airfield::{
    clean_airfield, inspect_airfield, AirfieldInfo, EASTERN_PLANE_COALITIONS,
    WESTERN_PLANE_COALITIONS,
};
use crate::bombers::{
    extract_exclusive_plans, inspect_plan, link_bomber_plans_with, looks_like_exclusive_pack,
    BomberInput, BomberPlanInfo, SUGGESTED_END_NAMES, SUGGESTED_TRIGGER_NAMES,
};
use crate::duplicate::apply_overrides;
use crate::flights::{configure_aircraft, flight_sizes, FlightConfig};
use crate::frontlines::{
    attack_arrow_points, battles_in_period, generate_front, mark_for_battle, preview_dots,
    preview_front_xz, suggested_aircraft, timeline_index, timeline_preview, Battle, FrontOptions,
    MapFighterPack, MapGroundPack, MapRefGroup, MapShipPack, PreviewKind, Season, TimelineMark, ARROW_TAIL_WIDTH, BATTLES,
    PLACE_MARGIN, TIMELINE, YEARS,
};
use crate::geo::{self, MAP_MAX, MAP_MIN};
use crate::help::{self, HelpTopic};
use crate::locale::{has_sidecars, merge_template_sidecars, write_sidecars, LANG_EXTS};
use crate::mapclip::{
    apply_salients, can_extend_salient, can_extend_west_east, clip_linestring_to_rect,
    clip_polyline_to_aabb, clip_ring_to_aabb, linestring_to_points, point_north_of_front,
    points_to_linestring, snap_to_front, stroke_self_intersects, WorldAabb, FRONT_PLACE_BAND,
};
use crate::mapfighters::{
    country_for_coalition, place_in_coalition, rtb_ao_point, MapFighterLayout, MAX_PACKS,
};
use crate::mapground::{
    numbered_ground_issues, place_ground_jobs, GroundJob, GroundKind, GroundSpot, MapGroundLayout,
    GROUP_DELAY_S as GROUND_GROUP_DELAY_S, START_DELAY_S as GROUND_START_DELAY_S,
    ARTY_OBJECTIVE_RADIUS,
};
use crate::mapnet;
use crate::mapshipping::{place_ships, MapShipLayout, ShipSpot, GROUP_DELAY_S, START_DELAY_S};
use crate::model_spec::{self, ModelClass};
use crate::placement::PlaceOpts;
use crate::pack::{builtin_template, generate_pack, generate_pack_at, park_rtbs, zone_in_radius};
use crate::parser::{parse_group_file, parse_il2_document};
use crate::recon::{
    allocate_copies, allocate_mix, apply_randomizer_typed, combine_placed_packs, generate_recon_ex,
    inspect_army_copies, inspect_placed_pack, inspect_unit, looks_like_placed_pack,
    park_army_mixed, park_recon_copies_headed, park_recon_copies_spots,
    restore_always_on, snap_army_attack_areas,
    snap_copy_attack_areas, wanted_winners, ArmyCopyInfo, ReconBuild, ReconInput, RestoreKind,
    TypeMix, UnitPlanInfo, SUGGESTED_ZONE_NAMES,
};
use crate::serialize::serialize_group;
use crate::template::{
    append_seat, apply_formation_numbers, apply_suggested_attack_area, bundled_catalog,
    copy_seat_attributes, flight_lead_of, formation_label, formations_for, generate_template,
    has_linked_wingmen, is_follower, lead_indexes, load_catalog, load_catalog_as_user_added,
    insert_goto_waypoint_after, merge_catalog, move_seat, next_waypoint_number,
    normalize_order_chain, order_seat_indexes, order_tree_columns, place_offset, receives_orders,
    refresh_attack_areas_for_seat, remap_event_then, remap_index_vec, remap_seat_index,
    set_report_following, used_waypoint_count, BringUp, CatalogUnit, EntityEvent, EventHook,
    EventThen, FlightRole, OrderKind, OrderSpec, PlaceLayout, TemplateOptions, TemplateSeat,
    UnitKind as CatalogKind, ZoneCoalition, DEFAULT_TIME_ON_TARGET_S, PLACEMENT_SPACING,
    attack_area_range_limit, carriage_label, catalog_carriage_scripts,
};
use crate::weapon_range::{self, ArmyUnitKind};

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 1280.0])
            .with_min_inner_size([900.0, 800.0])
            .with_decorations(true),
        ..Default::default()
    };
    eframe::run_native(
        "IL-2 Group Generator",
        options,
        Box::new(|cc| {
            apply_readable_style(&cc.egui_ctx);
            Ok(Box::new(GroupGeneratorApp::default()))
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawnMark {
    Salient,
    AttackArrow,
}

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
    Train,
}

impl UnitKind {
    const ALL: [Self; 5] = [Self::Ship, Self::Armor, Self::Supply, Self::Artillery, Self::Train];

    fn label(self) -> &'static str {
        match self {
            Self::Ship => "Ship",
            Self::Armor => "Armor",
            Self::Supply => "Supply",
            Self::Artillery => "Artillery",
            Self::Train => "Train",
        }
    }

    fn terrain_hint(self) -> &'static str {
        match self {
            Self::Ship => "water",
            Self::Train => "railroad",
            Self::Armor | Self::Supply | Self::Artillery => "open ground or road column",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Ship => 0,
            Self::Armor => 1,
            Self::Supply => 2,
            Self::Artillery => 3,
            Self::Train => 4,
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
            ArmyUnitKind::Train => Self::Train,
        }
    }

    fn ground(self) -> Option<GroundKind> {
        match self {
            Self::Ship => None,
            Self::Armor => Some(GroundKind::Armor),
            Self::Supply => Some(GroundKind::Supply),
            Self::Artillery => Some(GroundKind::Artillery),
            Self::Train => Some(GroundKind::Train),
        }
    }
}

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
    recon_submode: ReconSubmode,
    recon_slots: Vec<ReconSlot>,
    recon_rework: Vec<ReconSlot>,
    recon_total: u32,
    recon_percent: u32,
    recon_keep_positions: bool,
    recon_import_kind: UnitKind,
    recon_group_delay_ms: u32,
    recon_start_delay_s: u32,
    recon_strip_randomizer: bool,
    airfield_path: Option<PathBuf>,
    airfield_root: Option<crate::ast::Il2Entity>,
    airfield_info: Option<AirfieldInfo>,
    airfield_western: bool,
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
    current_salient: Vec<(f64, f64)>,
    salients: Vec<Vec<(f64, f64)>>,
    attack_arrows: Vec<((f64, f64), (f64, f64))>,
    attack_drag: Option<((f64, f64), (f64, f64))>,
    drawn_marks: Vec<DrawnMark>,
	redo_marks: Vec<DrawnMark>,
    redo_salients: Vec<Vec<(f64, f64)>>,
    redo_attack_arrows: Vec<((f64, f64), (f64, f64))>,
    map_fighters: Option<MapFighterLayout>,
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
    tpl_path: Option<PathBuf>,
    tpl_catalog: Vec<CatalogUnit>,
    tpl_kind: CatalogKind,
    tpl_class: Option<ModelClass>,
    tpl_add_pick: usize,
    /// When true, the formation-view card shows the catalog pick (adding a unit).
    /// Otherwise it follows the highlighted seat.
    tpl_preview_from_catalog: bool,
    tpl_model_tex: HashMap<String, TextureHandle>,
    tpl_seats: Vec<TemplateSeat>,
    tpl_select: Option<TplSelect>,
    tpl_bring_up: BringUp,
    tpl_spawn_reset: bool,
    tpl_spawn_cooldown_min: f32,
    tpl_place_layout: PlaceLayout,
    tpl_per_group: u32,
    tpl_zone_in: f32,
    tpl_zone_out: f32,
    tpl_wp_spacing: f32,
    tpl_wp_speed: f32,
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

fn default_template_seats() -> Vec<TemplateSeat> {
    let cat = bundled_catalog();
    let unit = cat
        .iter()
        .find(|u| u.kind == CatalogKind::Plane && u.script.contains("mig15bis"))
        .or_else(|| cat.iter().find(|u| u.kind == CatalogKind::Plane))
        .cloned();
    let Some(unit) = unit else {
        return Vec::new();
    };
    (0..4)
        .map(|i| {
            let mut seat = TemplateSeat::new(unit.clone());
            seat.number_in_formation = i as i32;
            if i == 0 {
                seat.orders.push(OrderSpec::for_kind(CatalogKind::Plane));
            }
            seat
        })
        .collect()
}

fn country_short(country: i32) -> String {
    COUNTRIES
        .iter()
        .find(|(id, _)| *id == country)
        .map(|(_, l)| (*l).to_string())
        .unwrap_or_else(|| country.to_string())
}

/// Preview color for a country's side: NATO (teal), Eastern (red), neutral (grey).
fn side_color(country: i32) -> Color32 {
    if country / 100 == 6 {
        Color32::from_rgb(0, 120, 150)   // NATO
    } else if country / 100 == 5 {
        Color32::from_rgb(155, 0, 0)      // Eastern
    } else {
        Color32::from_rgb(140, 140, 140)  // neutral / unassigned
    }
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(15.0));
    ui.add_space(4.0);
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
    } else if matches!(
        kind,
        OrderKind::TimeOnTarget | OrderKind::MissionComplete
    ) {
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

fn draw_template_order_chip(
    ui: &mut egui::Ui,
    si: usize,
    oi: usize,
    n_orders: usize,
    kind: OrderKind,
    extra: usize,
    selected: bool,
    selected_fill: Color32,
    order_fill: Color32,
    report_fill: Color32,
    chain_fill: Color32,
    clicked: &mut Option<TplSelect>,
    remove_order: &mut Option<(usize, usize)>,
    move_order: &mut Option<(usize, usize, i32)>,
) {
    let text = order_chip_label(oi, kind, extra);
    let fill = order_chip_fill(
        kind,
        selected,
        selected_fill,
        order_fill,
        report_fill,
        chain_fill,
    );
    let wide = kind.is_report()
        || matches!(
            kind,
            OrderKind::TimeOnTarget | OrderKind::MissionComplete
        );
    let hover = if kind.is_wp_parallel() {
        "Starts with Attack / Time on Target from the waypoint, not after a delay."
    } else if kind == OrderKind::MissionComplete {
        "On waypoint arrival (or when Time on Target expires) pulses MISSION END."
    } else {
        ""
    };
    let chip = egui::Button::new(text)
        .fill(fill)
        .min_size(Vec2::new(if wide { 140.0 } else { 92.0 }, 24.0));
    let resp = ui.add(chip);
    if !hover.is_empty() {
        resp.clone().on_hover_text(hover);
    }
    if resp.clicked() {
        *clicked = Some(TplSelect::Order { seat: si, order: oi });
    }
    if ui.small_button("×").on_hover_text("Remove order").clicked() {
        *remove_order = Some((si, oi));
    }
    if selected {
        if ui.small_button("<").clicked() {
            *move_order = Some((si, oi, -1));
        }
        ui.add_enabled_ui(oi + 1 < n_orders, |ui| {
            if ui.small_button(">").clicked() {
                *move_order = Some((si, oi, 1));
            }
        });
    }
}

enum Status {
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
            recon_submode: ReconSubmode::New,
            recon_slots: Vec::new(),
            recon_rework: Vec::new(),
            recon_total: 2,
            recon_percent: 50,
            recon_keep_positions: false,
            recon_import_kind: UnitKind::Armor,
            recon_group_delay_ms: 500,
            recon_start_delay_s: 0,
            recon_strip_randomizer: false,
            airfield_path: None,
            airfield_root: None,
            airfield_info: None,
            airfield_western: true,
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
            current_salient: Vec::new(),
            salients: Vec::new(),
            attack_arrows: Vec::new(),
            attack_drag: None,
            drawn_marks: Vec::new(),
			redo_marks: Vec::new(),
            redo_salients: Vec::new(),
            redo_attack_arrows: Vec::new(),			
            map_fighters: None,
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
            tpl_path: None,
            tpl_catalog: bundled_catalog(),
            tpl_kind: CatalogKind::Plane,
            tpl_class: None,
            tpl_add_pick: 0,
            tpl_preview_from_catalog: false,
            tpl_model_tex: HashMap::new(),
            tpl_seats: default_template_seats(),
            tpl_select: Some(TplSelect::Seat(0)),
            tpl_bring_up: BringUp::Activate,
            tpl_spawn_reset: false,
            tpl_spawn_cooldown_min: 5.0,
            tpl_place_layout: PlaceLayout::InvertedVee,
            tpl_per_group: 4,
            tpl_zone_in: 7_500.0,
            tpl_zone_out: 8_500.0,
            tpl_wp_spacing: 4_000.0,
            tpl_wp_speed: 100.0,
            tpl_zone_coalition: ZoneCoalition::Western,
            tpl_view_zoom: 1.0,
            tpl_view_pan: Vec2::ZERO,
        }
    }
}

impl eframe::App for GroupGeneratorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading("IL-2 Group Generator");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Help").clicked() {
                            let topic = self.page_help_topic();
                            self.open_help(topic);
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.mode, AppMode::Template, "Template");
                    ui.selectable_value(&mut self.mode, AppMode::Recon, "Army Generator");
                    ui.selectable_value(&mut self.mode, AppMode::Fighter, "Fighter Pack");
                    ui.selectable_value(&mut self.mode, AppMode::Exclusive, "Exclusive Activation");
                    ui.selectable_value(&mut self.mode, AppMode::Airfield, "Airfield");
                    ui.selectable_value(&mut self.mode, AppMode::Map, "Map");
                });
                ui.add_space(8.0);

                match self.mode {
                    AppMode::Template => self.template_panel(ui),
                    AppMode::Fighter => self.fighter_panel(ui),
                    AppMode::Exclusive => self.bomber_panel(ui),
                    AppMode::Recon => self.recon_panel(ui),
                    AppMode::Airfield => self.airfield_panel(ui),
                    AppMode::Map => self.map_panel(ui),
                }

                ui.add_space(6.0);
                self.status_line(ui);
                ui.add_space(8.0);
            });
        });
        help::show_window(ctx, &mut self.help_open, &mut self.help_topic);
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

    fn page_header(&mut self, ui: &mut egui::Ui, title: &str, topic: HelpTopic) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Help").clicked() {
                    self.open_help(topic);
                }
            });
        });
    }

    fn template_panel(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Template Builder", HelpTopic::Template);
        ui.label(
            RichText::new(
                "Build a proximity-triggered unit group. This mode does not write NodeGates, but is intended to be used with the Army Generator mode.",
            )
            .small(),
        );
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            self.template_bring_up_section(&mut cols[0]);
            self.template_placement_section(&mut cols[1]);
        });
        ui.add_space(10.0);
        self.template_formation_view_section(ui);
        ui.add_space(10.0);
        self.template_add_units_section(ui);
        ui.add_space(10.0);
        self.template_unit_list_section(ui);
        ui.add_space(10.0);
        self.template_waypoints_section(ui);
        ui.add_space(10.0);
        self.template_catalog_section(ui);
        ui.add_space(12.0);

        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let generate = egui::Button::new(RichText::new("Generate File").strong())
                .min_size(Vec2::new(200.0, 32.0));
            if ui.add(generate).clicked() {
                self.generate_unit_template();
            }
        });
    }

    fn template_catalog_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            section_title(ui, "Catalog");
            ui.horizontal(|ui| {
                if ui.button("Load catalog…").clicked() {
                    self.load_unit_catalog();
                }
                if ui.button("Add group…").clicked() {
                    self.add_user_catalog_group();
                }
                if ui.small_button("Use built-in catalog").clicked() {
                    self.tpl_path = None;
                    self.tpl_catalog = bundled_catalog();
                    self.tpl_class = None;
                    self.tpl_add_pick = 0;
                    self.tpl_preview_from_catalog = false;
                }
                let label = self
                    .tpl_path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or("Built-in ModelTypes + Fixed Objects");
                ui.label(RichText::new(label).italics());
            });
        });
    }

    fn template_add_units_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            section_title(ui, "Add Units");
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Kind").strong());
                for kind in CatalogKind::ALL {
                    if ui
                        .selectable_label(self.tpl_kind == kind, kind.label())
                        .clicked()
                    {
                        self.tpl_kind = kind;
                        self.tpl_class = None;
                        self.tpl_add_pick = 0;
                        self.tpl_preview_from_catalog = true;
                    }
                }
            });
            self.draw_template_model_browser(ui);
        });
    }

    fn template_unit_list_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            section_title(ui, "Unit List");
            ui.label(
                RichText::new(
                    "UNIT → OnSpawned → orders (Goto WP, Attack, Time on Target, Mission Complete). Events link off the unit.",
                )
                .italics()
                .small(),
            );
            self.draw_template_seat_list(ui);
            self.draw_template_details(ui);
        });
    }

    fn template_formation_view_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            section_title(ui, "Formation View");
            ui.horizontal(|ui| {
                ui.label("Zoom");
                ui.add(
                    egui::Slider::new(&mut self.tpl_view_zoom, 0.04..=12.0).show_value(false),
                );
                ui.label(RichText::new(format!("{:.0}%", self.tpl_view_zoom * 100.0)));
                if ui
                    .button("Reset View")
                    .on_hover_text("Fit the formation. Scroll to zoom, right-drag to pan.")
                    .clicked()
                {
                    self.tpl_view_zoom = 1.0;
                    self.tpl_view_pan = Vec2::ZERO;
                }
            });
            self.draw_template_schematic(ui);
        });
    }

    fn template_bring_up_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            section_title(ui, "Activate or Spawn");
            let flights_need_activate = has_linked_wingmen(&self.tpl_seats);
            if flights_need_activate && self.tpl_bring_up == BringUp::Spawn {
                self.tpl_bring_up = BringUp::Activate;
            }
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
            if flights_need_activate {
                ui.label(
                    RichText::new(
                        "A flight lead has followers (target-linked wingmen). That flight must be activated, not spawned. Independent units in the same file are activated with them.",
                    )
                    .italics()
                    .small(),
                );
            }
            if self.tpl_bring_up == BringUp::Spawn {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.tpl_spawn_reset, "Allow multiple spawns")
                        .on_hover_text(
                            "One-shot: leave unchecked. Repeat spawn: Zone Out always cleans up and zeros DeathCount. If every unit is destroyed (OnPlaneDestroyed / OnKilled), cooldown pulses the spawner even while the player stays. A hiding unit is cleaned up when the player leaves, not mid-fight.",
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
        ui.group(|ui| {
            section_title(ui, "Placement & Checkzones");
            ui.horizontal(|ui| {
                ui.label("Layout");
                let prev_layout = self.tpl_place_layout;
                egui::ComboBox::from_id_salt("tpl_place_layout")
                    .selected_text(self.tpl_place_layout.label())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for layout in PlaceLayout::ALL {
                            ui.selectable_value(
                                &mut self.tpl_place_layout,
                                layout,
                                layout.label(),
                            );
                        }
                    });
                if self.tpl_place_layout != prev_layout {
                    self.tpl_per_group = self.tpl_place_layout.default_per_group();
                }
                ui.label("Per group");
                ui.add(egui::DragValue::new(&mut self.tpl_per_group).range(1..=8));
            });
            ui.label(
                RichText::new(
                    "Spacing is 150 m. Inverted Vee is finger-four (4). Combat Box is two 3-ship vees (6). Ground and ships usually use Column.",
                )
                .italics()
                .small(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Trigger coalition");
                egui::ComboBox::from_id_salt("tpl_zone_coalition")
                    .selected_text(self.tpl_zone_coalition.label())
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for c in ZoneCoalition::ALL {
                            ui.selectable_value(&mut self.tpl_zone_coalition, c, c.label());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Zone IN (visual range)");
                ui.add(
                    egui::Slider::new(&mut self.tpl_zone_in, 500.0..=25_000.0)
                        .suffix(" m")
                        .logarithmic(true),
                );
            });
            if self.tpl_zone_out < self.tpl_zone_in + 200.0 {
                self.tpl_zone_out = self.tpl_zone_in + 200.0;
            }
            ui.horizontal(|ui| {
                ui.label("Zone Out");
                ui.add(
                    egui::Slider::new(&mut self.tpl_zone_out, 700.0..=30_000.0)
                        .suffix(" m")
                        .logarithmic(true),
                );
            });
        });
    }

    fn template_waypoints_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            section_title(ui, "Waypoints");
            ui.horizontal(|ui| {
                let n = used_waypoint_count(&self.tpl_seats);
                ui.label(if n == 0 {
                    "None (add a Goto WP order)".to_string()
                } else {
                    format!("{n} from Goto WP orders")
                });
                ui.label("Spacing");
                ui.add(
                    egui::DragValue::new(&mut self.tpl_wp_spacing)
                        .range(200.0..=20_000.0)
                        .suffix(" m"),
                );
                ui.label("Speed");
                ui.add(
                    egui::DragValue::new(&mut self.tpl_wp_speed)
                        .range(10.0..=900.0)
                        .suffix(" m/s"),
                );
            });
        });
    }

    fn draw_template_seat_list(&mut self, ui: &mut egui::Ui) {
        let selected_fill = Color32::from_rgb(252, 186, 3);
        let unit_fill = Color32::from_rgb(196, 196, 196);
        let order_fill = Color32::from_rgb(94, 167, 181);
        let report_fill = Color32::from_rgb(94, 181, 133);
        let chain_fill = Color32::from_rgb(181, 148, 94);
        let event_fill = Color32::from_rgb(175, 94, 181);
        let mut add_order = None;
        let mut add_event = None;
        let mut remove_seat = None;
        let mut remove_order = None;
        let mut remove_event = None;
        let mut move_order: Option<(usize, usize, i32)> = None;
        let mut move_seat_dir: Option<(usize, i32)> = None;
        let mut clicked = None;

        for si in 0..self.tpl_seats.len() {
            ui.horizontal(|ui| {
                let unit_sel = matches!(self.tpl_select, Some(TplSelect::Seat(s)) if s == si);
                let role = match self.tpl_seats[si].role {
                    FlightRole::Lead => "Lead",
                    FlightRole::Follows(_) if is_follower(&self.tpl_seats, si) => "Wing",
                    _ => "",
                };
                let label = if role.is_empty() {
                    self.tpl_seats[si].unit.label().to_string()
                } else {
                    format!("{} ({role})", self.tpl_seats[si].unit.label())
                };
                let unit_btn = egui::Button::new(RichText::new(label).strong())
                    .fill(if unit_sel { selected_fill } else { unit_fill })
                    .min_size(Vec2::new(140.0, 24.0));
                if ui.add(unit_btn).clicked() {
                    clicked = Some(TplSelect::Seat(si));
                }
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
                    let n_orders = self.tpl_seats[si].orders.len();
                    for oi in 0..n_orders {
                        let order_sel = matches!(
                            self.tpl_select,
                            Some(TplSelect::Order { seat, order }) if seat == si && order == oi
                        );
                        let kind = self.tpl_seats[si].orders[oi].kind;
                        let extra = self.tpl_seats[si].orders[oi].shared_with.len();
                        let text = if extra > 0 {
                            format!("{} {} +{}", oi + 1, kind.label(), extra)
                        } else {
                            format!("{} {}", oi + 1, kind.label())
                        };
                        let fill = if order_sel {
                            selected_fill
                        } else if kind.is_report() {
                            report_fill
                        } else if matches!(
                            kind,
                            OrderKind::TimeOnTarget | OrderKind::MissionComplete
                        ) {
                            chain_fill
                        } else {
                            order_fill
                        };
                        let wide = kind.is_report()
                            || matches!(
                                kind,
                                OrderKind::TimeOnTarget | OrderKind::MissionComplete
                            );
                        let chip = egui::Button::new(text)
                            .fill(fill)
                            .min_size(Vec2::new(if wide { 140.0 } else { 92.0 }, 24.0));
                        if ui.add(chip).clicked() {
                            clicked = Some(TplSelect::Order { seat: si, order: oi });
                        }
                        if ui.small_button("×").on_hover_text("Remove order").clicked() {
                            remove_order = Some((si, oi));
                        }
                        if order_sel {
                            if ui.small_button("<").clicked() {
                                move_order = Some((si, oi, -1));
                            }
                            if ui.small_button(">").clicked() {
                                move_order = Some((si, oi, 1));
                            }
                        }
                    }
                    if ui.small_button("+").on_hover_text("Add order").clicked() {
                        add_order = Some(si);
                    }
                } else {
                    ui.label(
                        RichText::new("follows lead")
                            .italics()
                            .small(),
                    );
                }
                let n_events = self.tpl_seats[si].events.len();
                for ei in 0..n_events {
                    let event_sel = matches!(
                        self.tpl_select,
                        Some(TplSelect::Event { seat, event }) if seat == si && event == ei
                    );
                    let kind = self.tpl_seats[si].events[ei].kind;
                    let chip = egui::Button::new(kind.label())
                        .fill(if event_sel { selected_fill } else { event_fill })
                        .min_size(Vec2::new(110.0, 24.0));
                    if ui.add(chip).clicked() {
                        clicked = Some(TplSelect::Event { seat: si, event: ei });
                    }
                    if ui.small_button("×").on_hover_text("Remove event").clicked() {
                        remove_event = Some((si, ei));
                    }
                }
                if ui.small_button("+evt").on_hover_text("Add event").clicked() {
                    add_event = Some(si);
                }
                if ui.small_button("×").on_hover_text("Remove unit").clicked() {
                    remove_seat = Some(si);
                }
            });
        }

        if let Some(sel) = clicked {
            self.tpl_select = Some(sel);
            self.tpl_preview_from_catalog = false;
        }
        if let Some(si) = add_order {
            let unit = self.tpl_seats[si].unit.clone();
            self.tpl_seats[si].orders.push(OrderSpec::for_unit(&unit));
            let oi = self.tpl_seats[si].orders.len() - 1;
            self.tpl_select = Some(TplSelect::Order { seat: si, order: oi });
        }
        if let Some(si) = add_event {
            let kind = self.tpl_seats[si].unit.kind;
            self.tpl_seats[si].events.push(EventHook::default_for(kind));
            let ei = self.tpl_seats[si].events.len() - 1;
            self.tpl_select = Some(TplSelect::Event { seat: si, event: ei });
        }
        if let Some((si, oi)) = remove_order {
            if si < self.tpl_seats.len() && oi < self.tpl_seats[si].orders.len() {
                self.tpl_seats[si].orders.remove(oi);
                for hook in &mut self.tpl_seats[si].events {
                    remap_event_then(&mut hook.then, oi);
                }
                self.tpl_select = Some(TplSelect::Seat(si));
            }
        }
        if let Some((si, ei)) = remove_event {
            if si < self.tpl_seats.len() && ei < self.tpl_seats[si].events.len() {
                self.tpl_seats[si].events.remove(ei);
                self.tpl_select = Some(TplSelect::Seat(si));
            }
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
        if let Some(si) = remove_seat {
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
        }
    }

    fn draw_train_carriages(&mut self, ui: &mut egui::Ui, si: usize) {
        ui.add_space(4.0);
        ui.label(RichText::new("Carriages").strong());
        ui.label(
            RichText::new(
                "The locomotive is the selected train. Add, remove, or reorder cars. Tender first is typical.",
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
            ui.horizontal(|ui| {
                ui.label(format!("{}.", ci + 1));
                ui.label(carriage_label(&self.tpl_seats[si].carriages[ci]));
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
                if ui.small_button("×").on_hover_text("Remove carriage").clicked() {
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
            }
        }
        let mut choices = self.tpl_seats[si].unit.prototype_carriages();
        for extra in catalog_carriage_scripts(&self.tpl_catalog) {
            if !choices.iter().any(|s| s.eq_ignore_ascii_case(&extra)) {
                choices.push(extra);
            }
        }
        if !choices.is_empty() {
            ui.horizontal(|ui| {
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

    fn draw_template_details(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        match self.tpl_select {
            Some(TplSelect::Seat(si)) if si < self.tpl_seats.len() => {
                ui.label(RichText::new("Unit details").strong());
                let mut moved = false;
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Seat {} · {} · {}",
                        si + 1,
                        self.tpl_seats[si].unit.kind.label(),
                        self.tpl_seats[si].unit.label()
                    ));
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
                if moved {
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("Role");
                    let follow_choices: Vec<(usize, String)> = lead_indexes(&self.tpl_seats)
                        .into_iter()
                        .filter(|&i| i != si)
                        .map(|i| {
                            (
                                i,
                                format!(
                                    "Follows seat {} ({})",
                                    i + 1,
                                    self.tpl_seats[i].unit.label()
                                ),
                            )
                        })
                        .collect();
                    let current = match self.tpl_seats[si].role {
                        FlightRole::Independent => "Independent".to_string(),
                        FlightRole::Lead => "Lead".to_string(),
                        FlightRole::Follows(i) => format!("Follows seat {}", i + 1),
                    };
                    egui::ComboBox::from_id_salt(format!("tpl_role_{si}"))
                        .selected_text(current)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.tpl_seats[si].role == FlightRole::Independent,
                                    "Independent",
                                )
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
                                .selectable_label(
                                    self.tpl_seats[si].role == FlightRole::Lead,
                                    "Lead",
                                )
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
                                    .selectable_label(
                                        self.tpl_seats[si].role == FlightRole::Follows(*i),
                                        text,
                                    )
                                    .clicked()
                                {
                                    self.tpl_seats[si].role = FlightRole::Follows(*i);
                                    let n = self
                                        .tpl_seats
                                        .iter()
                                        .enumerate()
                                        .filter(|(j, s)| {
                                            *j != si && s.role == FlightRole::Follows(*i)
                                        })
                                        .count() as i32
                                        + 1;
                                    self.tpl_seats[si].number_in_formation = n;
                                }
                            }
                        });
                });
                if self.tpl_seats[si].role == FlightRole::Lead {
                    ui.horizontal(|ui| {
                        ui.label("In formation");
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
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Country");
                    egui::ComboBox::from_id_salt(format!("tpl_seat_country_{si}"))
                        .selected_text(
                            COUNTRIES
                                .iter()
                                .find(|(id, _)| *id == self.tpl_seats[si].country)
                                .map(|(_, l)| *l)
                                .unwrap_or("?"),
                        )
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for (id, label) in COUNTRIES {
                                ui.selectable_value(
                                    &mut self.tpl_seats[si].country,
                                    *id,
                                    *label,
                                );
                            }
                        });
                    ui.label("Skill");
                    ui.add(egui::Slider::new(&mut self.tpl_seats[si].skill, 0..=4));
                });
                if self.tpl_seats[si].unit.is_air() {
                    ui.horizontal(|ui| {
                        ui.label("Altitude");
                        ui.add(
                            egui::Slider::new(&mut self.tpl_seats[si].altitude, 50.0..=8000.0)
                                .suffix(" m")
                                .logarithmic(true),
                        );
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Number in formation");
                    ui.add(
                        egui::DragValue::new(&mut self.tpl_seats[si].number_in_formation)
                            .range(0..=8),
                    );
                    ui.label("Fuel");
                    ui.add(
                        egui::DragValue::new(&mut self.tpl_seats[si].fuel)
                            .range(0.0..=1.0)
                            .speed(0.05),
                    );
                    ui.label("Payload");
                    ui.add(egui::DragValue::new(&mut self.tpl_seats[si].payload_id).range(0..=99));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.tpl_seats[si].vulnerable, "Vulnerable");
                    ui.checkbox(&mut self.tpl_seats[si].engageable, "Engageable");
                    ui.checkbox(&mut self.tpl_seats[si].limit_ammo, "Limit ammo");
                    if self.tpl_seats[si].unit.is_air() {
                        ui.checkbox(&mut self.tpl_seats[si].ai_rtb, "AI RTB");
                    }
                });
                if is_follower(&self.tpl_seats, si) {
                    ui.label(
                        RichText::new("This unit follows its lead (entity target link). Orders are given to the lead only.")
                            .italics()
                            .small(),
                    );
                }
                ui.label(
                    RichText::new(self.tpl_seats[si].unit.script.as_str())
                        .italics()
                        .small(),
                );
                if self.tpl_seats[si].unit.is_train() {
                    self.draw_train_carriages(ui, si);
                }
                ui.horizontal(|ui| {
                    ui.label(format!("Model: {}", self.tpl_seats[si].unit.label()));
                    let picked = self.displayed_catalog().get(self.tpl_add_pick).cloned();
                    if let Some(picked) = picked {
                        if !picked
                            .script
                            .eq_ignore_ascii_case(&self.tpl_seats[si].unit.script)
                            && ui
                                .button(format!("Change to {}", picked.label()))
                                .clicked()
                        {
                            let was_train = self.tpl_seats[si].unit.is_train();
                            self.tpl_seats[si].unit = picked.clone();
                            if picked.is_train() {
                                self.tpl_seats[si].carriages = picked.default_carriages();
                            } else if was_train {
                                self.tpl_seats[si].carriages.clear();
                            }
                            refresh_attack_areas_for_seat(&mut self.tpl_seats, si);
                        }
                    }
                });
            }
            Some(TplSelect::Order { seat, order })
                if seat < self.tpl_seats.len() && order < self.tpl_seats[seat].orders.len() =>
            {
                ui.label(RichText::new("Order details").strong());
                ui.horizontal(|ui| {
                    ui.label(format!("Seat {} · Order {}", seat + 1, order + 1));
                    let kind = self.tpl_seats[seat].orders[order].kind;
                    let unit_kind = if self.tpl_seats[seat].unit.is_air() {
                        CatalogKind::Plane
                    } else {
                        CatalogKind::Vehicle
                    };
                    egui::ComboBox::from_id_salt(format!("tpl_ord_kind_{seat}_{order}"))
                        .selected_text(kind.label())
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            ui.label(RichText::new("Commands").small().italics());
                            for k in OrderKind::available(unit_kind) {
                                if k.is_report() {
                                    continue;
                                }
                                if ui
                                    .selectable_label(
                                        self.tpl_seats[seat].orders[order].kind == *k,
                                        k.label(),
                                    )
                                    .clicked()
                                {
                                    let was_goto = self.tpl_seats[seat].orders[order].kind
                                        == OrderKind::GotoWaypoint;
                                    let next_wp = next_waypoint_number(&self.tpl_seats);
                                    self.tpl_seats[seat].orders[order].kind = *k;
                                    if *k == OrderKind::GotoWaypoint && !was_goto {
                                        self.tpl_seats[seat].orders[order].waypoint = next_wp;
                                    }
                                    if *k == OrderKind::Formation {
                                        let presets = formations_for(unit_kind);
                                        let id = self.tpl_seats[seat].orders[order].formation_type;
                                        if !presets.iter().any(|p| p.id == id) {
                                            self.tpl_seats[seat].orders[order].formation_type =
                                                OrderSpec::for_kind(unit_kind).formation_type;
                                        }
                                    }
                                    if *k == OrderKind::TimeOnTarget {
                                        self.tpl_seats[seat].orders[order].time_s =
                                            DEFAULT_TIME_ON_TARGET_S;
                                    }
                                    if *k == OrderKind::AttackArea {
                                        apply_suggested_attack_area(
                                            &mut self.tpl_seats,
                                            seat,
                                            order,
                                        );
                                    }
                                }
                            }
                            ui.separator();
                            ui.label(RichText::new("Reports").small().italics());
                            for k in OrderKind::available(unit_kind) {
                                if !k.is_report() {
                                    continue;
                                }
                                if ui
                                    .selectable_label(
                                        self.tpl_seats[seat].orders[order].kind == *k,
                                        k.label(),
                                    )
                                    .clicked()
                                {
                                    self.tpl_seats[seat].orders[order].kind = *k;
                                }
                            }
                        });
                    if ui.small_button("Remove order").clicked() {
                        self.tpl_seats[seat].orders.remove(order);
                        for hook in &mut self.tpl_seats[seat].events {
                            remap_event_then(&mut hook.then, order);
                        }
                        self.tpl_select = Some(TplSelect::Seat(seat));
                        return;
                    }
                });
                let order = {
                    let s = &mut self.tpl_seats[seat];
                    normalize_order_chain(&mut s.orders, &mut s.events, order)
                };
                self.tpl_select = Some(TplSelect::Order { seat, order });
                if seat >= self.tpl_seats.len() || order >= self.tpl_seats[seat].orders.len() {
                    return;
                }
                match self.tpl_seats[seat].orders[order].kind {
                    OrderKind::Attack => {
                        ui.label(
                            "MCU_CMD_AttackTarget: Objects = this unit (or its lead), Targets = the unit to attack.",
                        );
                        let others = order_seat_indexes(&self.tpl_seats)
                            .into_iter()
                            .filter(|&i| i != seat)
                            .collect::<Vec<_>>();
                        if others.is_empty() {
                            ui.label(RichText::new("Add another unit to attack.").italics());
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Target");
                                let current = self.tpl_seats[seat].orders[order]
                                    .attack_seat
                                    .and_then(|i| {
                                        self.tpl_seats.get(i).map(|s| {
                                            format!("Seat {} ({})", i + 1, s.unit.label())
                                        })
                                    })
                                    .unwrap_or_else(|| "Choose unit…".into());
                                egui::ComboBox::from_id_salt(format!("tpl_atk_{seat}_{order}"))
                                    .selected_text(current)
                                    .width(220.0)
                                    .show_ui(ui, |ui| {
                                        for i in &others {
                                            let text = format!(
                                                "Seat {} ({})",
                                                i + 1,
                                                self.tpl_seats[*i].unit.label()
                                            );
                                            ui.selectable_value(
                                                &mut self.tpl_seats[seat].orders[order].attack_seat,
                                                Some(*i),
                                                text,
                                            );
                                        }
                                    });
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.checkbox(
                                &mut self.tpl_seats[seat].orders[order].attack_group,
                                "Attack group",
                            );
                            ui.label("Priority");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].priority,
                                )
                                .range(0..=2),
                            );
                        });
                    }
                    OrderKind::GotoWaypoint => {
                        ui.label(
                            "Pulses waypoint MCU WP n (there is no Goto command). On arrival that WP pulses the next order's timer — AttackArea, Time on Target, or the next Goto WP — not the MCU itself. The next waypoint is reached through that timer, not a WP n → WP n+1 MCU link.",
                        );
                        ui.label(
                            RichText::new(
                                "Bombers: put Time on Target after the attack. The IP waypoint pulses the attack timer and the TOT timer; when TOT expires the chain continues. Use Mission Complete at the end to pulse MISSION END (Force Complete, RTB, Land cleanup).",
                            )
                            .italics()
                            .small(),
                        );
                        ui.horizontal(|ui| {
                            ui.label("Waypoint");
                            let max_wp = used_waypoint_count(&self.tpl_seats).max(1);
                            let wp = self.tpl_seats[seat].orders[order]
                                .waypoint
                                .clamp(1, max_wp);
                            self.tpl_seats[seat].orders[order].waypoint = wp;
                            egui::ComboBox::from_id_salt(format!("tpl_goto_{seat}_{order}"))
                                .selected_text(format!("WP {wp}"))
                                .width(100.0)
                                .show_ui(ui, |ui| {
                                    for n in 1..=max_wp {
                                        ui.selectable_value(
                                            &mut self.tpl_seats[seat].orders[order].waypoint,
                                            n,
                                            format!("WP {n}"),
                                        );
                                    }
                                });
                            if ui.button("New").on_hover_text("Add another waypoint after this hop").clicked()
                            {
                                let idx = insert_goto_waypoint_after(
                                    &mut self.tpl_seats,
                                    seat,
                                    order,
                                );
                                self.tpl_select = Some(TplSelect::Order { seat, order: idx });
                            }
                        });
                        ui.label(
                            RichText::new("Click a WP diamond on the diagram to pick it.")
                                .italics()
                                .small(),
                        );
                    }
                    OrderKind::TimeOnTarget => {
                        ui.label(
                            "Timer pulsed from the waypoint before Attack / AttackArea (not the previous delay). When it expires, the next order in the chain fires. Use this so the flight is updated after hanging on the target.",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Time on target");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].time_s,
                                )
                                .range(0.0..=3600.0)
                                .suffix(" s"),
                            );
                        });
                    }
                    OrderKind::MissionComplete => {
                        ui.label(
                            "Timer from the previous order (or Time on Target) pulses MISSION END: Force Complete, RTB if that order is on a unit, then deactivate / delete. Put Land or Force Complete before this if the flight should receive those commands first.",
                        );
                    }
                    OrderKind::ForceComplete => {
                        ui.label(
                            "MCU_CMD_ForceComplete on this unit (or shared Objects). Mission Complete pulses the shared MISSION END hub instead.",
                        );
                    }
                    OrderKind::RtbOnZoneOut => {
                        ui.label(
                            "On Zone Out this unit (or its lead) flies to an RTB waypoint. Deactivate waits 1 minute. Each placement group and coalition gets its own RTB. No RTB waypoint is written unless at least one unit has this order.",
                        );
                    }
                    OrderKind::AttackArea => {
                        let ground = self.tpl_seats[seat].orders[order].attack_ground
                            || self.tpl_seats[seat].orders[order].attack_g_targets;
                        ui.horizontal(|ui| {
                            ui.label("Area");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].attack_area,
                                )
                                .range(100.0..=20_000.0)
                                .suffix(" m"),
                            );
                            if ui
                                .small_button("Match range")
                                .on_hover_text(
                                    "Set the area to this unit’s (and Also-apply) system range, capped at 3 km.",
                                )
                                .clicked()
                            {
                                apply_suggested_attack_area(&mut self.tpl_seats, seat, order);
                            }
                            ui.label("Time");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].time_s,
                                )
                                .range(0.0..=3600.0)
                                .suffix(" s"),
                            );
                            ui.checkbox(
                                &mut self.tpl_seats[seat].orders[order].attack_air,
                                "Air",
                            );
                            ui.checkbox(
                                &mut self.tpl_seats[seat].orders[order].attack_ground,
                                "Ground",
                            );
                            ui.checkbox(
                                &mut self.tpl_seats[seat].orders[order].attack_g_targets,
                                "Ground targets",
                            );
                        });
                        let area = self.tpl_seats[seat].orders[order].attack_area;
                        if let Some(limit) =
                            attack_area_range_limit(&self.tpl_seats, seat, order)
                        {
                            if f64::from(area) > limit + 0.5 {
                                ui.label(
                                    RichText::new(format!(
                                        "Area {:.0} m is larger than this unit’s {:.1} km range. The far edge of the bubble is out of reach.",
                                        area,
                                        limit / 1000.0
                                    ))
                                    .color(Color32::from_rgb(200, 160, 80)),
                                );
                            } else {
                                ui.label(
                                    RichText::new(format!(
                                        "This system reaches {:.1} km. On the map the group parks within that of a hashed objective, and this MCU (ground / ground targets) is moved onto that objective.",
                                        limit / 1000.0
                                    ))
                                    .italics()
                                    .small(),
                                );
                            }
                        } else if ground {
                            ui.label(
                                RichText::new(
                                    "Ground / ground-target AttackArea sits on the group origin here. Generate on the Map moves it onto the hashed objective.",
                                )
                                .italics()
                                .small(),
                            );
                        }
                        ui.label(
                            RichText::new(
                                "MCU Time is how long AttackArea runs. Time on Target in the chain is a separate timer from the waypoint, for leaving the target.",
                            )
                            .italics()
                            .small(),
                        );
                    }
                    OrderKind::Formation => {
                        ui.horizontal(|ui| {
                            ui.label("Formation");
                            let current = formation_label(
                                self.tpl_seats[seat].orders[order].formation_type,
                                if self.tpl_seats[seat].unit.is_air() {
                                    CatalogKind::Plane
                                } else {
                                    CatalogKind::Vehicle
                                },
                            );
                            egui::ComboBox::from_id_salt(format!("tpl_form_{seat}_{order}"))
                                .selected_text(current)
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    let kind = if self.tpl_seats[seat].unit.is_air() {
                                        CatalogKind::Plane
                                    } else {
                                        CatalogKind::Vehicle
                                    };
                                    for preset in formations_for(kind) {
                                        ui.selectable_value(
                                            &mut self.tpl_seats[seat].orders[order].formation_type,
                                            preset.id,
                                            preset.label,
                                        );
                                    }
                                });
                        });
                    }
                    OrderKind::Behaviour => {
                        ui.horizontal(|ui| {
                            ui.label("Filter");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].behaviour_filter,
                                )
                                .range(0..=32),
                            );
                        });
                    }
                    OrderKind::Flare => {
                        ui.horizontal(|ui| {
                            ui.label("Color");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].flare_color,
                                )
                                .range(0..=4),
                            );
                        });
                    }
                    OrderKind::Effect => {
                        ui.checkbox(
                            &mut self.tpl_seats[seat].orders[order].effect_start,
                            "Start (off = stop)",
                        );
                    }
                    OrderKind::Cover => {
                        ui.label(
                            "Cover another unit. If this seat is a flight lead with wingmen, CoverGroup is set and the order goes to the lead (lead-to-lead). Independent units cover the chosen unit directly.",
                        );
                        let others = order_seat_indexes(&self.tpl_seats)
                            .into_iter()
                            .filter(|&i| i != seat)
                            .collect::<Vec<_>>();
                        if others.is_empty() {
                            ui.label(
                                RichText::new("Add another unit to cover.")
                                    .italics(),
                            );
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Cover");
                                let current = self.tpl_seats[seat].orders[order]
                                    .cover_lead
                                    .and_then(|i| {
                                        self.tpl_seats.get(i).map(|s| {
                                            format!("Seat {} ({})", i + 1, s.unit.label())
                                        })
                                    })
                                    .unwrap_or_else(|| "Choose unit…".into());
                                egui::ComboBox::from_id_salt(format!("tpl_cover_{seat}_{order}"))
                                    .selected_text(current)
                                    .width(220.0)
                                    .show_ui(ui, |ui| {
                                        for i in &others {
                                            let text = format!(
                                                "Seat {} ({})",
                                                i + 1,
                                                self.tpl_seats[*i].unit.label()
                                            );
                                            ui.selectable_value(
                                                &mut self.tpl_seats[seat].orders[order].cover_lead,
                                                Some(*i),
                                                text,
                                            );
                                        }
                                    });
                            });
                        }
                    }
                    OrderKind::Land => {
                        ui.horizontal(|ui| {
                            ui.label("Priority");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].priority,
                                )
                                .range(0..=2),
                            );
                        });
                    }
                    OrderKind::TakeOff => {
                        ui.label(
                            "MCU_CMD_TakeOff. Put OnTookOff after this so the next order waits until airborne.",
                        );
                    }
                    OrderKind::OnSpawned
                    | OrderKind::OnTargetAttacked
                    | OrderKind::OnAreaAttacked
                    | OrderKind::OnTookOff
                    | OrderKind::OnLanded => {
                        let hint = match self.tpl_seats[seat].orders[order].kind {
                            OrderKind::OnSpawned => {
                                "OnSpawned: CmdId = spawner, TarId = this timer, then the next order. Use Spawn Units."
                            }
                            OrderKind::OnTargetAttacked => {
                                "OnTargetAttacked: waits until Attack finishes, then this timer, then the next order."
                            }
                            OrderKind::OnAreaAttacked => {
                                "OnAreaAttacked: waits until AttackArea Time runs out, then this timer, then the next order."
                            }
                            OrderKind::OnTookOff => {
                                "OnTookOff: waits until Take Off finishes, then this timer, then the next order."
                            }
                            OrderKind::OnLanded => {
                                "OnLanded: waits until Land finishes, then this timer, then the next order."
                            }
                            _ => "",
                        };
                        ui.label(hint);
                        if self.tpl_seats[seat].orders[order].kind == OrderKind::OnSpawned
                            && self.tpl_bring_up != BringUp::Spawn
                        {
                            ui.label(
                                RichText::new(
                                    "Spawn Units is off: this timer still starts the chain from AFTER BRING UP.",
                                )
                                .italics()
                                .small(),
                            );
                        }
                        ui.horizontal(|ui| {
                            ui.label("Timer");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.tpl_seats[seat].orders[order].delay_s,
                                )
                                .range(0.0..=60.0)
                                .suffix(" s")
                                .speed(0.1),
                            );
                        });
                        let unit_kind = if self.tpl_seats[seat].unit.is_air() {
                            CatalogKind::Plane
                        } else {
                            CatalogKind::Vehicle
                        };
                        let then_label = self.tpl_seats[seat]
                            .orders
                            .get(order + 1)
                            .filter(|o| !o.kind.is_report())
                            .map(|o| o.kind.label())
                            .unwrap_or("Choose next order…");
                        ui.horizontal(|ui| {
                            ui.label("Then");
                            egui::ComboBox::from_id_salt(format!("tpl_rep_then_{seat}_{order}"))
                                .selected_text(then_label)
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    for k in OrderKind::following(unit_kind) {
                                        let selected = self
                                            .tpl_seats[seat]
                                            .orders
                                            .get(order + 1)
                                            .is_some_and(|o| o.kind == k);
                                        if ui.selectable_label(selected, k.label()).clicked() {
                                            set_report_following(
                                                &mut self.tpl_seats[seat].orders,
                                                order,
                                                k,
                                                unit_kind,
                                            );
                                        }
                                    }
                                });
                        });
                    }
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
                        .map(|i| {
                            (
                                i,
                                format!("Seat {} ({})", i + 1, self.tpl_seats[i].unit.label()),
                            )
                        })
                        .collect();
                    if others.is_empty() {
                        ui.label(RichText::new("Add another unit to share this order.").italics());
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            for (i, label) in &others {
                                let mut on = self.tpl_seats[seat].orders[order]
                                    .shared_with
                                    .contains(i);
                                if ui.checkbox(&mut on, label).changed() {
                                    let list =
                                        &mut self.tpl_seats[seat].orders[order].shared_with;
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
            }
            Some(TplSelect::Event { seat, event })
                if seat < self.tpl_seats.len() && event < self.tpl_seats[seat].events.len() =>
            {
                ui.label(RichText::new("Event details").strong());
                ui.label(
                    "OnEvent (TarId only). Links this unit to Force Complete or an order timer.",
                );
                let unit_kind = self.tpl_seats[seat].unit.kind;
                let chain_si = if receives_orders(&self.tpl_seats, seat) {
                    seat
                } else {
                    flight_lead_of(&self.tpl_seats, seat)
                };
                ui.horizontal(|ui| {
                    ui.label(format!("Seat {} · Event {}", seat + 1, event + 1));
                    let kind = self.tpl_seats[seat].events[event].kind;
                    egui::ComboBox::from_id_salt(format!("tpl_evt_kind_{seat}_{event}"))
                        .selected_text(kind.label())
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for k in EntityEvent::available(unit_kind) {
                                ui.selectable_value(
                                    &mut self.tpl_seats[seat].events[event].kind,
                                    *k,
                                    k.label(),
                                );
                            }
                        });
                    if ui.small_button("Remove event").clicked() {
                        self.tpl_seats[seat].events.remove(event);
                        self.tpl_select = Some(TplSelect::Seat(seat));
                        return;
                    }
                });
                if seat >= self.tpl_seats.len() || event >= self.tpl_seats[seat].events.len() {
                    return;
                }
                let then_orders = self.tpl_seats[chain_si].orders.clone();
                let current = self.tpl_seats[seat].events[event]
                    .then
                    .label(&then_orders);
                ui.horizontal(|ui| {
                    ui.label("Then");
                    egui::ComboBox::from_id_salt(format!("tpl_evt_then_{seat}_{event}"))
                        .selected_text(current)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.tpl_seats[seat].events[event].then,
                                EventThen::ForceComplete,
                                "Force Complete",
                            );
                            for (i, o) in then_orders.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.tpl_seats[seat].events[event].then,
                                    EventThen::Order(i),
                                    format!("{} {}", i + 1, o.kind.label()),
                                );
                            }
                        });
                });
            }
            _ => {
                ui.label(RichText::new("Select a unit or order in the list, or a dot on the diagram.").italics());
            }
        }
    }

    fn draw_template_model_browser(&mut self, ui: &mut egui::Ui) {
        let classes = self.classes_for_kind(self.tpl_kind);
        if self.tpl_class.is_some_and(|c| !classes.contains(&c)) {
            self.tpl_class = None;
        }
        if !classes.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Type").strong());
                if ui
                    .selectable_label(self.tpl_class.is_none(), "All")
                    .clicked()
                {
                    self.tpl_class = None;
                    self.tpl_add_pick = 0;
                    self.tpl_preview_from_catalog = true;
                }
                for class in classes {
                    if ui
                        .selectable_label(self.tpl_class == Some(class), class.label())
                        .clicked()
                    {
                        self.tpl_class = Some(class);
                        self.tpl_add_pick = 0;
                        self.tpl_preview_from_catalog = true;
                    }
                }
            });
        }

        let models = self.displayed_catalog();
        if models.is_empty() {
            ui.label(
                RichText::new("No models of this kind in the catalog yet.").italics(),
            );
            return;
        }
        if self.tpl_add_pick >= models.len() {
            self.tpl_add_pick = 0;
        }
        let selected_script = models
            .get(self.tpl_add_pick)
            .map(|u| u.script.clone())
            .unwrap_or_default();

        ui.add_space(4.0);
        if let Some(unit) = self.draw_model_button_grid(ui, &models, Some(&selected_script)) {
            if let Some(i) = models.iter().position(|u| u.script == unit.script) {
                self.tpl_add_pick = i;
            }
            self.tpl_preview_from_catalog = true;
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Add unit").clicked() {
                if let Some(unit) = models.get(self.tpl_add_pick).cloned() {
                    append_seat(&mut self.tpl_seats, unit, self.tpl_per_group);
                    self.tpl_select = Some(TplSelect::Seat(self.tpl_seats.len() - 1));
                    self.tpl_preview_from_catalog = false;
                }
            }
            if !self.tpl_seats.is_empty()
                && ui
                    .button("Copy attributes to all")
                    .on_hover_text(
                        "Copy country, skill, fuel, payload, and flags from the selected unit onto every other unit.",
                    )
                    .clicked()
            {
                let from = match self.tpl_select {
                    Some(TplSelect::Seat(si)) => si,
                    Some(TplSelect::Order { seat, .. } | TplSelect::Event { seat, .. }) => seat,
                    None => 0,
                };
                copy_seat_attributes(&mut self.tpl_seats, from);
            }
        });
    }

    fn draw_model_button_grid(
        &self,
        ui: &mut egui::Ui,
        models: &[CatalogUnit],
        selected_script: Option<&str>,
    ) -> Option<CatalogUnit> {
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for unit in models {
                let selected = selected_script.is_some_and(|s| s.eq_ignore_ascii_case(&unit.script));
                let btn = egui::Button::new(RichText::new(unit.label()).small())
                    .min_size(Vec2::new(124.0, 30.0))
                    .selected(selected);
                if ui.add(btn).clicked() {
                    picked = Some(unit.clone());
                }
            }
        });
        picked
    }

    fn draw_model_preview(&mut self, ui: &mut egui::Ui, unit: Option<&CatalogUnit>, caption: &str) {
        ui.label(
            RichText::new(caption)
                .small()
                .color(Color32::from_rgb(150, 150, 160)),
        );
        let Some(unit) = unit else {
            ui.label(RichText::new("Select a model.").italics());
            return;
        };
        ui.label(RichText::new(unit.label()).strong());
        let ctx = ui.ctx().clone();
        let tex = self.model_texture(&ctx, &unit.script);
        let size = tex.size_vec2();
        let max_w = ui.available_width().max(1.0);
        let scale = (max_w / size.x).min(110.0 / size.y).min(1.0);
        ui.add(egui::Image::new((tex.id(), size * scale)));
        let class = model_spec::class_for(&unit.script);
        let cruise = model_spec::spec_for(&unit.script)
            .map(|s| s.cruise_line())
            .unwrap_or_else(|| model_spec::format_cruise(None));
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(format!("Type: {}", class.label()));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(format!("Cruise speed: {cruise}"));
        });
        if let Some(spec) = model_spec::spec_for(&unit.script) {
            if spec.ceiling_m > 0.0 {
                let ft = spec.ceiling_m * 3.280_84;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Ceiling: {:.0} m / {:.0} ft", spec.ceiling_m, ft));
                });
            }
            if !spec.notes.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(spec.notes).small().weak());
                });
            }
        }
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("Skins: —").small());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("Loadouts: —").small());
        });
    }

    fn schematic_preview_unit(&self) -> Option<(CatalogUnit, String)> {
        if self.tpl_preview_from_catalog {
            let models = self.displayed_catalog();
            return models.get(self.tpl_add_pick).cloned().map(|u| (u, "Catalog".into()));
        }
        let seat = match self.tpl_select {
            Some(
                TplSelect::Seat(s)
                | TplSelect::Order { seat: s, .. }
                | TplSelect::Event { seat: s, .. },
            ) => Some(s),
            None => None,
        };
        if let Some(s) = seat {
            if s < self.tpl_seats.len() {
                return Some((
                    self.tpl_seats[s].unit.clone(),
                    format!("Seat {}", s + 1),
                ));
            }
        }
        let models = self.displayed_catalog();
        models
            .get(self.tpl_add_pick)
            .cloned()
            .map(|u| (u, "Catalog".into()))
    }

    fn model_texture(&mut self, ctx: &egui::Context, script: &str) -> TextureHandle {
        let id = model_spec::script_id(script);
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

    fn displayed_catalog(&self) -> Vec<CatalogUnit> {
        let mut models: Vec<CatalogUnit> = self
            .tpl_catalog
            .iter()
            .filter(|u| u.kind == self.tpl_kind)
            .filter(|u| {
                self.tpl_class
                    .map(|c| model_spec::class_for(&u.script) == c)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        models.sort_by(|a, b| a.label().cmp(b.label()));
        models
    }

    fn load_unit_catalog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
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
                "Catalog has no Plane / Vehicle / Train / Ship / Fixed prototypes. Use subgroups named Planes, Vehicles, Trains, Ships, Fixed Units / Fixed Objects, or User Added.".into(),
            );
            return;
        }
        let n = cat.len();
        self.tpl_catalog = cat;
        self.tpl_path = Some(path);
        self.tpl_class = None;
        self.tpl_add_pick = 0;
        self.tpl_preview_from_catalog = true;
        self.status = Status::Info(format!("Loaded {n} prototype(s) from the catalog."));
    }

    fn add_user_catalog_group(&mut self) {
        let Some(path) = rfd::FileDialog::new()
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
                "That group has no Plane / Vehicle / Train / Ship / Fixed prototypes to add.".into(),
            );
            return;
        }
        let n = extra.len();
        merge_catalog(&mut self.tpl_catalog, extra);
        self.tpl_kind = CatalogKind::UserAdded;
        self.tpl_class = None;
        self.tpl_add_pick = 0;
        self.tpl_preview_from_catalog = true;
        self.tpl_path = Some(path);
        self.status = Status::Info(format!("Appended {n} prototype(s) to User Added."));
    }

    fn generate_unit_template(&mut self) {
        if self.tpl_zone_out < self.tpl_zone_in + 200.0 {
            self.tpl_zone_out = self.tpl_zone_in + 200.0;
        }
        let opts = TemplateOptions {
            name: self
                .tpl_path
                .as_ref()
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
            waypoint_spacing: self.tpl_wp_spacing,
            waypoint_speed: self.tpl_wp_speed,
            zone_coalition: self.tpl_zone_coalition,
        };
        let pack = match generate_template(&opts) {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let text = serialize_group(&pack);
        let Some(save_path) = rfd::FileDialog::new()
            .add_filter("IL-2 Group", &["Group"])
            .set_file_name("Unit_Template.Group")
            .save_file()
        else {
            return;
        };
        let locale = self.tpl_path.as_ref().map(|p| vec![p.clone()]).unwrap_or_default();
        self.status = save_with_sidecars(
            &save_path,
            &text,
            &locale,
            &format!(
                "Wrote {} units ({}) with Zone IN/Out and MISSION END cleanup",
                opts.seats.len(),
                opts.bring_up.label()
            ),
        );
    }

    fn draw_template_schematic(&mut self, ui: &mut egui::Ui) {
        let preview_w = 252.0;
        let height = 380.0;
        ui.horizontal(|ui| {
        let preview = self.schematic_preview_unit();
        let caption = preview
            .as_ref()
            .map(|(_, c)| c.clone())
            .unwrap_or_else(|| "Catalog".into());
        let unit = preview.map(|(u, _)| u);
        ui.allocate_ui(Vec2::new(preview_w, height), |ui| {
            let pr = ui.max_rect();
            let panel_fill = ui.visuals().panel_fill;
            ui.painter().rect_filled(pr, 4.0, panel_fill);
            ui.painter().rect_stroke(
                pr,
                4.0,
                Stroke::new(
                    1.0_f32,
                    Color32::from_rgb(
                        panel_fill.r().saturating_add(18),
                        panel_fill.g().saturating_add(18),
                        panel_fill.b().saturating_add(18),
                    ),
                ),
                egui::StrokeKind::Inside,
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_max_width(preview_w - 16.0);
                    self.draw_model_preview(ui, unit.as_ref(), &caption);
                });
            });
        });
        let map_w = (ui.available_width() - preview_w - ui.spacing().item_spacing.x).max(180.0);
        let size = Vec2::new(map_w, height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 30, 34));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, Color32::from_rgb(48, 52, 58)),
            egui::StrokeKind::Inside,
        );

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
        painter.circle_stroke(
            origin,
            (self.tpl_zone_in * scale).max(2.0),
            Stroke::new(1.5_f32, Color32::from_rgb(70, 150, 110)),
        );
        painter.circle_stroke(
            origin,
            (self.tpl_zone_out * scale).max(2.0),
            Stroke::new(1.5_f32, Color32::from_rgb(170, 90, 80)),
        );
        painter.text(
            to_screen(0.0, self.tpl_zone_in as f64),
            Align2::LEFT_CENTER,
            "IN",
            FontId::proportional(11.0),
            Color32::from_rgb(110, 190, 140),
        );
        painter.text(
            to_screen(0.0, self.tpl_zone_out as f64),
            Align2::LEFT_CENTER,
            "OUT",
            FontId::proportional(11.0),
            Color32::from_rgb(200, 130, 120),
        );
        if let Some(area) = selected_area {
            painter.circle_stroke(
                origin,
                (area * scale).max(2.0),
                Stroke::new(1.4_f32, Color32::from_rgb(210, 160, 70)),
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
                    Stroke::new(1.4_f32, Color32::from_rgb(80, 140, 160)),
                );
            }
            for &(num, p) in &wp_pts {
                let sel = selected_wp == Some(num);
                let r = if sel { 8.0 } else { 6.5 };
                let color = if sel {
                    Color32::from_rgb(255, 170, 60)
                } else {
                    Color32::from_rgb(120, 190, 210)
                };
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
                    FontId::proportional(11.0),
                    Color32::from_rgb(170, 210, 220),
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
                Stroke::new(1.0_f32, Color32::from_rgb(58, 64, 74)),
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
                        Stroke::new(1.6_f32, Color32::from_rgb(90, 110, 140)),
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
                Color32::from_rgb(255, 170, 60)
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
                CatalogKind::Vehicle | CatalogKind::Fixed => {
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
                painter.circle_stroke(*p, r + 5.0, Stroke::new(1.6_f32, Color32::from_rgb(255, 210, 70)));
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
                FontId::proportional(11.0),
                Color32::from_rgb(210, 210, 220),
            );
        }

        if let Some(i) = hover {
            let seat = &self.tpl_seats[i];
            let alt = if seat.unit.is_air() {
                format!(" · {:.0} m", seat.altitude)
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
            painter.text(
                rect.min + Vec2::new(10.0, 28.0),
                Align2::LEFT_TOP,
                text,
                FontId::proportional(13.0),
                Color32::from_rgb(255, 230, 160),
            );
        } else if let Some(num) = hover_wp {
            painter.text(
                rect.min + Vec2::new(10.0, 28.0),
                Align2::LEFT_TOP,
                format!(
                    "WP {num} · {:.0} m north of origin · Area 200 m",
                    (num as f32) * self.tpl_wp_spacing
                ),
                FontId::proportional(13.0),
                Color32::from_rgb(180, 230, 240),
            );
        }

        painter.text(
            rect.min + Vec2::new(10.0, 8.0),
            Align2::LEFT_TOP,
            format!(
                "{} · {} / group · 150 m   Zone IN {:.1} km · Out {:.1} km   N up · scroll zoom · right-drag pan",
                self.tpl_place_layout.label(),
                self.tpl_per_group,
                self.tpl_zone_in / 1000.0,
                self.tpl_zone_out / 1000.0
            ),
            FontId::proportional(11.0),
            Color32::from_rgb(170, 170, 180),
        );
        painter.text(
            Pos2::new(rect.center().x, rect.min.y + 10.0),
            Align2::CENTER_TOP,
            "N",
            FontId::proportional(12.0),
            Color32::from_rgb(140, 160, 180),
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

        });
    }

    fn fighter_panel(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Fighter Pack", HelpTopic::Fighter);
        ui.label(
            RichText::new(
                "Group logic is built in. Configure flights, then generate a linked N-pack.",
            )
        );
        ui.add_space(10.0);

        self.pack_section(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        self.flight_section(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        self.types_section(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        self.country_section(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        self.timers_section(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        self.altitude_section(ui);
        ui.add_space(16.0);

        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let generate = egui::Button::new(RichText::new("Generate File").strong())
                .min_size(Vec2::new(200.0, 32.0));
            if ui.add(generate).clicked() {
                self.generate_fighter_file();
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);
        self.optional_template_section(ui);
    }

    fn bomber_panel(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Exclusive Activation", HelpTopic::Exclusive);
        ui.label(
            RichText::new(
                "This mode takes existing groups that may have complex mission logic or are known to be resource intensive and only allows a single one to activate at a time.
The start checkzones triggers it's mission profile and closes the others until the end timer fires.
A good example of when to use this, is preplanned bomber flights. 
New templates park on a square 10 km grid from 40000, 40000 unless 'export in place' is selected.",
            )
        );
        ui.label(
            RichText::new(format!(
                "Name start zones {SUGGESTED_TRIGGER_NAMES}. Name the end timer {SUGGESTED_END_NAMES}; it should target a Deactivate and/or Delete MCU whose Objects are the units in the template."
            ))
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Add template or pack…").clicked() {
                self.add_bomber_template();
            }
        });
        ui.checkbox(
            &mut self.bomber_keep_positions,
            "Export in place (leave groups where they are)",
        );
        ui.label(
            RichText::new(
                "Load a generated Exclusive Activation .Group to list its plans, add another template, and write back without moving them.",
            )
            .color(Color32::from_rgb(160, 160, 170)),
        );
        ui.add_space(6.0);

        if self.bomber_slots.is_empty() {
            ui.label(
                RichText::new("No templates yet — add a .Group to begin.")
                    .italics()
            );
        }

        let mut remove = None;
        let mut duplicate = None;
        for i in 0..self.bomber_slots.len() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}.", i + 1)).strong());
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&self.bomber_slots[i].info.name).strong());
                        let file = self.bomber_slots[i]
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
						ui.label(RichText::new(file));
                        ui.label(
                            RichText::new(format!(
                                "{} units",
                                self.bomber_slots[i].info.unit_count
                            ))
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                        if ui.small_button("Add again").clicked() {
                            duplicate = Some(i);
                        }
                    });
                });

                show_missing_locale_hint(ui, &self.bomber_slots[i].path);

                let slot = &mut self.bomber_slots[i];
                let zones = slot.info.checkzones.clone();
                let suggested_triggers = slot.info.suggested_triggers.clone();
                let timers = slot.info.timers.clone();
                let suggested_end = slot.info.suggested_completion;
                let many_zones = zones.len() > 1;
                let end_missing = slot.selected_completion.is_none();

                if many_zones {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Multiple checkzones — select which ones this plan should enable and disable.")
                            .color(Color32::from_rgb(180, 150, 70)),
                    );
                }

                for zone in &zones {
                    let mut on = slot.selected_triggers.contains(&zone.index);
                    let suggested = suggested_triggers.contains(&zone.index);
                    let label = if suggested {
                        format!("{}  (suggested)", zone.name)
                    } else {
                        zone.name.clone()
                    };
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            if !slot.selected_triggers.contains(&zone.index) {
                                slot.selected_triggers.push(zone.index);
                            }
                        } else {
                            slot.selected_triggers.retain(|id| *id != zone.index);
                        }
                    }
                }
                if slot.selected_triggers.is_empty() {
                    ui.label(
                        RichText::new("Select at least one checkzone.")
                            .color(Color32::from_rgb(200, 90, 90)),
                    );
                }
                for id in &slot.selected_triggers {
                    if let Some(msg) = slot.info.trigger_warnings.get(id) {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(msg)
                                .color(Color32::from_rgb(200, 90, 90)),
                        );
                    }
                }

                ui.add_space(6.0);
                if end_missing {
                    ui.label(
                        RichText::new(format!(
                            "No end timer was detected. Select the MCU_Timer that finishes the plan. Name it {SUGGESTED_END_NAMES}; it should target a Deactivate and/or Delete MCU that lists the units."
                        ))
                        .color(Color32::from_rgb(180, 150, 70)),
                    );
                } else {
					ui.label(RichText::new("End timer"));
                }

                let selected_label = slot
                    .selected_completion
                    .and_then(|id| timers.iter().find(|t| t.index == id).map(|t| t.name.clone()))
                    .unwrap_or_else(|| "Select end trigger timer…".into());
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
                            ui.selectable_value(
                                &mut slot.selected_completion,
                                Some(timer.index),
                                text,
                            );
                        }
                    });
                if let Some(id) = slot.selected_completion {
                    if let Some(msg) = slot.info.cleanup_warnings.get(&id) {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(msg)
                                .color(Color32::from_rgb(200, 90, 90)),
                        );
                    }
                }
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            self.bomber_slots.remove(i);
        }
        if let Some(i) = duplicate {
            let copy = BomberSlot {
                path: self.bomber_slots[i].path.clone(),
                root: self.bomber_slots[i].root.clone(),
                info: self.bomber_slots[i].info.clone(),
                selected_triggers: self.bomber_slots[i].selected_triggers.clone(),
                selected_completion: self.bomber_slots[i].selected_completion,
            };
            self.bomber_slots.push(copy);
        }

        ui.add_space(12.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let generate = egui::Button::new(RichText::new("Generate File").strong())
                .min_size(Vec2::new(200.0, 32.0));
            if ui.add(generate).clicked() {
                self.generate_bomber_file();
            }
        });
    }

    fn recon_panel(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Army Generator", HelpTopic::Recon);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.recon_submode, ReconSubmode::New, "New From Templates");
            ui.selectable_value(&mut self.recon_submode, ReconSubmode::Rework, "Rework Existing");
        });
        ui.add_space(8.0);
        match self.recon_submode {
            ReconSubmode::New => self.recon_new_panel(ui),
            ReconSubmode::Rework => self.recon_rework_panel(ui),
        }
    }

    fn airfield_panel(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Task Editor Airfield to Multiplayer", HelpTopic::Airfield);
        ui.label(
            RichText::new(
                "Freeflight airfields from the Task Editor include a player aircraft and SP logic that multiplayer does not use.
This mode strips the player and retargets the checkzones that were object-linked to it quickly making it multiplayer ready.
Be advised, adding in planes to fly and setting the starting location is still required.",
            )
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "In game open a Freeflight mission and select takeoff from the desired airfield.
Use the offical mission editor to open the /missions/_gen.mission file, select the desired field and use File, Save Selection to File. 
Load the messy airfield group here — it may sit in a Group wrapper or as loose blocks at the root in your group.",
            )
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Load airfield…").clicked() {
                self.load_airfield();
            }
            if let Some(path) = &self.airfield_path {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                ui.label(RichText::new(name).italics());
            }
        });
        ui.add_space(8.0);

        ui.label("Friendly plane coalition (USA airfields: Western)");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.airfield_western, true, "Western  [2]");
            ui.selectable_value(&mut self.airfield_western, false, "Eastern  [1]");
        });
        ui.add_space(8.0);

        if let Some(info) = &self.airfield_info {
            ui.separator();
            ui.add_space(6.0);
            ui.label(RichText::new("Airfield").strong());
            ui.label(format!("Name: {}", info.name));
            ui.label(format!(
                "Layout: {}",
                if info.in_group {
                    "inside a Group"
                } else {
                    "blocks at the root"
                }
            ));
            if let Some((x, z)) = info.origin_xz {
                ui.label(format!("Origin: {x:.0}, {z:.0}"));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("On the field").strong());
            ui.label(format!("{} vehicles/ships", info.vehicle_count));
            ui.label(format!("{} AI aircraft", info.ai_plane_count));
            ui.label(format!("{} blocks", info.block_count));
            ui.label(format!("{} checkzones", info.checkzone_count));
            ui.add_space(6.0);
            ui.label(RichText::new("Player (will be removed)").strong());
            if info.player_planes.is_empty() {
                ui.label(
                    RichText::new("No player aircraft found — this file may already be cleaned.")
                        .color(Color32::from_rgb(180, 150, 70)),
                );
            } else {
                for p in &info.player_planes {
                    let country = COUNTRIES
                        .iter()
                        .find(|(id, _)| *id == p.country)
                        .map(|(_, label)| *label)
                        .unwrap_or("unknown country");
                    ui.label(format!("{}  ({country})", p.name));
                }
            }
            if info.has_autoremove {
                ui.label(
                    RichText::new("AutoRemove subgroup will be deleted.")
                );
            }
            ui.label(format!(
                "{} objects in the player / SP graph will be stripped",
                info.strip_count
            ));
            ui.add_space(6.0);
            ui.label(RichText::new("Checkzones to unlink").strong());
            if info.unlink_zones.is_empty() {
                ui.label(
                    RichText::new("None object-linked to the player.")
                );
            } else {
                ui.label(
                    RichText::new(format!(
                        "{} zone(s) will drop the player object link and use {}.",
                        info.unlink_zones.len(),
                        if self.airfield_western {
                            "Western [2]"
                        } else {
                            "Eastern [1]"
                        }
                    ))
                );
                for name in &info.unlink_zones {
					ui.label(RichText::new(format!("  {name}")));
                }
            }
            ui.add_space(12.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                let export = egui::Button::new(RichText::new("Export File").strong())
                    .min_size(Vec2::new(200.0, 32.0));
                if ui.add(export).clicked() {
                    self.export_airfield();
                }
            });
        } else {
            ui.label(
                RichText::new("Load an airfield group exported from _gen.mission.")
                    .italics()
            );
        }
    }

	fn map_panel(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Map", HelpTopic::Front);
        ui.label(
            RichText::new(
                "Draw a box on Korea to clip the period front and areas of influence.
Add reference groups (airfields, blocks) to stamp at their saved locations; they are trimmed to the box plus 10 km.",
            )
        );
        ui.add_space(6.0);

        self.handle_map_timeline_keys(ui);

        ui.horizontal(|ui| {
            ui.label("Year");
            egui::ComboBox::from_id_salt("front_year")
                .selected_text(self.front_year.to_string())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for y in YEARS {
                        if ui
                            .selectable_label(self.front_year == y, y.to_string())
                            .clicked()
                        {
                            self.front_year = y;
                            self.front_focus = None;
                            self.snap_timeline(timeline_index(self.front_year, self.front_season));
                        }
                    }
                });
            ui.label("Season");
            egui::ComboBox::from_id_salt("front_season")
                .selected_text(self.front_season.label())
                .width(140.0)
                .show_ui(ui, |ui| {
                    for s in Season::ALL {
                        if ui
                            .selectable_label(self.front_season == s, s.label())
                            .clicked()
                        {
                            self.front_season = s;
                            self.front_focus = None;
                            self.snap_timeline(timeline_index(self.front_year, self.front_season));
                        }
                    }
                });
        });

        let mark = self.current_mark();
        ui.label(RichText::new(mark.title).strong());
        ui.label(RichText::new(mark.note));
        ui.label(
            RichText::new(mark.editor_hint())
                .strong()
                .color(Color32::from_rgb(170, 200, 255)),
        );
        ui.add_space(4.0);
        let n_slots = TIMELINE.len().max(1);
        ui.label(RichText::new("Front date").strong());
        let slider_w = ui.available_width().max(640.0);
        let slider = egui::Slider::new(&mut self.front_t, 0.0..=(n_slots - 1) as f32)
            .show_value(false);
        if ui.add_sized([slider_w, 22.0], slider).changed() {
            self.custom_front_xz.clear();
            let i = self
                .front_t
                .round()
                .clamp(0.0, (n_slots - 1) as f32) as usize;
            self.apply_timeline_mark(i, true);
        }
        ui.add_space(6.0);
        ui.label("Focus on a battle");
        let period_battles = battles_in_period(self.front_year, self.front_season);
        let selected_battle = self
            .front_focus
            .and_then(|id| BATTLES.iter().find(|b| b.id == id));
        let focus_text = selected_battle
            .map(|b| b.name)
            .unwrap_or("Entire front (this period)");
        egui::ComboBox::from_id_salt("front_battle")
            .selected_text(focus_text)
            .width(360.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.front_focus.is_none(), "Entire front (this period)")
                    .clicked()
                {
                    self.front_focus = None;
                }
                ui.separator();
                ui.label(RichText::new("This period"));
                for b in &period_battles {
                    if ui
                        .selectable_label(self.front_focus == Some(b.id), b.name)
                        .clicked()
                    {
                        self.focus_battle(b);
                    }
                }
                ui.separator();
                ui.label(RichText::new("Jump to any battle"));
                for b in BATTLES {
                    let label = format!("{}  ({} {})", b.name, b.season.label(), b.year);
                    if ui
                        .selectable_label(self.front_focus == Some(b.id), label)
                        .clicked()
                    {
                        self.focus_battle(b);
                    }
                }
            });
        if let Some(b) = selected_battle {
            ui.label(RichText::new(b.note));
        }
        ui.add_space(6.0);

        ui.label(
            RichText::new(
                suggested_aircraft(self.front_year, self.front_season)
                    .iter()
                    .map(|a| a.label)
                    .collect::<Vec<_>>()
                    .join("  ·  "),
            )
            .color(Color32::from_rgb(110, 150, 110)),
        );
        ui.add_space(6.0);

        ui.with_layout(Layout::top_down(Align::Center), |ui| { 
            self.ensure_map_assets(ui.ctx());
            self.draw_korea_map(ui);
        });
        
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.vertical(|ui| {
                self.map_view_toolbar(ui);
                ui.separator();
                self.map_draw_toolbar(ui);
            });
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.vertical(|ui| {
                self.map_fighter_toolbar(ui);
                ui.separator();
                self.map_objectives_toolbar(ui);
                ui.separator();
                self.map_units_toolbar(ui);
            });
        });

        ui.add_space(4.0);
        
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "AABB  XPos {:.0} – {:.0}   ZPos {:.0} – {:.0}",
                    self.front_aabb.x_min, self.front_aabb.x_max, self.front_aabb.z_min, self.front_aabb.z_max
                )).monospace(),
            );
        });

        self.draw_map_legend(ui);
        ui.add_space(6.0);

        ui.collapsing("Loaded Reference Groups", |ui| {
            ui.label(RichText::new("Airfields/blocks/entities are stamped in the box plus 10 km. Landscape MCU_Waypoint marks show on the preview as nested dots but are not exported.").italics());
            ui.horizontal(|ui| {
                if ui.button("Add reference groups…").clicked() {
                    self.add_map_refs();
                }
            });
            let mut remove_at: Option<usize> = None;
            for (i, g) in self.map_refs.iter().enumerate() {
                ui.horizontal(|ui| {
                    let label = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or("group");
                    let xz = g.entity.first_xz().map(|(x, z)| format!("  X {x:.0}  Z {z:.0}")).unwrap_or_default();
                    ui.label(format!("{label}{xz}"));
                    if ui.small_button("Remove").clicked() {
                        remove_at = Some(i);
                    }
                });
            }
            if let Some(i) = remove_at {
                self.map_refs.remove(i);
            }
            if self.map_refs.is_empty() {
                ui.label(RichText::new("No reference groups loaded.").italics());
            }
        });
        
        ui.add_space(12.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let generate = egui::Button::new(RichText::new("Generate Base Map").strong())
                .min_size(Vec2::new(200.0, 32.0));
            if ui.add(generate).clicked() {
                self.generate_front_file();
            }
        });
    }
   
fn map_view_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Reset AO").on_hover_text("Reset Area of Operations box to full map").clicked() {
                self.front_aabb = WorldAabb::full_map();
                self.map_drag_uv = None;
            }
            ui.label("Zoom:");
            if ui.add(egui::Slider::new(&mut self.map_zoom, 1.0..=12.0).show_value(false)).changed() {
                self.clamp_map_pan();
            }
            ui.label(RichText::new(format!("{:.0}%", self.map_zoom * 100.0)));
            if ui.button("Reset View").on_hover_text("Reset map zoom and pan position").clicked() {
                self.map_zoom = 1.0;
                self.map_pan = Pos2::new(0.5, 0.5);
            }
        });
    }

	fn map_draw_toolbar(&mut self, ui: &mut egui::Ui) {
        // Handle shortcuts
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
            self.remove_last_mark();
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)) {
            self.redo_last_mark();
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Map Tools:").strong());
        });
		
        // Top Row: Primary Creation
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.map_drawing_mode, MapDrawingMode::None, "Select AO / Move Units")
                .on_hover_text("Left Click and drag to select Area of Operations (AO) or to move a placed unit / Right Click to Pan the map view.");
            ui.selectable_value(&mut self.map_drawing_mode, MapDrawingMode::BaseFront, "Draw Custom Front")
                .on_hover_text("Left Click sequentially (or Click and drag) from  West to East to draw a custom front.");
            ui.selectable_value(&mut self.map_drawing_mode, MapDrawingMode::Salient, "Draw Salient")
                .on_hover_text("Left Click sequentially (or Click and drag) to draw along the front. Right-click to finish.");
            ui.selectable_value(&mut self.map_drawing_mode, MapDrawingMode::AttackArrow, "Draw Attack Arrow")
                .on_hover_text("Drag from tail to tip. Colour follows the tail's side of the front.");
        });

        // Second Row: Modifiers
        ui.horizontal(|ui| {
            let has_custom = !self.custom_front_xz.is_empty() || !self.salients.is_empty() || !self.attack_arrows.is_empty();
            ui.add_enabled_ui(has_custom, |ui| {
                if ui.button("Clear Custom Lines").clicked() {
                    self.custom_front_xz.clear();
                    self.salients.clear();
                    self.current_salient.clear();
                    self.attack_arrows.clear();
                    self.attack_drag = None;
                    self.drawn_marks.clear();
                    self.clear_redo_stack();
                }
            });

            let has_marks = !self.salients.is_empty() || !self.current_salient.is_empty() || !self.attack_arrows.is_empty() || self.attack_drag.is_some();
            ui.add_enabled_ui(has_marks, |ui| {
                if ui.button("Undo").on_hover_text("Ctrl-Z").clicked() {
                    self.remove_last_mark();
                }
            });

            let has_redo = !self.redo_marks.is_empty();
            ui.add_enabled_ui(has_redo, |ui| {
                if ui.button("Redo").on_hover_text("Ctrl-Y").clicked() {
                    self.redo_last_mark();
                }
            });

            let has_salients = !self.salients.is_empty() || !self.current_salient.is_empty();
            ui.add_enabled_ui(has_salients, |ui| {
                if ui.button("Clear Salients").clicked() {
                    self.salients.clear();
                    self.current_salient.clear();
                    self.drawn_marks.retain(|m| *m != DrawnMark::Salient);
                }
            });

            let has_arrows = !self.attack_arrows.is_empty() || self.attack_drag.is_some();
            ui.add_enabled_ui(has_arrows, |ui| {
                if ui.button("Clear Attack Arrows").clicked() {
                    self.attack_arrows.clear();
                    self.attack_drag = None;
                    self.drawn_marks.retain(|m| *m != DrawnMark::AttackArrow);
                }
            });
        });
    }

    fn map_fighter_toolbar(&mut self, ui: &mut egui::Ui) {
		ui.horizontal(|ui| {
            ui.label(RichText::new("Fighters").strong());
			ui.label(RichText::new("Set in the 'Fighter Pack' mode"));
        });
		
        ui.horizontal(|ui| {
            if ui.button("Eastern").on_hover_text("Place Eastern fighters in their coalition zone").clicked() { self.place_map_fighters(true); }
            if ui.button("NATO").on_hover_text("Place NATO fighters in their coalition zone").clicked() { self.place_map_fighters(false); }
            
            ui.separator();
            labeled_slider(ui, "Number of Groups active at once:", &mut self.fighter_waves, 1..=6);
            ui.checkbox(&mut self.fighter_fill, format!("Fill AO (will allow up to {MAX_PACKS} at once)")).on_hover_text("Fill AO at Zone IN spacing");
            
            let has_fighters = self.map_fighters.as_ref().is_some_and(|l| !l.spots.is_empty());
            ui.add_enabled_ui(has_fighters, |ui| {
                if ui.button("Clear fighters").clicked() {
                    self.map_fighters = None;
                    self.fighter_drag = None;
                }
            });

            if let Some(layout) = &self.map_fighters {
                let n = layout.spots.len();
                let packs = layout.spots.iter().map(|s| s.pack).collect::<std::collections::BTreeSet<_>>().len();
                let side = if layout.eastern { "Eastern" } else { "NATO" };
                ui.label(RichText::new(format!("{side}: {n} grps in {packs} pack(s)")).color(Color32::from_rgb(180, 180, 190)));
            }
        });
    }

    fn map_objectives_toolbar(&mut self, ui: &mut egui::Ui) {
		ui.horizontal(|ui| {
            ui.label(RichText::new("Objectives").strong());
        });
        ui.horizontal(|ui| {
            let e_active = self.map_drawing_mode == MapDrawingMode::PlaceEastObjective;
            let n_active = self.map_drawing_mode == MapDrawingMode::PlaceNatoObjective;

            if ui.selectable_label(e_active, "Eastern").on_hover_text("Select Eastern objective, then click map").clicked() {
                self.map_drawing_mode = if e_active { MapDrawingMode::None } else { MapDrawingMode::PlaceEastObjective };
            }
            if ui.selectable_label(n_active, "NATO").on_hover_text("Select NATO objective, then click map").clicked() {
                self.map_drawing_mode = if n_active { MapDrawingMode::None } else { MapDrawingMode::PlaceNatoObjective };
            }
            
            let has_objs = !self.east_objectives.is_empty() || !self.nato_objectives.is_empty();
            ui.add_enabled_ui(has_objs, |ui| {
                if ui.button("Clear objectives").clicked() {
                    self.east_objectives.clear();
                    self.nato_objectives.clear();
                    self.objective_drag = None;
                    self.reaim_map_ground();
                }
            });
            ui.label(RichText::new(format!("Eastern {} · NATO {}", self.east_objectives.len(), self.nato_objectives.len())).color(Color32::from_rgb(180, 180, 190)));
        });
    }

    fn map_units_toolbar(&mut self, ui: &mut egui::Ui) {
		ui.horizontal(|ui| {
            ui.label(RichText::new("Units").strong());
			ui.label(RichText::new(
                "Units details may be set in the 'Army Generator' mode, or load groups to reposition.",
            ));
        });
        ui.horizontal(|ui| {            
            ui.add_enabled_ui(!self.recon_keep_positions, |ui| {
                if ui.button("Eastern").on_hover_text("Place Army Generator units along the front as Eastern").clicked() { self.place_map_units(true); }
                if ui.button("NATO").on_hover_text("Place Army Generator units along the front as NATO").clicked() { self.place_map_units(false); }
            });
            if ui.button("Load Eastern…").on_hover_text("Load a .Group as the Eastern army. Reposition along the front, or keep authored positions.").clicked() {
                self.load_map_armies(true);
            }
            if ui.button("Load NATO…").on_hover_text("Load a .Group as the NATO army. Reposition along the front, or keep authored positions.").clicked() {
                self.load_map_armies(false);
            }

            let has_units = self.map_ships.is_some()
                || self.map_ground_east.is_some()
                || self.map_ground_nato.is_some()
                || !self.map_armies.is_empty();
            ui.add_enabled_ui(has_units, |ui| {
                if ui.button("Clear units").clicked() {
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

            let mut parts = Vec::new();
            let e_n = self.map_ground_east.as_ref().map(|l| l.spots.len()).unwrap_or(0)
                + self.map_armies.iter().filter(|a| a.eastern).map(|a| {
                    a.ground.as_ref().map(|g| g.spots.len()).unwrap_or(0)
                        + a.ships.as_ref().map(|s| s.spots.len()).unwrap_or(0)
                }).sum::<usize>();
            let n_n = self.map_ground_nato.as_ref().map(|l| l.spots.len()).unwrap_or(0)
                + self.map_armies.iter().filter(|a| !a.eastern).map(|a| {
                    a.ground.as_ref().map(|g| g.spots.len()).unwrap_or(0)
                        + a.ships.as_ref().map(|s| s.spots.len()).unwrap_or(0)
                }).sum::<usize>();
            let ships = self.map_ships.as_ref().map(|l| l.spots.len()).unwrap_or(0);
            if e_n > 0 { parts.push(format!("E: {e_n}")); }
            if n_n > 0 { parts.push(format!("N: {n_n}")); }
            if ships > 0 { parts.push(format!("Ships: {ships}")); }
            
            if !parts.is_empty() {
                ui.label(RichText::new(parts.join(" · ")).color(Color32::from_rgb(180, 180, 190)));
            } else if self.recon_keep_positions {
                ui.label(RichText::new("Keep loaded positions is ON").color(Color32::from_rgb(200, 160, 80)));
            }
        });
        if !self.map_armies.is_empty() {
            let mut remove_at: Option<usize> = None;
            let mut toggle_at: Option<usize> = None;
            for (i, slot) in self.map_armies.iter().enumerate() {
                ui.horizontal(|ui| {
                    let name = slot
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("group");
                    let side = if slot.eastern { "Eastern" } else { "NATO" };
                    ui.label(format!("{side}  {name}  {}", army_mix_label(&slot.copies)));
                    let mut repo = slot.reposition;
                    if ui.checkbox(&mut repo, "Reposition").on_hover_text("Park along the front using detected unit types. Off keeps the group's authored X/Z.").changed() {
                        toggle_at = Some(i);
                    }
                    if ui.small_button("Remove").clicked() {
                        remove_at = Some(i);
                    }
                });
            }
            if let Some(i) = toggle_at {
                self.map_armies[i].reposition = !self.map_armies[i].reposition;
                self.refresh_army_slot(i);
            }
            if let Some(i) = remove_at {
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
	
    fn ensure_map_assets(&mut self, ctx: &egui::Context) {
        if self.fighter_tex_east.is_none() {
            self.fighter_tex_east = Some(ctx.load_texture(
                "eastern_fighter",
                load_fighter_svg(include_bytes!("../assets/EasternFighter.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.fighter_tex_nato.is_none() {
            self.fighter_tex_nato = Some(ctx.load_texture(
                "nato_fighter",
                load_fighter_svg(include_bytes!("../assets/NatoFighter.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.ship_tex_east.is_none() {
            self.ship_tex_east = Some(ctx.load_texture(
                "eastern_shipping",
                load_fighter_svg(include_bytes!("../assets/EasternShipping.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.ship_tex_nato.is_none() {
            self.ship_tex_nato = Some(ctx.load_texture(
                "nato_shipping",
                load_fighter_svg(include_bytes!("../assets/NatoShiping.svg")),
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
                load_fighter_svg(include_bytes!("../assets/EasternObjective.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.obj_tex_nato.is_none() {
            self.obj_tex_nato = Some(ctx.load_texture(
                "nato_objective",
                load_fighter_svg(include_bytes!("../assets/NatoObjective.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.armor_tex_east.is_none() {
            self.armor_tex_east = Some(ctx.load_texture(
                "eastern_armor",
                load_fighter_svg(include_bytes!("../assets/EasternArmor.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.armor_tex_nato.is_none() {
            self.armor_tex_nato = Some(ctx.load_texture(
                "nato_armor",
                load_fighter_svg(include_bytes!("../assets/NatoArmor.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.supply_tex_east.is_none() {
            self.supply_tex_east = Some(ctx.load_texture(
                "eastern_supply",
                load_fighter_svg(include_bytes!("../assets/EasternSupply.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.supply_tex_nato.is_none() {
            self.supply_tex_nato = Some(ctx.load_texture(
                "nato_supply",
                load_fighter_svg(include_bytes!("../assets/NatoSupply.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.arty_tex_east.is_none() {
            self.arty_tex_east = Some(ctx.load_texture(
                "eastern_arty",
                load_fighter_svg(include_bytes!("../assets/EasternArty.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.arty_tex_nato.is_none() {
            self.arty_tex_nato = Some(ctx.load_texture(
                "nato_arty",
                load_fighter_svg(include_bytes!("../assets/NatoArty.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.train_tex_east.is_none() {
            self.train_tex_east = Some(ctx.load_texture(
                "eastern_train",
                load_fighter_svg(include_bytes!("../assets/EasternTrain.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.train_tex_nato.is_none() {
            self.train_tex_nato = Some(ctx.load_texture(
                "nato_train",
                load_fighter_svg(include_bytes!("../assets/NatoTrain.svg")),
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
        let avail = ui.available_width().min(960.0);
        let size = lo.size_vec2();
        let scale = (avail / size.x).min(840.0 / size.y);
        let img_size = Vec2::new(size.x * scale, size.y * scale);
        let (rect, response) = ui.allocate_exact_size(img_size, Sense::click_and_drag());
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
        let end_snap = if self.current_salient.is_empty() {
            None
        } else {
            hover_xz
                .or_else(|| self.current_salient.last().copied())
                .and_then(|p| snap_to_front(&snap_line, p))
        };

        if let Some(pos) = response.interact_pointer_pos() {
            let uv = pos_to_uv(map_rect, pos);
            let (x, z) = uv_to_world(uv);

            match self.map_drawing_mode {
                MapDrawingMode::BaseFront => {
                    if response.dragged_by(egui::PointerButton::Primary)
                        || response.clicked_by(egui::PointerButton::Primary)
                    {
                        if can_extend_west_east(&self.custom_front_xz, (x, z), 2500.0) {
                            self.custom_front_xz.push((x, z));
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
                                self.drawn_marks.push(DrawnMark::AttackArrow);
								self.clear_redo_stack();
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
                                    spot.x = x;
                                    spot.z = z;
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

        let (composite_front, patches) = apply_salients(full_dense.clone(), &self.salients);
        let painter = ui.painter_at(rect);

        if self.map_drawing_mode != MapDrawingMode::None {
            let text = match self.map_drawing_mode {
                MapDrawingMode::BaseFront => "DRAWING BASE FRONT\nWest to east only. Timeline slider clears it.",
                MapDrawingMode::Salient => "DRAWING SALIENT\nClick or drag to draw. Light dot = start, dark dot = return.\nClick the dark dot or right-click to finish.",
                MapDrawingMode::AttackArrow => "DRAWING ATTACK ARROW\nDrag from tail to tip. Colour follows the tail's side of the front.",
                MapDrawingMode::PlaceEastObjective => "EASTERN OBJECTIVE\nClick to drop a preview marker. Right-click to remove. Not exported.",
                MapDrawingMode::PlaceNatoObjective => "NATO OBJECTIVE\nClick to drop a preview marker. Right-click to remove. Not exported.",
                MapDrawingMode::None => "",
            };
            painter.text(rect.min + Vec2::new(11.0, 11.0), Align2::LEFT_TOP, text, FontId::proportional(16.0), Color32::from_rgb(20, 20, 24));
            painter.text(rect.min + Vec2::new(10.0, 10.0), Align2::LEFT_TOP, text, FontId::proportional(16.0), Color32::from_rgb(255, 255, 100));
        } else if let Some((hit, wi)) = self.wp_selected {
            let n = hit.spot_i() + 1;
            let text = format!(
                "PLACE UNIT {n} WP{}\nClick a road or railroad — any branch, including behind the column.\nRight-click to cancel.",
                wi + 1
            );
            painter.text(rect.min + Vec2::new(11.0, 11.0), Align2::LEFT_TOP, &text, FontId::proportional(16.0), Color32::from_rgb(20, 20, 24));
            painter.text(rect.min + Vec2::new(10.0, 10.0), Align2::LEFT_TOP, &text, FontId::proportional(16.0), Color32::from_rgb(255, 255, 100));
        }

        let overlay = timeline_preview(self.front_t);

        draw_reference_overlays(&painter, map_rect);

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
            let salient_stroke = Stroke::new(1.5_f32, Color32::from_rgb(180, 180, 80));
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
        let box_rect = aabb_to_screen(map_rect, self.front_aabb);
        painter.rect_stroke(
            box_rect,
            0.0,
            Stroke::new(2.0_f32, Color32::from_rgb(255, 210, 70)),
            egui::StrokeKind::Outside,
        );
        painter.rect_filled(box_rect, 0.0, Color32::from_rgba_unmultiplied(255, 210, 70, 25));
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
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for spot in &layout.spots {
            let pos = world_to_pos(map_rect, spot.x, spot.z);
            if let Some(tex) = tex {
                painter.image(
                    tex.id(),
                    Rect::from_center_size(pos, size),
                    uv,
                    Color32::WHITE,
                );
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
                let Some(net) = &s.network else { continue };
                for (wi, &(x, z)) in net.waypoints.iter().enumerate() {
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
        if let Some(layout) = &mut self.map_ground_east {
            layout.aim_at_objectives(&self.east_objectives);
        }
        if let Some(layout) = &mut self.map_ground_nato {
            layout.aim_at_objectives(&self.nato_objectives);
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
                layout.aim_at_objectives(objs);
            }
        }
    }

    fn type_icons_east(&self) -> [Option<TextureHandle>; 5] {
        [
            self.ship_tex_east.clone(),
            self.armor_tex_east.clone(),
            self.supply_tex_east.clone(),
            self.arty_tex_east.clone(),
            self.train_tex_east.clone(),
        ]
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
            (true, UnitKind::Train) => self.train_tex_east.as_ref(),
            (false, UnitKind::Train) => self.train_tex_nato.as_ref(),
        }
    }

    fn unit_kind_picker(&mut self, ui: &mut egui::Ui) {
        let icons = self.type_icons_east();
        for kind in UnitKind::ALL {
            ui.vertical(|ui| {
                if unit_kind_icon_button(
                    ui,
                    icons[kind.index()].as_ref(),
                    kind.label(),
                    &kind.hover(),
                    self.recon_import_kind == kind,
                    28.0,
                ) {
                    self.recon_import_kind = kind;
                }
                ui.label(kind.label());
            });
        }
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
                        GroundKind::Supply | GroundKind::Train => None,
                    })
                };
                jobs.extend(std::iter::repeat(GroundJob {
                    kind,
                    range_m,
                    route,
                }).take(*n));
            }
        }
        jobs
    }

    fn recon_copy_split(&self) -> (usize, [usize; 4]) {
        let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
        let copies = allocate_copies(&weights, self.recon_total as usize);
        let mut ships = 0usize;
        let mut ground = [0usize; 4];
        for (slot, n) in self.recon_slots.iter().zip(copies) {
            match slot.kind {
                UnitKind::Ship => ships += n,
                UnitKind::Armor => ground[0] += n,
                UnitKind::Supply => ground[1] += n,
                UnitKind::Artillery => ground[2] += n,
                UnitKind::Train => ground[3] += n,
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
            (GroundKind::Train, ground[3]),
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
                if let Some(net) = &spot.network {
                    let wp_color = if net.rail {
                        Color32::from_rgb(30, 30, 32)
                    } else {
                        Color32::from_rgb(140, 28, 28)
                    };
                    for (wi, &(wx, wz)) in net.waypoints.iter().enumerate() {
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
            Stroke::new(1.15_f32, Color32::from_rgba_unmultiplied(110, 18, 18, 128)),
        );
        draw_network_lines(
            painter,
            map_rect,
            mapnet::railroads(),
            Stroke::new(1.35_f32, Color32::from_rgba_unmultiplied(16, 16, 18, 128)),
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
        let side = if eastern { "Eastern" } else { "NATO" };
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
                            "{open_n} {side} ground groups kept north heading — mark a {side} objective to aim them."
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
                        warnings.extend(numbered_ground_issues(
                            &full.spots,
                            objectives.is_empty(),
                        ));
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

        notes.push("Left-drag to move, right-drag to set heading. Click a WP, then click a road or rail to place it.".into());
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
        let Some(paths) = rfd::FileDialog::new()
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
                    heading_deg: 0.0,
                }),
                kind => {
                    if let Some(gkind) = UnitKind::from_army(kind).ground() {
                        ground_spots.push(GroundSpot::at(
                            copy.x,
                            copy.z,
                            in_ao,
                            0.0,
                            gkind,
                            None,
                        ));
                    }
                }
            }
        }
        let side = if eastern { "Eastern" } else { "NATO" };
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
        let side = if eastern { "Eastern" } else { "NATO" };
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
                        GroundKind::Supply | GroundKind::Train => None,
                    })
                };
                Some(GroundJob {
                    kind: gkind,
                    range_m,
                    route,
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
                    warnings.extend(numbered_ground_issues(
                        &layout.spots,
                        objectives.is_empty(),
                    ));
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
        notes.push("Left-drag to move, right-drag to set heading. Click a WP, then click a road or rail to place it.".into());
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
            let objectives: Vec<Option<(f64, f64)>> = {
                let mut gi = 0usize;
                slot.copies
                    .iter()
                    .map(|c| {
                        if c.kind == ArmyUnitKind::Ship {
                            None
                        } else {
                            let obj = ground_spots.get(gi).and_then(|s| s.objective);
                            gi += 1;
                            obj
                        }
                    })
                    .collect()
            };
            snap_army_attack_areas(&mut root, &objectives);
            let country = country_for_coalition(slot.eastern, self.country);
            apply_overrides(&mut root, "", country);
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
                return Err(format!("Select a Zone IN for {}.", slot.info.name));
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
                    return Err(format!("Select a Zone IN for {}.", slot.info.name));
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
        let attack_at: Vec<Option<(f64, f64)>> =
            layout.spots.iter().map(|s| s.objective).collect();
        snap_copy_attack_areas(&mut root, &attack_at);
        let country = country_for_coalition(layout.eastern, self.country);
        apply_overrides(&mut root, "", country);
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
                let side = if eastern { "Eastern" } else { "NATO" };
                self.status = Status::Info(format!(
                    "{side}: {n} groups in {packs} pack(s). Drag icons in Pan / Select AO to fine-tune."
                ));
                self.map_fighters = Some(layout);
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
        self.drawn_marks.push(DrawnMark::Salient);
		self.clear_redo_stack();
    }

	fn remove_last_mark(&mut self) {
        if self.attack_drag.take().is_some() {
            return;
        }
        if !self.current_salient.is_empty() {
            self.current_salient.clear();
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
            }
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
            }
        }
    }

    fn clear_redo_stack(&mut self) {
        self.redo_marks.clear();
        self.redo_salients.clear();
        self.redo_attack_arrows.clear();
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

	fn draw_map_legend(&self, ui: &mut egui::Ui) {
		ui.add_space(4.0);
		ui.label(RichText::new("Legend").strong());
		ui.horizontal_wrapped(|ui| {
			legend_swatch(ui, Color32::from_rgba_unmultiplied(110, 18, 18, 128), "Road");
			legend_swatch(ui, Color32::from_rgba_unmultiplied(16, 16, 18, 128), "Railroad");
			legend_swatch(ui, Color32::from_rgb(220, 30, 30), "Front Line");
			legend_swatch(ui, Color32::from_rgb(240, 220, 80), "Major Battle");
			legend_swatch(ui, Color32::from_rgb(155, 0, 0), "DPRK");
			legend_swatch(ui, Color32::from_rgb(0, 120, 150), "NATO");
			legend_swatch(ui, Color32::from_rgb(30, 90, 220), "Airfield");
			legend_swatch(ui, Color32::from_rgb(255, 140, 40), "Linked entity");
			legend_swatch(ui, Color32::from_rgb(255, 210, 70), "Block");
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

    fn recon_new_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(
				"Each template parks in its own square 10 km grid from 40000, 40000 (2×2 for 4 copies of that type, 3×3 for 9) so you can sort by hand if desired. 
A random logic may be used so that each mission spawns placed units at random.
If used mission begin is unlinked in each loaded group.
Influence is set to control how many copies of that type are created (from the share of Templates to create). 
Activate ratio then runs a waterfall inside each type: the last timer is 100%, and a win closes the remaining Outs so two cannot fire.
Winners run ENABLE / PULSE IN → Zone IN; losers are not spawned (or activated).",
            )
        );
        ui.label(
            RichText::new(
                "Icon and subtitle text stay as LC indexes in the group. If .eng (or other language) files sit next to a template they are copied beside the output; re-export from the editor if they are missing.",
            )
        );
        ui.label(
            RichText::new(format!(
                "Select the {SUGGESTED_ZONE_NAMES} checkzones that belong to each template so we know the group is valid."
            ))
        );
        ui.add_space(8.0);

        self.ensure_map_assets(ui.ctx());
        ui.label(RichText::new("Import as").strong());
        ui.label(
            RichText::new(
                "Default type for newly added templates. Each template keeps its own icon — click the icons on a template to change it. Map draws that same type: Ship on water, Train on railroad, Armor / Supply / Artillery on open ground."
            )
        );
        ui.horizontal(|ui| {
            self.unit_kind_picker(ui);
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Add templates…").clicked() {
                self.add_recon_template();
            }
            if ui.button("Add folder…").clicked() {
                self.add_recon_folder();
            }
        });
        ui.add_space(8.0);

        labeled_slider(ui, "Templates to create", &mut self.recon_total, 1..=64);
        ui.checkbox(
            &mut self.recon_strip_randomizer,
            "Spawn all copies (omit randomizer)",
        );
        if self.recon_strip_randomizer {
            ui.label(
                RichText::new(
                    "Every copy keeps its Mission Begin chain and starts. No waterfall, no deletions.",
                )
            );
        } else {
            labeled_slider(ui, "Activate ratio (%)", &mut self.recon_percent, 1..=100);
        }
        recon_dserver_note(ui);
        if !self.recon_strip_randomizer {
            self.recon_timing_sliders(ui);
        }
        ui.checkbox(
            &mut self.recon_keep_positions,
            "Keep loaded positions (do not park on the grid)",
        );
        ui.add_space(8.0);

        if self.recon_slots.is_empty() {
            ui.label(
                RichText::new("Add one or more .Group files, or a folder of them, then set influence.")
                    .italics()
            );
        }

        let kind_icons = self.type_icons_east();
        recon_slot_list(
            ui,
            &mut self.recon_slots,
            Some("Influence (share of copies created)"),
            Some(kind_icons),
        );

        if !self.recon_slots.is_empty() {
            let weights: Vec<u32> = self.recon_slots.iter().map(|s| s.influence).collect();
            let mix = allocate_mix(&weights, self.recon_total as usize, self.recon_percent);
            let placed: usize = mix.iter().map(|m| m.copies).sum();
            let live: usize = mix.iter().map(|m| m.activate).sum();
            ui.add_space(4.0);
			ui.label(RichText::new("Copy mix").strong());
            ui.label(
                RichText::new(
                    if self.recon_strip_randomizer {
                        "Influence splits how many copies of each type are placed. Spawn-all keeps every copy; no activate waterfall."
                    } else {
                        "Influence splits how many copies of each type are placed. Activate % is per type, not on the pack total, so two types at 50% of 10 copies is 5 placed / 3 live each (6 live), not 5 live overall."
                    },
                )
            );
            for (slot, m) in self.recon_slots.iter().zip(mix.iter()) {
                let live_n = if self.recon_strip_randomizer {
                    m.copies
                } else {
                    m.activate
                };
                ui.label(
                    RichText::new(format!(
                        "  {} placed, {} {}  ×  {} ({})",
                        m.copies,
                        live_n,
                        if self.recon_strip_randomizer {
                            "spawn"
                        } else {
                            "activate"
                        },
                        slot.info.name,
                        slot.kind.label()
                    ))
                );
            }
            let shown_live = if self.recon_strip_randomizer {
                placed
            } else {
                live
            };
            ui.label(
                RichText::new(format!(
                    "{shown_live} of {placed} copies will {}.",
                    if self.recon_strip_randomizer {
                        "spawn"
                    } else {
                        "activate"
                    }
                ))
                    .color(Color32::from_rgb(110, 150, 110)),
            );
        }

        ui.add_space(12.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let generate = egui::Button::new(RichText::new("Generate File").strong())
                .min_size(Vec2::new(200.0, 32.0));
            if ui.add(generate).clicked() {
                self.generate_recon_file();
            }
        });
    }

    fn recon_rework_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(
                "Add Ground Units packs that need to be modified once exported from this utility. 
	In this mode copies stay where you have placed them. 
	This may be used to add another unit pack in later, or to combine several packs into one group file.",
            )
        );
        ui.label(
            RichText::new(format!(
                "Select the {SUGGESTED_ZONE_NAMES} checkzones so we know each type is valid. Influence is the activate ratio for that type: how many of the copies already on the map will win that type's waterfall."
            ))
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Add pack…").clicked() {
                self.add_placed_packs();
            }
        });
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.recon_strip_randomizer,
            "Remove random logic (keep every copy, no waterfall)",
        );
        if self.recon_strip_randomizer {
            ui.label(
                RichText::new(
                    "Strips Recon Randomizer and reconnects each copy's Mission Begin so every group starts. Pick the MCU Mission Begin should fire: a timer, or a checkzone. Recommended: a timer that targets a Closer checkzone (Zone IN). ENABLE / PULSE IN is usually that timer. Pick the checkzone itself only if you want Mission Begin to pulse that zone directly.",
                )
            );
        }
        recon_dserver_note(ui);
        ui.add_space(8.0);

        let detected_total: usize = self.recon_rework.iter().filter_map(|s| s.detected).sum();
        ui.label(
            RichText::new(format!("{detected_total} groups detected on the map")).strong(),
        );
        if !self.recon_strip_randomizer {
            ui.label("Activate ratio (%)");
            ui.horizontal(|ui| {
                let changed = ui
                    .add(
                        egui::Slider::new(&mut self.recon_percent, 1..=100)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .changed();
                let typed = ui
                    .add(egui::DragValue::new(&mut self.recon_percent).range(1..=100).speed(0.2))
                    .changed();
                if changed || typed {
                    for slot in &mut self.recon_rework {
                        slot.influence = self.recon_percent;
                    }
                }
            });
            self.recon_timing_sliders(ui);
        }
        ui.add_space(8.0);

        if self.recon_rework.is_empty() {
            ui.label(
                RichText::new("Add one or more Random Ground Units .Group files exported from the editor.")
                    .italics()
            );
        }

        recon_slot_list(
            ui,
            &mut self.recon_rework,
            if self.recon_strip_randomizer {
                None
            } else {
                Some("Influence (activate %)")
            },
            None,
        );

        if self.recon_strip_randomizer && !self.recon_rework.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Reconnect Mission Begin").strong());
            for (i, slot) in self.recon_rework.iter_mut().enumerate() {
                if slot.restore_start.is_empty() {
                    if let Some(c) = slot.info.suggested_restore() {
                        slot.restore_start = c.name.clone();
                    }
                }
                let selected = slot.restore_start.clone();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&slot.info.name).strong());
                    egui::ComboBox::from_id_salt(format!("restore_start_{i}"))
                        .selected_text(if selected.is_empty() {
                            "Select a timer or checkzone".to_string()
                        } else {
                            selected.clone()
                        })
                        .width(360.0)
                        .show_ui(ui, |ui| {
                            for choice in &slot.info.restore_starts {
                                let kind = match choice.kind {
                                    RestoreKind::Timer => "Timer",
                                    RestoreKind::CheckZone => "CheckZone",
                                };
                                let rec = if choice.recommended { "  (recommended)" } else { "" };
                                let label = format!("{}  [{kind}]{rec}", choice.name);
                                if ui
                                    .selectable_label(slot.restore_start == choice.name, label)
                                    .clicked()
                                {
                                    slot.restore_start = choice.name.clone();
                                }
                            }
                        });
                });
                if let Some(c) = slot
                    .info
                    .restore_starts
                    .iter()
                    .find(|c| c.name == slot.restore_start)
                {
                    ui.label(
                        RichText::new(&c.hint).color(Color32::from_rgb(180, 150, 70)),
                    );
                }
            }
            ui.label(
                RichText::new(format!(
                    "All {detected_total} copies will start (no randomizer)."
                ))
                .color(Color32::from_rgb(110, 150, 110)),
            );
        } else if !self.recon_rework.is_empty() {
            ui.add_space(4.0);
			ui.label(RichText::new("Copy mix").strong());
            ui.label(
                RichText::new(
                    "Copies stay where they are. Influence is that type's activate %. Each type has its own waterfall.",
                )
            );
            let mut placed = 0usize;
            let mut live = 0usize;
            for slot in &self.recon_rework {
                let n = slot.detected.unwrap_or(0);
                let mix = TypeMix::from_copies(n, slot.influence.clamp(1, 100));
                placed += mix.copies;
                live += mix.activate;
                ui.label(
                    RichText::new(format!(
                        "  {} on map, {} activate  ×  {}",
                        mix.copies, mix.activate, slot.info.name
                    ))
                );
            }
            if placed > 0 {
                ui.label(
                    RichText::new(format!("{live} of {placed} copies will activate."))
                        .color(Color32::from_rgb(110, 150, 110)),
                );
            }
        }

        ui.add_space(12.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let generate = egui::Button::new(RichText::new("Generate File").strong())
                .min_size(Vec2::new(200.0, 32.0));
            if ui.add(generate).clicked() {
                self.generate_rework_file();
            }
        });
    }

    fn recon_timing_sliders(&mut self, ui: &mut egui::Ui) {
        labeled_slider(ui, "Start delay (s)", &mut self.recon_start_delay_s, 0..=180);
        ui.label(
            RichText::new(
                "Wait this long after Mission Begin before the first type starts. Use this when several Army Generator groups are in the same mission so they do not all fire at t=0.",
            )
        );
        labeled_slider(
            ui,
            "Delay between groups (ms)",
            &mut self.recon_group_delay_ms,
            0..=5000,
        );
        ui.label(
            RichText::new(
                "After the start delay, each following type waits this long (default 500 ms) so MCU load does not spike.",
            )
        );
    }

    fn pack_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Linked groups").strong());
        ui.label(
            RichText::new("Copies of the configured group are chained through 'NodeGates' and are parked in a square 10 km grid from 40000, 40000.")
        );
        ui.add_space(4.0);
        labeled_slider(ui, "Number of linked groups", &mut self.linked_groups, 1..=10);
    }

    fn flight_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Random aircraft flights").strong());
        ui.label(
            RichText::new(
                "Each flight is one randomizer slot. Pairs: AttackArea + Cover. A leftover aircraft gets AttackArea only.",
            )
        );
        ui.add_space(4.0);
        labeled_slider(ui, "Number of flights", &mut self.flight_count, 1..=10);
        labeled_slider(ui, "Max number in each flight", &mut self.max_in_flight, 1..=8);
        let sizes = flight_sizes(self.flight_count as usize, self.max_in_flight as usize);
        let total: usize = sizes.iter().sum();
        let mix = sizes
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("/");
        ui.label(
            RichText::new(format!(
                "{total} aircraft in Group 1  ·  flight sizes {mix}  ·  numbers {}–{}",
                crate::aircraft::flight_number(0, 0),
                crate::aircraft::flight_number(
                    (self.flight_count as usize).saturating_sub(1),
                    sizes.last().copied().unwrap_or(1).saturating_sub(1)
                )
            ))
            .color(Color32::from_rgb(110, 150, 110)),
        );
    }

    fn types_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Aircraft types").strong());
        ui.label(
            RichText::new("Flights cycle through the selected types. Skill is loosely applied; lead ≥ wingman.")
        );
        ui.add_space(4.0);
        for (i, ac) in AIRCRAFT_TYPES.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.type_enabled[i], ac.label);
                ui.add_enabled(
                    self.type_enabled[i],
                    egui::Slider::new(&mut self.type_skill[i], 0..=4)
                        .integer()
                        .text("skill")
                        .trailing_fill(true),
                );
            });
        }
    }

    fn country_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Country").strong());
        ui.label(
            RichText::new(
                "500-series: Zone IN/OUT trigger on western coalitions [2].  600-series: trigger on [1].",
            )
        );
        ui.add_space(4.0);
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
    }

    fn timers_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Timers (seconds)").strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Cooldown");
            ui.add(
                egui::DragValue::new(&mut self.cooldown)
                    .range(0.0..=1800.0)
                    .speed(1.0)
                    .suffix(" s"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Reinforcement");
            ui.add(
                egui::DragValue::new(&mut self.reinforcement)
                    .range(0.0..=1800.0)
                    .speed(1.0)
                    .suffix(" s"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Delete orders");
            ui.add(
                egui::DragValue::new(&mut self.delete_orders)
                    .range(0.0..=600.0)
                    .speed(1.0)
                    .suffix(" s"),
            );
        });
    }

    fn altitude_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Altitude range (m)").strong());
        ui.label(
            RichText::new("1- and 2-ships spread between min and max. A second pair is high cover (~2000 m up); low cover sits in a 500–1500 m band that rises with max. Wingmen stack 25–50 m on their lead.")
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Min");
            ui.add(
                egui::DragValue::new(&mut self.altitude_min)
                    .range(100.0..=9000.0)
                    .speed(25.0)
                    .suffix(" m"),
            );
            ui.label("Max");
            ui.add(
                egui::DragValue::new(&mut self.altitude_max)
                    .range(100.0..=9000.0)
                    .speed(25.0)
                    .suffix(" m"),
            );
        });
        if self.altitude_min > self.altitude_max {
            std::mem::swap(&mut self.altitude_min, &mut self.altitude_max);
        }
    }

    fn optional_template_section(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Custom pack template (experimental)", |ui| {
            ui.label(
                RichText::new("Leave unloaded to use the built-in logic.")
            );
            ui.horizontal(|ui| {
                if ui.button("Load…").clicked() {
                    self.pick_template();
                }
                if self.custom_path.is_some() && ui.button("Use built-in").clicked() {
                    self.custom_path = None;
                    self.status = Status::Info("Using built-in logic.".into());
                }
                let label = self
                    .custom_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("Built-in");
                ui.label(RichText::new(label).italics());
            });
        });
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        match &self.status {
            Status::Idle => {
                let msg = match self.mode {
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
                        "Load a Freeflight airfield from _gen.mission, then export the cleaned group."
                    }
                    AppMode::Map => {
                        "Draw a box on the Korea map, then generate icons for that area."
                    },
                };
				ui.label(RichText::new(msg));
            }
            Status::Info(msg) => {
                ui.label(
                    RichText::new(msg)
                        .color(Color32::from_rgb(110, 150, 110)),
                );
            }
            Status::Warn { lead, items } => {
                ui.vertical(|ui| {
                    if !lead.is_empty() {
                        ui.label(RichText::new(lead).color(STATUS_WARN));
                    }
                    for item in items {
                        ui.label(RichText::new(format!("• {item}")).color(STATUS_WARN));
                    }
                });
            }
            Status::Error(msg) => {
                ui.label(
                    RichText::new(msg)
                        .color(Color32::from_rgb(200, 90, 90)),
                );
            }
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
        let picked = rfd::FileDialog::new()
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
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        let before = self.bomber_slots.len();
        let mut last_err = None;
        let mut loaded_pack = false;
        for path in paths {
            match self.add_bomber_from_path(path) {
                Ok(from_pack) => {
                    if from_pack {
                        loaded_pack = true;
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

        let generated = match link_bomber_plans_with(&inputs, self.bomber_keep_positions) {
            Ok(g) => g,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let text = serialize_group(&generated);
        let suggested = format!(
            "Exclusive_Activation_{}plan.Group",
            self.bomber_slots.len()
        );
        let Some(save_path) = rfd::FileDialog::new()
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
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("IL-2 Group", &["Group", "group"])
            .pick_files()
        else {
            return;
        };
        self.add_recon_paths(paths);
    }

    fn add_recon_folder(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else {
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
                    "Select a Zone IN for {}.",
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

        let generated = match generate_recon_ex(
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
        let Some(save_path) = rfd::FileDialog::new()
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
        let Some(paths) = rfd::FileDialog::new()
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
                self.status = Status::Error(format!("Select a Zone IN for {}.", slot.info.name));
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
            let text = serialize_group(&combined);
            let suggested = format!("Ground_Units_{n}_always_on.Group");
            let Some(save_path) = rfd::FileDialog::new()
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
        let text = serialize_group(&combined);
        let suggested = format!("Random_Ground_Units_{live}of{n}.Group");
        let Some(save_path) = rfd::FileDialog::new()
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
    }

    fn load_airfield(&mut self) {
        let Some(path) = rfd::FileDialog::new()
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
        let Some(save_path) = rfd::FileDialog::new()
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
        let pack = match generate_front(&opts) {
            Ok(p) => p,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let text = serialize_group(&pack.root);
        let suggested = format!("Korea_BaseMap_{}.Group", self.current_mark().date_label());
        let Some(save_path) = rfd::FileDialog::new()
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
    }

    fn add_map_refs(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
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
        let text = serialize_group(&generated);

        let suggested = format!(
            "Eastern_Fighters_Random_{}pack.Group",
            self.linked_groups
        );
        let Some(save_path) = rfd::FileDialog::new()
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
                Color32::from_rgb(255, 210, 70) // Remains Yellow/Gold
            } else {
                Color32::from_rgba_unmultiplied(255, 210, 70, 90)
            };
            (color, if in_box { 2.4 } else { 1.7 })
        }
    }
}

fn move_row_button(ui: &mut egui::Ui, up: bool) -> egui::Response {
    let size = Vec2::splat(18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
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

fn skip_only_warnings(warnings: &[String]) -> Vec<String> {
    warnings
        .iter()
        .filter(|w| w.contains("skipped"))
        .cloned()
        .collect()
}

fn legend_swatch(ui: &mut egui::Ui, color: Color32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(rect, 1.0, color);
	ui.label(RichText::new(label));
    ui.add_space(8.0);
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

fn unit_kind_icon_button(
    ui: &mut egui::Ui,
    tex: Option<&TextureHandle>,
    fallback: &str,
    hover: &str,
    selected: bool,
    size: f32,
) -> bool {
    if let Some(tex) = tex {
        ui.add(egui::ImageButton::new((tex.id(), Vec2::splat(size))).selected(selected))
            .on_hover_text(hover)
            .clicked()
    } else {
        ui.selectable_label(selected, fallback)
            .on_hover_text(hover)
            .clicked()
    }
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
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .expect("fighter SVG in assets/ is not valid");
    let size = tree.size().to_int_size();
    let target = 128u32;
    let src_w = size.width().max(1);
    let src_h = size.height().max(1);
    let scale = target as f32 / src_w.max(src_h) as f32;
    let w = ((src_w as f32) * scale).round().max(1.0) as u32;
    let h = ((src_h as f32) * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).expect("fighter SVG pixmap");
    let transform = resvg::tiny_skia::Transform::from_scale(
        w as f32 / tree.size().width(),
        h as f32 / tree.size().height(),
    );
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
    let outside = Stroke::new(1.15_f32, Color32::from_rgb(220, 30, 30));
    let inside = Stroke::new(2.5_f32, Color32::from_rgb(220, 30, 30));
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

fn faction_map_color(eastern: bool) -> Color32 {
    if eastern {
        Color32::from_rgb(155, 0, 0)
    } else {
        Color32::from_rgb(0, 120, 150)
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
    let font = FontId::new(10.0, FontFamily::Proportional);
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

fn apply_readable_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let prop = FontFamily::Proportional;
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(10.0, prop.clone()),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(13.0, prop.clone()),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, prop.clone()),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(18.0, prop),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
    ctx.set_style(style);
}

fn army_mix_label(copies: &[ArmyCopyInfo]) -> String {
    let mut counts = [0usize; 6];
    for c in copies {
        let i = match c.kind {
            ArmyUnitKind::Ship => 0,
            ArmyUnitKind::Armor => 1,
            ArmyUnitKind::Supply => 2,
            ArmyUnitKind::Artillery => 3,
            ArmyUnitKind::Train => 4,
            ArmyUnitKind::MobileArtillery => 5,
        };
        counts[i] += 1;
    }
    let mut parts = Vec::new();
    for (kind, n) in [
        (ArmyUnitKind::Ship, counts[0]),
        (ArmyUnitKind::Armor, counts[1]),
        (ArmyUnitKind::Supply, counts[2]),
        (ArmyUnitKind::Artillery, counts[3]),
        (ArmyUnitKind::Train, counts[4]),
        (ArmyUnitKind::MobileArtillery, counts[5]),
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
    parse_group_file(&text).map_err(|err| format!("Parse failed: {err}"))
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

fn recon_slot_list(
    ui: &mut egui::Ui,
    slots: &mut Vec<ReconSlot>,
    influence_label: Option<&str>,
    kind_icons: Option<[Option<TextureHandle>; 5]>,
) {
    let mut remove = None;
    for i in 0..slots.len() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).strong());
                ui.vertical(|ui| {
                    ui.label(RichText::new(&slots[i].info.name).strong());
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
                                    22.0,
                                ) {
                                    slots[i].kind = k;
                                }
                            }
                        });
                    }
                    let file = slots[i]
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string();
                    let file = if slots[i].sources.len() > 1 {
                        format!("{} + {} more", file, slots[i].sources.len() - 1)
                    } else {
                        file
                    };
					ui.label(RichText::new(file));
                    ui.label(
                        RichText::new(format!(
                            "{} vehicles/ships/trains, {} blocks",
                            slots[i].info.vehicle_count, slots[i].info.block_count
                        ))
                    );
                    if let Some(n) = slots[i].detected {
                        ui.label(
                            RichText::new(format!("{n} detected on the map"))
                                .color(Color32::from_rgb(70, 140, 200)),
                        );
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            });

            show_missing_locale_hint(ui, &slots[i].path);

            if let Some(label) = influence_label {
                ui.add_space(4.0);
                ui.label(label);
                ui.add(egui::Slider::new(&mut slots[i].influence, 0..=100).trailing_fill(true));
            }

            let zones = slots[i].info.checkzones.clone();
            let suggested = slots[i].info.suggested_triggers.clone();
            if !zones.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Zone IN checkzones on this group (used to confirm the group is valid).",
                    )
                    .color(Color32::from_rgb(180, 150, 70)),
                );
            }
            for zone in &zones {
                let mut on = slots[i].selected_triggers.contains(&zone.index);
                let label = if suggested.contains(&zone.index) {
                    format!("{}  (suggested)", zone.name)
                } else {
                    zone.name.clone()
                };
                if ui.checkbox(&mut on, label).changed() {
                    if on {
                        if !slots[i].selected_triggers.contains(&zone.index) {
                            slots[i].selected_triggers.push(zone.index);
                        }
                    } else {
                        slots[i].selected_triggers.retain(|id| *id != zone.index);
                    }
                }
            }
            if slots[i].selected_triggers.is_empty() {
                ui.label(
                    RichText::new("Select at least one Zone IN.")
                        .color(Color32::from_rgb(200, 90, 90)),
                );
            }
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        slots.remove(i);
    }
}

fn recon_dserver_note(ui: &mut egui::Ui) {
    ui.label(
        RichText::new(
            "DServer struggles when too many random units fire at once. This pack is capped at 64 copies. Stay under 30 random units in the whole mission (this pack plus any others) unless you use Spawn all / omit randomizer.",
        )
        .color(Color32::from_rgb(180, 150, 70)),
    );
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
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(value, range.clone())
                .show_value(false)
                .trailing_fill(true),
        );
        ui.add(egui::DragValue::new(value).range(range).speed(0.2));
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
        .color(Color32::from_rgb(180, 150, 70)),
    );
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