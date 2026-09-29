# Handoff: IL-2 Group Generator UI redesign (Direction A)

## How to use this with Claude Code

Unzip at the repository root. `CLAUDE.md` points Claude Code here. Then say: "Start the UI redesign, phase 1."

## 1. Overview

This redesigns the six mode tabs of the IL-2 Group Generator (Template, Army Generator, Fighter Pack, Exclusive Activation, Airfield, Map). The goal is less clutter and a clearer hierarchy, with **the same features**:

- Today, every tab is one long vertical scroll inside a single `CentralPanel`.
- The new version gives every tab the same fixed layout: a mode rail on the left; a header with the primary action; input, canvas and settings panels that each scroll on their own; and a status bar.

## 2. About the design files

The files in `docs/ui-redesign/design/` are **design references built in HTML**. They show the intended layout and behavior. They are not code to ship. Recreate them in the existing Rust/eframe/egui code, using egui panels and widgets.

- Open `docs/ui-redesign/design/Group Generator Mockups.dc.html` in a browser. **Turn 2 (screens 2a–2f) is the approved direction.** Turn 1 is earlier exploration; ignore 1b and 1d.
- `docs/ui-redesign/design/Interface Spec.dc.html` holds the rules: layout, map tool safety, side markers, glossary, undo, minimum sizes, shortcuts, and the backlog.

`src/theme.rs` and `src/shell.rs` are **real starting code**. They were written against egui 0.29 (the repo uses `ComboBox::from_id_salt`) and have not been compiled against this repo. Fix any API drift noted in their header comments.

## 3. Fidelity

- **High fidelity** for layout, panel widths, the color tokens, type sizes, minimum targets, the order of controls, and labels.
- **Illustrative** for sample data (aircraft names, checkzone names, counts). Bind to real state instead.
- Icons: the mockups use placeholder shapes. Use the existing `assets/*.svg` textures (§7).

## 4. App shell (all tabs)

Panel order matters in egui: panels added first take the outer space.

```rust
const MODES: [(AppMode, &str); 6] = [
    (AppMode::Template, "Template"),
    (AppMode::Recon, "Army Generator"),
    (AppMode::Fighter, "Fighter Pack"),
    (AppMode::Exclusive, "Exclusive Activation"),
    (AppMode::Airfield, "Airfield"),
    (AppMode::Map, "Map"),
];

fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let keys = shell::read_shortcuts(ctx, self.mode == AppMode::Map);
    self.handle_shortcuts(ctx, &keys); // §6.4

    egui::SidePanel::left("rail").exact_width(shell::RAIL_W).resizable(false)
        .show(ctx, |ui| {
            let mut idx = MODES.iter().position(|(m, _)| *m == self.mode).unwrap_or(0);
            let labels: Vec<&str> = MODES.iter().map(|(_, l)| *l).collect();
            if shell::mode_rail(ui, &labels, &mut idx) { self.open_help(self.page_help_topic()); }
            self.set_mode(MODES[idx].0); // leaving Map resets the draw tool (§6.2)
        });
    egui::TopBottomPanel::bottom("status").exact_height(shell::STATUS_H)
        .show(ctx, |ui| self.status_line(ui)); // rewritten with shell::status_bar
    match self.mode {
        AppMode::Template => self.template_page(ctx),
        AppMode::Recon => self.recon_page(ctx),
        AppMode::Fighter => self.fighter_page(ctx),
        AppMode::Exclusive => self.bomber_page(ctx),
        AppMode::Airfield => self.airfield_page(ctx),
        AppMode::Map => self.map_page(ctx),
    }
    self.confirm_dialogs(ctx); // §6.3
    help::show_window(ctx, &mut self.help_open, &mut self.help_topic);
}

fn template_page(&mut self, ctx: &egui::Context) {
    egui::TopBottomPanel::top("tpl_header").exact_height(shell::HEADER_H).show(ctx, |ui| { /* shell::page_header */ });
    egui::SidePanel::left("tpl_left").exact_width(270.0).resizable(false)
        .show(ctx, |ui| egui::ScrollArea::vertical().show(ui, |ui| { /* Add units, Units list */ }));
    egui::SidePanel::right("tpl_right").exact_width(304.0).resizable(false)
        .show(ctx, |ui| egui::ScrollArea::vertical().show(ui, |ui| { /* Selection + settings */ }));
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::TopBottomPanel::bottom("tpl_tree").exact_height(236.0).show_inside(ui, |ui| { /* order tree */ });
        /* formation view fills the rest */
    });
}
```

| Region | Size | Contents |
|---|---|---|
| Mode rail | 172 px wide, full height | "IL-2 GROUP / GENERATOR" in Condensed 17 px; six 32 px rows showing a two-digit index and the label; "Ctrl 1–6" hint; Help + `F1` at the bottom |
| Header | 54 px tall, 18 px side padding | Title (Heading, uppercase), optional leading control, file name (italic, NEUTRAL_700); on the right: secondary buttons, then the primary button with a `Ctrl G` hint |
| Left panel | per tab (§5) | Inputs: files, units, templates, plans |
| Center | fills | Canvas or output preview |
| Right panel | per tab | The current selection first, then settings sections that collapse to a one-line summary |
| Status bar | 30 px tall | 8 px severity square (+ ⚠ for Warn/Error), message; on the right: undo label, "Undo" link, `Ctrl Z` |

Rules:
- The primary button is always top-right and always `Ctrl G`. **Load…** is always the first secondary button (`Ctrl O`).
- The window never scrolls; each panel has its own `ScrollArea`. Remove the outer `ScrollArea` in `update()`.
- An empty center names the first step and shows the button for it (e.g. "Add templates… to begin").
- Selected list items use `shell::card(ui, true, …)`. Collapsible settings use `CollapsingState` with a summary on the right (see `settings_section` below).

```rust
fn settings_section(ui: &mut Ui, id: &str, title: &str, summary: &str, open: bool, body: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(id);
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open)
        .show_header(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(summary).small().color(c::NEUTRAL_700)));
        })
        .body(|ui| body(ui));
    ui.separator();
}
```

## 5. Screens

State field names are the existing ones in `GroupGeneratorApp`. Copy comes from the mockups; where a mockup shortens existing text, the long version moves to Help (§6.5).

### 5.1 Template (mockup 2a)
- **Header:** "Template Builder" · "Editing {stem}" · Load… (`Ctrl O`) → `load_template_group`; Reset → confirm dialog (§6.3) → `reset_template_builder`; **Generate File** → `generate_unit_template`.
- **Left, 270 px:** the selected unit's picture and plain description first (the highlighted catalog model before a unit is added), then the model palette. Section "MODELS". The catalog name opens Load catalog… / Add group… / Use built-in catalog. Kind segmented control, then class and country combos and the model list. Rows are 28 px; the selected row gets ACCENT_100 and a bold "+ Add". **Add to Template** adds the highlighted model. A row can also be dragged into the formation. There is no added-units roster on this side.
- **Center (fills):** "FORMATION VIEW" over a 32 px grid. This is `draw_template_schematic`, with Zone In as a solid ACCENT_700 circle and Zone Out as a dashed ACCENT circle, labelled "Zone In 8 km" / "Zone Out 12 km". Bottom-right: a zoom group − / % / + / Fit (`tpl_view_zoom`, reset `tpl_view_pan`). Bottom-left hint: "Scroll to zoom · right-drag to pan". When a unit, order, or event is selected, a card on the view's 14 px left margin holds its options (Change Model and the seat, order, or event fields). The picture is above Models, not on this card. The formation draws in the open area to the right of the card.
- **Bottom (360 px):** "ORDER OF BATTLE". Each unit is a root: a small picture, the unit chip (side marker, "{n} · {model}"), role ("Lead ×n" / "Follows n"), and "{country} · {skill} · {k} orders". Clicking the chip selects it. Drag the chip up or down to reorder. Right-click: Duplicate, Delete, Move up, Move down, Add Order. Order and event chips stay to the right of the unit and still move by column (‹ › or drag). Hint: "UNIT → OnSpawned → orders. ‹ › or drag a chip left and right. Help ›".
- **Right, 304 px:** drawers for the whole template, available whether or not a unit is selected. A one-line guide, then Placement & Checkzones (open), Activate or Spawn, Waypoints, Catalog. Selecting a unit does not hide Place or Activate or Spawn. The unit's own fields are on the formation view.
- **Status:** the load/generate message; Undo appears after Remove or Reset.

### 5.2 Army Generator (mockup 2b)
- **Header:** "Army Generator" + leading segmented control "New from templates | Rework existing" (`recon_submode`). Secondary: Add templates… (`Ctrl O`) → `add_recon_template`, Add folder… → `add_recon_folder`. In Rework mode: Add packs… → `add_placed_packs`. **Generate File** → `generate_recon_file` / `generate_rework_file`.
- **Left, 330 px:** "TEMPLATES · n", note "click a type to change it". One card per `recon_slots` entry:
  - Side marker (from `recon_eastern`), "{n} · {name}", and a Remove link.
  - Meta line: "{file} · {counts}".
  - A row of six 36 × 28 type buttons (`unit_kind_icon_button` with the existing type textures).
  - An Influence slider with its value.
  - Zone In checkboxes ("(suggested)" in NEUTRAL_700).
  - `shell::warning("Select at least one Zone In.")` when none is selected.
- **Center:**
  - Top line: "Copies templates onto a 10 km parking grid and picks which copies activate each mission. Help ›".
  - Then "PARKING GRID · 10 km cells from 40000, 40000": a preview of the grid blocks per type (2×2, 3×3) with side markers and "{name} · {copies}" labels.
  - Bottom (auto height): "COPY MIX", a four-column table (Template, Type, Placed, Activate) from `allocate_mix` / `TypeMix`, then a bold ACCENT_800 line "{live} of {placed} copies will activate." In Rework mode the columns are On map / Activate.
- **Right, 300 px:**
  - **ARMY:** DPRK | NATO segmented control (`recon_eastern`), hint "Sets the icons on this page. Map › Place picks the side."; "Import new templates as" type row (`unit_kind_picker`).
  - **COPIES:** Templates to create (`recon_total`, 1–64); "Spawn all copies (no randomizer)" (`recon_strip_randomizer`); Activate ratio (`recon_percent`) with the hint "Share of each type's copies that start. Help ›"; "Keep loaded positions" (`recon_keep_positions`). Keep `recon_dserver_note`, shortened to one line.
  - **Timing**, collapsed, summary "Start {s} s · {ms} ms apart" (`recon_timing_sliders`).
  - Rework mode also shows **Reconnect Mission Begin** (the per-slot combos) when the randomizer is removed.

### 5.3 Fighter Pack (mockup 2c)
- **Header:** "Fighter Pack" · "Built-in logic" or the custom file name · Reset · **Generate File** → `generate_fighter_file`.
- **Left, 270 px:** "AIRCRAFT TYPES · {n} selected", hint "Flights cycle through the selected types. Lead skill ≥ wingman." One row per `AIRCRAFT_TYPES`: checkbox (`type_enabled`) + skill slider 0–4 (`type_skill`); disabled rows at 55 % opacity. Footer: "Skill 0–4" and a "Select all" link.
- **Center (on a 32 px grid):**
  - "PACK PREVIEW" plus a sentence built from `linked_groups`.
  - A row of group cards ("Group n", "{total} aircraft"; Group 1 has an accent border) joined by lines labelled "NodeGate".
  - A panel "GROUP 1 · FLIGHTS" with the note "{total} aircraft · sizes {mix}" (from `flight_sizes`). Table columns: Flight, Type, Aircraft, Role, Altitude.
  - An altitude strip from `altitude_min` to `altitude_max` with a tick per flight.
- **Right, 304 px:**
  - **PACK:** Linked groups (1–10), Flights (1–10), Max in a flight (1–8), hint "Each flight is one randomizer slot. Help ›".
  - **COUNTRY:** combo (`country`) with a side marker and the hint "Zone In / Zone Out trigger on NATO [2]." (600-series: "on DPRK [1]").
  - **Altitude range**, open: Min / Max drag values; hint "Complete 4-ships split 2 low / 2 high. Help ›".
  - **Timers**, collapsed: "Cooldown {s} s · Reinf. {s} s · Delete {s} s".
  - **Custom pack template**, collapsed: "Built-in · experimental" (from `optional_template_section`).

### 5.4 Exclusive Activation (mockup 2d)
- **Header:** "Exclusive Activation" · "Editing {stem}" · Add templates… (`Ctrl O`) → `add_bomber_template` · **Generate File** → `generate_bomber_file`.
- **Left, 290 px:** "PLANS · n", note "one active at a time". One card per `bomber_slots` entry: "{n} · {name}", meta "{file} · {units} units", and on the right a "Ready" accent tag or "⚠ End timer" / "⚠ Checkzone". Clicking a card selects it (new state `bomber_selected: Option<usize>`).
- **Center:** the selected plan.
  - Kicker "Plan n", name in Condensed 24 px, meta line; buttons Add again / Remove at the top-right.
  - Blueprint panel **START CHECKZONES** (note "opens this plan, closes the others"): the multiple-zone warning when there is more than one zone, then checkboxes with "(suggested)", plus `trigger_warnings`.
  - Blueprint panel **END TIMER** (border WARN when missing): the missing-timer warning text, then the timer combo (280 px) and `cleanup_warnings`.
  - **SEQUENCE:** one bar per plan with the selected plan highlighted, and the hint "A start zone opens one plan and closes the rest until its end timer fires."
- **Right, 290 px:**
  - **OUTPUT:** "Export in place" (`bomber_keep_positions`) with "Off: new templates park on a 10 km grid from 40000, 40000. On: groups stay where they are."
  - **NAMING:** one sentence + Help (the `SUGGESTED_TRIGGER_NAMES` / `SUGGESTED_END_NAMES` text moves to Help).
  - **USE IT FOR:** one sentence.
  - `show_missing_locale_hint` stays on the selected plan.

### 5.5 Airfield (mockup 2e)
- **Header:** "Airfield to Multiplayer" (was "Task Editor Airfield to Multiplayer") · the file name · Load airfield… (`Ctrl O`) → `load_airfield` · **Generate File** (was "Export File") → `export_airfield`. Disabled until `airfield_info` is `Some`.
- **Left, 290 px:**
  - **GET THE FILE:** three numbered steps (22 px boxed numbers):
    1. "In game, open a Freeflight mission and take off from the airfield."
    2. "Open /missions/_gen.mission in the mission editor. Select the field, then File › Save Selection to File."
    3. "Load that file here."

    Then Help ›.
  - **FRIENDLY PLANE COALITION:** segmented control "NATO [2] | DPRK [1]" (`airfield_western`), hint "USA airfields use NATO."
- **Center:** "WHAT GENERATE WILL CHANGE" + one sentence. Then:
  - Two blueprint panels. **Removed:** player planes with country, AutoRemove subgroup, "Player / SP graph objects {strip_count}". **Relinked to {side}:** `unlink_zones`, bordered ACCENT.
  - A **Kept** panel with four big numbers: vehicles / ships, AI aircraft, blocks, checkzones.
  - `shell::warning("Still required after export: add planes to fly and set the starting location.")`
  - Empty state: "Load an airfield group exported from _gen.mission." with the Load button.
- **Right, 270 px:** **AIRFIELD:** Name, Layout (inside a Group / blocks at the root), Origin (monospace).

### 5.6 Map (mockup 2f)
- **Header:** "Map" · "{Season} {Year} · {battle or 'Entire front'}" · Load base map… (`Ctrl O`) → `load_base_map` · **Generate Base Map** → `generate_front_file`.
- **Tool palette, 56 px, left:** 36 × 36 `shell::tool_button`s, top to bottom:

  | Key | Tool | `MapDrawingMode` | Banner hint |
  |---|---|---|---|
  | 1 | Select AO / move units | `None` | (no banner) |
  | 2 | Draw front | `BaseFront` | "Click west to east, or drag · Esc to cancel" |
  | 3 | Salient | `Salient` | "Click along the front · right-click to finish · Esc to cancel" |
  | 4 | Attack arrow | `AttackArrow` | "Drag from tail to tip · Esc to cancel" |
  | 5 | DPRK objective (DPRK tint) | `PlaceEastObjective` | "Click to place · Shift for more" |
  | 6 | NATO objective (NATO tint) | `PlaceNatoObjective` | "Click to place · Shift for more" |

  Then a divider, a spacer, and Undo / Redo at the bottom (disabled at 40 % when unavailable).
- **Center:**
  - The map (`draw_korea_map`) fills the space, with a 64 px "FRONT DATE" strip at the bottom: the `front_t` slider with year ticks 1950–1953 and a "← → step" hint. Keep `handle_map_timeline_keys`.
  - Overlays: the tool banner at top-center (`shell::map_tool_banner`, only when a drawing tool is active).
  - AO readout (monospace, boxed) at top-left, **52 px from the top** so it clears the banner.
  - Legend chip at bottom-left: Front, DPRK, NATO, AO, "All layers ▾" opens the full `draw_map_legend` list.
  - Zoom group at bottom-right: − / % / + / Reset view / Reset AO (from `map_view_toolbar`).
  - The crosshair cursor while a drawing tool is active.
- **Right dock, 304 px, three tabs** (new state `map_dock: MapDock { Period, Forces, References }`):
  - **Period:**
    - Year and Season combos side by side.
    - A blueprint card with `mark.title` (Condensed 16 px), `mark.note`, and `mark.editor_hint()` in bold ACCENT_700.
    - Battle focus combo (unchanged grouping) and the battle note.
    - "Suggested aircraft" as outline tags.
  - **Forces:**
    - **FIGHTERS** (note "from Fighter Pack"): Place DPRK / Place NATO; "Groups at once" (`fighter_waves`); "Fill AO (up to {MAX_PACKS} at once)"; the count line "{side}: {n} groups in {packs} packs" with a side marker; Clear.
    - **OBJECTIVES** (note "one click each · Shift for more"): DPRK · {n} / NATO · {n} buttons that pick tools 5 / 6; Clear.
    - **UNITS** (note "from Army Generator"): Place DPRK / Place NATO (disabled when `recon_keep_positions`), Load DPRK… / Load NATO…; counts; Clear.
  - **References {n}:** the list from "Loaded Reference Groups" (Add reference groups…, rows with X/Z and Remove). This is no longer a collapsing section.
- **Status:** the active tool name and hint, e.g. "Tool: Salient · right-click to finish · Esc to cancel"; Undo on the right after any Clear.

## 6. Interactions & behavior

### 6.1 Layout
Only panels scroll, and the rail, header and status bar never move. Settings sections remember whether they were open (`CollapsingState` stores this through its persistent id).

### 6.2 Map tool safety (spec item 4)
- One tool is active at a time. The active tool is filled ACCENT, the banner shows its name, and the status bar repeats the hint.
- **Esc:** clears `current_salient`, sets `attack_drag = None`, cancels an in-progress front stroke, and sets `map_drawing_mode = MapDrawingMode::None`.
- **Objective tools are one-shot:** after a successful placement, `if !ui.input(|i| i.modifiers.shift) { self.map_drawing_mode = MapDrawingMode::None; }`.
- Right-drag pans in every tool (unchanged).
- Switching away from Map sets `map_drawing_mode = None` (in `set_mode`).
- Cursor: `ctx.set_cursor_icon(egui::CursorIcon::Crosshair)` while hovering the map with a drawing tool.

### 6.3 Undo and confirmation (spec item 7)
- Add one `shell::Undo<T>` per tab, where `T` is a clone of the state being removed:
  - `tpl_undo: Undo<(Vec<TemplateSeat>, …settings)>`
  - `recon_undo: Undo<Vec<ReconSlot>>`
  - `bomber_undo: Undo<Vec<BomberSlot>>`
  - `map_undo: Undo<MapForces>`, where `MapForces` is a small struct holding `map_ships`, `map_ground_east`, `map_ground_nato`, `map_armies`, `map_fighters`, `map_imported_fighters`, `east_objectives`, `nato_objectives` and `map_refs`.

  If a field isn't `Clone`, derive it or snapshot only what Clear removes.
- Call `record(label, before)` before the change. Labels read like "Cleared 6 NATO units" or "Removed 76 mm Battery".
- **Ctrl Z and the status bar's Undo** restore it on the current tab. On Map, Ctrl Z first undoes a drawn mark (the existing `remove_last_mark`) when the last action was a drawing; otherwise it undoes the last Clear. Store the last action kind to decide.
- **Confirmation dialogs** (`shell::confirm_dialog`):
  - Reset (Template): "This clears {n} units, {m} orders and the placement settings. The catalog stays. You can undo with Ctrl Z."
  - Load… when the tab has unsaved edits: add `dirty: bool` per tab, set on any edit and cleared on Generate or Load.
- Esc closes a dialog.

### 6.4 Shortcuts (spec item 10)
`handle_shortcuts` maps `Shortcuts` to actions:

| Keys | Action |
|---|---|
| `Ctrl G` | This tab's primary action. Ignored if the button is disabled |
| `Ctrl O` | This tab's Load… / Add… (first secondary button) |
| `Ctrl Z` / `Ctrl Y` | Undo / redo (redo is Map drawings only, as today) |
| `Ctrl 1–6` | Switch tab |
| `F1` | Help for this tab (`open_help(page_help_topic())`) |
| `Esc` | Close dialog → cancel map line → back to Select |
| `1–6` (Map, not typing) | Pick map tool |
| `← →` | Front date (existing) |

Every shortcut appears in its button's hover text.

### 6.5 Short text, details in Help (spec item 8)
- Where a choice is made, keep one line of explanation, plus "Help ›" (`shell::hint(ui, text, true)` → `open_help(topic)`).
- Move the longer explanations into `crate::help` under the tab's topic. Examples:
  - The "Allow multiple spawns" hover text becomes "Respawns after the cooldown once every unit is destroyed." The Zone Out cleanup, DeathCount and hiding-unit details move to Help › Template.
  - The Army Generator intro paragraphs (`recon_new_panel`, `recon_rework_panel`) become the one center sentence in §5.2. Their content moves to Help › Army Generator.
  - The Exclusive Activation and Airfield intro paragraphs move the same way.
- Constraints are never shown only on hover (for example, why Spawn is locked).

### 6.6 Minimum sizes (spec item 9)
- Body text 13 px; small text 12 px and never smaller.
- Interactive height 28 px (`interact_size.y`); map tools 36 × 36; order-tree chips ≥ 28 px tall.
- At least 4 px between targets.
- Text in the accent color uses ACCENT_700 or darker.

## 7. Glossary and string changes (spec item 6)

| Use | Replaces |
|---|---|
| **DPRK** (or "DPRK [1]" where the coalition value matters) | Eastern, East, Axis, "Eastern  [1]" |
| **NATO** (or "NATO [2]") | Western, UN, "Western  [2]" |
| **Zone In / Zone Out** | Zone IN, ZONE IN |
| **Load…** replaces contents · **Add…** appends · **Place** puts on the map · **Remove** one item · **Clear** all of one kind · **Reset** defaults · **Generate File** writes output | "Export File", "Add template or pack…", "Add pack…", "Load Base Map…" |

String edits in ui.rs, among others:
- Map fighters/units "Eastern"/"NATO" → "Place DPRK"/"Place NATO"; "Load Eastern…" → "Load DPRK…"; "{side}: {n} grps in {packs} pack(s)" → "{side}: {n} groups in {packs} packs"; "Eastern {} · NATO {}" → "DPRK {} · NATO {}".
- "Draw Custom Front" → "Draw front"; "Clear Custom Lines" → "Clear lines"; "Clear Salients" → "Clear salients"; "Clear Attack Arrows" → "Clear arrows"; "Select AO / Move Units" → "Select AO / move units".
- Army "Eastern" toggle → "DPRK". Airfield "Western  [2]" / "Eastern  [1]" → "NATO [2]" / "DPRK [1]".
- Hover texts that say "Eastern" or "Western".
- Also check the `label()` fns in `crate::template` (e.g. `ZoneCoalition`) and `crate::frontlines` for "Eastern"/"Western"/"Axis".
- Do **not** rename identifiers, asset filenames or serialized values.

Use sentence case on buttons.

## 8. Design tokens

All are in `src/theme.rs` (`theme::c`). Summary:

- **Ground:** BG `#f2f2f3`, SURFACE `#e9e9ea`, TEXT `#1d1f20`, DIVIDER `#d0d0d1`, MARK `#7d7e7f`.
- **Accent ramp 100–900:** `#eef6ff #d6ebff #b5d9fd #94bce3 #749dc4 #597ea3 #416180 #2c455d #1d2d3d`; base `#5980a6`.
- **Neutral ramp 100–900:** `#f5f5f8 #e7e7ea #d4d4d7 #b7b7ba #98989b #7a7a7d #5d5d60 #424244 #2b2b2d`.
- **Data colors:** DPRK `#ad3136`, NATO = ACCENT_700, FRONT `#b8353a`, WARN `#c9913a` (status dot), WARN_TEXT `#93661c`. These are the only non-accent colors.
- **Type:** Barlow Condensed SemiBold for headings (22 px page title, 14 px uppercase section titles, 17 px brand); Barlow 13 px body, 12 px small; Barlow SemiBold for emphasis; monospace 12 px for coordinates, values and key hints. Put the TTFs in `assets/fonts/` (Google Fonts, SIL OFL).
- **Shape:** square corners (2 px rounding), 1 px hairlines. The primary button is the only solid fill. Selected cards and primary buttons carry "+" corner marks (11 px, MARK color).
- **States:** hover ACCENT_100 fill + ACCENT border; pressed ACCENT_200 / ACCENT_600; primary button ACCENT → hover ACCENT_600 → pressed ACCENT_700; disabled ACCENT_300. Focus: a 2 px ACCENT ring.

### Side markers (spec item 5)
- Use the existing textures: `assets/EasternFighter.svg`, `NatoFighter.svg`, `Eastern*/Nato*` Armor, Supply, Arty, Infantry, Train, Shipping and Objective.
- **DPRK icons point south, NATO icons point north** everywhere: lists, legend, rail badges and default map placement. Draw with `paint_rotated_image` at `Side::facing_rad()` (this assumes the SVGs are drawn pointing north; flip the constant if not).
- On the map, a unit's own heading (heading drag) wins once the user sets it. The default placement heading faces the front.
- `shell::paint_side_marker` is the fallback shape.

## 9. Build phases (for Claude Code)

Each phase must compile, keep every feature, and pass its checks before the next one starts.

1. **Theme and shell.** Add `mod theme; mod shell;` in `main.rs`/`lib.rs` (the files are already in `src/`); call `theme::apply` in `run()`; set the window's min size to 1280×800; restructure `update()` into the rail, status bar and per-mode page functions (§4), with the existing panel bodies moved in unchanged.
   *Check:* all six tabs reachable from the rail and with Ctrl 1–6; the window never scrolls; the status message still updates.
2. **Template** (§5.1).
   *Check:* adding, reordering and removing seats works; order and event chips select and edit; Generate writes the same file as before for the same input.
3. **Map** (§5.6, §6.2).
   *Check:* keys 1–6 pick tools; Esc cancels a salient in progress; an objective tool returns to Select after one click (Shift keeps it); leaving the tab resets the tool; the base map generated from the same input is unchanged.
4. **Army Generator, Fighter Pack, Exclusive Activation, Airfield** (§5.2–5.5).
   *Check:* same outputs for the same inputs; every warning that exists today still appears inline.
5. **Glossary, undo, confirmations, Help moves** (§6.3, §6.5, §7).
   *Check:* no "Eastern"/"Western"/"Zone IN" left in user-facing strings (`rg -n '"[^"]*(Eastern|Western|Zone IN)' src/`); Clear, then Ctrl Z restores the items; Reset asks first.
6. **Sizes and polish** (§6.6).
   *Check:* no text below 12 px; every interactive element ≥ 28 px tall; every primary button shows its shortcut.

## 10. Backlog (not in this release)

**Define a minimum usable group for each tab.** A readiness checklist next to Generate (spec item 2) is deferred until each tab has a written minimum valid output. Rules the code already enforces, as a starting point:
- Exclusive Activation: each plan has at least one start checkzone and an end timer.
- Army Generator: each template has at least one Zone In selected.
- Template: flights with wingmen must use Activate.
- Template, Fighter Pack, Airfield and Map: not yet defined.

## 11. Files

- `docs/ui-redesign/design/Group Generator Mockups.dc.html`: screens. Turn 2 (2a–2f) is final.
- `docs/ui-redesign/design/Interface Spec.dc.html`: the rules.
- `docs/ui-redesign/design/support.js`, `docs/ui-redesign/design/_ds/…`: needed to open the HTML files locally (open from the `docs/ui-redesign/design/` folder).
- `src/theme.rs`: tokens, fonts, egui visuals.
- `src/shell.rs`: rail, header, primary button, cards, section titles, hints, segmented control, tool buttons, status bar, tool banner, confirm dialog, `Undo<T>`, shortcuts, side markers.
- The original source is `src/ui.rs`, about 10.2k lines, eframe/egui.
