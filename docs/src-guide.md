# IL-2 Group Generator — `src/` module guide

> **For humans and AI models.** Read this before editing `src/`.
> One entry per file: what it owns, key public API, and which UI mode(s)
> use it. Entries are verified against the source as of the initial release.

## Ground rules

- Binary crate (`src/main.rs` only — there is no `lib.rs`). It generates
  IL-2 Sturmovik: Great Battles `.Group` / `.Mission` text files for the
  Korea map.
- `ui.rs` is the GUI entry point. The other egui files are `shell.rs`
  (shared page chrome and widgets), `theme.rs` (design tokens, fonts,
  visuals) and `help.rs` (detached Help window). Every other file is
  logic/data with no egui imports.
- Files are parsed by `parser` into an `ast::Il2Entity` tree (named
  blocks). Generation modules build trees; `serialize` writes them back
  to text. Unknown keys are kept; never invent a schema.
- Most Generate/Export paths go through `save_with_sidecars` (in
  `ui.rs`), which writes the group plus merged translation sidecars
  (`.eng`, …). Fighter Pack writes the group only. Map base maps write
  the group, then merge/overlay sidecars separately.
- Coordinates: world = game meters, **X north** (up on the Korea map),
  **Z east**. Full map square is `geo::MAP_MIN`/`MAP_MAX` = 0…499_200.
  Parking grid is `placement::MAP_MIN`/`MAP_MAX` = 40_000…470_000 with a
  10 km step. Map image UV/`Pos2` conversions live in `ui.rs`.
- `build.rs` embeds `assets/models/*.png` into `model_spec` (deduped
  identical bytes). Do not parse model images at runtime.
- Assets in `assets/`: unit SVG icons, Korea map JPEGs, `Models.Group`
  (+ language sidecars), `Payloads.txt`, `roads.svg` / `railroads.svg`,
  `combined_terrain.bin`, and per-model PNGs under `assets/models/`.

## Files

### `ui.rs` — egui front-end
The single GUI entry point. Owns `GroupGeneratorApp` (all state), the
six mode tabs, every panel, drag/drop map editing, order-tree widgets,
map UV/world/`Pos2` conversions, SVG/JPEG loading, and
`save_with_sidecars`. Calls into logic modules only through their public
APIs; contains no AST work and no generation logic. Help content is
rendered by `help.rs`. **All modes.**

Since the 2026 redesign (`docs/ui-redesign/README.md`) every tab is a
`*_page(ctx)` that adds fixed side panels and a center, each scrolling
on its own; `update()` draws the mode rail, header and status bar
around them. Per-tab undo snapshots (`shell::Undo`), confirmation
dialogs (`Confirm`) and unsaved-edit checks (`tpl_fingerprint`,
`map_fingerprint`) live here too.

`ui_tests.rs` (compiled with `cargo test` only) drives the whole app on a
headless `egui::Context`: it finds widgets by their AccessKit labels,
clicks, drags and types, and answers file dialogs from a queue
(`dialog::answer`) so Load / Generate run end to end into a temp folder.
Custom-painted widgets set `widget_info` so the tests (and screen readers)
can find them.

### `theme.rs` — design tokens
Color tokens (`theme::c`: ground, accent and neutral ramps, DPRK / NATO /
FRONT / WARN), the embedded Barlow fonts (Hack as the last fallback for
arrows), text sizes (12 px minimum) and light-only egui visuals.
`theme::apply` runs once at startup. **All modes.**

### `shell.rs` — page chrome and shared widgets
Mode rail, page header with the `Ctrl G` primary button, status bar with
Undo, cards, blueprint panels, section titles, hints with "Help ›",
28 px links, tags, segmented controls, map tool buttons and banner,
settings sections, confirmation dialog, `Undo<T>`, side markers, and
`read_shortcuts` (Ctrl G / O / Z / Y / 1–6, F1, Esc, map keys 1–6).
Presentation only. **All modes.**

### `ast.rs` — `Il2Entity`: the .Group AST
The schema-agnostic in-memory tree every loaded or generated `.Group`
becomes: `block_type`, typed `index`/`targets`/`objects` mirrors, raw
`properties: Vec<(String, String)>` (serialization source of truth —
file order, original quoting and decimal precision), and `children`.
Conventions that trip people up: `property()` returns the raw **quoted**
value while `name()` strips quotes; typed fields must be kept in sync
with their raw properties via `set_targets`/`set_objects`/`append_target`/
`replace_target_id` (direct field mutation leaves the property stale);
`set_existing_property` refuses to introduce keys the prototype lacks
(no plane-only keys on ground units); position writes (`translate_xz`,
`set_ypos`, mapnet's `set_xz`-style helpers) preserve the original
decimal count. Traversal: `for_each`/`for_each_mut` (take `&mut F`),
`find_by_name(_mut)`, `max_index`, `collect_indexes`, `count_block_type`,
`pos_xz`/`first_xz`. `parser` builds it, `serialize` emits it; every
generation module transforms or holds it. **All modules.**

### `aircraft.rs` — aircraft & country tables
Static Fighter Pack / Template identity tables, not catalog models.
`AIRCRAFT_TYPES` (id/label/script/model for the seven Korea fighters),
`COUNTRIES` (501 USSR, 502 DPRK, 503 PRC, 601 USA), `aircraft_by_id`,
`default_skill` / `loose_skill` / `pair_skills` (lead never below
wingman), 1950s `flight_number` / `flight_color` / `plane_display_name`
(Red 12, White 21, …), `encode_tcode` / `encode_tcode_color` (IL-2
glyph strings), `callsign_for` (MiG-15bis on 501 → Honcho 12), and
`plane_coalitions_for_country` (500-series watches `[2]`, 600-series
watches `[1]` — the *other* coalition, for fighter-pack checkzones),
and pack naming (`country_pack_tag`, `linked_fighter_pack_name`,
`fighter_pack_filename` — USSR/DPRK/PRC/USA from the country code).
**Fighter Pack, Template, Map.**

### `airfield.rs` — SP airfield → multiplayer
`inspect_airfield` / `AirfieldInfo` (player planes, checkzones to
unlink, strip count, vehicles/AI planes/blocks) and `clean_airfield`
(delete the player plane + entity and the SP graph hanging off them —
takeoff, music, objectives, tiny `CZ_PLAYER_OUT` bubbles; retarget
object-linked checkzones and set `PlaneCoalitions`).
`EASTERN_PLANE_COALITIONS` = `[1]`, `WESTERN_PLANE_COALITIONS` = `[2]`.
`CleanReport` is the export summary. `strip_ai_planes` drops the
remaining AI `Plane`s the same way (no proximity rules). **Airfield mode,
harvest.rs.**

### `harvest.rs` — `_gen.mission` → airfield database
Cuts the start airfield out of a Task Editor `_gen.mission` (radius around
the `Airfield` block, nearest-field split, link closure for logic up to
20 km out), cleans it with `airfield.rs`, and files it into a database
folder: `raw/` archive, `<name>_<country>.Group` + sidecars, `catalog.Group`
(`AirfieldRecord` rows, upserted), `models.tsv`. `harvest_root` is pure;
`harvest_file` does the I/O. `GenWatcher` polls the Missions folder and
reports a rewrite once it has been stable for 1.5 s. Drops `#` comments and
the `Options` header before parsing (`WindLayers` rows are not `.Group`
syntax). **Airfield mode.**

### `bombers.rs` — Exclusive Activation
Detects plans in template groups (`inspect_plan`, `BomberPlanInfo`:
checkzones, timers, suggested trigger/end timer, cleanup and trigger
warnings), detects already-generated packs (`looks_like_exclusive_pack`,
`extract_exclusive_plans` — strips NodeGates links and `[n]`/`*n*`
suffixes), and `link_bomber_plans` / `link_bomber_plans_with` write a
file where only one plan activates at a time (NodeGates mutex; optional
in-place keep-positions). Multiple `Zone IN` MCUs in one template count
as a single plan. `SUGGESTED_TRIGGER_NAMES` / `SUGGESTED_END_NAMES`.
**Exclusive mode.**

### `duplicate.rs` — duplication engine & country overrides
Index reallocation and MCU pointer reconnection used by every cloner.
`reallocate_ids` assigns sequential Indexes; `reconnect_pointers`
rewrites `targets`/`objects` plus `LinkTrId`/`MisObjID`/`TarId`/`CmdId`
(IDs not in the map stay so external links survive); `duplicate_template`
is reallocate + reconnect. `generate_groups` builds N copies (first
keeps original Indexes; used by serialize pipeline tests, not ui).
`apply_overrides` rewrites `Country` on Plane/Vehicle/Ship and
Script/Model on Plane — ui.rs uses this after fighter/map generation.
**Fighter, Exclusive, Army, Map, Template; serialize.rs pipeline tests.**

### `flights.rs` — fighter flight configuration
Rebuilds **Group 1** from Template Builder pair logic (Independent
seats, OnSpawned → AttackArea lead / Cover wing, leftover and extra
pair leads each get their own AttackArea, events → that plane’s
Mission Complete → Force Complete / RTB / deactivate for that bird
only, AI RTB off, Spawn + repeat) then injects the pack randomizer
(`Random i:pct%` → `Out i` → `Spawn i`, 500 ms equal-odds waterfall),
per-flight DeathCount, pack hooks (`Enable Spawner` / `Disable Spawner`
/ `Delete Orders`; the reinforcement timer is omitted), finger-four placement,
GUI altitudes (low-cover 500–1500 m scaled by max, each complete
4-ship 2 down / 2 up +2000 m, leftovers low, 25–50 m wing stacks),
timers, and identity (Script/Model/Country/Callsign/TCode). Zones stay
at the original pack sizes (16 km IN / 35 km OUT); AttackArea is 30 km
air / 600 s. NodeGates and `RTB - 1` are kept from the loaded linked
pack. `FlightConfig` (defaults: 4 flights, max 4, mig15bis + la11
skills 3/2, country 501, 180 s cooldown, 60 s
delete orders, 1000–5500 m altitude; reinforcement is stored but not written), `configure_aircraft`,
`flight_sizes` (cycles max…1, so 4/4 → 4,3,2,1).
**Fighter Pack, Map.**

### `frontlines.rs` — Korea timeline & base map
Assembles the Map-mode pack. Timeline data lives in
`frontlines/timeline.rs` and is re-exported (`TIMELINE`, `TimelineMark`,
`preview_front_xz`, `front_xz`, `timeline_index`, `mark_for_battle`).
This file owns `Season` / `YEARS` / `BATTLES` / `Battle`, suggested
aircraft per period, `snapshot_front_xz` / `timeline_preview` /
`preview_dots`, and `generate_front` — clips the dated front to the
user AABB, paints influence / salients / attack arrows / battle marks,
and stamps fighter/ship/ground packs plus user reference groups
(`FrontOptions`, `FrontPack`, `MapFighterPack` / `MapShipPack` /
`MapGroundPack` / `MapRefGroup`). `inspect_base_map` / `ImportedBaseMap`
split a previously generated Korea base map back into AO, front,
attack arrows, fighter packs, and unit packs. Constants: `ARROW_TAIL_WIDTH`,
`PLACE_MARGIN` (10 km), `AOI_GAP` (5 km). **Map mode.**

### `frontlines/timeline.rs` — dated front polylines
Submodule of `frontlines`. `TIMELINE` is a dated sequence of WGS84
front polylines (weekly/tighter in 1950, monthly through mid-1951,
then seasonal once the MLR freezes). `TimelineMark` carries date,
season, title/note, front, and optional UN east-coast pocket ring.
`preview_front_xz` / `preview_pocket_xz` interpolate for the slider;
`front_xz` / `pocket_xz` project a mark; `editor_map` is the IL-2
landscape season name. **Map mode (via frontlines.rs).**

### `geo.rs` — map geography
Full Korea square (`MAP_MIN` / `MAP_MAX` = 0…499_200), two-point
Seoul–Sinuiju projection (`latlon_to_xz`, `on_map`), the 38th-parallel
line (`parallel_38_xz`), Yalu polyline (`yalu_river_xz` / `yalu_x_at_z`
— used to keep the front on the Korean bank), `MAJOR_WATERWAYS`, and
`cities_on_map` / `MAJOR_CITIES` (`RefCity`: name, exact game X/Z, DPRK
flag, label side — hardcoded so labels sit on the JPEG, not the
projection). **Map mode.** Distinct from `placement::MAP_MIN` (parking).

### `help.rs` — help window
Egui code outside `ui.rs`, like `shell.rs` and `theme.rs`. Embeds `USER_MANUAL.md`, splits it
by `##` heading (`section_for`), and opens a decorated native viewport
(`show_window`) with a topic combo. `HelpTopic` covers every mode plus
Language / Import / Troubleshooting. Does not touch the AST.
**All modes.**

### `locale.rs` — translation sidecars
MCU_Icon / MCU_TR_Subtitle store numeric LC indexes; the strings live
in UTF-16 LE (BOM) files next to the `.Group`. The editor will not
invent them on resave. `LANG_EXTS` = eng/chs/fra/ger/rus/spa.
`LocaleTable` parse/serialize/merge/overlay; `has_sidecars`,
`merge_template_sidecars`, `write_sidecars`; `decode_locale_bytes` /
`encode_locale_utf16le`. **All generate/export paths that carry
labels.**

### `mapclip.rs` — 2D geometry utilities
The map editor's math in mission X/Z (`geo` `Coord { x: XPos, y: ZPos }`).
`WorldAabb` (area of operations, `full_map` / `from_corners`),
polyline/linestring/ring clipping to the box, influence polygons
(stretched, minus salients, fill quads), `apply_salients` / salient
patches, `snap_to_front`, `point_north_of_front`,
`stroke_self_intersects` / `ring_self_intersects`, and the extension
rules (`can_extend_salient`, `can_extend_west_east`).
`FRONT_PLACE_BAND` (10 km) places units near the front.
`clamp_point_south_of_yalu` keeps strokes on the Korean bank.
**Map mode.**

### `mapfighters.rs` — fighter placement on the map
`place_in_coalition` parks fighter groups in the side's influence
polygon (checkerboard, waves × groups, optional AO fill, 8-pack cap).
`MapFighterLayout` / `FighterSpot` (wave/pack/slot), `rtb_ao_point`
(friendly-rear AO corner closer to the group), `country_for_coalition`
(eastern keeps a 500-series setpoint or 501; NATO is always 601),
`MAX_PACKS` = 8. **Map mode.**

### `mapground.rs` — ground unit placement
`place_ground` / `place_ground_jobs` park armor/supply/artillery on
dry open terrain, infantry on any dry land (not water), and trains/columns on roads or rails (`mapnet`).
`GroundKind`, `GroundJob` (optional `RouteLayout` + weapon range),
`GroundSpot` (pos, heading, network pose, hashed objective, in-AO,
soft `issue`), `MapGroundLayout`, `START_DELAY_S` / `GROUP_DELAY_S`,
`ARTY_OBJECTIVE_RADIUS` (unknown-artillery fallback),
`numbered_ground_issues` (UI warnings), `attack_across_front` (AttackArea
past the FLOT when no objective is marked), `layout_path_waypoints`
(off-road Goto WP hops toward the objective or front, staying on dry land
when possible). Infantry parks in the front band
and faces the closest objective, or the front when none are marked.
Other front-band groups also face the front when no objective is set. **Map mode, Army Generator.**

### `mapload.rs` — Korea reference-point catalog
Loads airfield and building points from `References/` (and optional
`templates/`): `MCU_Waypoint` entries in
`landscape_Korea_FullScene.Group` MARKS groups, plus standalone `*AFB*`
`.Group` files. `MapCatalog` / `MapPoint` / `PointKind`,
`load_catalog` / `load_catalog_from_dirs` / `template_dirs`. Compiled
and unit-tested; **not yet called from `ui.rs`** — Map labels currently
use `geo::cities_on_map`, and user reference groups go through
`frontlines::MapRefGroup`. **Map (future / tests).**

### `mapnet.rs` — road & railroad network
Korea road/rail polylines parsed from mission-editor SVGs
(`assets/roads.svg`, `assets/railroads.svg`) into world X/Z (50 m per
SVG unit, `svg_to_world`). Owns the whole network-placement pipeline:
`Network`/`PolyLine` (arc-length indexed lines, `nearest` point
snapping), `RouteLayout` (extracted from authored groups by
`inspect_route` — trains → rail; roads require a detected perfect
column), `NetworkSpot` (live pose: line, lead distance,
forward/reverse, unit + waypoint world positions), seeded `place_route`
parking (2.5 km `NETWORK_SPACING`, AO bias, `prefer` hook), interactive
snapping (`snap_lead_to_pointer`, `snap_waypoint_to_pointer` — a WP may
sit on another branch or behind the column — `align_heading_to_path`),
two-WP Zone IN straddle logic (`column_waypoint_dists`,
`sample_network`), `park_route_copy` (writes column positions into
the group, preserving decimal precision), `inspect_path_waypoints` /
`park_path_waypoints` for off-road Goto WP hops, and
`inspect_visual_heading` / `inspect_waypoint_xz` / `inspect_parked_network`
to restore an already-exported column (including a curved road).
Unit-tested against `TemplateExamples/` fixtures. **Map mode, Army Generator.**

### `mapshipping.rs` — ship placement
`place_ships` parks ship groups on coalition water inside the AO
(leftovers go to the nearest friendly water outside the box).
`MapShipLayout` / `ShipSpot` (pos, heading, in-AO);
`randomize_headings` / `aim_at_hashed_objectives`; `MAX_SHIPS` = 64,
`SHIP_SPACING` = 8 km, `START_DELAY_S` / `GROUP_DELAY_S`.
**Map mode.**

### `model_spec.rs` — aircraft / unit model data
Per-script UI overlay (the AST stays schema-agnostic): `ModelClass` /
`ModelSpec`, `spec_for`, `class_for` / `classes_in`, `ceiling_m`,
`format_cruise`, `suggested_waypoint_speed_kmh` (90 % of the slowest
moving unit, rounded to 10 km/h), `script_id`, and preview lookup
(`png_for_script`, `PLACEHOLDER_PNG` from `build.rs`). Infantry squads
all use `assets/models/infantry.png`.
**Template, Fighter Pack, Map.**

### `payloads.rs` — payloads & modifications
Armament/mod catalog parsed once from `assets/Payloads.txt`.
`catalog()` / `PayloadCatalog::for_script`, `payload_preview` /
`mods_preview`, munition lookup, mod slots (exclusive radio vs toggle)
and mask parse/encode (`parse_mod_mask`, `encode_mod_mask`,
`select_exclusive`, `set_toggle`, `option_selected`). Updating the
text file and rebuilding picks up new aircraft. **Template mode.**

### `placement.rs` — parking grid & shared placement helpers
Two jobs in one file:

1. **Parking grid** for generated copies: `MAP_MIN`/`MAP_MAX` =
   40_000…470_000, `GRID_STEP` = 10 km, `grid_xz` / `move_to_grid` /
   `template_square_origins`, `move_anchor_to`, `apply_group_heading`
   (yaw visuals about the model centroid; logic MCUs stay put).
2. **Map placement helpers** consumed by ship/ground/route code:
   `PlaceOpts` (front band, favored objectives, seed, occupied points),
   `heading_toward` / `heading_toward_nearest` (0 = north / +X, 90 =
   east / +Z), `mix_index` / `hashed_pick`, `subsample_favoring` /
   `subsample_spaced`, `UNIT_PLACE_SPACING` = 4.5 km.

**Fighter, Exclusive, Army, Map.**

### `pack.rs` — fighter pack generation
Clones **Group 1** + `RTB - 1` from a configured fighter-pack template
N times via `duplicate_template` (fresh indices from
`root.max_index() + 1`) and rebuilds `NodeGates` so copies stay linked
as in the shipped 3/5-packs: a 6-timer cell per group
(`nIN/OUT - ENABLE/DISABLE` + two fanouts named `n`); each group's
Zone IN/OUT retargeted at its own cell's OUT timers; each cell's IN
timers point at the group's `Enable Spawner` / `ENABLE / PULSE IN` and
`Delete Orders` / `Disable Spawner`; fanouts pulse the *other* cells'
IN timers (mutual exclusion — fanout self-skip is unit-tested).
Parking is the map grid (`placement::move_to_grid`) or explicit
positions, translating the group tree, its RTB waypoint and its gate
cell by one shared delta. `generate_pack` (grid; root name
`{nation} Fighters Npack - Linked` from the planes' country),
`generate_pack_at`
(explicit positions), `builtin_template` (bundled 3pack V6),
`park_rtbs`, `zone_in_radius` (used by Map UI). `inspect_pack` /
`PackInfo` and `group_anchor_xz` are for tests / internal parking
(`inspect_pack` is `#[allow(dead_code)]` from ui.rs's point of view).
**Fighter Pack, Map.**

### `parser.rs` — text → `Il2Entity` parser
Nom-based, schema-agnostic parser for IL-2 `.Group` nested-bracket
files. No regex, no required schema: unknown block types/keys are kept
verbatim and numeric values stay text, so parse + serialize round-trips
are lossless for any editor file. Handles identifier **or integer**
property keys (`Damaged` tables), quoted strings, integer arrays
(`Targets`/`Objects` are also lifted into typed `Il2Entity` fields),
bare text values (clock times, dotted dates), quoted string list items
(`Trailers`), and unquoted `x, y;` coordinate pairs (influence-area
`Boundary` — stored with empty property keys). `parse_group_file`
requires exactly one root and no trailing garbage; `parse_il2_document`
accepts one or many roots and wraps several in a synthetic "Airfield"
`Group` (the airfield flow). Strips a UTF-8 BOM. Unit-tested with
synthetic blocks and real `TemplateExamples/` fixtures.
**All load paths.**

### `recon.rs` — Army Generator
Random ground-unit packs. `generate_recon(_ex)` clones each template
into numbered copies (`{name} [n]`), parks each type on its own 10 km
grid square (`placement::template_square_origins`, parking `MAP_MIN`
grid; `keep_positions` keeps template coords), silences the clone
`Mission Begin`s (IL-2 fires them even with `Enabled = 0`), and builds
the `Recon Randomizer` group: `Recon: Mission Begin` → optional
`Randomizer:DELAY n` (`start_delay_s` hold, then `group_delay_s`,
default 500 ms, per type) → `Randomizer:INPUT [n]` → per-type mutex
waterfall (`Random i:pct%` at 500 ms steps, equal odds as `flights.rs`;
activate % applied **per type** via `allocate_mix` / `wanted_winners` —
50% of 5+5 gives 3+3, not 6 of 10). A winning `Out i` fires the copy's
original Mission Begin chain (`ENABLE / PULSE IN` → Zone IN) + `Keep i`;
`Close_Remaining_Output(s)` cuts the chain; the delayed `Delete unused i`
→ `Dump unused i` (MCU_Delete over the copy's Vehicle/Ship/Plane/Block/
entities) removes the losers. `spawn_all` skips the randomizer
(`Army N`, Mission Begins left live). Allocation (`allocate_copies`
largest-remainder), inspection (`inspect_unit` / `UnitPlanInfo` —
checkzones + `Zone IN` triggers, vehicle/block counts,
`restore_start_choices` recommending timers that target Closer zones
and `ENABLE / PULSE IN`; weapon-range + `mapnet` route hints;
`inspect_placed_pack` / `looks_like_placed_pack` /
`placed_copy_count` / `copy_type_name` — strips generator `[n]` and
editor `*n*` suffixes; `inspect_army_copies` via
`weapon_range::classify_army_unit`), parking placed copies onto map
spots (`park_recon_copies(_headed/_spots)` — road/rail via
`mapnet::park_route_copy`; off-road Goto WP hops via
`mapnet::park_path_waypoints`; `park_army_group(_spots)`, `park_army_mixed`
ships + ground), AttackArea snapping (`snap_copy_attack_areas` /
`snap_army_attack_areas` / `snap_placed_attack_areas` /
`snap_army_placed_attack_areas` via
`weapon_range::snap_ground_attack_areas`; no-objective groups fire
across the front along heading),
and rework on exported packs (`combine_placed_packs`, `strip_randomizer`,
`restore_always_on` — rewires each copy's Mission Begin to a chosen
start MCU and renames `Ground Units N`; `apply_randomizer` /
`apply_randomizer_typed` rebuild the selector in place without moving
copies; `group_start_delays`). Randomizer MCU icons sit near the first
copy on a 40 m stagger. `SUGGESTED_ZONE_NAMES` = `Zone IN`.
**Army Generator, Map.**

### `serialize.rs` — AST → .Group text
The only writer in the crate: emits an `Il2Entity` tree in the editor's
nested-bracket format — CRLF line endings, 2-space indent, `Key = Value;`
properties, blank line between sibling child blocks. `Index`/`Targets`/
`Objects` are emitted from the typed AST fields (compact array re-render
via `format_int_array`), so index reallocation and link reconnection
done through the AST reach the file; empty-key list items (Trailers
paths, influence-area boundary pairs) emit as bare `value;`; every
other property emits verbatim (original quoting and decimal precision).
That asymmetry is what makes parse → mutate → serialize lossless.
`serialize_group` backs every Generate/Export path in ui.rs
(`save_with_sidecars`, or a direct write for base maps / fighter packs).
Round-trip tested with synthetic blocks and real `TemplateExamples/`
fixtures, including the full parse → duplicate → override → serialize
pipeline. **All generation paths.**

### `template.rs` — Template Builder core
Seat model + generator for one proximity-triggered unit group.
`generate_template(&TemplateOptions)` emits `Logic` (`Translator Mission
Begin` → `ENABLE / PULSE IN` → `Zone IN` / `Zone Out` `MCU_CheckZone`
with per-mix defaults — air 16/35 km, ground 10/19 km, train 19/30 km —
plus `PULSE OUT`, Activate or Spawn bring-up: `Trigger Spawner` +
`SpawnCount`, optional `DeathCount` → `COOLDOWN` repeat via
`Modifier Set Value` + `Reset Counter`), and the `MISSION END` →
`MISSION END ORDERS` (→ `Force Complete - High`, optional `RTB DELAY` →
`RTB East/West n` per coalition/group) + `DELAYED END ORDERS` (60 s
with RTB, 2 s without) → `Deactivate Units` → `DELETE DELAY` →
`Trigger Delete` cleanup hub. `Units` = cloned object + `MCU_TR_Entity`
pairs (wingmen target-linked to their lead; `AiRTBDecision` /
`StartType` written only when the prototype has the key — writing them
on a Vehicle breaks mission-editor parsing); `Orders` = command MCUs
(one MCU shared across `shared_with` seats; ground AttackArea sits at
the group origin); `Waypoints` = `WP n` hops along +X at 4 km (300 m
area for planes, 100 m ground, unclamped speed, 0 m altitude for
ground, per-hop altitude/priority override). Goto WP
delays pulse only the WP MCU; on arrival the WP pulses Attack /
AttackArea and Time on Target in parallel (list order doesn't matter),
and TOT expiry continues the chain. Seat model + bookkeeping:
`TemplateSeat` / `CatalogUnit` / `FlightRole` / `PlaneStart`,
`append_seat` / `replace_seat_unit` / `copy_seat_attributes` /
`move_seat`, `apply_plane_start` / `AIR_START_ALTITUDE_M` (airstart
defaults to 1500 m for every aircraft; an existing height is kept;
ground starts clear altitude), `normalize_order_chain` + index remapping,
`insert_goto_waypoint_after`, `set_report_following`,
`order_tree_columns` / `order_tree_layout` / `event_triggers_order`
(GUI: OnSpawned is its own column; an event that Then's an order
breaks the previous-hop line so cleanup waits for the event), formations
(`AIR_FORMATIONS` / `GROUND_FORMATIONS`), placement (`PlaceLayout`,
`place_offset`, `finger_four_offset`, `PLACEMENT_SPACING` = 150 m),
checkzones (`ZoneCoalition`, `ZoneMix`, `zone_defaults`,
`visual_range_m`, `near_visual_range`, `zone_mix_for_seats`), train
carriages (`catalog_carriage_scripts`, `carriage_label`), attack-area
suggestions via `weapon_range` (`OrderSpec::for_unit`,
`apply_suggested_attack_area`, `refresh_attack_areas_for_seat`,
`attack_area_range_limit`), waypoint altitude/area helpers, and catalog
loading (`bundled_catalog` from `assets/Models.Group`,
`builtin_plane_catalog`, `load_catalog` — subgroups `Planes` /
`All Planes` / `Aircraft`, `Vehicles`, `Trains`, `Ships`, `Fixed`,
`User Added`, loose-block fallback — `load_catalog_as_user_added`,
`merge_catalog`). `load_template` / `TemplateLoad` fills the builder
from a `.Group`: native files (`ENABLE / PULSE IN` + `Zone IN` +
`MISSION END` + `Units`) round-trip seats and orders; anything else is
rebuilt from world objects and `MCU_CMD_*` / waypoints, with warnings
for dropped icons, extra checkzones, NodeGates, and unmapped events.
Generated at a fixed origin (40 000, 40 000) on a
150 m MCU grid; moving it onto the map is the caller's job. Names other
modes look for: `Zone IN`, `ENABLE / PULSE IN`, `MISSION END`,
`Trigger Delete`, `Force Complete - High`. Never writes `NodeGates`.
**Template mode. Consumed by Exclusive (bombers.rs `inspect_plan`
validates the cleanup graph).**

### `weapon_range.rs` — weapon range data
Range tables keyed by script type-id (artillery max, armor / MG max
effective). `range_for_script`, `group_weapon_range`,
`attack_area_radius_m` / `suggested_attack_area_m` /
`shortest_range_m` / `area_exceeds_range` (Template Builder
AttackArea sizing, capped at 3000 m in the GUI), `ArmyUnitKind` +
`classify_army_unit` (Ship / Train / Artillery / MobileArtillery /
Armor / Supply / Infantry — used by Map and Army), `snap_ground_attack_areas`.
`is_infantry_script` / `group_is_infantry` detect squads.
Fallbacks: `UNKNOWN_ARTILLERY_M` = 15 km, `UNKNOWN_ARMOR_M` = 2 km,
`ARTILLERY_RANGE_MIN_M` = 4.5 km. **Template, Map, Army Generator.**

### `terrain.rs` — measured ground heights
Sparse store of ground heights on a 100 m lattice (224 × 224-node tiles,
HGT1 file under `%APPDATA%\IL2MissionUtility\terrain`). `lookup` falls back
from 100 m to 200 / 400 / 800 m cells and reports the spacing that
answered; `ground_margin_m` / `parked_plane_margin_m` give the lift for
that spacing. Stores and answers only: no parsing, no placement.
**Map mode (Terrain tab), terrain_apply.rs.**

### `terrain_apply.rs` — units onto the measured ground at export
`apply_terrain_heights` runs on a finished tree just before it is written:
Vehicle / Train and their ground waypoints get terrain + margin, planes with
`StartType` 1–3 get terrain + a small lift, ships go to 0, entities keep
their offset. Units already within 5 m of the ground keep their Y;
unmeasured spots (or survey-only 800 m data) keep the old Y and are listed
in `ApplyReport`. Airborne planes, MCUs and static objects are never
touched; X/Z never change; applying twice is a no-op. ui.rs calls it for
Template, Exclusive, Army (generate + rework), Map and Fighter Pack exports
when *Apply terrain heights on export* is on (off by default) — not for Airfield.
**All generate paths except Airfield.**

### `heightprobe.rs` — terrain probe files
Builds T-34 probe groups (one per 100 m land node, one file per tile; the
800 m survey pass over the whole map) and ingests files the user snapped
with the editor's *set to ground*, refusing ones that look unsnapped.
`run_cli` serves `--probe-survey`, `--probe-ingest`, `--probe-status`.
**Map mode (Terrain tab) and the command line.**

### `watermap.rs` — water/terrain queries
Packed Korea terrain mask from `assets/combined_terrain.bin`
(`WMAP` + width/height + cells). Pixel (0,0) is the north-west corner
of the mission square. Bitfield: water `& 1`, road `& 2` (reserved),
open `& 4`. `TerrainMap` (`WaterMap` is a historical alias):
`builtin()`, `is_water_xz` / `is_open_xz`. Used by ship placement
(stay in water) and ground placement (stay on dry open land).
**Map mode, Army Generator.**

### `main.rs`
Binary entry: declares every `src/*.rs` module (plus
`frontlines/timeline.rs` via `frontlines`) and calls `ui::run()`.
There is no `lib.rs`.

## Header template (for per-file doc comments)

```rust
//! <file> — <one-line purpose>
//!
//! <2–6 sentences: what it owns, what it does NOT do, key design notes.>
//!
//! ## Public API
//! * `fn ...` — ...
//!
//! ## Used by
//! * ui.rs (<mode>) — ...
```
