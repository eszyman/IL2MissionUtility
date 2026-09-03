# IL-2 Group Generator — User Manual

This utility builds IL-2 Sturmovik Great Battles `.Group` files for the Korea map. Each mode writes (or rewrites) a group you import into the mission editor. It does not run the mission; it prepares linked MCU logic so random fighters, exclusive bomber plans, random ground units, multiplayer airfields, and period front-line icons behave correctly in-game.

Use **Help** on any page to open this manual at that topic.

---

## Overview

### What this is for

You author small, well-named templates in the IL-2 editor, then use this app to clone, randomize, and link them. The output is a `.Group` plus language sidecar files (`.eng`, `.chs`, …). Drop the group into a mission, move the copies onto the map, and save.

### The six modes

- **Template Builder** — author one memory-efficient unit group (activate or spawn, checkzones, per-unit orders) that Exclusive Activation and Army Generator can load.
- **Army Generator** — many ground (or ship) copies; a waterfall picks an activate count, or **Spawn all** so every copy starts.
- **Fighter Pack** — linked random CAP / intercept flights (N copies of one flight group, mutex through NodeGates).
- **Exclusive Activation** — several preplanned groups (bombers, recon, artillery) where only one plan can start at a time.
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

## Template Builder

### Intended use

Author **one** unit group that is cheap for a multiplayer server: units stay disabled until a player enters **Zone IN** (visual range / line of sight from the group center), then clean up when everyone leaves **Zone Out**. Choose **Activate Units** or **Spawn Units** depending on the profile. The same file is a valid template for **Exclusive Activation** and **Army Generator** (`Zone IN`, `ENABLE / PULSE IN`, `MISSION END`). This mode never writes NodeGates.

Built-in models come from `ModelTypes.Group` (planes, vehicles, trains, ships) plus `Unit_Template_Fixed.Group` (**Fixed Units**). **User Added** starts empty. **Add group…** appends another `.Group` to **User Added** without replacing the built-in list. **Load catalog…** replaces the list.

### Catalog group (your model list)

Export one `.Group` from the editor with **subgroups named**:

- `Planes` or `All Planes`
- `Vehicles` or `All Vehicles`
- `Trains` or `All Trains`
- `Ships` or `All Ships`
- `Fixed`, `Fixed Units`, or `Fixed Objects` (AAA / static `Vehicle` scripts under `fixedobjects`)
- `User Added`

Trains are `Train` objects. The catalog lists every rail car on those prototypes; in **Unit details** use **Carriages** to add, remove, and reorder the cars that will be written (tender is selected by default). Inside each subgroup, place one prototype of each model: the world object (`Plane` / `Vehicle` / `Train` / `Ship`) **and** its `MCU_TR_Entity`, linked with `LinkTrId` / `MisObjID`. Names and Script/Model paths are read from those prototypes. Extra MCUs in the catalog are ignored. Loose units at the root are used only if those named subgroups are missing.

### What you set

- **Bring-up** — **Activate Units** (parked entities) or **Spawn Units** (counter of 1 into a spawner whose **Objects** are the unit entities). Spawn is for independent units (targets, ambiance, or the same unit more than once). **Allow multiple spawns**: **Zone Out** always runs **MISSION END** cleanup and **Reset Counter** (zeros **DeathCount**). If the player ducks in, sees nobody, and leaves, that leave is the cleanup. **COOLDOWN** (minutes, default 5) starts only when **every** unit is destroyed (`OnPlaneDestroyed` for aircraft, `OnKilled` otherwise), even if the player is still inside — then it pulses **Trigger Spawner** for the next wave. A unit that hides is not deleted mid-fight; cleanup and counter reset wait for Zone Out. Unchecked is one-shot and COOLDOWN is 0 s. Only when a unit is a **Lead** with **followers** (target-linked wingmen) must that group be activated rather than spawned.
- **Add unit** — pick **Kind** (Planes, Vehicles, Trains, Ships, Fixed Units, User Added), then a **Type** (fighter, bomber, light flak, …), then a model from the button grid. Those pickers sit above the formation view. Picture, type, and cruise speed (km/h and mph) show in the formation view — skins and loadouts will be listed there later. Click a seat on the diagram to see that unit instead. **Add unit** appends a row: **UNIT**, then orders and reports in one chain, then **Events**. Each unit has a **Role**: Independent (own orders, can spawn), Lead, or Follows a lead. If a Lead is already set, each newly added unit follows it and `NumberInFormation` (`#`) is numbered 0, 1, 2, … **Up / Down** on the selected unit (or in Unit details) move that seat up or down in the list; Follows / Cover / Attack / Also-apply indexes stay pointed at the same units. **Copy attributes to all** copies country, skill, fuel, payload, and flags from the selected unit onto every other unit (role, orders, and events stay). Click a seat to highlight it. × on an order or event chip removes it. Order delay is baked into the chain (one order at a time, with a timer between). Reports wait on spawn / attack / takeoff / land instead of the previous timer.
- **Per seat** — country (alliance), skill (0–4), fuel, payload, Vulnerable / Engageable / Limit ammo. **AI RTB** and **Altitude** are planes only; new planes copy the last plane’s height. Ground units and ships sit at 0 m and do not write plane-only keys (`AiRTBDecision`, `StartType`). **Trains** have a **Carriages** list: add, remove, and reorder cars from the catalog (the locomotive itself is the selected Train model; a matching tender is the default). Planes get flight numbers and tail codes; MiG-15bis on 501 keeps callsign 12 (Honcho).
- **Place** — formation (Inverted Vee is finger-four, also Vee, Combat Box, Pairs, Echelon, Line abreast, Column) and **how many per group**. Combat Box defaults to 6 per group; the others default to 4. Spacing is always 150 m. If a unit is a **Lead**, **In formation** sets `NumberInFormation` for that flight (0 = lead) as a modifier on the per-group count. The diagram is the same style as before, now with map-like **Zoom** (slider, scroll wheel, **Reset View**) and right-drag pan. **Zone IN / Zone Out** rings are drawn in metres (zoom out to see them). Waypoint diamonds sit north of the origin; with a **Goto WP** order selected, click a diamond to pick that WP.
- **Zone IN** — visual range from the middle of the formation (line of sight). **Zone Out** is always larger. **Trigger coalition** is who the checkzones watch: Eastern `[1]`, Western `[2]`, or both. Zone IN pulses a timer, then Activate/Spawn — not the MCU directly.
- **Orders** — Attack, AttackArea, Behavior, Cover, Effect, Flare, Force Complete, Formation, **Goto WP**, **Time on Target**, **Mission Complete**, Land, **Take Off**, **RTB on Zone Out**. Ground units omit Cover, Land, Take Off, and RTB. **Attack** writes `MCU_CMD_AttackTarget`. **AttackArea** uses that unit’s known system range, **capped at 3000 m** (a 1 km MG gets 1000 m, not a 3 km bubble). If you enlarge the area past the weapon’s range, a warning is shown; **Match range** snaps it back. **Goto WP** is how waypoints are created: each hop is an order. Assign **WP 1**, then **New** to add the next. On arrival that WP pulses the next order’s timer (AttackArea, Time on Target, or the next Goto WP), not the MCU itself — so those timers are actually fed. The next waypoint is reached through that timer, not a WP n → WP n+1 MCU link. **Time on Target** is a timer pulsed from the waypoint before the attack (not the previous delay); when it expires, the next order fires — use this after a bomber attack so the flight is not left hanging. **Mission Complete** is a timer from the previous order that pulses **MISSION END** (Force Complete, RTB if set, then deactivate / delete). Put Land before Mission Complete if the flight should land first. **RTB on Zone Out** is the only case that writes RTB waypoints: one per placement group and coalition (East/West). Deactivate then waits **60 s**. Formation types are named (Pairs, Wedge, Right, Left, Heavy Wedge, Heavy Echelon Right, Heavy Combat Box, User for aircraft; Road Column 1 way, Road Column 2 way, Panic Stop, Continue Moving for vehicles). **Cover** targets another unit. When this seat is a flight lead with wingmen, `CoverGroup` is set and the order is lead-to-lead. Independent units cover the chosen unit directly. A unit cannot cover itself. Command orders have **Also apply to**: extra units share that one MCU (`Objects`). Default is this unit only — e.g. one AttackArea on six guns, another AttackArea on two others.
- **Reports** sit in the same UNIT → order chain (teal chips). **OnSpawned** is always first: `OnReport` Type 0, CmdId = the spawner, TarId = a timer, then the next order. **OnTargetAttacked** / **OnAreaAttacked** sit after Attack / AttackArea (CmdId = that MCU). Aircraft also have **OnTookOff** and **OnLanded** after Take Off / Land. The report’s **Then** list is the next command. Commands without a report between them still chain on timers. Spawn + OnSpawned starts the chain from the spawn report, not AFTER BRING UP.
- **Events** (purple chips, **+evt**) replace the old On killed checkbox. Each `OnEvent` is TarId only and can pulse Force Complete or an order timer. Vehicles, fixed objects, and ships: OnDamaged, OnKilled, OnMovedTo, OnSpottingStarted, OnTrailerKilled, OnTrailerDamaged, OnTrailerAttached, OnTrailerDetached, OnRadarRequestAirSupport. Aircraft: OnPilotKilled, OnPilotWounded, OnPlaneCrashed, OnPlaneCriticalDamage, OnPlaneDestroyed, OnPlaneLanded, OnPlaneTookOff, OnBingoFuel, OnBingoMainMG, OnBingoBombs, OnBingoTurrets, OnPlaneGunnersKilled, OnDamaged, OnKilled, OnMovedTo, OnBingoCargo.
- **Waypoints** — spacing and speed only. Count comes from **Goto WP** orders (one order per hop; **New** adds the next). WP 1…n are map objects with **Area 200 m**. There is no leftover **WP DELAY**; unused AttackArea timers are not written as orphans. Planes keep each seat’s altitude on the path.

### Generated names (do not rename)

- `ENABLE / PULSE IN` — Mission Begin and Army Generator fire this; it pulses **Zone IN**. Repeat spawn also fires it from Zone Out so the next visit can start immediately.
- `PULSE OUT` — Zone IN starts this (0.1 s) after **Zone Out ReActivate**. Checkzones must be activated (if off) and then pulsed before they evaluate.
- `Zone IN` — Closer = 1. Visual range from group center. Deactivates itself, activates Zone Out, pulses **PULSE OUT**, and pulses **MISSION BEGIN** / **SPAWN UNITS** (a timer, then the MCU). Repeat spawn also re-enables **DeathCount**.
- `Zone Out` — Closer = 0. Always pulses **MISSION END** (cleanup), **DeathCount Deactivate**, **Zone In ReActivate**, **ENABLE / PULSE IN**, and **Reset Counter**.
- `DeathCount` — repeat spawn only. `OnPlaneDestroyed` (aircraft) or `OnKilled` (ground) from every unit. When the count is full, **COOLDOWN** starts. It does not pulse **MISSION END** — cleanup waits for Zone Out so a dogfight is never cut short.
- `Reset Counter` — repeat spawn only. 0.5 s after Zone Out; pulses **Modifier Set Value**.
- `Modifier Set Value` — repeat spawn only. `MCU_ModifierSetVal` (`ParamIndex` 0, `Data0`–`Data3` = 0) writes 0 onto **DeathCount** so a partial wipe cannot fire **COOLDOWN** early on the next visit.
- `COOLDOWN` — pulses **Trigger Spawner**. Time is 0 unless Spawn and **Allow multiple spawns** (then the minutes you set, default 5).
- `MISSION BEGIN` — timer, then Activate Units (was named ACTIVATE UNITS).
- `MISSION END` — short cleanup hub. Exclusive completion hook.
- `MISSION END ORDERS` — Force Complete (Priority high). If any unit has **RTB on Zone Out**, also **RTB DELAY** then that group’s RTB waypoint (East/West). One AI order at a time.
- `DELAYED END ORDERS` — deactivate units (60 s after Zone Out when RTB is used, otherwise 2 s), then **DELETE DELAY**, then **Trigger Delete** (object-linked to every unit).
- `Units`, `Orders`, `Waypoints`, `Logic`

Copies park at **40000, 40000**. Move the whole group in the editor.

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

Copies park on the 10 km grid from 40000, 40000 so you can sort them, then place each plan on its real route — unless you tick **Export in place**.

### What you set in the app

- **Add template or pack…** — one `.Group` per plan, or a generated **Exclusive Activation** file. A pack is detected automatically: each original plan is listed, NodeGates are stripped, and **Export in place** turns on so positions stay where you placed them. Add another template afterward to insert a new plan into that developed group. **Add again** duplicates a slot (same file, second copy).
- **Export in place** — leave groups at their current X/Z (do not park on the grid). Use this when regenerating a pack you already placed in the editor.
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

## Army Generator

### Intended use

Scatter many copies of ground (or naval) templates. By default a **mutex waterfall** activates an exact count at mission begin and **deletes** losers. Tick **Spawn all copies (omit randomizer)** so every copy starts with its own Mission Begin — no waterfall.

Two submodes:

- **New From Templates** — clone raw unit templates onto the parking grid, then you place them by hand in the editor.
- **Rework Existing** — load packs you already placed and exported, keep world positions, rebuild the randomizer (and optionally merge several packs).

Use this for recon columns, AAA, armor, trains, shipping — a random subset, or a full army when spawn-all is on.

### New From Templates

1. Select a type icon (**Ship**, **Armor**, **Supply**, **Artillery**, or **Train**) as the default for newly added files. Each imported template then keeps its own type icon — click that template’s icons to change it. A file that contains `Train` objects is imported as **Train** even if another type is selected. Ship parks on water; Train parks on railroad; Armor / Supply / Artillery park on open ground. Map draws that same type for placed units (Eastern or NATO artwork by coalition).
2. **Add templates…** or **Add folder…**
3. Set **Templates to create** (total copies, **max 64**). Optionally tick **Spawn all copies (omit randomizer)** so every copy starts. Otherwise set **Activate ratio (%)**. DServer often fails when too many random units fire at once — stay **under 30 random units in the whole mission** (this pack plus any others) unless spawn-all is on.
4. Set **Influence** per template — share of the copies created (e.g. two types at equal influence and 10 copies → 5 placed each). Activate % then runs **inside each type**, so 5 copies at 50% is **3 live**, not half of the whole pack. Copy mix lists placed and activate (or spawn) counts per type. You can change a template’s type after import by clicking its type icons.
5. **Start delay (s)** — wait this long after Mission Begin before the first type chain (default **0**). Use this when several Army Generator groups sit in the same mission so they do not all fire at t=0. Hidden when spawn-all is on.
6. **Delay between groups (ms)** — after the start delay, each following type waits this long (default **500 ms**) so MCU load does not spike.
7. Optionally **Keep loaded positions** if you already placed the source file (Army Generator will not park copies on the 10 km grid; Map **Place Eastern / Place NATO** from those templates is disabled). You can still **Load Eastern / Load NATO** on Map to bring a saved army group in with or without repositioning.
8. **Generate File**, or leave Keep loaded positions off and **Place Eastern / Place NATO** on Map (Army Generator mix, one coalition at a time). To keep Eastern and NATO independent, generate each army as a `.Group` here, then load those groups on Map.

Each type parks in its **own** square on the 10 km grid (2×2 for 4 copies, 3×3 for 9, …) so you can sort by type, then drag copies onto the map. Map placement along the front is the other path: specify types here, then place on Map.

### Rework Existing

1. Place copies in the editor, export the group.
2. **Add pack…** (you can add several packs; types combine).
3. Select **Zone IN** on each type. **Influence** here is that type’s activate percent (how many of the copies already on the map will win). Copy mix lists on-map and activate counts per type.
4. The top slider can push the same percent onto every type.
5. **Start delay (s)** and **Delay between groups (ms)** work as in New From Templates.
6. **Generate File** writes a **new** file (it does not overwrite the one you loaded).

Detected copies are grouped by name after stripping editor suffixes like `[3]` or `*3*`. Stay **under 30 random units** in the whole mission.

### Remove random logic (Rework)

Use this to turn an existing Random Ground Units pack into ordinary always-on groups (for example a 17-of-40 pack that DServer cannot handle).

1. **Add pack…**
2. Check **Remove random logic (keep every copy, no waterfall)**.
3. For each type, pick the MCU **Mission Begin** should fire:
   - Prefer a **timer that targets a Closer checkzone** (`Closer = 1`, usually **Zone IN**).
   - **`ENABLE / PULSE IN`** is that timer on a well-authored template.
   - You can pick the **checkzone** itself if you want Mission Begin to pulse that zone directly.
4. **Generate File** writes a new group with **no** `Recon Randomizer`. Every copy starts. Clone Mission Begins are re-enabled and retargeted.

Do not keep the randomizer in the same mission as this always-on export of the same copies.

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

The generated group **`Recon Randomizer`** owns **`Recon: Mission Begin`**, waterfall timers, and **`Randomizer:DELAY`** (start delay and/or between types). Do not hand-edit that group; re-run Rework instead.

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

Build a **base map** for a Korean War date: the front, areas of influence, the AO outline, attack arrows you draw, fighter packs you place, and the airfield/block groups you choose to stamp in. The preview also shows major battles and the suggested front for that date so you can plan; those battle markers are **not** written into the group.

Towns and buildings are **never** written as map icons. Airfields and scenery blocks come only from `.Group` files you add (for example `References/K13 AFB_mp.Group`). Fronts and faction territory come from the built-in 1950–1953 snapshots. Line styles follow IL-2’s Attack / Defence / Zone types so they read like NATO military map symbols (FLOT, assembly areas, axes of advance).

### What you set

- **Year** (1950–1953) and **Season** (early spring, late spring, summer, fall, winter). Jump to the first dated mark in that year/season. The mark title and note describe that date (38th Parallel, June invasion, Inchon to the Yalu, Chosin, Hungnam, Ripper, MLR, Triangle Hill, Pork Chop, armistice DMZ, …).
- **Front date** slider — a **non-uniform** dated timeline: week-to-week in early 1950, month-to-month through mid-1951, then seasonal after the MLR. Drag interpolates the preview between adjacent marks. **Generate Base Map** uses the nearest mark (not a coarse season average). **Left/Right** arrow keys step one mark; **Home/End** jump to the first/last date. The slider is a full-width row so you can hit a specific interesting date.
- **Editor map** — the selected date recommends the IL-2 landscape to load (Spring / Summer / Autumn / Winter + year), for example `Editor map: Winter 1950 (IL-2 landscape)`.
- **Korea map** — the preview opens on the low-quality map (`assets/DD052_en_map_01_LowQ.jpg`) at 100% scale so pan and zoom stay responsive. The high-resolution map (`assets/DD052_en_map_01.jpg`) loads in the background and is used for detail once you zoom in. **Scroll** to zoom, **right-drag** to pan, or use **Reset box / Zoom in / Zoom out / Reset view** just above **Pan / Select AO**. In Pan mode, left-drag draws the AO box. The whole suggested front stays visible (thinner outside the box). The front stays about **8 km south of the Yalu**. DPRK (north) influence continues **past the Yalu** to the north of the box; USA (south) influence never crosses the river. Cities, the Yalu, and the 38th parallel are preview-only.
- **Draw Custom Front** — sketch a replacement FLOT **west to east** (left to right) so the line cannot fold over itself. The timeline slider clears it. Consecutive vertices that share the same X/Z are omitted on export so `LineType` 13 (IconId 0) can render.
- **Draw Salient** — click or drag a bulge that leaves the front and returns. A **light** dot shows where the stroke starts on the front; a **dark** dot shows where it will rejoin. Click the dark dot or **right-click** to finish; after that the next start dot follows the new front (including the salient). Strokes are cropped to the AO. Self-crossing strokes are discarded. Export writes **LineType 4** (`SalientReference.Group`: dashed border, hashed infill) in the **opposite** colour of the side it cuts into (into red → blue, into blue → red). Influence is computed from the **base** front, then the salient polygon is subtracted from the side it enters. Use this for Hungnam / Wonsan-style pockets; those dates no longer seed a pocket automatically.
- **Draw Attack Arrow** — drag **from tail to tip**. Colour follows the side the tail sits on (north of the front = Eastern RGB **155, 0, 0**; south = NATO RGB **0, 120, 150**). These arrows are previewed and **exported** as `LineType` 11 (`CorrectedAttack_arrow.Group`). The **second-to-last** icon is titled **Attack**.
- **Remove last** — drops the last salient or attack arrow you drew (or cancels a stroke still in progress).
- **Preview vs export** — the map always previews the suggested front (thin outside the AO, full weight inside) and major battles for the selected date. **Generate Base Map** writes only what sits in the AO: areas of influence, front lines (including salients, cropped to the box), the AO outline, attack arrows you drew, fighter packs you placed, units you placed from Army Generator or loaded as armies, and any added reference groups. Objectives, historical battles, cities, the Yalu, and the 38th parallel stay on the preview.
- **Legend** — under the map: AO box (yellow), **Road** (dark red, 50% translucent, from `assets/roads.svg`), **Railroad** (black, 50% translucent, from `assets/railroads.svg`), Front (red), Salient, Battle, Eastern / NATO, Airfield (red), Linked entity (orange), Block (yellow), Eastern fighters, NATO fighters, Eastern shipping, NATO shipping, Eastern / NATO objectives, Eastern / NATO ground. Dots inside the box are bright; the **10 km** margin is dimmer.
- **Fighter CAP** — **Eastern** / **NATO** buttons (`assets/EasternFighter.svg`, `assets/NatoFighter.svg`) build linked packs from the current **Fighter Pack** setpoints (linked groups, flights, types, timers). Only one coalition is placed at a time, in the influence area that side controls. Groups sit in a **checkerboard**, never closer than that pack’s **Zone IN** radius (16 km on the built-in template). **Waves** are separate N-packs mixed on that grid (two 5-packs, not one 10-pack). The wave number sits at the **lower left** of each icon. Tick **Fill AO at Zone IN spacing** to keep the tight spacing and add packs (up to 8) as the box grows; leave it off to spread the requested count. RTB waypoints are not drawn on the preview. **Generate Base Map** parks each group’s **RTB** on the closer friendly corner of the AO (Eastern north, NATO south; west vs east by the group). In **Pan / Select AO**, drag an icon to fine-tune. Packs are written at those positions with NodeGates intact.
- **Objectives** — **Eastern** / **NATO** markers (`assets/EasternObjective.svg`, `assets/NatoObjective.svg`). Select a coalition, then click the map to drop a marker. Right-click a marker of that side to remove it. In **Pan / Select AO** you can drag them. Objectives are **preview only** and are not written into the group. They assign each ground / ship group a hashed marker, set heading, park **Artillery / armor / MGs** within that template’s published system range (ML-20 17.2 km, Katyusha 8.5 km, T-34-85 1.5 km, …; unknown artillery **15 km**, unknown armor **2 km**), and on **Generate Base Map** move each copy’s ground / ground-target **AttackArea** MCU onto that objective. Supply stays in the front band.
- **Units** — details may be set on **Army Generator**, or **Load Eastern… / Load NATO…** to bring in saved army `.Group` files (templates or generated packs) and optionally reposition them. Each Army Generator template has its own type icon (Ship / Armor / Supply / Artillery / Train); **Place Eastern** / **Place NATO** draws that same icon on the map from the current Army Generator mix. Loaded groups are classified automatically: **Ship** objects → Ship; **Train** objects → Train (always on railroads; icons `assets/EasternTrain.svg`, `assets/NatoTrain.svg`); a ground / ground-target **AttackArea** plus a long (or unknown) gun → Artillery, or **mobile artillery** if the vehicles sit in a perfect column; known short-range guns (tanks, MGs) → Armor even without that area; a long gun without a ground area (a Katyusha in a truck column) stays Supply. **Reposition** (on by default) parks along the front using those types and ranges; turn it off to keep authored X/Z (like reference groups, but exported as the army, not trimmed). Eastern and NATO armies can both be loaded. **Trains** always sit on railroad polylines inside the AO, yawed to the track, travel direction random, with template waypoints placed ahead along the rail. **Perfect columns** (vehicles in a file along their facing, including mobile artillery and truck runs with or without waypoints) sit on roads the same way: each vehicle on the road, the whole column facing one way, waypoints ahead as numbered dots labelled with that unit’s preview number (**N WPk**). Other ground stays on dry open ground. **Supply** parks along the front within **10 km**. **Armor** and static **Artillery** park within that group’s weapon range of its hashed objective. Short-range groups (tanks, MGs) cluster tighter than **4.5 km** so they can fit inside a 1.5 km disk. If that disk has no friendly open ground, placement **warns** (orange bullets, one line per unit with its preview number) and parks on the closest friendly open ground, then along the front; the same orange number is drawn on that unit. Hard failures (no terrain, empty mix) stay **red**. **Shipping** uses water from `assets/combined_terrain.bin` (water bit: `packed & 1 != 0`; icons `assets/EasternShipping.svg`, `assets/NatoShiping.svg`). **Armor**, Supply, and Artillery use dry open ground (`packed & 4 != 0` and `packed & 1 == 0`; `assets/EasternArmor.svg` / `NatoArmor.svg`, `EasternSupply.svg` / `NatoSupply.svg`, `EasternArty.svg` / `NatoArty.svg`). Salients are ignored for placement so a bulge does not shove units — fine-tune by dragging as with ships. Influence and activate ratio (or spawn-all) for **Place Eastern / Place NATO** come from Army Generator (5 s start delay, 500 ms between groups unless spawn-all). If no objective is marked, ships get a random heading and ground stays north (an orange warning is shown for each affected unit). Leftovers that do not fit in the AO park on the nearest friendly water/open ground outside the box. If **Keep loaded positions** is on, Map cannot place or move **Army Generator** units; loaded army groups can still be repositioned or dragged. Preview numbers sit at the **lower left**; `assets/direction.svg` (north-up) shows heading. Road/rail waypoints show as coloured dots labelled **N WPk**; in **Pan / Select AO**, click a waypoint to select it, then click a road or railroad to place it — including **another branch** at a fork, or behind the column so it must turn around. You can also drag a selected waypoint onto the network. Vehicles stay on their current track; only the waypoint moves. Left-drag a unit to snap it along that network, **right-drag** to aim (network groups stay tangent to the path). Placement prefers the coalition’s side of the front; if the only usable track is on the wrong side, that unit is listed with its number and parked on a track in the AO. On export, each copy is yawed around the centroid of its `Model` objects: vehicles/ships/planes, associated `Block` / `Ground`, and `MCU_TR_Entity` linked by `LinkTrId`. Column and train copies instead place each vehicle/train and each `MCU_Waypoint` on the polyline. Timers, checkzones, and AttackArea stay put (AttackArea is snapped onto the objective). **Generate Base Map** writes the pack(s) at those positions and headings, and snaps ground AttackArea MCUs onto the hashed objective. Country is set to that coalition.
- **Reference groups** — **Add reference groups…** loads one or more `.Group` files. Each is placed at the X/Z already saved in that file, then trimmed to the drawn box plus **10 km**. Typical picks are the K13/K14/K15 airfield groups in `References/`. `landscape_Korea_FullScene.Group` MARKS (`MCU_Waypoint`) show on the preview as nested dots so you can see towns and fields; those waypoints are **not** copied into the generated group (the editor already has the landscape). Preview colors: **Airfields red**, linked `MCU_TR_Entity` **orange**, Blocks **yellow**. Nested objects inside subgroups (and landscape marks) are transparent. Other MCU types are ignored.
- **Focus on a battle** — entire front, a battle in this period, or jump to any listed battle (sets the dated mark and a box around that fight).
- **Aircraft for this date** — suggestion only (what is in the sim by that date). It does not modify a fighter pack.
  - Early 1950: F-80C-10, F-51D, Yak-9P, La-11
  - Fall 1950: F-84E and MiG-15bis join
  - Winter 1950 onward: F-86A-5 as well

**Generate Base Map** writes `Korea_BaseMap_YYYY-MM-DD.Group` plus language sidecars (merged from any reference-group translations, then the generated icon labels).

### Output

- Front line — white `LineType` 13 chain, red `LineType` 1 end vertex (`FrontLine.Group`), coalitions `[1, 2]` on the body and `[1, 2, 0]` on the end. The front is extended to the west and east edges of the box (over water) so both influence areas fill the AO. Vertex spacing is 4 km. Consecutive vertices that round to the same `{:.3}` X/Z are dropped so the editor can draw the line. A user-drawn front is not flattened through a salient bulge. Hungnam / salient bubbles are subtracted from the influence they cut into instead of offsetting around the S-curve.
- Salient — a closed **LineType 4** icon from `SalientReference.Group` (dashed border, hashed infill), coloured opposite the side it cuts into and cropped to the AO. The old front span is omitted. Draw Hungnam / Wonsan-style pockets by hand with **Draw Salient**.
- **AO outline** — two concentric rectangles around the drawn box so players can see the stay-inside zone. Inner border RGB **110, 90, 0**; outer border RGB **200, 170, 0**, about **800 m** outside the inner. Closed `MCU_Icon` polylines (`LineType` 22).
- Areas of influence — `MCU_TR_InfluenceArea` with a `Boundary` polygon (`AoI.Group`): USA `Country = 601`, DPRK `Country = 503`, clipped to the AABB, with about **5 km** cleared on each side of the front
- Reference groups — copied airfield/block groups at their original coordinates, trimmed to the box plus 10 km. Indexes are reallocated so they do not collide with the icons
- Attack arrows — user-drawn axes (`LineType` 11 from `CorrectedAttack_arrow.Group`): tail width, fade, point. Colour is the side the tail was drawn from (Eastern RGB **155, 0, 0**, NATO RGB **0, 120, 150**). The **second-to-last** vertex is titled **Attack**; other vertices use `LCName = 0`.
- Fighter packs — one coalition’s linked N-packs parked on the checkerboard (or where you dragged them). Each group’s `RTB - N` waypoint sits on the closer friendly corner of the AO (Eastern north, NATO south). NodeGates stay inside each pack so two 5-packs can be live at once.
- Shipping — Army Generator ship mix from **Place Eastern / Place NATO**, plus any loaded ship army groups, parked on water (or just outside the AO if the box has too little water). Each copy is yawed to the preview heading (`Model` objects, associated blocks, and linked `MCU_TR_Entity`). Activate ratio or spawn-all, 5 s start delay, and 500 ms between groups come from Army Generator for placed mix. Country is set to that coalition.
- Ground — one pack per coalition you placed from Army Generator, plus each loaded army group. Open-ground groups park on dry open ground (or just outside the AO if the box has too little). **Trains** park on railroads and **road columns** (including mobile artillery) park on roads, each vehicle yawed to the path and waypoints on the chosen road or rail (including another branch or behind the column). Other copies are yawed like shipping. Supply stays in the 10 km front band; Armor and static Artillery sit within that template’s system range of the hashed objective when open ground exists, otherwise the closest friendly open ground or the front (an orange warning lists each numbered unit). Ground / ground-target **AttackArea** MCUs are moved onto that objective. Activate ratio or spawn-all, 5 s start delay, and 500 ms between groups come from Army Generator for placed mix. Country is set to that coalition. Objectives are not written.

Historical battles are **preview-only** and are not written into the group.

Labels live in the sidecar LC table, not as plain text in the group. Named polylines title the first vertex; attack arrows title the second-to-last vertex instead. The rest use `LCName = 0` so airfield sidecar strings (for example “You have been killed, press Esc.”) cannot appear on map icons.

### Map limits

Positions are lat/lon projected onto the Korea map (X north, Z east). **Pusan is south of the map**. Summer 1950 notes that and draws only what still fits. A battle that sits off-map is skipped with a status note. The front stays about **8 km** south of the Yalu. DPRK influence continues north of the river to the box edge; USA influence never crosses the Yalu.

### Front-line data (for denser wartime traces)

To match a strategic-map animation (ANZAC / DVA style), supply one west→east polyline per date:

- **Format** — CSV (`date,seq,lat,lon`) or GeoJSON `LineString` in WGS84
- **date** — `YYYY-MM-DD` (daily is ideal; weekly is enough for 1950, monthly after July 1951)
- **Vertices** — 15–40 points, **increasing longitude** (west to east), denser at turns (Pusan, Inchon, Iron Triangle, Punchbowl)
- **Never north of the Yalu** — Korean bank only
- **Sources worth tracing** — USMA West Point Korean War atlas, US Army CMH campaign maps, ANZAC Portal strategic map stills, Perry-Castañeda / Library of Congress sheets

After import, generate again with a different box if you need a tighter AO.

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

Fighter Pack and Exclusive Activation already contain NodeGates. Army Generator already contain `Recon Randomizer`. Do not add a second Mission Begin that fans out to every copy.

---

## Troubleshooting

**Parse failed** — the file is not a valid `.Group` (or `_gen.mission` document). Export again from the editor.

**Not a linked fighter pack** — custom template needs `Group 1` and `NodeGates`.

**Template has no MCU_CheckZone** — Exclusive and Army Generator require checkzones. Name start zones as documented.

**No end timer / cleanup warning** — Exclusive Activation: name the timer `END` or `MISSION END` and point it at Deactivate/Delete that lists **all** units.

**Zone warning (Closer / coalitions / Activate)** — Exclusive start zone: Closer = 1, coalitions set, Targets reach Activate or Spawner of the units.

**No ENABLE / PULSE IN** — Army Generator: the win path must pulse that named MCU (or Mission Begin targets that the generator can reuse).

**Orange unit-placement bullets** — Map parked the groups but not where the range disk wanted (closest open ground, along the front, or outside the AO). Red status is a hard failure (no mix, no terrain, empty coalition side).

**Copies all spawn / HUD breaks** — a clone `MCU_TR_MissionBegin` was reconnected. Rework the pack here instead of wiring Begin by hand.

**Icons have no text** — missing `.eng` (etc.). Re-export the template or keep Map sidecars next to the group.

**Two plans fire at once** — Exclusive: the start zone was not selected, or the end timer never fires so you may have other issues, or zones were not Closer / not actually targeting OUT DISABLE after a hand edit. Regenerate rather than patching NodeGates.

**Airfield still has a player** — load from `_gen.mission` Freeflight, not a hand-built group that never marked a player plane.

**Front line empty** — the box does not intersect that snapshot, or the layer sits off-map (Pusan). Draw a larger box or pick another season.
