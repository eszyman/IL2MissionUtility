# IL-2 Group Generator — User Manual

This utility builds IL-2 Sturmovik Great Battles `.Group` files for the Korea map. Each mode writes (or rewrites) a group you import into the mission editor. It does not run the mission; it prepares linked MCU logic so random fighters, exclusive bomber plans, random ground units, multiplayer airfields, and period front-line icons behave correctly in-game.

Use **Help** on any page to open this manual at that topic.

---

## Overview

### What this is for

You author small, well-named templates in the IL-2 editor, then use this app to clone, randomize, and link them. The output is a `.Group` plus language sidecar files (`.eng`, `.chs`, …). Drop the group into a mission, move the copies onto the map, and save.

### The five modes

- **Fighter Pack** — linked random CAP / intercept flights (N copies of one flight group, mutex through NodeGates).
- **Exclusive Activation** — several preplanned groups (bombers, recon, artillery) where only one plan can start at a time.
- **Random Units** — many ground (or ship) copies; a waterfall picks an exact activate count at mission start.
- **Airfield** — strip Freeflight / single-player logic from an airfield exported from `_gen.mission` so it works in multiplayer.
- **Map** — Korean War 1950–1953 map icons. Draw a box on the peninsula to clip the front, areas of influence, airfields, and towns.

### Typical workflow

1. Pick a mode.
2. Load templates or set options (Map needs no extra template beyond `References/`).
3. Click **Generate File** / **Generate Base Map** (Airfield: **Export File**) and choose a save path.
4. In the IL-2 mission editor, import the `.Group`. Keep the sidecar files next to it.
5. Move groups from the parking grid onto the map. Do not break the MCU names this app relies on.

Generated copies park in a square **10 km** grid starting at map **40000, 40000** (except Airfield, which keeps the airfield where it was, and Map, which projects onto Korea).

### Country and coalitions

IL-2 checkzones use coalition lists, not country IDs.

- **Western / UN / USA** planes: coalition **`[2]`**
- **Eastern / USSR / DPRK / PRC** planes: coalition **`[1]`**

Country **500-series** (501 USSR, 502 DPRK, 503 PRC) sets fighter-pack Zone IN/OUT to eastern `[1]`. Country **600-series** (601 USA) sets them to western `[2]`.

---

## Fighter Pack

### Intended use

Build a linked N-pack of random AI fighter flights for Korea: several groups on the map, only one “live” at a time through NodeGates, each group internally randomizing which flight spawns. Use it for CAP, intercept, or roaming fighters that recycle after cooldown and reinforcement timers.

You do not need to load a file. The built-in 3-pack logic (`Group 1` + `NodeGates`) is enough. Optionally load a custom linked fighter pack if you have changed that logic in the editor.

### What you set in the app

- **Number of linked groups** (1–10) — clones of Group 1, chained through NodeGates.
- **Number of flights** / **Max number in each flight** — how many randomizer slots and aircraft per slot. Sizes are spread (for example 4 flights × max 4 is not always 4/4/4/4).
- **Aircraft types and skill** — flights cycle through the checked types. Skill is 0–4; lead is never below wingman.
- **Country** — 501 / 502 / 503 / 601. This also sets Zone IN/OUT plane coalitions.
- **Cooldown, Reinforcement, Delete orders** (seconds) — timers inside each group.
- **Altitude min/max** — 1- and 2-ships spread between min and max. A second pair in a 3- or 4-ship is high cover (~2000 m above). Low cover sits in a 500–1500 m band that rises with max altitude. Wingmen stack 25–50 m on their lead.

Each flight is one randomizer slot. Pairs get AttackArea + Cover. A leftover singleton gets AttackArea only.

### Required template structure (custom pack)

A custom file must already be a **linked fighter pack**:

- A group named **`Group 1`**
- A waypoint named **`RTB - 1`**
- A group named **`NodeGates`**

If those are missing, the app rejects the file. Leave the custom template unloaded to use the built-in pack.

### Hooks the generator expects inside Group 1

These names must exist. The built-in template has them. Do not rename them in a custom pack.

**Spawn / enable path**

- **`ENABLE / PULSE IN`** — NodeGates `nIN - ENABLE` targets this together with **`Enable Spawner`**. This is the pulse that starts the group’s spawn chain.
- **`Enable Spawner`** — enables the aircraft spawners.
- **`Disable Spawner`** — NodeGates `nIN - DISABLE` targets this together with **`Delete Orders`**.
- **`Delete Orders`** — cleanup when the group is closed.

**Randomizer (inside Group 1)**

- Group **`Randomizer`**
- Timer **`Wait for Output 600ms`** — prototype for waterfall timers (the app uses **0.5 s** steps; 100 ms is too tight for IL-2).
- **`Spawn 1`** — spawner prototype.
- **`CloseInput`** — deactivate prototype (a win closes other outputs).
- **`ReOpen Outputs`** — activate prototype.

**Aircraft / pairing**

- Group **`Airplanes`** — at least one Plane + `MCU_TR_Entity` pair to clone.
- **`Cover Lead`** — cover MCU prototype.
- **`DeathCount`**, **`SpawnCount`**
- Timer **`50ms`** (often under a **`Logics`** group) — short delay prototype.

**NodeGates timers** (pattern copied from group 1)

- `{n}IN - ENABLE` / `{n}IN - DISABLE`
- `{n}OUT - ENABLE` / `{n}OUT - DISABLE`
- Fan-out timers that enable/disable the **other** groups’ IN cells so only one pack group is live.

When one group’s zone fires, NodeGates disable the other groups’ inputs. When that group finishes, outputs reopen the others.

### After generate

Import the N-pack. Move **Group 1 … Group N** (and their RTB waypoints) onto patrol areas. Leave NodeGates linked. Do not duplicate the pack again by hand inside the editor — generate a new file here if you need a different N or mix.

---

## Exclusive Activation

### Intended use

Load two or more complete plans (typical case: preplanned bomber streams, but any Plane / Vehicle / Ship group works) so **only one plan can trigger**. When a start checkzone fires, the other plans’ start zones close. When that plan’s **end timer** fires, the other zones reopen.

Use this when several scripted packages would otherwise all go if a player flew through every corridor.

Copies park on the 10 km grid from 40000, 40000 so you can sort them, then place each plan on its real route.

### What you set in the app

- **Add template…** — one `.Group` per plan. **Add again** duplicates a slot (same file, second copy).
- For each plan, tick the **start checkzones** this plan should open and close.
- Pick the **end timer** that means “this plan is finished.”

The app suggests names and warns when a zone or timer is wired incorrectly. Fix the template in the editor if you see a red warning; do not ignore it.

### Required MCUs in each template

The file must contain at least one **`MCU_CheckZone`** and one **`MCU_Timer`**. Prefer also **`MCU_Activate`** and **`MCU_Deactivate`** so NodeGates can clone real prototypes.

#### Start checkzones (triggers)

**Recommended names (auto-selected):** `Zone IN` or `MISSION START` (case-insensitive).

If the template has several checkzones (for example a B-29 corridor with multiple Zone INs), select every zone that should count as “this plan started.” Multiple Zone INs in one template are still **one plan**.

Each selected zone should be:

1. **Distance Type = Closer** (`Closer = 1`). Farther/unset zones will warn.
2. **At least one coalition list** filled: `PlaneCoalitions`, `VehicleCoalitions`, `ShipCoalitions`, or `CountryCoalitions`.
3. **Targets** that eventually reach an **`MCU_Activate`** or **`MCU_Spawner`** whose **Objects** include the plan’s units (Plane / Vehicle / Ship, or their `LinkTrId` entities).

The generator appends a target from each selected zone to that plan’s NodeGates **OUT DISABLE**, so firing the zone closes the other plans.

#### End timer (completion)

**Recommended names (auto-selected):** `END` or `MISSION END` (case-insensitive).

Also recognized if you have not renamed yet: `Delay Delete`, `MISSION CLEAN UP` (and names that start with `MISSION CLEAN UP`). Any timer that already targets a Delete/Deactivate of the units can be picked from the dropdown.

The end timer **must target** an **`MCU_Deactivate`** and/or **`MCU_Delete`** whose **Objects** list **every** Plane / Vehicle / Ship in the template (by object Index or `LinkTrId`). A partial list warns: leftover aircraft will keep flying and the mutex will reopen while the first plan is still alive.

The generator appends a target from the end timer to that plan’s NodeGates **OUT ENABLE**, so completion reopens the other plans.

### What the generator adds

A **`NodeGates`** group with, per plan:

- `{n}IN - ENABLE` / `{n}IN - DISABLE`
- `{n}OUT - ENABLE` / `{n}OUT - DISABLE`
- Fan-outs to the other plans
- Activate/Deactivate MCUs that open and close **this plan’s selected checkzones**

You do not add NodeGates by hand. Do not rename those generated timers.

### Language files

If the template has icon or subtitle LC indexes, keep `.eng` (and other language) files next to the `.Group`. The app copies them beside the output. If they are missing, re-export the template from the editor; the editor will not invent sidecars on a later resave of the generated pack.

---

## Random Units

### Intended use

Scatter many copies of ground (or naval) templates, then let a **mutex waterfall** activate an exact count at mission begin. Losers are **deleted** so unused vehicles and blocks are not left in the mission.

Two submodes:

- **New From Templates** — clone raw unit templates onto the parking grid, then you place them by hand in the editor.
- **Rework Existing** — load packs you already placed and exported, keep world positions, rebuild the randomizer (and optionally merge several packs).

Use this for recon columns, AAA, armor, trains, shipping — anything that should appear as a random subset, not all at once.

### New From Templates

1. **Add templates…** or **Add folder…**
2. Set **Templates to create** (total copies) and **Activate ratio (%)**. The UI shows the exact winner count (never zero).
3. Set **Influence** per template — share of the total (e.g. two types at equal influence and 10 copies → 5 + 5).
4. **Delay between groups (ms)** — first type chain starts immediately; each following type waits this long (default **500 ms**) so MCU load does not spike at t=0.
5. Optionally **Keep loaded positions** if you already placed the source file.
6. **Generate File**.

Each type parks in its **own** square on the 10 km grid (2×2 for 4 copies, 3×3 for 9, …) so you can sort by type, then drag copies onto the map.

### Rework Existing

1. Place copies in the editor, export the group.
2. **Add pack…** (you can add several packs; types combine).
3. Select **Zone IN** on each type. **Influence** here is that type’s activate percent.
4. The top slider can push the same percent onto every type.
5. **Generate File** writes a **new** file (it does not overwrite the one you loaded).

Detected copies are grouped by name after stripping editor suffixes like `[3]` or `*3*`.

### Required MCUs in each unit template

The file must contain at least one **`MCU_CheckZone`**.

#### Zone IN (required)

**Name:** `Zone IN` (case-insensitive). Select it in the UI so the app knows the group is valid.

This is the proximity / trigger zone that becomes live when that copy **wins**. The randomizer does **not** activate the zone with `MCU_Activate` directly.

#### ENABLE / PULSE IN (required for a win)

**Name:** `ENABLE / PULSE IN`

On a win, the randomizer fires this MCU (same as a Mission Begin target in a well-authored template). That pulse should enable **Zone IN** and the rest of the unit’s start chain (icons, subtitles, spawn, waypoints).

If the template has **no** Mission Begin targets **and** no `ENABLE / PULSE IN`, generate fails.

#### Mission Begin

A well-authored template has **`MCU_TR_MissionBegin`** targeting `ENABLE / PULSE IN` (and only the start chain you want at t=0 when the group is used alone).

**Important:** IL-2 often fires Mission Begin even when `Enabled = 0`. The generator **disconnects clone Mission Begins** (empties their target lists) so copies do not all start, and so HUD / End Mission are not broken. Subtitle MCUs are left as you authored them.

Do not re-link those silenced Mission Begins in the editor.

#### Winner / loser behavior

- **Winner:** randomizer timer (last in the chain is 100%) fires `ENABLE / PULSE IN` → Zone IN. A win **deactivates the remaining Outs** in that type’s waterfall so a later 100% timer cannot also fire.
- **Loser:** the copy is **deleted**.

#### Other useful pieces (not renamed by the app)

- Vehicles / ships / blocks / planes you want in the copy
- Delete / deactivate for the unit’s own end-of-life, if you need it
- Icons and subtitles (see Language files)

The generated group **`Recon Randomizer`** owns **`Recon: Mission Begin`**, waterfall timers, and **`Randomizer:DELAY`** between types. Do not hand-edit that group; re-run Rework instead.

### After generate (New)

Import, then **move copies off the parking grid** onto the map. Keep each copy’s internal MCUs together. Export and use **Rework Existing** if you change the mix later.

---

## Airfield

### Intended use

Freeflight missions from the Task Editor include a **player aircraft** and single-player graph (takeoff helpers, music, objectives, tiny player-out bubbles). Multiplayer does not use that. This mode strips the player and retargets proximity checkzones that were object-linked to the player, so the field can sit in an MP mission as AI + blocks + friendly-plane zones.

### Source file (required)

In the IL-2 map / mission editor:

1. Open a **Freeflight** mission.
2. Take the airfield from the generated **`_gen.mission`** file — **not** a `.Group` you authored by hand.
3. Export or copy that airfield (it may be inside a `Group` wrapper or as loose blocks at the root).
4. **Load airfield…** here.

The inspector shows name, layout, origin, vehicles, AI aircraft, blocks, checkzones, player aircraft to remove, and which zones will be unlinked.

### What you set

- **Western `[2]`** — USA / UN fields (default). Matches Seoul AFB friendly-plane checkzones.
- **Eastern `[1]`** — USSR / DPRK / PRC fields.

### What the cleaner does (no special names required)

There are no “name this MCU” hooks. Behavior is structural:

**Removed**

- Player `Plane` objects (`AILevel` / player flags as exported) and their `MCU_TR_Entity`
- The **`AutoRemove`** subgroup, if present
- MCUs object-linked to the player except large proximity checkzones
- Tiny checkzones (**radius under 200 m**) that object-link the player (SP “player out” bubbles)
- Nearby Mission Objectives, icons, and subtitles in the player orbit (~2.5 km)
- The rest of the single-player graph hanging off those nodes (takeoff, music, etc.), without deleting ordinary vehicles, ships, AI planes, or blocks

**Kept, but retargeted**

- `MCU_CheckZone`s that object-linked the player and are **not** tiny bubbles: the player id is dropped from **Objects**, and **`PlaneCoalitions`** is set to Western `[2]` or Eastern `[1]`

AI aircraft, vehicles, ships, blocks, and normal field checkzones stay.

If no player aircraft is found, the file may already be cleaned; export is then a no-op besides rewriting the group.

### After export

Import into an MP mission. Confirm friendly-plane coalitions on the field’s checkzones. You still place spawners / ramps in the editor as usual; this mode does not add MP spawn logic.

---

## Map

### Intended use

Build a **base map** for a Korean War year and season: the front, areas of influence, troop assembly zones, attack axes, defensive belts, major battles, naval routes, and the airfield/block groups you choose to stamp in. Use it as the planning layer when building a historical (or historically flavored) mission. It does not spawn fighter logic or MCU triggers.

Towns and buildings are **never** written as map icons. Airfields and scenery blocks come only from `.Group` files you add (for example `References/K13 AFB_mp.Group`). Fronts and faction territory come from the built-in 1950–1953 snapshots. Line styles follow IL-2’s Attack / Defence / Zone types so they read like NATO military map symbols (FLOT, assembly areas, axes of advance).

### What you set

- **Year** (1950–1953) and **Season** (early spring, late spring, summer, fall, winter). Jump to the first dated mark in that year/season. The mark title and note describe that date (38th Parallel, June invasion, Inchon to the Yalu, Chosin, Hungnam, Ripper, MLR, Triangle Hill, Pork Chop, armistice DMZ, …).
- **Front date** slider — a **non-uniform** dated timeline: week-to-week in early 1950, month-to-month through mid-1951, then seasonal after the MLR. Drag interpolates the preview between adjacent marks. **Generate Base Map** uses the nearest mark (not a coarse season average). **Left/Right** arrow keys step one mark; **Home/End** jump to the first/last date. The slider is a full-width row so you can hit a specific interesting date.
- **Editor map** — the selected date recommends the IL-2 landscape to load (Spring / Summer / Autumn / Winter + year), for example `Editor map: Winter 1950 (IL-2 landscape)`.
- **Korea map** — the developer blog map (`assets/DD052_en_map_01.jpg`) is scaled to fit the preview box (same world square as the editor: X north, Z east, 40000–470000). **Scroll** to zoom, **right-drag** (or middle-drag) to pan, or use **Zoom in / Zoom out / Reset view**. Left-drag still draws the AO box. The yellow rectangle is the AO. The red line is the period front (dim on the full map, bright where it is stretched to the west/east box edges so influence can cover water). In December 1950 the main front is **not** stretched east through the **Hungnam–Wonsan pocket**; that evacuation bubble is a separate cyan loop (Hamhung–Wonsan). The front and the red DPRK influence stop about **8 km south of the Yalu** so they never enter China. **Major cities, the Yalu, and the 38th parallel stay on the preview for reference and are not written into the generated group.**
- **Legend** — under the map: AO box (yellow), Front (red), Pocket (when that date has one), Airfield (red), Linked entity (orange), Block (yellow), Nested subgroup objects (transparent dots), AoI fills if enabled. Dots inside the box are bright; the **10 km** margin is dimmer.
- **Reference groups** — **Add reference groups…** loads one or more `.Group` files. Each is placed at the X/Z already saved in that file, then trimmed to the drawn box plus **10 km**. Typical picks are the K13/K14/K15 airfield groups in `References/`. `landscape_Korea_FullScene.Group` MARKS (`MCU_Waypoint`) show on the preview as nested dots so you can see towns and fields; those waypoints are **not** copied into the generated group (the editor already has the landscape). Preview colors: **Airfields red**, linked `MCU_TR_Entity` **orange**, Blocks **yellow**. Nested objects inside subgroups (and landscape marks) are transparent. Other MCU types are ignored.
- **Focus on a battle** — entire front, a battle in this period, or jump to any listed battle (sets the dated mark and a box around that fight).
- **Aircraft for this date** — suggestion only (what is in the sim by that date). It does not modify a fighter pack.
  - Early 1950: F-80C-10, F-51D, Yak-9P, La-11
  - Fall 1950: F-84E and MiG-15bis join
  - Winter 1950 onward: F-86A-5 as well
- **Layers** — Front line, Areas of influence, Major battles, Troop buildups, Defensive positions, Areas to attack, Major naval routes. Enable at least one layer, or add a reference group.

**Generate Base Map** writes `Korea_BaseMap_YYYY-MM-DD.Group` plus language sidecars (merged from any reference-group translations, then the generated icon labels).

### Output (no MCU triggers)

- Front line — white `LineType` 13 chain, red `LineType` 1 end vertex (`FrontLine.Group`), coalitions `[1, 2]` on the body and `[1, 2, 0]` on the end. The front is extended to the west and east edges of the box (over water) so both influence areas fill the AO, except when a Hungnam–Wonsan pocket is present (west stretch only). Vertex spacing is 4 km.
- Hungnam-Wonsan pocket — on 5 and 11 December 1950, a second **closed** front-style chain (`LineType` 13, last vertex targets the first) for the Hamhung-Wonsan evacuation bubble. Group names and language strings are ASCII so the mission editor will import the file.
- **AO outline** — two concentric rectangles around the drawn box so players can see the stay-inside zone. Inner border RGB **110, 90, 0**; outer border RGB **200, 170, 0**, about **800 m** outside the inner. Closed `MCU_Icon` polylines (`LineType` 22).
- Areas of influence — `MCU_TR_InfluenceArea` with a `Boundary` polygon (`AoI.Group`): USA `Country = 601`, DPRK `Country = 503`, clipped to the AABB, with about **5 km** cleared on each side of the front
- Reference groups — copied airfield/block groups at their original coordinates, trimmed to the box plus 10 km. Indexes are reallocated so they do not collide with the icons
- Battles — yellow point markers (`IconId` 501), a closed **Defend Area** ring (`LineType` 12 from `CorrectedDefendArea.Group`), and an **Attack** arrow (`LineType` 11 from `CorrectedAttack_arrow.Group`) pointing at the ring. NATO actions use RGB **0, 150, 200**; Eastern forces use RGB **155, 0, 0**. No hatched/zig-zag fill.
- Troop buildups — closed assembly rings (`LineType` 12), faction-colored
- Defensive positions — closed belts (`LineType` 12), faction-colored
- Areas to attack — Attack arrows (`LineType` 11): tail width, fade, point
- Naval routes — cyan polylines (`LineType` 22 end vertex)

Labels live in the sidecar LC table, not as plain text in the group. Only the first vertex of each named polyline gets a title; the rest use `LCName = 0` so airfield sidecar strings (for example “You have been killed, press Esc.”) cannot appear on map icons.

### Map limits

Positions are lat/lon projected onto the Korea map (X north, Z east). **Pusan is south of the map**. Summer 1950 notes that and draws only what still fits. A battle that sits off-map is skipped with a status note. The front and DPRK influence stop about **8 km** south of the Yalu (the red fill is clamped on every vertex, including west of the river mouth).

### Front-line data (for denser wartime traces)

To match a strategic-map animation (ANZAC / DVA style), supply one west→east polyline per date:

- **Format** — CSV (`date,seq,lat,lon`) or GeoJSON `LineString` in WGS84
- **date** — `YYYY-MM-DD` (daily is ideal; weekly is enough for 1950, monthly after July 1951)
- **Vertices** — 15–40 points, **increasing longitude** (west to east), denser at turns (Pusan, Inchon, Iron Triangle, Punchbowl)
- **Never north of the Yalu** — Korean bank only
- **Sources worth tracing** — USMA West Point Korean War atlas, US Army CMH campaign maps, ANZAC Portal strategic map stills, Perry-Castañeda / Library of Congress sheets

After import, you can hide layers you do not want, or generate again with a different box.

---

## Language files

MCU_Icon and MCU_TR_Subtitle store numeric **LC indexes**. The strings live in sidecars next to the `.Group`:

`.eng` `.chs` `.fra` `.ger` `.rus` `.spa`

Those files are **UTF-16 LE** with a BOM. The mission editor will **not** invent them if you resave a group that never had them.

- If a template has sidecars, this app **merges** them into the output (first value wins when two templates share an LC index).
- If they are missing, you get a yellow hint: re-export the template from the editor.
- Map always writes a full sidecar set for the icons it creates.

Keep the sidecars in the **same folder and same base name** as the `.Group` when you import.

---

## Importing into the mission editor

1. Save the generated `.Group` and its language files together.
2. In the mission editor, import / insert the group into your mission.
3. Move parked copies (40000, 40000 grid) to their real positions. Move a whole copy (the group), not only the vehicles, or MCU links stay behind.
4. Do not rename the MCUs listed in this manual (`Zone IN`, `ENABLE / PULSE IN`, `END`, `MISSION END`, `Group 1`, `NodeGates`, `Recon Randomizer`, …).
5. Test in a small mission: confirm only the intended subset activates, mutex reopens, and translations show.

Fighter Pack and Exclusive Activation already contain NodeGates. Random Units already contain `Recon Randomizer`. Do not add a second Mission Begin that fans out to every copy.

---

## Troubleshooting

**Parse failed** — the file is not a valid `.Group` (or `_gen.mission` document). Export again from the editor.

**Not a linked fighter pack** — custom template needs `Group 1` and `NodeGates`.

**Template has no MCU_CheckZone** — Exclusive and Random Units require checkzones. Name start zones as documented.

**No end timer / cleanup warning** — Exclusive Activation: name the timer `END` or `MISSION END` and point it at Deactivate/Delete that lists **all** units.

**Zone warning (Closer / coalitions / Activate)** — Exclusive start zone: Closer = 1, coalitions set, Targets reach Activate or Spawner of the units.

**No ENABLE / PULSE IN** — Random Units: the win path must pulse that named MCU (or Mission Begin targets that the generator can reuse).

**Copies all spawn / HUD breaks** — a clone `MCU_TR_MissionBegin` was reconnected. Rework the pack here instead of wiring Begin by hand.

**Icons have no text** — missing `.eng` (etc.). Re-export the template or keep Map sidecars next to the group.

**Two plans fire at once** — Exclusive: the start zone was not selected, or the end timer never fires so you may have other issues, or zones were not Closer / not actually targeting OUT DISABLE after a hand edit. Regenerate rather than patching NodeGates.

**Airfield still has a player** — load from `_gen.mission` Freeflight, not a hand-built group that never marked a player plane.

**Front line empty** — the box does not intersect that snapshot, or the layer sits off-map (Pusan). Draw a larger box or pick another season.
