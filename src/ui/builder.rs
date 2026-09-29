//! # builder.rs — Template Builder tab
//!
//! One mode of [`super::GroupGeneratorApp`]. This file owns the Template
//! page: model palette, formation view, order of battle, and the inspector.
//!
//! `ui.rs` is the anchor. It owns the app struct (the `tpl_*` fields), the
//! mode rail, and the shortcuts, and it calls the hooks listed below. The
//! other modes stay in `ui.rs` until they move into their own files in
//! `src/ui/`, the same way.
//!
//! Hooks `ui.rs` calls (keep these `pub(super)`):
//! * `template_page` — the tab
//! * `generate_unit_template` — Generate File / Ctrl G
//! * `load_template_now` — Load… / Ctrl O
//! * `reset_template_confirmed` — Reset
//! * `tpl_fingerprint` — unsaved-edit check
//! * `tpl_restore` — Ctrl Z
//! * `sync_template_waypoint_speed` — after an undo
//! * `default_template_seats` — app startup
//!
//! Presentation only. Group text is built by [`crate::template`].

use std::path::PathBuf;

use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke,
    TextureHandle, Vec2,
};

use crate::aircraft::COUNTRIES;
use crate::help::HelpTopic;
use crate::model_spec::{self, ModelClass};
use crate::parser::{parse_group_file, parse_il2_document};
use crate::payloads;
use crate::serialize::serialize_group;
use crate::shell;
use crate::template::{
    append_seat, apply_formation_numbers, apply_plane_start, apply_suggested_attack_area,
    attack_area_range_limit, bundled_catalog, carriage_label, catalog_carriage_scripts,
    copy_seat_attributes, event_triggers_order, flight_lead_of, formation_label, formations_for,
    generate_template, has_linked_wingmen, insert_goto_waypoint_after, is_follower, lead_indexes,
    load_catalog, load_catalog_as_user_added, load_template, merge_catalog, move_seat,
    near_visual_range, next_waypoint_number, normalize_order_chain, order_chip_detail,
    order_seat_indexes, order_tree_columns, order_tree_layout, path_waypoint_display_m,
    place_offset, priority_label, shift_tree_order, tree_order_can_shift,
    receives_orders, refresh_attack_areas_for_seat, remap_event_then, remap_index_vec,
    remap_seat_index, replace_seat_unit, set_report_following, used_waypoint_count,
    visual_range_m, waypoint_area_m, waypoint_display_altitude, waypoint_display_priority,
    zone_defaults, zone_mix_for_seats, AIR_START_ALTITUDE_M, AIR_ZONE_IN_M, AIR_ZONE_OUT_M,
    AttackAreaTarget, BringUp, CatalogUnit, DEFAULT_TIME_ON_TARGET_S,
    DEFAULT_TIMER_S, EntityEvent, EventHook, EventThen, FlightRole, OrderKind, OrderSpec,
    OrderTreeNode, PLACEMENT_SPACING, PlaceLayout, PlaneStart, TemplateOptions, TemplateSeat,
    UnitKind as CatalogKind, WAYPOINT_SPACING_M, ZoneCoalition, ZoneMix,
};
use crate::theme::{self, c};

use super::{
    country_short, dialog, load_model_png, paint_seat_marker, save_with_sidecars, seat_side,
    section_heading, AppMode, Status,
};

/// What Template Reset / Remove / Load can take away; restored by Ctrl Z.
pub(super) struct TemplateSnapshot {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TplSelect {
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

/// Commands, specials and reports for one unit. Shared by the tree header
/// and the unit's right-click menu.
fn order_menu_contents(ui: &mut egui::Ui, si: usize, unit_kind: CatalogKind, add: &mut Option<AddTreeItem>) {
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
}

/// "+ Order ▾" in the order tree header: commands, specials and reports for
/// the selected unit.
fn tree_order_menu(ui: &mut egui::Ui, si: usize, unit_kind: CatalogKind, add: &mut Option<AddTreeItem>) {
    ui.menu_button("+ Order ▾", |ui| order_menu_contents(ui, si, unit_kind, add))
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
pub(super) fn skill_name(skill: i32) -> &'static str {
    match skill.clamp(0, 4) {
        0 => "Plain",
        1 => "Low",
        2 => "Normal",
        3 => "Veteran",
        _ => "Ace",
    }
}

/// Label column of the right-panel field grids (README §5.1).
pub(super) const FIELD_LABEL_W: f32 = 96.0;

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

pub(super) fn default_template_seats() -> Vec<TemplateSeat> {
    Vec::new()
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
    draggable: bool,
) -> egui::Response {
    let sense = if draggable { Sense::click_and_drag() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
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
    let (rect, response) = ui.allocate_exact_size(Vec2::new(TREE_UNIT_W, TREE_CHIP_H), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, format!("Unit card {n}"))
    });
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
    response
}

/// What the model is, for someone who does not already know the name.
pub(super) fn plain_class_line(class: ModelClass) -> &'static str {
    match class {
        ModelClass::Fighter => "A fighter. It hunts other aircraft.",
        ModelClass::FighterBomber => "A fighter that can also bomb targets on the ground.",
        ModelClass::Attack => "A ground-attack aircraft. It strikes targets on the ground.",
        ModelClass::Bomber => "A bomber. It carries bombs to a target.",
        ModelClass::Transport => "A transport. It carries people or cargo.",
        ModelClass::Armor => "An armored vehicle, such as a tank.",
        ModelClass::TankDestroyer => "A tank destroyer. It is built to knock out armored vehicles.",
        ModelClass::Spg => "A self-propelled gun. It fires artillery from a vehicle.",
        ModelClass::LightAaa => "A light anti-aircraft vehicle.",
        ModelClass::Truck => "A truck. It carries supplies or tows equipment.",
        ModelClass::Jeep => "A light car or jeep.",
        ModelClass::Tractor => "A tractor. It tows guns or equipment.",
        ModelClass::Infantry => "Infantry. Soldiers on foot.",
        ModelClass::RocketArtillery => "A rocket launcher. It fires a salvo of rockets.",
        ModelClass::LightFlak => "A light anti-aircraft gun.",
        ModelClass::HeavyFlak => "A heavy anti-aircraft gun.",
        ModelClass::Artillery => "An artillery piece. It shells targets from long range.",
        ModelClass::Mortar => "A mortar. It lobs shells in a high arc.",
        ModelClass::MachineGun => "A machine gun.",
        ModelClass::Radar => "A radar. It detects aircraft.",
        ModelClass::Airfield => "An airfield object.",
        ModelClass::Dummy => "A decoy. It looks like a real unit.",
        ModelClass::CargoShip => "A cargo ship.",
        ModelClass::Destroyer => "A destroyer. A fast warship.",
        ModelClass::TorpedoBoat => "A torpedo boat.",
        ModelClass::LandingCraft => "A landing craft. It carries troops to a shore.",
        ModelClass::Gunboat => "A gunboat.",
        ModelClass::SmallBoat => "A small boat.",
        ModelClass::Locomotive => "A locomotive. It pulls a train.",
        ModelClass::Unknown => "A unit from the catalog.",
    }
}

fn seat_role_tag(seats: &[TemplateSeat], si: usize) -> String {
    match seats[si].role {
        FlightRole::Lead => {
            let size = 1 + (0..seats.len()).filter(|&j| seats[j].role == FlightRole::Follows(si)).count();
            format!("Lead ×{size}")
        }
        FlightRole::Follows(lead) if is_follower(seats, si) => format!("Follows {}", lead + 1),
        _ => String::new(),
    }
}

fn seat_meta_line(seats: &[TemplateSeat], si: usize) -> String {
    let seat = &seats[si];
    let country_name = country_short(seat.country);
    let country_name = country_name.split_once(' ').map_or(country_name.as_str(), |(_, n)| n.trim());
    let tail = if receives_orders(seats, si) {
        match seat.orders.len() {
            1 => "1 order".to_string(),
            k => format!("{k} orders"),
        }
    } else {
        "lead's orders".to_string()
    };
    format!("{country_name} · {} · {tail}", skill_name(seat.skill))
}

const TREE_ARROW_SLOT: f32 = 28.0;
/// Two text lines at 13 + 12 px; README §6.6 asks for at least 28.
const TREE_CHIP_H: f32 = 36.0;
const TREE_CHIP_W: f32 = 140.0;
const TREE_UNIT_W: f32 = 98.0;
const TREE_ROW_GAP: f32 = 6.0;
const TREE_UNIT_GAP: f32 = 10.0;
/// Same inset as the formation title and the "Scroll to zoom" hint.
const FORMATION_MARGIN: f32 = 14.0;
const FORMATION_CARD_W: f32 = 280.0;

/// Selected-unit card on the formation view. Left edge matches the title.
fn formation_card_rect(canvas: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(canvas.left() + FORMATION_MARGIN, canvas.top() + 52.0),
        Pos2::new(
            canvas.left() + FORMATION_MARGIN + FORMATION_CARD_W,
            canvas.bottom() - 32.0,
        ),
    )
}

/// The part of the formation view to the right of the unit-options card.
/// With no card, the formation uses the whole window.
fn formation_open_rect(canvas: Rect, show_card: bool) -> Rect {
    if !show_card {
        return canvas;
    }
    let card = formation_card_rect(canvas);
    Rect::from_min_max(Pos2::new(card.right() + 12.0, canvas.top()), canvas.max)
}

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
    kind: OrderKind,
    extra: usize,
    detail: &str,
    selected: bool,
    selected_fill: Color32,
    order_fill: Color32,
    report_fill: Color32,
    chain_fill: Color32,
    orders: &[OrderSpec],
    clicked: &mut Option<TplSelect>,
    remove_order: &mut Option<(usize, usize)>,
    move_order: &mut Option<(usize, usize, i32)>,
    drag_order: &mut Option<(usize, usize)>,
) -> Rect {
    let mut chip_rect = Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if tree_arrow_slot(ui, selected, true, tree_order_can_shift(orders, oi, -1)) {
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
        let resp = draw_tree_chip(ui, &text, detail, fill, selected, false, order_chip_size(kind), true);
        let drag_hint = "Drag left or right to move this order.";
        if hover.is_empty() {
            resp.clone().on_hover_text(drag_hint);
        } else {
            resp.clone().on_hover_text(format!("{hover}\n{drag_hint}"));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if resp.drag_started() {
            *drag_order = Some((si, oi));
            *clicked = Some(TplSelect::Order { seat: si, order: oi });
        }
        if resp.clicked() {
            *clicked = Some(TplSelect::Order { seat: si, order: oi });
        }
        chip_rect = resp.rect;
        if tree_arrow_slot(ui, selected, false, tree_order_can_shift(orders, oi, 1)) {
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
            false,
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
    drag_order: &mut Option<(usize, usize)>,
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
                kind,
                extra,
                &detail,
                selected,
                selected_fill,
                order_fill,
                report_fill,
                chain_fill,
                orders,
                clicked,
                remove_order,
                move_order,
                drag_order,
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
    drag_order: &mut Option<(usize, usize)>,
    column_bands: &mut Vec<(usize, usize, Rect)>,
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
        let mut order_ci = 0usize;
        for col in columns {
            let mut col_geom = Vec::new();
            let has_order = col.iter().any(|n| matches!(n, OrderTreeNode::Order(_)));
            let band_ci = has_order.then_some(order_ci);
            if has_order {
                order_ci += 1;
            }
            let col_rect = ui.vertical(|ui| {
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
                            drag_order,
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
                            drag_order,
                        ),
                    ));
                }
            })
            .response
            .rect;
            if let Some(ci) = band_ci {
                column_bands.push((si, ci, col_rect));
            }
            geom.push(col_geom);
        }
    });
    paint_order_tree_lines(ui, line_idx, &geom, orders, events);
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

impl super::GroupGeneratorApp {
    /// Template tab: the model palette on the left, the formation view in the
    /// center, and a contextual inspector on the right. The order of battle
    /// (units, then their orders and events) is a resizable band underneath.
    /// Each panel scrolls on its own.
    pub(super) fn template_page(&mut self, ctx: &egui::Context) {
        self.sync_template_zone_defaults();
        // Allocated before the side columns so the tree runs under both of them.
        egui::TopBottomPanel::bottom("tpl_tree")
            .exact_height(360.0)
            .resizable(false)
            .show(ctx, |ui| self.template_order_tree(ui));
        egui::SidePanel::left("tpl_left")
            .exact_width(270.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                self.template_add_units_section(ui);
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
                        self.template_inspector(ui);
                    });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0))
            .show(ctx, |ui| self.draw_template_schematic(ui));
    }

    /// The seat the current selection belongs to (unit, order or event).
    pub(super) fn selected_tpl_seat(&self) -> Option<usize> {
        match self.tpl_select {
            Some(
                TplSelect::Seat(s) | TplSelect::Order { seat: s, .. } | TplSelect::Event { seat: s, .. },
            ) if s < self.tpl_seats.len() => Some(s),
            _ => None,
        }
    }

    pub(super) fn template_catalog_label(&self) -> String {
        self.tpl_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .map_or_else(|| "Built-in catalog".to_string(), str::to_owned)
    }

    pub(super) fn use_builtin_catalog(&mut self) {
        self.tpl_path = None;
        self.tpl_catalog = bundled_catalog();
        self.tpl_class = None;
        self.tpl_country = None;
        self.tpl_add_pick = 0;
        self.tpl_preview_from_catalog = false;
    }

    pub(super) fn template_catalog_section(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn template_add_units_section(&mut self, ui: &mut egui::Ui) {
        // Fixed height so selecting a unit does not push the kind buttons down.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 176.0), Sense::hover());
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("tpl_left_preview")
                .auto_shrink([false, false])
                .show(ui, |ui| self.template_unit_preview(ui));
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);
        let catalog = self.template_catalog_label();
        ui.horizontal(|ui| {
            ui.label(section_heading("Models"));
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
    pub(super) fn template_add_model(&mut self, unit: CatalogUnit) {
        append_seat(&mut self.tpl_seats, unit, self.tpl_per_group);
        self.tpl_select = Some(TplSelect::Seat(self.tpl_seats.len() - 1));
        self.tpl_preview_from_catalog = false;
        self.sync_template_waypoint_speed();
    }

    /// Right panel. Place, Activate or Spawn, and Waypoints stay here while a
    /// unit is selected. The picture sits above Models; the unit's options are
    /// on the formation view.
    pub(super) fn template_inspector(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(
                "Place the group and choose Activate or Spawn here. Edit the selected unit on the formation view.",
            )
            .font(FontId::proportional(12.0))
            .color(c::NEUTRAL_700),
        );
        ui.add_space(6.0);
        self.template_placement_section(ui);
        self.template_bring_up_section(ui);
        self.template_waypoints_section(ui);
        self.template_catalog_section(ui);
    }

    /// Picture and a plain-language description of the selected unit, or of the
    /// highlighted catalog model before one is added.
    fn template_unit_preview(&mut self, ui: &mut egui::Ui) {
        let seat = self.selected_tpl_seat().filter(|_| !self.tpl_preview_from_catalog);
        let (unit, payload, mods, status) = if let Some(si) = seat {
            let unit = self.tpl_seats[si].unit.clone();
            let payload = payloads::payload_preview(&unit.script, self.tpl_seats[si].payload_id);
            let mods = payloads::mods_preview(&unit.script, &self.tpl_seats[si].mod_mask);
            let status = unit.is_air().then(|| {
                PlaneStart::from_i32(self.tpl_seats[si].start_type).preview_status(self.tpl_seats[si].altitude)
            });
            (Some(unit), Some(payload), Some(mods), status)
        } else {
            (self.displayed_catalog().get(self.tpl_add_pick).cloned(), None, None, None)
        };
        let name = unit.as_ref().map(|u| u.label().to_string());
        self.draw_model_preview(
            ui,
            unit.as_ref(),
            name.as_deref(),
            payload.as_deref(),
            mods.as_deref(),
            status.as_deref(),
        );
    }

    /// Moves a seat one step at a time so `move_seat` keeps roles and
    /// order targets consistent.
    pub(super) fn move_tpl_seat_to(&mut self, mut from: usize, to: usize) {
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

    pub(super) fn remove_tpl_seat(&mut self, si: usize) {
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

    pub(super) fn template_order_tree(&mut self, ui: &mut egui::Ui) {
        let seat = self.selected_tpl_seat();
        let unit_kind = seat.map_or(CatalogKind::Plane, |s| self.tpl_seats[s].unit.kind);
        let can_orders = seat.is_some_and(|s| receives_orders(&self.tpl_seats, s));
        let mut add = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(section_heading("Order of battle"));
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
                .scroll_source(egui::scroll_area::ScrollSource {
                    scroll_bar: true,
                    drag: false,
                    mouse_wheel: true,
                })
                .auto_shrink([false, false])
                .max_height(max_h)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| self.draw_template_seat_list(ui));
        });
        if shell::hint(
            ui,
            "UNIT → OnSpawned → orders. ‹ › or drag a chip left and right.",
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

    /// What is selected, in large type, then its fields. Drawn on the formation
    /// view under the unit's picture.
    pub(super) fn template_selection_block(&mut self, ui: &mut egui::Ui) {
        let kicker = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text.to_uppercase()).small().color(c::NEUTRAL_700));
        };
        let title = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text).font(FontId::new(22.0, theme::heading_family())));
        };
        let Some(si) = self.selected_tpl_seat() else {
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
            }
        }
        self.draw_template_details(ui);
    }

    pub(super) fn template_bring_up_section(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn template_placement_section(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn template_waypoints_section(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn draw_template_seat_list(&mut self, ui: &mut egui::Ui) {
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
        let mut drag_order = None;
        let mut column_bands: Vec<(usize, usize, Rect)> = Vec::new();
        let mut menu_add = None;
        let mut duplicate_seat = None;
        let mut remove_seat = None;
        let mut unit_rects = Vec::new();

        if self.tpl_seats.is_empty() {
            ui.label(
                RichText::new("No units yet. Pick a model on the left and click Add to Template.")
                    .small()
                    .color(c::NEUTRAL_700),
            );
        }
        if self.tpl_card_drag.is_some_and(|d| d >= self.tpl_seats.len()) {
            self.tpl_card_drag = None;
        }

        for si in 0..self.tpl_seats.len() {
            if si > 0 {
                ui.add_space(TREE_UNIT_GAP);
            }
            let role = seat_role_tag(&self.tpl_seats, si);
            let meta = seat_meta_line(&self.tpl_seats, si);
            let can_orders = receives_orders(&self.tpl_seats, si);
            let unit_kind = self.tpl_seats[si].unit.kind;
            let n_seats = self.tpl_seats.len();
            ui.horizontal_top(|ui| {
                let unit_sel = matches!(self.tpl_select, Some(TplSelect::Seat(s)) if s == si);
                let label = self.tpl_seats[si].unit.label().to_string();
                let country = self.tpl_seats[si].country;
                let script = self.tpl_seats[si].unit.script.clone();
                let response = ui
                    .vertical(|ui| {
                        ui.set_width(TREE_UNIT_W + 36.0);
                        let ctx = ui.ctx().clone();
                        let tex = self.model_texture(&ctx, &script);
                        let size = tex.size_vec2();
                        let scale = (32.0 / size.x).min(32.0 / size.y).min(1.0);
                        let response = ui
                            .horizontal(|ui| {
                                ui.add(egui::Image::new((tex.id(), size * scale)));
                                draw_tree_unit_chip(ui, si + 1, &label, country, unit_sel)
                            })
                            .inner;
                        if !role.is_empty() {
                            ui.label(RichText::new(&role).small().color(c::NEUTRAL_700));
                        }
                        ui.label(RichText::new(&meta).small().color(c::NEUTRAL_700));
                        response
                    })
                    .inner
                    .on_hover_text(format!(
                        "{label}. Drag up or down to reorder. Right-click for Duplicate, Delete, or Add Order. Change the model on the formation view."
                    ));
                if response.clicked() {
                    clicked = Some(TplSelect::Seat(si));
                }
                if response.drag_started() {
                    self.tpl_card_drag = Some(si);
                }
                if response.hovered() && self.tpl_card_drag.is_none() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                unit_rects.push(response.rect);
                response.context_menu(|ui| {
                    if ui.button("Duplicate").clicked() {
                        duplicate_seat = Some(si);
                        ui.close();
                    }
                    if ui.button("Delete").clicked() {
                        remove_seat = Some(si);
                        ui.close();
                    }
                    ui.add_enabled_ui(si > 0, |ui| {
                        if ui.button("Move up").clicked() {
                            move_seat_dir = Some((si, -1));
                            ui.close();
                        }
                    });
                    ui.add_enabled_ui(si + 1 < n_seats, |ui| {
                        if ui.button("Move down").clicked() {
                            move_seat_dir = Some((si, 1));
                            ui.close();
                        }
                    });
                    if can_orders {
                        ui.menu_button("Add Order", |ui| {
                            order_menu_contents(ui, si, unit_kind, &mut menu_add);
                        });
                    } else {
                        ui.add_enabled(false, egui::Button::new("Add Order"))
                            .on_disabled_hover_text("Wingmen take no orders. Add the order on the lead.");
                    }
                });
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
                        &mut drag_order,
                        &mut column_bands,
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

        let reordering = self.tpl_card_drag.is_some();
        if let Some(sel) = clicked {
            if !reordering {
                self.tpl_select = Some(sel);
                self.tpl_preview_from_catalog = false;
            }
        }
        if let Some((si, oi)) = remove_order {
            self.remove_tpl_order(si, oi);
        }
        if let Some((si, ei)) = remove_event {
            self.remove_tpl_event(si, ei);
        }
        if let Some((si, oi)) = drag_order {
            self.tpl_order_drag = Some((si, oi));
        }
        if let Some((si, oi)) = self.tpl_order_drag {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let pointer = ui.ctx().pointer_interact_pos();
            let target = pointer.and_then(|p| {
                column_bands
                    .iter()
                    .filter(|(seat, _, _)| *seat == si)
                    .find(|(_, _, r)| p.x >= r.left() && p.x <= r.right())
                    .map(|(_, ci, _)| *ci)
            });
            if let Some(ci) = target {
                if let Some((_, _, r)) = column_bands.iter().find(|(seat, band, _)| *seat == si && *band == ci) {
                    ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(2.0_f32, c::ACCENT));
                }
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.tpl_order_drag = None;
                if let Some(ci) = target {
                    self.move_tpl_order_to_column(si, oi, ci);
                }
            }
        }
        if let Some((si, oi, dir)) = move_order {
            self.shift_tpl_order(si, oi, dir);
        }
        if let Some((si, dir)) = move_seat_dir {
            if let Some(dest) = move_seat(&mut self.tpl_seats, si, dir) {
                swap_tpl_select(&mut self.tpl_select, si, dest);
            }
        }
        if let Some(item) = menu_add {
            self.apply_tree_add(item);
        }
        if let Some(si) = duplicate_seat {
            self.duplicate_tpl_seat(si);
        }
        if let Some(si) = remove_seat {
            if si < self.tpl_seats.len() {
                self.record_tpl_undo(format!("Removed {}", self.tpl_seats[si].unit.label()));
                self.remove_tpl_seat(si);
            }
        }
        let n = unit_rects.len();
        if let Some(src) = self.tpl_card_drag {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let insert = ui
                .ctx()
                .pointer_interact_pos()
                .or_else(|| ui.ctx().pointer_latest_pos())
                .map(|p| unit_rects.iter().position(|r| p.y < r.center().y).unwrap_or(n));
            if let Some(ins) = insert.filter(|_| n > 0) {
                let y = if ins < n {
                    unit_rects[ins].top() - 2.0
                } else {
                    unit_rects[n - 1].bottom() + 2.0
                };
                ui.painter().line_segment(
                    [Pos2::new(unit_rects[0].left(), y), Pos2::new(unit_rects[0].right(), y)],
                    Stroke::new(2.0_f32, c::ACCENT),
                );
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.tpl_card_drag = None;
                if let Some(ins) = insert {
                    let dest = if ins > src { ins - 1 } else { ins };
                    if dest != src {
                        self.record_tpl_undo("Reordered units".into());
                        self.move_tpl_seat_to(src, dest);
                    }
                }
            }
        }
    }

    fn replace_tpl_seat_model(&mut self, si: usize, unit: CatalogUnit) {
        if si >= self.tpl_seats.len() {
            return;
        }
        self.record_tpl_undo(format!("Changed {} to {}", self.tpl_seats[si].unit.label(), unit.label()));
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

    /// Insert a copy of the seat after it. A lead's copy is independent, so
    /// the original keeps its wingmen.
    fn duplicate_tpl_seat(&mut self, si: usize) {
        if si >= self.tpl_seats.len() {
            return;
        }
        self.record_tpl_undo(format!("Duplicated {}", self.tpl_seats[si].unit.label()));
        let mut clone = self.tpl_seats[si].clone();
        if clone.role == FlightRole::Lead {
            clone.role = FlightRole::Independent;
            clone.number_in_formation = 0;
            clone.formation_count = 0;
        }
        let dest = si + 1;
        self.tpl_seats.insert(dest, clone);
        let bump = |t: usize| if t >= dest { t + 1 } else { t };
        for seat in &mut self.tpl_seats {
            if let FlightRole::Follows(t) = seat.role {
                seat.role = FlightRole::Follows(bump(t));
            }
            for order in &mut seat.orders {
                if let Some(t) = order.cover_lead.as_mut() {
                    *t = bump(*t);
                }
                if let Some(t) = order.attack_seat.as_mut() {
                    *t = bump(*t);
                }
                for id in &mut order.shared_with {
                    *id = bump(*id);
                }
            }
        }
        self.tpl_select = Some(TplSelect::Seat(dest));
        self.tpl_preview_from_catalog = false;
    }

    pub(super) fn draw_train_carriages(&mut self, ui: &mut egui::Ui, si: usize) {
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
    pub(super) fn draw_template_details(&mut self, ui: &mut egui::Ui) {
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
                    RichText::new("Select a unit or an order in the order of battle.")
                        .color(c::NEUTRAL_700),
                );
            }
        }
    }

    pub(super) fn draw_template_seat_fields(&mut self, ui: &mut egui::Ui, si: usize) {
        let ctrl_w = field_control_w(ui);
        let mut moved = false;
        let mut payload_note = None;
        let mut picked_model = None;
        let kind = self.tpl_seats[si].unit.kind;
        let current_script = self.tpl_seats[si].unit.script.clone();
        let mut models: Vec<CatalogUnit> = self
            .tpl_catalog
            .iter()
            .filter(|u| u.kind == kind)
            .cloned()
            .collect();
        models.sort_by(|a, b| a.label().cmp(b.label()));
        ui.menu_button("Change Model", |ui| {
            ui.set_min_width(220.0);
            ui.set_max_height(280.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for unit in &models {
                    let selected = unit.script.eq_ignore_ascii_case(&current_script);
                    if ui.selectable_label(selected, unit.label()).clicked() {
                        picked_model = Some(unit.clone());
                        ui.close();
                    }
                }
            });
        })
        .response
        .on_hover_text("Replace this unit with another model of the same kind.");
        if ui
            .button("Copy attributes to all")
            .on_hover_text(
                "Copy this unit's country, skill, fuel, and flags to every unit, and its altitude to every plane (capped at each plane's ceiling). Payload and modifications copy only to the same aircraft type.",
            )
            .clicked()
        {
            self.record_tpl_undo(format!("Copied attributes from {}", self.tpl_seats[si].unit.label()));
            copy_seat_attributes(&mut self.tpl_seats, si);
        }
        if ui.button("Remove unit").on_hover_text("Ctrl Z undoes it").clicked() {
            self.record_tpl_undo(format!("Removed {}", self.tpl_seats[si].unit.label()));
            self.remove_tpl_seat(si);
            moved = true;
        }
        if let Some(unit) = picked_model {
            self.replace_tpl_seat_model(si, unit);
            return;
        }
        if moved {
            return;
        }
        ui.add_space(6.0);
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

    pub(super) fn draw_template_order_fields(&mut self, ui: &mut egui::Ui, seat: usize, order: usize) {
        let ctrl_w = field_control_w(ui);
        let unit_kind = if self.tpl_seats[seat].unit.is_air() { CatalogKind::Plane } else { CatalogKind::Vehicle };
        let mut kind_changed = false;
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
                            kind_changed = true;
                        }
                    }
                });
            ui.end_row();
        });
        // Snap a newly chosen report onto its command. Doing this every frame
        // pulled a report the user had moved back onto that command.
        if ui.button("Remove order").on_hover_text("Ctrl Z undoes it").clicked() {
            self.remove_tpl_order(seat, order);
            return;
        }
        ui.add_space(6.0);
        let order = if kind_changed {
            let order = {
                let s = &mut self.tpl_seats[seat];
                normalize_order_chain(&mut s.orders, &mut s.events, order)
            };
            self.tpl_select = Some(TplSelect::Order { seat, order });
            order
        } else {
            order
        };
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
    }

    pub(super) fn draw_template_event_fields(&mut self, ui: &mut egui::Ui, seat: usize, event: usize) {
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

    fn shift_tpl_order(&mut self, si: usize, oi: usize, dir: i32) {
        if si >= self.tpl_seats.len() || !tree_order_can_shift(&self.tpl_seats[si].orders, oi, dir) {
            return;
        }
        self.record_tpl_undo("Moved an order".into());
        let seat = &mut self.tpl_seats[si];
        if let Some(new_oi) = shift_tree_order(&mut seat.orders, &mut seat.events, oi, dir) {
            self.tpl_select = Some(TplSelect::Order { seat: si, order: new_oi });
        }
    }

    /// Drop a dragged order onto visual column `dest` of the same unit.
    fn move_tpl_order_to_column(&mut self, si: usize, oi: usize, dest: usize) {
        if si >= self.tpl_seats.len() || oi >= self.tpl_seats[si].orders.len() {
            return;
        }
        let now = order_tree_columns(&self.tpl_seats[si].orders).iter().position(|c| c.contains(&oi));
        if now == Some(dest) {
            return;
        }
        let Some(now) = now else {
            return;
        };
        let dir = if dest > now { 1 } else { -1 };
        if !tree_order_can_shift(&self.tpl_seats[si].orders, oi, dir) {
            return;
        }
        self.record_tpl_undo("Moved an order".into());
        let mut current = oi;
        for _ in 0..self.tpl_seats[si].orders.len() {
            let cols = order_tree_columns(&self.tpl_seats[si].orders);
            let at = cols.iter().position(|c| c.contains(&current));
            if at == Some(dest) {
                break;
            }
            let seat = &mut self.tpl_seats[si];
            let Some(next) = shift_tree_order(&mut seat.orders, &mut seat.events, current, dir) else {
                break;
            };
            current = next;
            let after = order_tree_columns(&self.tpl_seats[si].orders).iter().position(|c| c.contains(&current));
            if after == Some(dest) || after.is_none_or(|a| (dir > 0 && a > dest) || (dir < 0 && a < dest)) {
                break;
            }
        }
        self.tpl_select = Some(TplSelect::Order { seat: si, order: current });
    }

    /// Removes one order (undoable) and selects its unit, like the tree's ×.
    pub(super) fn remove_tpl_order(&mut self, si: usize, oi: usize) {
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
    pub(super) fn remove_tpl_event(&mut self, si: usize, ei: usize) {
        if si < self.tpl_seats.len() && ei < self.tpl_seats[si].events.len() {
            self.record_tpl_undo(format!("Removed {}", self.tpl_seats[si].events[ei].kind.label()));
            self.tpl_seats[si].events.remove(ei);
            self.tpl_select = Some(TplSelect::Seat(si));
        }
    }

    /// Filter combo, then one 28 px row per model: click a row to preview it,
    /// click its "+ Add" (or double-click the row) to add a unit.
    pub(super) fn draw_template_model_browser(&mut self, ui: &mut egui::Ui) {
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
        let list_h = (ui.available_height() - 40.0).max(160.0);
        let list = egui::ScrollArea::vertical()
            .id_salt("tpl_model_list")
            .max_height(list_h)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let w = ui.available_width();
                for (i, unit) in models.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click_and_drag());
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
                    let resp = resp.on_hover_text(format!(
                        "{}. Double-click, + Add, or drag the row into the formation.",
                        unit.label()
                    ));
                    if resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                    let dropped_outside = resp.drag_stopped()
                        && ui.ctx().pointer_latest_pos().is_some_and(|p| p.x > rect.right() + 24.0);
                    if resp.double_clicked() || (resp.clicked() && over_add) || dropped_outside {
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
        ui.add_space(6.0);
        let picked = models[self.tpl_add_pick].label().to_string();
        if ui
            .add_sized(Vec2::new(ui.available_width(), 28.0), egui::Button::new("Add to Template"))
            .on_hover_text(format!("Add {picked}"))
            .clicked()
        {
            self.template_add_model(models[self.tpl_add_pick].clone());
        }
    }

    /// Payload and Modifications rows of the seat `field_grid`. Returns the
    /// payload description, which the caller shows under the grid.
    pub(super) fn draw_seat_payload_mods(&mut self, ui: &mut egui::Ui, si: usize, ctrl_w: f32) -> Option<String> {
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
    pub(super) fn draw_model_preview(
        &mut self,
        ui: &mut egui::Ui,
        unit: Option<&CatalogUnit>,
        name: Option<&str>,
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
        let scale = (72.0 / size.x).min(72.0 / size.y).min(1.0);
        ui.horizontal_top(|ui| {
            ui.add(egui::Image::new((tex.id(), size * scale)));
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                if let Some(name) = name {
                    ui.add(
                        egui::Label::new(RichText::new(name).font(FontId::new(16.0, theme::heading_family())))
                            .wrap(),
                    );
                }
                let class = model_spec::class_for(&unit.script);
                ui.add(egui::Label::new(plain_class_line(class)).wrap());
                let cruise = model_spec::spec_for(&unit.script)
                    .map(|s| s.cruise_line())
                    .unwrap_or_else(|| model_spec::format_cruise(None));
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
                let payload = payload_line.unwrap_or("—");
                let mods = mods_line.unwrap_or("—");
                ui.add(
                    egui::Label::new(RichText::new(format!("Payload: {payload} · Modifications: {mods} · Skins: —")).small())
                        .wrap(),
                );
            });
        });
    }

    pub(super) fn model_texture(&mut self, ctx: &egui::Context, script: &str) -> TextureHandle {
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

    pub(super) fn classes_for_kind(&self, kind: CatalogKind) -> Vec<ModelClass> {
        model_spec::classes_in(
            self.tpl_catalog
                .iter()
                .filter(|u| u.kind == kind)
                .map(|u| u.script.as_str()),
        )
    }

    /// Countries on the catalog prototypes of `kind`, sorted.
    pub(super) fn countries_for_kind(&self, kind: CatalogKind) -> Vec<i32> {
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
    pub(super) fn displayed_catalog(&self) -> Vec<CatalogUnit> {
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

    pub(super) fn load_unit_catalog(&mut self) {
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

    pub(super) fn add_user_catalog_group(&mut self) {
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

    pub(super) fn sync_template_waypoint_speed(&mut self) {
        self.tpl_wp_speed = model_spec::suggested_waypoint_speed_kmh(
            self.tpl_seats.iter().map(|s| s.unit.script.as_str()),
        );
    }

    pub(super) fn template_zone_mix(&self) -> ZoneMix {
        zone_mix_for_seats(&self.tpl_seats)
            .or(self.tpl_zone_mix)
            .unwrap_or(ZoneMix::Air)
    }

    pub(super) fn sync_template_zone_defaults(&mut self) {
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

    pub(super) fn generate_unit_template(&mut self) {
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

    pub(super) fn reset_template_builder(&mut self) {
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

    pub(super) fn load_template_group(&mut self) {
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
    pub(super) fn draw_template_schematic(&mut self, ui: &mut egui::Ui) {
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
        let show_card = self.selected_tpl_seat().is_some() && !self.tpl_preview_from_catalog;
        let open = formation_open_rect(rect, show_card);
        let fit_scale = (open.width().min(open.height()) * 0.72) / span as f32;
        let mid_x = (min_x + max_x) * 0.5;
        let mid_z = (min_z + max_z) * 0.5;
        let c = open.center();
        self.tpl_view_zoom = self.tpl_view_zoom.clamp(0.04, 12.0);
        let pointer_on_card = show_card
            && ui
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|p| formation_card_rect(rect).contains(p));

        if response.hovered() && !pointer_on_card {
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
        if response.dragged_by(egui::PointerButton::Secondary) && !pointer_on_card {
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
            painter.text(
                Pos2::new(open.left(), rect.top() + 52.0),
                Align2::LEFT_TOP,
                text,
                FontId::proportional(13.0),
                c::TEXT,
            );
        } else if let Some(num) = hover_wp {
            painter.text(
                Pos2::new(open.left(), rect.top() + 52.0),
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
            rect.min + Vec2::new(FORMATION_MARGIN, 12.0),
            Align2::LEFT_TOP,
            "FORMATION VIEW",
            FontId::new(14.0, theme::heading_family()),
            c::TEXT,
        );
        painter.text(
            rect.min + Vec2::new(FORMATION_MARGIN, 32.0),
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
        self.template_formation_unit_card(ui, rect);
        if n == 0 {
            self.template_empty_state(ui, rect);
        }
        painter.text(
            rect.left_bottom() + Vec2::new(FORMATION_MARGIN, -14.0),
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
            if let Some(pos) = response.interact_pointer_pos().filter(|p| {
                !(show_card && formation_card_rect(rect).contains(*p))
            }) {
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
    /// The selected unit's options, on the formation view's 14 px left margin.
    /// The picture and description are above Models.
    fn template_formation_unit_card(&mut self, ui: &mut egui::Ui, canvas: Rect) {
        if self.selected_tpl_seat().is_none() || self.tpl_preview_from_catalog {
            return;
        }
        let card = formation_card_rect(canvas);
        if card.height() < 80.0 {
            return;
        }
        let painter = ui.painter();
        painter.rect_filled(card, 2.0, c::BG);
        painter.rect_stroke(card, 2.0, Stroke::new(1.0_f32, c::DIVIDER), egui::StrokeKind::Inside);
        let body = Rect::from_min_max(Pos2::new(card.left(), card.top() + 8.0), card.max - Vec2::new(8.0, 8.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            ui.spacing_mut().slider_width = 80.0;
            egui::ScrollArea::vertical()
                .id_salt("tpl_formation_unit")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    self.template_selection_block(ui);
                });
        });
    }

    pub(super) fn template_empty_state(&mut self, ui: &mut egui::Ui, canvas: Rect) {
        let pick = self.displayed_catalog().get(self.tpl_add_pick).cloned();
        let open = formation_open_rect(canvas, false);
        let area = Rect::from_center_size(
            open.center() + Vec2::new(0.0, 60.0),
            Vec2::new(open.width().min(420.0), 84.0),
        );
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

    pub(super) fn tpl_snapshot(&self) -> TemplateSnapshot {
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

    pub(super) fn tpl_restore(&mut self, s: TemplateSnapshot) {
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

    pub(super) fn record_tpl_undo(&mut self, label: String) {
        self.tpl_undo.record(label, self.tpl_snapshot());
    }

    /// Everything the Template's Generate reads, as text. Equal text means no edits.
    pub(super) fn tpl_fingerprint(&self) -> String {
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

    pub(super) fn reset_template_confirmed(&mut self) {
        self.record_tpl_undo("Reset the template".into());
        self.reset_template_builder();
        self.mark_saved(AppMode::Template);
    }

    /// Load… on Template. Recorded for undo only when something was loaded.
    pub(super) fn load_template_now(&mut self) {
        let before = self.tpl_snapshot();
        let fp = self.tpl_fingerprint();
        self.run_io(AppMode::Template, |s| s.load_template_group());
        if self.tpl_fingerprint() != fp {
            self.tpl_undo.record("Replaced by Load", before);
        }
    }

}
