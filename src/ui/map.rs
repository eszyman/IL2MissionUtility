//! # map.rs — Map mode
//!
//! One mode of [`super::GroupGeneratorApp`]. This file owns the Korea map:
//! drawing, the Period / Forces / References / Terrain dock, AO lock, fighter
//! textures, and the height-store controls.
//!
//! `ui.rs` is the anchor. It owns the app struct (the `front_*` / `map_*` /
//! `terrain_*` fields), the mode rail, and the shortcuts, and it calls the
//! hooks listed below. State stays on the app.
//!
//! Hooks `ui.rs` calls (keep these `pub(super)`):
//! * `map_page` — the tab
//! * `generate_front_file` — Generate Base Map / Ctrl G
//! * `load_base_map` — Load base map… / Ctrl O
//! * `apply_terrain` / `add_terrain_note` — export heights, every mode except Airfield
//! * `map_fingerprint` — unsaved-edit check
//! * `cancel_map_tool` / `pick_map_tool` / `map_tool_status` / `redo_last_mark`
//! * `map_undo_kind` / `map_undo_label` / `restore_map_forces` / `remove_last_mark`
//! * `ensure_side_textures` — list side markers, once per frame
//!
//! Presentation only. Group text is built by [`crate::frontlines`] and the
//! other map modules. Heights come from [`crate::terrain::open_store`].

use std::path::PathBuf;

use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, FontFamily, FontId, Layout, Pos2, Rect, RichText,
    Sense, Stroke, TextureHandle, Vec2,
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
use crate::heightprobe;
use crate::help::HelpTopic;
use crate::locale::{merge_template_sidecars, write_sidecars, LANG_EXTS};
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
use crate::placement::PlaceOpts;
use crate::pack::{builtin_template, generate_pack_at, group_anchor_xz, park_rtbs, zone_in_radius};
use crate::parser::{parse_group_file, parse_il2_document};
use crate::recon::{
    allocate_copies, generate_recon_ex, inspect_army_copies, park_army_mixed,
    park_recon_copies_headed, park_recon_copies_spots, snap_army_placed_attack_areas,
    snap_placed_attack_areas, ArmyCopyInfo, ReconBuild, ReconInput, wanted_winners,
};
use crate::serialize::serialize_group;
use crate::shell;
use crate::template::TRAIN_ZONE_IN_M;
use crate::terrain::HeightStore;
use crate::theme::{self, c};
use crate::weapon_range::{self, ArmyUnitKind};

use super::{
    dialog, group_digits, labeled_slider, section_heading, ReconSlot, Status, UnitKind,
};

/// Terrain lattice spacing in metres (terrain::STEP_M).
const TERRAIN_STEP: f64 = crate::terrain::STEP_M;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MapDrawingMode {
    None,
    BaseFront,
    Salient,
    AttackArrow,
    PlaceEastObjective,
    PlaceNatoObjective,
}

/// Map tool palette, top to bottom; keys 1–6 pick them (README §5.6).
/// (mode, icon, name, banner / status hint)
pub(super) const MAP_TOOLS: [(MapDrawingMode, shell::ToolIcon, &str, &str); 6] = [
    (MapDrawingMode::None, shell::ToolIcon::Select, "Select AO / move units", "Drag a box for the AO, or drag a unit"),
    (MapDrawingMode::BaseFront, shell::ToolIcon::Front, "Draw front", "Click west to east, or drag · Esc to cancel"),
    (MapDrawingMode::Salient, shell::ToolIcon::Salient, "Salient", "Click along the front · right-click to finish · Esc to cancel"),
    (MapDrawingMode::AttackArrow, shell::ToolIcon::Arrow, "Attack arrow", "Drag from tail to tip · Esc to cancel"),
    (MapDrawingMode::PlaceEastObjective, shell::ToolIcon::Objective, "DPRK objective", "Click to place · Shift for more"),
    (MapDrawingMode::PlaceNatoObjective, shell::ToolIcon::Objective, "NATO objective", "Click to place · Shift for more"),
];

/// What a Map Clear or Remove can take away (README §6.3).
pub(super) struct MapForces {
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
    /// `ui.rs` reads this while settling undo, so the field is visible there.
    pub(super) lines: Option<MapLines>,
}

/// What Clear lines / salients / arrows can take away.
pub(super) struct MapLines {
    custom_front: Vec<(f64, f64)>,
    salients: Vec<Vec<(f64, f64)>>,
    attack_arrows: Vec<((f64, f64), (f64, f64))>,
    drawn_marks: Vec<DrawnMark>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MapAction {
    Drawing,
    Clear,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MapDock {
    Period,
    Forces,
    References,
    Terrain,
}

impl MapDock {
    /// The dock's tab strip; References carries its count.
    pub(super) fn tabs(refs: usize) -> [(MapDock, &'static str, Option<usize>); 4] {
        [
            (MapDock::Period, "Period", None),
            (MapDock::Forces, "Forces", None),
            (MapDock::References, "References", Some(refs)),
            (MapDock::Terrain, "Terrain", None),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DrawnMark {
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
pub(super) struct MapArmySlot {
    path: PathBuf,
    entity: crate::ast::Il2Entity,
    eastern: bool,
    reposition: bool,
    copies: Vec<ArmyCopyInfo>,
    ground: Option<MapGroundLayout>,
    ships: Option<MapShipLayout>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GroundHit {
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
pub(super) enum ShipHit {
    Ag(usize),
    Army { slot: usize, i: usize },
}

impl ShipHit {
    fn is_ag(self) -> bool {
        matches!(self, ShipHit::Ag(_))
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
pub(super) fn fighter_svg_north(eastern: bool, target: u32) -> ColorImage {
    if eastern {
        rasterize_svg_turned(include_bytes!("../../assets/EasternFighter.svg"), target, FIGHTER_SVG_TO_NORTH_DEG[0])
    } else {
        rasterize_svg_turned(include_bytes!("../../assets/NatoFighter.svg"), target, FIGHTER_SVG_TO_NORTH_DEG[1])
    }
}

/// Registers the side-marker silhouettes once (`shell::paint_side_marker`).
/// 64 px with mipmaps stays crisp from 10 to 18 px, also on HiDPI screens.
pub(super) fn ensure_side_textures(ctx: &egui::Context) {
    if shell::side_textures(ctx).is_none() {
        shell::register_side_textures(ctx, side_marker_image(true), side_marker_image(false));
    }
}

/// The list-size side marker: the fighter without its ring, cropped to the
/// plane, so the silhouette and its facing still read at 12–18 px.
fn side_marker_image(eastern: bool) -> ColorImage {
    let svg: &[u8] = if eastern {
        include_bytes!("../../assets/EasternFighter.svg")
    } else {
        include_bytes!("../../assets/NatoFighter.svg")
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

pub(super) fn rasterize_svg(bytes: &[u8], target: u32) -> ColorImage {
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

pub(super) enum KoreaMapLayer {
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

impl super::GroupGeneratorApp {
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

    pub(super) fn restore_map_forces(&mut self, f: MapForces) {
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
    pub(super) fn map_undo_kind(&self) -> Option<MapAction> {
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
    pub(super) fn map_undo_label(&self) -> Option<&str> {
        match self.map_undo_kind()? {
            MapAction::Clear => self.map_undo.label(),
            MapAction::Drawing if self.map_stroke_in_progress() => Some("Unfinished drawing"),
            MapAction::Drawing => self.drawn_marks.last().map(|m| m.label()),
        }
    }

    pub(super) fn map_fingerprint(&self) -> String {
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

    /// The height store, loaded the first time it is needed.
    ///
    /// Production starts from the baked store and merges a newer file on
    /// top. UI tests set `terrain_use_builtin` off and read only the file.
    fn terrain_store(&mut self) -> Option<&HeightStore> {
        if self.terrain_store.is_none() && self.terrain_error.is_none() {
            let loaded = if self.terrain_use_builtin {
                crate::terrain::open_store(&self.terrain_store_path)
            } else {
                HeightStore::load(&self.terrain_store_path, crate::terrain::KOREA_MAP_ID)
            };
            match loaded {
                Ok(s) => self.terrain_store = Some(s),
                Err(e) => {
                    self.terrain_error = Some(e);
                    if self.terrain_use_builtin {
                        self.terrain_store = crate::terrain::builtin().ok();
                    }
                }
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
    pub(super) fn terrain_readout(&mut self) -> Option<String> {
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
        if self.terrain_use_builtin {
            ui.label(
                RichText::new("Built into this version. A newer file is merged on top.")
                    .small()
                    .color(c::NEUTRAL_700),
            );
        }
        let path = self.terrain_store_path.display().to_string();
        ui.add(egui::Label::new(RichText::new(path).small().monospace().color(c::NEUTRAL_700)).truncate());
        let total_probes: usize = self.terrain_tiles().iter().map(|t| t.2).sum();
        let tiles = self.terrain_tiles().to_vec();
        if let Some(err) = self.terrain_error.clone() {
            shell::warning(ui, &err);
        }
        if let Some(store) = self.terrain_store() {
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
        let reload = if self.terrain_use_builtin {
            "Read the built-in heights again, then merge the file"
        } else {
            "Read the store from disk again"
        };
        if ui.button("Reload").on_hover_text(reload).clicked() {
            self.terrain_store = None;
            self.terrain_error = None;
            self.terrain_relief = None;
        }
        ui.checkbox(&mut self.terrain_apply, "Apply terrain heights on export").on_hover_text(
            "Ground units and their waypoints go to the measured ground + margin, parked planes just above it, ships to sea level. Units already on the ground keep their height.",
        );
        shell::hint(
            ui,
            "On for this session. Template, Exclusive, Army, Map and Fighter Pack exports use it (not Airfield). Turn it off to keep authored heights. Where the terrain is not measured, units keep their height and the export lists them.",
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
    pub(super) fn apply_terrain(&mut self, root: &mut crate::ast::Il2Entity) -> Option<(String, bool)> {
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
    pub(super) fn add_terrain_note(&mut self, note: Option<(String, bool)>) {
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
        let g = heightprobe::probe_group(heightprobe::SURVEY_NAME, &heightprobe::survey_nodes());
        let n = g.children.len();
        match std::fs::write(&path, serialize_group(&g)) {
            Ok(()) => {
                self.status = Status::Info(format!(
                    "Wrote {n} survey probes to {}. Import it, select all, set to ground, save, then Import snapped.",
                    path.display()
                ));
            }
            Err(e) => self.status = Status::Error(format!("{}: {e}", path.display())),
        }
    }

    fn terrain_export_tiles(&mut self, tiles: &[(usize, usize, usize)]) {
        let Some(dir) = dialog::FileDialog::new().pick_folder() else {
            return;
        };
        let mut written = 0;
        let mut probes = 0;
        for &(ti, tj, _) in tiles {
            let Some(g) = heightprobe::tile_probe_group(ti, tj) else {
                continue;
            };
            let path = dir.join(format!("{}.Group", heightprobe::tile_name(ti, tj)));
            if let Err(e) = std::fs::write(&path, serialize_group(&g)) {
                self.status = Status::Error(format!("{}: {e}", path.display()));
                return;
            }
            written += 1;
            probes += g.children.len();
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

    /// Map tab (README §5.6): tool palette, the map with a front-date strip,
    /// and a right dock with Period / Forces / References.
    pub(super) fn map_page(&mut self, ctx: &egui::Context) {
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
    pub(super) fn cancel_map_tool(&mut self) {
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
    pub(super) fn pick_map_tool(&mut self, mode: MapDrawingMode) {
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
    pub(super) fn map_tool_status(&self) -> Option<String> {
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

    fn side_has_fighters(&self, eastern: bool) -> bool {
        self.map_fighters
            .as_ref()
            .is_some_and(|l| l.eastern == eastern && !l.spots.is_empty())
            || self
                .map_imported_fighters
                .iter()
                .any(|p| p.eastern == eastern && !p.spots.is_empty())
    }

    /// Clears the fighter layout when it is this side, and that side's imported packs.
    fn clear_side_fighters(&mut self, eastern: bool) {
        let n = self
            .map_fighters
            .as_ref()
            .filter(|l| l.eastern == eastern)
            .map_or(0, |l| l.spots.len());
        let side = if eastern { "DPRK" } else { "NATO" };
        self.record_map_undo(format!("Cleared {n} {side} fighter groups"));
        if self.map_fighters.as_ref().is_some_and(|l| l.eastern == eastern) {
            self.map_fighters = None;
        }
        self.map_imported_fighters.retain(|p| p.eastern != eastern);
        self.fighter_drag = None;
    }

    fn clear_side_objectives(&mut self, eastern: bool) {
        let n = if eastern {
            self.east_objectives.len()
        } else {
            self.nato_objectives.len()
        };
        let side = if eastern { "DPRK" } else { "NATO" };
        self.record_map_undo(format!("Cleared {n} {side} objectives"));
        if eastern {
            self.east_objectives.clear();
        } else {
            self.nato_objectives.clear();
        }
        if self.objective_drag.is_some_and(|(e, _)| e == eastern) {
            self.objective_drag = None;
        }
        self.reaim_map_ground();
    }

    fn side_has_units(&self, eastern: bool) -> bool {
        let ground = if eastern { &self.map_ground_east } else { &self.map_ground_nato };
        ground.is_some()
            || self.map_armies.iter().any(|a| a.eastern == eastern)
            || self.map_ships.as_ref().is_some_and(|l| l.eastern == eastern)
    }

    /// Clears this side's placed ground, loaded armies, and ships (the ship
    /// layout when it is this side).
    fn clear_side_units(&mut self, eastern: bool) {
        let ground_n = (if eastern { &self.map_ground_east } else { &self.map_ground_nato })
            .as_ref()
            .map_or(0, |l| l.spots.len());
        let army_n: usize = self
            .map_armies
            .iter()
            .filter(|a| a.eastern == eastern)
            .map(|a| {
                a.ground.as_ref().map_or(0, |g| g.spots.len())
                    + a.ships.as_ref().map_or(0, |s| s.spots.len())
            })
            .sum();
        let ship_n = self
            .map_ships
            .as_ref()
            .filter(|l| l.eastern == eastern)
            .map_or(0, |l| l.spots.len());
        let side = if eastern { "DPRK" } else { "NATO" };
        self.record_map_undo(format!("Cleared {} {side} units", ground_n + army_n + ship_n));
        if eastern {
            self.map_ground_east = None;
        } else {
            self.map_ground_nato = None;
        }
        if self.map_ships.as_ref().is_some_and(|l| l.eastern == eastern) {
            self.map_ships = None;
        }
        self.map_armies.retain(|a| a.eastern != eastern);
        self.ship_drag = None;
        self.ship_heading_drag = None;
        self.ground_drag = None;
        self.ground_heading_drag = None;
        self.wp_drag = None;
        self.wp_selected = None;
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
        if let Some(layout) = &self.map_fighters {
            ui.horizontal(|ui| {
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
            });
        }
        let clear_dprk_f = self.side_has_fighters(true);
        let clear_nato_f = self.side_has_fighters(false);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(clear_dprk_f, |ui| {
                if ui.button("Clear DPRK").on_hover_text("Clear DPRK fighters. Ctrl Z brings them back.").clicked() {
                    self.clear_side_fighters(true);
                }
            });
            ui.add_enabled_ui(clear_nato_f, |ui| {
                if ui.button("Clear NATO").on_hover_text("Clear NATO fighters. Ctrl Z brings them back.").clicked() {
                    self.clear_side_fighters(false);
                }
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
        });
        
        let clear_nato_o = !self.nato_objectives.is_empty();
        let clear_dprk_o = !self.east_objectives.is_empty();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(clear_dprk_o, |ui| {
                if ui.button("Clear DPRK").on_hover_text("Clear DPRK objectives. Ctrl Z brings them back.").clicked() {
                    self.clear_side_objectives(true);
                }
            });
            ui.add_enabled_ui(clear_nato_o, |ui| {
                if ui.button("Clear NATO").on_hover_text("Clear NATO objectives. Ctrl Z brings them back.").clicked() {
                    self.clear_side_objectives(false);
                }
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
        
        let clear_nato_u = self.side_has_units(false);
        let clear_dprk_u = self.side_has_units(true);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(clear_dprk_u, |ui| {
                if ui.button("Clear DPRK").on_hover_text("Clear DPRK ground units, armies and ships. Ctrl Z brings them back.").clicked() {
                    self.clear_side_units(true);
                }
            });
            ui.add_enabled_ui(clear_nato_u, |ui| {
                if ui.button("Clear NATO").on_hover_text("Clear NATO ground units, armies and ships. Ctrl Z brings them back.").clicked() {
                    self.clear_side_units(false);
                }
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
        let locked = if self.ao_locked { "  locked" } else { "" };
        let ao = format!(
            "AO  X {:.0}–{:.0}  Z {:.0}–{:.0}{locked}",
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
        // Lock AO sits in the zoom chip, so the legend stops short of that chip.
        let zoom_w = 470.0;
        let legend_rect = Rect::from_min_max(
            area.left_bottom() + Vec2::new(12.0, -46.0),
            area.left_bottom() + Vec2::new((area.width() - zoom_w - 24.0).max(160.0), -10.0),
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
            area.right_bottom() - Vec2::new(12.0 + zoom_w, 46.0),
            area.right_bottom() - Vec2::new(12.0, 10.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(zoom_rect).layout(Layout::right_to_left(Align::Max)),
            |ui| {
                // Right-to-left: added in reverse so it reads − % + Reset view Lock AO Reset AO.
                chip(ui, &mut |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_enabled_ui(!self.ao_locked, |ui| {
                        if ui
                            .button("Reset AO")
                            .on_hover_text("Reset the AO box to the full map")
                            .on_disabled_hover_text("Unlock AO to reset the box")
                            .clicked()
                        {
                            self.front_aabb = WorldAabb::full_map();
                            self.map_drag_uv = None;
                        }
                    });
                    let lock = if self.ao_locked { "Unlock AO" } else { "Lock AO" };
                    if ui
                        .button(lock)
                        .on_hover_text("While locked, dragging empty map does not redraw the AO box. Load base map can still restore a saved box.")
                        .clicked()
                    {
                        self.ao_locked = !self.ao_locked;
                        if self.ao_locked {
                            self.map_drag_uv = None;
                        }
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

    pub(super) fn ensure_map_assets(&mut self, ctx: &egui::Context) {
        if self.fighter_tex_east.is_none() {
            // Authored SVG: diagonal nose-northwest, red, ring included. No recolor, no turn.
            self.fighter_tex_east = Some(ctx.load_texture(
                "eastern_fighter",
                rasterize_svg(include_bytes!("../../assets/EasternFighter.svg"), 128),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.fighter_tex_nato.is_none() {
            // Authored SVG: diagonal nose-northeast, blue-green, ring included.
            self.fighter_tex_nato = Some(ctx.load_texture(
                "nato_fighter",
                rasterize_svg(include_bytes!("../../assets/NatoFighter.svg"), 128),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.ship_tex_east.is_none() {
            self.ship_tex_east = Some(ctx.load_texture(
                "eastern_shipping",
                load_side_svg(include_bytes!("../../assets/EasternShipping.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.ship_tex_nato.is_none() {
            self.ship_tex_nato = Some(ctx.load_texture(
                "nato_shipping",
                load_side_svg(include_bytes!("../../assets/NatoShiping.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.dir_tex.is_none() {
            self.dir_tex = Some(ctx.load_texture(
                "heading_marker",
                load_fighter_svg(include_bytes!("../../assets/direction.svg")),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.obj_tex_east.is_none() {
            self.obj_tex_east = Some(ctx.load_texture(
                "eastern_objective",
                load_side_svg(include_bytes!("../../assets/EasternObjective.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.obj_tex_nato.is_none() {
            self.obj_tex_nato = Some(ctx.load_texture(
                "nato_objective",
                load_side_svg(include_bytes!("../../assets/NatoObjective.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.armor_tex_east.is_none() {
            self.armor_tex_east = Some(ctx.load_texture(
                "eastern_armor",
                load_side_svg(include_bytes!("../../assets/EasternArmor.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.armor_tex_nato.is_none() {
            self.armor_tex_nato = Some(ctx.load_texture(
                "nato_armor",
                load_side_svg(include_bytes!("../../assets/NatoArmor.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.supply_tex_east.is_none() {
            self.supply_tex_east = Some(ctx.load_texture(
                "eastern_supply",
                load_side_svg(include_bytes!("../../assets/EasternSupply.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.supply_tex_nato.is_none() {
            self.supply_tex_nato = Some(ctx.load_texture(
                "nato_supply",
                load_side_svg(include_bytes!("../../assets/NatoSupply.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.arty_tex_east.is_none() {
            self.arty_tex_east = Some(ctx.load_texture(
                "eastern_arty",
                load_side_svg(include_bytes!("../../assets/EasternArty.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.arty_tex_nato.is_none() {
            self.arty_tex_nato = Some(ctx.load_texture(
                "nato_arty",
                load_side_svg(include_bytes!("../../assets/NatoArty.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.train_tex_east.is_none() {
            self.train_tex_east = Some(ctx.load_texture(
                "eastern_train",
                load_side_svg(include_bytes!("../../assets/EasternTrain.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.train_tex_nato.is_none() {
            self.train_tex_nato = Some(ctx.load_texture(
                "nato_train",
                load_side_svg(include_bytes!("../../assets/NatoTrain.svg"), false),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.infantry_tex_east.is_none() {
            self.infantry_tex_east = Some(ctx.load_texture(
                "eastern_infantry",
                load_side_svg(include_bytes!("../../assets/EasternInfantry.svg"), true),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.infantry_tex_nato.is_none() {
            self.infantry_tex_nato = Some(ctx.load_texture(
                "nato_infantry",
                load_side_svg(include_bytes!("../../assets/NatoInfantry.svg"), false),
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
                    include_bytes!("../../assets/DD052_en_map_01_LowQ.jpg"),
                    "assets/DD052_en_map_01_LowQ.jpg",
                );
                let _ = tx.send(KoreaMapLayer::Overview(lo));
                ctx_clone.request_repaint();
                let hi = load_korea_jpeg(
                    include_bytes!("../../assets/DD052_en_map_01.jpg"),
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
                        } else if !self.ao_locked {
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
                        } else if let Some(origin) = self.map_drag_uv.filter(|_| !self.ao_locked) {
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
        // Paint angle 0: the SVG's diagonal stays (DPRK nose northwest, NATO northeast).
        for spot in &layout.spots {
            let pos = world_to_pos(map_rect, spot.x, spot.z);
            if let Some(tex) = tex {
                paint_rotated_image(painter, tex, pos, size, 0.0, Color32::WHITE);
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

    pub(super) fn configured_fighter_root(&self, country: i32) -> Result<crate::ast::Il2Entity, String> {
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
    pub(super) fn remove_last_mark(&mut self) {
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

    pub(super) fn redo_last_mark(&mut self) {
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

    pub(super) fn generate_front_file(&mut self) {
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

    pub(super) fn load_base_map(&mut self) {
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
}
