# Korea 1950–53: sourced unit reference for template authors

Use this file when you build a Fighter Pack, a Template Builder group, or an Army Generator template. It gives historically plausible unit sizes, compositions, formations, spacings, frontages and altitudes.

Built 2026-09-23 from nine public documents: US Army field manuals, official USAF histories and a declassified Pacific Fleet report (section 10). Every number in sections 2–8 traces to a page of those documents, and the quoted source line for every citation is in [korea-1950-53-unit-reference-evidence.md](korea-1950-53-unit-reference-evidence.md).

## 1. How to use this file

- **Citations** look like `[S3 p.102, par. 94]`. `S3` is a source ID from section 10. `p.102` is the **PDF page** of that source, not its printed page number. `par.` is the manual's own paragraph number when there is one. The source PDFs and their extracted text are kept locally in `docs/historical-reference/sources/`, which is git-ignored. See `SOURCES.md` there for how to rebuild them.
- **Units:** metres first, with the source's own unit in brackets, e.g. `46 m [50 yd]`. **"—"** means the sources do not give the value. **Section 9** says what is missing and why.
- **Dates matter.** Air tactics changed a lot between 1950 and 1953, so every air row is dated. Match the row to your mission date from Map mode.
- **Two sources are not what their titles suggest:**
  - **S4** is FM 6-120, which covers artillery observation units, not firing batteries.
  - **S6** is the 1960 edition of the Chinese Army handbook, not the 1952 one. Rows tagged `CCA 1955 TOE (1960 hbk)` describe the Chinese army after the war.
- **Section 7 is advice.** The IL-2 model and formation columns there are editorial suggestions matched to the app's catalog (`TemplateExamples/ModelTypes.Group`, `Unit_Template_Fixed.Group`, `src/template.rs` formation presets). They are not historical claims.

## 2. Air — UN / US

Sources: S9 = Futrell, *The USAF in Korea 1950–1953* (Windows OCR); S10 = *USAF in Korea: Campaigns, Units, and Stations*. Page numbers are PDF page indices. Metric first, original in brackets. "M" = Mach.

| Date | Topic | Fact (paraphrased) | Cite |
|---|---|---|---|
| 26 Jun 1950 | F-82 evacuation cover | F-82s flew in **flights of four**, orbiting in relays over Inchon harbour. | [S9 p.31] |
| 27 Jun 1950 | Layered fighter cover | The F-80s flew **high cover** over Seoul because their fuel economy was best at altitude. The F-82s orbited at lower levels. From first light the F-80s orbited along the Han River. | [S9 p.34] |
| Jun 1950 | F-80C armament and radius | 6 × .50-cal guns. Up to **16 × 5-in HVAR** on rocket posts, no bomb pylons. Radius ≈160 km [100 mi] on internal fuel. With rockets and two 165-gal tip tanks ≈362 km [225 mi]. Two 1,000-lb bombs could replace the tip tanks, which cut the radius to ≈161 km [100 mi]. All figures assume cruise above 4,570 m [15,000 ft]. | [S9 p.81] |
| Jul 1950 | F-80 "Misawa" tanks | The 265-gal tip tanks gave a radius of ≈563 km [350 mi]. With standard tanks the F-80 had ≤15 min over the target; with Misawa tanks, up to 45 min. At first only about 1 flight in 4 had the big tanks. | [S9 p.82], [S9 p.109] |
| Jul 1950 | F-80 anti-tank rocket attack | Approach the tank from the 4 o'clock position, dive at 30° and fire at ≈460 m [1,500 ft]. A salvo of **all four rockets** worked best. The cloud base was often ≤305 m [1,000 ft], which forced flat attack angles. | [S9 p.111] |
| Aug 1950 | B-29 bridge tactic (no opposition) | A **stream of individual aircraft** at ≈3,050 m [10,000 ft] approached at a 40° angle. Each dropped a string of four 500-lb GP bombs per run. An average bridge took 13.3 runs. | [S9 p.152] |
| Jul–Aug 1950 | B-29 strategic formations | 47 B-29s passed over Hungnam within 4 min in **squadron "vic"/V formations**. Base altitude was 4,880 m [16,000 ft]. Squadrons were often scheduled over the target 5–10 min apart. In cloud the fallback was "Hometown": a stream of individual aircraft at **1-min intervals**, each bombing by radar. | [S9 p.212], [S9 p.213], [S9 p.214] |
| 8 Nov 1950 | Sinuiju strike package | F-80s and F-51s first strafed or rocketed the flak positions. 51st FIW F-80s flew **top cover**, and MiGs engaged them. Then 70 B-29s dropped incendiaries while 9 B-29s bombed the bridges. The bombers stayed **above 5,490 m [18,000 ft]**, in squadrons in close trail. | [S9 p.245] |
| Nov 1950 | B-29s at the Yalu | The B-29s had to bomb above a minimum altitude to escape flak, but the OCR dropped the figure. Winds exceeded 222 km/h [120 kt]. On incendiary raids, icing forced the B-29s above their usual 2,440 m [8,000 ft] cruise. | [S9 p.246], [S9 p.248] |
| Nov–Dec 1950 | Armed-recon element split | Armed reconnaissance flights provided their own top cover. The **lower element searched** for traffic and the **upper element watched** for MiGs. | [S9 p.268] |
| Dec 1950 | B-29s in NW Korea | B-29s now had to attack in mutually supporting strength or with escort. On 21–22 Dec the whole force flew **four-plane formation** attacks on bridges. | [S9 p.285] |
| Dec 1950 | F-86 first tactics (4th FIW) | **Flights of four** took off at **5-min intervals** in **fingertip**, breaking into elements of two in combat. Patrol altitude was 8,230–10,060 m [27,000–33,000 ft], just below contrail level. The first patrols cruised at M0.62. The round trip Kimpo–Yalu was ≈692 km [430 mi]. Combat radius with two 120-gal tanks was 907 km [490 nm]. | [S9 p.272] |
| Dec 1950 | F-86 revised patrol | Sabres entered the patrol area at **≥M0.85, preferably above M0.87**. Patrols were cut to 20 min. Standard patrol: **16 aircraft = four flights of four**, arriving at 5-min intervals at different altitudes. | [S9 p.273] |
| Dec 1950 | "Jet stream" and fluid four | Flights arrived **5 min apart**. The first to see MiGs called their location, altitude and heading, and all flights converged. The **"fluid-four"** was four Sabres spaced roughly in fingertip. The element leaders did the shooting and the wingmen covered the rear. | [S9 p.274] |
| Mar 1951 | Sabre screen from Suwon | Flights of four left at periodic intervals and took stations over landmarks. The lead flight went to Sinuiju. "Dust on the runway" at Antung meant MiGs were taking off. "Hey Rube" meant all flights should assemble at Sinanju. Time on patrol in MiG Alley was ≈25 min. | [S9 p.318] |
| 1 Mar 1951 | F-80 escort failure | **22 F-80s** were to escort **18 B-29s**. The bombers made rendezvous late; the jets had to break off and return to base, and 9 MiGs then attacked the unescorted B-29s. | [S9 p.316] |
| 30 Mar 1951 | Escort layout | 4th Wing Sabres flew **high cover**; 8th and 49th Wing F-80s gave **close escort** in the target area. There were three bomber formations of **12 B-29s** each, and **eight Sabre flights** patrolled the Yalu. At 7,620 m [25,000 ft] MiGs were ≈161 km/h [100 mph] faster than F-80s. | [S9 p.319] |
| 7 Apr 1951 | F-84 escort | 27th FEW launched **48 F-84s in 15 min**. The Thunderjets flew **parallel to the bomber boxes** while the Sabres screened above. | [S9 p.319] |
| 12 Apr 1951 | Escort breakdown | Aborts cut the force to 39 of 48 B-29s. The three formations strung out and the F-84s had to split. The 19th Group's eight B-29s met 40–50 MiGs. Afterwards F-84s were judged too slow to escort against MiGs. | [S9 p.320], [S9 p.322] |
| Apr 1951 → Nov 1951 | Six-ship Sabre flights | Sabre flights grew to **six**: four chased the climbing MiG element and two the diving one. They were later dropped as "too unhandy" once larger MiG numbers appeared. | [S9 p.324], [S9 p.436] |
| Apr 1951 | Close-support load | A flight of two F-80s expended four 260-lb fragmentation bombs, eight HVAR and 3,600 rounds of .50-cal. | [S9 p.386] |
| May 1951 | Armed recon versus flak | Because of flak, Mustangs never flew armed reconnaissance with **less than a full flight of four**. F-80s used evasive action. An F-80 flight released napalm from 30 m [100 ft]. | [S9 p.357] |
| 3 Jul 1951 | Flak suppression plus escort | **32 F-84s** suppressed the flak while **6 B-29s escorted by 33 Sabres** bombed Pyongyang Downtown airfield. | [S9 p.334] |
| mid-1951 → Oct 1951 | B-29 daylight sortie pattern | The routine was **three flights of three** against airfields or **two flights of four** against bridges. For the Oct 1951 airfield raids the pattern was shoran runs by three flights of three. The strike windows were first light–1000 h and 1500 h–last light, to fit the Sabre screen, which needed 5 h to turn around. At most four shoran approaches existed per target. | [S9 p.430], [S9 p.432] |
| 23 Oct 1951 | Namsi raid | About 100 MiGs boxed in the **34 screening Sabres**. **8 B-29s in three flights ("Able", "Baker", "Charlie")** met **55 F-84s** as close escort. About 50 MiGs circled the formation, then made pursuit-curve, diving and from-below passes. 3 B-29s were lost. The 307th Wing wrote that ≥150 F-86s would have been adequate escort. | [S9 p.432], [S9 p.433] |
| 24–27 Oct 1951 | Meteor/F-84 escort | 8 B-29s with 16 RAAF Meteors and 10 F-84s met 40–70 MiGs. Next, 8 B-29s with 16 Meteors and 32 F-84s were routed over the Yellow Sea, but ≈95 MiGs attacked. Daylight B-29 main effort ended on 28 Oct 1951. | [S9 p.433], [S10 p.35] |
| Oct–Nov 1951 | Switch to night | Straight-wing escorts above 6,100 m [20,000 ft] flew too near their Mach limits to fight. From 4 Nov 1951 Bomber Command flew at night, **single B-29s along shoran arcs**. | [S9 p.438] |
| Nov 1951 | Sabre patrol size | Jet-stream fluid-four flights were staggered in time or altitude, stacked down from 10,670 m [35,000 ft]. A patrol had **≤32 Sabres in two 16-ship sections**. RF-80s were customarily escorted by **12–16 F-86s**. | [S9 p.436] |
| Dec 1951 | Sabre strength | FEAF had 165 F-86s, of which 127 were committed in Korea. In Jun 1951 there had been 89 F-86s in theatre, 44 of them in the 4th Wing. | [S9 p.437], [S9 p.424] |
| Jan–Feb 1952 | High patrols | Sabres entered the patrol area at up to 12,190 m [40,000 ft]; the 51st Wing (F-86E) flew a few thousand feet higher. On 6 and 25 Jan the 51st entered at **13,720 m [45,000 ft]**. | [S9 p.442], [S9 p.443] |
| Mar–Apr 1952 | Stacked entry | Sabres entered the combat area **stacked down from 12,190 m [40,000 ft]**. | [S9 p.444] |
| May 1952 | Barrier lowered | MiGs were entering at 4,570–10,670 m [15,000–35,000 ft], so the Sabres lowered their **barrier patrols**. Other flights flew top cover for fighter-bomber strikes. May 1952 saw the war's peak of 5,190 F-86 sorties. | [S9 p.445] |
| Summer 1952 | Standard Sabre tactic; F-86F | The standard was a Yalu barrier patrol at high cruise speed by fluid-four flights. The F-86F with the solid leading edge reached **15,850 m [52,000 ft]** and M1.05. | [S9 p.531], [S9 p.534] |
| Oct 1952 | Two-level patrols | **High patrols of F-86Fs at ≈12,190 m [40,000 ft]; low patrols of F-86Es at ≈9,140 m [30,000 ft]**. A subsidiary barrier along the Chongchon was flown by **four Sabres or Meteor 8s**. The 51st Wing flew flights of six and the 4th Wing sections of eight. | [S9 p.536] |
| Jul 1952 | Operation Pressure Pump | Strikes went in at 1000, 1400 and 1800 h. Sabres and Meteors patrolled north of the Chongchon. That night 54 shoran B-29s followed, for 1,254 sorties in all. | [S9 p.539] |
| Jul 1952 | B-26 night fire raids | A pathfinder marked the target. Streams of B-26s followed at **≈1,220 m [4,000 ft], 5 min apart**. | [S9 p.538], [S9 p.540] |
| Nov–Dec 1952 | Anti-"box-in" | Sabre flights were posted **south of the Yalu** for ground-controlled intercepts. The main patrol went home **over the Yellow Sea**. | [S9 p.631] |
| Early 1953 | Sabre squadron "train" | **Six flights of four**, each a fluid four, in loose trail **≈1.6 km [1 mi]** apart. Four-ship patrols flew north of Cho-do between main patrols. | [S9 p.632] |
| Feb 1953 | Sui-ho strike | **22 F-84s, each with two 1,000-lb SAP bombs**, went in at low level. **82 escorting/covering Sabres** drew off 30 MiGs. | [S9 p.646] |
| Sep 1952–Apr 1953 | Fighter-bomber minimum altitude | A **914 m [3,000 ft] minimum recovery altitude** cut hits per sortie by 19%. | [S9 p.661] |
| Apr–Jul 1953 | F-86F fighter-bomber | With 200-gal tanks the F-86F carried **two 1,000-lb bombs** to a radius of 667 km [360 nm]. 8th Wing F-86 fighter-bomber missions began on 13 Apr 1953 and 18th Wing missions on 14 Apr. | [S9 p.661] |
| May–Jul 1953 | Late-war Sabre ops | Sabres cruised at up to **98% power** while awaiting combat. In Jun 1953 Sabres began 70 of 92 engagements, mostly **below 12,190 m [40,000 ft]**. In Jul 1953 the median combat altitude was **6,100 m [20,000 ft]** (text is OCR column-interleaved). | [S9 p.675], [S9 p.676], [S9 p.678] |
| 1952–53 | B-29 night: flak floor | Flak was not dangerous if the B-29s stayed **above 5,490 m [18,000 ft]**, except on the Yalu. | [S9 p.634] |
| Oct 1952 → | B-29 night compression | Crews attacked at **1-min instead of 3-min intervals**, with up to four shoran aiming points. Up to **9 aircraft fitted in a space 305 m × 12.9 km [1,000 ft × 8 mi]**. | [S9 p.636] |
| Jul 1952 → Jan 1953 | Night escort for B-29s | From Jul 1952 **4 F7Fs** flew ≈5 min ahead of the stream between the IP and the target. From Nov 1952 F3D Skynights flew "barrier cover" 32–80 km [20–50 mi] north of the stream. From Jan 1953 F3Ds flew overhead cover **610–910 m [2,000–3,000 ft] above** the bombers, and **4–6 F-94s** flew a barrier ≈48 km [30 mi] beyond the target. | [S9 p.636], [S9 p.637] |
| 1953 | B-29 survival rules | Irregular schedules, altitudes varied within shoran limits, a compressed stream, no contrail altitudes, dark of the moon, ECM against gun-laying and searchlight radars. | [S9 p.638] |
| 1951–52 | Reconnaissance | The RF-80 (redlined at M0.8) needed **heavy Sabre escort**. RF-51s kept a **1,830 m [6,000 ft] minimum** from Feb 1952, with a wingman ≈305 m [1,000 ft] higher calling out flak. RB-26s photographed at night from 2,130–2,440 m [7,000–8,000 ft]. From Jun 1951 the Fifth AF would not tie up **8–16 fighters** escorting bomber-type recon aircraft. RB-29s had to stay above 6,100 m [20,000 ft]. RF-86As needed only a minimum of Sabre escort. | [S9 p.570], [S9 p.571], [S9 p.572], [S9 p.573] |
| 1951–53 | Rear-area night defence | F-82s stood strip alert near Seoul, and four T-6s armed with .30-cal guns stood alert at Kimpo. From Apr 1953 interceptors were barred from engaging below **610 m [2,000 ft] or slower than 257 km/h [160 mph]**. | [S9 p.450], [S9 p.686] |

### 2.1 Air order of battle by base

K-numbers from the S10 K-site list [S10 p.185]; S9 confirms Taegu = K-2 [S9 p.111]. Base names and dates come from each unit's "Stations" entry. Japan and Okinawa bases have no K-number. Dates are the S10 arrival dates; "→" means to the end of the war.

| Wing / group | Aircraft (period) | Base (K-no.) | Dates | Cite |
|---|---|---|---|---|
| 4th FIW / FIG | F-86A/E/F | Taegu K-2 (S10) **or** Kimpo K-14 (S9): Detachment A. Then Suwon K-13, then Kimpo K-14 | Taegu/Kimpo Dec 15 1950–early 1951 (conflict: S10 says Taegu, S9 says Kimpo); Suwon (the 334th Sq staged through Suwon from 6 Mar 1951) Mar 30 1951 (group) / May 7 1951 (wing); Kimpo Aug 23 1951 → | [S10 p.67], [S10 p.68], [S9 p.270], [S9 p.317], [S10 p.185] |
| 51st FIW / FIG | F-80C → F-86E/F (combat from 1 Dec 1951) | Kimpo K-14, then Japan, then Suwon K-13 | Kimpo Oct 1950–Dec 1950/Jan 1951; Itazuke/Tsuiki (Japan) to Jul 1951; Suwon Jul 31 1951 (group) / Oct 1 1951 (wing) → | [S10 p.71], [S10 p.72], [S9 p.437] |
| 8th FBW / FBG | F-80C; F-51 Aug 1950–Dec 1950; F-80 again from Dec 1950; F-86F from Apr 1953 | Suwon K-13, Kimpo K-14, Pyongyang K-23, Seoul K-16, Japan, Kimpo K-14, Suwon K-13 | Suwon Oct 7 1950; Kimpo Oct 28 1950; Pyongyang Nov 25 1950; Seoul Dec 3 1950; Itazuke Dec 10 1950; Kimpo Jun 25 1951; Suwon Aug 24 1951 → (first F-86 mission 7 Apr 1953; wing at full Sabre strength 4 Jun 1953) | [S10 p.53], [S10 p.54], [S10 p.55], [S9 p.661] |
| 18th FBG / FBW (+2 Sqn SAAF attached) | F-80 → F-51 (Jul 1950); F-86F from Jan–Feb 1953 | Taegu K-2, Pusan East K-9, Pyongyang East K-24, Suwon K-13, Chinhae K-10, Hoengsong K-46, Osan-ni K-55 | Taegu Jul 28 1950; Ashiya (Japan) Aug 8 1950; Pusan East Sep 8 1950; Pyongyang East c. Nov 21 1950; Suwon Dec 1 1950; Chinhae Dec 9 1950; Hoengsong Jun 2 1952; Osan-ni Jan 11 1953 → (first F-86 mission 26 Feb 1953) | [S10 p.56], [S10 p.57] |
| 35th FIG / FIW | F-80 → F-51 (Jul 1950) | Pohang K-3, Yonpo K-27, Pusan East K-9 | Pohang Jul 14 1950; Tsuiki (Japan) Aug 13 1950; Pohang Oct 3 1950; Yonpo Nov 18 1950; Pusan c. Dec 3 1950–May 25 1951 (the S10 Jan 1951 station roster lists the 35th FIW at Pusan East) | [S10 p.70], [S10 p.123] |
| 49th FBG / FBW | F-51 and F-80 (1950); F-80 only from Oct 1950; F-84 Jun–Sep 1951 | Taegu K-2, then Kunsan K-8 (designation swap) | Itazuke (Japan) Jul 9 1950; Taegu Oct 1 1950 (wing in Tsuiki Jan 26–Feb 24 1951); Kunsan Apr 1 1953 → | [S10 p.58], [S10 p.59] |
| 27th FEW / FEG | F-84 | Taegu K-2, then Itazuke (Japan) | Taegu Dec 1950–Jan 31 1951; Itazuke to Jul 1951 | [S10 p.66], [S10 p.67] |
| 136th FBW / FBG (ANG) | F-84 | Itazuke (Japan), then Taegu K-2 | Itazuke May 1951; Taegu c. Sep 26 1951 (group) / Nov 16 1951 (wing)–Jul 10 1952 | [S10 p.63], [S10 p.64] |
| 116th FBG (ANG) | F-84 | Misawa (Japan); squadrons rotated to Taegu K-2 | Misawa Jul 1951; Taegu Nov 1951–c. Jun 1952 | [S10 p.62] |
| 58th FBW / FBG | F-84D → F-84G (late 1952) | Taegu K-2 | Jul/Aug 1952 → | [S10 p.60], [S10 p.61] |
| 474th FBW / FBG | F-84E (from the 116th); F-84 | Kunsan K-8, then Taegu K-2 (designation swap) | Kunsan Jul 10 1952; Taegu Apr 1 1953 → | [S10 p.64], [S10 p.65] |
| 319th FIS | F-94 (night/all-weather) | Suwon K-13 | Mar 10 1952 → | [S10 p.73] |
| 3d BW / BG (Light) | B-26 (night intruder) | Iwakuni (Japan), then Kunsan K-8 | Kunsan Aug 22 1951 → | [S10 p.74], [S10 p.75] |
| 452d BW / BG (Light) | B-26 | Itazuke/Miho (Japan), then Pusan East K-9 | Pusan East May 1951–May 10 1952 | [S10 p.77], [S10 p.78] |
| 17th BW / BG (Light) | B-26 | Pusan East K-9 / Pusan West K-1 | Pusan East May 10 1952; group at Pusan West Oct 1 1952; Pusan East Dec 20 1952 → | [S10 p.76], [S10 p.77] |
| 543d TSG, then 67th TRW / TRG | RF-80, RB-26, RF-51, RF-86 (six conversions); T-6 and F-51 with the 45th TRS | Taegu K-2, then Kimpo K-14 | Taegu c. Oct 10 1950–Jan 1951; Taegu Mar 1951; Kimpo Aug 1951 → | [S10 p.86], [S10 p.87], [S10 p.88] |
| B-29 groups/wings: 19th, 22d, 92d, 98th, 307th | B-29 (all flew from Japan or Okinawa, not Korea) | Kadena (19th, 22d, 307th); Yokota (92d, 98th) | 19th Jul 5 1950 →; 22d Jul–Oct 1950; 92d Jul–Oct 1950; 98th Aug 5 1950 →; 307th Aug 1950 → | [S10 p.79], [S10 p.81], [S10 p.82], [S10 p.83], [S10 p.84], [S10 p.85] |
| 91st SRS | RB-29, RB-45, RB-50 | Yokota (Japan) | Dec 19 1950 → | [S10 p.90] |
| Snapshot, Jul–Nov 1951 | 77 Sqn RAAF (jets) at Kimpo; SAAF Mustangs and a USAF FB group at Chinhae; two FB groups at Taegu; one FB and one FI group at Suwon; a light bomber group at Kunsan; B-29s and cargo aircraft in Japan | K-14, K-10, K-2, K-13, K-8 | Jul–Nov 1951 | [S10 p.35] |

### 2.2 Early war 1950: F-51, F-80, Yak, Il-10, La-9/La-11

#### Template-ready summary 1950

| Type | Typical flight size | Altitudes | Attack profile | Load | Mission type | Base(s) |
|---|---|---|---|---|---|---|
| **F-51** | Flights of 4 ("four Mustangs") [S9 p.189], [S9 p.239], [S9 p.285]; pairs sometimes sent [S9 p.241]; 20 aircraft averaged over 34 sorties/day at Pohang [S9 p.119] | Cruise altitude —. Flew under ceilings below 46 m [150 ft] [S9 p.119]; let down through holes in cloud [S9 p.183]; climbed over coastal ridges of 1,520–1,830 m [5,000–6,000 ft] [S9 p.112], [S9 p.85] | Bomb run, then circle back for strafing passes [S9 p.156]; napalm and strafing within 46 m [50 yd] of friendly troops [S9 p.184]; bombs plus rockets [S9 p.162]; flak suppression with guns, rockets and napalm [S9 p.245] | 6 × .50 cal; 2 × 227 kg [500 lb] bombs [S9 p.156]; light-case thermite/napalm bombs [S9 p.117]; rockets (number —) [S9 p.162]; drop tanks for long missions [S9 p.255] | Mainly close support under Mosquito/TACP control from 11 Aug [S9 p.134], [S9 p.130]; armed reconnaissance [S10 p.56]; transport escort [S9 p.231]; 2–3 h over the lines from Taegu [S9 p.112] | Taegu K-2 (Jul; staging Aug–Sep) [S9 p.117], [S9 p.162]; Pohang K-3 (Jul–13 Aug, Oct–Nov) [S9 p.117], [S9 p.200]; Ashiya / Tsuiki (Aug–Sep) [S9 p.134]; Pusan East K-9 (Sep, Dec) [S9 p.180], [S9 p.289]; Suwon K-13 / Kimpo K-14 (Oct) [S9 p.202]; Pyongyang K-23 / K-24, Yonpo K-27 (Nov) [S9 p.254]; Chinhae K-10 (Dec) [S9 p.290] |
| **F-80C** | Flights of 4 [S9 p.49]; flights launched every 20 min [S9 p.101]; 4 on air alert [S9 p.35]; 7-ship airfield strike [S9 p.123] | Cruise above 4,570 m [15,000 ft] for fuel economy [S9 p.81]; CAP orbit 3,050 m [10,000 ft] [S9 p.53]; high cover over transports [S9 p.34]; HVAR fired at ≈460 m [1,500 ft] from a 30° dive, with cloud base often ≤305 m [1,000 ft] [S9 p.111] | CAP for 15–20 min, then 1–2 strafing passes [S9 p.53]; HVAR attack from the 4 o'clock position, all four in one salvo [S9 p.111]; repeated passes on airfields [S9 p.123] | 6 × .50 cal; up to 16 HVAR; no bomb racks in Jul; 2 × 454 kg [1,000 lb] bombs possible in place of tip tanks [S9 p.81], [S9 p.110]; 625 L [165 gal] or 1,000 L [265 gal] tip tanks: 15 or 45 min over target [S9 p.109] | Air alert/CAP, close support under Angelo/Mosquito control, mainly interdiction and armed reconnaissance from Aug [S9 p.134], [S9 p.155]; top cover at the Yalu in Nov [S9 p.245] | Itazuke / Ashiya (Jun–Sep) [S9 p.89]; Taegu K-2 (49th, from 28 Sep) [S9 p.199]; Kimpo K-14 (51st, Oct–Dec) [S9 p.202]; Itazuke again from 10 Dec [S9 p.290] |
| **Yak (NK)** Yak-3, Yak-7B, Yak-9 | 2, 4, 5 or 6 per raid [S9 p.29], [S9 p.34], [S9 p.50]; attacked in pairs, threes and fours [S9 p.121]; 70 Yak-3/7B on 25 Jun [S9 p.41] | 3,050 m [10,000 ft] on the way to Kimpo [S9 p.34]; otherwise —; came out of or through cloud [S9 p.31], [S9 p.53] | Strafing and light-bomb attacks on airfields and transports, including aircraft in the landing pattern [S9 p.50]; sneak attacks timed for when the F-80s were short of fuel [S9 p.121]; dawn and night raids [S9 p.180], [S9 p.268] | Guns; light bombs [S9 p.121], [S9 p.180]; weights — | Airfield attack, anti-transport, attacks on ground troops, interception of liaison aircraft and bombers [S9 p.103], [S9 p.121] | Pyongyang (2 fields), Yonpo, Wonsan; forward strips at Sinmak etc. [S9 p.41]; Kimpo (Jul) [S9 p.121]; Sinuiju (Nov) [S9 p.241] |
| **Il-10** | 8 on 27 Jun [S9 p.35]; 2 detached to Sinmak [S9 p.41]; 62 on 25 Jun [S9 p.41] | — | Attacked transports at Kimpo; broke off after losing 4 [S9 p.35] | — | Ground attack (a ground-attack regiment) [S9 p.41] | Pyongyang / Heijo, Sinmak [S9 p.35], [S9 p.41] |
| **La-9 / La-11** | None in 1950. First La-9: escort of 16 aircraft, 30 Nov 1951 [S9 p.437]; first La-11: night hecklers, Apr 1953 [S9 p.686]. In 1950 only an La-7 [S9 p.53] and an La-5 [S9 p.124] | — | 1950 La-5: lone attack on a B-29 [S9 p.124] | — | — | — |

#### Early war 1950 — F-51

| Date | Topic | Fact (paraphrased) | Cite |
|---|---|---|---|
| 25 Jun 1950 | ROK request | President Rhee asked for 10 F-51s with bombs and rockets, to be handed to Korean pilots waiting at Taegu. | [S9 p.29], [S9 p.30] |
| Jun–Jul 1950 | Why Mustangs | Planners judged that the Mustang had a longer range than the F-80 and could use shorter, rougher strips. Stratemeyer called the F-51 and F-82 "exceptionally well suited" for the long-range, low-level missions Korea required. | [S9 p.90] |
| 29 Jun 1950 | First USAF Mustang combat | The 8th FBW used a flight of F-51s that was being readied for ROK pilots as part of the escort for MacArthur's C-54 at Suwon. Four Yaks came in undetected through scattered cloud. The Yaks were slightly more manoeuvrable, but the Mustangs were faster, and all four Yaks were shot down. | [S9 p.53] |
| 30 Jun – Jul 1950 | "Bout-One" at Taegu | 10 Mustangs went to the ROKAF, with 9 USAF instructor pilots. The unit moved to Taegu (K-2) on the evening of 30 Jun. The F-51 was too tricky for the inexperienced Korean pilots, so Americans flew all the combat missions. The aircraft were worn-out target tugs. Taegu had a sod-and-gravel runway full of potholes. | [S9 p.111] |
| Jul 1950 | Bout-One profile | Requests were informal (from KMAG, the 24th Division, or Walker in person). Pilots bombed on the Hamchang front, then climbed over the mountains to strafe targets of opportunity on the east coast. Based near the front, the Mustangs could search the enemy lines for 2–3 h. | [S9 p.112] |
| 7 Jul 1950 | Conversion decision | FEAF agreed to convert six F-80 squadrons to F-51s. USAF had 764 ANG F-51s and 794 in storage; 145 were being recalled for shipment on the carrier *Boxer*. | [S9 p.91], [S9 p.92] |
| 8 Jul 1950 | Value judgement | Timberlake: one F-51 supported and fought from Taegu was worth four F-80s based on Kyushu, partly because the Mustang could carry napalm. | [S9 p.116] |
| 10 Jul 1950 | Stage-through pattern | Ten Mustangs taken from storage flew from Itazuke, struck targets, landed at Taegu to rearm, and flew several more missions that day. Fuel and armament were airlifted from Ashiya. | [S9 p.113] |
| 10–16 Jul 1950 | First units in Korea | The 51st Fighter Squadron (Provisional) was formed at Taegu on 10 Jul. The "Dallas" squadron flew its first missions on 15 Jul. The 40th FIS (35th FIG) was the first squadron to convert, and moved to Pohang (K-3) on 16 Jul. Pohang got a 152 m [500 ft] PSP runway extension and 27 Mustang hardstands. | [S9 p.117] |
| Jul 1950 | Loads at Taegu | Light-case 227 kg [500 lb] bombs filled with thermite or napalm were used against tanks and troops. Even a near miss with napalm usually set a tank alight (rubber in the tracks). | [S9 p.117] |
| mid-Jul 1950 | Pohang sortie rate; ceilings | The 40th Squadron averaged over 34 sorties a day with 20 F-51s when the weather allowed. For a week it flew under ceilings of less than 46 m [150 ft]. | [S9 p.119] |
| 23–30 Jul 1950 | Reinforcement | 145 F-51s arrived at Tokyo on the *Boxer* on 23 Jul. On 30 Jul the two squadrons in Korea were each brought up to their authorised strength of 25 aircraft. | [S9 p.133] |
| 30 Jul – 11 Aug 1950 | More conversions | The 18th FBG moved to Ashiya on 30 Jul, and its HQ went to Taegu on 3 Aug; the 67th Squadron stayed at Ashiya. The 39th Squadron got Mustangs on 7 Aug and moved to Pohang on 8 Aug. The 8th FBG converted on 11 Aug while moving to Tsuiki. S10 says the 39th arrived on 10 Aug. | [S9 p.133], [S9 p.134], [S10 p.70] |
| 11 Aug 1950 | Role split | The Mustang had the range to reach anywhere in Korea and could use crude strips. Partridge ordered Mustangs to be used primarily for close support, and F-80s mainly for interdiction sweeps. | [S9 p.134] |
| 4–13 Aug 1950 | Pull-back to Japan | After the Naktong withdrawal the 18th FBG went back to Ashiya on 6 Aug. The 6149th Air Base Unit stayed at Taegu to service Mustangs staging through. The 35th FIG evacuated Pohang to Tsuiki on 13 Aug. | [S9 p.141], [S9 p.146] |
| 5 Aug 1950 | Attack sequence (example) | Maj. Sebille led a flight of Mustangs against guns and troops near Hamchang. Each aircraft carried two 227 kg [500 lb] bombs. The flight made an initial bombing attack, then circled and came back for strafing passes with six .50-cal guns, and drew ground fire. | [S9 p.156] |
| Aug 1950 | Reporting targets | Crews who sighted enemy movement called it by voice radio to the first armed-reconnaissance flight that answered. On 25 Aug one fighter's remaining napalm bomb blocked a tunnel so that a train could be destroyed. | [S9 p.156] |
| Jul–Aug 1950 | Night sorties failed | Mustang night-harassment missions gave "almost nil" results because the flash of the pilots' own rockets and guns blinded them. | [S9 p.157] |
| 18–30 Aug 1950 | Short-range staging | Mustangs from southern Japan struck, landed at Taegu to refuel and rearm, and attacked targets close enough to be watched from the strip. On 30 Aug a flight of Mustangs bombed and rocketed troops NW of Pohang, and the ROK counted 700 dead. | [S9 p.162] |
| Aug–Sep 1950 | Close-support tempo (all types) | Close-support sorties averaged 175 a day over ten days (Walker, 10 Aug). August totalled 7,397 close-support sorties, 238 a day. Peak day, 11 Sep: 307 support and 130 interdiction sorties. | [S9 p.144], [S9 p.159], [S9 p.168] |
| 1–4 Sep 1950 | Perimeter crisis (all types) | 1 Sep: 167 close-support sorties along 64 km [40 mi] of front. During the crisis Mustangs encircled a company's attackers with fire. 2 Sep, generally poor weather: 201 close-support sorties. 4 Sep, bad weather: 43 sorties claimed 11 tanks. | [S9 p.164], [S9 p.165], [S9 p.167] |
| 7–14 Sep 1950 | Pusan East; typhoon | The 18th FBG moved to Pusan East (K-9) on 7 Sep and gave close support "in the foulest weather". To avoid typhoon Kezia, the 8th FBG worked its Mustangs from Taegu on 12–14 Sep. S10 gives the 18th FBG at Pusan East from 8 Sep, but its campaign summary says all FEAF units flew from Japan in Aug–Sep, with Taegu as a staging field only, until 28 Sep. | [S9 p.180], [S10 p.56], [S10 p.19] |
| 16–19 Sep 1950 | Breakout support | 16 Sep: F-80s and F-51s let down through holes in the cloud until the weather closed in. 17 Sep: fighter-bombers dropped 260 napalm tanks of 416 L [110 gal] each on troops retreating across the Naktong. 19 Sep: Mustangs used napalm and strafing within 46 m [50 yd] of friendly troops. | [S9 p.183], [S9 p.184] |
| 21 Sep 1950 | Column cover | Mosquitoes had carried SCR-300 radios since August, so they could talk directly to tank columns. When 30 enemy tanks moved up, Mustangs and F-80s knocked out 14 of them. | [S9 p.186] |
| 23–25 Sep 1950 | Identification | Four Mustangs mistakenly strafed and napalmed the Argylls. Pilots were then ordered to confirm every target as hostile, and some came home with bombs still on the racks. | [S9 p.189] |
| 3–30 Oct 1950 | Back to Korea | The 35th FIG (39th and 40th Squadrons) returned to Pohang on 3–7 Oct. RAAF 77 Sqn joined on 12 Oct, making three F-51 squadrons. The 8th FBG and its 35th Squadron went to Suwon on 7 Oct, where the field was barely adequate for one Mustang squadron. They moved to Kimpo on 30 Oct. | [S9 p.200], [S9 p.202] |
| Oct 1950 | Fuel handling | At Suwon, aircraft were fuelled by hand from 208 L [55 gal] drums trucked from Inchon. | [S9 p.203] |
| 15 Oct 1950 | Yalu flak | Four Mustangs sent to Sinuiju drew heavy AA fire from across the Yalu, and one was shot down. | [S9 p.239] |
| 20 Oct 1950 | Airborne-drop support | Mustangs escorted the transport fleet from Kimpo. The Fifth Air Force used 75 F-51s, 62 F-80s and 5 B-26s for pre-drop strikes and for support under Mosquito "Nightmare". | [S9 p.231] |
| 1–7 Nov 1950 | MiG appearance | On 1 Nov six MiG-15s fired on a Mosquito and a flight of Mustangs, which escaped. Two 18th Group Mustangs were called in to finish off Yaks that same day. On 7 Nov MiGs intercepted Mustangs in five engagements south of the Yalu. | [S9 p.241], [S9 p.244] |
| 8 Nov 1950 | Flak suppression | Before the B-29s arrived over Sinuiju, F-80s and F-51s raked the AA positions with guns, rockets and napalm. | [S9 p.245] |
| Nov–Dec 1950 | Versus MiG | A Mustang pilot jumped by a MiG could only keep turning inside it, get down to the deck and head for home. | [S9 p.266] |
| 17–25 Nov 1950 | North Korean strips | The 35th FIG and 77 RAAF took off from Pohang, flew strikes, and landed at Yonpo (K-27) on 17–19 Nov. The 18th FBG and 2 SAAF were at Pyongyang East (K-24) by 22 Nov. The 8th FBG had two squadrons at Pyongyang (K-23) by 25 Nov. Yonpo's surfaced strip was about 915 m [3,000 ft] long. Pyongyang East was sod, either dusty or muddy. | [S9 p.254] |
| Nov 1950 | Distance effect; hazards | From Pusan, 18th Group missions lasted up to 5 h. From Pyongyang they were shorter, gave more time to find targets, and did not need drop tanks. Two Mustangs crashed on landing in swirling dust. Low cloud, snow flurries and morning fog made targets hard to identify. | [S9 p.255] |
| 28 Nov 1950 | Night raid on Mustang base | At about 0300 a light plane (probably a Po-2) bombed the 8th FBG ramp at Pyongyang, damaging 11 Mustangs. Three were so badly hit that they had to be destroyed at evacuation. | [S9 p.268] |
| 28 Nov 1950 | X Corps support | Partridge sent the Yonpo-based 35th FIG to support X Corps. | [S9 p.277] |
| 30 Nov – mid-Dec 1950 | Retreat | The 8th and 18th Wings moved to Seoul and Suwon in four days, and the Mustangs never missed a day's operations. The 18th FBW went to Chinhae (K-10), leaving a detachment at Suwon to service Mustangs staging forward. S9 says the 35th FIW moved from Yonpo to Pusan East on "3 November", but the context is December; S10 gives c. 3 Dec. | [S9 p.288], [S9 p.289], [S9 p.290], [S10 p.57], [S10 p.70] |
| 10 Dec 1950 | 8th Wing leaves the type | The 8th Wing gave its flyable Mustangs to other wings and moved to Itazuke to re-equip with F-80Cs. | [S9 p.290], [S10 p.54] |
| 28 Dec 1950 | Very close support | Four 67th FBS Mustangs bombed and strafed 73 m [80 yd] beyond friendly lines near the Hwachon reservoir, after which more than 100 Chinese surrendered. | [S9 p.285] |
| 1950 | S10 unit notes | The 35th FIG went back to the "rugged and longer-range" F-51. The 8th FBG's F-51 used less fuel and could loiter longer over targets. The 18th FBG arrived with F-80s and exchanged them for F-51s. The 18th FBG's DUC covers about 2,400 vehicles destroyed, Nov 1950 – Jan 1951. | [S10 p.70], [S10 p.54], [S10 p.56] |

#### Early war 1950 — F-80 (F-82 rows marked)

| Date | Topic | Fact (paraphrased) | Cite |
|---|---|---|---|
| 25 Jun 1950 | Order of battle | 8th FBW at Itazuke: F-80C, plus the 68th FAWS with F-82s. 49th FBW at Misawa: F-80C. 35th FIW at Yokota: F-80C, plus the 339th FAWS with F-82s. | [S9 p.25] |
| 31 May 1950 | Strength | FEAF units held 365 F-80s and 32 F-82s. | [S9 p.80] |
| Jul 1950 | Runways | Only four airfields in Japan had the 2,130 m [7,000 ft] runways that combat-loaded jets needed. Heavy transports wrecked the runway at Pusan, so all jets were based on Kyushu (Itazuke and Ashiya). | [S9 p.81], [S9 p.89] |
| Jun 1950 | Korean airfields | Only Kimpo and Suwon suited high-performance aircraft. Runways: Suwon 1,490 m [4,900 ft] concrete; Pusan 1,500 m [4,930 ft]; Pohang 1,520 m [5,000 ft]; Taegu 1,160 m [3,800 ft] clay and gravel. | [S9 p.87] |
| 26 Jun 1950 | **F-82** evacuation CAP | Covering the evacuation ships was judged a job for long-range conventional aircraft, not fuel-hungry jets. The 68th FAWS had 12 operational F-82s; the 339th and 8 aircraft of the 4th Squadron came up to reinforce. | [S9 p.30] |
| 26 Jun 1950 | **F-82** first contact | At 1333 a radial-engine Communist fighter came out of the clouds and bounced two F-82s, which evaded. After dark, two F-82s escorted the evacuation ship. | [S9 p.31] |
| 27 Jun 1950 | **F-82** route escort | The first transports left Itazuke before dawn with F-82 route escort. | [S9 p.34] |
| 27 Jun 1950 | Air alert kills | Four 35th FBS F-80Cs on air alert over Seoul shot down four of eight Il-10s with a minimum of manoeuvring. These were the first USAF jet victories. | [S9 p.35] |
| 28 Jun 1950 | Planned effort | The Fifth Air Force was ordered to use four F-80 squadrons and two F-82 squadrons (plus two B-26 squadrons) against tanks, guns, columns and bridges. | [S9 p.47] |
| 28 Jun 1950 | Range and weather | Ceilings at Itazuke were about 60 m [200 ft]. Itazuke to the Han was about 500 km [310 mi], and every F-80 came home short of fuel. Six flights of four F-80s went out mid-morning and again mid-afternoon; their fires were visible for about 80 km [50 mi]. The **F-82**s flew 11 sorties, mostly top cover for transports at Suwon. | [S9 p.49] |
| 29 Jun 1950 | **F-82** first napalm | F-82s flew most of the day's close support. The 68th Squadron made the first napalm attack, dropping jettisonable fuel tanks as fire bombs. | [S9 p.52] |
| 29 Jun 1950 | Patrol-then-strafe profile | F-80s carried .50-cal ammunition only, with no bombs or rockets. They orbited over the Han at 3,050 m [10,000 ft] for 15–20 min. If no enemy aircraft appeared, they made one or two strafing passes on road traffic around Seoul and returned to Itazuke. That day they shot down an La-7 and an Il-10. | [S9 p.53] |
| 30 Jun 1950 | Same tactic; low-level hazard | The patrol-and-strafe tactic continued. 36th Squadron pilots bounced two Yak-9s and shot both down. One F-80 flew through a power line. | [S9 p.55] |
| Jul 1950 | Rocket training | Few FEAF pilots had ever fired a 127 mm [5 in] HVAR; they had to learn in combat. The Misawa tanks gave about one extra hour of flight. | [S9 p.82] |
| Jul 1950 | Control and scheduling | F-80 flights left Itazuke and Ashiya at 20-min intervals through the day and checked in with "Angelo" control over Taejon. If there was no target, they flew armed reconnaissance of the Osan–Seoul roads. A TACP was one pilot (the forward air controller) plus radio airmen with a jeep radio. | [S9 p.101] |
| Jul 1950 | Periodic flights | F-80s of the 8th, 35th and 49th FBGs flew from Ashiya and Itazuke at periodic intervals from dawn to dusk. They took targets from Army liaison aircraft or Air Force controllers, or else searched the lines of communication. | [S9 p.107] |
| 8–11 Jul 1950 | TACP limits | The weather was murky with the ceiling "on the ground". The first TACP-controlled F-80 flight was on 8 Jul. By 11 Jul only three radio jeeps still worked. Jets could spend only a short time at low altitude, so they needed a target ready the moment they arrived. | [S9 p.102] |
| 9–15 Jul 1950 | Airborne controllers | On 9 Jul two airborne controllers each directed about ten F-80 flights; one was bounced by two Yaks. A T-6 was tried on 10 Jul. From 15 Jul the T-6s used the call sign "Mosquito", and they carried eight-channel AN/ARC-3 radios. | [S9 p.103], [S9 p.105] |
| Jul 1950 | Strike timeline | A close-support strike took about 40 min from the TACP request until the aircraft were back at base (diagram; its individual times are illegible). | [S9 p.104] |
| Jul 1950 | Distance problem | S9: the F-80s were based about 240 km [150 mi] too far from their targets. On 7 Jul two 35th FBS pilots made dead-stick landings at Ashiya after running out of fuel. S10: the bases were more than 240 km [150 mi] from the front, so after a short time over the target the F-80s had to go back to Japan. | [S9 p.109], [S10 p.16] |
| Jul 1950 | Strafer; no bomb racks | The F-80 was an excellent strafer: no propeller torque to upset aim, six .50-cal nose guns, and speed that let it arrive before troops scattered. It had no wing bomb racks yet. With the 1,000 L [265 gal] Misawa tanks it was "not quite so aerobatic". | [S9 p.110] |
| by 15 Jul 1950 | Share of effort | F-80s flew 70% of all combat sorties and were credited with 85% of enemy losses to air attack. In mid-July they flew almost 200 sorties a day. | [S9 p.109], [S9 p.117] |
| 10 Jul 1950 | Pyongtaek convoy | An F-80 flight slipped in under the clouds and found a convoy halted at a blown bridge. B-26s, F-82s and F-80s then destroyed 117 trucks, 38 tanks and 7 half-tracks. There were 280 strikes that day. | [S9 p.113] |
| Jul 1950 | Ground fire | 49th FBG CO: when strafed, NK soldiers stood up in their trucks and fired rifles at the aircraft. Small-arms fire was costly to low-flying piston bombers, so by 7 Jul the B-26s had moved to medium altitude. | [S9 p.107], [S9 p.108] |
| Jul 1950 | **F-82** role and withdrawal | The F-82 had the range to escort B-29s into North Korea and to hunt targets at night along the Han. It was FEAF's only night and bad-weather interceptor, so it had to be withdrawn from combat. USAF had only 168 F-82s and could support them for no more than 60 days. | [S9 p.107], [S9 p.108], [S9 p.91] |
| 17–20 Jul 1950 | Ground-controlled intercepts | 8th Group F-80s shot down Yaks on 17, 19 and 20 Jul. Ground controllers directed them to the Yaks even when they were low on ammunition. | [S9 p.123] |
| 19 Jul 1950 | Airfield strike | Seven 8th FBG F-80s came in at low level over the Pyonggang strip and made pass after pass. They destroyed 14 fighters and a twin-engine bomber, and damaged 7 more. | [S9 p.123] |
| Aug 1950 | No escort needed | Early in August Bomber Command told the Fifth Air Force its B-29s no longer needed fighter escort. | [S9 p.125] |
| Aug 1950 | Armed reconnaissance | Pilots flew road sweeps when there were no close-support targets. The F-80 was the best armed-reconnaissance type: less vulnerable to small arms and automatic weapons, and fast enough to attack before troops dispersed or fired back. An NK staff officer said his soldiers feared the jets because they arrived before their sound. Partridge put about one-third of his effort into interdiction. | [S9 p.155], [S9 p.153] |
| Jul–Aug 1950 | **F-82** / F-80 at night | In July one flight of three 68th FAWS F-82s flew night missions, useful only against fixed targets. F-80 pilots found night strafing almost impossible. On 30 Aug two F-82 crews knocked out three locomotives north of Seoul at night. | [S9 p.157], [S9 p.158] |
| 1–4 Sep 1950 | Japan squadrons released | F-80 squadrons held back for the defence of Japan were released: the 80th FBS at Itazuke, and the 9th FBS, which rejoined the 49th at Itazuke on 4 Sep. Partridge was fighting with 8 fighter squadrons while 6 defended Japan. | [S9 p.174], [S9 p.175] |
| 22 Sep 1950 | 51st FIG arrives | The 16th and 25th FIS flew from Naha to Itazuke and some pilots were over Korea within 2 h of landing. S10: they flew F-80 CAP, armed reconnaissance and close support. | [S9 p.175], [S10 p.72] |
| 28 Sep – Oct 1950 | 49th FBG at Taegu | The 49th was the first jet group based under field conditions in Korea (S10 says it moved on 1 Oct). Its PSP runway was 1,740 m [5,700 ft] long. Main-gear tyres lasted 7–8 landings, or 22 with practice. Water-alcohol injection cut the take-off roll by about 150 m [500 ft]; without it the jets probably could not have used Taegu. Aircraft in commission in Oct: 82.55%. | [S9 p.199], [S9 p.204], [S10 p.58] |
| Oct 1950 | Kimpo | The 51st FIW took over Kimpo on 6 Oct, and the 80th FBS arrived on 25 Oct. About 227,000 L [60,000 gal] of jet fuel a day had to be trucked in. | [S9 p.202], [S9 p.203] |
| 8 Oct 1950 | Marginal weather | Two 49th Group F-80 pilots searching for targets in marginal weather strafed a Soviet airfield across the Siberian border. | [S9 p.171] |
| Oct 1950 | Border rules | Targets within about 80 km [50 mi] of the border could be attacked only on FEAF special order and under visual conditions. On 17 Oct a "chop line" was drawn about 32 km [20 mi] from the border, with visual armed reconnaissance allowed south of it. From 25 Oct, close support under TACP or airborne control could go as close to the border as needed, flown by "selected" pilots. | [S9 p.229], [S9 p.236], [S9 p.237] |
| 7–17 Oct 1950 | Mosquito-led attacks | 7 Oct: Mosquito "Antidote" called in F-51 and F-80 flights on anti-tank guns. 17 Oct: Mosquito "Hammer" found a troop train and four F-80s smashed it; three more flights attacked troops nearby. | [S9 p.233], [S9 p.230] |
| 1 Nov 1950 | Sinuiju strafe | Three F-80 flights strafed 15 Yaks in revetments. The revetments opened toward the Yalu, and flak from across the river shot down one F-80. | [S9 p.241] |
| early Nov 1950 | Flak at Sinuiju | Partridge reported that flak dispersed through Sinuiju had killed one of his pilots. | [S9 p.243] |
| 8 Nov 1950 | First jet combat | 51st FIW F-80s flew top cover and were engaged by MiGs. The MiG was clearly superior to the F-80C, but Lt. Brown shot one down. S10: B-29s bombed from at least 6,100 m [20,000 ft] to stay above flak. | [S9 p.245], [S10 p.23] |
| Nov–Dec 1950 | Outclassed | In level flight the MiG was about 161 km/h [100 mph] faster than the F-80C, and climbed away from it. RF-80s at the border had to be escorted by F-80s, which were inadequate for the job. Because of cautious tactics, no Fifth Air Force aircraft was lost air-to-air in November. | [S9 p.266], [S9 p.268] |
| 1 Dec 1950 | "The Pass", Kunu-ri (type not stated) | Relays of fighter-bombers flew so low into the defile that it seemed some must crash. Napalm spilled down the cliffs, .50-cal fire chipped the rock, and rocket blasts concussed friendly troops. In one day the 38th Regiment received 72 sorties. S10: the 51st FIG flew 763 sorties in December, including close support at Kunu-ri. | [S9 p.276], [S9 p.277], [S10 p.72] |
| Dec 1950 | Chinese in daylight | Early in December the Chinese marched in daylight and kept their vehicle lights on at night. 49th Group flight commanders claimed hundreds of troops killed at the Chongchon crossings. From mid-December the Chinese hid their equipment at dawn. FEAF flew 7,654 armed-reconnaissance and interdiction sorties in December. | [S9 p.284], [S9 p.285], [S9 p.283] |
| 10 Dec 1950 | Back to Japan | The 51st Wing left a combat echelon at Kimpo and moved its main body to Itazuke. The 8th Wing was flying F-80s again at Itazuke before the end of December. | [S9 p.290] |

#### Early war 1950 — North Korean Yak / Il-10

| Date | Topic | Fact (paraphrased) | Cite |
|---|---|---|---|
| 25 Jun 1950 | First raids | At 1315 two silver Yaks buzzed Seoul and Kimpo without attacking. Later two strafed Kimpo while four strafed Seoul airfield, damaging 7 of 10 ROK trainers. At about 1900 six more fighters strafed Kimpo. | [S9 p.29], [S10 p.15] |
| Jun 1950 | Forward strips; pilots | Besides the Pyongyang, Yonpo and Wonsan fields, the NKAF was building forward strips at Sinmak, Pyonggang, Kumchon and Kansong. Its pilots were young volunteers with limited experience but cocky and aggressive. | [S9 p.41] |
| 26 Jun 1950 | Cloud ambush | At 1333 a single radial-engine fighter came out of cloud and bounced two F-82s over Inchon, then broke off. | [S9 p.31] |
| 27 Jun 1950 | Il-10 raid | Eight Il-10s (called "fighters" in S9) attacked the transports at Kimpo. Four were shot down by four F-80Cs and the rest fled, probably to Heijo (Pyongyang). S10: 3 NK fighters were destroyed that day. | [S9 p.35], [S10 p.15] |
| 28 Jun 1950 | Suwon raids | About 1330: four Yaks strafed Suwon. About 1830: six Yaks working in pairs jumped a C-54 in the landing pattern and destroyed another on the ground. | [S9 p.50] |
| 29 Jun 1950 | Suwon and Kimpo | Two Yaks and an unidentified fighter attacked B-29s over Kimpo. Suwon was attacked six times, once by six Yaks. An La-7 and an Il-10 were shot down. Four Yaks approached through scattered cloud and met Mustangs. Only one Yak-3 opposed the B-26 raid on Pyongyang, which claimed 25 aircraft on the ground. | [S9 p.51], [S9 p.53], [S9 p.54] |
| 30 Jun 1950 | Yak-9 | Two Yak-9s were bounced and shot down by F-80s. | [S9 p.55] |
| Jun–Jul 1950 | Pilot quality | A captured pilot said the NKAF had 80 pilots: 2 good and 40 fair. FEAF thought Soviet instructors possibly flew early missions. On 2 Jul B-29s found only 16 aircraft at Yonpo. | [S9 p.120] |
| 9 Jul 1950 | Anti-liaison | Two Yaks bounced an airborne controller. Yaks had already shot down several liaison aircraft, which is one reason the faster T-6 replaced them. | [S9 p.103] |
| 10–15 Jul 1950 | Sneak-attack pattern | 10 Jul: four Yaks bombed and strafed the US 19th Regiment. 11 Jul: three Yaks surprised F-80s that were strafing; the jets were too low on fuel to counter. 12 Jul: two Yaks jumped F-80s strafing near Chochiwon, and two others shot down an L-4. 15 Jul: two Yaks attacked four B-26s; the same day strafers destroyed 2–3 of the dispersed Yaks at Kimpo. | [S9 p.121] |
| 19 Jul 1950 | Camouflaged strip | About 25 aircraft were hidden under branches along a grass strip near Pyonggang. 14 fighters and a twin-engine bomber were destroyed there. | [S9 p.123] |
| after 20 Jul 1950 | Effectively neutralised | After 20 Jul the NKAF made virtually no more offensive use of its aircraft. On 22 Jul FEAF estimated 65 aircraft left, perhaps 30 serviceable. S10: 20 NK aircraft shot down in June–July, and air superiority by the end of July. | [S9 p.123], [S10 p.16] |
| Aug 1950 | Residual activity | Revetments were repaired at Kimpo and Suwon. On 4 Aug fighters were seen taking off from Kimpo; 9 were destroyed there on 5 Aug and 9 at Pyongyang on 6 Aug. The NKAF used dummies and propped-up wrecks and kept moving its few aircraft between fields. By 10 Aug 110 had been destroyed and 35 remained. 23 Aug: two Yaks damaged a British destroyer. By end August: at most 18 aircraft and about 16 sorties a day. | [S9 p.124] |
| 11–17 Sep 1950 | Before Inchon | 11 Sep: a Yak was destroyed at Sinmak. 12 Sep: fighters caught ground crews camouflaging four Yaks at Pyongyang. 17 Sep at daybreak: two Yaks near-missed USS *Rochester* with light bombs, and one was shot down while strafing HMS *Jamaica*. Joy warned the enemy might have up to 180 fighters available. | [S9 p.180], [S9 p.181] |
| 14–18 Oct 1950 | Night nuisance; Antung | Two aircraft bombed Kimpo at 0400 and again at 2110 on 14 Oct. On 18 Oct an RB-29 counted more than 75 fighters at Antung. | [S9 p.239] |
| 1 Nov 1950 | Yak return | Three Yaks bounced a Mosquito and a B-26 near Yangsi. 15 Yaks were in revetments at Sinuiju, protected by flak from across the Yalu. | [S9 p.241] |
| Nov 1950 | Too slow for sanctuary tactics | The Yaks copied the MiGs' cross-border dive attacks but were too slow. Seven were shot down in the first week of November. | [S9 p.244] |
| 19–28 Nov 1950 | Night and dawn raids | 19 Nov before daybreak: a single-engine aircraft bombed Sunchon, and another bombed and strafed Eighth Army positions on the Chongchon. 28 Nov at 0300: a "light liaison" plane, probably a Po-2, bombed the Mustang ramp at Pyongyang. | [S9 p.268] |
| Jul–Sep 1950 | Ground AA threat (context) | NK divisions had mainly heavy AA machine guns, "mere toys" against modern aircraft. A regimental order diverted 50% of infantry weapons to AA fire during a river crossing. | [S9 p.194], [S9 p.193] |

#### Early war 1950 — La-9 / La-11

**Answer:** S9 records **no La-9 or La-11 in 1950**. The only Lavochkins it names in 1950 are an **La-7** (29 Jun) and an **La-5** (15 Aug). The first La-9 in S9 is on **30 Nov 1951**. The first La-11 is in the night-heckler raids of **mid-April 1953**.

| Date | Topic | Fact (paraphrased) | Cite |
|---|---|---|---|
| 29 Jun 1950 | La-7 | F-80 pilots Norris and Marsh shot down an La-7 and an Il-10 near Suwon. | [S9 p.53] |
| 15 Aug 1950 | La-5 | An La-5 attacked a 307th BG B-29 and was driven off by two bursts from the tail gunner. | [S9 p.124] |
| Dec 1950 | CCAF order of battle (types not named) | 650 combat aircraft, including 250 conventional and jet fighters and 175 ground-attack aircraft. The CCAF commander had promised air support to the Fourth Field Army. S6: the CCAF began with Soviet help in 1950. | [S9 p.267], [S9 p.297], [S6 p.8] |
| Mar 1951 | Il-10 regiments | Two air regiments equipped with Il-10s were training near Kaiyuan in Manchuria. | [S9 p.307] |
| Apr–May 1951 | La-5 at Sinuiju | Yak-9s, Il-10s and La-5s (38 in all) sat in revetments at Sinuiju. | [S9 p.324] |
| 30 Nov 1951 | **First La-9** | 12 Tu-2s escorted by 16 La-9s, with 16 MiGs as cover, headed for Taehwa-do. | [S9 p.437] |
| 15 Apr 1953 → | **First La-11** | North Korean night fliers used Po-2s, La-11s and Yak-18s against Chunchon, Kimpo and the front line. | [S9 p.685], [S9 p.686] |
| 1 Jul 1953 | La-9 or La-11 | Bordelon (Corsair) shot down two light aircraft identified as either La-9s or La-11s. | [S9 p.687] |

## 3. Air — Communist (Soviet, Chinese, North Korean)

| Date | Topic | Fact (paraphrased) | Cite |
|---|---|---|---|
| 25 Jun 1950 | NKAF strength | One air division (a fighter, a ground-attack and a training regiment). **62 Il-10, 70 Yak-3/Yak-7B, 22 Yak-16, 8 Po-2**, i.e. 132 combat planes. They were based at the two Pyongyang fields and Yonpo. On 26 Jun, 10 Yak-7Bs and 2 Il-10s moved to Sinmak. | [S9 p.41] |
| Jun 1950 | NKAF personnel | About 2,000 men and **80 pilots** (a POW report). | [S9 p.120] |
| 27 Jun 1950 | Yak raid | **Five Yaks at 3,050 m [10,000 ft]** crossed Seoul heading for Kimpo and met five F-82s. | [S9 p.34] |
| Jul 1950 | NKAF tactics | 4 Yak-9s in South Korean markings strafed Osan. **Seven camouflaged, dispersed Yaks at Kimpo** flew short sneak attacks timed for when the F-80s had left for lack of fuel. Attacks came in pairs, threes and fours. | [S9 p.121] |
| 1 Nov 1950 | First MiG-15s | 3 Yaks bounced a Mosquito and a B-26. Later **six swept-wing jets** crossed the Yalu, the first MiG-15s. | [S9 p.241] |
| Nov 1950 | Hit-and-run from sanctuary | MiGs took off from Antung and climbed on their own side of the river. They crossed the border at **≈9,140 m [30,000 ft]**, dived on UN aircraft, then went back across the Yalu. The Yaks tried the same thing and were too slow. | [S9 p.244] |
| Nov–Dec 1950 | Early MiG behaviour | MiGs hugged the Yalu and attacked from high and behind, **rarely making more than two passes**, and their gunnery was poor. On 4 Dec MiGs boxed in an RF-80: one MiG took the tail while others flew on its wings ≈46 m [50 yd] out. | [S9 p.268] |
| Dec 1950 | CCAF strength; Antung | **650 combat aircraft** (250 fighters, 175 ground-attack, 150 twin-engine bombers, 75 transports), plus 400–500 Soviet aircraft near Dairen. Antung had a new 1,830 m [6,000 ft] concrete runway. Its early-warning radar tracked FEAF bombers at ≈241 km [150 mi]. | [S9 p.267] |
| Dec 1950 | MiG counter-tactics | MiGs timed attacks for the **end of Sabre patrols** (Sabres short on fuel), always attacking from above at maximum speed. MiGs engaged: 22 Dec, >15; 30 Dec, 36 MiGs against 16 F-86s. | [S9 p.273], [S9 p.274] |
| Mar 1951 | MiG base | At least one air division with **75 MiGs at Antung**. | [S9 p.318] |
| Apr 1951 | MiG formations | MiGs flew **four-ship flights in formations of up to 16**. Jumped four-ship flights split into pairs, one climbing and one diving. On 12 Apr, 40–50 MiGs attacked eight B-29s. On 22 Apr, 36 MiGs crossed the Yalu. | [S9 p.320], [S9 p.323], [S9 p.324] |
| Apr–May 1951 | Pistons at Sinuiju | **38 Yak-9, Il-10 and La-5** sat in revetments under heavy flak and MiG cover. When the airfield was attacked on 9 May, ≈50 MiGs took off but only 18 crossed the Yalu. | [S9 p.324] |
| Jun–Jul 1951 | "Bedcheck Charlie" Po-2 | Po-2 biplanes flew at **80 kt [≈148 km/h]**, low along moonlit valleys, arriving after midnight in pairs. Radar saw them only ≈19 km [12 mi] north of Seoul. Each dropped a pair of small bombs; on 17 Jun they destroyed 1 Sabre and damaged 8 at Suwon. The raids stopped after 12 Jul 1951. | [S9 p.331], [S9 p.332], [S9 p.334] |
| 20 Jun 1951 | Il-10 sortie | **8 Il-10s** headed for Sinmi-do; **6 Yak-9s** joined in; MiGs arrived later. The ground-attack test failed. | [S9 p.332] |
| Jun 1951 | "Yo-Yo" | **20+ MiGs orbited above** UN formations. **Pairs** dived from up-sun to attack from high astern, then zoomed back up. Using drop tanks, the MiGs reached Pyongyang and Chinnampo. | [S9 p.333] |
| Jun 1951 | Strength | **1,050 combat planes**, ≈690 of them in Manchuria. The Antung, Ta-tung-kou and Ta-ku-shan fields could hold >300 MiGs. There were **445 MiG-15s against 89 F-86s**. | [S9 p.423], [S9 p.424] |
| Jul–Sep 1951 | Altitude advantage | MiGs had the edge **above 10,670 m [35,000 ft]**. They slipped past Sabre patrols and flew above 35,000 ft to Pyongyang, then let down onto fighter-bombers on the way home. By Sep 1951 the order of battle was **525 MiGs**, with **up to 90** in Korea at one time. | [S9 p.425], [S9 p.426] |
| Sep 1951 | New formations | MiGs used trail attacks and the Lufbery circle. In one case **four flights (16) made line-abreast head-on passes** at a single Sabre. **"Pools" of MiGs orbited high** and made dive-and-zoom passes. In Sep, 1,177 MiG sorties were sighted and 911 engaged. | [S9 p.426], [S10 p.35] |
| Oct 1951 | Peak | 2,573 MiGs sighted and 2,166 engaged in the month. On 23 Oct about **100 MiGs** boxed in the Sabre screen while about **50** hit the B-29s. MiGs moved in: 26 to Uiju, and 64 conventional aircraft to Sinuiju. | [S9 p.432], [S9 p.433], [S9 p.434] |
| Nov 1951 | Antung complex; MiG-15bis | **290 MiG-15s** on the Antung bases, with more staging through from Anshan, Liaoyang and Mukden. The MiG-15bis appeared, with a 6,000-lb-thrust VK-1 engine. Observed sorties: Nov 2,326; Dec 3,997. | [S9 p.434] |
| Nov 1951 | "Trains" (pincer and envelopment) | **Coordinated "trains" of 60–80 MiGs** crossed the Yalu over Antung and the Sui-ho reservoir **above 10,670 m [35,000 ft]**. There was a "west coast train" and a "central train". Each dropped off flights to engage the Sabres while the main bodies met over Pyongyang and turned home. Some **dropped to 4,570 m [15,000 ft]** to catch fighter-bombers and stragglers. A fresh section covered the withdrawal as far south as Sinanju. | [S9 p.436] |
| 30 Nov 1951 | Bomber raid | **12 Tu-2s with 16 La-9 escorts and 16 MiGs as cover** headed for Taehwa-do. Tu-2s had already bombed the island on 6 Nov. | [S9 p.437] |
| Dec 1951 | Unaggressive trains | Trains came in over Sui-ho and patrolled at **10,670–12,800 m [35,000–42,000 ft]**, turning back at the Chongchon. Pilots rotated in training "classes" that started high and fast and came lower and more aggressive as they gained experience. | [S9 p.440], [S9 p.441] |
| Jan 1952 | Massed high formations | **100–200 MiGs** formed up over Manchuria and came in at ≈M0.99, in three sections: below, in, and above the contrails, up to **15,240 m [50,000 ft]**. | [S9 p.442] |
| Feb 1952 | Strength; altitude | **540 MiGs** at the Antung fields. Formations flew at **≥12,190 m [40,000 ft]**; some were seen at **16,150 m [53,000 ft]**. | [S9 p.443] |
| Mar–Apr 1952 | Hit-and-run | MiGs fought in **two-, four- and six-ship** formations. They came out **above 12,190 m [40,000 ft]** at high Mach, swept down looking for fighter-bombers and left at low altitude. On 13 Apr, 400–500 MiGs were seen parked at Ta-tung-kou. | [S9 p.443], [S9 p.444] |
| May 1952 | GCI-directed MiGs | MiGs were **ground-radar controlled**. They dropped through cloud onto UN aircraft and went after fighter-bombers within ≈64 km [40 mi] of the Yalu. 620 MiG sorties that month. | [S9 p.445] |
| Jun 1952 | Order of battle; radar | CCAF: 22 air divisions, **1,830 aircraft (1,000 jet fighters)**, 1,115 of them in Manchuria. Communist Far East total ≈7,000: ≈5,000 Soviet, 2,000 Chinese, ≈270 North Korean. Five MiG fields could each support **up to 300 aircraft**. The radar net had up to **25 early-warning and 11 GCI** stations. | [S9 p.528] |
| mid-1952 | GCI reach | Antung GCI placed fighters **within 3.2–8 km [2–5 mi]** of UN aircraft out to ≈113 km [70 mi], best within ≈145 km [90 mi] of Antung. Nearly **90% of MiGs** sighted were in MiG Alley; combat went up to 15,240 m [50,000 ft]. | [S9 p.529], [S9 p.530], [S9 p.531] |
| Jun–Oct 1952 | Monthly activity | MiG sorties: Jun 298; Jul 404 by day; Aug 1,155; Sep 1,857; Oct 1,360, mostly **≥13,110 m [43,000 ft]**. Tactics were end runs, decoys and yo-yo. About 175 MiGs were met on 9 Sep. | [S9 p.534], [S9 p.535], [S9 p.536] |
| Jun 1952 → | Night fighters | Over Kwaksan, **24 searchlights** held the B-29s while an airborne controller paced the stream; **≈12 jets** attacked. Night-fighter sightings rose from 17 in Apr to 50 in May 1952. | [S9 p.447] |
| Nov 1952–Jan 1953 | Night-fighter methods | A spotter aircraft dropped flares above the bomber to guide searchlights and fighters. The night fighters had **no airborne-intercept radar**: GCI put them within 3.2–8 km [2–5 mi] and they needed moonlight or contrails to see the bomber. From Dec 1952 there were reports of **two Soviet night-fighter squadrons**. | [S9 p.634], [S9 p.635] |
| Winter 1952–53 | Manchurian strength | **1,485 aircraft**: 950 jet fighters, 165 conventional fighters, **100 Il-28s**, 65 conventional light bombers, 115 ground-attack and 90 transports. Il-28 radius 1,110 km [690 mi] with 2 t of bombs; S9 records no Il-28 attack; two were flown along the Yalu on 17 Dec 1952 as a show of force. | [S9 p.629] |
| Dec 1952–Jan 1953 | "Box-in" | About 20 min before the Sabres' "Bingo" withdrawal call, MiG flights crossed at Sui-ho and posted at the Chongchon. They then made **head-on passes at the retiring Sabres** while other flights chased from the Yalu. Large formations stayed above 10,670 m [35,000 ft], and flights of **4–8** broke off to attack. Most fights were **above 12,190 m [40,000 ft]**. | [S9 p.630], [S9 p.631] |
| Mar 1953 | Low MiG combat | MiGs carried drop tanks down to Sariwon and Chinnampo and fought as low as **5,180 m [17,000 ft]**, as close as 61 km [38 mi] north of the front. | [S9 p.633] |
| May–Jul 1953 | Late-war MiGs | After 8 May most MiGs carried Chinese or North Korean markings and were flown by inexperienced pilots. Jun 1953: 1,268 sighted, 501 engaged, 77 destroyed. | [S9 p.675], [S9 p.676] |
| Oct 1952–Jul 1953 | Night hecklers return | From Oct 1952 there were Po-2 raids on Cho-do and Seoul. In 1953 they used **Po-2, La-11 and Yak-18**. On 16/17 Jun 1953 **≈15** of them hit Seoul and Inchon. An F-94 slowed to 177 km/h [110 mph] to kill a Po-2. The raids ended in Jul 1953 after the B-29s bombed Pyongyang Main. | [S9 p.684], [S9 p.686], [S9 p.687] |
| **AAA** Nov 1950 | Yalu flak | Flak from the Manchurian side shot down an F-80 over Sinuiju. The B-29s held above 5,490 m [18,000 ft]. | [S9 p.241], [S9 p.245] |
| **AAA** spring 1951 | Flak build-up | Chinese regiments gained **12.7 mm AA companies**. In May 1951, 252 flak guns and 673 automatic weapons were plotted. **Truck-towed 37 mm M1939 guns**, effective to ≈1,370 m [4,500 ft], appeared along the main supply routes. | [S9 p.357] |
| **AAA** Oct–Dec 1951 | Airfield/shoran flak | The new Saamcham, Taechon and Namsi fields had AA guns and automatic weapons. Heavy guns were sited **along the shoran-arc approaches**. Uiju had **radar-controlled flak, >50 searchlights** and fighters. | [S9 p.428], [S9 p.440] |
| **AAA** Feb 1952 | Searchlight belts | Radar-controlled searchlights and flak covered the shoran approaches to the Sinanju/Chongchon bridges. | [S9 p.446] |
| **AAA** Jul 1952 | Pyongyang | **48 guns and >100 automatic weapons**, "one of the worst flak traps". | [S9 p.539] |
| **AAA** winter 1952–53 | Peak flak OOB | ≈**786 AA guns and 1,672 automatic weapons**. The **85 mm M1939** had an effective ceiling of ≈7,620 m [25,000 ft]. The **37 mm M1939** fired ≈160 rpm to ≈1,370 m [4,500 ft]. Guns were massed at Pyongyang, Sinanju, Antung–Sinuiju, Sui-ho and Manpojin. There were ≈500 searchlights, **20–30 per defended area**, with beams reaching 9,140 m [30,000 ft]. | [S9 p.530] |
| **AAA** Feb 1953 | Sui-ho | **141 heavy guns but only 26 automatic weapons** (sited for B-29 attacks). | [S9 p.646] |

## 4. Ground — US / UN

### 4.1 US infantry (FM 7-40 1950 / FM 7-10 1949)

Editions: **S1** = FM 7-40 *Infantry Regiment*, Dept. of the Army, January 1950. **S2** = FM 7-10 *Rifle Company, Infantry Regiment*, 3 October 1949, bound with Change 1 of 16 October 1950 (C1, PDF pp. 2–12). Page numbers are PDF page indices. Neither manual prints unit personnel totals: both refer to the current T/O&E (S2 names T/O&E 7-17), and their organization charts (S1 fig. 1, S2 fig. 1) could not be read in the OCR. Totals marked *derived* are simple sums of the positions each paragraph lists. They are not T/O&E figures.

Conversions: 1 yd = 0.9144 m, rounded to whole metres; 1 mile = 1.609 km. **Paces are left unconverted** because neither source defines the length of a pace (see Gaps).

#### Organization

| Echelon | Composition (as stated) | Key weapons / vehicles | Personnel | Cite |
|---|---|---|---|---|
| Infantry regiment | HQ & HQ company, service company, medical company, tank company, heavy mortar company, 3 infantry battalions | Everything except tanks, the tank recovery vehicle and the utility armored vehicle is air-transportable | — (T/O&E; fig. 1 illegible) | [S1 p.8, par. 4a, 4c]; [S1 p.8] |
| Regimental tank company | Company HQ + 4 tank platoons. HQ has 2 M45 tanks. Each platoon has 2 sections: 1st section = platoon leader's tank + 2 tanks, 2nd section = 2 tanks. Platoons have M26 tanks | M26: one 90-mm gun, two cal .30 MG, one cal .50 MG. M45: one 105-mm howitzer, two cal .30 MG, one cal .50 MG. *Derived:* 20 M26 + 2 M45 = 22 tanks | — | [S2 p.498–499, app. IV sec. IV par. 1, 3] |
| Heavy mortar company | Company HQ (with an operations and fire direction section) + 3 platoons, each with a platoon HQ and 4 mortar squads | *Derived:* 12 mortars. The calibre is not given in this section (see Gaps). Max. effective range HE 4,023 m [4,400 yd], WP 3,895 m [4,260 yd] | — | [S2 p.496, app. IV sec. III par. 1]; [S2 p.497, par. 3] |
| Infantry battalion | Has a heavy weapons company. In defense the front-line battalion normally puts 2 rifle companies on the MLR and 1 in reserve | — | — | [S2 p.488, app. IV par. 1b]; [S2 p.364, par. 294] |
| Heavy weapons company (battalion) | Company HQ, MG platoon (HQ + 2 sections × 2 squads), 81-mm mortar platoon (HQ + 2 sections × 2 squads), 75-mm rifle platoon (HQ + 2 sections, no squads) | 81-mm HE light ≈3,658 m [4,000 yd]. 75-mm rifle HE 6,360 m [6,955 yd], HE-AT 1,554 m [1,700 yd], penetrates 4 in. MG ground observation rarely effective beyond 1,829 m [2,000 yd] | — | [S2 p.490, app. IV sec. II par. 1]; [S2 p.491–492, 495–496] |
| Rifle company | Company HQ (command group + administrative group), 3 rifle platoons, 1 weapons platoon | Crew-served weapons: 57-mm rifle, 60-mm mortar, LMG, cal .50 MG, automatic rifle, rocket launcher. Vehicles: four 1/4-ton trucks, four 1/4-ton trailers, one 2½-ton truck, one 1-ton trailer | — (T/O&E 7-17) | [S2 p.71–72, par. 4]; [S2 p.74, par. 6]; [S2 p.76–77, par. 8] |
| Weapons platoon | Platoon HQ (leader, sergeant, 2 truck drivers, 2 messengers; two 1/4-ton trucks, a cal .50 MG may be mounted on one) + 57-mm rifle section (section leader, messenger, 3 squads) + 60-mm mortar section (section leader, messenger, 3 squads). Each squad = leader, gunner, assistant gunner, 2 ammunition bearers | 3 × 57-mm rifles, 3 × 60-mm mortars (one per squad) | *Derived:* 40 | [S2 p.223, par. 142] |
| Rifle platoon | Platoon HQ (leader, platoon sergeant, assistant platoon sergeant, 2 messengers, plus an aid man attached from the regimental medical company) + 3 rifle squads + 1 weapons squad | 3 BAR, 1 LMG, 1 rocket launcher (from the squad rows below) | *Derived:* 41 + 1 attached aid man | [S2 p.179, par. 110] |
| Weapons squad | Squad leader + LMG team (gunner, assistant gunner, 2 ammo bearers) + rocket launcher team (the same) | 1 LMG, 1 rocket launcher | 9 (numbered 1–9) | [S2 p.163, par. 84]; [S2 p.463, app. II sec. II par. 1 note] |
| Rifle squad | Squad leader (No. 1), 5 riflemen (Nos. 2–6), automatic rifleman (No. 7), assistant automatic rifleman (No. 8), assistant squad leader (No. 9) | 1 automatic rifle (the "AR team" is Nos. 7–8). One scoped rifle per squad, used by the sniper | 9 | [S2 p.110, par. 40]; [S2 p.463, app. II sec. II par. 1]; [S2 p.72, par. 5b] |
| Division support (context) | Division has a tank battalion and division artillery of 3 × 105-mm howitzer battalions + 1 × 155-mm howitzer battalion. One 105-mm battalion normally in direct support of each regiment | — | — | [S2 p.488, app. IV par. 1b]; [S1 p.10, par. 5b] |

The two sources name the regimental tank unit differently. S2 app. IV par. 1b calls it a "heavy tank company" [S2 p.488], while S2 app. IV sec. IV and S1 par. 4a call it the "tank company" / "regimental tank company" [S2 p.498; S1 p.8]. Both descriptions are shown above.

#### Weapon ranges useful for AI/trigger radii

| Weapon | Range (as stated) | Cite |
|---|---|---|
| Rifleman, effective rifle range (defense fire-opening limit) | max 457 m [500 yd] | [S2 p.159, par. 82a]; [S2 p.357, par. 289b] |
| Carbine, effective | 274 m [300 yd] | [S2 p.74, par. 5d] |
| Pistol, effective | < 46 m [50 yd] | [S2 p.74, par. 5e] |
| 57-mm rifle | max 3,968 m [4,340 yd]; seldom used beyond 732 m [800 yd] vs moving or 1,737 m [1,900 yd] vs stationary point targets; HE-AT penetrates ≈3 in | [S2 p.74–75, par. 6a] |
| 60-mm mortar | max 1,815 m [1,985 yd]; rarely engages point targets beyond 914 m [1,000 yd]; concentration covers a 46 m [50 yd] diameter area | [S2 p.75, par. 6b]; [S2 p.5, C1 par. 146b] |
| Heavy mortar platoon (regiment) | concentration 137 m [150 yd] diameter; barrage 91 m [100 yd] deep × 183–366 m [200–400 yd] wide | [S1 p.249, par. 250b] |
| M26 90-mm gun | HVAP / APC 2,195 m [2,400 yd] | [S2 p.499, app. IV sec. IV par. 3c] |
| Cal .50 MG (tank) | 1,829 m [2,000 yd] ground; 914 m [1,000 yd] AA | [S2 p.499, par. 3c(4)] |

#### Formations and intervals

| Unit | Formation | Spacing / geometry | Use | Cite |
|---|---|---|---|---|
| Rifle squad | Drill (any formation) | Normal interval ≈5 paces between men; may be varied | — | [S2 p.463, app. II sec. II par. 2] |
| Rifle squad | March column | Column of files, one on each side of the road | Foot marches behind friendly lines | [S2 p.115, par. 51]; [S2 p.464, app. II par. 4] |
| Rifle squad | Squad column | No. 2 man moves 30 paces forward; even-numbered men go right, odd-numbered men left of the squad leader | Woods, fog, smoke, darkness, trails, defiles | [S2 p.464, app. II par. 5]; [S2 p.124, par. 56a] |
| Rifle squad | Squad diamond | No. 2 man moves 15 paces forward; even-numbered men right, odd-numbered men left | Squad alone or leading the platoon; contact imminent | [S2 p.465, app. II par. 6]; [S2 p.124, par. 56b] |
| Rifle squad | As skirmishers (AR team right / left) | Line abreast (the fig. 73 spacing is illegible) | Assault; dashing across open ground | [S2 p.124, 127, par. 56c]; [S2 p.468, app. II par. 8] |
| Rifle squad | Assault | Squad leader and assistant stay several paces behind the line; each man fires ≥1 shot every 2–3 paces | Assault fire | [S2 p.147, par. 72a] |
| Point (1 rifle squad) | Diamond or column of twos (one file on each side of the road) | Minimum 10 paces between men; the leading 2–3 riflemen about 20 paces; usually within 183 m [200 yd] of the advance party | Advance-guard point | [S2 p.115, par. 52b]; [S2 p.93, par. 25] |
| Rifle platoon | March column | Column of files on each side of the road | Foot marches | [S2 p.181, par. 116] |
| Rifle platoon (advance party) | Column of squads, each squad a column of twos | ≤5 paces between men; point 137–229 m [150–250 yd] ahead in open country, less at night | Advance party | [S2 p.181, 183, par. 117b] |
| Rifle platoon | Platoon column | 23 m [25 yd] between squads (fig. 75 label, see note) | Enemy ahead but not located; woods, night, defiles | [S2 p.473, fig. 75]; [S2 p.184, par. 120a] |
| Rifle platoon | Platoon line | 37 m [40 yd] between rifle squads (fig. 75 label) | Assault; crossing fire-swept ground | [S2 p.473, fig. 75]; [S2 p.184, par. 120c] |
| Rifle platoon | Platoon vee | 37 m [40 yd] (fig. 75 label, twice) | Enemy strength and location known | [S2 p.474, fig. 75]; [S2 p.184, par. 120b] |
| Rifle platoon | Platoon wedge | 37 m [40 yd] (fig. 75 label) | Enemy little known; platoon alone | [S2 p.476, fig. 75]; [S2 p.184–185, par. 120d] |
| Rifle platoon | Platoon echelon R/L | — (OCR reads "140 YARDS", which is ambiguous) | Protect an exposed flank | [S2 p.185, par. 120e]; Gaps |
| Rifle platoon | Base squad | Center rifle squad when three are abreast; otherwise the leading or right-leading rifle squad. Weapons squad normally in march column | — | [S2 p.471–472, app. II sec. III par. 2] |
| Rifle company (main body) | Tactical column, foot | Column of twos, one file on each side of the road | Company in the main body | [S2 p.89, par. 21a] |
| Support proper (rifle co. less 1 platoon) | Two single-file columns, one on each side of the road | ≈2 paces between men | Advance guard | [S2 p.92, par. 23] |
| Advance party (1 reinforced platoon less 1 squad) | Two single-file columns, one on each side of the road | ≈5 paces between men (1949 text; C1 of Oct 1950 changed this sentence, but the new value is illegible) | Advance guard | [S2 p.93, par. 24]; [S2 p.2, C1 par. 24] |

Note on fig. 75: the "25 YARDS" labels are printed beside the platoon-column diagram on a page that also carries the march-column diagram. Assigning 25 yd to the platoon column relies on where the labels sit in the OCR layout. S2 also says these inter-squad distances are drill values and may be changed to suit the drill field [S2 p.471, app. II sec. III par. 2].

#### Defensive frontages and depths

| Echelon | Frontage | Depth / spacing | Cite |
|---|---|---|---|
| Rifle squad (foxholes) | Interval between foxholes 5–18 m [5–20 yd]. Open terrain: single foxholes up to 9 m [10 yd] apart, double foxholes up to 18 m [20 yd]. Close terrain: 5 m [5 yd] single, 9 m [10 yd] double | Double foxhole preferred when conditions allow | [S2 p.156, par. 79b] |
| Rifle platoon | ≤274 m [300 yd] in wooded or broken terrain; 549 m [600 yd] in open terrain; >549 m [600 yd] if flat, open or behind an obstacle. Normally 274–549 m [300–600 yd]. Add ≈23 m [25 yd] occupied frontage per crew-served weapon | 46 m [50 yd] if front-line holes can fire to the flanks and rear; up to 183 m [200 yd] otherwise | [S2 p.214–215, par. 135a–c]; [S2 p.346–347, par. 279] |
| Rifle company | 549–1,097 m [600–1,200 yd]; more than 1,097 m [1,200 yd] in exceptional cases. Two platoons abreast on the MLR, one in support | Depth behind the MLR ≤640 m [700 yd]. Responsibility forward of the MLR seldom exceeds 457 m [500 yd] | [S2 p.343, par. 274–275]; [S2 p.345, par. 278a] |
| Support platoon (company) | — | Within 457 m [500 yd] (rifle supporting distance) of the forward platoons and ≥137 m [150 yd] behind them | [S2 p.348, par. 280b] |
| Reserve rifle company (battalion) | 3 platoon areas on line across the battalion area | Within 457 m [500 yd] of the forward companies' support platoons and ≥137 m [150 yd] from them | [S2 p.366, par. 296] |
| Rifle company, perimeter | Rifle platoons each hold part of the outer perimeter with 3 squads abreast. Two-man foxholes 5–18 m [5–20 yd] apart, 9–23 m [10–25 yd] per crew-served weapon. 60-mm mortars in the inner perimeter; all three 57-mm rifles usually on the outer perimeter | — | [S2 p.374–375, par. 310b–d] |
| Infantry battalion | ≤1,097 m [1,200 yd] in wooded or broken terrain; up to 2,195 m [2,400 yd] in open terrain; >2,195 m [2,400 yd] behind obstacles (widen the intervals between defense areas rather than thin them) | 732–1,280 m [800–1,400 yd] | [S1 p.242, par. 241b–c]; [S1 p.244, par. 243] |
| Infantry regiment | 2,195 m [2,400 yd] in broken, wooded terrain to 4,389 m [4,800 yd] in flat, open terrain; usually 2,743–3,658 m [3,000–4,000 yd] in rolling terrain; >4,389 m [4,800 yd] possible. Two battalions forward, one in reserve | 1,372–2,743 m [1,500–3,000 yd] | [S1 p.240, par. 240, 241a]; [S1 p.242, par. 241a]; [S1 p.244, par. 243] |
| Regiment, wide front | Defense principles still apply, with adjustments, up to 9,144 m [10,000 yd] | Key terrain held strongly, the rest lightly | [S1 p.243, par. 242b–c] |
| Delaying position | Frontages ≈2× those of a sustained defense; much less depth | Successive positions in open terrain force the enemy artillery to displace (5,486 m [6,000 yd]) | [S1 p.321, par. 365a–b]; [S1 p.319–320] |
| Reverse-slope defense | MLR usually ≥183 m [200 yd] behind the crest | — | [S2 p.363] |
| Rear battle position after a withdrawal | ≥5,486 m [6,000 yd] behind, except in close terrain | — | [S1 p.297] |

#### Outposts and security distances

| Element | Distance / strength | Cite |
|---|---|---|
| General outpost | ≈5,486 m [6,000 yd] forward of the battle position (set by the division) | [S2 p.383, par. 319a] |
| Combat outpost | 732–1,829 m [800–2,000 yd] forward of the battle position/MLR. Per battalion front: a rifle platoon up to a reinforced rifle company | [S2 p.384, par. 320]; [S1 p.263, par. 270d] |
| Combat outpost outguards | From a half squad to a reinforced rifle platoon, preferably within sight of each other | [S2 p.385, par. 321a] |
| Delaying-action outpost | 732–1,829 m [800–2,000 yd] forward in open country | [S1 p.319] |
| General outpost riflemen | Open fire at long range, 732–1,097 m [800–1,200 yd]. Automatic weapons normally sited in pairs may be split up | [S1 p.282, par. 298] |
| Local security, front-line company | Sentinels on the nearest observing terrain, generally ≤366 m [400 yd] out. Posts of 2–4 observers. At least 1 sentinel per squad area, doubled at night, relieved every 2 h | [S2 p.389, par. 324b–c] |
| Assembly-area outguards | 183–457 m [200–500 yd] from the assembly area. Each is part or all of a rifle squad. 3-man visiting patrols at intervals of ≤2 h | [S2 p.382–383] |
| Sentinel post / listening post / visiting patrol | 1–2 men / 2–4 men / 2–3 men | [S2 p.161–162, par. 83a, b, d] |
| Squad as motorized flank guard | Keeps the enemy from firing on the column within midrange, 366–549 m [400–600 yd] | [S2 p.121, par. 54a] |
| Motorized flank guards from the support proper | Protection out to 457 m [500 yd] | [S2 p.95, par. 26b(2)] |

#### Attack frontages and distances

| Item | Value | Cite |
|---|---|---|
| Interior rifle company zone of action | 183–457 m [200–500 yd] wide | [S2 p.261, par. 178a] |
| Assault rifle platoon frontage | 91–229 m [100–250 yd] | [S2 p.263, par. 178b]; [S2 p.188, par. 123] |
| Line of departure to squad objective | 274/366–914 m [300 or 400 to 1,000 yd] | [S2 p.133, par. 64a(2)] |
| Assault position to objective | 91–137 m [100–150 yd] | [S2 p.134, par. 64a(3)]; [S2 p.201, par. 128a]; [S2 p.275, par. 189a] |
| Night attack, probable line of deployment | 91–183 m [100–200 yd] from the objective | [S2 p.297, par. 216] |
| Tanks leading infantry (method 2) | Tanks 46–274 m [50–300 yd] ahead of the infantry | [S2 p.265, par. 181d(2)] |
| Assault company CP | Seldom more than 274 m [300 yd] behind the front line | [S2 p.274] |
| Regimental main vs secondary attack | Secondary attack gets a relatively wider frontage and the minimum force | [S1 p.139] |

#### March columns

| Item | Value | Cite |
|---|---|---|
| Advance guard, daylight: point (squad) | Road space 46 m [50 yd]; 183 m [200 yd] to the next element | [S2 p.92, par. 22d] |
| Advance party (platoon less 1 squad) | Road space 69 m [75 yd]; 366 m [400 yd] to the next element | [S2 p.92, par. 22d] |
| Support proper (rifle company less 1 platoon) | Road space 137 m [150 yd]; 457 m [500 yd] to the reserve | [S2 p.92, par. 22d]; [S2 p.91, fig. 4] |
| Security/recon elements of higher commanders | 4.8–8.0 km [3–5 miles] ahead of the point (fig. 4 label) | [S2 p.91, fig. 4] |
| Night / open country | Distances shorter at night or in poor visibility; greater in open country | [S2 p.92, par. 22d] |
| Motorized advance guard | Support follows the recon elements by ≥4.8 km [3 miles or more]. Advance party (reinforced platoon) ≈5 min ahead of the support proper; no point | [S2 p.98–99, par. 31a–b] |
| Motor column formations | Close column, open column, infiltration, or combinations (details in FM 25-10, not in these sources) | [S1 p.70, par. 77c] |
| Serial / march unit | The battalion is a convenient march serial. Serials and march units are separated by time intervals set in the march order | [S1 p.70, par. 77a]; [S1 p.73, par. 81] |
| Foot march halts | 15-min halt after the first 45 min; then 50 min march / 10 min rest | [S1 p.76, par. 87e] |
| Forced march threshold | More than 32 km/day [20 miles] at 4.0 km/h [2½ mph] on roads | [S1 p.77, par. 87g] |
| Night march | Foot rate on good roads ≈ day rate; vehicle speed greatly reduced; compact column with shorter distances | [S2 p.102, par. 35b–c]; [S1 p.77, par. 88c] |
| Air threat | Distances between individuals, vehicles and column elements are increased | [S1 p.96]; [S1 p.84, par. 98b] |
| Company vehicles on a foot march | Separate serials under regiment or battalion control, moving by bounds behind the foot column | [S2 p.89, par. 21b]; [S1 p.104, par. 113b, d] |
| Motor march, main body | Vehicles move with the company; cal .50 MGs mounted for AA; one air guard per vehicle | [S2 p.98, par. 30] |
| Tank company on the march | Route column: marches separately. Tactical column / approach march: 1+ platoons attached to the advance guard battalion | [S1 p.107, par. 117a]; [S1 p.96] |
| Heavy mortar company on the march | Marches as a unit; one platoon attached to the advance guard battalion in the approach march | [S1 p.106, par. 116a] |
| Assembly area | If possible within ≈1 hour's march of the attack position | [S2 p.105, par. 37a] |

### 4.2 US armor (FM 17-33, 1949)

Source S3 = FM 17-33 *Tank Battalion*, 22 Sep 1949 (with C1, 14 May 1951). Tank-company and tank-platoon organization is **not** in this manual: it refers readers to FM 17-32 and FM 7-35 (see Gaps). No tank model designations (M4, M26, M46 etc.) appear in the text.

**Organization**

| Item | Value | Source |
|---|---|---|
| Medium tank bn (armored div) | HQ, HQ & service co; 4 medium tank companies; medical detachment | [S3 p.24, par. 10] |
| Tank bn (infantry & airborne div) | HQ, HQ & service co; 3 tank companies; medical det.; equipped with medium tanks | [S3 p.307, par. 270] |
| Heavy tank bn (armored div) | Organized like the infantry-division tank bn (3 companies) | [S3 p.281, par. 236]; [S3 p.307, par. 270] |
| Infantry division tank assets | 1 organic tank bn per division + 1 organic tank company per infantry regiment | [S3 p.20, par. 5]; [S3 p.21, par. 6] |
| Airborne division | 2 tank bns (one more than the infantry div); no regimental tank companies | [S3 p.305, par. 267] |
| Assault gun platoon (medium tank bn) | Platoon HQ + 6 tanks with 105-mm howitzers + ammunition section | [S3 p.28, par. 16] |
| Bn reconnaissance platoon | Platoon HQ, scout section, tank section, rifle squad, support squad | [S3 p.27, par. 15] |
| Supply trucks with combat trains | Normally 1 fuel/lubricant truck + 1 ammunition truck per company | [S3 p.29, par. 17] |
| Example combat trains (sample order) | 6 gasoline trucks + 6 ammunition trucks | [S3 p.488, app. V] |
| Heavy tank role | Best antitank weapon when enemy armour outclasses the medium tank | [S3 p.20, par. 4] |

**Tank–infantry attachment**

| Item | Value | Source |
|---|---|---|
| Reinforced tank bn, examples | Bn minus 1–2 companies + 1–2 armored infantry companies + 1 engineer platoon; or whole tank bn + armored infantry bn + engineer company | [S3 p.30–31, par. 22] |
| Infantry-division context | Manual flags the "small ratio of tanks to infantry" | [S3 p.305, par. 265] |
| Delaying action (fig. 49) | Infantry company with a tank platoon attached holds the first delaying position | [S3 p.275, fig. 49] |
| Pursuit, infantry carried on tanks | Normally 1 infantry platoon rides on the tanks of 1 tank platoon | [S3 p.335, par. 300] |
| Artillery forward observers (defense) | 1 per company | [S3 p.249, par. 200] |

**March**

| Item | Value | Source |
|---|---|---|
| Rate of march, day | 19–32 km/h [12–20 mph] | [S3 p.100, par. 93] |
| Rate of march, night | 13–16 km/h [8–10 mph] | [S3 p.100, par. 93] |
| Sustained 24 km/h [15 mph] or more | Only under optimum conditions | [S3 p.102, par. 93] |
| Sample exploitation order | Main body not to exceed 24 km/h [15 mph] | [S3 p.488, app. V] |
| Vehicle distance, day | 46 m [50 yd] | [S3 p.102, par. 94] |
| Vehicle distance, night | Maximum at which the driver can still see the blackout taillight ahead | [S3 p.103, par. 94]; [S3 p.112, par. 101] |
| Time gap between march units | 1 min | [S3 p.103, par. 94] |
| Time gap between serials | 3 min | [S3 p.103, par. 94] |
| Open column density | 10, 15 or 20 vehicles per mile (≈6.2–12.4 per km, i.e. ≈80–161 m average gap) | [S3 p.90, par. 84] |
| Infiltration density | 3, 4, 5 or 6 vehicles per mile (≈1.9–3.7 per km, i.e. ≈268–536 m average gap) | [S3 p.89, par. 84] |
| Close column | Vehicles closed up to safe driving distance | [S3 p.89, par. 84] |
| Column make-up | Bn marches as 1 serial with each company as a march unit; bn HQ near the centre, service elements at the rear | [S3 p.97, par. 91] |
| Halts | 10 min each hour, or 15 min every 2 hours; units do not close up at halts | [S3 p.103, par. 96]; [S3 p.104, par. 96] |
| Halts in contact / long halts | Units may "coil up" on each side of the road | [S3 p.104, par. 96] |
| Refuelling interval | ≤ 121 km [75 miles] | [S3 p.104, par. 96] |
| Air defence on the march | Air sentry on each vehicle; AA guns continuously manned | [S3 p.107, par. 97] |
| Bivouac/assembly dispersion (ideal) | Vehicles ≥ 46 m [50 yd] apart | [S3 p.114, fig. 11] |
| Assembly-area layout | Tank companies encircle and protect HQ & service company | [S3 p.123, par. 113] |

**Attack**

| Item | Value | Source |
|---|---|---|
| Minimum frontage | At least a two-company front if possible | [S3 p.144, par. 128] |
| Penetration | Narrow frontage, great depth | [S3 p.144, par. 128] |
| Battalion formations | Column (maximum control and driving power); echelon right/left (flank bn); line (2+ companies abreast, little depth) | [S3 p.174, par. 146]; [S3 p.175, par. 146] |
| Tanks lead when | Enemy strong in armour; terrain favours tanks; exploitation possible | [S3 p.175, par. 147] |
| Tank–infantry advance, 3 options | Tanks leading infantry; infantry leading, supported by tanks; infantry accompanying tanks | [S3 p.329, par. 295] |
| Assault | Tanks normally precede the infantry onto the objective | [S3 p.333, par. 297] |
| On the objective | Tanks take hull-defilade positions to cover the consolidating infantry | [S3 p.334, par. 298] |
| Jungle attack | Two or more waves in staggered line; distance between tanks reduced | [S3 p.441, par. 408]; [S3 p.442, par. 408] |
| Tank-to-tank intervals within platoon/company formations | — | Gaps |

**Defense**

| Item | Value | Source |
|---|---|---|
| Combat outpost distance | 732–1,829 m [800–2,000 yd] in front of the MLR | [S3 p.246, par. 197] |
| Combat outpost strength | One or more reinforced tank platoons | [S3 p.246, par. 197] |
| Outpost outguards | Can be as small as a reinforced tank section | [S3 p.247, par. 197] |
| Mobile-defense strong points | 1–2 tanks with a few infantry, up to reinforced-company strength | [S3 p.233, par. 184] |
| Mobile-defense sector width | Much wider than a sustained-defense sector | [S3 p.232, par. 183] |
| Reinforced company sector (schematic) | Labelled 686–1,372 m [750–1,500 yd] | [S3 p.250, fig. 44] |
| Battalion reserve | Mainly tanks, used for counterattack | [S3 p.249, par. 200] |
| Tanks with front-line infantry bn | Some fire in front of the MLR from within or near platoon defense areas; the rest in depth | [S3 p.340, par. 308] |
| Delaying action | Open fire at maximum range | [S3 p.274, par. 229] |

### 4.3 US field artillery (S4 = FM 6-120, 1951: observation units only)

**Source mismatch:** the S4 text is **FM 6-120, *The Field Artillery Observation Battalion and Batteries*, July 1951**, not FM 6-140. It covers sound, flash and radar target-location units and defers firing-battery matters to FM 6-140 [S4 p.3]; [S4 p.8, par. 2]. The only firing-battery data are the few ballistic figures below.

| Item | Value | Source |
|---|---|---|
| Observation bn | HQ & HQ battery + 3 observation batteries; motorized; normally 1 per corps | [S4 p.10, par. 11] |
| Observation battery | Battery HQ, flash ranging plt, sound ranging plt, radar plt, communication plt, maintenance section | [S4 p.21, par. 15] |
| Flash ranging platoon | Mans 4 observation posts | [S4 p.23, par. 15] |
| Radar platoon | 2 sections, each operating 1 radar set | [S4 p.24, par. 15] |
| Sound base | 4–6 microphones, normally 700–2,000+ m apart; full installation 5–6, minimum 4 | [S4 p.154, par. 96]; [S4 p.156, par. 97] |
| Sound outpost observers | 1–2 observers ≥ 1,000 m in front of the sound base | [S4 p.155, par. 96] |
| Sound ranging range and accuracy | 46–91 m [50–100 yd] at up to 13,716 m [15,000 yd]; locates artillery at 3,200–18,288 m [3,500–20,000 yd] | [S4 p.33, par. 18]; [S4 p.35, par. 18] |
| Normal location accuracy | Flash 23–46 m [25–50 yd]; radar 46–91 m [50–100 yd]; sound 46–91 m [50–100 yd] | [S4 p.71, par. 57] |
| Siting | Installations not so far forward that enemy action or mortar fire often interrupts them | [S4 p.33, par. 17] |
| Bn CP radio/panel station | Several hundred yards from other CP elements, preferably on a flank | [S4 p.60, par. 43] |
| Time of flight at 5,944 m [6,500 yd] | 155-mm how M1, ch. 5: 20.0 s; 105-mm how M2, ch. 6: 20.6 s; 155-mm how M1917, ch. 6: 18.8 s | [S4 p.262, par. 121] |
| Battery composition (howitzers, prime movers, personnel) | — | Gaps |
| Piece spacing, battery front/depth, distance to FDC/OP | — | Gaps |
| Displacement and march intervals | — | Gaps |
| Maximum ranges of 105/155 howitzers | — | Gaps |

### 4.4 US AAA automatic weapons (FM 44-2, 1950)

Source S5 = FM 44-2, 24 Aug 1950, plus Changes No. 1 of 26 Nov 1952 (bound at the end of the scan) [S5 p.382].

**Organization and equipment**

| Item | Value | Source |
|---|---|---|
| AW battery | 2 platoons × 4 firing sections; each section = MG squad + AW squad, each squad a fire unit (so 16 fire units per battery, which matches the 6/6/4 split below) | [S5 p.9, par. 5]; [S5 p.64, par. 35] |
| Mobile (towed) firing section | 40-mm gun on carriage M2A1 + director M5A3 + power plant; multiple cal .50 trailer mount M55 | [S5 p.10, par. 5] |
| Self-propelled firing section | 1 M16 multiple MG motor carriage + 1 M19 twin 40-mm gun motor carriage (so 8 M16 + 8 M19 per SP battery; derived) | [S5 p.10, par. 5] |
| Airborne firing section | 40-mm gun on M2A1 + M55 trailer mount | [S5 p.10, par. 5] |
| Mobile AW bn | HQ & HQ battery + 4 mobile firing batteries (+ medical det.) | [S5 p.10, par. 6] |
| SP AW bn | HQ & HQ battery + 4 SP firing batteries; the divisional SP bn has no medical det. | [S5 p.10, par. 6]; [S5 p.12, par. 6] |
| Airborne AW bn | HQ & HQ battery + 3 firing batteries; plus 52 cal .50 MGs on individual mounts for the assault echelon | [S5 p.12, par. 6]; [S5 p.120, par. 105] |
| Divisional assignment | 1 organic SP AW bn in each infantry and armored division | [S5 p.61, par. 30]; [S5 p.69, par. 41] |
| M16 | M45 quad cal .50 mount on a half-track; low-angle fire to the front blocked by the cab | [S5 p.23, par. 14] |
| M19 | Two 40-mm guns mounted coaxially on a full-track vehicle | [S5 p.23, par. 14] |
| M55 | M45 turret on a trailer; carried in a 2½-ton truck; emplaces in 1–2 min | [S5 p.23, par. 13] |
| 40-mm M2A1 emplacement | 1–2 min with direct-fire sights; 5–30 min for director control | [S5 p.20, par. 13] |
| Change 1 (1952) replacements | Twin 40-mm T141 replaces M19A1; M16A1 replaces M16 | [S5 p.383, par. 253] |

**Weapon performance**

| Item | Cal .50 (quad) | 40-mm | Source |
|---|---|---|---|
| Rate of fire | 400–600 rds/min per gun | 120 rds/min (short bursts) | [S5 p.19, par. 12]; [S5 p.20, par. 12] |
| Planning range for fields of fire (horizontal) | 549 m [600 yd] | 1,372 m [1,500 yd] | [S5 p.20, par. 12]; [S5 p.19, par. 12] |
| Effective hitting range | 732 m [800 yd] | 1,097 m [1,200 yd] on-carriage; 1,829 m [2,000 yd] director | [S5 p.18, par. 11] |
| Maximum hitting range | 914 m [1,000 yd] | 1,646 m [1,800 yd] on-carriage; ≈2,286 m [2,500 yd] director | [S5 p.17, par. 11] |
| Extreme deterrent (tracer burn-out) range | 1,646 m [1,800 yd] or 2,240 m [2,450 yd] (par. 11); par. 10 says 1,692 m [1,850 yd] or 2,240 m [2,450 yd] | 3,200 m [3,500 yd] or 5,029 m [5,500 yd] (40-mm HE self-destructs at burn-out) | [S5 p.17, par. 11]; [S5 p.15, par. 10] |
| Minimum range | Virtually none (power turret) | 366 m [400 yd] slant with director; M19 ≈274 m [300 yd] | [S5 p.19, par. 11] |
| Director dead area | — | 70° horizontal (bisected by the gun–director line); vertical normally 30° | [S5 p.53, par. 20]; [S5 p.41, par. 15] |

**Siting and defense patterns**

| Item | Value | Source |
|---|---|---|
| Spacing between adjacent fire units | About 274–366 m [300–400 yd]; never more than 686 m [750 yd]; at least 137 m [150 yd] | [S5 p.53, par. 19] |
| Placement | As close to the objective's edge as practicable, then extended outward along likely approach routes | [S5 p.53, par. 19] |
| "Single objective" | ≤ 457 m [500 yd] across (e.g. bridge, FA battery in position) | [S5 p.47, par. 17] |
| "Vital area" | > 457 m [500 yd] across, or a group of objectives ≤ 1,372 m [1,500 yd] apart (e.g. airfield with dispersal areas, marshalling yard, artillery area) | [S5 p.47, par. 17]; [S5 p.49, par. 17] |
| Anti-dive-bombing | Weapons on or close to the objective; some inside if the area is > 549–732 m [600–800 yd] wide | [S5 p.51, par. 18] |
| Anti-minimum-altitude attack | 40-mm near the objective or on approach routes (to get incoming courses); quad .50s for close-in crossing targets, some near the objective | [S5 p.51, par. 18]; [S5 p.52, par. 18] |
| Anti-low-level horizontal attack | Weapons far enough out to fire before bomb release (40-mm preferred) | [S5 p.52, par. 18] |
| Dead-area coordination | Each 40-mm dead area covered by an adjacent unit; dead area faces a mask or lies tangent to the objective | [S5 p.53, par. 20]; [S5 p.55, par. 20] |
| Director and power plant offset | Director 4–5 m [13–15 ft] from the gun; power cable allows ≈69 m [225 ft] | [S5 p.149, par. 130] |
| Defense of FA | 1 AW battery per FA bn; if the FA moves by battery, provisional platoons of 6, 6 and 4 fire units | [S5 p.64, par. 35]; [S5 p.72, par. 46] |
| AW vs FA pieces | ≥ 91 m [100 yd] from the nearest FA piece; flanks preferred (attacks likely along the line of guns) | [S5 p.64, par. 35] |
| River crossing | AW defense centred on bridge sites; AW among first units at crossings; part crosses | [S5 p.67, par. 37] |
| Local-security wire | 27–91 m [30–100 yd] around the position | [S5 p.135, par. 119] |

**March-column protection**

| Item | Value | Source |
|---|---|---|
| Method | SP weapons in the line of march is the best column defense | [S5 p.60, par. 26] |
| Distribution | SP weapons spread through the column within mutually supporting distance; heaviest where the enemy concentrates attacks | [S5 p.63, par. 34] |
| Halted column | Every weapon placed to fire, in column or moved to the flank | [S5 p.72, par. 45] |
| Armored column priorities | Supply trains most vulnerable; armored FA has 1 cal .50 per armored weapon; each armored infantry carrier has 1 cal .50 | [S5 p.71, par. 45] |
| Rail movement | AW mounted on flat cars/gondolas throughout the train | [S5 p.63–64, par. 34] |
| Night moves | Preferred; AW in position before daylight | [S5 p.59, par. 22] |

**Surface (ground-support) mission**

| Item | Value | Source |
|---|---|---|
| Allocation | 1 battery per attacking infantry regiment; 1 platoon per front-line infantry bn | [S5 p.76, par. 51] |
| Siting vs targets | Cal .50 within 914 m [1,000 yd]; 40-mm within 1,372 m [1,500 yd] of target, if possible | [S5 p.84, par. 61] |
| Fires supporting outposts | 914–2,286 m [1,000–2,500 yd] | [S5 p.88, par. 65] |
| Indirect fire (Change 1, 1952) | By platoon (4 weapons, 8 barrels); guns ≈18 m [20 yd] apart for a 55 m [60 yd] sheaf | [S5 p.384, pars. 254–255] |

**Expected enemy air attack (planning basis)**

| Item | Value | Source |
|---|---|---|
| Dive bombing | Dive 50°–80°; pull-out 305–1,524 m [1,000–5,000 ft] | [S5 p.43, par. 17] |
| Minimum-altitude attack | 5–61 m [15–200 ft] | [S5 p.44, par. 17] |
| Low-level approach and sighting time | 2–10 s | [S5 p.50, par. 17] |

## 5. Ground — Chinese (CCF) and North Korean (KPA)

### 5.1 Chinese Communist Forces (DA Pam 30-51, 1960 edition, with Korea-period notes)

> **Edition warning (read first).** The file supplied as S6 is **not** the September 1952 pamphlet. It is the **7 December 1960 edition**, which says it "supersedes DA Pam 30-51, 30 September 1952" [S6 p.4]. Its tables of organization show the Chinese Communist Army (CCA) **after the June 1955 reorganization**, with Soviet equipment of the late 1950s such as the T-54, MiG-17 and MiG-19 [S6 p.39, para 44; p.87; p.122]. The handbook itself describes those tables as the full wartime TOE, "not those of actual units at present" [S6 p.40, fig. 22]. The only rows that describe the Korean War are the ones tagged **Korea 1950–53** (the handbook's own look back at the war), **S9 (Futrell, Korea)** or **S10 (USAF, Korea)**. Treat every row tagged **CCA 1955 TOE (1960 hbk)** as post-war, and use it at most as an upper bound for 1952–53 units.
>
> The period tags used below are: `Korea 1950–53` for statements about the war itself; `CCA 1955 TOE (1960 hbk)` for post-reorganization organization, weapons and doctrine; and `S9 (Futrell, Korea)` or `S10 (USAF, Korea)` for enemy behaviour observed from the air, with the month or year added. Most S9 figures are USAF claims or prisoner-of-war (POW) statements, and the rows say so.

#### A. Korean War period: what the sources say directly

| Topic | Fact (paraphrased) | Period | Cite |
|---|---|---|---|
| Structure entering the war | The 1949 reorganization set up a Field Army → Army Group → Army → Division → Regiment chain. Armies were triangular: 3 divisions of 3 regiments each. Units still varied in strength and organization. | Korea 1950–53 (1949 baseline) | [S6 p.13, para 7m] |
| Quality in 1950 | Still essentially a mass of infantry with rifles and small quantities of automatic weapons. Artillery support was inadequate and the logistic service was haphazard. | Korea 1950–53 | [S6 p.13, para 9a] |
| Source of troops | The first troops committed came from the Fourth Field Army. By the summer 1953 truce, about one-third of the Chinese Communist Army was in Korea. | Korea 1950–53 | [S6 p.14, para 9b] |
| Human-sea tactics and endurance | "Human sea" tactics worked at first. Because of logistic weakness, offensives could be sustained for **less than 10 consecutive days**, after which troops had to regroup and wait for resupply. | Korea 1950–53 | [S6 p.14, para 9c] |
| Wartime reorganization | A programme began in late 1950 and peaked in late 1952 and early 1953. It added service and support units and brought "a substantial increase in the number of artillery units and a change to larger caliber artillery". | Korea 1950–53 | [S6 p.14–15, para 10a, c] |
| Equipment priority | During the war most Soviet-supplied equipment was issued to the forces in Korea. | Korea 1950–53 | [S6 p.15, para 10d] |
| Reorganization dates | Reorganizations took place in early 1951 (to fit the armies for Korea), in 1953 (at the cease-fire) and in June 1955 (the formal one). | Korea 1950–53 | [S6 p.39, para 44] |
| Captured kit | Non-standard items captured from UN forces in Korea were still in use in 1960. | Korea 1950–53 → 1960 | [S6 p.39, para 45a] |
| Rocket artillery | Rocket launcher divisions were "observed in Korea". | Korea 1950–53 | [S6 p.43, para 48b] |
| March performance | In Korea, large units marched 200–300 mi (322–483 km) but averaged only 20 mi (32 km) a day. | Korea 1950–53 | [S6 p.68, para 86] |
| Smoke | During the Korean War the CCF showed an excellent smoke-screening capability. | Korea 1950–53 | [S6 p.99] |
| Mines | Until the Korean War there was no standard mine-warfare equipment, so the CCF relied on improvised and captured mines. | Korea 1950–53 | [S6 p.92, para 112a] |
| Air force | The Air Force expanded rapidly in 1950–51. Its poor showing against UN aircraft is put down to inadequate training, not to a shortage or inferiority of equipment. | Korea 1950–53 | [S6 p.121, app. II para 1d] |
| Naval air | The naval air arm was formed near the end of the Korean War. | Korea 1950–53 | [S6 p.120, app. I para 8] |
| Night-only movement | In December 1950, after CCF troops travelled by day north of Pyongyang, Fifth AF pilots killed or wounded an estimated 33,000. By mid-December, Communist forces moved only at night. | S10 (USAF, Korea), Dec 1950 | [S10 p.24] |
| Yalu crossings | When bridges were cut, the Communists built pontoon bridges or crossed the Yalu on the ice. Supplies were dispersed or held in Manchuria. | S10 (USAF, Korea), Nov 1950 | [S10 p.23] |
| Yalu flak | B-29s bombed the Yalu bridges from at least 20,000 ft (6,096 m) to avoid flak. | S10 (USAF, Korea), Nov 1950 | [S10 p.23] |
| Rail repair | A fighter-bomber cut of the Chongju–Sinanju railway closed the line for only five days. Rail interdiction forced the CCF and KPA to divert troops and materiel to protect and repair the railways. | S10 (USAF, Korea), Jan–Mar 1952 | [S10 p.39] |
| Static-front fieldworks | Under alternating air and artillery strikes in January 1952, CCF and KPA troops dug deeper trenches and tunnels that were generally invulnerable to both. | S10 (USAF, Korea), Jan 1952 | [S10 p.40] |
| CCF army size (1950) | A Chinese army (corps) normally had headquarters troops and 3 divisions of 8,000 men each, about 30,000 at full strength. | S9 (Futrell, Korea), Oct 1950 | [S9 p.250] |
| Yalu crossing and approach | According to captured CCF records, troops began crossing the Yalu as early as 14 October 1950. By 26 October the 38th, 39th, 40th and 42d Armies had crossed and were marching mostly at night. | S9 (Futrell, Korea), Oct 1950 | [S9 p.250] |
| Camouflage skill | Fourth Field Army troops were "masters of camouflage". Heavy photo reconnaissance (27,643 photos) found little. Chinese troops had special camouflage training before Korea, but their tracks in snow sometimes gave them away. | S9 (Futrell, Korea), Dec 1950–Jan 1951 | [S9 p.295; p.353] |
| Day to night switch | A POW from the 112th Division said it moved by day until air attacks destroyed most of its trucks, then only by night. From mid-December, troops moved mostly at night, or by day only under rigid camouflage discipline. | S9 (Futrell, Korea), Dec 1950 | [S9 p.284] |
| Dawn hide | Each day at dawn, mobile equipment was hidden in ravines, under bridges and in other concealed spots along the main supply routes (MSRs). | S9 (Futrell, Korea), Dec 1950 | [S9 p.285] |
| Night traffic volume | Before dawn all moving transport was halted and under cover, and daytime movement was unusual. At night, 3–5 trains and as many as 2,000 vehicles moving were "not unusual". Trains ran from tunnel to tunnel over very short open stretches. | S9 (Futrell, Korea), early 1951 | [S9 p.348] |
| Headlights and road guards | Drivers were supposed to drive blacked out but mostly used headlights. Guards along the MSRs warned of approaching aircraft, so lights rarely stayed on more than 10–15 s after an intruder appeared. | S9 (Futrell, Korea), 1951 | [S9 p.351] |
| Aircraft-warning sentries | Logistical commands posted aircraft-warning sentries every 300–400 m along the MSRs. They warned night-running trucks of road obstructions and approaching aircraft. | S9 (Futrell, Korea), 1951 | [S9 p.360] |
| Vehicle shelters and truck cycle | By late March 1951, vehicles were no longer only hidden in villages, in woods or as fake straw stacks; they went into tunnels or log-reinforced bunkers in ravines. Trucking ran on a 3-night cycle: flak-protected area in the north → bunker zone → front and back to the bunkers → flak areas. | S9 (Futrell, Korea), Mar 1951 | [S9 p.356] |
| Roadside caves | The Chinese built caves, revetments and trenches to protect vehicles from daytime fighters. One intelligence report said 400,000 laborers were mobilized to build trenches and caves along the highway from Sinuiju to the front. | S9 (Futrell, Korea), mid-1951 | [S9 p.358] |
| Logistics chain | Rear logistical commands supplied the rear-service departments of front-line units (a "delivery-forward" system). The Fourth Field Army Logistical Command was at Antung, and the Third Field Army's reportedly at Chian. In early 1951, seven "Branch Units" (logistical commands) held areas in Korea. | S9 (Futrell, Korea), early 1951 | [S9 p.359] |
| Logistical command transport | Each command had 4 motor-transport regiments of 120 GAZ-51 trucks each, a porter battalion, and an aircraft-spotter unit of 1,200. Main depots had 2 motor companies of 65 trucks each. Subdepots had a truck company and "numerous porter teams". Rail was used where possible, with trucks in support. | S9 (Futrell, Korea), early 1951 | [S9 p.359] |
| Truck totals | Each Chinese army had organic truck companies. At least 7 more truck regiments were operating in spring 1951. By July 1951, FEC estimated the CCF and KPA were operating at least 16,624 vehicles in Korea. | S9 (Futrell, Korea), spring–Jul 1951 | [S9 p.359] |
| Non-motor transport | Oxcarts, wagons, pack animals (including camels) and human porters made up for the shortage of equipment. At the front, carts, pack animals and porters could move a lot of tonnage but were too slow for mobile war. | S9 (Futrell, Korea), 1951 | [S9 p.359; p.362] |
| Supply depth | Division supply points held 7–10 days' supplies, which offensives used up quickly. The dispersed, dug-in supply points and subdepots were "virtually immobile". | S9 (Futrell, Korea), Apr–May 1951 | [S9 p.362] |
| Dump dispersal | Small supply dumps were spread across the terrain in caves, tunnels, revetments, ditches, holes, ravines and houses. Supplies were also stored in buildings or villages that had already been bombed. | S9 (Futrell, Korea), Mar 1951 | [S9 p.361] |
| Logistics scale | About 500,000 soldiers and civilians were organized into a logistical system supporting some 70 combat divisions "echeloned in depth". | S9 (Futrell, Korea), Feb–Apr 1951 | [S9 p.361] |
| Replacements on the march | Replacements formed regiment-sized units in Manchuria and marched south, always at night; reinforcement divisions also walked. Of POWs questioned, 70% of Chinese and 81% of North Koreans said they were never attacked while marching at night. 67% of Chinese and 62% of North Koreans had their daytime camp sites attacked at least once. | S9 (Futrell, Korea), 1951 | [S9 p.360–361] |
| Rail workarounds | A train might run over as little as 11 mi (18 km) of clear track, then unload onto another train waiting beyond the break. | S9 (Futrell, Korea), 1951 | [S9 p.360] |
| Rail cut repair | Photo interpreters judged that coolie laborers starting at dusk could repair a rail cut within 8 hours, opening the line between midnight and sunrise. | S9 (Futrell, Korea), Dec 1951 | [S9 p.469] |
| A-frame / truck resupply | Replacement rails were judged too heavy to carry by coolies with "A-frames", or usually even by trucks. Weyland said the 4–5% of pre-war rail traffic that survived was enough base to add "truck and A-frame transportation" for a static supply line. | S9 (Futrell, Korea), 1951–52 | [S9 p.461; p.495] |
| Winter convoy behaviour | From November 1951, night intruders saw fewer vehicles moving with lights. Convoys became "smaller and better dispersed". | S9 (Futrell, Korea), Nov–Dec 1951 | [S9 p.481] |
| Regimental AA | By April 1951, Chinese regiments had automatic-weapons air-defense companies with Soviet 12.7-mm MGs, most effective at low altitude. | S9 (Futrell, Korea), Apr 1951 | [S9 p.357] |
| AA build-up | In May 1951, FEAF plotted 252 flak guns and 673 automatic weapons. Most guns were in fixed defenses, but truck-towed 37-mm M1939s, effective to 1,372 m [4,500 ft], appeared along the MSRs. By 1 July 1951 there were 275 AAA guns and 600 automatic weapons emplaced. From April onward, regularly organized AAA regiments were added. | S9 (Futrell, Korea), May–Jul 1951 | [S9 p.357; p.360] |
| Hunter Groups | From 4 January 1951, the Fourth Field Army set up volunteer "Hunter Groups" armed with infantry weapons, including heavy MGs. They often damaged low-flying aircraft; three kills in 90 days earned "hero" rewards. | S9 (Futrell, Korea), Jan 1951 | [S9 p.360] |
| Flak traps | Decoys used as flak traps: open parachutes hung in trees, straw dummy troops, cables strung across valleys, and strings of lights at measured intervals on mountainsides that looked like a convoy at night. | S9 (Futrell, Korea), 1951 | [S9 p.360] |
| UN response to flak | Losses to ground fire were 59 aircraft in April–May 1951 and 22 in June. Mustang armed-reconnaissance flights changed to a leader on the deck, an element leader at 1,219 m [4,000 ft] watching for flak, and wingmen covering. | S9 (Futrell, Korea), Apr–Jun 1951 | [S9 p.358] |

#### B. Formations and strengths (full wartime TOE, rounded; post-1955)

| Unit | Strength | Composition / equipment as printed | Period | Cite |
|---|---|---|---|---|
| Army (corps-equivalent) | 56,400 | HQ 1,900 and 3 infantry divisions, plus reconnaissance, guard, engineer and signal battalions (strengths in Gaps). Artillery is normally allocated to the army for passing down. There may be a smoke and flamethrower battalion. | CCA 1955 TOE (1960 hbk) | [S6 p.40, para 46, fig. 22] |
| Infantry division | 17,600 | 3 infantry regiments, a tank-assault gun regiment, an artillery regiment, an antitank battalion, an AA artillery battalion (450), an engineer battalion (500), a signal battalion, a chemical warfare company and a reconnaissance company. | CCA 1955 TOE (1960 hbk) | [S6 p.40, para 46; p.41, fig. 23] |
| Tank-assault gun regiment in the infantry division | — | Not all divisions have one yet: they "may" get it "as sufficient armor becomes available". | CCA 1955 TOE (1960 hbk) | [S6 p.53, para 62] |
| Infantry regiment | 4,150 (OCR "4, ISO") | HQ 500. Also a heavy weapons battalion, engineer company, AAMG battery, signal company (160), chemical warfare platoon (40), and guard and reconnaissance platoons. | CCA 1955 TOE (1960 hbk) | [S6 p.41, fig. 23] |
| Infantry battalion | 840 | HQ 80. Company strengths printed in fig. 23 are: rifle company 200 (OCR "ZOO"), mortar company 190, heavy MG company 170. The number of each per battalion cannot be recovered. | CCA 1955 TOE (1960 hbk) | [S6 p.41, fig. 23] |
| Armored division | 7,800 | An armored regiment, an infantry regiment, an artillery regiment, an AA artillery battalion, and signal, engineer, guard and reconnaissance companies. | CCA 1955 TOE (1960 hbk) | [S6 p.42, fig. 24] |
| Armored regiment: companies | — | Medium tank company: 10 T-34 each. Heavy tank company: 5 JS-2. Assault-gun company: 4 assault guns. Heavy weapons company: 4 × 76-mm guns. | CCA 1955 TOE (1960 hbk) | [S6 p.42, fig. 24] |
| Armor branch | — | Consists of armored divisions, separate tank regiments and the tank-assault gun regiment. Self-propelled artillery counts as armor. | CCA 1955 TOE (1960 hbk) | [S6 p.40, para 47; p.43, para 48b] |
| Artillery division (Gun) | 5,700 | A 122-mm gun regiment and a 152-mm gun-howitzer regiment, each 3 battalions of 12 guns. Also an AAA battalion, a command company, a guard platoon and a chemical warfare company. | CCA 1955 TOE (1960 hbk) | [S6 p.44, fig. 26(1)] |
| Artillery division (Howitzer), the Chinese counterpart of a Soviet breakthrough division | 6,700 | A 122-mm howitzer regiment and a 152-mm howitzer regiment (each 3 battalions of 12). A rocket launcher regiment (980 men; 2 battalions of 12 × 132-mm RL). A heavy mortar regiment (unconfirmed). An AAA battalion. | CCA 1955 TOE (1960 hbk) | [S6 p.43, para 48b; p.44, fig. 26(2)] |
| Antitank artillery division | 4,200 | Probably 4 antitank regiments of 890 men, each with 24 AT guns. One regiment has 57-mm, one 85-mm, one 100-mm, and one mixed 57-mm and 76-mm. Antitank battery 90 men. | CCA 1955 TOE (1960 hbk) | [S6 p.45, fig. 27] |
| Antitank organization (other) | — | Antitank battalions are organic to the infantry division, and antitank companies are organic to the regimental heavy weapons battalion. | CCA 1955 TOE (1960 hbk) | [S6 p.43, para 48c] |
| AAA division | 3,100–4,700 (flexible) | 3 AAA regiments: medium; medium or light; light. HQ 380. Heavy AAA battery: 4 × 85/100-mm guns. AAAW battery (90 men): 4 × 37/57-mm guns. There are also separate AAA regiments. Personnel come from the Army, Air Force or Navy, under Air Defense HQ. | CCA 1955 TOE (1960 hbk) | [S6 p.43, para 48d; p.46, fig. 28] |
| Public security division (second-line) | 8,200 | Public security regiment 2,350. Battalion 610. Artillery battery: 4 × 76-mm guns. HMG company: 9 HMG. Mortar company (190): 9 × 82-mm mortars. Lighter armament than the line units; not capable of sustained independent combat. | CCA 1955 TOE (1960 hbk) | [S6 p.48, para 51; p.49, fig. 31] |
| Cavalry division | "5000 ?" (as printed) | Cavalry regiment, battalion, company, platoon and squad, plus mortar platoons. A mounted light force for western China. | CCA 1955 TOE (1960 hbk) | [S6 p.47, fig. 29; p.48] |
| Motor transport regiment | — (see Gaps) | Regiment: 525 trucks (4×2/6×6), 20 × ¼-ton, 14 wreckers. Battalion: 140 trucks, 5 × ¼-ton, 4 wreckers. Company: 46 trucks. Supplements units that lack organic transport. | CCA 1955 TOE (1960 hbk) | [S6 p.48, para 54; p.50, fig. 32] |
| Engineers | — | Engineer regiments at army level; engineer battalions organic to armies and infantry divisions; companies in other combat units. Tasks: fortifications, obstacles, demolition, bridging and roads. | CCA 1955 TOE (1960 hbk) | [S6 p.48, para 52] |
| Chemical / medical | — | A chemical warfare company per division and a platoon per regiment; they also operate flamethrowers and smoke generators. Medical: the army has 2–5 mobile field hospitals, the division a medical battalion, the regiment a medical company. | CCA 1955 TOE (1960 hbk) | [S6 p.51, paras 55–56] |

#### C. Weapons and vehicles (1960 standard; post-Korea)

| Class | Items as stated | Period | Cite |
|---|---|---|---|
| Small arms | AK (Type 56) is the standard shoulder arm. The RPD (Type 56) is the squad LMG and the RP-46 the company LMG; the DP/DPM is being replaced. | CCA 1955 TOE (1960 hbk) | [S6 p.71, para 95; p.74, para 97] |
| HMG / AA MG | The SG-43 (Type 53) is the battalion HMG: wheeled, AA-capable, with a 250-round belt. The 12.7-mm DShK (Type 54) and the 14.5-mm ZPU-2 and ZPU-4 are used mainly against low-level air attack, and also in ground support. The ZPUs have manual traverse and elevation and no off-carriage fire control. | CCA 1955 TOE (1960 hbk) | [S6 p.74–75, para 97] |
| Infantry AT | 90-mm Type 51 RL (a copy of the US M20). 57-mm Type 36 RR (copy of the M18). 75-mm Type 52 RR. The RPG-2 has an effective range of 151 m [165 yd]. | CCA 1955 TOE (1960 hbk) | [S6 p.75, para 98a; p.77, para 98b] |
| Mortars | 82-mm M1937, 120-mm and 160-mm M1943. Mortars are often used as a substitute for artillery. The 82-mm breaks down into a three-man or one-animal load. | CCA 1955 TOE (1960 hbk) | [S6 p.77, para 99] |
| Field artillery | 85-mm D-44 field gun: range 15,540 m [17,000 yd], penetrates 137 mm at 503 m [5.4 in at 550 yd]. 122-mm howitzer M1938: range 11,890 m [13,000 yd]. 122-mm gun D-74. 152-mm gun-howitzer D-20: range 17,370 m [19,000 yd]. 152-mm howitzer M1943: range 12,400 m [13,560 yd]. | CCA 1955 TOE (1960 hbk) | [S6 p.79, para 101a; p.81] |
| AT artillery | 57-mm M1943 (Type 55): HE range 8,230 m [9,000 yd], penetrates 140 mm at 503 m [5.5 in at 550 yd]. Also the 85-mm D-44. | CCA 1955 TOE (1960 hbk) | [S6 p.83, para 102] |
| AAA guns | 57-mm S-60 and 100-mm KS-19 are standard. The 37-mm M1939 (Type 55) and 85-mm M1939 are being phased out. | CCA 1955 TOE (1960 hbk) | [S6 p.83, para 103] |
| AAA fire control | WHIFF radar (a copy of the SCR-584) tracks at 37 km [20 nmi] and works with the 85-mm gun. The PUAZO-3 director has a slant range of 11,890 m [13,000 yd] and a height range of 9,450 m [31,000 ft]. The PUAZO-6 and FIRE CAN are used with the 57-mm and 100-mm guns. | CCA 1955 TOE (1960 hbk) | [S6 p.91, para 111] |
| Armor pictured | T-54 medium tank; JSU-122 SP gun; T-34 (85) on a ponton ferry. The descriptive text (paras 105–107) is missing; see Gaps. | CCA 1955 TOE (1960 hbk) | [S6 p.87; p.89; p.113] |
| Trucks | GAZ-51, ZIS/ZIL-150, GAZ-63, ZIS/ZIL-151, ZIS-157; GAZ-67B and GAZ-69 jeeps. A K-800 prime mover towing an 85-mm gun is pictured. Animal and human portage is still used over short distances. | CCA 1955 TOE (1960 hbk) | [S6 p.116, paras 139–140] |
| Bridging | Improvised footbridges. Wooden ponton boats for ferrying or bridging (6.6–33 short tons). Steel ponton boats (bridging 6.6–82.5 short tons). Bamboo and timber short-span bridges that can carry tanks and artillery. | CCA 1955 TOE (1960 hbk) | [S6 p.113, para 131] |

#### D. Tactics that affect placement (1960 doctrine; much of it rooted in Korean War practice)

| Topic | Rule as stated | Period | Cite |
|---|---|---|---|
| Force ratio | Accept a decisive engagement only with 2 to 6 times the enemy's strength. Surround the enemy and attack from at least two directions. | CCA doctrine (1960 hbk) | [S6 p.24, para 22b(4)–(5)] |
| Night / infiltration | Doctrine emphasizes night operations and infiltration to the enemy's flanks and rear, or into positions that block his withdrawal. | CCA doctrine (1960 hbk) | [S6 p.28, para 26] |
| Four Fast–One Slow | "One Slow" is careful planning. "Four Fast" means speed in preparation, advance, exploitation and pursuit. | CCA doctrine (1960 hbk) | [S6 p.28, para 27a] |
| One Point–Two Sides | Overwhelming strength concentrated on one weak point, attacked by two or more efforts from units that deployed before the battle. It is not limited to two sides and is distinct from an envelopment. | CCA doctrine (1960 hbk) | [S6 p.28–29, para 27b] |
| Divide-and-Destroy | Isolate and reduce individual strong points, then roll up adjacent ones from the flank or rear. | CCA doctrine (1960 hbk) | [S6 p.29, para 27c] |
| Deployment echelons | Only regiments and smaller units use attack positions; divisions and larger deploy through successive assembly areas. Assault positions are as close to the objective as possible, allow assault from several directions, and aim the main effort at the flanks or rear. | CCA doctrine (1960 hbk) | [S6 p.30, para 28a–b] |
| Unobserved approach | When cover exists (terrain, darkness, fog, smoke), the attack phase is skipped and companies move straight from the battalion attack position to the assault positions. | CCA doctrine (1960 hbk) | [S6 p.30, para 28b] |
| Division attack depths | Initial objectives lie 3.2–4.0 km [2–2.5 mi] behind the enemy MLR. Final objectives are 9.7–14.5 km [6–9 mi] deep. | CCA doctrine (1960 hbk) | [S6 p.30, para 29b] |
| Pursuit | Begin pursuit at once if the enemy retreats, even before the final objectives are taken. | CCA doctrine (1960 hbk) | [S6 p.30, para 28e] |
| Defense type | Mobile defense is preferred: lure the enemy into an ambush by overwhelmingly superior forces. Position defense is used only for vital areas and does not include delaying on successive positions. | CCA doctrine (1960 hbk) | [S6 p.31, para 31a–c] |
| Overrun position | Defenders fall back into bunkers, call artillery and tactical air onto their own position, and wait for a counterattack. | CCA doctrine (1960 hbk) | [S6 p.31, para 31c] |
| Wide front | Hold the critical terrain that controls adjacent ground rather than a continuous line. | CCA doctrine (1960 hbk) | [S6 p.32, para 31d] |
| Defense zones (army and above) | From front to rear: advance zone; main defense zone (main and second battle positions); second defense zone. Alternate zones are added as time allows. | CCA doctrine (1960 hbk) | [S6 p.32, para 32a] |
| Defense-zone depths (fig. 15) | Combat security positions 4–6 km. Main defense zone 12–15 km. One or more alternate defense zones to a total depth of 50–75 km. Interior divisions and armies may put 3 components on line in secondary sectors. | CCA doctrine (1960 hbk) | [S6 p.33, fig. 15] |
| Work priorities | (1) Mines, obstacles and fieldworks forward of the main zone; (2) clearing fields of fire; (3) battalion-area fortifications, AT defenses and trenches; (4) the same in the rest of the zone; (5) artillery-firing and tank positions; (6) camouflage. | CCA doctrine (1960 hbk) | [S6 p.32, para 33a] |
| Fortification style | Built in depth for mutual support. Unusually strong, well concealed, with a low silhouette. Sited on terrain that is itself an obstacle, plus minefields and AT ditches. | CCA doctrine (1960 hbk) | [S6 p.32, para 33b–c] |
| Camouflage | Emphasized, especially in the main defense position. Uses field expedients and local materials. Deceptive camouflage continues after a position is evacuated. Camouflage nets are scarce, so troops rely almost entirely on natural materials. | CCA doctrine (1960 hbk) | [S6 p.32, para 33d; p.114, para 133] |
| Artillery employment | Artillery is organic from army down to regiment. "Combat groups" of artillery are attached directly to the infantry they support. | CCA doctrine (1960 hbk) | [S6 p.27, para 24c] |
| Infiltration raids | Raids are reinforced with extra automatic weapons, light mortars and grenades, and last hours to days. Recon personnel sometimes wear civilian clothes. Airborne raids are platoon or company size. | CCA doctrine (1960 hbk) | [S6 p.35, para 36] |
| Rail movement | 48 men per 30-ton boxcar, with the platoon as the loading unit. An average train is 30 cars, roughly 1 infantry battalion. At least 85% of long moves on the mainland go by rail. | CCA doctrine (1960 hbk) | [S6 p.65, para 82] |
| Motor movement | Only about 5% of troop moves are by motor. Convoys average 24 km/h [15 mph] by day, about 193 km [120 mi] a day, and about 16 km/h [10 mph] at night. A truck carries 13–16 men. Units often take up tactical march formations after they detrain or detruck. | CCA doctrine (1960 hbk) | [S6 p.65–66, paras 81a, 83] |
| Foot movement | Normal march day 7–8 h; 10–12 h counts as forced; 16 h is the maximum. Road speed 4.8 km/h [3 mph] by day, 3.2 km/h [2 mph] at night, 1.6 km/h [1 mph] cross-country. | CCA doctrine (1960 hbk) | [S6 p.67–68, para 86] |
| Night training | Night training is "considered of great importance". | CCA (1960 hbk) | [S6 p.59, para 72b] |
| Minefield patterns | AT road "W": mines 3.0–4.9 m [10–16 ft] apart; legs up to 22.9 m [75 ft] apart at the closed end and 68.6 m [225 ft] at the open end. Field zig-zag: AT mines 3.0 m [10 ft] apart; AP mines 0.9 m [3 ft] apart. Forward minefields are not marked, and hasty ones are not recorded. | CCA doctrine (1960 hbk) | [S6 p.96, para 114a; p.97, para 114b–c] |

#### E. Chinese Communist Air Force (1960 handbook, with Korea notes)

| Topic | Fact | Period | Cite |
|---|---|---|---|
| Air regiment | The basic combat unit, "normally" 37 aircraft, split into 3 squadrons for tactical control. | CCAF (1960 hbk) | [S6 p.123, app. II para 6] |
| Air division | "Believed to consist of" 2–3 regiments. It is the largest combat organization. | CCAF (1960 hbk) | [S6 p.123, app. II para 6] |
| Types | Fighters: mainly MiG-15 and MiG-17, with a few MiG-19s. Ground attack: Il-10. Light bombers: Il-28 and Tu-2. A few Tu-4. Transports: Li-2 and Il-12. | CCAF (1960 hbk) | [S6 p.121–122, app. II para 2] |
| Air-ground support | The principal mission is air defense. Strikes are preplanned, requested or made on the air commander's initiative, and are synchronized with the lifting of friendly artillery fire. The ground commander chooses targets and timing. | CCAF (1960 hbk) | [S6 p.123, app. II paras 7–9] |
| Korea-era airfields | In October 1951 the North Koreans built three new airfields a few miles apart (Saamcham, Taechon, Namsi), 80–113 km [50–70 mi] N and NW of Pyongyang. | S10 (USAF, Korea) | [S10 p.36] |

### 5.2 North Korean People's Army

Sources: S9 (Futrell, a USAF history) and S10. Neither gives a KPA table of organization. The strengths below are USAF or FEC estimates, and the losses are USAF claims or POW statements. See also the table A rows shared with the CCF (trenches and tunnels, rail repair, night marches, sentries, flak traps).

| Topic | Fact | Period | Cite |
|---|---|---|---|
| Origins | The army grew around 2 hardened divisions of Korean exiles who had served in Soviet forces. In 1949–50 the CCF handed over 3 complete divisions of Koreans. | S9 (Futrell, Korea), 1945–50 | [S9 p.40] |
| Pre-war intelligence | On 25 May 1950, KMAG knew of 6 regular divisions between the 38th and 39th parallels and suspected 7 more forming near the Manchurian border. | S9 (Futrell, Korea), May 1950 | [S9 p.41] |
| Strength at invasion | On 25 June 1950: about 100,000 troops in 8 infantry divisions, 3 border constabulary brigades and 1 armored brigade. The infantry divisions and the armored brigade were "freely provided" with Soviet equipment. | S9 (Futrell, Korea), 25 Jun 1950 | [S9 p.41] |
| Invasion method | The attack began at 04:00 on 25 June under cover of bad weather. By 06:00, infantry columns "spearheaded by Soviet-built T-34 tanks" had broken through toward Kaesong and Chunchon. Small boats and junks put troops ashore south of Kangnung. On day 1, a tank thrust reached Uijongbu, 27 km [17 mi] north of Seoul. | S9 (Futrell, Korea), 25–26 Jun 1950 | [S9 p.27; p.29; p.30] |
| North Korean Air Force | One air division (fighter, ground-attack and training regiments), HQ Pyongyang. It had 62 Il-10, 70 Yak-3/Yak-7B, 22 Yak-16 transports and 8 Po-2 trainers, so 132 combat aircraft. These were mostly at the 2 Pyongyang fields and Yonpo, with forward strips being built at Sinmak, Pyonggang, Kumchon and Kansong. On 26 June, 10 Yak-7Bs and 2 Il-10s moved to Sinmak. | S9 (Futrell, Korea), Jun 1950 | [S9 p.41] |
| Armor employment | Tank battalions were attached to assault rifle divisions to spearhead major attacks. The infantry fixed each position and then outflanked it. Infiltrators disguised as refugees set up roadblocks behind UN lines. | S9 (Futrell, Korea), Jul 1950 | [S9 p.106–107] |
| Road columns at blown bridges | Convoys were found "bumper to bumper" against knocked-out bridges. When strafed, every man in the trucks stood up and fired his rifle at the aircraft. | S9 (Futrell, Korea), early Jul 1950 | [S9 p.107] |
| Ground fire, early | On 6 July, B-26s hit a tank and vehicle concentration north of Pyongtaek (claimed 6–10 tanks burning, trucks, horse-drawn vehicles and an MG position); ground fire downed 1 B-26. By 7 July, small-arms fire forced the B-26s up to medium altitude. | S9 (Futrell, Korea), Jul 1950 | [S9 p.108] |
| Column size seen from the air (claims) | 7–9 July, Pyongtaek–Seoul roads: 197 trucks and 44 tanks claimed. 10 July at Pyongtaek: a large convoy lined up north of a bombed bridge; the combined attack claimed 117 trucks, 38 tanks and 7 half-tracks. | S9 (Futrell, Korea), Jul 1950 | [S9 p.108; p.113] |
| Napalm vs T-34 | The rubber in Soviet tank treads meant a near miss with napalm usually set the tank on fire. North Korean infantry scattered when napalm or thermite fell nearby. | S9 (Futrell, Korea), Jul 1950 | [S9 p.117] |
| Adaptation to air attack | By mid-July: reluctant to move or fight by day. Tanks and trucks used back roads and trails for any daylight march. Forward dumps were dispersed, and troops kept "vigorous camouflage discipline". | S9 (Futrell, Korea), Jul 1950 | [S9 p.119] |
| Night movement | By August the KPA customarily moved at night and dispersed and camouflaged troops and equipment by day. At the Pusan perimeter it could "move and fight only at night". A photo shows a ferry boat carrying a partly camouflaged truck. | S9 (Futrell, Korea), Aug 1950 | [S9 p.156; p.158; p.159] |
| Perimeter strength | Around the Pusan perimeter: an estimated 150,000 troops in 13 rifle divisions, a tank brigade, a mechanized division and a tank division. The armor, battered by air attack, was "in shambles" and used in small groups. | S9 (Futrell, Korea), Aug–Sep 1950 | [S9 p.159] |
| Field orders on air defense | Captured orders told troops to camouflage extensively, dig air-attack emplacements, and turn ground weapons on aircraft. | S9 (Futrell, Korea), Aug–Sep 1950 | [S9 p.193] |
| Supply under interdiction | Ingenious repairs meant a road or rail cut rarely delayed supplies more than 1–2 days. POWs said air attack accounted for over 80% of about 800 trucks destroyed en route. Trained drivers ran short, so American POWs were made to drive. Animal-drawn transport and impressed "battalions of ROK civilians as human supply trains" were used more and more. POWs estimated over half the supply tonnage was destroyed en route. | S9 (Futrell, Korea), Aug 1950 | [S9 p.196] |
| Unit losses from air (POW accounts) | 8th Division, 5 August: ten 76-mm field guns, three 122-mm howitzers, 20 tanks and 50 ammunition trucks. 105th Tank Division: 6 tanks, 4 trucks and 150 men lost in one attack by 4 aircraft. 16th Tank Brigade: less than half its tanks reached combat. | S9 (Futrell, Korea), Aug 1950 | [S9 p.197] |
| Share of losses to air (POW-based table) | Credited to aircraft: tanks 452 (75%), trucks 637 (81%), artillery pieces 301 (72%), personnel 49,527 (47%). The OCR table is scrambled; the pairs follow printed order. | S9 (Futrell, Korea), Jun–Sep 1950 | [S9 p.197] |
| Retreat exposure | During the September 1950 counteroffensive, troops and equipment were out on the roads in daylight without camouflage or concealment. | S9 (Futrell, Korea), Sep 1950 | [S9 p.186] |
| Pyongyang last stand | A scratch force south of Pyongyang had about 25 tanks, 8 SP guns and several heavy mortars. | S9 (Futrell, Korea), Oct 1950 | [S9 p.230] |
| Road and rail repair organization | The North Korean military highway administration had 12 regiments, each of 3 or more 550-man battalions. Each battalion held a sector of an MSR, with platoons as close as every 3 km. The railroad bureau had 3 brigades of 7,700 men each, with 50-man repair units at major stations. Local labor was impressed at each break. | S9 (Futrell, Korea), 1951 | [S9 p.360] |
| Pyongyang flak (1952) | Pyongyang was defended by 48 guns and more than 100 automatic weapons, "one of the worst 'flak traps' in Korea". | S9 (Futrell, Korea), Jul 1952 | [S9 p.539] |
| Pyongyang AA | After the 11 July 1952 strikes (targets included AA gun sites), North Korea upgraded its AA defenses. UN fighter-bombers and B-26s then had to bomb from higher altitude and gave up accuracy. | S10 (USAF, Korea), Jul 1952 | [S10 p.43] |

## 6. Naval

S7 is the CINCPACFLT Interim Evaluation Report No. 1, **Vol. I only**, which is the 28-page summary. S9 (Futrell) and S10 (USAF units) add material from the USAF side. OCR damage is marked `[OCR: …]`. Distances are given in km first, with the source's unit in brackets.

#### N1. Force composition and organization

| # | Period | Item | Value | Cite |
|---|---|---|---|---|
| 1 | Jun 1950 | Seventh Fleet's carrier strength at the outbreak | One large carrier, *Valley Forge*. The fleet moved from Philippine waters to Sasebo and came under Commander Naval Forces Far East (VAdm C. Turner Joy). | [S9 p.31] |
| 2 | Jun–Dec 1950 | Task Force 77 (fast carrier force) composition | "1 to [OCR: number illegible]" CV-9 class carriers, one cruiser and normally one battleship in the supporting group, plus a destroyer screen | [S7 p.22] |
| 3 | 31 Jul 1950 | TF 77 carrier count | *Philippine Sea* joined on 31 Jul, which doubled the strike aircraft. The Navy held that the two fast carriers had to operate together for mutual protection. | [S9 p.144] |
| 4 | 15–17 Sep 1950 | TF 77 at Inchon | Three fast carriers (*Boxer* had joined) gave air cover to the beachhead | [S9 p.180] |
| 5 | Jun–Dec 1950 | TF 77 air group aircraft types | AD, F4U, F9F jet fighter | [S7 p.22] |
| 6 | Jun–Dec 1950 | Jets on fast carriers | Every fast carrier had at least one jet squadron except one ship. Jets were almost never given close-support tasks. | [S7 p.22] |
| 7 | Aug–Sep 1950 | Escort carriers (CVE) with Marine air | *Sicily* and *Badoeng Strait* [OCR: "Bandoeng"] formed Task Element 96.23 off the south coast of Korea. They carried VMF-214 and VMF-323 (F4U). | [S9 p.143] |
| 8 | Jun–Dec 1950 | Use of the CVEs | For most of the first 6 months, Marine F4U fighter-bomber squadrons flew from two escort carriers, mainly on close air support | [S7 p.20] |
| 9 | 1950 | CVE aircraft limits | Neither the AD nor any current jet could operate from a CVE. Loaded F4U take-offs were marginal in low wind, and bomb loads were sometimes reduced. | [S7 p.20] |
| 10 | Nov 1950–Jan 1951 | Carriers on the line | *Valley Forge*, *Philippine Sea* and *Leyte* attacked the Sinuiju bridges from 9 Nov 1950 and carried most of the air support in central and eastern Korea in Jan 1951 | [S9 p.246], [S9 p.302] |
| 11 | Apr–May 1951 | TF 77 carriers | *Boxer*, *Princeton*, *Philippine Sea* (back from a Formosa Strait sweep on 16 Apr). Three fast carriers reported for close support on 18 May. | [S9 p.386], [S9 p.388] |
| 12 | 25 Aug 1951 | *Essex* jets | 23 F9F and F2H jets from *Essex* escorted 35 B-29s to Rashin | [S9 p.456] |
| 13 | Late 1951 | TF 77 carriers (east-coast rail interdiction) | *Bon Homme Richard*, *Essex*, *Antietam*. They kept 10 [OCR: "IO"] rail bridges and 17 highway bridges cut. | [S9 p.465] |
| 14 | 23 Jun 1952 | Sui-ho Dam strike (carrier contribution) | 35 AD plus 35 F9F from *Boxer*, *Princeton* and *Philippine Sea* | [S9 p.509] |
| 15 | 11 Jul 1952 | Commonwealth carrier | HMS *Ocean* (Sea Furies and Fireflies) flew two missions from the west coast in the Pyongyang strike. The strike also included Seventh Fleet, 1st MAW, ROKAF and Fifth AF aircraft. | [S9 p.539], [S10 p.43] |
| 16 | 29 Aug–13 Sep 1952 | Carriers | *Boxer* and *Essex* (216 sorties, Pyongyang, 29 Aug). *Essex*, *Princeton* and *Boxer* hit Aoji on 1 Sep (259 sorties, the largest all-Navy strike). *Bon Homme Richard* and *Princeton* hit Hoeryong on 13 Sep. | [S9 p.547], [S9 p.548] |
| 17 | Jun 1953 | TF 77 carriers | *Princeton*, *Boxer*, *Philippine Sea* and *Lake Champlain* stayed on the line for 7 days | [S9 p.696] |
| 18 | Jun–Nov 1950 | Designators in the S7 annex list (detailed volumes not held) | CTF 77 (air ops); COMCARDIV 15; COMPHIBGRU ONE = CTF 90 (amphibious); Blockading and Escort Force = [OCR: "cr1G 95.9", probably CTG 95.9]; COMCRUDIV-3 commanded TG 96.5 and 95.2 [OCR: split across lines] | [S7 p.5], [S7 p.6] |
| 19 | Jun–Nov 1950 | Other naval air units named (annexes) | Seaplane tenders USS *Gardiners Bay* (AVP-39) and USS *Curtiss* (AV-4); patrol squadrons VP-42, VP-47 and PATRON SIX (FAIRWING 6); submarine report from COMSUBPAC | [S7 p.5], [S7 p.7] |
| 20 | Jun–Nov 1950 | Force types that have their own studies in Vols. II–X (not held) | Fast carrier TF, support carrier TF, amphibious attack forces, blockade forces, convoy escort forces, naval bombardment forces, mine countermeasures | [S7 p.4], [S7 p.5] |
| 21 | 26–27 Jun 1950 | Early UN/merchant shipping | Norwegian merchant ship *Reinholte* took 682 evacuees from Inchon, met escorting destroyers on 27 Jun, and had F-82 and then B-26 cover | [S9 p.31] |

#### N2. Operating arrangements and tactics

| # | Period | Arrangement | Detail | Cite |
|---|---|---|---|---|
| 22 | 27 Jun 1950 | NavFE's first mission | Attack enemy vessels in Korean coastal waters south of 38°N, destroy North Korean invasion forces along the South Korean coasts, and isolate Formosa | [S9 p.46], [S9 p.47] |
| 23 | 30 Jun 1950 | Blockade | Truman approved a naval blockade of North Korea (proposed by Adm Sherman) | [S9 p.59] |
| 24 | 3–4 Jul 1950 | TF 77's first strikes | Airfields at Pyongyang and Onjong-ni: 2 Yaks shot down and 10 aircraft damaged on the ground. TF 77 kept radio silence at sea. | [S9 p.121], [S9 p.71] |
| 25 | 18–19 Jul 1950 | TF 77 airfield strikes | Pyongyang (14 destroyed, 13 damaged). East coast: Yonpo (15) and near Sondok (3). | [S9 p.121] |
| 26 | 25–26 Jul 1950 | TF 77 close support (SW Korea) | Exclusive area Kunsan–Chonju–Namwon–Kwangju. On 26 Jul about 60 sorties in 4 launches (12–16 aircraft each) worked under Mosquito control. Navy AD controllers stayed on station 3–4 h. | [S9 p.137], [S9 p.140] |
| 27 | 3 Aug 1950 | Priorities for carrier aircraft (NavFE–FEAF agreement) | 1) ground support under the JOC; 2) interdiction south of 38°N with Fifth AF; 3) interdiction north of 38°N with Bomber Command | [S9 p.140] |
| 28 | Aug 1950 | Carrier launch pattern | Carriers launched strikes by the deckload, and the flights stacked up waiting for "Mellow" control | [S9 p.144] |
| 29 | Aug 1950 | Marine CVE sortie rate | VMF-214 and VMF-323 flew about 45 close-support sorties a day for the 1st Provisional Marine Brigade | [S9 p.143] |
| 30 | 12–24 Aug 1950 | TF 77 operating area shift | Joy moved the carriers up the west coast to hit interdiction targets in North Korea. On 19 Aug *Philippine Sea* and *Valley Forge* sent 37 Corsairs and Skyraiders against the Seoul rail bridge. | [S9 p.151], [S9 p.153] |
| 31 | 1–3 Sep 1950 | TF 77 distance and sorties | When recalled, the carriers were in the NE Yellow Sea. They launched about 400 km (250 miles, unit unstated) from the target, flew 85 sorties on 1 Sep and 127 on 2 Sep, then broke off refuelling for 28 on 3 Sep. | [S9 p.164], [S9 p.165], [S9 p.166] |
| 32 | Jun–Dec 1950 | Replenishment cycle | Fast carriers took avgas 1 day in 3, which cut available strength by a third. CVEs took avgas 1 day in 8 or 9. | [S7 p.22] |
| 33 | Jun–Dec 1950 | Cruising formation and air defence | Circular anti-aircraft cruising formations with day CAP, and CAP over amphibious shipping. Radar pickets and AEW were generally not used. | [S7 p.25] |
| 34 | mid-Nov 1950 | TF 77 radar performance (2-week test) | In circular formation without pickets or AEW, the best detection of (friendly) raids was at about 80 km (50 miles, unit unstated). Two destroyers with AN/SPS-6B got 2–3× the TF 77 average. | [S7 p.22] |
| 35 | Jun–Dec 1950 | Small units inshore | Small task units of naval vessels and shipping worked along the Korean coasts, away from fighter cover | [S7 p.25] |
| 36 | Sep–Nov 1950 | Gunfire-support standoff forced by mines | East coast: ships stayed outside the 100-fathom curve, about 183 m (100 fathoms) depth. West coast: ships stayed outside mineable waters. | [S7 p.23] |
| 37 | 1950 | Helicopter roles with the fleet | Plane guard, transport between ships and ship to shore, gunfire spotting, rescue, mine location | [S7 p.24] |
| 38 | Jun–Dec 1950 | Nature of fast-carrier strikes | Most fast-carrier effort went to so-called interdiction, which S7 says was really armed reconnaissance. Navy close support was very effective under Marine control and rarely effective under Air Force control. | [S7 p.21], [S7 p.22] |
| 39 | Jun–Dec 1950 | IFF | The Mark 3 IFF was compromised. Friendly aircraft that did not use IFF near surface forces caused many false alarms. | [S7 p.25] |
| 40 | Feb–Mar 1951 | NavFE interdiction zones | NavFE took interdiction zones F, G and H, which run from Wonsan north to the Siberian border. Air and surface-gunnery attacks on the east-coast railway were coordinated from 20 Feb. The line was effective from about 8 Mar 1951. | [S9 p.340] |
| 41 | Aug 1951 on | Navy rail sectors | The Seventh Fleet took the lateral line Samdong-ni–Kowon and the east-coast line Kilchu–Hungnam–Wonsan–Pyonggang. Pilots called the lateral line "Death Valley" because of flak. | [S9 p.462], [S9 p.465] |
| 42 | Nov 1950 on | TF 77–JOC link | From Nov 1950 there was a naval liaison group at Fifth AF. TF 77 passed the next day's air schedule to the JOC by noon each day. The radio nets could carry about 1/10 of the traffic needed. | [S9 p.364], [S9 p.365] |
| 43 | Jul 1952 | Carrier dive recovery altitude | Adm Clark ordered TF 77 pilots to pull out no lower than 914 m (3,000 ft). Navy flak-suppression aircraft [OCR: "flak destroyers"] led the strikes of 11 Jul. | [S9 p.541], [S9 p.539] |
| 44 | Dec 1952 | "Cherokee" strikes | TF 77 and Fifth AF sent 24–36-aircraft strikes at targets beyond 3,000 m and generally within 20,000 m of the front. Almost all TF 77 aircraft flew Cherokee strikes. | [S9 p.641] |
| 45 | Summer–fall 1952 | Combined close-support rate | FEAF, Navy and USMC together flew 2,000–4,000 close-support sorties a month | [S10 p.44] |
| 46 | Oct 1952 | Navy peak month | 11,004 sorties [OCR: "II ,004"], the highest monthly total of the war. 667 sorties on 12 Oct (Kojo). | [S9 p.553] |
| 47 | 12 Jul 1953 | TF 77–Fifth AF link | A radioteletype circuit with on-line crypto was opened, and a naval member sat in the JOC (late Jun 1953) | [S9 p.698], [S9 p.699] |

#### N3. Amphibious operations

| # | Period | Operation | Detail | Cite |
|---|---|---|---|---|
| 48 | Jul–Aug 1950 | Pohang withdrawal | An isolated ROK division was taken off at Pohang without casualties, under the guns of the fleet | [S7 p.16] |
| 49 | Jul–Sep 1950 | 1st Provisional Marine Brigade | About 5,000 men. Re-embarked in Navy amphibious ships for Inchon. | [S7 p.17] |
| 50 | 15 Sep 1950 | Inchon, assault force | The 1st Marine Division was the only assault force. X Corps = 1st Marine Division + 7th Infantry Division. NavFE provided transport, air, naval gunfire and initial logistics. | [S7 p.16], [S7 p.17], [S9 p.170] |
| 51 | 15 Sep 1950 | Inchon, tides | Because of the tides, amphibious vessels could beach only on a few hours of 15 Sep, 11 Oct or 3 Nov | [S9 p.169] |
| 52 | 13–15 Sep 1950 | Inchon, preparation | Joint Task Force Seven. Two days of Marine napalm strikes plus destroyer bombardment neutralized Wolmi-do. | [S9 p.180] |
| 53 | Sep 1950 | Inchon, air arrangements | NavFE was to neutralize enemy airfields within about 241 km (150 miles) of Inchon from 2 Sep. MAG-33 went from the escort carriers to Kimpo (19–20 Sep). | [S9 p.173], [S9 p.181] |
| 54 | Sep 1950 | Inchon, USAF recon support | 8th TRS photographed the seawalls at high and low tide for the Navy a few days before the landing | [S10 p.85] |
| 55 | Oct 1950 | Wonsan, force size | About 50,000 X Corps troops embarked in Adm Struble's 250-ship armada. ROK I Corps took Wonsan by land on 10 Oct. | [S9 p.233], [S10 p.20] |
| 56 | Oct 1950 | Wonsan, air control | Adm Joy had coordination control within about 80 km (50-mile circle) of Wonsan. TF 77 gave initial air support and air defence. | [S9 p.224] |
| 57 | Oct 1950 | Wonsan, mine delay (**conflict**) | S7 says the administrative landing was delayed **5 days**. S9 says the force waited **6 days** while minesweepers cleared a channel, and landed 26 Oct. | [S7 p.23], [S9 p.236] |
| 58 | 29 Oct 1950 | Iwon landing | 7th Division landed over the beaches at Iwon, about 167 km (90 nautical miles) NE of Wonsan | [S9 p.236] |
| 59 | Dec 1950 | Hungnam evacuation | Army 3rd and 7th Divisions, 1st Marine Division [OCR: "1st Division of the U.S. Corps"] and about 91,000 refugees [OCR: "91,COO"], under fleet guns and aircraft | [S7 p.16] |
| 60 | 5–24 Dec 1950 | Evacuation dates and ports | Wonsan 5–15 Dec and Hungnam 15–24 Dec (S10). S9 adds that troops were loaded at Hungnam, Songjin and Wonsan, that naval gunfire and carrier aircraft laid a continuous barrage, and that the evacuation ended at 1436 on 24 Dec. | [S10 p.24], [S9 p.282] |
| 61 | Dec 1950 | Chinnampo evacuation | Heavy USAF and engineer equipment was taken out of Chinnampo on two LSTs | [S9 p.288] |
| 62 | 15 Oct 1952 | Kojo feint | Joint Amphibious Task Force Seven, "the largest naval force assembled since 1945". Landing boats turned back 3.66 km (4,000 yd) from the beach. | [S9 p.553] |
| 63 | 1950 | Amphibious shipping shortage | Too few LSTs for coastal shipping. Amphibious ships had limited room. | [S7 p.18], [S7 p.25] |

#### N4. Enemy naval, mine and air threat to ships

| # | Period | Threat | Detail | Cite |
|---|---|---|---|---|
| 64 | Jun–Dec 1950 | Enemy naval, air and submarine opposition | There was no effective enemy opposition at sea, in the air or by submarines, and no hostile submarine operations in the first 6 months | [S7 p.13], [S7 p.25] |
| 65 | Jun–Dec 1950 | Enemy surface threat | Sea power removed any chance of enemy surface action against Japan or Formosa | [S7 p.16] |
| 66 | to Nov 1950 | UN ship losses | UN ships sunk or damaged by mines: [OCR: "IO", probably 10]. By gunfire or bombs: 5. All naval vessels involved were destroyer type or smaller. | [S7 p.23] |
| 67 | Sep 1950 | First mines found | Off Chinnampo on the NW coast, [OCR: day illegible] Sep 1950. The major fields were laid from early Sep. | [S7 p.23] |
| 68 | Sep–Oct 1950 | Mine supply and laying | "Some [OCR: illegible],000" mines came by rail through Wonsan. Wonsan fields were laid under Soviet supervision (intelligence reports). NW-coast fields were laid by North Koreans alone. | [S7 p.23] |
| 69 | Sep–Nov 1950 | Mine types | Mostly chemical horned moored mines and magnetic bottom mines. No pressure mines were found. S9 calls the Wonsan mines "hair-triggered contact mines". | [S7 p.23], [S9 p.233] |
| 70 | Sep–Nov 1950 | Mine placement | Moored mines in shallow water within 1.8 m (6 ft) of the surface. Laying methods were primitive and cheap. | [S7 p.23] |
| 71 | Sep–Nov 1950 | Ports mined | Chinnampo (closed until swept, opened 9 Nov), Wonsan, Hungnam, Chongjin | [S7 p.23], [S9 p.254] |
| 72 | 1950 | Coast difference | Mine clearance was much easier on the west coast than the east coast because of differences in water depth and tides | [S7 p.23] |
| 73 | 1950 | UN countermeasures | Ships were reactivated and Japanese minesweepers were used. Seaplanes and helicopters located mines. Gunfire destroyed mines effectively, bombs did not, and depth charges were only slightly better. Only 4 submarines had mine detectors, and there were no mine-locator ships. | [S7 p.23] |
| 74 | 28 Jul 1950 | Friendly fire at sea | B-29 gunners shot down a Seafire from HMS *Triumph* | [S9 p.123] |
| 75 | 23 Aug 1950 | Enemy air vs ship | Two Yaks attacked and damaged a British destroyer off the west coast | [S9 p.124] |
| 76 | 17 Sep 1950 | Enemy air vs ship (Inchon) | At daybreak two Yaks dropped light bombs as near misses on heavy cruiser USS *Rochester*. One was shot down by HMS *Jamaica* while strafing her. Joy warned that up to 180 enemy fighters might be available. | [S9 p.180], [S9 p.181] |
| 77 | 18 Nov 1952 | Soviet MiGs vs TF 77 | During the TF 77 strike on Hoeryong, unmarked MiG-15s came from Vladivostok. Three F9Fs from *Oriskany* engaged and shot one down. | [S9 p.630] |
| 78 | 13 Sep 1952 | Soviet air near TF 77 | Fleet radar tracked presumed Soviet aircraft orbiting about 80 km (50 miles) east of Hoeryong over Siberia | [S9 p.548] |

#### N5. USAF–Navy interface relevant to naval scenes (S10, S9)

| # | Period | Item | Detail | Cite |
|---|---|---|---|---|
| 79 | 26 Jun 1950 | Evacuation cover | FEAF fighters covered ships evacuating Americans from Inchon | [S10 p.15] |
| 80 | from 27 Jun 1950 | Shipping escort by B-26 | 3d BG B-26s flew reconnaissance and protected allied shipping in Korean waters. They later tried wing-mounted naval searchlights for night work. | [S10 p.75] |
| 81 | Jul 1950 | Aircraft ferried by carrier | *Boxer* carried 145 F-51s as deck cargo from Alameda and reached Tokyo on 23 Jul | [S9 p.133] |
| 82 | Nov 1950 | F-86 ferry | 4th FIW Sabres arrived in Japan aboard aircraft carriers: escort carriers *Cape Esperance* and *Sitkoh Bay*, departing 1 and 9 Nov | [S10 p.67], [S9 p.435] |
| 83 | Jul 1951 | F-84 ferry | Two escort carriers sailed on 10 and 12 Jul with the 116th's 75 F-84s. 33 were damaged or corroded (S10). S9 says "nearly half". | [S10 p.62], [S9 p.424] |
| 84 | Nov 1950–1953 | USAF shipping surveillance | 512th RS (WB-29), to 20 Feb 1951. 56th SRS, to the end of the war. 91st SRS (RB-29/45/50) over the Sea of Japan near the Siberian coast. | [S10 p.90], [S10 p.91] |
| 85 | Jul–Nov 1951 | Naval gunfire direction by USAF | 67th TRG aircrews directed artillery and naval gunfire | [S10 p.88] |
| 86 | 1950–1953 | Sea rescue | 3d ARS used SA-16 amphibians. SA-16s could not normally land in waves over 1.5 m (5 ft). H-5 helicopters moved to Cho-do island (Yellow Sea) in Dec 1951. H-5s flew wounded to a hospital ship offshore (Dec 1951). | [S10 p.92], [S9 p.602] |
| 87 | 1951 | Cho-do radar | A surveillance radar on Cho-do (Yellow Sea) plotted MiG flights for Kimpo | [S9 p.445] |

## 7. IL-2 template mapping

The "IL-2 models" and "Game formation" columns are **suggestions**. Model names are the `.mgm` basenames in `TemplateExamples/ModelTypes.Group` (moving units) or `Unit_Template_Fixed.Group` (fixed units).

**What the game can do.** The Korea editor Formation Type list is planes then vehicles, and the file integer is that list index. Planes: V-Form (0), Left Edge Form (1), Right Edge Form (2), Column - Pairs (3), Column - Flights wedge (4), Column - Flights right (5), Column - Flights left (6), Heavy - Wedge (7), Heavy - Echelon right (8), Heavy - Echelon left (9), Heavy - Combat box (10), Heavy - User formation (11), then Reserved 9–15 (12–18). Vehicles: On Road Column one-way (19), On Road Column two-way (20), Off Road Column (21), Off Road User Formation (22), Forward (23), Backward (24), Stop (25), Panic Stop (26), Continue Moving (27), Set Direction and Stop (28), Reset Off Road User Formation (29), and User Formation Line Left / Right / Center / Back (30–33). Historical ground formations (platoon wedge, AA ring, battery layout) therefore have to be built from **where you place the units** in Template Builder or the editor, not from a formation command.

### Air

| Historical unit | Size / composition | IL-2 models (suggestion) | Game formation (suggestion) | Spacing / altitude to set | Cite |
|---|---|---|---|---|---|
| F-86 flight ("fluid four"), Dec 1950 → | 4 Sabres in two elements of 2. Element leaders shoot, wingmen cover. | `f86a5` | Column - Pairs (3) per element, or the app's finger-four layout | Entry at ≥M0.85; patrol at 8,230–10,060 m [27,000–33,000 ft] in Dec 1950 | [S9 p.272], [S9 p.273], [S9 p.274] |
| F-86 standard patrol, Dec 1950 | 16 = four flights of four | `f86a5` | Fighter Pack: 4 linked flights | Flights arrive **5 min apart at different altitudes**; about 20 min on patrol | [S9 p.273] |
| F-86 patrol, Nov 1951 | ≤32 Sabres in two 16-ship sections | `f86a5` | Fighter Pack: 2 packs of 4 flights | Stacked down from 10,670 m [35,000 ft] | [S9 p.436] |
| F-86 squadron "train", early 1953 | 6 flights of 4 in loose trail | `f86a5` | Fighter Pack: 6 flights | About 1.6 km [1 mi] between flights | [S9 p.632] |
| F-86 high/low patrol, Oct 1952 | F-86F high patrol, F-86E low patrol | `f86a5` (the only Sabre in the catalog) | Two Fighter Packs | High ≈12,190 m [40,000 ft]; low ≈9,140 m [30,000 ft] | [S9 p.536] |
| MiG-15 group, Apr 1951 | 4-ship flights in formations of up to 16. A bounced four splits into pairs, one climbing and one diving. | `mig15bis` | Column - Pairs (3) or V-Form (0) | — | [S9 p.320], [S9 p.324] |
| MiG "train", Nov 1951 | 60–80 MiGs. Flights drop off to engage Sabres; some go down after fighter-bombers. | `mig15bis` (scale it down; see checklist 8.2) | Several linked packs | Cross above 10,670 m [35,000 ft]; the fighter-bomber hunters drop to 4,570 m [15,000 ft] | [S9 p.436] |
| MiG hit-and-run, Mar–Apr 1952 | 2-, 4- and 6-ship formations | `mig15bis` | Column - Pairs (3) | Come out above 12,190 m [40,000 ft], leave low | [S9 p.443] |
| B-29 daylight raid, mid-1951 | 3 flights of 3 (airfields) or 2 flights of 4 (bridges) | `b29` / `simpleb29` | Heavy - Wedge (7) or Heavy - Combat box (10) | Stay above 5,490 m [18,000 ft] to be safe from flak (except on the Yalu) | [S9 p.430], [S9 p.634] |
| B-29 escort, Namsi, 23 Oct 1951 | 8 B-29s in 3 flights; 55 F-84s close escort; 34 F-86s screening | `b29`, `f84e`, `f86a5` | Bombers Heavy - Wedge (7); escorts Column - Pairs (3) | F-84s fly parallel to the bomber boxes (Apr 1951 practice); Sabres screen above | [S9 p.432], [S9 p.433], [S9 p.319] |
| B-29 night stream, Oct 1952 → | Single B-29s on shoran runs | `b29` | none (single aircraft) | **1-min intervals**; up to 9 aircraft within 305 m × 12.9 km [1,000 ft × 8 mi] | [S9 p.636] |
| B-29 vic formation, Jul–Aug 1950 | Squadron V formations; 47 B-29s over the target within 4 min | `b29` | Heavy - Wedge (7) | Base altitude 4,880 m [16,000 ft]; squadrons 5–10 min apart | [S9 p.212] |
| F-80 close support, Apr 1951 | Flight of 2 F-80s: four 260-lb frag bombs, 8 HVAR, 3,600 rounds .50-cal | `f80c10` | Column - Pairs (3) | — | [S9 p.386] |
| F-80 anti-tank rocket pass, Jul 1950 | Rocket attack | `f80c10` | — | Approach from 4 o'clock, dive at 30°, fire at ≈460 m [1,500 ft] | [S9 p.111] |
| F-51 armed recon, May 1951 | Never less than a full flight of 4 (because of flak) | `f51d` | V-Form (0) | Lower element searches, upper element covers (armed recon, Nov–Dec 1950) | [S9 p.357], [S9 p.268] |
| Fighter-bomber recovery rule, Sep 1952–Apr 1953 | — | `f84e`, `f80c10`, `f51d` | — | Minimum recovery altitude 914 m [3,000 ft] | [S9 p.661] |
| Il-10 attack, 20 Jun 1951 | 8 Il-10s with 6 Yak-9s joining | `il10`, `yak9p` | Il-10s V-Form (0); Yaks Column - Pairs (3) | — | [S9 p.332] |
| Tu-2 raid, 30 Nov 1951 | 12 Tu-2s, 16 La-9 escorts, 16 MiG cover | `tu2`, `la11` (no La-9 in the catalog), `mig15bis` | Tu-2s Heavy - Wedge (7) | — | [S9 p.437] |
| Early NKAF Yak raids, Jun–Jul 1950 | 5 Yaks at 3,050 m [10,000 ft]; sneak attacks in pairs, threes and fours | `yak9p`, `il10` | Column - Pairs (3) | 3,050 m [10,000 ft] | [S9 p.34], [S9 p.121] |
| Po-2 "Bedcheck Charlie", Jun–Jul 1951 | Pairs after midnight, low along valleys | no model | — | ≈148 km/h [80 kt] | [S9 p.331] |

### Ground

| Historical unit | Size / composition | IL-2 models (suggestion) | Game formation (suggestion) | Spacing to set | Cite |
|---|---|---|---|---|---|
| US rifle platoon, 1949–50 | 3 rifle squads (9 men, 1 BAR) plus 1 weapons squad (LMG and rocket launcher) | 3 × `squad-rifle-1950-usa` + 1 × `squad-mg-1950-usa` | placement only | 37 m [40 yd] between squads in line, vee or wedge; 23 m [25 yd] in column | [S2 p.110], [S2 p.163], [S2 p.179], [S2 p.473], [S2 p.474], [S2 p.476] |
| US rifle company in defense | 2 platoons forward, 1 in support | 9 × `squad-rifle-1950-usa` + 3 × `squad-mg-1950-usa`; weapons platoon 3 × 60-mm mortars (catalog mortars: `m29-mortar`, `m30-mortar`) | placement only | Frontage 549–1,097 m [600–1,200 yd]; support platoon 137–457 m [150–500 yd] behind; foxholes 5–18 m [5–20 yd] apart | [S2 p.343], [S2 p.348], [S2 p.156] |
| US regimental tank company | 4 platoons of 5 M26, plus 2 M45 in HQ (22 tanks, derived) | `m46` or `m4a3e8` (no M26 in the catalog) | On Road Column one-way (19) on the march | 46 m [50 yd] between vehicles by day; 19–32 km/h [12–20 mph] by day, 13–16 km/h [8–10 mph] at night | [S2 p.498], [S3 p.100], [S3 p.102] |
| US tank combat outpost | One or more reinforced tank platoons | `m46` / `m4a3e8` + US squads | placement only | 732–1,829 m [800–2,000 yd] in front of the main line | [S3 p.246] |
| US self-propelled AA battery (M16/M19) | 2 platoons × 4 sections; each section 1 M16 + 1 M19, so 8 + 8 | `m16-mgmc`, `m19` | placement only | Fire units 274–366 m [300–400 yd] apart (min 137 m [150 yd], max 686 m [750 yd]), at the edge of the defended object | [S5 p.10], [S5 p.53] |
| US towed AA section | 40-mm M2A1 with director, plus M55 quad .50 | `boforsl60`, `m2cal50-aa` (no M55 in the catalog) | placement only | same siting rules | [S5 p.10], [S5 p.53] |
| AA cover for a field artillery battalion | 1 AW battery per FA battalion | `m16-mgmc`, `m19` | placement only | ≥91 m [100 yd] from the nearest gun, preferably on the flanks | [S5 p.64] |
| US division artillery | 3 × 105-mm battalions + 1 × 155-mm battalion; one 105 battalion in direct support of each regiment | `m2a1-105mm`, `m1a1-155mm` | placement only | Battery layout: — (section 9) | [S2 p.488], [S1 p.10] |
| US advance-guard march (rifle company) | Point squad → advance party → support | `squad-rifle-1950-usa`, `willysmb` | placement or On Road Column one-way (19) | Road space / gap to next element: point 46 m / 183 m [50 / 200 yd]; advance party 69 m / 366 m [75 / 400 yd]; support 137 m / 457 m [150 / 500 yd] | [S2 p.92] |
| Communist flak on supply routes, spring 1951 | Truck-towed 37-mm M1939 guns | `61k` (37 mm) plus a tow truck | placement only | Effective to ≈1,370 m [4,500 ft] | [S9 p.357] |
| Communist heavy-flak defended area, winter 1952–53 | 85-mm M1939 guns; 20–30 searchlights per defended area | `ks12` (85 mm), `61k`; no searchlight model identified | placement only | 85 mm effective to ≈7,620 m [25,000 ft] | [S9 p.530] |
| Pyongyang flak trap, Jul 1952 | 48 guns and more than 100 automatic weapons | `ks12`, `61k`, `dshk-aa` | placement only | — | [S9 p.539] |
| CCF/KPA infantry, Korea | Massed infantry, few automatic weapons (1950); moved only at night from mid-Dec 1950 | `squad-rifle-1950-prc` / `-dprk`, `squad-smg-…`, `squad-mg-…` | placement only | Assault positions as close to the objective as possible; attack from at least two directions (doctrine in the 1960 handbook) | [S6 p.13], [S10 p.24], [S6 p.24], [S6 p.30] |
| Chinese truck convoy (1960 handbook) | 13–16 men per truck load | `studebakerus6`, `gaz63` | On Road Column one-way (19) | ≈24 km/h [15 mph] by day on Chinese roads, ≈16 km/h [10 mph] at night | [S6 p.66] |
| North Korean armor, Jun–Jul 1950 | Tank battalions attached to rifle divisions as spearheads. On 25 Jun 1950, 1 armored brigade with 8 infantry divisions. | `t34-85`, `su76m`, `studebakerus6` | On Road Column one-way (19) | Road columns piled up "bumper to bumper" at blown bridges; moves by day only until mid-Jul 1950 | [S9 p.41], [S9 p.106], [S9 p.107], [S9 p.119] |
| Communist supply route, 1951 | Night-running trucks with aircraft-warning sentries along the MSR; vehicles hidden at dawn | `gaz63`, `studebakerus6`; sentries as `squad-rifle-1950-prc` | On Road Column one-way (19), night activation | Sentries every 300–400 m along the route | [S9 p.360], [S9 p.285] |

### Naval

| Historical unit | Size / composition | IL-2 models (suggestion) | Game formation (suggestion) | Spacing to set | Cite |
|---|---|---|---|---|---|
| Task Force 77 (fast carriers), Jun–Dec 1950 | 1–3 CV-9 class carriers, 1 cruiser, normally 1 battleship, destroyer screen; circular AA formation with day CAP; no radar pickets | `gleaves` (destroyer screen only; no carrier, cruiser or battleship in the catalog) | placement only | — (screen distances: section 9) | [S7 p.22], [S7 p.25] |
| Escort carriers with Marine F4Us, Aug–Sep 1950 | *Sicily*, *Badoeng Strait* off the south coast | no model | — | — | [S9 p.143] |
| Gunfire-support ships, Sep–Nov 1950 | Destroyer-type and smaller | `gleaves` | placement only | East coast: outside the 100-fathom (≈183 m depth) line because of mines | [S7 p.23] |
| Amphibious lift | LSTs (in short supply), landing craft | `lst`, `lci`, `lcvp` | placement only | Kojo feint (Oct 1952): boats turned back 3.66 km [4,000 yd] from the beach | [S7 p.18], [S9 p.553] |
| Allied shipping | Merchant ships, sometimes escorted | `libertyship`, `cargoship1/2`, `tankership` | placement only | — | [S9 p.31] |
| Enemy small craft and minelayers | not covered by the sources | `sampan`, `seiner-gunboat`, `pinnace-gunboat` | — | — (section 9) | — |

## 8. Template checklists

Short checks to run before exporting a template. Each item repeats a fact from sections 2–7 with its citation. Items marked *app* are about this app, not history.

### 8.1 Fighter Pack: UN / US

- [ ] Flights of **four**, fought as two elements of two [S9 p.272], [S9 p.274].
- [ ] Flights spaced in **time and altitude**: 5 min apart at different altitudes in Dec 1950 [S9 p.273]; staggered in time or altitude and stacked down from 10,670 m [35,000 ft] in Nov 1951 [S9 p.436]; a squadron "train" of 6 flights about 1.6 km [1 mi] apart in early 1953 [S9 p.632].
- [ ] Patrol altitude matches the mission date:
  - Dec 1950: 8,230–10,060 m [27,000–33,000 ft] [S9 p.272]
  - Mar–Apr 1952: stacked down from 12,190 m [40,000 ft] [S9 p.444]
  - Oct 1952: F-86F high patrol at ≈12,190 m [40,000 ft], F-86E low patrol at ≈9,140 m [30,000 ft] [S9 p.536]
  - Jul 1953: median combat altitude 6,100 m [20,000 ft] [S9 p.678]
- [ ] Time on station about 20–25 min (Dec 1950 – Mar 1951) [S9 p.273], [S9 p.318].
- [ ] Home base matches the date:
  - 4th FIW: Suwon K-13 from spring 1951, Kimpo K-14 from Aug 1951 [S10 p.67], [S10 p.68]
  - 51st FIW: Suwon K-13 from Jul/Oct 1951 [S10 p.71], [S10 p.72]

### 8.2 Fighter Pack: Communist

- [ ] MiG-15 flights of four, in formations of up to 16, in Apr 1951 [S9 p.320], [S9 p.324]. Two-, four- and six-ship formations in Mar–Apr 1952 [S9 p.443].
- [ ] Entry altitude above your Sabres: above 10,670 m [35,000 ft] in Nov 1951 [S9 p.436]; 12,190 m [40,000 ft] or higher in Feb–Apr 1952 [S9 p.443].
- [ ] Some flights hunt fighter-bombers at ≈4,570 m [15,000 ft] [S9 p.436].
- [ ] Attacks timed for when Sabres are leaving: the end of their patrol (Dec 1950) [S9 p.273], or the "box-in" of retiring Sabres (Dec 1952 – Jan 1953) [S9 p.630].
- [ ] Came from the Antung / Sui-ho side of the Yalu [S9 p.436].
- [ ] *app:* historical MiG "trains" were 60–80 aircraft [S9 p.436], but the manual advises staying under about 30 random units in a whole mission. Represent a train with a few linked packs, not its real size.

### 8.3 Template Builder: strike and escort groups

- [ ] Daylight B-29 cell: 3 × 3 against airfields, or 2 × 4 against bridges (mid-1951) [S9 p.430].
- [ ] B-29s above 5,490 m [18,000 ft] to be safe from flak, except on the Yalu [S9 p.634].
- [ ] Escort: close escort beside the bomber boxes, Sabre screen above (Mar–Apr 1951) [S9 p.319].
- [ ] From Nov 1951, B-29s fly **at night, singly**, along shoran runs [S9 p.438]. From Oct 1952 they are 1 min apart [S9 p.636].
- [ ] Fighter-bombers pull out no lower than 914 m [3,000 ft] (Sep 1952 – Apr 1953) [S9 p.661].
- [ ] F-51 armed reconnaissance in flights of four, never fewer [S9 p.357]. The lower element searches while the upper element covers [S9 p.268].

### 8.4 Army Generator: armor

- [ ] US regimental tank company: 22 tanks in 4 platoons of 5 plus 2 in HQ (derived) [S2 p.498].
- [ ] March: 46 m [50 yd] between vehicles by day [S3 p.102]; 19–32 km/h [12–20 mph] by day, 13–16 km/h [8–10 mph] at night [S3 p.100].
- [ ] Combat outpost of one or more reinforced tank platoons, 732–1,829 m [800–2,000 yd] in front of the main line [S3 p.246].
- [ ] Tank-to-tank spacing inside platoon formations is **not sourced** (section 9). Set it by eye.
- [ ] North Korean armor in 1950: tank battalions spearheading rifle divisions [S9 p.106]. Columns bunched at blown bridges [S9 p.107]. Only night moves from mid-Jul 1950 [S9 p.119].

### 8.5 Army Generator: artillery and anti-aircraft

- [ ] US division artillery: 3 × 105-mm battalions and 1 × 155-mm battalion; one 105 battalion supports each regiment [S2 p.488], [S1 p.10]. Battery layout is **not sourced** (section 9).
- [ ] AA fire units 274–366 m [300–400 yd] apart, never under 137 m [150 yd] or over 686 m [750 yd], placed at the edge of the defended object and extended along approach routes [S5 p.53].
- [ ] One AW battery per artillery battalion, at least 91 m [100 yd] from the nearest gun, on the flanks [S5 p.64].
- [ ] Self-propelled AA battery: 8 × M16 + 8 × M19 [S5 p.10].
- [ ] Enemy flak:
  - Truck-towed 37 mm along supply routes (spring 1951) [S9 p.357]
  - Heavy concentrations at Pyongyang, Sinanju, Antung–Sinuiju, Sui-ho and Manpojin, with 20–30 searchlights per defended area (winter 1952–53) [S9 p.530]

### 8.6 Army Generator: infantry

- [ ] US platoon: 3 rifle squads plus a weapons squad [S2 p.179]. Squads 37 m [40 yd] apart in line, vee or wedge [S2 p.473], [S2 p.474], [S2 p.476].
- [ ] US company defense: 549–1,097 m [600–1,200 yd] of front, two platoons forward [S2 p.343]. Foxholes 5–18 m [5–20 yd] apart [S2 p.156].
- [ ] Combat outposts 732–1,829 m [800–2,000 yd] forward [S2 p.384].
- [ ] Chinese / North Korean:
  - Attack from at least two directions (doctrine in the 1960 handbook) [S6 p.24]
  - From Jan 1952, dug-in trenches and tunnels [S10 p.40]
  - From mid-Dec 1950, movement only at night [S10 p.24]

### 8.7 Army Generator: supply columns

- [ ] US advance-guard spacing, point to support: 183 m, 366 m and 457 m [200, 400, 500 yd] gaps [S2 p.92].
- [ ] Communist road movement at night after mid-Dec 1950 [S10 p.24]. Use night activation for eastern convoys.
- [ ] Chinese convoy speed about 24 km/h [15 mph] by day and about 16 km/h [10 mph] at night (1960 handbook) [S6 p.66].
- [ ] Communist supply routes in 1951: aircraft-warning sentries every 300–400 m [S9 p.360]; vehicles hidden in ravines and under bridges at dawn [S9 p.285]; flak traps and decoy convoy lights [S9 p.360].

### 8.8 Army Generator: ships

- [ ] TF 77: carriers + 1 cruiser + normally 1 battleship + destroyer screen in a circular AA formation. Only the destroyer (`gleaves`) exists in the catalog [S7 p.22], [S7 p.25].
- [ ] Gunfire ships stay outside the 100-fathom line on the east coast, because of mines [S7 p.23].
- [ ] Mined ports: Chinnampo, Wonsan, Hungnam, Chongjin (1950) [S7 p.23].

### 8.9 Searchlights (night defended area)

**What the sources say:**
- There were about 500 lights in all, 20–30 per defended area, with beams reaching 9,140 m [30,000 ft] (winter 1952–53) [S9 p.530]. Uiju had more than 50 [S9 p.440].
- Lights and heavy guns were sited **along the shoran approaches**, not evenly around the target (Oct 1951 – Feb 1952) [S9 p.440], [S9 p.446].
- Their job was to **hold the bomber for night fighters and flak**. Over Kwaksan, 24 lights held the B-29s while about 12 jets attacked [S9 p.447]. Later a spotter aircraft dropped flares to guide the lights and fighters [S9 p.634].
- B-29s flew above 5,490 m [18,000 ft] [S9 p.634]. The night fighters had no radar and relied on GCI plus visual contact [S9 p.635].

**What that means in the game (derived and editorial, not sourced):**
- **Reach.** With a 9,140 m beam, a light can reach a bomber at 5,490 m only if the bomber is within about **7.3 km** horizontally, and at 7,620 m only within about **5.0 km** (Pythagoras on the sourced figures). A light far from the bomb run never touches the bomber.
- **Count.** One game light stands for a historical battery of several. Use **4–6 lights per defended area**, and **up to 8** for a top target (Pyongyang, the Sinanju bridges, Sui-ho), instead of 20–50. Each light is an AI vehicle with its own attack-area command, and a light only helps when there is flak or a fighter to use it.
- **Where to put them:**
  - Two or three **pairs** under the known bomb-run approach, starting a few km before the release point and ending at the target. Put the two lights in a pair **a few hundred metres apart** so their beams cross on one bomber.
  - Heavy guns (`ks12`) go on the same axis [S9 p.440].
  - No lights on the far side of the target.
- **When they run:**
  - Night missions only.
  - Spawn or activate them from a Zone IN checkzone around the approach (about 15–20 km, so they are on before the bomber enters their 5–7 km reach). Delete or deactivate them on Zone OUT. This is the app's Template Builder zone logic.
  - Don't start them at Mission Begin.
- **Pair them with something that uses the light:**
  - Tie the lights to 2–4 AI `mig15bis` night fighters (the historical ratio was 2 lights per fighter [S9 p.447]), or to the flak.
  - Lights alone cost server resources and add almost nothing.
- **Model.** Only `ge1942a` (a US 60-inch light) is in the catalog. Eastern sites have to reuse it. The US airfield groups (`References/K1x AFB_mp.Group`) drive one light with two attack-area commands: `VERTICAL_SearchLightArea` at ≈3,000 m and `SearchLightArea` near the ground.

### 8.10 Army Generator: trains

- [ ] US rail moves: AA weapons on flat cars throughout the train [S5 p.63–64].
- [ ] Eastern trains on the east-coast and lateral lines faced carrier strikes; the lateral line was "Death Valley" because of its flak (Aug 1951 →) [S9 p.462], [S9 p.465].
- [ ] Eastern rail cuts were repaired fast; one cut closed the Chongju–Sinanju line for only five days (Jan–Mar 1952) [S10 p.39].

## 9. Gaps

What the sources do not cover, where they conflict, or where the OCR is unreadable. Nothing listed here appears as a value in sections 2–8.

### Air

- **Unavailable sources:** the Soviet 64th IAK report of July 1953 (S8) and the DTIC MiG studies (ADA177788 *MiG Operations in Korea*, ADA440091 *MiG Alley*, ADA340611) could not be fetched. As a result there is nothing on:
  - Soviet or Chinese MiG unit organisation (regiment and squadron sizes, pairs and fours doctrine);
  - Soviet MiG formation spacing;
  - MiG-side loss figures;
  - the split between Soviet, Chinese and North Korean pilots by date. Futrell gives only anecdotal nationality evidence.
- **Formation spacing:** S9 gives no lateral or vertical distances inside a fingertip or fluid-four flight, and none between elements. The only spacing figure is ≈1.6 km [1 mi] between flights in the 1953 "train". The 1950 figure is time, not distance: 5-min intervals.
- **Speeds:** Sabre patrol speeds are given only as Mach (0.62 → ≥0.85–0.87; ~98% power in 1953). No knots or IAS figures.
- **Ordnance:** there are no standard loads for F-51, F-84 or B-26 fighter-bomber and interdiction sorties. The only specific loads are:
  - early-war F-80 (HVAR, 1,000-lb bombs);
  - the Apr 1951 F-80 close-support mission;
  - the Feb 1953 Sui-ho F-84 strike;
  - the 1953 F-86F.
- **B-29 daylight 1951:** formation altitudes and box geometry for the 1951 raids are missing. The minimum bombing altitude at the Yalu bridges in Nov 1950 is illegible in the OCR [S9 p.246].
- **Piston-type tactics:** there is little on how La-9, La-11, Yak-9 or Tu-2 formations were flown beyond the counts given (e.g. 12 Tu-2 + 16 La-9 + 16 MiG). Nothing on their altitudes.
- **Il-28:** strength only. No combat use is recorded in S9.
- **AAA:** totals, calibres and hot spots are given, but there is no flak order of battle by unit and no heavy-gun density per defended target, except Pyongyang (Jul 1952) and Sui-ho (Feb 1953).
- **Base conflicts:**
  - 4th FIW Detachment A in Dec 1950: S9 says Kimpo, S10 says Taegu. Both are shown.
  - 35th FIW/FIG "Pusan AB": mapped to Pusan East (K-9) from the S10 station roster.
- **Coverage of the order of battle:** S10 covers USAF units only. Marine, Navy, RAAF, SAAF and ROKAF units appear only in passing mentions. Some tactical-support wings (6002d, 6131st, 6149th, 6150th) are not tabulated.
- **Chosen fighter group:** S10 p.35 says a USAF fighter-bomber group was at Chinhae in 1951 but does not name it. The 18th FBG station list implies it was the 18th.

### Air, early war 1950

- **F-51 altitudes and dive geometry.** For 1950, S9 gives no cruise altitude, attack entry altitude, dive angle, release altitude or pull-out height. The only figures are ceilings (46 m [150 ft]) and "letting down through holes". The number of rockets per Mustang is not stated.
- **Flight size as doctrine.** S9 never states a standard flight size for the F-51 in 1950; "flights of four" is inferred from the recorded instances. The rule that armed reconnaissance was never flown with less than a full flight is May 1951 (in air.md, [S9 p.357]), not 1950.
- **Napalm tanks.** The 416 L [110 gal] napalm tanks (17 Sep) are credited to "fighter-bombers" without naming the type.
- **Numbers by month.** There is no monthly F-51 or F-80 inventory. The data points are: 365 F-80 (31 May); 20 F-51 at Pohang (mid-Jul); 25 per squadron (30 Jul); 145 F-51 delivered by *Boxer*; 75 F-51 and 62 F-80 used on 20 Oct.
- **F-80 strafing geometry.** Pass altitude and speed are not quantified. Sorties per aircraft per day are given only as group or Fifth Air Force totals.
- **NKAF details.** There are no Yak or Il-10 altitudes apart from 3,050 m [10,000 ft] on 27 Jun. There are no weapon loads, and nothing on Il-10 attack technique. The Yak-3 / Yak-7B / Yak-9 split per raid is mostly unstated ("Yaks").
- **La-9 / La-11.** None in 1950 (see that section). S6 says nothing about air operations in Korea in 1950 beyond the CCAF being founded in 1950.
- **1950 flak.** S9 gives no North Korean AAA order of battle for 1950. It names only small arms and heavy AA machine guns in the south, and Chinese flak firing across the Yalu from 15 Oct. The B-29 minimum altitude over the Yalu is illegible in S9 p.246 (see air.md); S10 gives at least 6,100 m [20,000 ft] [S10 p.23].
- **Conflicts** (both versions are shown in the tables):
  - 49th FBG to Taegu: S9 says 28–30 Sep, S10 says 1 Oct.
  - 39th FIS to Pohang: S9 says 8 Aug, S10 says 10 Aug.
  - 18th FBG at Pusan East from 7–8 Sep (S9, S10 unit entry) versus the S10 campaign summary saying all FEAF units flew from Japan in Aug–Sep.
  - 35th FIW Yonpo→Pusan: S9 p.289 says "3 November" in December context; S10 says c. 3 Dec.
- **Illegible.** The individual times in the S9 p.104 close-support diagram cannot be read; only its caption is used.
- **Coverage.** S9 PDF pages 60–79 were checked by keyword search and contain no air-tactics data.

### US infantry

- **Personnel totals** for the rifle company, battalion and regiment are not given in S1 or S2. Both defer to the T/O&E (T/O&E 7-17 for the company), and the organization charts (S1 p.9 fig. 1, S2 p.73 fig. 1) are illegible in the OCR. Company HQ roles are listed without numbers for every role [S2 p.79–80, par. 11], so no company total can be derived.
- **Battalion composition** (the number of rifle companies and HQ elements) is never stated outright. Only the heavy weapons company is described, and defense doctrine implies three rifle companies (2 forward + 1 reserve).
- **Weapon counts** for the heavy weapons company (MGs, 81-mm mortars, 75-mm rifles per section) are not given. Squads and sections are listed, but not the weapons per squad or section.
- **Heavy mortar calibre**: the heavy mortar company section of S2 (app. IV sec. III) does not name the calibre. S2 p.237 mentions "81-mm and 4.2-inch mortars" in general terms only.
- **Rocket launcher calibre** (2.36-in vs 3.5-in) is not stated for the rifle platoon. "2.36-inch" appears only in the title of FM 23-30 in the reference list [S2 p.604].
- **Recoilless rifle / rocket launcher / .50 MG counts at company level** apart from those derived above (3 × 57-mm, 3 × 60-mm, 3 LMG, 3 rocket launchers, 9 automatic rifles). The number of cal .50 MGs per company is not stated.
- **Regimental vehicle totals** and **tank company personnel**: not given.
- **Pace length** is not defined in S1 or S2, so pace-based intervals (squad drill ≈5 paces, point ≥10 paces, squad column 30 paces, diamond 15 paces, support proper ≈2 paces) are left unconverted.
- **Platoon echelon distance**: the fig. 75 OCR reads "140 YARDS" / "YARDSO", which is ambiguous (probably 40 yd, like the other figures), so it is not used.
- **Squad skirmish-line and diamond geometry** (fig. 73) is illegible. Only the text distances (30 / 15 paces for the No. 2 man) are usable.
- **C1 (16 Oct 1950) change to par. 24**: the new spacing for the advance party is illegible in the OCR ("The advance party ' 'I paces between men"), so the 1949 value (≈5 paces) is shown and may have been superseded.
- **Motor-march vehicle spacing** (vehicle distance in close or open column, vehicles per mile) and **convoy speeds**: not in S1 or S2, which defer to FM 25-10. The S1 march table and strip map (pp. 377–380) are illegible.
- **Attack frontages for the battalion and regiment**: no figures. S1 only says secondary attacks get wider frontages and that frontages in woods depend on how dense the woods are [S1 p.201].
- **Company defense schematic distances** in S2 fig. 40 and fig. 54 (e.g. "150.300 YARDS", "6.200 YARDS") are garbled and not used.
- **Bivouac/assembly-area dispersal distances between vehicles or units**: none stated. S2 lists only "enough space for the proper dispersal" [S2 p.105, par. 37b(3)].

### US armor, artillery and AAA

- **S4 is the wrong manual.** `S4_FM6-140_FA_Battery.txt` contains FM 6-120 (*FA Observation Battalion and Batteries*, July 1951), not FM 6-140 [S4 p.3]. So none of the requested field-artillery firing-battery data are sourced: howitzers per battery, prime movers, personnel, spacing between pieces, battery front/depth, distance to FDC/OP, displacement or march intervals, and maximum howitzer ranges. The real FM 6-140 is needed. SOURCES.md should be corrected.
- **Tank company/platoon organization and formations.** FM 17-33 defers these to FM 17-32 (medium tank company) and FM 7-35 (tank company) [S3 p.29, par. 19]; [S3 p.308, par. 273]. Missing: tanks per platoon and company, company support vehicles, platoon formations (wedge, vee, diamond, line, echelon at platoon level), and tank-to-tank intervals in combat formations. S3 gives only battalion-level column/echelon/line and no intervals.
- **Tank types.** S3 names no tank model. Tanks are described only as "medium" or "heavy", plus the assault-gun tanks with 105-mm howitzers.
- **Organization charts illegible.** S3 figs. 3 and 55 (HQ & service company) [S3 p.26]; [S3 p.309], S4 figs. 1–3 [S4 p.15]; [S4 p.22], and S5 figs. 1–3 (battalion charts) [S5 p.11] are unreadable OCR. No vehicle or personnel totals are available.
- **Attack frontages in metres.** S3 par. 128 gives only qualitative frontage rules. The only number is the 750–1,500 yd label in the fig. 44 schematic [S3 p.250], and whether it means sector width comes from the figure layout only. The OCR caption also reads "Figure 14" although the text cites fig. 44.
- **AAA airfield/bridge layouts.** S5 figs. 19–22 (single objective, airdrome, long narrow objective, dead-area coordination) are illegible OCR [S5 p.46–54]. There is no drawn weapon layout for an airfield or bridge, only the siting rules in the tables.
- **40-mm elevation lower limit.** The OCR reads "-- 6�" [S5 p.19], so the sign and value are uncertain and the figure is not used. The +90° upper limit and 360° traverse are legible.
- **M16/M19 counts per battalion.** These are derived (4 batteries × 8 sections), not stated directly. Divisional SP bn battery count is stated as four [S5 p.10], but nothing confirms that all firing sections in a divisional battery are SP-equipped.
- **AAA march spacing.** S5 gives no numerical distance between AA vehicles in a column, only "mutually supporting distance".
- **Tank-infantry ratios in the infantry division.** There is no numeric ratio beyond one infantry platoon riding one tank platoon in pursuit and the delaying-action example.

### Chinese and North Korean forces

- **The 1952 edition is not in the sources.** S6 is the 1960 edition. For 1950–53 the only verified CCF strength is S9's army of 3 divisions of about 8,000 (about 30,000) [S9 p.250]. There are no wartime TOEs for regiment, battalion, company, platoon or squad strengths or weapons. Every organization table here is the post-1955 TOE.
- **Squad and platoon organization.** The CCF squad and platoon, and their weapon counts, do not appear in the text.
- **Chart numbers that OCR garbled.** In figs. 22–28 and 31–32, a leading "1" (probably a chart line) makes these values ambiguous, so they were not used:
  - Army recon bn "1500", guard bn "1400", engineer bn "1500", signal bn "1300". The readings 500/400/500/300 would make the parts add up to 56,400 exactly with HQ 1,900 and 3 × 17,600, but that reading is my inference.
  - Division HQ ",920"; artillery regt "11,500"; tank-assault gun regt "1700"; AT bn "1400"; signal bn "1300"; chem co "1150"; recon co "I ISO".
  - Regimental heavy weapons bn, engineer co and AAMG battery: "1130" (the OCR attaches this value only to the engineer co and AAMG battery; the heavy weapons bn has no clear figure); guard and recon platoon "140".
  - Armored division: armored regt "11000", infantry regt "12,750", artillery regt "11,050", and most company figures.
  - Artillery division regiments "1500"; AAA division regiments "BOO 1" and "1800"; heavy AAA battery "no".
  - Public security artillery bn "1500" and battery "1160"; motor transport regiment "1750".
- **Parent-child layout of fig. 23.** Company counts per battalion, and which companies belong to the battalion and which to the regiment, cannot be recovered from the text (840 ≠ 80 + 3 × 200 + 190 + 170).
- **Tank-assault gun regiment.** Its composition and vehicle types are not given in the text.
- **Armored vehicles section (paras 105–107, pamphlet pp. 81–85).** Only figure captions survive (T-54, JSU-122). There is no text on tank types or numbers.
- **Missing pages.** PDF pp. 26, 34, 36, 76, 80, 86 and 88 have no text layer, and p.72 is fragmentary. Lost with them: parts of paras 24b–25 (armor and air support), para 36 (infiltration and special operations; only its end survives on p.35), cold-weather operations, submachine guns and carbines, and the start of armored vehicles.
- **Horizontal distances in fig. 15.** The "20 Km", "12 Km" and "3 Km" marks and the advance-zone "(15-(OO Km)" are garbled or unlabeled. The OCR for "Switch positions (4-SKm)" and "second defense zone (5-S Km)" is ambiguous (it could be 5 or 8). None of these were used.
- **Not in the sources:**
  - attack frontages for any echelon, and distances between echelons in the attack;
  - march-column spacing, vehicle intervals, and dispersal distances for bivouacs or AA positions;
  - CCF AA gun siting geometry, such as guns per site and spacing around bridges or MSRs. S9 gives totals, the 12.7-mm companies, truck-towed 37-mm guns and the decoys, but no layouts;
  - animal counts per unit (only the 82-mm one-animal load and general "animal and human portage"; S9 gives no numbers either);
  - PLAAF/CCAF units deployed in Korea and their aircraft counts; S6's "37 aircraft per regiment" is a 1960 statement. S9's MiG material was not extracted for this ground section;
  - internal organization of the NKPA tank brigade or division, rifle division, or artillery units (S9 names the formations but gives no TOE);
  - **T-34 count at the invasion**, KPA artillery pieces and truck totals on 25 June 1950 (S9 p.27 and p.41 give no numbers);
  - the internal structure of a KPA or CCF convoy, meaning vehicle intervals and trucks per convoy as a norm. S9 gives only claim-based snapshots: 117 trucks and 38 tanks at one bridge, "as many as 2,000 vehicles" a night, convoys "smaller and better dispersed" from November 1951.
- **Claims, not counts.** S9's destruction figures are USAF claims or POW estimates. S9 itself notes that night-intruder claims tracked the number of vehicles sighted and sorties flown [S9 p.481]. Use them for scale only.
- **S9 p.197 table.** The OCR scrambled the table. The ground-arm column is missing one value (only "56,270 / 143 / 112" survive against 4 labels), so only the aircraft column is used.
- **Yalu crossing date conflict.** S6 p.14 says CCF forces crossed "in late November 1950". S9 p.250, citing captured CCF records, has crossings starting 14 October and 4 armies across by 26 October. S10 p.23 has UN troops "confronted by Chinese troops" on 3 November. S9 and S10 agree with each other, so S6's wording is probably loose; treat October 1950 as the entry date.

### Naval

- **Scope of S7:** S7 is only Vol. I, the 50-minute summary. The 53 project studies (Vols. II–X), which include fast carrier TF, support carrier TF, blockade, convoy escort, naval bombardment and mine countermeasures, and the 37 command narratives (Vols. XI–XVI), which include CTF 77, COMCARDIV 15, CTF 90, CTG 95.x and COMCRUDIV-3, were **not available**. Ship-by-ship orders of battle, screen compositions and patrol patterns are in those volumes and are missing here.
- **TF 77 carrier count, Jun–Dec 1950:** the upper bound in S7 p.22 is lost to OCR ("1 to CV-9 class carriers"). S9 gives 1 carrier (*Valley Forge*, Jun), then 2 (from 31 Jul), then 3 (Sep, with *Boxer*).
- **Illegible numbers in S7 p.23:** the number of mines shipped through Wonsan ("Some ,OOO"), the day in Sep 1950 of the first mine find off Chinnampo, and the mine-casualty count (OCR "IO", read as 10 but not certain).
- **Sea-lift percentage:** S7 p.16 says sea power carried "approximately DC percent" of the tonnage. The number is illegible.
- **Blockade:** no blockade patrol stations, sectors, rotation or ship counts appear in any held source. There is only the authorization (S9 p.59), the S7 annex title "Blockading and Escort Force", and task-group numbers (TG 95.2, 95.9, 96.5). The meaning of those numbers is not explained.
- **Screen and surface groups:** there are no destroyer counts per screen, no named battleships or cruisers for TF 77 (only "one cruiser and normally one battleship"), and no frigate, LST or minesweeper numbers.
- **UN navies by nationality:** only British ships are named (HMS *Triumph*, *Jamaica*, *Ocean*, and an unnamed British destroyer). No Australian, Canadian, New Zealand, Dutch, French, Colombian, Thai or ROK Navy ship lists are in the held sources.
- **Enemy small craft and torpedo boats:** none of the held sources describes North Korean torpedo boats, gunboats, patrol craft or sampan minelayers. S9 p.47 mentions only NavFE's mission "to attack and destroy all enemy vessels" south of 38°N.
- **Amphibious assault group makeup:** there are no numbers of transports, LSTs, LSMRs or fire-support ships for Inchon. For Wonsan the only figure is "250-ship armada" (S9 p.233).
- **Carrier air group composition:** squadron counts, aircraft per carrier, deck-cycle timings and sortie rates per carrier are missing. The only figures are 4 launches of 12–16 aircraft on 26 Jul 1950 and the daily totals in rows 26, 31 and 46.
- **Gunfire support stations:** there are no named station positions. The only constraint is "outside the 100-fathom curve" on the east coast.
- **S9 dates from OCR:** S9 p.282 shows "17 November" in a December context (it is taken as OCR/context noise and not used). S9 OCR renders 1 as "I" and 10 as "IO" in several places. Rows 13 and 46 read them as 10 and 11,004.

## 10. Sources

| ID | Title | Edition / date | Origin |
|---|---|---|---|
| S1 | FM 7-40, *Infantry Regiment* | Dept. of the Army, January 1950 | <https://archive.org/details/KoreanWarFieldManuals> |
| S2 | FM 7-10, *Rifle Company, Infantry Regiment* | Dept. of the Army, October 1949 | <https://archive.org/details/KoreanWarFieldManuals> |
| S3 | FM 17-33, *Tank Battalion* | Dept. of the Army, September 1949 | <https://archive.org/details/KoreanWarFieldManuals> |
| S4 | **FM 6-120, *The Field Artillery Observation Battalion and Batteries*.** archive.org mislabels this file as "FM 6-140 (The Field Artillery Battery)"; the title page reads FM 6-120. | Dept. of the Army, July 1951 | <https://archive.org/details/KoreanWarFieldManuals> |
| S5 | FM 44-2, *Antiaircraft Artillery Automatic Weapons* | Dept. of the Army, August 1950 | <https://archive.org/details/KoreanWarFieldManuals> |
| S6 | DA Pamphlet 30-51, *Handbook on the Chinese Communist Army* | Dept. of the Army, **7 December 1960** edition. It supersedes the 30 September 1952 edition (p.4). Its organization charts show the Chinese army after the 1955 reorganization, not Korean War tables of organization. | <https://upload.wikimedia.org/wikipedia/commons/7/74/DA_Pamphlet_30-51_Handbook_on_the_Chinese_Communist_Army_7_Dece.pdf> |
| S7 | *Korean War: U.S. Pacific Fleet Operations*, CINCPACFLT Interim Evaluation Report No. 1, Vol. I (Main Report) | Covers 25 June–15 November 1950 | <https://www.history.navy.mil/content/dam/nhhc/research/library/online-reading-room/war-and-conflict/korean-war/korean-war-interim-evalution/Korean-War-U.S.-Pacific-Fleet-Operations-Interim-Evaluation-Report-No-1-Volume-1%20-%2025%20June%20to%2015%20November%201950.pdf> |
| S9 | Robert F. Futrell, *The United States Air Force in Korea, 1950–1953* | Office of Air Force History, rev. ed. 1983 | <https://media.defense.gov/2010/Dec/02/2001329903/-1/-1/0/usaf_in_korea-2.pdf> |
| S10 | *The U.S. Air Force in Korea: Campaigns, Units, and Stations, 1950–1953* | Air Force History and Museums Program | <https://media.defense.gov/2025/Jun/12/2003737607/-1/-1/0/KOREA_CAMPAIGNS.PDF> |

Not available: S8, the Soviet 64th Fighter Aviation Corps report of July 1953 (Wilson Center Digital Archive; the host did not resolve). Also unavailable: the DTIC studies ADA177788, ADA440091 and ADA340611.
