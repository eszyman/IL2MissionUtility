# HANDOFF — IL-2 Mission Utility (Claude working copy)

> Read this first in every new session. Update the **Session log** and
> **State** sections before you stop. Last updated: 2026-11-20.

## 1. Where things live

| What | Path |
|---|---|
| **Working copy (all work happens here)** | `C:\Claude\IL2MissionUtility\Claude IL2Mission Utility\` |
| Original drop (read-only reference, do not edit) | `C:\Claude\IL2MissionUtility\` (everything except this folder) |
| Module map for `src/` (read before editing) | `docs/src-guide.md` |
| End-user manual (embedded in the Help window) | `USER_MANUAL.md` |
| Coding rules | `.cursorrules` (nom parser only, schema-agnostic AST, never skip tests, GUI decoupled from AST, minimal UI) |

**Git.** The working copy is a local git repo (`main`). There is **no
GitHub remote** yet. The first commit is the untouched 2026-09-04 source.
`.gitattributes` is `* -text`: files are stored byte-for-byte because `.Group`
fixtures are line-ending sensitive. Do not remove it.

**Build and test.**

```bash
cargo test --offline     # 376 passed after P1 (371 at baseline); ~45 s cold build
cargo build --release    # ships target/release/il2_mission_utility.exe
```

`--offline` works because every crate is already in the local cargo cache.

## 2. Team roles (codex-delegation skill)

| Role | Who | Notes |
|---|---|---|
| Overlord / reviewer | **Claude Opus acting as Fable** (Fable quota exhausted, per user 2026-09-23) | Plans, scopes, reviews every diff, merges, owns accountability. |
| Executor | **GPT-6 Astra via Codex broker**, if the account gets access | All coding work. `max` effort for implementation, `ultra` for reviews. **Not available as of 2026-09-23** (see below). |

**Delegation loop in use.** There is no remote, but Claude and Codex share
this machine's disk. So the loop is:

1. Claude commits on `main`. The primary clone must be clean.
2. Astra `git clone`s the primary clone into its own temp dir, branches
   `codex/<slug>`, implements, runs `cargo test --offline`, and commits
   there. The Codex sandbox cannot write the primary clone's `.git`.
3. Claude runs `git fetch <temp-clone> codex/<slug>:codex/<slug>`, reviews
   the diff, and re-runs the tests itself.
4. Claude merges with `--no-ff` into `main`, or sends it back with `codex_resume`.

Once a GitHub remote exists, switch to the skill's standard `git_push`/`git_pull` flow.

**Astra is not on this OpenAI account (verified 2026-09-23).**
`~/.codex/models_cache.json` lists only `gpt-6-luna` (the CLI default when no
model is set), `gpt-5.6-terra`, `gpt-5.6-luna`, `gpt-5.5`, `gpt-reserve` and
`codex-auto-review`. There is no Astra and no Sol. `config.toml` sets no
`model`, so a delegation that does not pass `model` silently runs **Luna**.
Always pass `model` explicitly, and confirm the model in
`~/.codex/sessions/.../rollout-*.jsonl` (`"model":`). For P1 the user chose
Claude to implement (one-task waiver). Ask again for each new task: Astra
after a plan upgrade, `gpt-5.6-terra`, or Claude.

**Codex install (done 2026-09-23).** The user installs the CLI:
`npm.cmd i -g @openai/codex`, then `codex.cmd login`. Use the `.cmd` forms
because the PowerShell execution policy blocks `npm.ps1`. Then **fully quit
Claude from the system tray** and reopen it. Closing the window is not
enough: the broker caches the binary lookup once per process. It
auto-finds the vendored exe under `%APPDATA%\npm\node_modules\@openai\codex\...`,
which is installed (codex-cli 0.156.1). If the
broker still reports `ENOENT`, set `CODEX_BIN` to the absolute path of the
real `codex.exe`, not the `codex.cmd` shim. Saved delegation prompts live
in `handoff/`.

## 3. What the application is

A native Windows desktop tool (Rust 2024, `eframe`/`egui` 0.32, `nom`
parser) that removes the tedious wiring from building **IL-2 Sturmovik:
Great Battles** missions on the **Korea** map. It reads and writes the mission
editor's `.Group` text files; it does not replace the editor. Version
`0.7.0`; ~40k lines in `src/`. `ui.rs` is the GUI anchor; Template Builder is `src/ui/builder.rs`; Map is `src/ui/map.rs`. `template.rs` is the group builder (~7.9k).

Pipeline: `.Group` text → `parser.rs` (nom) → `ast::Il2Entity` tree (unknown
keys preserved) → generation modules transform/clone the tree →
`serialize.rs` writes it back (CRLF, lossless round-trip) → `locale.rs`
writes the UTF-16 translation sidecars (`.eng`, `.rus`, …).

### The six modes (tabs)

| Mode | What it does | Core modules |
|---|---|---|
| **Template Builder** | Builds one self-contained, proximity-triggered unit group: seats (planes/vehicles/trains/ships), a tree of orders (formation → waypoint → attack → complete), Zone IN/OUT checkzones, spawn/cooldown, cleanup on zone-out. | `template.rs`, `payloads.rs`, `model_spec.rs`, `weapon_range.rs` |
| **Fighter Pack** | Clones a fighter group N times and wires the copies with NodeGates so they take turns spawning. Includes a randomizer, pair skills, finger-four altitudes and tail codes. | `pack.rs`, `flights.rs`, `aircraft.rs` |
| **Exclusive Activation** | Links several pre-built checkzone "plans" so only one fires (mutex). | `bombers.rs` |
| **Army Generator** | Clones ground/ship/train templates and adds a per-type randomizer that decides at mission load which copies spawn. | `recon.rs`, `mapground.rs`, `mapnet.rs` |
| **Map** | Dated Korea front line from a 1950–53 timeline, AO box, salients, arrows, influence areas. Auto-places fighters, ground units, ships, trains and convoys on terrain, roads and rails. Exports a base-map `.Group`. | `frontlines.rs` (+`timeline.rs`), `mapclip.rs`, `mapfighters.rs`, `mapground.rs`, `mapshipping.rs`, `mapnet.rs`, `watermap.rs`, `geo.rs` |
| **Airfield** | Strips single-player clutter from an SP airfield so it is MP-ready. | `airfield.rs` |

`MapHelper/` is a separate small Rust CLI. It pre-bakes
`assets/combined_terrain.bin` (water/road/open bitmask) from map images.
`py -3 tools/mapcapture/bake_woods.py` then sets the woods bit from editor tiles.

### Current state (verified 2026-09-23)

- All six modes are implemented and wired in the UI.
- `cargo test`: **371 pass, 0 fail**.
- `cargo build`: **70 warnings**, mostly dead code: unused constants,
  `mapload::PointKind`, and pack helpers marked `#[allow(dead_code)]`.
- `mapload.rs` is compiled and tested but **not called from the UI**. Map
  labels use hardcoded `geo::cities_on_map` instead.
- The aircraft identity tables (`aircraft.rs`) cover the 7 Korea fighters only.
- The UI is English-only.

## 4. Next development phases

**Author-stated** (README "Todo"):
- **Rewrite `USER_MANUAL.md`** into plain, human-readable text. The current
  manual is dense and reads as AI-written.
- **Localization** of the app UI. `locale.rs` handles mission sidecars only;
  UI strings are hardcoded in `ui.rs`.

**Proposed by Claude.** These are inferences from the code and need the user's confirmation.

| # | Phase | Why | Size |
|---|---|---|---|
| P1 | **Template altitude defaults** (in progress, see §5) | User request, 2026-09-23. | S |
| P2 | **Hygiene:** clear the 70 warnings (wire or delete dead code); add a GitHub remote | Warnings hide real regressions. A remote unlocks the standard broker flow. | S |
| P3 | **Wire `mapload.rs`** reference catalog (airfields/buildings from `References/`) into Map-mode labels and snapping | Already built and tested, but unused. | M |
| P4 | **Split `ui.rs`** (10k lines) into per-mode panel modules, with no behavior change | Prerequisite for localization. Lowers merge risk for every UI task. | M–L |
| P5 | **UI localization:** string table plus a language picker | Author todo. Easier after P4. | L |
| P6 | **Manual rewrite** (author todo) | Should follow the feature changes so it does not go stale twice. | M |
| P7 | **Template altitude follow-ups** (see §5) | All done or closed (2026-09-23). | S |
| P8 | **Features the historical templates need.** Details and acceptance criteria are in `handoff/R3-template-feature-gaps.md`. F1: respawn linked flights on a cooldown. F2: loader round-trip for every spawn layout. F3: break off after N losses. F4: cross-template trigger hooks. F5: date-aware aircraft warning (La-11 in 1950). F6: headless generation via lib/CLI. F7: waypoint speed without the UI. F8: searchlight defended-area preset. F9: per-element altitude step. | Found while building `TemplateExamples/Historical1950/` (2026-09-24). Each one forced a workaround or left out sourced behaviour. **Not scheduled; not in a 0.6 release.** | S–M each |
| P9 | **Mission logic checker** (frontier pick). A "Check" panel that takes any `.Group` and lists broken targets/object links, empty checkzones, MCUs nothing triggers, timer-less loops, and spawn/cooldown settings that will be ignored. A dry-run trigger simulator follows in v0.2. Details: `handoff/R4-frontier-proposals.md`. | The editor has no debugger; today broken wiring is found by flying. `inspect_plan` only checks that a checkzone and a timer exist. F1 shows even the app's own output can fail silently. | M |
| P10 | **Playtest replay.** Parse IL-2's text mission report (ATYPE log) into a spawn timeline and Map-tab overlay; flag templates that never spawned. | Runtime half of P9. Also field-checks the heightmap (underground spawns). Needs text logging on in `startup.cfg`. | M |
| P11 | **Historical order of battle from date.** Reference §2.2/§3/§7 as data; Map-mode "Historical fill" seeds Fighter Pack / Army Generator pickers with date-valid types and writes source citations into group descriptions. | Goes beyond F5 (a warning) to generation. Uses the evidence-checked reference and `timeline.rs`. Coverage is uneven past 1950 (R2), so show a coverage indicator. | M–L |
| P12 | **Terrain-aware placement.** v0.1: terrain clearance along whole flight legs. Later: line of sight for AAA/searchlights (§8.9), reverse-slope CCF positions (§5 D). | Blocked on the heightmap. Store heights as a queryable grid, not only a relief image. Settles the heightmap "Revisit" on AGL waypoints. | M |
| P13 | **Semantic `.Group` diff.** Show what changed between two mission versions (added units, changed radii, retargeted MCUs), matched by index + name. | Cheap on the lossless parser. Niche unless co-authoring MP missions. First check whether the editor renumbers indices on import. | S–M |

P9–P13 came from the frontier-feature review on 2026-09-24. **All are proposals, not scheduled.** Ranked in that order; P9 and P10 together make one "debugging" phase. If most users only produce missions and rarely debug them, P11 moves to the top.

## 5. Feature log

### P1 — Auto altitude at 50% of service ceiling (Template Builder)

**Request (2026-09-23):** "allow aircraft under template mode for aircraft
altitude to automatically be set at 50% of operating ceiling."

**Design (Claude, as Fable):**
- **Logic.** `model_spec::auto_altitude_m(script)` returns
  `round(ceiling_m × 0.5)`. For example, MiG-15bis gives 7500 m, F-86A-5
  gives 7620 m, and an unknown aircraft gives 4000 m.
  `template::apply_auto_altitude(_all)` sets the seat altitude and re-derives
  `StartType`, which becomes Airstart.
- **UI.** A checkbox, **Auto altitude (50% ceiling)**, sits in the Template
  "Add unit" row and defaults to **on**. When it is on, new planes and model
  swaps get 50% of their own ceiling. Switching it on re-heights every plane.
  A per-seat **50% ceiling** button next to the altitude slider works
  regardless of the checkbox.
- **Not touched by auto.** Loaded template files, manual slider edits made
  afterwards, Fighter Pack altitudes (`flights.rs`), and "Copy attributes to all".
- **Behavior change to note.** Every catalog plane has `YPos = 0`, so new
  planes used to default to a **ground start**. With auto on (the default)
  they now airstart at half their ceiling. To build a runway or parked
  flight, turn auto off.

**Follow-ups (P7). User rulings 2026-09-23:**
- **Mixed-model formations. RULED: wingmen match their lead.** DONE, merge
  `72106da`. With auto on, a wingman takes its lead's altitude, capped at
  its own ceiling. A ground-start lead keeps its wingmen on the ground with
  the same engine state. Moving the lead's slider moves its wingmen, and
  switching a seat to Follows adopts the lead's height. The per-seat button
  reads **Match lead** on a wingman. 381 tests pass. Smoke-tested: MiG
  lead at 7500 m brings its La-11 wingman to 7500 m (not 5075 m); dragging
  the lead to 1500 m moves the wingman to 1500 m. The ground-start Engine
  choice (Running/Warm/Cold) on a lead now syncs to its wingmen too. It was
  missed at first; fixed in the next merge below.
- **Ceiling clamp on copy. DONE** (user said yes, 2026-09-23). This was a
  pre-existing bug, not a design question. "Copy attributes to all" copies the selected
  plane's altitude onto every other plane without capping it at that
  plane's own ceiling. For example, an F-86 at 12,000 m copied onto an
  IL-10 (6,950 m ceiling) gives 12,000 m. The proposed fix is a one-line
  clamp in `copy_seat_attributes`. Fixed, with 383 tests passing. Merged
  together with the engine sync.
- **Persistence. RULED: default on is correct.** Resetting to on at each
  app start is the intended behavior. No persistence is needed. Closed.

**Status: DONE, merged to `main` 2026-09-23** (`aab880c`, merge `9389c6a`).
Claude implemented it (user waiver: Astra is unavailable on the account).
The tests went from 371 to 376, and the build still has the same 70
warnings, so none are new. It was smoke-tested in the running app: adding
a B-29 gives 5334 m airstart; dragging to 0 m gives a ground start
(Running); the **50% ceiling** button restores 5334 m.

The notes below record the earlier blocker.

~~BLOCKED: Codex CLI is not installed~~, so the design was ready
but not implemented. The broker extension exists (`~/.codex-broker/`), but
there is no `codex.exe` and no `~/.codex/config.toml`. Job
`20260924024246-e38e95ca` failed at spawn with `ENOENT` and changed nothing.
The user chose to install Codex rather than have Claude implement it.
**To resume:** once `codex.exe` resolves, re-send
`handoff/P1-auto-altitude-codex-prompt.md` verbatim with `codex_start`, then
continue at delegation-loop step 3 (§2).

## 6. Session log

| Date | Who | What |
|---|---|---|
| 2026-09-23 | Claude (Fable role) | Created working copy and git baseline (`.gitattributes * -text`). Verified 371 tests pass. Wrote this handoff. Designed P1 and delegated it to Astra (job `20260924024246-e38e95ca`, branch `codex/auto-altitude`). The job failed because the Codex CLI is not installed. The user will install Codex; the prompt is saved in `handoff/`. |
| 2026-09-23 | Claude (Fable role) | Codex was installed, but the broker kept ENOENT because Claude never fully quit (the lookup is cached). Ran the CLI directly and found the default model is **gpt-6-luna**. Astra is not on the account, so I stopped the run before it made any edits. The user chose Claude to implement P1. Built on `claude/auto-altitude`, 376 tests pass, smoke-tested in the app, merged to `main`. An untracked `handoff/R1-historical-reference-codex-prompt.md` (a Korea 1950–53 unit-reference task, not written by Claude) was left untouched and was not run; it waits for the user. |
| 2026-09-23 | Claude (Fable role) | User rulings on P7: wingmen match their lead (built on `claude/auto-altitude-follow-lead`, 381 tests, smoke-tested, merged `72106da`), and auto altitude default on is correct (closed). Explained that the copy-attributes clamp is a bug, not a question; awaiting the go-ahead. |
| 2026-09-23 | Claude (Fable role) | The user asked why the engine choice did not sync: it was an oversight, since only the slider and button were hooked up. Fixed so the Engine choice on a lead syncs to its wingmen, and added the copy-attributes ceiling clamp (user said yes). 383 tests pass, and there are still 70 warnings. Both merged to `main`. |
| 2026-09-23 | Claude Opus (Fable role, parallel session) | **R1, historical reference: done, committed (branch `claude/historical-reference`).** The `handoff/R1-…` prompt came from this parallel session, not from an outside author. It was drafted for Codex, but the user dropped Codex ("misbehaving") and asked Opus to do it. Sources (9 public PDFs) were downloaded with the user's approval into `docs/historical-reference/sources/`. That folder is git-ignored except `SOURCES.md`. Futrell and the Pacific Fleet report were scans, OCR'd with the Windows built-in OCR. Five subagents extracted the data, and every Evidence line was checked by script against its cited page. Outputs: `docs/historical-reference/korea-1950-53-unit-reference.md` (10 sections, ~600 table rows) and `…-evidence.md`. Findings: archive.org's "FM 6-140" file is really FM 6-120, and the Wikimedia DA Pam 30-51 is the 1960 edition. The R1 Codex prompt is now obsolete. **Sources still needed are listed by priority in `handoff/R2-sources-still-needed.md`.** Also added reference section 8.9 on searchlight placement: 4–8 lights per defended area, set along the bomb-run approach, zone-activated, and paired with flak or night fighters, in place of 20–50. |
| 2026-09-24 | Claude Opus (Fable role, parallel session) | **Early-war 1950 air templates.** Added reference §2.2 (1950 F-51, F-80, Yak and Il-10 data: about 110 rows, 120/120 evidence lines checked). Found that **no La-9 or La-11 flew in 1950**: La-9 first appears Nov 1951, La-11 Apr 1953. The shipped Eastern random packs mix La-11, Yak-9P and MiG, which is anachronistic for 1950 dates. Built 6 templates in `TemplateExamples/Historical1950/` (see its README) with the app's own `generate_template`, using a scratch crate that `#[path]`-includes the non-UI `src/` modules plus `build.rs`. The repo code is unchanged. Every file round-trips through `load_template` with no warnings and passes `bombers::inspect_plan`. Findings: `generate_template` forces Activate when wingmen are linked, so a linked flight can't respawn on a cooldown; that needs Fighter Pack. The Template loader does not recognise the Pairs spawn layout ("~37 m off" warning), so the 2+2 templates use finger-four placement with per-group 2. Not smoke-tested in the game itself. |
| 2026-09-24 | Claude Opus (Fable role) | Frontier-feature review. Added proposed phases **P9–P13** to §4 (logic checker, playtest replay, historical OOB from date, terrain-aware placement, semantic diff), with acceptance criteria in `handoff/R4-frontier-proposals.md`. They don't overlap P2–P8 or R3 F1–F9. No code changed. |
| 2026-09-24 | Claude Opus (Fable role) | **UI redesign phase 1 (theme + shell), branch `claude/ui-shell` (`b5aac43`), merged to `main` as `fb26e94` (not pushed).** The source is the Claude Design handoff `UI mockups form survey.zip` (repo root, untracked); its spec is now at `docs/ui-redesign/README.md`, with 6 phases in §9. Ported `theme.rs`/`shell.rs` to egui 0.32. Barlow TTFs were downloaded with the user's OK into `assets/fonts/`. The UI now has a rail, a header with Generate (Ctrl G) and Load/Add (Ctrl O), per-tab scrolling bodies and a status bar. 383 tests pass, 70 warnings, clippy clean on new code. All 6 tabs were captured via PrintWindow. **Not click-tested:** another session held computer-use, so Ctrl 1–6, rail clicks and header buttons are untested in the running app. Next: phase 2 (Template, §5.1). |
| 2026-09-24 | Claude Opus (Fable role) | **UI redesign phase 2 (Template tab), branch `claude/ui-template` (`7f12739`), merged to `main` as `1f095ca` (not pushed).** Three-panel layout per README §5.1: add units and drag-reorder unit cards on the left; the formation view (light grid, Zone In solid / Zone Out dashed, zoom group) over a 236 px order tree in the center; the selection plus collapsible settings on the right. Order/event add moved from each tree row to the tree header (acts on the selected unit). Verified: the 6 `Historical1950` templates generate byte-identical files on `main` and on the branch (temporary env-var hook, not committed). 383 tests, 70 warnings. **Not click-tested:** the user declined computer-use this session, so drag-reorder, + Add, and the menus are unverified in the running app. Deferred to phase 5: DPRK/NATO labels (`ZoneCoalition::label`), Reset confirm, undo, moving long help texts. Side markers use the fallback shape, not the SVG textures. |
| 2026-09-24 | Claude Opus (Fable role) | **UI redesign phase 3 (Map tab), branch `claude/ui-map` (`c3b00ee`), NOT merged.** Layout per README §5.6: 56 px tool palette (keys 1–6, Undo/Redo), the map filling the center with banner, AO readout, legend chip and zoom group, a 64 px front-date strip, and a right dock with Period / Forces / References tabs (`MapDock`). Tool safety (§6.2): Esc cancels, objectives are one-shot unless Shift is held, leaving the tab resets the tool, and the status bar shows the tool hint. Verified: a fixed period + AO + arrow + objective generates a byte-identical base map on `main` and on the branch (temporary hook, not committed). 383 tests, 70 warnings. **Not click-tested** (no computer-use); keys 1–6, Esc and Shift-place are unverified in the running app. Map drawing colors (front, AO box, sides) are unchanged; restyling them is not in the spec's phase list. |
| 2026-09-24 | Claude Opus (Fable role) | **UI redesign phases 4–6, branch `claude/ui-final` (`39062bc`, `71499af`, `dfdf764`), stacked on phase 3 (`c3b00ee`); none merged.** P4: Army Generator, Fighter Pack, Exclusive Activation and Airfield rebuilt as side panels + center (README §5.2–5.5). The Fighter preview uses the new `flights::preview_flights`, with a test tying it to `build_seats` (384 tests). P5: DPRK/NATO and Zone In wording in user text; names written to or matched in `.Group` files keep "Eastern"/"Zone IN" (commented at each site). Per-tab undo (Template, Army, Exclusive, Map) on Ctrl Z and the status bar. Confirm dialogs on Reset and on Load over unsaved edits. Manual labels updated. P6: no text under 12 px; small buttons, move arrows and links have 28 px targets. Verified: no generate/export/load function changed (text compare of 38 fns); a headless hook checked that undo restores identical state; captured every tab. Release build OK. **Not click-tested** (no computer-use). The README §10 readiness checklist is still backlog. |
| 2026-09-24 | Claude Opus (Fable role) | **Harvester + terrain integrated into the redesign; headless UI tests.** Merged `main` (harvester `3ea0053`, terrain `06fa1b3`) into `claude/ui-final` (`289e351`). Harvester controls now sit at the top of the Airfield left panel, with its log in the center. New Map dock tab **Terrain** (`1f0328b`): store status, Export survey pass / AO tiles (100 m), Import snapped (refuses unsnapped) / Learn from mission, Coverage and Relief layers, pointer height readout. No export toggle yet (terrain phase 2 is not built). The 3-pass plan's 200 m / rough-100 m generators don't exist in `heightprobe` yet; add buttons when they do. **Testing:** `src/ui_tests.rs` drives the app headless (AccessKit labels, synthetic clicks/drags/keys, file dialogs from a queue): 22 end-to-end tests over every tab. 434 tests pass, twice in a row. Live click-through **not done**: the computer-use screenshot timed out twice (another session likely held the screen). Nothing pushed; `claude/ui-map` + `claude/ui-final` still unmerged to main. |
| 2026-09-24 | Claude Opus (Fable role) | **Handoff for the next session: `handoff/U1-ui-redesign-test-and-fix.md`.** It covers the live click-through checklist (only Fighter Pack done), the known issues, and the merge-and-push steps (approved once the live run is clean). The live driver is `tools/ui-live/drive.ps1`: real input and PrintWindow capture, because the computer-use screenshot hangs here. |
| 2026-09-24 | Claude Opus (Fable role) | **UI redesign finished on `claude/ui-final` (`85ca06e` + docs), NOT merged or pushed: the user wants to check it first.** Audited every tab against README §4–§8 and the mockups (≈75 gaps), then fixed them in three parallel worktrees (Template+Army `33f87dd`, Fighter/Exclusive/Airfield `88d2153`, Map/side markers/shell/Help `8c2c84a`). Added the README §10 readiness chip (`b855a2f`: Generate disabled until each tab's minimum is met), stale-undo guard, clickable section headers, flight colour names, per-tab status, Map front/clear undo, painted tool icons, dock tabs, fighter side markers (ring removed, cropped), Keyboard shortcuts help. **Live click-through done** with `tools/ui-live/drive.ps1` + new `dialog.ps1` (native file dialogs): every tab, Generate via Ctrl G + save dialog, Load/Add via dialogs, undo/confirm, map tools, Terrain layers + readout, Help Esc, 1280×800. Live bugs fixed in `85ca06e` (missing ✓ glyph, Airfield panel overflow + black gap, COPY MIX clipped, chip names cut, wrapped button), each with a guard test. 466 tests, 69 warnings. Only message strings and two count fields changed outside the UI; generation untouched. User decisions: skill 3 reads **Veteran** (Plain/Low/Normal/Veteran/Ace); Template translation sidecars **on hold** (Army's warning stays for now); `claude/terrain-apply` stays out of this merge, its toggle to default off. |
| 2026-09-24 | Claude Opus (Fable role) | **UI polish merged to local `main` (`e796a3c`), not pushed.** Exclusive undo reselects the restored plan; Terrain counts digit-grouped; Fighter flights stripes full width; status info/error messages retire after the next edit on that tab (`age_status`); order tree gets a visible solid scrollbar; Map AO/height readouts fade under the pointer. Verified live. 468 tests, 69 warnings. |
| 2026-09-24 | Claude Opus (Fable role) | **All branches merged to `main`; tagged v0.6.** `claude/historical-templates-1950` (+ `historical-reference`) were stale pre-rebase copies whose content was already on `main` (`fc8e98f`, `989b3fe`); merged to close them. `claude/terrain-apply` (terrain heights phase 2) merged onto the redesigned UI: conflicts in the Terrain tab and the Exclusive generate path resolved; **Apply terrain heights on export now defaults to off** (user decision; superseded 2026-09-26, it defaults to on), test added; manual and src-guide updated. 476 tests, 69 warnings. |
| 2026-09-25 | Cursor (Grok) | **HeightHelper CLI.** `--height-run` drives the existing probe batches (default: 5-minute dry run, timestamped log, no keys). `--shard K/N` splits the pass across machines. `--live` writes `korea_mK.hgt`. `--height-merge` joins those into one `HeightStore`. |
| 2026-09-25 | Cursor | **HeightHelper dialog filter.** `counts_as_file_dialog` ignores a `#32770` whose title contains "Mission Properties". The unsaved title `IL2 Series Editor - <empty> *` is left alone. One live `--limit-test --chunk 512` survey batch: import dialog 0.043s, editor load 0.081s, set to ground 0.810s. Import file `HeightHelper\import\batch_in.Group` is 123,279 bytes, 512 probes. Failed after save: `batch_out.Group was not written within 45s`. `done_m0.txt` was not written. |
| 2026-09-25 | Cursor | **HeightHelper select wait.** One batch is now import, click the map view, Ctrl+A twice with a UI-thread settle after each, set to ground, export the same selection, delete. The File menu stays closed until the last select finishes. No second select before export. Mission Properties is still ignored; `* empty` is not an error. Exe: `target\debug\heighthelper.exe`. Not live-run. |
| 2026-09-26 | Cursor | **Map mode.** Baked `assets/korea_100m.hgt` (31,110,412 bytes) so heights load with no AppData file; a newer store at the AppData path merges on top. HeightHelper stays the harvest tool. Forces Clear is now **Clear DPRK** and **Clear NATO** (fighters, objectives, units), each undoable. **Lock AO** stops empty-map drags from redrawing the box and disables Reset AO; Load base map can still restore a saved box. **Apply terrain heights on export** defaults on (the 2026-09-24 off default is superseded); the checkbox can still be turned off for one session. Map fighter icons use `EasternFighter.svg` and `NatoFighter.svg` in their authored colours (DPRK still faces south, NATO north). |
| 2026-09-26 | Cursor | **Map fighters.** Placed DPRK and NATO fighters use `EasternFighter.svg` and `NatoFighter.svg` in the rotation drawn in those files (DPRK north-west, NATO north-east). They are not turned to point south or north, and they are not recolored. |
| 2026-09-27 | Cursor | **Template Builder formations are the Korea editor list.** `FormationType` is the Formation Advanced combo index (planes 0–18, vehicles 19–33). Plane default is Heavy - Wedge (7); vehicle default is On Road Column one-way (19). Flare color is Red / Green / White. Attack area labels already matched the editor. |
| 2026-09-28 | Cursor | **Order tree moves by column.** Left/right arrows move a chip one visual column. A stacked report or Time on Target moves by itself; a lone command takes its stack with it. Chips can be dragged onto another column. The order panel no longer re-snaps a moved report every frame. |
| 2026-09-28 | Cursor | **Template Builder moved to `src/ui/builder.rs`.** `ui.rs` stays the anchor (`mod builder`; no `src/ui/mod.rs`). The other five modes are still in `ui.rs`. Same behavior. |
| 2026-09-28 | Cursor | **Order-tree drag restored in `src/ui/builder.rs`.** The split had put the arrows back to a list swap, which snapped a report onto its command. Arrows and chip drag again move one visual column (`shift_tree_order`). The order panel snaps a report only when its kind changes. |
| 2026-09-28 | Cursor | **Template layout is a palette, an order of battle, and an inspector.** The left panel is the model catalog only (Add to Template, or drag a row into the formation). Units live in the bottom order of battle; clicking one selects it. Right-click duplicates, deletes, or adds an order. Change Model is in the inspector. With nothing selected, the inspector shows placement, zones, and waypoint settings, and the formation view gets the space above a 360 px order of battle. |
| 2026-09-28 | Cursor | **Unit preview sits under the formation view.** Same left inset as the formation. It shows the highlighted catalog model before any unit is added, and the selected unit after that. The formation keeps the full center width above the strip. |
| 2026-09-28 | Cursor | **Place and Activate or Spawn stay available while a unit is selected.** They are collapsible drawers under a short guide, not a card that replaces them. Each unit in the order of battle has a picture and a plain line (an Il-10 reads as a ground-attack aircraft, with its cruise speed). The inspector repeats that picture with type, cruise, and ceiling. The formation view is the full center again; the order of battle stays 360 px. |
| 2026-09-28 | Cursor | **Selected unit edits sit on the formation view.** The card uses the view's 14 px left margin. It shows the picture, the plain description, and that unit's options (or the highlighted model before one is added). The planes draw in the open area to the right. Place, Activate or Spawn, and Waypoints stay in the right-hand drawers. |
| 2026-09-28 | Cursor | **Unit picture and description sit above Models.** The order of battle keeps each unit's picture and drops the descriptive line. The formation card keeps that unit's options only. |
| 2026-09-28 | Cursor | **Map mode restored and moved to `src/ui/map.rs`.** Baked heights stay on `terrain::open_store` (`assets/korea_100m.hgt`, 31,110,412 bytes); a missing AppData file is not an error, and a different file merges over the bake. Forces **Clear** is **Clear DPRK** and **Clear NATO**. **Lock AO** sits with the zoom controls. **Apply terrain heights on export** defaults on (the 2026-09-24 off default stays superseded). Placed fighters use `EasternFighter.svg` and `NatoFighter.svg` as authored, paint angle 0. |
| 2026-09-28 | Cursor | **Version 0.7.0, tagged `v0.7-Alpha`.** Forces fighters use **Clear DPRK** and **Clear NATO** (the single Clear wiped both sides and failed the release test). The tag push builds the Windows exe and attaches it to the GitHub release. |
| 2026-10-02 | Cursor | **Harvest accepts a game-escaped quote.** `References/_gen.Mission` failed because the player F-51D tail code is `TCode = "   ""&";` and the parser treated the second `"` as the end of the string. Quoted values now keep `""` as a literal quote. The file harvests K-27 Yonpo (130 taxi nodes, 561 blocks). |
| 2026-10-02 | Cursor | **Air start fake fields.** The Airfield tab keeps a bank of fakefield air starts (coalition, facing, aircraft options) and can export them as one group or one file each. Map Forces places copies; drag moves them and right-drag turns the arrow icon. |
| 2026-10-02 | Cursor | **Air start plane count.** Each aircraft has `Number`. It starts unlimited (`-1`), as in `Airstart.Group`. Turning Unlimited off sets how many of that type the field can launch. |
| 2026-10-02 | Cursor | **Taxi-graph flags.** `Runway = 0` marks a runway node (Type 1 centerline). `RunwayEnd = 0` marks a threshold. Type 3 nodes are the ramp pads. A harvested fakefield can be moved onto a ramp and turned down the runway on K-13. K-14 and K-16 keep their Type 3 row beside the middle of the runway, so one automatic placement rule does not cover every field. |
| 2026-10-02 | Cursor | **Taxi-graph runway flags.** `Runway = 0` marks a runway node and `RunwayEnd = 0` marks a threshold. The old note that a zero meant "not a runway" is removed from `harvest.rs`. `AxisHeading` is still the principal axis of every taxi node. |
| 2026-10-02 | Cursor | **Field spawn.** `place_field_spawn` puts a harvested fakefield just short of the threshold, nose toward the runway, with `StartType` 1 and altitude 0. The Airfield tab calls it **Field spawn** / **Export field spawn…** and reuses the aircraft list. It is not an air start. Harvest defaults are the Korea Missions folder and `Template\MP Airfields`. |
| 2026-10-02 | Cursor | **Airfield UI moved to `src/ui/airfield.rs`.** `ui.rs` stays the anchor (`mod airfield`; no `src/ui/mod.rs`). Page, panels, harvest, field spawn, and the air-start bank moved. Same behavior. |
| 2026-10-02 | Cursor | **Field spawn has its own aircraft list.** Engine running writes `fakefield_rnwspawn.mgm` and `fakefield_rnwspawn.txt`, `StartType` 1, altitude 0, on the existing hold-short. Engine off, parking writes `StartType` 2 and `SnapTo` 2 and leaves the field there. Air starts stay on `fakefield.mgm` and `StartType` 0. |
| 2026-10-02 | Cursor | **Field-spawn plane names.** Each field-spawn `Plane` `Name` states the start and the load (`At ramp - Clean`, `Parked - Long range`, `On runway - Strike 2`). Air starts keep an empty name. StartType, SnapTo, the hold-short, and the fakefield model are unchanged. |
| 2026-10-02 | Cursor | **Map fighter placement.** Two waves of a pack get equal counts and cover the coalition zone, including narrow strips. Fill AO keeps Zone In spacing across the whole zone and still splits the waves evenly, up to 8 packs. Forces can load a saved fighter pack and place it with the fighter icons. |
| 2026-10-04 | Cursor | **Road columns use the editor's formation ids.** On Road Column one-way is `FormationType` 4 and two-way is 18 (`VehicleFormationTypes.Group`, `DPRK Truck Run.Group`). The 2026-09-27 combo-index values (19 and 20) are aircraft Pairs and Wedge, so a convoy saved that way does not stay on the road. |
| 2026-10-04 | Cursor | **Formation audit.** Commit `af36b61` is the only order-id rewrite. Aircraft formations are still the combo list (default 7); editor files save Heavy Wedge as 23 and Pairs as 19 (`FormationTypes.Group`). Panic Stop and Continue Moving are still 26 and 27; `K49 DPRK_AB.Group` saves them as 10 and 11. Vehicle commands 21–33 have no editor file. `Payloads.txt` was not in that commit (last change 2026-09-11). Flares match the editor (Red 0, Green 1, White 2). |
| 2026-10-04 | Cursor | **Template reload drops road and rail coordinates.** `load_template` keeps no X/Z. Generate writes a formation at 40000, 40000 and waypoints due north of that. Map's polyline snap exists only in the file until the next Generate. Write precision is 0.001 m. |
| 2026-10-04 | Cursor | **Train placement is already on the rail line.** A train is one locomotive; cars are scripts. The K49 editor train sits 9 m from `railroads.svg` (nearest chord 684 m). Map writes the locomotive on that line to 1 mm. Consecutive rail segments meet at a median 9° corner, and the heading follows one segment. |
| 2026-10-04 | Cursor | **Trains park between rail nodes.** `place_route` puts a locomotive at the midpoint of a segment instead of on a corner. Road columns still prefer vertices. |
| 2026-10-05 | Cursor | **Trains park on rail nodes again.** The locomotive sits on a corner and its heading bisects the two segments there. Waypoints stay on the segment midpoint. |
| 2026-10-05 | Cursor | **Truck columns bisect road corners.** A lead on a road vertex aims halfway through the bend, the same rule as trains. Waypoints stay on their authored offsets. |
| 2026-10-05 | Cursor | **Trains take Y from the height map at the parked XZ.** `park_route_copy` wrote X, Z, and heading and left the template YPos. A parked train now gets the same ground placement height other ground units get. |
| 2026-10-05 | Cursor | **Template + Mission.** Left of + Order, aircraft leads can take CAP (low-priority diamond, loop until bingo guns, bingo fuel, critical damage, or wounded, then a high-priority RTB) or Strike. A new aircraft with no prototype height starts at 1500 m airstart. |
| 2026-10-09 | Cursor | **Goto timer names.** A hop to waypoint N is "Goto WP N". A loop back is "Return to WP M". The order chip uses that name. Load takes the seat from the waypoint's unit, not the trailing number. |
| 2026-10-09 | Cursor | **Map strike targets.** Exclusive Activation is for profiles built outside the utility. Template strikes pick a unique enemy target on the map. The priority list and the objective pull-down live under Missions. Stacked Zone In checkzones separate by 100 m. |
| 2026-10-09 | Cursor | **Strike jumps to a target.** Add mission places the flight on the next unused target of one default type (tanks, trucks, rail, ships, airbases, bridges) and centers the map. AFB groups and bridge scripts are targets; air starts are not. Shift-drag snaps, and the map banner says so. |
| 2026-10-09 | Cursor | **Landscape bridges and taxi runways.** Bridge targets are `graphics\bridges\bridge_` span models (not dams). A crossing's spans collapse to one target. An airfield is the midpoint of the taxi graph's runway ends. |
| 2026-10-09 | Cursor | **Dams are their own strike class.** `graphics\bridges\dam_` pieces are targets under Default target → Dams. Pieces of one named dam are a single target. |
| 2026-10-09 | Cursor | **Strike snap stays on the map.** The per-mission target list is gone (it hung on every bridge). Add and Shift-drag only use targets inside the AO. The snap list is built once when the drag starts. |
| 2026-10-09 | Cursor | **Snap target, and Attack Ground for structures.** The Missions control is Snap target. A bridge, dam, or airbase exports Attack Ground only. An ordinary target drag does not scan the landscape; Shift builds the snap list once. The scan keeps one class, skips objects outside the AO before clustering, and is cached for the next Add or Shift-drag in the same AO. |
| 2026-10-09 | Cursor | **Strike IP is 8 km.** The initial point is placed 8 km from the target, and a map drag cannot bring it closer. The longer run gives the flight room to line up on the AttackArea. |
| 2026-10-09 | Cursor | **Several missions share Exclusive Activation.** Generate mutexes two or more placed strikes so only one plan runs. A group with more than one flight and more than one AttackArea can aim each later flight at its own target; the spawn, IP, and waypoints stay one plan. |
| 2026-10-09 | Cursor | **Strike import and export.** Loading a strike keeps Formation, IP, AttackArea, OnAreaAttacked, home Goto, Mission Complete. AttackArea exports at high priority. Map export turns on Attack Ground Targets when the target is enemy tanks, trucks, trains, or boats, and warns when that flag was off. |
| 2026-10-09 | Cursor | **Map mission drags.** The strike spawn moves on its own. The IP cannot be placed closer than 2 km to the target. The formation faces the IP, and the route legs carry small arrowheads. |
| 2026-10-06 | Cursor | **Map Missions.** Forces has a collapsible Missions section. A strike group can be placed and dragged on the map: the flight moves the whole route, the target and the IP reshape it, and the formation faces the target. Generate Base Map writes that route (WP 1, WP 2, Zone In). |
| 2026-10-05 | Cursor | **Strike package.** V-Form, medium IP about 6 km off the inbound by 35°, 3 km ground AttackArea for 2 min, then one high-priority Goto home and Mission Complete. Abort events (critical damage, wounded, bingo bombs, bingo fuel) Then that same Goto. Planes spawn at the home point: due north for DPRK, south-east for NATO, 20 km out until Exclusive Activation has an AO. Strike turns Zone Out off so a failed opposition does not end the mission. Exclusive Activation lists target priority (tanks, truck routes, ships, airbases, bridges) and, when the map has an opposite-coalition group inside the AO, centers Zone In on it and puts the aircraft on the AO edge. |
| 2026-10-10 | Cursor | **Placement and the map overlay use `highways.ini` and `railroads.ini`.** The SVGs are no longer the network. Bridge spans stay on the centerline, and the map still draws roads and railroads for placement and planning. |
| 2026-10-10 | Cursor | **Aircraft FormationType uses the editor ids.** Strike Wedge is 20, Pairs is 19, Heavy Wedge is 23. Combo indexes 0 and 4 crash the editor when that command is opened. Vehicle on-road columns stay 4 and 18. The mission editor's infantry graphics check is outside this utility; reference groups are not rewritten for it. |
| 2026-10-10 | Cursor | **Korea aircraft FormationType follows the editor dropdown (0 crashes), density is loose/safe/dense by role.** V-Form is 1. Fighters use Column - Pairs (4) loose, attack aircraft use Column - Flights wedge (5) safe, bombers use Heavy - Wedge (8) dense. Vehicle on-road columns stay 4 and 18. |
| 2026-10-10 | Cursor | **Strike Time on Target is wired again.** The IP waypoint pulses AttackArea and a 2 min Time on Target together. OnAreaAttacked still starts the home Goto, and Time on Target does too when that report does not fire. Adding Time on Target from the order menu stacks it on the attack instead of leaving it unlinked after Mission Complete. |
| 2026-10-10 | Cursor | **Strike STRIKE END timer.** MISSION BEGIN starts it when the group activates. It pulses MISSION END after the route time plus the attack window plus 3 min. Placing the strike on the map recalculates that time. |
| 2026-10-11 | Cursor | **Map unit labels and waypoint selection.** Placed fighter, ship, and ground numbers, and waypoint captions, draw in orange. Selecting a waypoint still rings that waypoint in yellow, and now rings its unit and the rest of that unit's waypoints in the same stroke, in orange. |
| 2026-10-11 | Cursor | **Strike icons on the map.** A placed strike draws `Eastern_Strike.svg` or `NATO_Strike.svg` at the flight, as authored, in place of the filled circle. The heading tick starts outside the icon. |
| 2026-10-11 | Cursor | **Unit numbers and loaded strikes.** Fighter, ship, and ground numbers sit just outside the lower-left of the icon, right-aligned so a second digit stays off the artwork. Loading a base map restores a plane group that has an AttackArea and Mission Complete as a strike mission. It is no longer classified as artillery. |
| 2026-10-11 | Cursor | **Truck margin, fighter orbits, route highlight.** Automatic truck routes prefer a road at least 10 km inside the AO when one fits, and fall back to the full AO otherwise. Trains are unchanged. A fighter pack draws a thin dashed circle in its faction color at the AttackArea MCU radius. Dragging a truck, or clicking a waypoint, keeps that unit and its waypoints highlighted until another click. |
| 2026-10-11 | Cursor | **Waypoint drag ends reposition.** Releasing a dragged waypoint leaves it where it was dropped. A single click still waits for the next click to place it. The orange group highlight stays until another click or Esc, and Esc also leaves reposition mode. |
| 2026-10-11 | Cursor | **Fighter orbit ring.** The dashed AttackArea circle is the side colour mixed 40% toward white, so it reads lighter on the terrain. |
| 2026-10-11 | Cursor | **Strike home Goto no longer pulses Force Complete.** Goto WP 2 targets the waypoint only. Arrival pulses Mission Complete, and MISSION END runs Force Complete after the flight is home. Firing both together cancelled the waypoint and the aircraft circled. |
| 2026-10-11 | Cursor | **Removed STRIKE END.** The flight-time timer started at activation and pulsed MISSION END before a slow strike reached the target. Exclusive Activation still reopens only when MISSION END fires. |
| 2026-10-11 | Cursor | **Exclusive 3 hour window, and ALL DESTROYED.** NodeGates `{n} WINDOW` starts when a plan triggers and pulses that plan's end timer after 3 hours. The end timer deactivates the window when the plan finishes first. CAP and Strike add OnPlaneDestroyed → All destroyed on each attacker. That counter sits with flight cleanup and pulses MISSION END once when every attacker is gone. |
| 2026-10-11 | Cursor | **Map unit text.** Fighter, ship, and ground numbers, and waypoint captions, are light gray at partial opacity again, with the existing dark shadow. Positions are unchanged. Orange stays on the selection rings. |
| 2026-10-11 | Cursor | **AO resize undo.** An unlocked empty-map drag that redraws the AO, and Reset AO, join the map undo. Ctrl Z restores the previous box. |
| 2026-10-14 | Cursor | **Korea 1.005 logic (issue 1).** Death counters and ALL DESTROYED listen for OnKilled. Fighter Pack sends that event to the plane's Delay Delete so a kill counts once. Spawn Trigger Delete lists entity indexes; Activate omits Delete. CAP and Strike drop bingo fuel, and abort events share a once-only Go Home Once counter. 543 tests pass. |
| 2026-10-25 | Cursor | **Map provisions.** The References tab takes a template folder and imports the nine landscape groups (not airfields) into every base map. Harvested groups in `Template\MP Airfields` are imported too, and the Airfield tab lists them as separate airbases so each can have its own field spawn. |
| 2026-10-25 | Cursor | **Ferry ships.** `ferrym2` and `ferryn2p` from `TemplateExamples/ferry_bridge.Group` are ship units in the catalog (M2 ferry, NATO; N2P ferry, DPRK). |
| 2026-10-25 | Cursor | **Airbases in the map area.** The Airbases list leads with fields inside the map box plus 10 km. A field in that area with no runway group, or a catalog row whose group file is missing, is marked Data required. |
| 2026-10-25 | Cursor | **Airbases by side of the front.** Fields in the map area are listed under DPRK or NATO. DPRK is north of the active front, NATO is south, and a detached pocket stays with the side that holds it. |
| 2026-10-28 | Cursor | **One airfield group per coalition.** A base map places the harvested file whose country matches that field's side of the front, and leaves the other country's copy on disk. The Airfield tab and Map References list the Korea fields inside the AO that still need a harvest for that side. |
| 2026-10-28 | Cursor | **Ramp start on every coalition.** A harvested airfield moves to the hold-short on its own taxi graph when it is loaded onto a base map, for USA, USSR, DPRK, and PRC. A field with no taxi graph or fewer than two runway ends stays where it is and is reported as missing a ramp start. |
| 2026-10-31 | Cursor | **Editor map capture.** `tools/mapcapture/capture.py` photographs the mission editor at the zoom on screen and stitches one georeferenced image. `init` presses Alt+F9 twice and then stops so the view can be framed. Open water is filled, not photographed. Output is `References/MapCapture/` (git-ignored). |
| 2026-10-31 | Cursor | **Move Camera without restoring the window.** The capture script was calling Windows restore, which shrinks a maximized editor. It now leaves a maximized window maximized and posts View → Move Camera to (command 32844, the Ctrl+M item). Checked live: the dialog opened, Cancel closed it, the window stayed maximized, and the camera was not moved. X is the top field, Z the bottom field. |
| 2026-10-31 | Cursor | **Status bar coordinates.** The map-corner crops were contour lines. Scale now comes from the status bar (`Mouse(m)` and `Camera`). On the saved frames that reads camera 250000, 250000 and 250000, 253000, zoom 1.292. |
| 2026-10-31 | Cursor | **Move Camera applies the next tile.** The first tile reached X 4265, Z 6785, then the next dialog closed on that same point. The fields are checked before OK, the editor is brought forward again, and a move that does not change the picture is tried again. |
| 2026-10-31 | Cursor | **Already-there camera.** A grab that never changes pixels was waiting out the timeout, and Alt-tab made it look like the camera had moved. If the picture stays put, the status-bar camera is read, and a match is kept. |
| 2026-10-31 | Cursor | **Move Camera after a status read.** The status-bar OCR opened a PowerShell window, so the next OK closed on the old point (4265, 67850 stayed, 80062 never applied). OCR is now a hidden window, and the dialog commits the fields before OK. |
| 2026-10-31 | Cursor | **Middle-map stitch sample.** The full plan starts on the empty south edge. `capture --sample` writes a 2 by 2 around the map center to `References/MapCapture/sample`. |
| 2026-11-01 | Cursor | **Editor tiles in the map.** Close zoom draws the captured PNGs instead of the 7 GB TIFF, up to 48×. Ground placement treats olive as woods and beige as a field when those tiles are present. Water still comes from the coarse mask. |
| 2026-11-08 | Cursor | **Map strikes see airbases.** A strike aimed at airbases uses the Airfield tab's harvest, one copy per field, the country on that side of the front. Map mode no longer asks which copy to keep. |
| 2026-11-08 | Cursor | **Unarmed airbase strikes strafe.** A map strike sent to an airbase turns on Attack Ground Targets when the aircraft have no bombs. A flight with bombs, and a strike on a bridge or dam, stays Attack Ground. |
| 2026-11-01 | Cursor | **Base-map air starts.** Load base map restores fakefield air starts (position, facing, aircraft) onto the map and into the Airfield bank. Defined airfields in the file keep their aircraft lists on the Airfield tab. An air-start group is no longer imported as a ground unit. |
| 2026-11-01 | Cursor | **Base map copies and airfield layout.** A saved base map no longer reimports landscape or airfield groups that the utility already provisions. Airfields outside the area plus 10 km stay out of the export, including their logic blocks. Harvested fields and air starts export under `Airfields` / DPRK or NATO. The Airfield tab lists air starts with the airbases. Two in-area copies of one K-site open a dialog to remove one or keep both; the harvest file stays on disk. |
| 2026-11-01 | Cursor | **Airfield list headings.** Every placed air start is a row on the right, under DPRK or NATO with the airbases, including a second copy of the same name. Instructions are one note under Airbases. Side headings no longer carry their own instructions. |
| 2026-11-02 | Cursor | **Independent air starts.** Each placed air start keeps its own aircraft. Selecting one no longer highlights the other copy or edits a shared list. Airbases outside the map area are left off the Airfield list. |
| 2026-11-02 | Cursor | **Airbase list and fuel defaults.** A preloaded airbase can be removed from the list without deleting its file, then brought back with Restore preloaded or added from a file. New aircraft start with a full fuel load. The F-51 stays at 65%. |
| 2026-11-02 | Cursor | **Copy aircraft between airfields.** Each aircraft card has Copy aircraft. Copy all and Paste sit under the list on a field spawn and on an air start. Paste appends the copied aircraft and leaves the copy on the clipboard. |
| 2026-11-02 | Cursor | **Airfield tab owns base-map airfields.** Map export no longer auto-loads harvested airfields into the area. It places the airfields listed on the Airfield tab, and it drops airfield groups that arrived as map references. |
| 2026-11-02 | Cursor | **Map missions get Exclusive Activation.** Adding a profile that already holds several strike plans splits them onto separate targets. Generate writes an Exclusive Activation group whenever two or more plans are on the map, including profiles whose start zone is named like Zone In 1 and whose end timer is Mission Complete. |
| 2026-11-02 | Cursor | **Anti-air icons and placement.** Army Generator and Map use `EasternAA.svg` and `NATOAA.svg`. A pure flak or light-AAA template is AA. Place parks those batteries in clearings around enemy objectives, enemy strike targets, or a concentration of friendly units. |
| 2026-11-02 | Cursor | **Dead-end column waypoints.** When a two-waypoint truck column or train cannot reach the Zone In rim on both sides, one hop stops at the end of the spur and the other runs to the Zone Out rim along any connected road. |
| 2026-11-07 | Cursor | **Strike package cleanup.** Bingo, critical damage, and wounded no longer abort the flight. Only OnKilled counts toward ALL DESTROYED. Time on Target follows the lead's skill and starts from a proximity over the target. FAILSAFE IP and FAILSAFE WP2 bound the two legs. |
| 2026-11-09 | Cursor | **Map same-target warning.** The Missions section warns when two or more placed flights aim at the same target point. Generate stays available. Placement and `.Group` output are unchanged. |
| 2026-11-09 | Cursor | **Shared-target sequence.** Flights on one target can be ordered. Export enlarges the earlier Zone In so it is at least 300 m larger than the next flight. A later zone is not shrunk. |
| 2026-11-09 | Cursor | **Air Tasking.** A tab queues current strike groups (Zone IN and MISSION END). Mission Begin rolls the order once, only that first Zone IN starts open, and each MISSION END opens the next slot. A 3 hour package window cleans up a plan that never finishes and does not start another. |
| 2026-11-09 | Cursor | **WP 2 proximity ends a strike.** Arriving at WP 2 fires a proximity 100 m outside the waypoint and pulses MISSION END. Goto WP 2 arms it. PULSE OUT does not. The mark shares the flight's country. |
| 2026-11-09 | Cursor | **Air Tasking, full.** Full Air Tasking features from the fork, sorties built with the current strike template. |
| 2026-11-09 | Cursor | **Air Tasking on the map.** Adding a sortie places it on the next unused enemy target inside the area and draws it with the other strikes. The tab says how the queue plays and what each setting changes. |
| 2026-11-18 | Cursor | **Airfields follow the front.** The Airfield list and base-map export drop a harvested file whose country is on the wrong side of the front. A drawn front is that line. With none drawn, the 38th parallel is the front, DPRK north and NATO south. Moving the line swaps in the other country's file. |
| 2026-11-18 | Cursor | **Base-map export clears stripped waypoint indexes.** The patched mission lists approach waypoints on the airfield entity. Export still drops those waypoints, then removes their ids from Objects and Targets before it assigns new indexes. A leftover id no longer lands on another field, which was drawing K-21's flag on Kimpo. |
| 2026-11-19 | Cursor | **Aircraft names and the area list.** Air starts and field spawns write a plane name from the start and the load (`In air - Strike 2`, `At ramp - Long range`). The aircraft card shows that name and keeps whatever is typed over it. Loading a map or mission brings a typed name back. An airbase just outside the drawn area stays off the list and out of the base map. |
| 2026-11-19 | Cursor | **F-80 payloads 52–72.** The F-80C-10 table now includes the four-store loads from the game: 500 lb, cluster, napalm, the mixed 500/1000, and the tip-tank chute flare with eight HVARs. |
| 2026-11-19 | Cursor | **Re-import keeps the unit.** A loaded truck, train, ship, or gun sits on its model, not the logic MCU beside it. Generating again without moving it writes the same positions. A fighter pack that was not moved keeps its RTB points. |
| 2026-11-19 | Cursor | **Mission Tasking.** Air Tasking is renamed and sits between Army Generator and Map. Exclusive plans and fighter packs are sections of that tab. The roster lists flights, plans, and armies. The map Forces dock shows those flights as layers that can be hidden while placing. Timer-or-players waits 0.5 s so it does not share the zone-arm tick. A base map spaces each later group's Mission Begin timers by the same 0.5 s. Airfield harvest is one rollup, field spawn is off the page, and air starts sit above Korea fields. |
| 2026-11-19 | Cursor | **Placement lives in Mission Tasking.** Fighter packs, missions, and Army Generator groups are placed from the tasking column. Place parks army groups along the front. The map Forces dock keeps layer visibility and objectives. Air starts are a tool under the objective flags. |
| 2026-11-20 | Cursor | **Airfields before Map.** The rail is Template, Army Generator, Mission Tasking, Airfields, Map. Airbase rows stay in the right-hand column. Ground placement keeps each editor-map photo's cover vote, so a second pass does not read the file again. |
| 2026-11-20 | Cursor | **No photo reads for woods.** Place uses the packed open-ground mask. Close-up map tiles decode on a worker and upload one at a time, so a pan does not wait on a picture. |
| 2026-11-20 | Cursor | **Woods bit on the terrain mask.** `tools/mapcapture/bake_woods.py` reads the editor tiles once and sets bit 8 on `assets/combined_terrain.bin`. Olive cells clear the open bit; beige cells set it. Water and road bits stay as MapHelper wrote them. Place still does one byte lookup. A cell is about 111 m, so a copse smaller than that can still read as open. |
| 2026-11-20 | Cursor | **Package first, then the mission.** Mission Tasking lists Automatic fighters, Air support library, Missions, and Units. A library or template package opens in the center. Sweep or Strike is set there, then the mission joins the rollup. A strike aims at airfields, bridges, rail stations, dams, cities, encampments, ports, mines, industry, or an imported unit, and sets the home base. The map can hide one army group, one ground side, or one sortie under the master switches. |
| 2026-11-20 | Cursor | **Edit a sortie in place.** A loaded package keeps its group name. Duplicate and Edit sit on the mission in the center. Home base lists the rear of the area and friendly airfields inside it. Loaded units use Reload position and their own type icon. Shift-drag snaps between visible targets. Each side has one air-start tool, and a start is placed only once. Map is first on the rail; the app still opens on Template. |
| 2026-11-20 | Cursor | **Tasking clicks and the unit row.** Clicking a mission's name selects it. Home base sits at the top right of the center and lists the Korea field names on the friendly side of the area, plus Rear of the area. Units is one Load units button; each file's coalition comes from its vehicle countries. The type-icon row uses the unit glyphs, including ships, at a third of the 128 px texture or smaller so seven fit on one line. |
| 2026-11-20 | Cursor | **Home base column.** Home base is its own column under the package name, to the right of Target type. The title row is just the side and the package name. |
| 2026-11-20 | Cursor | **Fighter pack steppers.** Flights, Max in a flight, and the altitude min and max have + and − beside the number. The scroll wheel changes that number while the pointer is on it, 1 flight or 100 m per notch. |
| 2026-11-20 | Cursor | **Sorties ride the base map.** Generate writes Mission Tasking sorties with the same placement as the old map strikes. Load base map brings them back to edit. A sortie flagged Attack Ground Targets keeps the unit it was aimed at. Any other target comes back unknown, and the route stays. |
| 2026-11-20 | Cursor | **Sortie clock.** Mission Tasking sorties launch on the timeline, not when a player enters a counted area. The mission window defaults to 180 minutes and can be set to 5–10 for a test. Draw chance and times in the order are chips in one shuffle. Map Generate writes those sorties as Air Tasking and leaves other missions on Exclusive Activation, armed only until the window ends. |
| 2026-11-20 | Cursor | **Sweep and strike groups import.** The editor rejects a group that writes `Enabled` on a timer, checkzone, activate, deactivate, or counter. Mission Tasking no longer writes that field. Objects that must start off are deactivated by Disarm At Start before the clock runs. Generate the group again; the two Debugs files from this morning still have the old field. |
| 2026-11-20 | Cursor | **Draw chance on the timeline.** Mission Tasking draws each sortie's chips on the clock. A wider chip sits on an earlier mark. Each extra time in the order fills the next mark. Launches that do not fit the window are counted under the line. |
| 2026-11-20 | Cursor | **Sortie clock never started.** With two or more sorties the clock starts only after a draw claims one. That claim was switched off at mission start, and the arm and the pulse were the same event, so the game dropped it. Zones stay closed, so flying through the area cannot start a flight either. The claim now stays on, and activate/deactivate name the MCU in both Objects and Targets. Generate the group again. |
| 2026-11-20 | Cursor | **Timer only, and the route on the timeline.** On a timer only spawns at each mark with no check zone. A mark while the cap is full is missed, not delayed. Launch while a sortie is out allows another sortie that is not already flying. On a timer with a check zone keeps the joint logic, and the area can close before the next mark. Times in the order is how many marks fit on the window, spread even, front, middle, or end. Draw chance is 1 in Y on the overlap of the gold route bars. Mission length goes to 360 minutes. Generate the group again. |
| 2026-11-20 | Cursor | **One sortie per mark, and a magazine per path.** Each sortie has its own load (even, front, middle, end) and its own times in the order. Generate builds that many flights on that path. A shared mark launches one of them. The template Mission Begin no longer starts the flight itself, which was putting every strike in the air at once. Duplicating a sortie counts 2, 3, 4, 5. Generate the group again. |
| 2026-11-20 | Cursor | **Time on Target arms with the launch.** Timer-only starts the flight without the check zone, so PULSE OUT never ran and the 8 minute timer never started. The home waypoint is ordered only by that timer. Plan Start now fires PULSE OUT with the bring-up. Generate the group again. |
