# H1 — Airfield database harvester

Status (2026-09-24): **built on branch `claude/airfield-harvester`, not merged.**
Waiting for the user's in-game test. The branch is checked out in the
worktree `C:\Claude\IL2-harvester-wt` because a parallel session was editing
`HANDOFF.md` on `main` in the primary clone. Add a row to HANDOFF §4/§6 when
merging. This branch does not touch `HANDOFF.md`, so it merges cleanly.

## Goal

A database of every Korea airfield with its full operational layer (Airfield
entity, taxi graph, lights, searchlights, flares, AAA, logic), without the
mission-editor "Save Selection to File" step per field. The end goal is Map
mode letting each field be Off / Target-only / Active base, with auto-generated
attacks targeting them. **That Map-mode part is not built yet** (next phase).

## What was built

- `src/harvest.rs` (new). Parses `_gen.mission`: drops `#` lines and the
  `Options` header, because `WindLayers` rows are not `.Group` syntax. It
  finds the Airfield nearest the player plane (every field if there is no
  player) and cuts it out:
  - every indexed object within the radius (4 km default) that is not
    closer to another Airfield;
  - plus logic reached by links up to 20 km (K13's approach icons sit
    13–15 km out). World objects and entities are never pulled in by links.
  - Dangling links are scrubbed.

  It then runs `clean_airfield` and, by default, `strip_ai_planes`. It writes
  the following into the DB folder (default `References/Airfields`, relative
  to the working dir):
  - `raw/<UTC stamp>_gen.Mission` and its sidecars (archived before parsing);
  - `<Name>_<country>.Group` and its sidecars, trimmed to the LC ids used;
  - `catalog.Group` (`AirfieldRecord` rows, upserted by name + country);
  - `models.tsv`.

  `GenWatcher` polls for a rewrite that has been stable for 1.5 s.
- `src/airfield.rs`: generalised `plan_clean` into `plan_strip(seeds,
  player_extras)` and added `strip_ai_planes`. Existing behaviour is unchanged
  and the existing tests pass. `node_link_ids` and `scrub_deleted` are now
  `pub(crate)`.
- `src/ui.rs`: an "Airfield database (automatic)" section at the top of the
  Airfield tab (Missions folder, DB folder, Radius, Keep AI planes, Watch,
  Harvest current, Harvest a mission file…, log). The poll runs in `update()`,
  so it works on any tab.
- `USER_MANUAL.md` (Airfield + Troubleshooting) and `docs/src-guide.md`.

## Verification

- `cargo test --offline`: 393 pass (was 383), 1 ignored (real-mission smoke
  test: `IL2_MISSION=<path> cargo test -- --ignored real_mission_from_env`).
- Warnings: still 70.
- Real game missions parse. These are BoS/Normandy `.Mission` files of up to
  about 500k lines: "Lightning Strikes MS", "Air School P-51D P-47D-28", and
  "Battle_of_Moscow".
- UI smoke test: a scratch Missions folder with a real mission copied as
  `_gen.mission`.
  - Harvest current: 10 fields harvested, 246 models found, raw file archived.
  - Watcher: after overwriting `_gen.mission` with a player mission, it
    auto-harvested only the start field.

## Unverified until the user tests

- That Korea Freeflight actually writes a **text** `_gen.mission` in
  `data\Missions` (the existing Airfield-tab instructions say it does).
- That a harvested Korea field matches a hand export such as `K13 AFB_mp.Group`.
  Compare object counts; `k13_harvests_whole_package` shows K13 survives the
  cut intact when it is the input.
- Whether 4 km is right for every Korea field. If a field loses logic, raise
  Radius. The log shows how many nodes were kept via links.

## Known limits

- `Runway = 0` marks a runway node and `RunwayEnd = 0` marks a threshold
  (K13–K15: Type 1 centerline; the two `RunwayEnd` nodes are the ends).
  `AxisHeading/AxisLength` are still the principal axis of every taxi node
  (grid north), so they follow that runway and also take in the taxiways.
  Legacy maps' `Airfield { Chart }` points are counted but give no axis,
  because their local-frame rotation convention is unverified.
- Timestamps are UTC (no chrono dependency; the build must stay offline).
- `mapload.rs` does not scan `References/Airfields/` (no recursion). Wire it in
  with P3 or the Map-mode phase.

## Next phase (proposed, not started)

Map mode: list catalog airfields inside the AO, and assign each one's side
from the front line. Give each field a state:
- **Off:** not exported.
- **Target-only:** a light variant without light/searchlight logic.
- **Active base:** the full package, re-countried to the owning side.

Also expose each field's target objects for the auto-generated attacks.
