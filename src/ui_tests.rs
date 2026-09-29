//! ui_tests.rs — end-to-end UI tests on a headless `egui::Context`
//!
//! A `Harness` runs real frames of `GroupGeneratorApp::ui`, reads the
//! AccessKit tree egui builds each frame to find widgets by label, and
//! clicks, drags and types through synthetic input events. File dialogs are
//! answered from a queue (`dialog::answer`), so Load… / Generate paths run
//! exactly as in the app, writing into a temp folder. Generation logic has
//! its own unit tests in each module; these check that the UI drives it.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use eframe::egui::accesskit::{self as ak, Role};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

use super::builder::{skill_name, FIELD_LABEL_W};
use super::map::{fighter_svg_north, rasterize_svg};
use super::*;
use crate::heightprobe;
use crate::template::{append_seat, OrderKind, TemplateOptions};

/// Test stand-in for `rfd::FileDialog`: every pick pops the next prepared
/// answer. A dialog with no prepared answer is a test bug and panics.
pub mod dialog {
    use super::*;

    thread_local! {
        static ANSWERS: RefCell<VecDeque<Vec<PathBuf>>> = const { RefCell::new(VecDeque::new()) };
    }

    /// Queue the paths the next dialog returns (one path for single picks).
    pub fn answer(paths: Vec<PathBuf>) {
        ANSWERS.with(|a| a.borrow_mut().push_back(paths));
    }

    pub fn pending() -> usize {
        ANSWERS.with(|a| a.borrow().len())
    }

    fn next() -> Vec<PathBuf> {
        ANSWERS.with(|a| a.borrow_mut().pop_front()).expect("a file dialog opened with no prepared answer")
    }

    pub struct FileDialog;

    impl FileDialog {
        pub fn new() -> Self {
            FileDialog
        }
        pub fn add_filter(self, _name: impl Into<String>, _extensions: &[impl ToString]) -> Self {
            self
        }
        pub fn set_file_name(self, _name: impl Into<String>) -> Self {
            self
        }
        pub fn pick_file(self) -> Option<PathBuf> {
            next().into_iter().next()
        }
        pub fn pick_files(self) -> Option<Vec<PathBuf>> {
            Some(next())
        }
        pub fn save_file(self) -> Option<PathBuf> {
            next().into_iter().next()
        }
        pub fn pick_folder(self) -> Option<PathBuf> {
            next().into_iter().next()
        }
    }
}

const SCREEN: Vec2 = Vec2::new(1400.0, 1280.0);
const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

#[derive(Clone, Debug)]
struct Node {
    label: String,
    role: Role,
    rect: Rect,
    disabled: bool,
    toggled: Option<bool>,
}

struct Harness {
    ctx: egui::Context,
    app: GroupGeneratorApp,
    nodes: Vec<Node>,
    time: f64,
    mods: Modifiers,
    pointer: Pos2,
    dir: PathBuf,
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

impl Harness {
    /// A fresh app with every output folder and the height store in a temp dir.
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("il2_ui_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::apply(&ctx);
        let mut app = GroupGeneratorApp::default();
        app.terrain_store_path = dir.join("store").join("korea_100m.hgt");
        app.terrain_use_builtin = false;
        app.harvest_db_dir = dir.join("airfields").display().to_string();
        app.harvest_missions_dir = dir.join("missions").display().to_string();
        app.mark_saved(AppMode::Template);
        app.mark_saved(AppMode::Map);
        let mut h = Harness { ctx, app, nodes: Vec::new(), time: 0.0, mods: Modifiers::NONE, pointer: Pos2::ZERO, dir };
        h.settle();
        h
    }

    fn step(&mut self, events: Vec<Event>) {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN)),
            time: Some(self.time),
            modifiers: self.mods,
            events,
            // A real GPU allows large textures; the high-resolution map needs one.
            max_texture_side: Some(16_384),
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let app = &mut self.app;
        let out = self.ctx.run(raw, |ctx| app.ui(ctx));
        if let Some(update) = out.platform_output.accesskit_update {
            self.nodes = update.nodes.iter().filter_map(|(_, n)| to_node(n)).collect();
        }
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.step(Vec::new());
        }
    }

    fn all(&self, label: &str) -> Vec<Node> {
        self.nodes.iter().filter(|n| n.label == label).cloned().collect()
    }

    fn find(&self, label: &str) -> Node {
        let hits = self.all(label);
        assert!(!hits.is_empty(), "no widget labelled {label:?}. Labels: {:?}", self.labels());
        hits[0].clone()
    }

    fn find_prefix(&self, prefix: &str) -> Node {
        self.nodes
            .iter()
            .find(|n| n.label.starts_with(prefix))
            .cloned()
            .unwrap_or_else(|| panic!("no widget starting {prefix:?}. Labels: {:?}", self.labels()))
    }

    fn has(&self, label: &str) -> bool {
        self.nodes.iter().any(|n| n.label == label)
    }

    fn labels(&self) -> Vec<String> {
        self.nodes.iter().map(|n| n.label.clone()).filter(|l| !l.is_empty()).collect()
    }

    fn move_to(&mut self, pos: Pos2) {
        self.pointer = pos;
        self.step(vec![Event::PointerMoved(pos)]);
    }

    fn click_at(&mut self, pos: Pos2) {
        self.click_at_with(pos, PointerButton::Primary);
    }

    fn click_at_with(&mut self, pos: Pos2, button: PointerButton) {
        self.move_to(pos);
        let mods = self.mods;
        self.step(vec![Event::PointerButton { pos, button, pressed: true, modifiers: mods }]);
        self.step(vec![Event::PointerButton { pos, button, pressed: false, modifiers: mods }]);
        self.settle();
    }

    fn click(&mut self, label: &str) {
        let n = self.find(label);
        assert!(!n.disabled, "{label:?} is disabled");
        self.click_at(n.rect.center());
    }

    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.move_to(from);
        let mods = self.mods;
        self.step(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: mods }]);
        for k in 1..=8 {
            let p = from + (to - from) * (k as f32 / 8.0);
            self.move_to(p);
        }
        self.step(vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: mods }]);
        self.settle();
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        let before = self.mods;
        self.mods = modifiers;
        self.step(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
        self.step(vec![Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers }]);
        self.mods = before;
        self.settle();
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::NONE);
    }

    fn tab(&mut self, label: &str) {
        self.click(label);
    }

    /// Step until the map textures are loaded (they decode on a thread).
    fn wait_for_map(&mut self) {
        for _ in 0..2000 {
            if self.app.map_lo_tex.is_some() {
                self.settle();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.step(Vec::new());
        }
        panic!("map texture never loaded");
    }

    fn status(&self) -> String {
        self.app.status_text()
    }

    fn out(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn to_node(n: &ak::Node) -> Option<Node> {
    let r = n.bounds()?;
    Some(Node {
        // Buttons carry their text as the label; plain labels and links as the value.
        label: n.label().or_else(|| n.value()).unwrap_or_default().to_string(),
        role: n.role(),
        rect: Rect::from_min_max(Pos2::new(r.x0 as f32, r.y0 as f32), Pos2::new(r.x1 as f32, r.y1 as f32)),
        disabled: n.is_disabled(),
        toggled: n.toggled().map(|t| t == ak::Toggled::True),
    })
}

fn assert_group_file(path: &Path) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(text.len() > 100, "{} is nearly empty", path.display());
    crate::parser::parse_group_file(&text).unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
}

// ── Shell ─────────────────────────────────────────────────────────────────

#[test]
fn rail_and_ctrl_numbers_switch_every_tab() {
    let mut h = Harness::new("rail");
    for (mode, label) in MODES {
        h.tab(label);
        assert!(h.app.mode == mode, "rail {label}");
    }
    for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].into_iter().enumerate() {
        h.key_with(key, CTRL);
        assert!(h.app.mode == MODES[i].0, "Ctrl {}", i + 1);
    }
}

#[test]
fn f1_and_rail_help_open_the_help_for_the_tab() {
    let mut h = Harness::new("help");
    h.tab("Exclusive Activation");
    h.key(Key::F1);
    assert!(h.app.help_open);
    assert!(h.app.help_topic == HelpTopic::Exclusive);
    h.app.help_open = false;
    h.tab("Map");
    h.click("Help");
    assert!(h.app.help_open && h.app.help_topic == HelpTopic::Front);
}

#[test]
fn every_tab_has_a_primary_button_with_its_shortcut() {
    let mut h = Harness::new("primary");
    for (mode, label) in MODES {
        h.tab(label);
        let primary = if mode == AppMode::Map { "Generate Base Map" } else { "Generate File" };
        let n = h.find(primary);
        assert!(n.rect.height() >= 28.0, "{label}: primary button {} px tall", n.rect.height());
    }
}

// ── Template ──────────────────────────────────────────────────────────────

/// "+ Add" sits at the right end of each model row.
fn add_model(h: &mut Harness, model: &str) {
    let row = h.find(&format!("Model {model}"));
    h.click_at(Pos2::new(row.rect.right() - 20.0, row.rect.center().y));
}

#[test]
fn template_add_select_remove_undo_reset_and_generate() {
    let mut h = Harness::new("template");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    add_model(&mut h, "F-51D");
    add_model(&mut h, "Il-10");
    assert_eq!(h.app.tpl_seats.len(), 3);
    assert_eq!(h.app.tpl_seats[2].unit.label(), "Il-10");

    // Select card 1: the right panel shows the unit.
    h.click("Unit card 1");
    assert!(matches!(h.app.tpl_select, Some(TplSelect::Seat(0))));
    assert!(h.has("SELECTED · UNIT 1"), "{:?}", h.labels());

    // + Order ▾ → Goto WP adds an order to the selected unit.
    let orders = h.app.tpl_seats[0].orders.len();
    h.click("+ Order ▾");
    h.click("Goto WP");
    assert_eq!(h.app.tpl_seats[0].orders.len(), orders + 1);

    // Remove unit 2 from the inspector, then Ctrl Z brings it back identical.
    let before = h.app.tpl_fingerprint();
    h.click("Unit card 2");
    h.click("Remove unit");
    assert_eq!(h.app.tpl_seats.len(), 2);
    assert!(h.status().is_empty() || !h.status().contains("Undone"));
    assert!(h.has("Undo"), "status bar offers Undo");
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.tpl_seats.len(), 3);
    assert_eq!(h.app.tpl_fingerprint(), before);

    // Reset asks first; Esc cancels; confirming clears; Ctrl Z restores.
    h.click("Reset");
    assert!(h.app.confirm == Some(Confirm::ResetTemplate));
    h.key(Key::Escape);
    assert!(h.app.confirm.is_none());
    assert_eq!(h.app.tpl_seats.len(), 3);
    h.click("Reset");
    let dialog_reset = h.all("Reset").into_iter().max_by(|a, b| a.rect.top().total_cmp(&b.rect.top())).unwrap();
    h.click_at(dialog_reset.rect.center());
    assert!(h.app.tpl_seats.is_empty());
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.tpl_seats.len(), 3);

    // Ctrl G writes the file the Generate button writes.
    let out = h.out("tpl.Group");
    dialog::answer(vec![out.clone()]);
    h.key_with(Key::G, CTRL);
    assert_group_file(&out);
    assert!(!h.app.is_dirty(AppMode::Template), "Generate marks the template saved");
    assert_eq!(dialog::pending(), 0);
}

#[test]
fn template_cards_reorder_by_drag() {
    let mut h = Harness::new("drag");
    h.tab("Template");
    for m in ["F-51D", "Il-10", "B-29 (simple)"] {
        add_model(&mut h, m);
    }
    let c3 = h.find("Unit card 3").rect;
    let c1 = h.find("Unit card 1").rect;
    h.drag(c3.center(), Pos2::new(c1.center().x, c1.top() + 4.0));
    let order: Vec<&str> = h.app.tpl_seats.iter().map(|s| s.unit.label()).collect();
    assert_eq!(order, ["B-29 (simple)", "F-51D", "Il-10"]);
}

#[test]
fn template_load_over_edits_asks_first() {
    let mut h = Harness::new("load");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    h.click("Load…");
    assert!(h.app.confirm == Some(Confirm::LoadTemplate), "unsaved edits ask before Load");
    let file = repo("TemplateExamples/Historical1950/1950_US_F51_ArmedRecon_2plus2.Group");
    dialog::answer(vec![file]);
    let confirm = h.all("Load…").into_iter().max_by(|a, b| a.rect.top().total_cmp(&b.rect.top())).unwrap();
    h.click_at(confirm.rect.center());
    assert_eq!(h.app.tpl_seats.len(), 4, "{}", h.status());
    assert!(!h.app.is_dirty(AppMode::Template));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.tpl_seats.len(), 1, "Ctrl Z undoes the load");
}

#[test]
fn template_preview_shows_the_highlighted_model_before_add() {
    let mut h = Harness::new("tplpreview");
    h.tab("Template");
    assert!(h.app.tpl_seats.is_empty());
    let pick = h.app.displayed_catalog()[h.app.tpl_add_pick].label().to_string();
    assert!(h.has(&pick), "the highlighted model is named in the preview");
    let specs = h.find_prefix("Type: ");
    let formation = h.find("Formation view");
    let models = h.find("MODELS");
    assert!(specs.rect.top() < models.rect.top(), "preview sits above Models");
    assert!(specs.rect.right() < formation.rect.left(), "preview stays in the model panel");
    assert!(formation.rect.width() > 400.0, "the formation view keeps the center");
    assert!(h.has("Placement & Checkzones"), "place and spawn stay reachable before a unit is added");
    assert!(h.has("Activate or Spawn"));
    let script = h.app.displayed_catalog()[h.app.tpl_add_pick].script.clone();
    let plain = super::builder::plain_class_line(crate::model_spec::class_for(&script));
    assert!(h.has(plain), "a plain description of the highlighted model");

    add_model(&mut h, "Il-10");
    let formation = h.find("Formation view");
    let kicker = h.find("SELECTED · UNIT 1");
    assert!(
        (kicker.rect.left() - (formation.rect.left() + 14.0)).abs() < 2.0,
        "unit options use the formation's 14 px left margin"
    );
    let about = h.find("A ground-attack aircraft. It strikes targets on the ground.");
    let models = h.find("MODELS");
    assert!(about.rect.top() < models.rect.top(), "the description stays above Models");
    assert!(about.rect.bottom() < h.find("Unit card 1").rect.top(), "the order of battle does not repeat it");
    assert!(h.has("Change Model"));
    assert!(h.has("Placement & Checkzones"), "selecting a unit does not hide place and spawn");
    assert!(h.has("Activate or Spawn"));
}

#[test]
fn template_empty_view_names_the_first_step_and_adds_the_pick() {
    let mut h = Harness::new("tplempty");
    h.tab("Template");
    assert!(h.has("Pick a model on the left, then + Add to begin."), "{:?}", h.labels());
    let pick = h.app.displayed_catalog()[h.app.tpl_add_pick].label().to_string();
    h.click(&format!("+ Add {pick}"));
    assert_eq!(h.app.tpl_seats.len(), 1);
    assert_eq!(h.app.tpl_seats[0].unit.label(), pick);
    assert!(!h.has("Pick a model on the left, then + Add to begin."));

    // A kind with no models offers to show the list instead.
    h.app.tpl_seats.clear();
    h.app.tpl_catalog.retain(|u| u.kind != CatalogKind::UserAdded);
    h.click("User Added");
    h.click("Show model list");
    assert!(!h.app.tpl_show_models, "the left panel consumed the request");
    assert!(h.app.tpl_seats.is_empty());
}

#[test]
fn template_kind_cells_and_both_filters() {
    let mut h = Harness::new("tplkinds");
    h.tab("Template");
    // Two rows of equal cells: four, then three.
    let row1: Vec<Node> = ["Planes", "Vehicles", "Infantry", "Trains"].iter().map(|l| h.find(l)).collect();
    let row2: Vec<Node> = ["Ships", "Fixed Units", "User Added"].iter().map(|l| h.find(l)).collect();
    for row in [&row1, &row2] {
        for n in row.iter() {
            assert!((n.rect.width() - row[0].rect.width()).abs() < 1.0, "{} is {} px", n.label, n.rect.width());
            assert!((n.rect.top() - row[0].rect.top()).abs() < 0.5 && n.rect.height() >= 28.0);
        }
    }
    assert!(row2[0].rect.top() > row1[0].rect.top());
    h.click("Vehicles");
    assert!(h.app.tpl_kind == CatalogKind::Vehicle);
    h.click("Planes");

    // Planes have both a class and a country filter; the list obeys both.
    let countries = h.app.countries_for_kind(CatalogKind::Plane);
    assert!(countries.len() > 1 && h.app.classes_for_kind(CatalogKind::Plane).len() > 1);
    let panel_right = h.find("Planes").rect.left() + 270.0;
    let mut combos: Vec<Node> =
        h.nodes.iter().filter(|n| n.role == Role::ComboBox && n.rect.left() < panel_right).cloned().collect();
    combos.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
    assert_eq!(combos.len(), 2, "class and country combos");
    h.click_at(combos[1].rect.center());
    let country = countries[countries.len() - 1];
    h.click(&country_short(country));
    assert_eq!(h.app.tpl_country, Some(country));
    let shown = h.app.displayed_catalog();
    assert!(!shown.is_empty() && shown.iter().all(|u| u.country() == country));
    for u in &shown {
        assert!(h.has(&format!("Model {}", u.label())));
    }
    let hidden = h.app.tpl_catalog.iter().find(|u| u.kind == CatalogKind::Plane && u.country() != country).unwrap();
    assert!(!h.has(&format!("Model {}", hidden.label())), "{} is filtered out", hidden.label());
}

#[test]
fn template_card_meta_tree_hint_and_selection_grid() {
    let mut h = Harness::new("tplmeta");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    let seat = &h.app.tpl_seats[0];
    let country = country_short(seat.country);
    let country = country.split_once(' ').map_or(country.as_str(), |(_, n)| n.trim()).to_string();
    let k = seat.orders.len();
    let meta = format!("{country} · {} · {k} {}", skill_name(seat.skill), if k == 1 { "order" } else { "orders" });
    assert!(h.has(&meta), "{meta:?} in {:?}", h.labels());
    assert!(h.has("UNIT → OnSpawned → orders. ‹ › or drag a chip left and right."));
    assert!(h.has("Add to Template"));

    // Nothing selected: the inspector is the template settings, including Zone In.
    h.app.tpl_select = None;
    h.settle();
    assert!(h.has("Placement & Checkzones"), "{:?}", h.labels());
    assert!(h.has(&format!("{:.1} km", h.app.tpl_zone_in / 1000.0)), "{:?}", h.labels());
    assert!(h.app.tpl_zone_in > 1000.0);
    assert!(!h.has("Change Model"));

    // Seat fields: labels in a 96 px column, controls to the right of it.
    h.click("Unit card 1");
    assert!(h.has("Change Model"));
    assert!(h.has("Placement & Checkzones"), "place and spawn stay open while a unit is selected");
    assert!(h.has("Activate or Spawn"));
    let role = h.find("Role").rect;
    let country_label = h.find("Country").rect;
    assert!((role.left() - country_label.left()).abs() < 0.5);
    let role_combo = h
        .nodes
        .iter()
        .find(|n| n.role == Role::ComboBox && (n.rect.center().y - role.center().y).abs() < 8.0)
        .expect("Role combo on the Role row")
        .clone();
    assert!((role_combo.rect.left() - role.left() - FIELD_LABEL_W - 8.0).abs() < 1.0, "control column starts after 96 px");
}

#[test]
fn template_remove_order_selects_its_unit() {
    let mut h = Harness::new("tplrmorder");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    h.click("+ Order ▾");
    h.click("Goto WP");
    h.click("+ Order ▾");
    h.click("AttackArea");
    let before = h.app.tpl_seats[0].orders.len();
    let gi = h.app.tpl_seats[0].orders.iter().position(|o| o.kind == OrderKind::GotoWaypoint).unwrap();
    assert!(gi + 1 < before, "an order follows Goto WP");
    h.click_at(h.find(&format!("{} Goto WP", gi + 1)).rect.center());
    h.click("Remove order");
    assert_eq!(h.app.tpl_seats[0].orders.len(), before - 1);
    assert!(matches!(h.app.tpl_select, Some(TplSelect::Seat(0))), "the unit is selected, not a neighbouring order");
    assert!(h.has("SELECTED · UNIT 1"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.tpl_seats[0].orders.len(), before);
}

#[test]
fn template_copy_attributes_needs_a_unit_and_undoes() {
    let mut h = Harness::new("tplcopy");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    add_model(&mut h, "Il-10");
    h.app.tpl_select = None;
    h.settle();
    assert!(!h.has("Copy attributes to all"), "copy belongs to the selected unit");
    assert!(h.has("Placement & Checkzones"));

    h.app.tpl_seats[0].skill = 4;
    h.app.tpl_seats[1].skill = 1;
    h.click("Unit card 1");
    h.click("Copy attributes to all");
    assert_eq!(h.app.tpl_seats[1].skill, 4);
    assert_eq!(h.app.undo_label(), Some("Copied attributes from F-51D"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.tpl_seats[1].skill, 1);
}

#[test]
fn template_unit_menu_duplicates_and_deletes() {
    let mut h = Harness::new("tplmenu");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    h.click_at_with(h.find("Unit card 1").rect.center(), PointerButton::Secondary);
    h.click("Duplicate");
    assert_eq!(h.app.tpl_seats.len(), 2);
    assert_eq!(h.app.tpl_seats[1].unit.label(), "F-51D");
    h.click_at_with(h.find("Unit card 2").rect.center(), PointerButton::Secondary);
    h.click("Delete");
    assert_eq!(h.app.tpl_seats.len(), 1);
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.tpl_seats.len(), 2);
}

// ── Army Generator ────────────────────────────────────────────────────────

/// A Template Builder vehicle group: the input the Army Generator expects.
fn write_ground_template(path: &Path, name: &str) {
    let unit = crate::template::bundled_catalog()
        .into_iter()
        .find(|u| u.kind == CatalogKind::Vehicle)
        .expect("a catalog vehicle");
    let mut seats = Vec::new();
    for _ in 0..3 {
        append_seat(&mut seats, unit.clone(), 4);
    }
    let opts = TemplateOptions {
        name: name.into(),
        seats,
        zone_in: 10_000.0,
        zone_out: 19_000.0,
        ..TemplateOptions::default()
    };
    let root = crate::template::generate_template(&opts).expect("generate ground template");
    std::fs::write(path, serialize_group(&root)).unwrap();
}

#[test]
fn army_add_retype_remove_undo_and_generate() {
    let mut h = Harness::new("army");
    h.tab("Army Generator");
    let (a, b) = (h.out("armor_a.Group"), h.out("armor_b.Group"));
    write_ground_template(&a, "Armor A");
    write_ground_template(&b, "Armor B");
    dialog::answer(vec![a, b]);
    h.click("Add templates…");
    assert_eq!(h.app.recon_slots.len(), 2, "{}", h.status());
    assert!(h.has("COPY MIX"), "center shows the copy mix");

    // Type buttons on card 1 (left panel; the right panel has the import picker).
    let supply = h
        .all("Supply")
        .into_iter()
        .filter(|n| n.role == Role::Button)
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()).then(a.rect.top().total_cmp(&b.rect.top())))
        .expect("Supply type button");
    h.click_at(supply.rect.center());
    assert!(h.app.recon_slots[0].kind == UnitKind::Supply);

    let remove = h.all("Remove")[0].clone();
    h.click_at(remove.rect.center());
    assert_eq!(h.app.recon_slots.len(), 1);
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.recon_slots.len(), 2);

    let out = h.out("army.Group");
    dialog::answer(vec![out.clone()]);
    h.click("Generate File");
    assert_group_file(&out);
}

#[test]
fn army_rework_detects_copies() {
    let mut h = Harness::new("rework");
    h.tab("Army Generator");
    h.click("Rework existing");
    assert!(h.app.recon_submode == ReconSubmode::Rework);
    dialog::answer(vec![repo("TemplateExamples/Random_Ground_Units_22of72.Group")]);
    h.click("Add packs…");
    assert!(!h.app.recon_rework.is_empty(), "{}", h.status());
    let detected: usize = h.app.recon_rework.iter().filter_map(|s| s.detected).sum();
    assert_eq!(detected, 72);
    assert!(h.has("COPY MIX") && h.has("On map"));
    // The same randomizer label in both submodes.
    h.click("Spawn all copies (no randomizer)");
    assert!(h.app.recon_strip_randomizer);
    h.click("Spawn all copies (no randomizer)");
    let out = h.out("rework.Group");
    dialog::answer(vec![out.clone()]);
    h.key_with(Key::G, CTRL);
    assert_group_file(&out);
}

#[test]
fn army_cards_mix_block_import_row_and_settings() {
    let mut h = Harness::new("armylayout");
    h.tab("Army Generator");
    let (a, b) = (h.out("armor_a.Group"), h.out("armor_b.Group"));
    write_ground_template(&a, "Armor A");
    write_ground_template(&b, "Armor B");
    dialog::answer(vec![a, b]);
    h.click("Add templates…");

    // Type buttons are 36 × 28; the import picker is one row of six.
    let kinds = ["Ship", "Armor", "Supply", "Artillery", "Infantry", "Train"];
    for label in kinds {
        let buttons: Vec<Node> = h.all(label).into_iter().filter(|n| n.role == Role::Button).collect();
        assert_eq!(buttons.len(), 3, "{label}: two cards and the import picker");
        for n in &buttons {
            assert!((n.rect.width() - 36.0).abs() < 0.5 && (n.rect.height() - 28.0).abs() < 0.5, "{label} is {:?}", n.rect.size());
        }
    }
    let picker = |h: &Harness, label: &str| {
        h.all(label).into_iter().filter(|n| n.role == Role::Button).max_by(|a, b| a.rect.left().total_cmp(&b.rect.left())).unwrap()
    };
    let top = picker(&h, "Ship").rect.top();
    assert!(kinds.iter().all(|k| (picker(&h, k).rect.top() - top).abs() < 0.5), "one row");
    h.click_at(picker(&h, "Train").rect.center());
    assert!(h.app.recon_import_kind == UnitKind::Train);

    // Card meta line: file and counts, zero counts left out.
    let info = &h.app.recon_slots[0].info;
    assert_eq!(info.ship_count + info.train_count + info.block_count, 0);
    assert!(h.has(&format!("armor_a.Group · {} vehicles", info.vehicle_count)), "{:?}", h.labels());
    assert!(h.has("Zone In checkzones"));
    assert!(!h.labels().iter().any(|l| l.contains("confirm the group is valid")));

    // COPY MIX is its own block below the parking grid.
    let grid = h.find_prefix("PARKING GRID").rect;
    let mix = h.find("COPY MIX").rect;
    assert!(mix.top() > grid.bottom() + 40.0, "COPY MIX sits at the bottom");

    // Activate ratio shows a percent value.
    assert!(h.has("Activate ratio"));
    assert!(h.has(&format!("{}%", h.app.recon_percent)), "{:?}", h.labels());

    // Timing stays visible with spawn-all on; its sliders are disabled.
    h.click("Timing");
    h.click("Spawn all copies (no randomizer)");
    assert!(h.app.recon_strip_randomizer);
    assert!(h.has(&format!("Start {} s · {} ms apart", h.app.recon_start_delay_s, h.app.recon_group_delay_ms)));
    assert!(h.has("Timing applies only with the randomizer."));
    let timing = h.find("Timing").rect;
    let sliders: Vec<Node> = h
        .nodes
        .iter()
        .filter(|n| n.role == Role::Slider && n.rect.top() > timing.top() && n.rect.left() >= timing.left() - 20.0)
        .cloned()
        .collect();
    assert_eq!(sliders.len(), 2, "start delay and group delay");
    assert!(sliders.iter().all(|n| n.disabled));
}

#[test]
fn recon_counts_line_omits_zero_counts() {
    let info = |vehicle_count, ship_count, train_count, block_count| UnitPlanInfo {
        name: String::new(),
        vehicle_count,
        ship_count,
        train_count,
        block_count,
        checkzones: Vec::new(),
        suggested_triggers: Vec::new(),
        restore_starts: Vec::new(),
        weapon_range_m: None,
        route: None,
        wp_ahead: Vec::new(),
    };
    assert_eq!(recon_counts_line(&info(6, 0, 0, 0)), "6 vehicles");
    assert_eq!(recon_counts_line(&info(3, 0, 1, 2)), "2 vehicles, 1 train, 2 blocks");
    assert_eq!(recon_counts_line(&info(1, 0, 1, 0)), "1 train");
    assert_eq!(recon_counts_line(&info(2, 2, 0, 1)), "2 ships, 1 block");
    assert_eq!(recon_counts_line(&info(0, 0, 0, 0)), "no units");
}

// ── Fighter Pack ──────────────────────────────────────────────────────────

#[test]
fn fighter_types_reset_confirm_and_generate() {
    let mut h = Harness::new("fighter");
    h.tab("Fighter Pack");
    let yak = AIRCRAFT_TYPES.iter().position(|a| a.label == "Yak-9P").unwrap();
    assert!(!h.app.type_enabled[yak]);
    h.click("Yak-9P");
    assert!(h.app.type_enabled[yak]);
    assert!(h.has("3 of Group 1 flights") || h.has("GROUP 1 · FLIGHTS"), "preview renders");

    h.click("Reset");
    assert!(h.app.confirm == Some(Confirm::ResetFighter));
    let dialog_reset = h.all("Reset").into_iter().max_by(|a, b| a.rect.top().total_cmp(&b.rect.top())).unwrap();
    h.click_at(dialog_reset.rect.center());
    assert!(!h.app.type_enabled[yak], "Reset restores the default types");

    let out = h.out("fighter.Group");
    dialog::answer(vec![out.clone()]);
    h.click("Generate File");
    assert_group_file(&out);
}

#[test]
fn fighter_preview_sentence_table_and_empty_state() {
    let mut h = Harness::new("fighter_preview");
    h.tab("Fighter Pack");
    h.app.linked_groups = 3;
    h.app.flight_count = 3;
    h.app.max_in_flight = 4;
    h.settle();
    assert!(
        h.has("Group logic is built in. 3 linked groups, chained through NodeGates, parked on a 10 km grid from 40000, 40000."),
        "{:?}",
        h.labels()
    );
    // Flights 4/3/2: colour + number, roles counted from the preview seats.
    for name in ["Red 11", "Blue 21", "Yellow 31"] {
        assert!(h.has(name), "flight {name}: {:?}", h.labels());
    }
    assert!(h.has("2 AttackArea · 2 Cover"));
    assert!(h.has("2 AttackArea · 1 Cover"));
    assert!(h.has("AttackArea + Cover"));
    // The 4-ship shows its low and high elements, the 2-ship one altitude.
    let flights = crate::flights::preview_flights(&h.app.fighter_flight_config());
    let (low, high) = GroupGeneratorApp::flight_element_altitudes(&flights[0].seats);
    let high = high.expect("a complete 4-ship has a high pair");
    assert!(h.has(&format!("{} / {} m", group_digits(low), group_digits(high))));
    assert_eq!(GroupGeneratorApp::flight_element_altitudes(&flights[2].seats).1, None);
    assert!(h.has("Group 3 · 9 aircraft"));

    h.app.linked_groups = 1;
    h.app.custom_path = Some(PathBuf::from("My_Pack.Group"));
    h.settle();
    assert!(h.has("Group logic from My_Pack.Group. One group, parked at 40000, 40000."), "{:?}", h.labels());
    h.app.custom_path = None;

    // No types: an empty state instead of the fallback type, and Generate is off.
    h.app.type_enabled.iter_mut().for_each(|e| *e = false);
    h.settle();
    assert!(h.has("Select at least one aircraft type to preview the pack."));
    assert!(!h.has("GROUP 1 · FLIGHTS"));
    assert!(!h.has("Red 11"));
    assert!(h.find("Generate File").disabled);
}

#[test]
fn fighter_flight_names_stay_unique_up_to_ten_flights() {
    let names: Vec<String> = (0..10).map(|f| crate::aircraft::plane_display_name(f, 0)).collect();
    let unique: std::collections::HashSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "{names:?}");
    assert_eq!(names[0], "Red 11");
    assert_eq!(names[6], "Red 71", "the colour list wraps after six");
}

#[test]
fn fighter_role_and_altitude_text() {
    assert_eq!(GroupGeneratorApp::flight_role_text(&[(1.0, true)]), "AttackArea");
    assert_eq!(GroupGeneratorApp::flight_role_text(&[(1.0, true), (1.0, false)]), "AttackArea + Cover");
    assert_eq!(
        GroupGeneratorApp::flight_role_text(&[(1.0, true), (1.0, false), (3.0, true), (3.0, false)]),
        "2 AttackArea · 2 Cover"
    );
    assert_eq!(GroupGeneratorApp::altitude_text(1800.0, Some(3800.0)), "1 800 / 3 800 m");
    assert_eq!(GroupGeneratorApp::altitude_text(5200.0, None), "5 200 m");
    assert_eq!(group_digits(227_880.4), "227 880");
    assert_eq!(group_digits(950.0), "950");
    assert_eq!(group_digits(-1234.0), "-1 234");
}

#[test]
fn fighter_nodegate_links_never_end_a_row() {
    for groups in 1..=10 {
        let mut w = 130.0;
        while w < 1400.0 {
            let rows = GroupGeneratorApp::pack_card_rows(groups, w);
            let mut next = 0;
            for (r, row) in rows.iter().enumerate() {
                assert_eq!(row.start, next, "rows are contiguous");
                assert!(!row.is_empty());
                next = row.end;
                // A row is its cards plus the link in front of each card after group 1,
                // so every link sits directly before a card on the same row.
                let n = row.len() as f32;
                let links = if r == 0 { n - 1.0 } else { n };
                let row_w = n * PACK_CARD_W + links * PACK_LINK_W;
                assert!(row_w <= w || row.len() == 1, "{groups} groups at {w} px: row {r} is {row_w} px");
            }
            assert_eq!(next, groups);
            w += 37.0;
        }
    }
    // Ten groups render as cards, each inside the window.
    let mut h = Harness::new("fighter_cards");
    h.tab("Fighter Pack");
    h.app.linked_groups = 10;
    h.settle();
    for g in 1..=10 {
        let card = h.find_prefix(&format!("Group {g} · "));
        assert!(card.rect.right() <= SCREEN.x, "Group {g} card off screen: {:?}", card.rect);
    }
}

#[test]
fn fighter_altitude_strip_skips_crowded_labels_but_every_tick_hovers() {
    // Skipping: a label that would touch the last drawn one is left out.
    let shown = strip_labels_shown(&[0.0, 20.0, 45.0, 100.0], &[40.0, 40.0, 40.0, 40.0], 6.0);
    assert_eq!(shown, vec![true, false, false, true]);
    let shown = strip_labels_shown(&[100.0, 0.0], &[40.0, 40.0], 6.0);
    assert_eq!(shown, vec![true, true], "order follows position, not flight number");

    let mut h = Harness::new("fighter_strip");
    h.tab("Fighter Pack");
    h.app.flight_count = 10;
    h.settle();
    let ticks: Vec<Node> = h.nodes.iter().filter(|n| n.label.starts_with("Altitude tick ")).cloned().collect();
    assert_eq!(ticks.len(), 10, "one tick per flight: {:?}", h.labels());
    // Hovering a tick names its flight (whether or not its label was drawn).
    let tick = h.find("Altitude tick Green 11");
    h.move_to(tick.rect.center());
    for _ in 0..40 {
        h.step(Vec::new());
    }
    assert!(
        h.nodes.iter().any(|n| n.label.starts_with("Green 11 · ")),
        "hover names the flight: {:?}",
        h.labels()
    );
}

// ── Exclusive Activation ──────────────────────────────────────────────────

#[test]
fn exclusive_plans_select_remove_undo_and_generate() {
    let mut h = Harness::new("exclusive");
    h.tab("Exclusive Activation");
    dialog::answer(vec![repo("TemplateExamples/Exclusive_Activation_6plan.Group")]);
    h.click("Add templates…");
    assert_eq!(h.app.bomber_slots.len(), 6, "{}", h.status());
    h.click("Plan card 3");
    assert_eq!(h.app.bomber_selected, Some(2));
    h.click("Sequence 5");
    assert_eq!(h.app.bomber_selected, Some(4));
    h.click("Remove");
    assert_eq!(h.app.bomber_slots.len(), 5);
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.bomber_slots.len(), 6);
    let out = h.out("exclusive.Group");
    dialog::answer(vec![out.clone()]);
    h.key_with(Key::G, CTRL);
    assert_group_file(&out);
}

#[test]
fn exclusive_header_tags_kicker_and_idle_problem() {
    let mut h = Harness::new("exclusive_tags");
    h.tab("Exclusive Activation");
    assert!(!h.labels().iter().any(|l| l.starts_with("Editing")), "no pack loaded yet");
    dialog::answer(vec![repo("TemplateExamples/Exclusive_Activation_6plan.Group")]);
    h.click("Add templates…");
    assert!(h.has("Editing Exclusive_Activation_6plan"), "{:?}", h.labels());
    assert!(h.has("Plan 1"), "kicker in sentence case");
    assert!(!h.has("PLAN 1"));

    // Plan 2 loses its end timer, then its checkzones: tags and the idle status follow.
    h.app.bomber_slots[1].selected_completion = None;
    h.app.status = Status::Idle;
    h.settle();
    assert_eq!(plan_tags(&h.app.bomber_slots[1]), vec![("⚠ End timer", false)]);
    assert!(h.has("Plan 2 has no end timer"), "{:?}", h.labels());
    h.app.bomber_slots[1].selected_triggers.clear();
    h.settle();
    assert_eq!(plan_tags(&h.app.bomber_slots[1]), vec![("⚠ Checkzone", false), ("⚠ End timer", false)]);
    assert!(h.has("Plan 2 has no start checkzone"));
    assert!(h.find("Generate File").disabled);

    // A plan with a wiring warning is not Ready.
    let mut warned = h.app.bomber_slots[0].clone();
    let zone = warned.selected_triggers[0];
    warned.info.trigger_warnings.insert(zone, "zone is not Closer".into());
    assert_eq!(plan_tags(&warned), vec![("⚠ Check", false)]);

    // A long name truncates and leaves room for the tag.
    let long = "Very long plan name ".repeat(12);
    h.app.bomber_slots[0].info.name = long.clone();
    h.settle();
    let card = h.find("Plan card 1");
    let title = h.find(&format!("1 · {long}"));
    assert!(title.rect.right() + 30.0 < card.rect.right(), "title {:?} leaves no room in {:?}", title.rect, card.rect);
}

#[test]
fn exclusive_editing_clears_on_generate_and_when_emptied() {
    let mut h = Harness::new("exclusive_editing");
    h.tab("Exclusive Activation");
    dialog::answer(vec![repo("TemplateExamples/Exclusive_Activation_6plan.Group")]);
    h.click("Add templates…");
    assert!(h.has("Editing Exclusive_Activation_6plan"));
    let out = h.out("exclusive.Group");
    dialog::answer(vec![out.clone()]);
    h.click("Generate File");
    assert_group_file(&out);
    assert!(h.app.bomber_loaded_path.is_none());
    assert!(!h.has("Editing Exclusive_Activation_6plan"));

    dialog::answer(vec![repo("TemplateExamples/Exclusive_Activation_6plan.Group")]);
    h.click("Add templates…");
    h.app.bomber_slots.retain(|s| s.path != repo("TemplateExamples/Exclusive_Activation_6plan.Group"));
    h.settle();
    assert!(!h.labels().iter().any(|l| l.starts_with("Editing")), "no plans from the pack, no Editing");
}

// ── Airfield ──────────────────────────────────────────────────────────────

#[test]
fn airfield_load_side_and_generate() {
    let mut h = Harness::new("airfield");
    h.tab("Airfield");
    assert!(h.find("Generate File").disabled, "Generate waits for a file");
    dialog::answer(vec![repo("TemplateExamples/Airfield_Mess.Group")]);
    h.click("Load airfield…");
    assert!(h.app.airfield_info.is_some(), "{}", h.status());
    assert!(h.has("KEPT"));
    h.click("DPRK [1]");
    assert!(!h.app.airfield_western);
    assert!(h.has("RELINKED TO DPRK [1]"));
    let out = h.out("airfield.Group");
    dialog::answer(vec![out.clone()]);
    h.click("Generate File");
    assert_group_file(&out);
}

#[test]
fn airfield_harvest_file_and_watch() {
    let mut h = Harness::new("harvest");
    h.tab("Airfield");
    // Harvest a file… (the K13 package stands in for a _gen.mission).
    dialog::answer(vec![repo("TemplateExamples/K13 AFB_mp.Group")]);
    h.click("Harvest a file…");
    let db = PathBuf::from(&h.app.harvest_db_dir);
    assert!(db.join("catalog.Group").is_file(), "{}", h.status());
    assert!(h.has("DATABASE HARVEST"), "the log shows in the center");
    let logged = h.app.harvest_log.len();

    // Watch: a new _gen.mission in the Missions folder is harvested by itself.
    let missions = PathBuf::from(&h.app.harvest_missions_dir);
    std::fs::create_dir_all(&missions).unwrap();
    h.click("Watch for new airfields");
    assert!(h.app.harvest_watcher.is_some(), "{}", h.status());
    std::fs::copy(repo("TemplateExamples/K13 AFB_mp.Group"), missions.join("_gen.mission")).unwrap();
    for _ in 0..400 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.step(Vec::new());
        if h.app.harvest_log.len() > logged {
            break;
        }
    }
    assert!(h.app.harvest_log.len() > logged, "watcher harvested the new file: {}", h.status());
    h.click("Watch for new airfields");
    assert!(h.app.harvest_watcher.is_none());
}

#[test]
fn airfield_panels_order_and_wording() {
    let mut h = Harness::new("airfield_panels");
    h.tab("Airfield");
    let get = h.find("GET THE FILE").rect.top();
    let side = h.find("FRIENDLY PLANE COALITION").rect.top();
    let harvest = h.find("HARVEST AUTOMATICALLY").rect.top();
    assert!(get < side && side < harvest, "Get the file, coalition, then the harvester");
    assert!(h.has("Load a Freeflight airfield from _gen.mission, then generate the cleaned group."));

    dialog::answer(vec![repo("TemplateExamples/Airfield_Mess.Group")]);
    h.click("Load airfield…");
    let info = h.app.airfield_info.clone().expect("airfield loaded");
    assert!(h.has("Strips the player and SP logic, then retargets the checkzones that were linked to the player."));
    // Removed: country name only.
    for p in &info.player_planes {
        assert!(h.has(&country_name(p.country)), "{:?}", h.labels());
    }
    assert!(!h.labels().iter().any(|l| COUNTRIES.iter().any(|(_, c)| l.contains(c))), "no raw country labels");
    // Relinked: a short count beside the side.
    let n = info.unlink_zones.len();
    assert!(h.has(&format!("{n} checkzone{}", if n == 1 { "" } else { "s" })), "{:?}", h.labels());
    // Right panel: sentence case layout, grouped origin.
    assert!(h.has(if info.in_group { "Inside a Group" } else { "Blocks at the root" }));
    if let Some((x, z)) = info.origin_xz {
        assert!(h.has(&format!("{}, {}", group_digits(x), group_digits(z))));
    }
    assert_eq!(country_name(601), "USA");
    assert_eq!(country_name(999), "country 999");
}

// ── Per-tab status and saved state ────────────────────────────────────────

#[test]
fn each_tab_keeps_its_own_status() {
    let mut h = Harness::new("tab_status");
    h.tab("Fighter Pack");
    h.app.status = Status::Info("Fighter message".into());
    h.tab("Airfield");
    assert_ne!(h.status(), "Fighter message", "a message stays on its tab");
    h.app.status = Status::Error("Airfield message".into());
    h.key_with(Key::Num3, CTRL);
    assert_eq!(h.status(), "Fighter message");
    h.tab("Airfield");
    assert_eq!(h.status(), "Airfield message");
}

#[test]
fn a_second_identical_generate_marks_the_tab_saved() {
    let mut h = Harness::new("saved");
    // The empty test store would warn about unmeasured ground, and a warning
    // stays on the tab. This check is about the plain "Wrote …" line.
    h.app.terrain_apply = false;
    h.tab("Template");
    add_model(&mut h, "F-51D");
    let out = h.out("tpl.Group");
    dialog::answer(vec![out.clone()]);
    h.click("Generate File");
    assert_group_file(&out);
    assert!(!h.app.is_dirty(AppMode::Template));
    let first = h.status();

    // An edit that does not change the Generate message.
    h.app.tpl_zone_in += 500.0;
    assert!(h.app.is_dirty(AppMode::Template));
    dialog::answer(vec![out.clone()]);
    h.click("Generate File");
    assert_eq!(h.status(), first, "same message as the first Generate");
    assert!(!h.app.is_dirty(AppMode::Template), "the second Generate counts as saved");

    // An edit retires the "Wrote …" message (age_status); a cancelled dialog
    // then reports nothing: the status stays and the edits stay unsaved.
    h.app.tpl_zone_in += 500.0;
    h.settle();
    assert_eq!(h.status(), "", "the Generate message is gone after an edit");
    dialog::answer(Vec::new());
    h.click("Generate File");
    assert_eq!(h.status(), "");
    assert!(h.app.is_dirty(AppMode::Template));
}

/// Info messages give way to the idle hint once the tab is edited; warnings stay.
#[test]
fn status_messages_retire_after_the_next_edit() {
    let mut h = Harness::new("agestatus");
    h.tab("Fighter Pack");
    h.app.status = Status::Info("Fighter Pack reset to the default settings.".into());
    h.settle();
    assert!(!h.status().is_empty(), "a fresh message stays while nothing changes");
    h.app.flight_count += 1;
    h.settle();
    assert_eq!(h.status(), "", "an edit retires it");
    h.app.status = Status::Warn { lead: "Placed with notes: ".into(), items: vec!["one".into()] };
    h.settle();
    h.app.flight_count -= 1;
    h.settle();
    assert!(h.status().starts_with("Placed with notes"), "warnings stay until replaced");
}

/// Undoing a Remove on Exclusive Activation selects the plan that came back.
#[test]
fn exclusive_undo_reselects_the_removed_plan() {
    let mut h = Harness::new("exundo");
    h.tab("Exclusive Activation");
    dialog::answer(vec![repo("TemplateExamples/Exclusive_Activation_6plan.Group")]);
    h.key_with(Key::O, CTRL);
    h.settle();
    assert_eq!(h.app.bomber_slots.len(), 6);
    h.app.bomber_selected = Some(5);
    h.settle();
    h.click("Remove");
    assert_eq!(h.app.bomber_slots.len(), 5);
    assert_eq!(h.app.bomber_selected, Some(4), "the neighbour is selected after Remove");
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.bomber_slots.len(), 6);
    assert_eq!(h.app.bomber_selected, Some(5), "Undo selects the restored plan");
}

// ── Map ───────────────────────────────────────────────────────────────────

#[test]
fn map_tools_keys_escape_one_shot_objectives_and_tab_reset() {
    let mut h = Harness::new("maptools");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].into_iter().enumerate() {
        h.key(key);
        assert!(h.app.map_drawing_mode == MAP_TOOLS[i].0, "key {}", i + 1);
    }
    // Esc drops a half-drawn salient and returns to Select.
    h.key(Key::Num3);
    h.click_at(map.center());
    h.click_at(map.center() + Vec2::new(30.0, 10.0));
    assert!(!h.app.current_salient.is_empty(), "salient in progress");
    assert!(h.status().is_empty() || h.has("SALIENT"), "banner shows");
    h.key(Key::Escape);
    assert!(h.app.current_salient.is_empty());
    assert!(h.app.map_drawing_mode == MapDrawingMode::None);

    // An objective tool places one, then returns to Select; Shift keeps it.
    h.key(Key::Num5);
    h.click_at(map.center());
    assert_eq!(h.app.east_objectives.len(), 1);
    assert!(h.app.map_drawing_mode == MapDrawingMode::None);
    h.key(Key::Num6);
    h.mods = Modifiers::SHIFT;
    h.click_at(map.center() + Vec2::new(40.0, 0.0));
    h.click_at(map.center() + Vec2::new(60.0, 0.0));
    h.mods = Modifiers::NONE;
    assert_eq!(h.app.nato_objectives.len(), 2);
    assert!(h.app.map_drawing_mode == MapDrawingMode::PlaceNatoObjective);

    // Leaving the tab puts the tool down.
    h.tab("Template");
    h.tab("Map");
    assert!(h.app.map_drawing_mode == MapDrawingMode::None);
}

#[test]
fn map_arrow_undo_redo_and_clear_undo() {
    let mut h = Harness::new("mapundo");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    h.key(Key::Num4);
    h.drag(map.center(), map.center() + Vec2::new(120.0, -60.0));
    assert_eq!(h.app.attack_arrows.len(), 1, "drag draws an attack arrow");
    h.key_with(Key::Z, CTRL);
    assert!(h.app.attack_arrows.is_empty());
    h.key_with(Key::Y, CTRL);
    assert_eq!(h.app.attack_arrows.len(), 1);

    // Clear objectives from the Forces dock, then Ctrl Z restores them.
    h.key(Key::Num5);
    h.click_at(map.center());
    h.click("Forces");
    let clear = h.all("Clear DPRK").into_iter().find(|n| !n.disabled && n.rect.top() > h.find("OBJECTIVES").rect.top()).expect("objectives Clear DPRK");
    h.click_at(clear.rect.center());
    assert!(h.app.east_objectives.is_empty());
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.east_objectives.len(), 1);
}

#[test]
fn map_generate_and_load_base_map_round_trip() {
    let mut h = Harness::new("basemap");
    h.tab("Map");
    h.wait_for_map();
    let out = h.out("base.Group");
    dialog::answer(vec![out.clone()]);
    h.key_with(Key::G, CTRL);
    assert_group_file(&out);
    assert!(!h.app.is_dirty(AppMode::Map));
    // Nothing changed since: Load does not ask.
    dialog::answer(vec![out]);
    h.click("Load base map…");
    assert!(h.app.confirm.is_none(), "{}", h.status());
    assert_eq!(dialog::pending(), 0);
}

#[test]
fn map_terrain_import_layers_and_export() {
    let mut h = Harness::new("terrain");
    h.tab("Map");
    h.wait_for_map();
    h.click("Terrain");
    assert!(h.has_prefix("0 of "), "empty store: {:?}", h.labels());

    // A snapped probe file: nine land nodes of one tile at 100–108 m.
    let nodes: Vec<(usize, usize)> = heightprobe::tile_probe_nodes(10, 10).into_iter().take(9).collect();
    let mut group = heightprobe::probe_group("snapped", &nodes);
    for (k, child) in group.children.iter_mut().enumerate() {
        child.set_property("YPos", format!("{:.3}", 100.0 + k as f64));
    }
    let snapped = h.out("snapped.Group");
    std::fs::write(&snapped, serialize_group(&group)).unwrap();
    dialog::answer(vec![snapped]);
    h.click("Import snapped…");
    assert_eq!(h.app.terrain_store.as_ref().unwrap().measured_nodes(), 9, "{}", h.status());
    assert!(h.app.terrain_store_path.is_file(), "the store is saved");
    let (i, j) = nodes[4];
    let y = h.app.terrain_store.as_ref().unwrap().node(i, j).unwrap();
    assert!((y - 104.0).abs() < 0.05);

    // An unsnapped file (all probes at 0 m) is refused.
    let raw = h.out("unsnapped.Group");
    std::fs::write(&raw, serialize_group(&heightprobe::tile_probe_group(11, 4).expect("tile 11,4 has probes"))).unwrap();
    dialog::answer(vec![raw]);
    h.click("Import snapped…");
    assert!(h.status().contains("not merged"), "{}", h.status());

    h.click("Relief");
    assert!(h.app.terrain_relief.is_some(), "relief texture built");
    h.click("Coverage");
    assert!(h.app.terrain_show_coverage);
    assert_eq!(h.find("Coverage").toggled, Some(true), "the checkbox reports checked");

    // Export the tiles under a small AO in the middle of tile 10_10.
    let (i_lo, i_hi, j_lo, j_hi) = crate::terrain::tile_node_range(10, 10);
    let (x, z) = ((i_lo + i_hi) as f64 * 50.0, (j_lo + j_hi) as f64 * 50.0);
    h.app.front_aabb = WorldAabb::from_corners(x - 500.0, z - 500.0, x + 500.0, z + 500.0);
    h.settle();
    let tiles = h.out("tiles");
    std::fs::create_dir_all(&tiles).unwrap();
    dialog::answer(vec![tiles.clone()]);
    let button = h.find_prefix("Export AO tiles (");
    h.click_at(button.rect.center());
    let written: Vec<_> = std::fs::read_dir(&tiles).unwrap().collect();
    assert_eq!(written.len(), 1, "{}", h.status());
    assert!(tiles.join(format!("{}.Group", heightprobe::tile_name(10, 10))).is_file());
}

#[test]
fn map_dock_tabs_all_render() {
    let mut h = Harness::new("dock");
    h.tab("Map");
    for (tab, marker) in [("Forces", "FIGHTERS"), ("References 0", "Add reference groups…"), ("Terrain", "HEIGHT STORE"), ("Period", "DRAWN MARKS")] {
        h.click(tab);
        assert!(h.has(marker), "{tab}: {:?}", h.labels());
    }
}

// ── Sizes (README §6.6) ───────────────────────────────────────────────────

#[test]
fn interactive_widgets_are_at_least_28_px_tall() {
    let mut h = Harness::new("sizes");
    let mut small = Vec::new();
    let mut checked = 0;
    for (_, label) in MODES {
        h.tab(label);
        for n in &h.nodes {
            let interactive = matches!(n.role, Role::Button | Role::CheckBox | Role::ComboBox | Role::Slider | Role::SpinButton);
            checked += usize::from(interactive);
            if interactive && !n.disabled && n.rect.height() < 27.5 && n.rect.width() > 0.0 {
                small.push(format!("{label}: {:?} {:?} {}px", n.label, n.role, n.rect.height()));
            }
        }
    }
    assert!(checked > 150, "only {checked} interactive widgets seen: the role filter is wrong");
    small.sort();
    small.dedup();
    assert!(small.is_empty(), "targets under 28 px:\n{}", small.join("\n"));
}

impl Harness {
    fn has_prefix(&self, prefix: &str) -> bool {
        self.nodes.iter().any(|n| n.label.starts_with(prefix))
    }
}

#[test]
fn template_tree_chip_selects_and_moves() {
    let mut h = Harness::new("tree");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    h.click("+ Order ▾");
    h.click("Goto WP");
    h.click("+ Order ▾");
    h.click("AttackArea");
    let kinds = |h: &Harness| h.app.tpl_seats[0].orders.iter().map(|o| o.kind).collect::<Vec<_>>();
    let before = kinds(&h);
    // Select the Goto WP chip ("<n> Goto WP"), then move it right with its arrow.
    let gi = before.iter().position(|k| *k == OrderKind::GotoWaypoint).expect("a Goto WP order");
    assert!(gi + 1 < before.len(), "Goto WP has an order after it");
    let chip = h.find(&format!("{} Goto WP", gi + 1));
    h.click_at(chip.rect.center());
    assert!(matches!(h.app.tpl_select, Some(TplSelect::Order { seat: 0, order }) if order == gi), "{:?}", h.app.tpl_select.is_some());
    assert!(h.has(&format!("SELECTED · UNIT 1 · ORDER {}", gi + 1)));
    h.click("Move right");
    let after = kinds(&h);
    assert_eq!(after[gi], before[gi + 1]);
    assert_eq!(after[gi + 1], before[gi]);
}

#[test]
fn template_report_arrow_leaves_its_command() {
    let mut h = Harness::new("reportmove");
    h.tab("Template");
    add_model(&mut h, "F-51D");
    h.click("+ Order ▾");
    h.click("Take Off");
    h.click("+ Order ▾");
    h.click("Formation");
    h.click("+ Order ▾");
    h.click("OnTookOff");
    let kinds = |h: &Harness| h.app.tpl_seats[0].orders.iter().map(|o| o.kind).collect::<Vec<_>>();
    let before = kinds(&h);
    let ri = before.iter().position(|k| *k == OrderKind::OnTookOff).expect("OnTookOff");
    let chip = h.find(&format!("{} OnTookOff", ri + 1));
    h.click_at(chip.rect.center());
    h.click("Move right");
    let after = kinds(&h);
    assert_ne!(after, before, "the report should leave the Take Off column");
    assert_eq!(after.last().copied(), Some(OrderKind::OnTookOff));
    let ri = after.iter().position(|k| *k == OrderKind::OnTookOff).unwrap();
    let chip = h.find(&format!("{} OnTookOff", ri + 1));
    let takeoff = after.iter().position(|k| *k == OrderKind::TakeOff).unwrap();
    let takeoff_chip = h.find(&format!("{} Take Off", takeoff + 1));
    h.press_and_move(chip.rect.center(), takeoff_chip.rect.center());
    h.release_at(takeoff_chip.rect.center());
    let dragged = kinds(&h);
    assert_eq!(dragged[0], OrderKind::OnTookOff);
    assert_eq!(dragged[1], OrderKind::TakeOff);
}

#[test]
fn map_places_fighters_from_the_forces_dock() {
    let mut h = Harness::new("placefighters");
    h.tab("Map");
    h.wait_for_map();
    h.click("Forces");
    // The Fighters section's Place DPRK (Units has one too, further down).
    let place = h
        .all("Place DPRK")
        .into_iter()
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .expect("Place DPRK");
    h.click_at(place.rect.center());
    let n = h.app.map_fighters.as_ref().map_or(0, |l| l.spots.len());
    assert!(n > 0, "no fighters placed: {}", h.status());
    assert!(h.has_prefix("DPRK: "), "count line shows: {:?}", h.labels());
    // Clear DPRK (the one in the Fighters section), then undo.
    let fighters_top = h.find("FIGHTERS").rect.top();
    let clear = h
        .all("Clear DPRK")
        .into_iter()
        .filter(|n| !n.disabled && n.rect.top() > fighters_top)
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .expect("fighters Clear DPRK");
    h.click_at(clear.rect.center());
    assert!(h.app.map_fighters.is_none());
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.map_fighters.as_ref().map_or(0, |l| l.spots.len()), n);
}

#[test]
fn map_load_over_unsaved_changes_asks_first() {
    let mut h = Harness::new("mapdirty");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    h.key(Key::Num5);
    h.click_at(map.center());
    assert!(h.app.is_dirty(AppMode::Map));
    h.click("Load base map…");
    assert!(h.app.confirm == Some(Confirm::LoadBaseMap));
    h.key(Key::Escape);
    assert!(h.app.confirm.is_none());
    assert_eq!(h.app.east_objectives.len(), 1, "cancel keeps the map");
}

#[test]
fn map_terrain_readout_under_the_pointer() {
    let mut h = Harness::new("readout");
    h.tab("Map");
    h.wait_for_map();
    h.click("Terrain");
    h.click("Coverage");
    let map = h.find("Korea map").rect;
    h.move_to(map.center());
    h.settle();
    assert_eq!(h.app.terrain_readout().as_deref(), Some("Ground not measured"));
    // Measure the ground under the pointer, then the readout shows it.
    let (x, z) = h.app.map_hover_xz.expect("pointer over the map");
    let store = h.app.terrain_store.as_mut().expect("store loaded");
    let (i, j) = ((x / 100.0).floor() as usize, (z / 100.0).floor() as usize);
    for (a, b) in [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)] {
        store.set_node(a, b, 321.0);
    }
    h.settle();
    assert_eq!(h.app.terrain_readout().as_deref(), Some("Ground 321 m · 100 m grid"));
}

impl Harness {
    /// Press the primary button at `from`, move to `to` in steps, but keep it held.
    fn press_and_move(&mut self, from: Pos2, to: Pos2) {
        self.move_to(from);
        let mods = self.mods;
        self.step(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: mods }]);
        for k in 1..=6 {
            self.move_to(from + (to - from) * (k as f32 / 6.0));
        }
    }

    fn release_at(&mut self, pos: Pos2) {
        let mods = self.mods;
        self.step(vec![Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: mods }]);
        self.settle();
    }
}

// ── Side markers (README §8) ──────────────────────────────────────────────

/// Opaque pixels in row `y`.
fn row_fill(img: &egui::ColorImage, y: usize) -> usize {
    let w = img.size[0];
    (0..w).filter(|&x| img.pixels[y * w + x].a() > 128).count()
}

/// The row holding the most of the icon; for a level fighter that is the wing.
fn widest_row(img: &egui::ColorImage) -> usize {
    (0..img.size[1]).max_by_key(|&y| row_fill(img, y)).unwrap_or(0)
}

/// Share of the icon's opaque pixels in the left half.
fn left_share(img: &egui::ColorImage) -> f32 {
    let [w, h] = img.size;
    let (mut left, mut all) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if img.pixels[y * w + x].a() > 128 {
                all += 1;
                left += usize::from(x < w / 2);
            }
        }
    }
    left as f32 / all.max(1) as f32
}

#[test]
fn side_marker_fighters_point_north_before_facing() {
    // `Side::facing_rad` turns DPRK by 180° and leaves NATO as drawn, which
    // is only right if both textures point north. The SVGs are drawn
    // diagonally (DPRK north-west, NATO north-east), so the rasterizer turns
    // them: afterwards the wing is forward of the middle and the plane is
    // left-right symmetric.
    for (eastern, name, bytes) in [
        (true, "EasternFighter", &include_bytes!("../assets/EasternFighter.svg")[..]),
        (false, "NatoFighter", &include_bytes!("../assets/NatoFighter.svg")[..]),
    ] {
        // Turned level, the wing spans a row far wider than anything in the
        // diagonal original.
        let raw = rasterize_svg(bytes, 64);
        let img = fighter_svg_north(eastern, 64);
        let (raw_w, img_w) = (row_fill(&raw, widest_row(&raw)), row_fill(&img, widest_row(&img)));
        eprintln!("{name}: widest row raw {raw_w}, turned {img_w}");
        assert!(img_w as f32 >= raw_w as f32 * 1.15, "{name}: turning did not level the wing ({raw_w} → {img_w} px)");
        let row = widest_row(&img);
        assert!(row < img.size[1] / 2, "{name}: widest row {row} of {} is not in the top half", img.size[1]);
        let share = left_share(&img);
        assert!((share - 0.5).abs() < 0.03, "{name}: {share} of the icon is left of centre");
    }
    assert_eq!(shell::Side::Nato.facing_rad(), 0.0);
    assert!((shell::Side::Dprk.facing_rad() - std::f32::consts::PI).abs() < 1e-6);
}

#[test]
fn map_tool_icons_are_painted_inside_their_box() {
    // Painted line icons (mockup 2f), not font glyphs: each has strokes that
    // stay inside the 24-unit viewBox the painter scales to 18 px.
    use shell::ToolIcon::*;
    for icon in [Select, Front, Salient, Arrow, Objective, Undo, Redo] {
        let paths = shell::tool_icon_paths(icon);
        assert!(!paths.is_empty(), "{icon:?} has no strokes");
        for (pts, _) in paths {
            assert!(pts.len() >= 2, "{icon:?} has a stroke with {} points", pts.len());
            for p in pts {
                assert!((1.0..=23.0).contains(&p.x) && (1.0..=23.0).contains(&p.y), "{icon:?} point {p:?} outside the box");
            }
        }
    }
    let icons: Vec<_> = MAP_TOOLS.iter().map(|t| t.1).collect();
    assert_eq!(icons[4], icons[5], "both objective tools use the flag, tinted by side");
}

#[test]
fn side_marker_textures_are_registered_at_startup() {
    let h = Harness::new("sidetex");
    let tex = shell::side_textures(&h.ctx).expect("side textures registered on the first frame");
    // Cropped to the plane (no ring), square, and large enough to stay crisp.
    for t in &tex {
        let [w, h] = t.size();
        assert_eq!(w, h, "side marker texture is square");
        assert!(w >= 48, "side marker texture is {w} px");
    }
}

// ── Map: front strokes, tool switching, line clears (README §6.2, §6.3) ──

#[test]
fn map_front_strokes_undo_redo_and_escape_mid_drag() {
    let mut h = Harness::new("frontstrokes");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    let at = |dx: f32| map.center() + Vec2::new(dx, 0.0);
    h.key(Key::Num2);
    // Two clicks: two strokes of one point each.
    h.click_at(at(-200.0));
    h.click_at(at(-170.0));
    assert_eq!(h.app.custom_front_xz.len(), 2);
    assert_eq!(h.app.undo_label().as_deref(), Some("Drew front line"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.custom_front_xz.len(), 1, "Ctrl Z takes back the last click");
    h.key_with(Key::Y, CTRL);
    assert_eq!(h.app.custom_front_xz.len(), 2, "Ctrl Y puts it back");

    // A drag is one stroke, however many points it adds.
    h.drag(at(-140.0), at(-20.0));
    let after_drag = h.app.custom_front_xz.len();
    assert!(after_drag > 3, "the drag added points: {after_drag}");
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.custom_front_xz.len(), 2, "one Ctrl Z drops the whole drag");
    h.key_with(Key::Y, CTRL);
    assert_eq!(h.app.custom_front_xz.len(), after_drag);

    // Esc during a drag drops that drag's points and returns to Select; the
    // rest of the drag moves nothing (not the AO either).
    let ao = h.app.front_aabb;
    h.press_and_move(at(0.0), at(60.0));
    assert!(h.app.custom_front_xz.len() > after_drag, "points are being added");
    h.key(Key::Escape);
    assert_eq!(h.app.custom_front_xz.len(), after_drag, "Esc dropped the drag in progress");
    assert!(h.app.map_drawing_mode == MapDrawingMode::None);
    h.move_to(at(120.0));
    h.release_at(at(120.0));
    assert_eq!(h.app.custom_front_xz.len(), after_drag);
    assert!(h.app.front_aabb == ao, "the cancelled drag did not draw an AO");

    // Esc with no drag keeps the finished front.
    h.key(Key::Num2);
    h.key(Key::Escape);
    assert_eq!(h.app.custom_front_xz.len(), after_drag);

    // The palette's Undo does what Ctrl Z does, and says what it undoes.
    let undo = h.find("Undo: Drew front line");
    h.click_at(undo.rect.center());
    assert_eq!(h.app.custom_front_xz.len(), 2);
}

#[test]
fn map_switching_tools_drops_a_half_drawn_mark() {
    let mut h = Harness::new("toolswitch");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    // Salient in progress, then key 4: dropped, like Esc.
    h.key(Key::Num3);
    h.click_at(map.center());
    h.click_at(map.center() + Vec2::new(30.0, 10.0));
    assert!(!h.app.current_salient.is_empty());
    h.key(Key::Num4);
    assert!(h.app.current_salient.is_empty(), "key 4 dropped the salient");
    assert!(h.app.map_drawing_mode == MapDrawingMode::AttackArrow);
    // Arrow drag in progress, then key 3: the arrow is dropped and the rest
    // of that drag draws nothing with the new tool.
    h.press_and_move(map.center(), map.center() + Vec2::new(80.0, -40.0));
    assert!(h.app.attack_drag.is_some(), "arrow being dragged");
    h.key(Key::Num3);
    assert!(h.app.attack_drag.is_none(), "key 3 dropped the arrow");
    h.move_to(map.center() + Vec2::new(120.0, -40.0));
    h.release_at(map.center() + Vec2::new(120.0, -40.0));
    assert!(h.app.attack_arrows.is_empty());
    assert!(h.app.current_salient.is_empty(), "the old drag did not start a salient");
    // Salient in progress, then the palette's Attack arrow tool: dropped too.
    h.click_at(map.center());
    h.click_at(map.center() + Vec2::new(30.0, 10.0));
    assert!(!h.app.current_salient.is_empty());
    h.click("Attack arrow");
    assert!(h.app.current_salient.is_empty(), "the palette dropped the salient");
    assert!(h.app.map_drawing_mode == MapDrawingMode::AttackArrow);
}

#[test]
fn map_clear_lines_salients_and_arrows_undo() {
    let mut h = Harness::new("clearlines");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    h.key(Key::Num4);
    h.drag(map.center(), map.center() + Vec2::new(120.0, -60.0));
    h.drag(map.center() + Vec2::new(0.0, 40.0), map.center() + Vec2::new(120.0, -20.0));
    assert_eq!(h.app.attack_arrows.len(), 2);
    h.key(Key::Num2);
    h.click_at(map.center() + Vec2::new(-200.0, 0.0));
    h.click_at(map.center() + Vec2::new(-150.0, 0.0));
    h.key(Key::Escape);
    h.click("Period");

    h.click("Clear arrows");
    assert!(h.app.attack_arrows.is_empty());
    assert_eq!(h.app.undo_label().as_deref(), Some("Cleared 2 arrows"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.attack_arrows.len(), 2, "Ctrl Z restores the arrows");
    assert_eq!(h.app.custom_front_xz.len(), 2, "and leaves the front alone");

    h.click("Clear lines");
    assert!(h.app.attack_arrows.is_empty() && h.app.custom_front_xz.is_empty());
    assert_eq!(h.app.undo_label().as_deref(), Some("Cleared the drawn front and 2 arrows"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.attack_arrows.len(), 2);
    assert_eq!(h.app.custom_front_xz.len(), 2);
    // The marks came back with them: Ctrl Z now undoes the last drawing.
    assert_eq!(h.app.undo_label().as_deref(), Some("Drew front line"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.custom_front_xz.len(), 1);
}

#[test]
fn map_undo_label_matches_what_ctrl_z_undoes() {
    let mut h = Harness::new("undolabel");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    h.key(Key::Num4);
    h.drag(map.center(), map.center() + Vec2::new(120.0, -60.0));
    h.key(Key::Num5);
    h.click_at(map.center() + Vec2::new(-80.0, 0.0));
    assert_eq!(h.app.map_tool_status(), None, "one-shot objective tool is back to Select");
    // Clear the objective, then draw: the drawing is undone first, then the Clear.
    h.click("Forces");
    let top = h.find("OBJECTIVES").rect.top();
    let clear = h.all("Clear DPRK").into_iter().find(|n| !n.disabled && n.rect.top() > top).expect("objectives Clear DPRK");
    h.click_at(clear.rect.center());
    assert_eq!(h.app.undo_label().as_deref(), Some("Cleared 1 DPRK objectives"));
    h.key(Key::Num4);
    h.drag(map.center() + Vec2::new(0.0, 40.0), map.center() + Vec2::new(120.0, -20.0));
    assert_eq!(h.app.attack_arrows.len(), 2);
    assert_eq!(h.app.undo_label().as_deref(), Some("Drew an attack arrow"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.attack_arrows.len(), 1);
    assert_eq!(h.app.undo_label().as_deref(), Some("Cleared 1 DPRK objectives"));
    h.key_with(Key::Z, CTRL);
    assert_eq!(h.app.east_objectives.len(), 1, "then the Clear");
    assert_eq!(h.app.attack_arrows.len(), 1, "without touching the older arrow");
    assert_eq!(h.app.undo_label().as_deref(), Some("Drew an attack arrow"));
}

#[test]
fn map_objective_tool_hint_is_short() {
    let mut h = Harness::new("objhint");
    h.tab("Map");
    h.key(Key::Num5);
    assert_eq!(h.app.map_tool_status().as_deref(), Some("Tool: DPRK objective · Click to place · Shift for more"));
    h.key(Key::Num6);
    assert_eq!(h.app.map_tool_status().as_deref(), Some("Tool: NATO objective · Click to place · Shift for more"));
}

#[test]
fn map_dock_tabs_fit_a_three_digit_reference_count() {
    let mut h = Harness::new("docktabs");
    h.tab("Map");
    let group = crate::frontlines::MapRefGroup { path: PathBuf::from("K13 AFB_mp.Group"), entity: crate::ast::Il2Entity::new("Group") };
    h.app.map_refs = vec![group; 123];
    h.settle();
    let tabs: Vec<Node> = ["Period", "Forces", "References 123", "Terrain"].iter().map(|l| h.find(l)).collect();
    // One row of cells, left to right, filling the 288 px dock content.
    for pair in tabs.windows(2) {
        assert!((pair[1].rect.top() - pair[0].rect.top()).abs() < 0.5, "{:?} is on another row", pair[1].label);
        assert!((pair[1].rect.left() - pair[0].rect.right()).abs() < 0.5, "{:?} does not follow {:?}", pair[1].label, pair[0].label);
    }
    let span = tabs[3].rect.right() - tabs[0].rect.left();
    assert!((287.0..=288.5).contains(&span), "tabs span {span} px, not the 288 px dock");
    // Each label (bold, as when selected) with its count fits its cell with padding.
    for ((_, label, count), node) in MapDock::tabs(123).into_iter().zip(&tabs) {
        let need = shell::dock_tab_needed_width(&h.ctx, label, count);
        let cell = node.rect.width();
        assert!(need + 2.0 * shell::DOCK_TAB_PAD <= cell + 0.5, "{label} {count:?} needs {need} px of a {cell} px cell");
    }
    h.click("References 123");
    assert!(h.app.map_dock == MapDock::References);
}

// ── Help (README §6.4) ────────────────────────────────────────────────────

#[test]
fn help_closes_on_escape_and_shortcuts_still_work() {
    let mut h = Harness::new("helpesc");
    h.key(Key::F1);
    assert!(h.app.help_open);
    h.key(Key::Escape);
    assert!(!h.app.help_open, "Esc closes Help");
    h.key_with(Key::Num6, CTRL);
    assert!(h.app.mode == AppMode::Map, "Ctrl 6 still switches tabs");
    h.key(Key::F1);
    assert!(h.app.help_open && h.app.help_topic == HelpTopic::Front);
}

/// Every non-ASCII character in the UI code and the manual (shown as Help)
/// must exist in a loaded font; a missing one draws as an empty box (✓ did).
#[test]
fn every_ui_glyph_is_in_a_loaded_font() {
    let h = Harness::new("glyphs");
    // The check itself works: U+2713 (check mark) is in none of the fonts.
    assert!(!h.ctx.fonts(|f| f.has_glyph(&egui::FontId::proportional(13.0), '\u{2713}')));
    let mut missing = std::collections::BTreeSet::new();
    let sources = [
        include_str!("ui.rs"),
        include_str!("shell.rs"),
        include_str!("help.rs"),
        include_str!("../USER_MANUAL.md"),
    ];
    for text in sources {
        for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
            for ch in line.chars().filter(|c| !c.is_ascii()) {
                let ok = h.ctx.fonts(|f| f.has_glyph(&egui::FontId::proportional(13.0), ch));
                if !ok {
                    missing.insert(ch);
                }
            }
        }
    }
    assert!(missing.is_empty(), "glyphs no loaded font has: {missing:?}");
}

/// No control spills out of a side panel (README §4 widths). An overflowing
/// row (the Airfield folder rows did) pushes egui into an unpainted gap.
#[test]
fn side_panel_controls_stay_inside_their_panel() {
    // (tab, left panel width, right panel width), after the 172 px rail.
    const PANELS: [(&str, f32, f32); 6] = [
        ("Template", 270.0, 304.0),
        ("Army Generator", 330.0, 300.0),
        ("Fighter Pack", 270.0, 304.0),
        ("Exclusive Activation", 290.0, 290.0),
        ("Airfield", 290.0, 270.0),
        ("Map", 56.0, 304.0),
    ];
    let mut h = Harness::new("panelfit");
    let mut out = Vec::new();
    for (tab, left_w, right_w) in PANELS {
        h.tab(tab);
        let left = (shell::RAIL_W, shell::RAIL_W + left_w);
        let right = (SCREEN.x - right_w, SCREEN.x);
        for n in &h.nodes {
            let control = matches!(n.role, Role::Button | Role::CheckBox | Role::ComboBox | Role::Slider | Role::SpinButton | Role::TextInput);
            if !control || n.rect.width() <= 0.0 || n.rect.top() < shell::HEADER_H {
                continue;
            }
            let x = n.rect.left();
            let inside = |(a, b): (f32, f32)| x >= a - 0.5 && x < b;
            if inside(left) && n.rect.right() > left.1 + 0.5 {
                out.push(format!("{tab} left: {:?} {:?} ends at {}", n.label, n.role, n.rect.right()));
            }
            if inside(right) && n.rect.right() > right.1 + 0.5 {
                out.push(format!("{tab} right: {:?} {:?} ends at {}", n.label, n.role, n.rect.right()));
            }
        }
    }
    assert!(out.is_empty(), "controls overflow their panel:\n{}", out.join("\n"));
}

/// A button label never wraps onto a second line ("Clear arrows" did in a
/// narrow add_enabled_ui child). Wrapped text makes the button ~44 px tall.
#[test]
fn button_labels_stay_on_one_line() {
    let mut h = Harness::new("onelinebuttons");
    let mut tall = Vec::new();
    for (_, label) in MODES {
        h.tab(label);
        if label == "Map" {
            for dock in ["Period", "Forces", "Terrain"] {
                h.click_prefix_if_present(dock);
                collect_tall_buttons(&h, &format!("Map › {dock}"), &mut tall);
            }
        }
        collect_tall_buttons(&h, label, &mut tall);
    }
    tall.sort();
    tall.dedup();
    assert!(tall.is_empty(), "buttons with wrapped labels:\n{}", tall.join("\n"));
}

fn collect_tall_buttons(h: &Harness, tab: &str, out: &mut Vec<String>) {
    for n in &h.nodes {
        // Tree chips are 36 px by design; the Map dock tabs (40 px, top row
        // of the dock) are skipped. A wrapped label makes a button 40 px.
        let dock_tab = n.rect.top() < 100.0 && ["Period", "Forces", "References", "Terrain"].iter().any(|t| n.label.starts_with(t));
        if n.role == Role::Button && !dock_tab && n.rect.height() > 36.5 && !n.label.is_empty() && n.rect.width() < 200.0 {
            out.push(format!("{tab}: {:?} {}x{}", n.label, n.rect.width(), n.rect.height()));
        }
    }
}

impl Harness {
    fn click_prefix_if_present(&mut self, prefix: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.role == Role::Button && n.label.starts_with(prefix)).cloned() {
            self.click_at(n.rect.center());
            self.settle();
        }
    }
}

/// Terrain heights apply on export unless this session turns the switch off.
#[test]
fn terrain_heights_on_export_start_on() {
    let mut h = Harness::new("terrainon");
    assert!(h.app.terrain_apply, "Apply terrain heights starts on");
    h.tab("Map");
    h.wait_for_map();
    h.click_prefix_if_present("Terrain");
    let n = h.find("Apply terrain heights on export");
    assert_eq!(n.toggled, Some(true));
}

#[test]
fn map_lock_ao_blocks_empty_drag_and_reset() {
    let mut h = Harness::new("lockao");
    h.tab("Map");
    h.wait_for_map();
    let map = h.find("Korea map").rect;
    let full = h.app.front_aabb;
    h.drag(map.center() + Vec2::new(-80.0, -40.0), map.center() + Vec2::new(80.0, 40.0));
    assert_ne!(h.app.front_aabb, full, "unlocked drag sets the AO");
    let locked_box = h.app.front_aabb;
    h.click("Lock AO");
    assert!(h.app.ao_locked);
    assert!(h.find("Reset AO").disabled, "Reset AO is off while locked");
    h.drag(map.center() + Vec2::new(-40.0, -20.0), map.center() + Vec2::new(40.0, 20.0));
    assert_eq!(h.app.front_aabb, locked_box, "locked drag leaves the AO");
    h.click("Unlock AO");
    assert!(!h.app.ao_locked);
    h.click("Reset AO");
    assert_eq!(h.app.front_aabb, full);
}
