//! # shell.rs — shared page layout and widgets for every mode
//!
//! Presentation only, like ui.rs. Nothing here touches the AST.
//! See docs/ui-redesign/README.md §4 for how `update()` composes these.
//! Ported to egui 0.32. Later redesign phases use the widgets that phase 1
//! (theme + shell) does not, hence the module-wide `dead_code` allow.

#![allow(dead_code)]

use std::f32::consts::PI;

use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, FontId, Layout, Pos2, Rect, Response, RichText,
    Sense, Stroke, StrokeKind, TextStyle, TextureHandle, Ui, Vec2,
};

use crate::theme::{bold_family, c, heading_family};

// ── Sizes (px) ──────────────────────────────────────────────────────────────
pub const RAIL_W: f32 = 172.0;
pub const HEADER_H: f32 = 54.0;
pub const STATUS_H: f32 = 30.0;
pub const TOOL_PALETTE_W: f32 = 56.0;
pub const TOOL_SIZE: f32 = 36.0;
pub const ROW_H: f32 = 32.0;

// ── Sides ───────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Dprk,
    Nato,
}

impl Side {
    pub fn from_eastern(eastern: bool) -> Self {
        if eastern { Side::Dprk } else { Side::Nato }
    }
    pub fn label(self) -> &'static str {
        match self {
            Side::Dprk => "DPRK",
            Side::Nato => "NATO",
        }
    }
    /// Only where the MCU coalition value matters.
    pub fn coalition_label(self) -> &'static str {
        match self {
            Side::Dprk => "DPRK [1]",
            Side::Nato => "NATO [2]",
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            Side::Dprk => c::DPRK,
            Side::Nato => c::NATO,
        }
    }
    /// Screen rotation for an icon authored pointing north (up).
    /// DPRK faces south toward the front, NATO faces north.
    pub fn facing_rad(self) -> f32 {
        match self {
            Side::Dprk => PI,
            Side::Nato => 0.0,
        }
    }
}

/// Where `register_side_textures` keeps the two fighter silhouettes.
fn side_textures_id() -> egui::Id {
    egui::Id::new("shell::side_textures")
}

/// Registers the side-marker silhouettes once at startup (§8): the DPRK and
/// NATO fighter icons, both authored pointing north. They become white alpha
/// masks, so every marker is tinted with its side token, and carry mipmaps so
/// they stay clean from 10 to 18 px. `paint_side_marker` finds them through
/// the painter's context; without them it draws the pentagon fallback.
pub fn register_side_textures(ctx: &egui::Context, dprk: ColorImage, nato: ColorImage) {
    let options = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    let mask = |mut img: ColorImage| {
        for p in &mut img.pixels {
            let a = p.a();
            *p = Color32::from_rgba_premultiplied(a, a, a, a);
        }
        img
    };
    let textures = [
        ctx.load_texture("side_marker_dprk", mask(dprk), options),
        ctx.load_texture("side_marker_nato", mask(nato), options),
    ];
    ctx.data_mut(|d| d.insert_temp(side_textures_id(), textures));
}

/// The registered side silhouettes `[DPRK, NATO]`, if any.
pub fn side_textures(ctx: &egui::Context) -> Option<[TextureHandle; 2]> {
    ctx.data(|d| d.get_temp::<[TextureHandle; 2]>(side_textures_id()))
}

/// A texture quad turned about its center (clockwise on screen for positive angles).
pub fn paint_rotated_texture(
    painter: &egui::Painter,
    tex: egui::TextureId,
    center: Pos2,
    size: Vec2,
    angle_rad: f32,
    tint: Color32,
) {
    let rot = egui::emath::Rot2::from_angle(angle_rad);
    let (hx, hy) = (size.x * 0.5, size.y * 0.5);
    let mut mesh = egui::Mesh::with_texture(tex);
    for (off, uv) in [
        (Vec2::new(-hx, -hy), Pos2::new(0.0, 0.0)),
        (Vec2::new(hx, -hy), Pos2::new(1.0, 0.0)),
        (Vec2::new(hx, hy), Pos2::new(1.0, 1.0)),
        (Vec2::new(-hx, hy), Pos2::new(0.0, 1.0)),
    ] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + rot * off,
            uv,
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Side marker (§8): the side's fighter silhouette in its token colour,
/// turned to `side.facing_rad()`. DPRK points south and NATO north, so the
/// two read apart without colour. Falls back to a pentagon pointing the same
/// way when `register_side_textures` has not run (e.g. headless tests).
pub fn paint_side_marker(painter: &egui::Painter, center: Pos2, size: f32, side: Side) {
    if let Some(tex) = side_textures(painter.ctx()) {
        let tex = match side {
            Side::Dprk => &tex[0],
            Side::Nato => &tex[1],
        };
        paint_rotated_texture(
            painter,
            tex.id(),
            center,
            Vec2::splat(size),
            side.facing_rad(),
            side.color(),
        );
    } else {
        paint_side_pentagon(painter, center, size, side);
    }
}

/// The fallback side marker: a pentagon pointing the side's way.
pub fn paint_side_pentagon(painter: &egui::Painter, center: Pos2, size: f32, side: Side) {
    let h = size / 2.0;
    let mut pts = vec![
        Pos2::new(-0.67 * h, 0.75 * h),
        Pos2::new(0.67 * h, 0.75 * h),
        Pos2::new(0.67 * h, -0.25 * h),
        Pos2::new(0.0, -0.92 * h),
        Pos2::new(-0.67 * h, -0.25 * h),
    ];
    if side == Side::Dprk {
        for p in &mut pts {
            p.y = -p.y;
        }
        pts.reverse(); // keep winding after the flip
    }
    let pts = pts.into_iter().map(|p| center + p.to_vec2()).collect();
    painter.add(egui::Shape::convex_polygon(pts, side.color(), Stroke::NONE));
}

pub fn side_marker(ui: &mut Ui, side: Side, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_side_marker(ui.painter(), rect.center(), size, side);
}

// ── Blueprint framing ───────────────────────────────────────────────────────
/// "+" registration marks at each corner, 11 px across, centered on the corner.
pub fn corner_marks(painter: &egui::Painter, rect: Rect, color: Color32) {
    let s = Stroke::new(1.0_f32, color);
    let arm = 5.5;
    for p in [
        rect.left_top(),
        rect.right_top(),
        rect.left_bottom(),
        rect.right_bottom(),
    ] {
        painter.line_segment([p - Vec2::X * arm, p + Vec2::X * arm], s);
        painter.line_segment([p - Vec2::Y * arm, p + Vec2::Y * arm], s);
    }
}

/// List card. Selected: accent border, ACCENT_100 fill, corner marks.
/// Unselected: hairline border, no fill, no marks.
pub fn card<R>(
    ui: &mut Ui,
    selected: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let (stroke, fill) = if selected {
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
        corner_marks(ui.painter(), inner.response.rect, c::MARK);
    }
    inner
}

/// Outlined panel with marks (e.g. "Start checkzones", "Removed").
pub fn blueprint<R>(
    ui: &mut Ui,
    border: Color32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let inner = egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, border))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
    corner_marks(ui.painter(), inner.response.rect, c::MARK);
    inner
}

// ── Text helpers ────────────────────────────────────────────────────────────
/// Condensed, uppercase, 14 px. Optional right-aligned note in Small.
pub fn section_title(ui: &mut Ui, title: &str, note: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title.to_uppercase())
                .text_style(TextStyle::Name("section".into()))
                .color(c::TEXT),
        );
        if let Some(n) = note {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(n).small().color(c::NEUTRAL_700));
            });
        }
    });
}

/// Collapsible settings block for the right panel. Collapsed, it still shows a
/// one-line summary on the right. Open/closed state persists by `id`.
pub fn settings_section(
    ui: &mut Ui,
    id: &str,
    title: &str,
    summary: &str,
    default_open: bool,
    body: impl FnOnce(&mut Ui),
) {
    let id = ui.make_persistent_id(id);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        default_open,
    );
    // The whole 28 px header row toggles (§6.6), not only the small arrow.
    let row = ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 28.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(title).font(FontId::new(13.0, bold_family())));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(summary).small().color(c::NEUTRAL_700))
                        .truncate(),
                );
            });
        },
    );
    let hit = ui.interact(row.response.rect, id.with("header"), Sense::click());
    hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, title));
    if hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if hit.clicked() {
        state.toggle(ui);
    }
    state.show_body_indented(&row.response, ui, |ui| {
        ui.add_space(4.0);
        body(ui);
    });
    ui.separator();
}

/// Small outlined tag (e.g. "Lead ×4"). Accent tags use ACCENT_700 text on
/// ACCENT_100. Painted as one widget so it wraps in `horizontal_wrapped`.
pub fn tag(ui: &mut Ui, text: &str, accent: bool) -> Response {
    let (fg, fill, border) = if accent {
        (c::ACCENT_700, c::ACCENT_100, c::ACCENT_300)
    } else {
        (c::NEUTRAL_800, Color32::TRANSPARENT, c::DIVIDER)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::new(12.0, bold_family()), fg);
    let size = galley.size() + Vec2::new(12.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, 0.0, fill);
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, border), StrokeKind::Inside);
        p.galley(rect.min + Vec2::new(6.0, 3.0), galley, fg);
    }
    resp
}

/// Dashed circle outline (egui has no dashed circle primitive).
pub fn dashed_circle(painter: &egui::Painter, center: Pos2, radius: f32, stroke: Stroke) {
    let n = ((radius * 0.6) as usize).clamp(24, 360);
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.extend(egui::Shape::dashed_line(&pts, stroke, 6.0, 5.0));
}

/// One-line explanation under a control (spec item 8). Optional "Help ›" link.
/// Returns true if the Help link was clicked.
pub fn hint(ui: &mut Ui, text: &str, with_help: bool) -> bool {
    let mut help = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(text).small().color(c::NEUTRAL_700));
        if with_help && link(ui, "Help ›").clicked() {
            help = true;
        }
    });
    help
}

/// Text link with a 28 px tall hit target (README §6.6) that keeps the
/// line height of the text around it.
pub fn link(ui: &mut Ui, text: &str) -> Response {
    let resp = ui.add(
        egui::Label::new(RichText::new(text).color(c::ACCENT_700))
            .sense(Sense::click())
            .selectable(false),
    );
    let pad = ((28.0 - resp.rect.height()) / 2.0).max(0.0);
    let hit = ui.interact(
        resp.rect.expand2(Vec2::new(2.0, pad)),
        resp.id.with("hit"),
        Sense::click(),
    );
    let resp = resp.union(hit);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let r = resp.rect.shrink2(Vec2::new(2.0, pad));
        ui.painter().line_segment(
            [r.left_bottom(), r.right_bottom()],
            Stroke::new(1.0_f32, c::ACCENT_700),
        );
    }
    resp
}

pub fn kbd(ui: &mut Ui, keys: &str) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, c::DIVIDER))
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).monospace().color(c::NEUTRAL_700))
        });
}

pub fn warning(ui: &mut Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("⚠").color(c::WARN_TEXT));
        ui.label(RichText::new(text).color(c::TEXT));
    });
}

// ── Buttons ─────────────────────────────────────────────────────────────────
/// The one solid object on the page: accent fill, marks, shortcut hint inside.
pub fn primary_button(ui: &mut Ui, label: &str, shortcut: &str, enabled: bool) -> Response {
    // Disabled keeps the ACCENT_300 fill; light text on it would be unreadable.
    let (label_c, key_c) = if enabled {
        (c::BG, c::ACCENT_100)
    } else {
        (c::ACCENT_800, c::ACCENT_700)
    };
    let label_g =
        ui.painter()
            .layout_no_wrap(label.to_owned(), FontId::new(13.0, bold_family()), label_c);
    let key_g = ui
        .painter()
        .layout_no_wrap(shortcut.to_owned(), FontId::monospace(12.0), key_c);
    let pad = 16.0;
    let gap = 10.0;
    let size = Vec2::new(pad * 2.0 + label_g.size().x + gap + key_g.size().x, 32.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        let fill = if !enabled {
            c::ACCENT_300
        } else if resp.is_pointer_button_down_on() {
            c::ACCENT_700
        } else if resp.hovered() {
            c::ACCENT_600
        } else {
            c::ACCENT
        };
        let p = ui.painter();
        p.rect_filled(rect, 2.0, fill);
        let y = rect.center().y;
        let key_w = key_g.size().x;
        let key_h = key_g.size().y;
        p.galley(
            Pos2::new(rect.left() + pad, y - label_g.size().y / 2.0),
            label_g,
            label_c,
        );
        p.galley(
            Pos2::new(rect.right() - pad - key_w, y - key_h / 2.0),
            key_g,
            key_c,
        );
        corner_marks(p, rect, c::MARK);
        if resp.has_focus() {
            p.rect_stroke(
                rect.expand(2.0),
                2.0,
                Stroke::new(2.0_f32, c::ACCENT),
                StrokeKind::Outside,
            );
        }
    }
    resp.on_hover_text(format!("{label}  ({shortcut})"))
}

/// Mutually exclusive options in one bordered row (e.g. DPRK | NATO).
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            for (v, label) in options {
                if ui.selectable_label(*value == *v, *label).clicked() && *value != *v {
                    *value = *v;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// 36 × 36 map tool. Active tool is filled accent. Key goes in the tooltip.
pub fn tool_button(
    ui: &mut Ui,
    icon: Option<&TextureHandle>,
    fallback_glyph: &str,
    tint: Option<Color32>,
    tooltip: &str,
    key: &str,
    active: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(TOOL_SIZE), Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, tooltip)
    });
    let (bg, fg) = if active {
        (c::ACCENT, c::BG)
    } else if resp.hovered() {
        (c::ACCENT_100, tint.unwrap_or(c::TEXT))
    } else {
        (Color32::TRANSPARENT, tint.unwrap_or(c::NEUTRAL_800))
    };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, bg);
    match icon {
        Some(tex) => {
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            p.image(tex.id(), rect.shrink(9.0), uv, fg);
        }
        None => {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                fallback_glyph,
                FontId::proportional(18.0),
                fg,
            );
        }
    }
    resp.on_hover_text(format!("{tooltip}  ({key})"))
}

/// Line icons for the map tool palette (mockup 2f), drawn with the painter
/// so no glyph needs a fallback font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolIcon {
    Select,
    Front,
    Salient,
    Arrow,
    Objective,
    /// An air start from the Airfield bank: a small aircraft.
    AirStart,
    Undo,
    Redo,
}

/// Cubic Bézier from `p0` to `p3`, sampled into `out` (without `p0`).
fn cubic(out: &mut Vec<Pos2>, p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2) {
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        let u = 1.0 - t;
        let v = p0.to_vec2() * (u * u * u)
            + p1.to_vec2() * (3.0 * u * u * t)
            + p2.to_vec2() * (3.0 * u * t * t)
            + p3.to_vec2() * (t * t * t);
        out.push(v.to_pos2());
    }
}

/// Half circle of radius 5 around `center` from its top to its bottom,
/// bulging right (`right = true`) or left. Appended to `out`.
fn half_circle(out: &mut Vec<Pos2>, center: Pos2, right: bool) {
    for i in 0..=12 {
        let a = -std::f32::consts::FRAC_PI_2 + PI * i as f32 / 12.0;
        let x = if right { a.cos() } else { -a.cos() };
        out.push(center + Vec2::new(x, a.sin()) * 5.0);
    }
}

/// The icon's strokes in the mockup's 24 × 24 viewBox.
pub(crate) fn tool_icon_paths(icon: ToolIcon) -> Vec<(Vec<Pos2>, bool)> {
    let p = Pos2::new;
    match icon {
        // M3 3l7 17 2.5-7.5L20 10z
        ToolIcon::Select => vec![(
            vec![p(3.0, 3.0), p(10.0, 20.0), p(12.5, 12.5), p(20.0, 10.0)],
            true,
        )],
        // M2 16c3-6 6 2 10-4s7-3 10-6
        ToolIcon::Front => {
            let mut pts = vec![p(2.0, 16.0)];
            cubic(
                &mut pts,
                p(2.0, 16.0),
                p(5.0, 10.0),
                p(8.0, 18.0),
                p(12.0, 12.0),
            );
            cubic(
                &mut pts,
                p(12.0, 12.0),
                p(16.0, 6.0),
                p(19.0, 9.0),
                p(22.0, 6.0),
            );
            vec![(pts, false)]
        }
        // M2 17h5c2 0 2-9 5-9s3 9 5 9h5
        ToolIcon::Salient => {
            let mut pts = vec![p(2.0, 17.0), p(7.0, 17.0)];
            cubic(
                &mut pts,
                p(7.0, 17.0),
                p(9.0, 17.0),
                p(9.0, 8.0),
                p(12.0, 8.0),
            );
            cubic(
                &mut pts,
                p(12.0, 8.0),
                p(15.0, 8.0),
                p(15.0, 17.0),
                p(17.0, 17.0),
            );
            pts.push(p(22.0, 17.0));
            vec![(pts, false)]
        }
        // M5 19L19 5M10 5h9v9
        ToolIcon::Arrow => vec![
            (vec![p(5.0, 19.0), p(19.0, 5.0)], false),
            (vec![p(10.0, 5.0), p(19.0, 5.0), p(19.0, 14.0)], false),
        ],
        // M5 22V3M5 4h12l-2.5 4L17 12H5
        ToolIcon::Objective => vec![
            (vec![p(5.0, 22.0), p(5.0, 3.0)], false),
            (
                vec![
                    p(5.0, 4.0),
                    p(17.0, 4.0),
                    p(14.5, 8.0),
                    p(17.0, 12.0),
                    p(5.0, 12.0),
                ],
                false,
            ),
        ],
        // Fuselage with wings, nose toward the top right.
        ToolIcon::AirStart => vec![
            (vec![p(6.0, 18.0), p(18.0, 6.0)], false),
            (vec![p(8.0, 10.0), p(16.0, 14.0)], false),
            (vec![p(14.0, 8.0), p(18.0, 6.0), p(16.0, 10.0)], false),
        ],
        // M9 14L4 9l5-5M4 9h11a5 5 0 010 10h-4
        ToolIcon::Undo => {
            let mut tail = vec![p(4.0, 9.0)];
            half_circle(&mut tail, p(15.0, 14.0), true);
            tail.push(p(11.0, 19.0));
            vec![
                (vec![p(9.0, 14.0), p(4.0, 9.0), p(9.0, 4.0)], false),
                (tail, false),
            ]
        }
        // M15 14l5-5-5-5M20 9H9a5 5 0 000 10h4
        ToolIcon::Redo => {
            let mut tail = vec![p(20.0, 9.0)];
            half_circle(&mut tail, p(9.0, 14.0), false);
            tail.push(p(13.0, 19.0));
            vec![
                (vec![p(15.0, 14.0), p(20.0, 9.0), p(15.0, 4.0)], false),
                (tail, false),
            ]
        }
    }
}

/// Paints `icon` as 1.5 px lines inside `rect` (the 24-unit viewBox scaled to fit).
pub fn paint_tool_icon(painter: &egui::Painter, rect: Rect, icon: ToolIcon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - Vec2::splat(12.0 * scale);
    let stroke = Stroke::new(1.5_f32, color);
    for (pts, closed) in tool_icon_paths(icon) {
        let pts: Vec<Pos2> = pts
            .into_iter()
            .map(|q| origin + q.to_vec2() * scale)
            .collect();
        if closed {
            painter.add(egui::Shape::closed_line(pts, stroke));
        } else {
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
}

/// 36 × 36 map tool with a painted line icon (mockup 2f). The active tool
/// is filled ACCENT; `key` (a digit) sits small in the bottom-right corner
/// when `corner_key` is set, and always goes in the hover text.
pub fn tool_icon_button(
    ui: &mut Ui,
    icon: ToolIcon,
    tint: Option<Color32>,
    tooltip: &str,
    key: &str,
    corner_key: bool,
    active: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(TOOL_SIZE), Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, tooltip)
    });
    if ui.is_rect_visible(rect) {
        let (bg, fg) = if active {
            (c::ACCENT, c::BG)
        } else if resp.hovered() {
            (c::ACCENT_100, tint.unwrap_or(c::TEXT))
        } else {
            (Color32::TRANSPARENT, tint.unwrap_or(c::NEUTRAL_800))
        };
        let p = ui.painter();
        p.rect_filled(rect, 0.0, bg);
        paint_tool_icon(
            p,
            Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
            icon,
            fg,
        );
        if corner_key {
            let key_c = if active {
                c::ACCENT_100
            } else {
                c::NEUTRAL_700
            };
            p.text(
                rect.right_bottom() + Vec2::new(-2.0, 1.0),
                Align2::RIGHT_BOTTOM,
                key,
                FontId::monospace(12.0),
                key_c,
            );
        }
        if resp.has_focus() {
            p.rect_stroke(
                rect,
                0.0,
                Stroke::new(2.0_f32, c::ACCENT),
                StrokeKind::Inside,
            );
        }
    }
    resp.on_hover_text(format!("{tooltip}  ({key})"))
}

/// Tab strip across the full width (the Map dock, mockup 2f). Cells are
/// equal when every label fits an equal share; otherwise each cell is its
/// label's width plus an equal share of the space left, so a long tab
/// ("References 123") never clips. The selected tab is bold with a 2 px
/// accent underline; a tab's count follows its label as a separate 12 px
/// NEUTRAL_700 number. Widths are measured bold, so selecting a tab never
/// moves the others. Returns true when the selection changed.
pub fn dock_tabs<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    tabs: &[(T, &str, Option<usize>)],
) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    let cells = dock_tab_widths(ui.ctx(), width, tabs);
    let (strip, _) = ui.allocate_exact_size(Vec2::new(width, DOCK_TAB_H), Sense::hover());
    let mut x = strip.left();
    for (i, (v, label, count)) in tabs.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(x, strip.top()), Vec2::new(cells[i], DOCK_TAB_H));
        x += cells[i];
        let selected = *value == *v;
        let name = match count {
            Some(n) => format!("{label} {n}"),
            None => (*label).to_owned(),
        };
        let resp = ui.interact(rect, ui.id().with(("dock_tab", i)), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &name)
        });
        if resp.clicked() && !selected {
            *value = *v;
            changed = true;
        }
        let (label_g, count_g) = dock_tab_galleys(ui.ctx(), label, *count, selected);
        let gap = if count_g.is_some() { DOCK_TAB_GAP } else { 0.0 };
        let total = label_g.size().x + gap + count_g.as_ref().map_or(0.0, |g| g.size().x);
        let p = ui.painter();
        if resp.hovered() && !selected {
            p.rect_filled(rect, 0.0, c::ACCENT_100);
        }
        let x0 = rect.center().x - total / 2.0;
        let y = rect.center().y;
        let label_w = label_g.size().x;
        let label_h = label_g.size().y;
        p.galley(Pos2::new(x0, y - label_h / 2.0), label_g, c::TEXT);
        if let Some(g) = count_g {
            // Baselines line up closely enough: both galleys are centred on the row.
            let h = g.size().y;
            p.galley(
                Pos2::new(x0 + label_w + gap, y - h / 2.0 + 1.0),
                g,
                c::NEUTRAL_700,
            );
        }
        if selected {
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(rect.left(), rect.bottom() - 2.0),
                    rect.right_bottom(),
                ),
                0.0,
                c::ACCENT,
            );
        }
        if resp.has_focus() {
            p.rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(2.0_f32, c::ACCENT),
                StrokeKind::Inside,
            );
        }
    }
    ui.painter().line_segment(
        [strip.left_bottom(), strip.right_bottom()],
        Stroke::new(1.0_f32, c::DIVIDER),
    );
    changed
}

/// Height of a `dock_tabs` row (the mockup's 40 px line).
pub const DOCK_TAB_H: f32 = 40.0;

fn dock_tab_galleys(
    ctx: &egui::Context,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> (
    std::sync::Arc<egui::Galley>,
    Option<std::sync::Arc<egui::Galley>>,
) {
    let (family, fg) = if selected {
        (bold_family(), c::ACCENT_800)
    } else {
        (egui::FontFamily::Proportional, c::TEXT)
    };
    ctx.fonts(|f| {
        let label_g = f.layout_no_wrap(label.to_owned(), FontId::new(13.0, family), fg);
        let count_g = count
            .map(|n| f.layout_no_wrap(n.to_string(), FontId::proportional(12.0), c::NEUTRAL_700));
        (label_g, count_g)
    })
}

/// Width a `dock_tabs` cell needs for `label` and `count` (bold, as when
/// selected), in px. Layout tests compare it with the cell width.
pub fn dock_tab_needed_width(ctx: &egui::Context, label: &str, count: Option<usize>) -> f32 {
    let (l, c) = dock_tab_galleys(ctx, label, count, true);
    l.size().x + c.map_or(0.0, |g| DOCK_TAB_GAP + g.size().x)
}

/// Space between a tab label and its count.
const DOCK_TAB_GAP: f32 = 4.0;
/// Least padding on each side of a tab's label.
pub const DOCK_TAB_PAD: f32 = 6.0;

/// Cell widths for `dock_tabs` across `width` (see there).
pub fn dock_tab_widths<T>(
    ctx: &egui::Context,
    width: f32,
    tabs: &[(T, &str, Option<usize>)],
) -> Vec<f32> {
    let n = tabs.len().max(1) as f32;
    let need: Vec<f32> = tabs
        .iter()
        .map(|(_, label, count)| dock_tab_needed_width(ctx, label, *count) + 2.0 * DOCK_TAB_PAD)
        .collect();
    let equal = width / n;
    if need.iter().all(|w| *w <= equal) {
        return vec![equal; tabs.len()];
    }
    let spare = (width - need.iter().sum::<f32>()).max(0.0) / n;
    need.iter().map(|w| w + spare).collect()
}

// ── Page chrome ─────────────────────────────────────────────────────────────
/// What the mode rail's bottom controls did this frame.
pub struct RailClick {
    pub help: bool,
    pub reset: bool,
}

/// Left mode rail. Help and Reset all sit at the bottom of the rail.
pub fn mode_rail(ui: &mut Ui, labels: &[&str], selected: &mut usize) -> RailClick {
    let mut help = false;
    let mut reset = false;
    ui.add_space(14.0);
    ui.label(RichText::new("IL-2 GROUP\nGENERATOR").font(FontId::new(17.0, heading_family())));
    ui.add_space(12.0);
    for (i, label) in labels.iter().enumerate() {
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
        let sel = *selected == i;
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, sel, *label)
        });
        let p = ui.painter();
        if sel {
            p.rect_filled(rect, 0.0, c::ACCENT_200);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, c::ACCENT_100);
        }
        let idx_color = if sel { c::ACCENT_800 } else { c::NEUTRAL_600 };
        let fg = if sel { c::ACCENT_900 } else { c::TEXT };
        let family = if sel {
            bold_family()
        } else {
            egui::FontFamily::Proportional
        };
        p.text(
            rect.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{:02}", i + 1),
            FontId::monospace(12.0),
            idx_color,
        );
        p.text(
            rect.left_center() + Vec2::new(34.0, 0.0),
            Align2::LEFT_CENTER,
            *label,
            FontId::new(13.0, family),
            fg,
        );
        if resp.clicked() {
            *selected = i;
        }
        resp.on_hover_text(format!("Ctrl {}", i + 1));
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("Ctrl 1–{}", labels.len()))
            .small()
            .color(c::NEUTRAL_700),
    );
    ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if link(ui, "Help")
                .on_hover_text("Help for this tab  (F1)")
                .clicked()
            {
                help = true;
            }
            kbd(ui, "F1");
        });
        ui.add_space(6.0);
        if ui
            .add_sized(
                [ui.available_width(), 28.0],
                egui::Button::new("Reset all"),
            )
            .on_hover_text(
                "Clear the template, map, missions, airfields, and every other setting.",
            )
            .clicked()
        {
            reset = true;
        }
    });
    RailClick { help, reset }
}

// ── Readiness (README §10) ──────────────────────────────────────────────────
/// One rule a tab's output must meet before Generate is enabled.
pub struct Check {
    pub ok: bool,
    pub text: String,
}

impl Check {
    pub fn new(ok: bool, text: impl Into<String>) -> Self {
        Check {
            ok,
            text: text.into(),
        }
    }
}

pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.ok)
}

/// Chip beside Generate: "✔ Ready" or "⚠ n to do". Its menu lists every
/// check, so the reason Generate is disabled is never hidden (§6.5).
pub fn readiness(ui: &mut Ui, checks: &[Check]) {
    if checks.is_empty() {
        return;
    }
    let open = checks.iter().filter(|c| !c.ok).count();
    let label = if open == 0 {
        RichText::new("✔ Ready ▾").color(c::ACCENT_700)
    } else {
        RichText::new(format!("⚠ {open} to do ▾")).color(c::WARN_TEXT)
    };
    ui.menu_button(label, |ui| {
        ui.set_min_width(260.0);
        ui.label(
            RichText::new("BEFORE YOU GENERATE")
                .small()
                .color(c::NEUTRAL_700),
        );
        for check in checks {
            ui.horizontal(|ui| {
                if check.ok {
                    ui.label(RichText::new("✔").color(c::ACCENT_700));
                    ui.label(&check.text);
                } else {
                    ui.label(RichText::new("✖").color(c::WARN_TEXT));
                    ui.label(RichText::new(&check.text).strong());
                }
            });
        }
    })
    .response
    .on_hover_text("What this tab needs before Generate");
}

/// Header row. `leading` draws right after the title (e.g. a sub-mode switch).
/// `secondary` runs inside a right-to-left layout: add buttons in REVERSE order.
/// `checks` show as a readiness chip next to the primary button.
/// Returns true when the primary button is clicked.
#[allow(clippy::too_many_arguments)]
pub fn page_header(
    ui: &mut Ui,
    title: &str,
    file: Option<&str>,
    primary: &str,
    primary_enabled: bool,
    checks: &[Check],
    leading: impl FnOnce(&mut Ui),
    secondary: impl FnOnce(&mut Ui),
) -> bool {
    let mut clicked = false;
    ui.horizontal_centered(|ui| {
        ui.label(RichText::new(title.to_uppercase()).text_style(TextStyle::Heading));
        leading(ui);
        if let Some(f) = file {
            ui.label(RichText::new(f).italics().color(c::NEUTRAL_700));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            clicked = primary_button(ui, primary, "Ctrl G", primary_enabled).clicked();
            ui.add_space(4.0);
            readiness(ui, checks);
            ui.add_space(8.0);
            secondary(ui);
        });
    });
    clicked
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// Bottom line. Returns true when Undo is clicked.
pub fn status_bar(
    ui: &mut Ui,
    severity: Severity,
    message: &str,
    undo_label: Option<&str>,
) -> bool {
    let mut undo = false;
    ui.horizontal_centered(|ui| {
        let dot = match severity {
            Severity::Info => c::ACCENT,
            Severity::Warn => c::WARN,
            Severity::Error => c::DPRK,
        };
        let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, dot);
        if severity != Severity::Info {
            ui.label(RichText::new("⚠").color(c::WARN_TEXT));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(label) = undo_label {
                kbd(ui, "Ctrl Z");
                if link(ui, "Undo")
                    .on_hover_text(format!("Undo: {label}  (Ctrl Z)"))
                    .clicked()
                {
                    undo = true;
                }
                ui.label(label);
            }
            // The message takes what is left and elides; the full text shows on hover.
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(message).truncate());
            });
        });
    });
    undo
}

/// Dark banner at the top-center of the map naming the active drawing tool.
pub fn map_tool_banner(painter: &egui::Painter, map_rect: Rect, name: &str, hint: &str) {
    let text = format!("{}    {}", name.to_uppercase(), hint);
    let galley = painter.layout_no_wrap(text, FontId::proportional(13.0), c::BG);
    let size = galley.size() + Vec2::new(24.0, 12.0);
    let rect = Rect::from_center_size(
        Pos2::new(map_rect.center().x, map_rect.top() + 12.0 + size.y / 2.0),
        size,
    );
    painter.rect_filled(rect, 0.0, c::ACCENT_900);
    painter.galley(rect.min + Vec2::new(12.0, 6.0), galley, c::BG);
}

/// Confirmation before a destructive action. Returns true on confirm.
/// Caller closes it on Esc by setting `*open = false`.
pub fn confirm_dialog(
    ctx: &egui::Context,
    open: &mut bool,
    title: &str,
    body: &str,
    confirm: &str,
) -> bool {
    if !*open {
        return false;
    }
    let mut confirmed = false;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label(body);
            ui.add_space(10.0);
            // A fixed-height row: a bare with_layout would take the window's full height.
            let row = Vec2::new(ui.available_width(), 32.0);
            ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
                let ok = primary_button(ui, confirm, "Enter", true);
                let cancel = ui.button("Cancel").on_hover_text("Esc");
                // Enter confirms, except when the keyboard focus is on Cancel.
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter)) && !cancel.has_focus();
                if cancel.clicked() {
                    *open = false;
                } else if ok.clicked() || enter {
                    confirmed = true;
                    *open = false;
                }
            });
        });
    confirmed
}

// ── Undo (one snapshot per tab, spec item 7) ────────────────────────────────
pub struct Undo<T> {
    slot: Option<(String, T)>,
    /// Fingerprint of the tab right after the change; see `settle`.
    after: Option<u64>,
}

impl<T> Default for Undo<T> {
    fn default() -> Self {
        Self {
            slot: None,
            after: None,
        }
    }
}

impl<T> Undo<T> {
    /// Call BEFORE the destructive change with a clone of what it removes.
    pub fn record(&mut self, label: impl Into<String>, before: T) {
        self.slot = Some((label.into(), before));
        self.after = None;
    }
    /// Call once per frame with a fingerprint of the tab's state. The first
    /// call after `record` remembers the state the change left; any later
    /// edit makes the snapshot stale, so it is dropped rather than letting
    /// Ctrl Z silently throw that edit away.
    pub fn settle(&mut self, fingerprint: u64) {
        if self.slot.is_none() {
            return;
        }
        match self.after {
            None => self.after = Some(fingerprint),
            Some(a) if a != fingerprint => self.clear(),
            Some(_) => {}
        }
    }
    /// The tab changed in a way that keeps the snapshot valid (a drawing
    /// that has its own undo): take the next `settle` fingerprint as the new
    /// "after" state instead of dropping the snapshot.
    pub fn rebase(&mut self) {
        self.after = None;
    }
    pub fn label(&self) -> Option<&str> {
        self.slot.as_ref().map(|(l, _)| l.as_str())
    }
    /// The snapshot, without taking it.
    pub fn peek(&self) -> Option<&T> {
        self.slot.as_ref().map(|(_, s)| s)
    }
    pub fn take(&mut self) -> Option<T> {
        self.slot.take().map(|(_, s)| s)
    }
    pub fn clear(&mut self) {
        self.slot = None;
    }
}

// ── Shortcuts (spec item 10) ────────────────────────────────────────────────
#[derive(Default)]
pub struct Shortcuts {
    pub generate: bool,
    pub load: bool,
    pub undo: bool,
    pub redo: bool,
    pub help: bool,
    pub escape: bool,
    pub tab: Option<usize>,
    pub map_tool: Option<usize>,
}

/// Read once at the top of `update()`, before any panel is drawn.
/// Remove the Ctrl-Z / Ctrl-Y handling inside `map_draw_toolbar` so keys are not consumed twice.
pub fn read_shortcuts(ctx: &egui::Context, map_active: bool) -> Shortcuts {
    use egui::{Key, Modifiers};
    let typing = ctx.wants_keyboard_input();
    let mut s = Shortcuts::default();
    ctx.input_mut(|i| {
        s.generate = i.consume_key(Modifiers::COMMAND, Key::G);
        s.load = i.consume_key(Modifiers::COMMAND, Key::O);
        // Leave Ctrl Z / Ctrl Y to a focused text field's own undo.
        if !typing {
            s.undo = i.consume_key(Modifiers::COMMAND, Key::Z);
            s.redo = i.consume_key(Modifiers::COMMAND, Key::Y);
        }
        s.help = i.consume_key(Modifiers::NONE, Key::F1);
        // Not consumed: egui's own popups, combo boxes and text edits also close on Esc.
        s.escape = i.key_pressed(Key::Escape);
        // Ctrl 1–6 follow the rail: Map, Template, Army, Mission Tasking,
        // Airfields, Mission. Bare 1–6 stay map tools, and only while the map is up.
        let nums = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
        ];
        for (n, k) in nums.iter().enumerate() {
            if i.consume_key(Modifiers::COMMAND, *k) {
                s.tab = Some(n);
            }
        }
        let tools = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
        ];
        if map_active && !typing {
            for (n, k) in tools.iter().enumerate() {
                if i.consume_key(Modifiers::NONE, *k) {
                    s.map_tool = Some(n);
                }
            }
        }
    });
    s
}
