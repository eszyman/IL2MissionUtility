# HANDOFF — IL-2 Mission Utility (Claude working copy)

> Read this first in every new session. Update the **Session log** and
> **State** sections before you stop. Last updated: 2026-10-02.

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
