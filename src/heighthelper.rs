//! heighthelper.rs — HeightHelper, the Mission Editor harvest driver
//!
//! Plays the import / set-to-ground / save key sequence against probe batches
//! from `heightprobe`, with a wall-clock budget, a timestamped log, and a
//! contiguous shard split so several machines can harvest at once. Each live
//! machine writes its own `korea_mK.hgt`. `--height-merge` joins those into
//! one `HeightStore` file the utility already reads.
//!
//! A dry run (the default) writes `batch_in.Group` and sleeps the same waits
//! the live run uses, but sends no keys and does not need the editor. The
//! default budget is 5 minutes. `--minutes 0` runs until the shard is done.
//!
//! ## Public API
//! * `run_cli` — `--height-run` and `--height-merge`
//!
//! ## Used by
//! * main.rs — before the window opens

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};

use crate::heightprobe::{self, PROBE_CHUNK};
use crate::serialize::serialize_group;
use crate::terrain::{self, HeightStore, STEP_M};

const KEY_GAP: Duration = Duration::from_millis(50);
const WAIT_IMPORT_DIALOG: Duration = Duration::from_millis(500);
const WAIT_LOAD: Duration = Duration::from_millis(3_500);
/// Live select: do not open the File menu before this. A fast select that
/// never shows as busy still waits this long; a longer select keeps waiting
/// until the UI thread has gone busy and answers again.
const SELECT_MIN: Duration = Duration::from_millis(1_000);
/// Pause after each Ctrl+A. Three presses, then set-to-ground.
const WAIT_SELECT: Duration = Duration::from_millis(500);
/// Import is not finished before this. A 50ms "load" means Ctrl+A ran
/// before the probes were in the scene.
const LOAD_MIN: Duration = Duration::from_millis(500);
/// After clicking the editor view, before Ctrl+A. Mission Properties stays
/// open for the whole session and would otherwise take the chord.
const WAIT_VIEW: Duration = Duration::from_millis(200);
/// After set-to-ground. A live run returns sooner if the UI thread goes
/// busy and then answers. If it never looks busy, continue once this elapses.
const WAIT_SNAP: Duration = Duration::from_millis(1_000);
const WAIT_SAVE_DIALOG: Duration = Duration::from_millis(500);
const WAIT_WRITE: Duration = Duration::from_millis(1_500);
const WAIT_CLEAR: Duration = Duration::from_millis(500);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(45);

const HELP: &str = r#"HeightHelper — harvest Korea terrain heights from the Mission Editor.

  --height-run [options]
  --height-merge --out <korea_100m.hgt> <korea_m0.hgt> <korea_m1.hgt> ...

--height-run options:
  --minutes N     stop after N minutes (default 5). 0 runs the whole shard
  --shard K/N     this machine's share of the batches (default 0/4)
  --pass survey   800 m survey, sea included (default)
  --pass land     100 m land lattice (the long harvest)
  --live          send keys to the Mission Editor and write korea_mK.hgt
  --work DIR      working folder (default .\HeightHelper)
  --window TEXT   editor window title must contain this (default "Mission Editor")
  --max-batches N play at most N batches (after the shard split)
  --chunk N       probes per file (default 256). 0 means one file for this shard
  --limit-test    one live batch that times the editor instead of sleeping

Dry run is the default: it writes batch_in.Group, sleeps the planned waits,
and logs a timestamp on every step. It does not touch the keyboard.
A live run pastes the import and export filenames into the editor dialogs.
The import file is import\batch_in.Group. Set to ground continues after about 1 second if the editor never looks busy.

Four machines, five-minute rehearsal (survey pass):
  --height-run --shard 0/4
  --height-run --shard 1/4
  --height-run --shard 2/4
  --height-run --shard 3/4

Live harvest on machine K (empty Korea mission, editor in front):
  --height-run --live --minutes 0 --shard K/4

Join the four stores (copy the korea_mK.hgt files onto one machine first):
  --height-merge --out korea_100m.hgt korea_m0.hgt korea_m1.hgt korea_m2.hgt korea_m3.hgt

Time one import (editor in front, empty mission). The default chunk is 256
probes, the same count the utility writes. On the survey pass (the default)
those 256 probes are one east-west line: a single X, Z spanning 204 km,
800 m apart. That is not a filled patch.
  --height-run --limit-test --window "IL2 Series Editor"

A utility land-tile file is also 256 probes, at 100 m, inside one 224-node
tile: about 22.3 km west-east and 100 m north-south.

--chunk 61000 is neither file. It is about 14.5 MB and 61 000 survey probes,
about 78 km by 499 km. --chunk 0 is one file for the whole shard, larger still.
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Survey,
    Land,
}

impl Pass {
    fn name(self) -> &'static str {
        match self {
            Pass::Survey => heightprobe::SURVEY_NAME,
            Pass::Land => "HG100_LAND",
        }
    }
}

#[derive(Clone, Debug)]
struct Batch {
    /// Index in the full pass, shared by every machine.
    id: usize,
    /// First node of this batch in the pass.
    start: usize,
    count: usize,
    sector: String,
}

#[derive(Clone, Debug)]
enum KeyName {
    Letter(char),
    Right,
    Down,
    Enter,
    /// Scene Delete. English VK_DELETE (0x2E), never backspace (0x08).
    Delete,
}

#[derive(Clone, Debug)]
enum KeyKind {
    Chord { alt: bool, ctrl: bool, key: KeyName },
    Tap(KeyName),
    /// Put `text` on the clipboard and paste it with Ctrl+V.
    Text(String),
}

#[derive(Clone, Copy, Debug)]
enum SettleKind {
    /// File dialog is open and the filename can be typed.
    Dialog,
    /// Import dialog has closed and the editor is accepting input.
    Import,
    /// Select-all has finished and the editor is accepting input.
    Select,
    /// Set-to-ground has finished and the editor is accepting input.
    Snap,
}

impl SettleKind {
    fn name(self) -> &'static str {
        match self {
            SettleKind::Dialog => "import dialog",
            SettleKind::Import => "editor load",
            SettleKind::Select => "selection",
            SettleKind::Snap => "set to ground",
        }
    }
}

#[derive(Clone, Debug)]
enum ActBody {
    Key(KeyKind),
    Wait,
    DeleteExport,
    Poll { timeout: Duration },
    /// Dry run sleeps `Act::gap`. A live run waits until the editor reaches `kind`.
    Settle { kind: SettleKind, timeout: Duration },
    /// Click the editor client area so the following keys reach the view.
    FocusView,
}

#[derive(Clone, Debug)]
struct Act {
    label: String,
    gap: Duration,
    body: ActBody,
}

struct PlayCfg {
    live: bool,
    budget: Option<Duration>,
    /// Probes in the whole pass, for the live timing extrapolation. 0 skips it.
    pass_probes: usize,
    machines: usize,
}

#[derive(Debug)]
struct PlayReport {
    played: usize,
    stop: Stop,
}

#[derive(Debug)]
enum Stop {
    Budget,
    Finished,
}

trait Host {
    fn elapsed(&mut self) -> Duration;
    fn sleep(&mut self, d: Duration);
    fn log(&mut self, msg: &str);
    fn note(&mut self, played: usize, batch: &Batch);
    fn mark(&mut self, played: usize);
    fn prepare(&mut self, batch: &Batch) -> Result<(), String>;
    fn send_key(&mut self, kind: &KeyKind) -> Result<(), String>;
    fn focus_view(&mut self) -> Result<(), String>;
    fn delete_export(&mut self) -> Result<(), String>;
    fn poll_export(&mut self, timeout: Duration) -> Result<(), String>;
    fn settle(&mut self, kind: SettleKind, timeout: Duration) -> Result<Duration, String>;
    fn finish(&mut self, batch: &Batch) -> Result<(), String>;
}

/// Inclusive-exclusive part range for one machine. The ranges of `0..machines`
/// cover `0..total` once each.
fn shard_bounds(total: usize, machines: usize, shard: usize) -> (usize, usize) {
    let start = shard.saturating_mul(total) / machines;
    let end = (shard + 1).saturating_mul(total) / machines;
    (start, end.min(total))
}

fn sector_of(nodes: &[(usize, usize)]) -> String {
    let (mut i0, mut i1, mut j0, mut j1) = (usize::MAX, 0usize, usize::MAX, 0usize);
    for &(i, j) in nodes {
        i0 = i0.min(i);
        i1 = i1.max(i);
        j0 = j0.min(j);
        j1 = j1.max(j);
    }
    let m = |n: usize| (n as f64 * STEP_M).round() as i64;
    format!("X {}-{} m, Z {}-{} m", m(i0), m(i1), m(j0), m(j1))
}

/// `chunk` 0 puts this shard's nodes in a single file.
fn batches_from(
    nodes: &[(usize, usize)],
    machines: usize,
    shard: usize,
    chunk: usize,
) -> Vec<Batch> {
    if chunk == 0 {
        let (lo, hi) = shard_bounds(nodes.len(), machines, shard);
        if lo >= hi {
            return Vec::new();
        }
        let slice = &nodes[lo..hi];
        return vec![Batch {
            id: shard,
            start: lo,
            count: slice.len(),
            sector: sector_of(slice),
        }];
    }
    let parts = nodes.len().div_ceil(chunk);
    let (start, end) = shard_bounds(parts, machines, shard);
    (start..end)
        .filter_map(|part| {
            let lo = part * chunk;
            let hi = (lo + chunk).min(nodes.len());
            if lo >= hi {
                return None;
            }
            let slice = &nodes[lo..hi];
            Some(Batch {
                id: part,
                start: lo,
                count: slice.len(),
                sector: sector_of(slice),
            })
        })
        .collect()
}

fn script(import: &str, export: &str) -> Vec<Act> {
    let press = |label: &str, kind: KeyKind| Act {
        label: label.to_string(),
        gap: KEY_GAP,
        body: ActBody::Key(kind),
    };
    let chord = |label: &str, alt: bool, ctrl: bool, name: KeyName| {
        press(
            label,
            KeyKind::Chord {
                alt,
                ctrl,
                key: name,
            },
        )
    };
    let tap = |label: &str, key_name: KeyName| press(label, KeyKind::Tap(key_name));
    let wait = |dur: Duration, why: &str| Act {
        label: format!("wait {}ms: {why}", dur.as_millis()),
        gap: dur,
        body: ActBody::Wait,
    };
    let settle = |why: &str, dry: Duration, kind: SettleKind, timeout: Duration| Act {
        label: format!("wait until {why}"),
        gap: dry,
        body: ActBody::Settle { kind, timeout },
    };
    let mut acts = Vec::new();
    // File -> Import, paste the full path, Open.
    acts.push(chord("Alt+F", true, false, KeyName::Letter('f')));
    acts.push(tap("I", KeyName::Letter('i')));
    acts.push(wait(WAIT_IMPORT_DIALOG, "import dialog"));
    acts.push(press(
        &format!("paste import {import}"),
        KeyKind::Text(import.to_string()),
    ));
    acts.push(chord("Alt+O", true, false, KeyName::Letter('o')));
    acts.push(settle(
        "editor load",
        WAIT_LOAD,
        SettleKind::Import,
        Duration::from_secs(120),
    ));
    // Mission Properties stays open and would take Ctrl+A. Click the view,
    // then Ctrl+A three times. The File menu stays closed until the third pause.
    acts.push(Act {
        label: "click editor view".to_string(),
        gap: WAIT_VIEW,
        body: ActBody::FocusView,
    });
    for _ in 0..3 {
        acts.push(chord("Ctrl+A", false, true, KeyName::Letter('a')));
        acts.push(wait(WAIT_SELECT, "selection"));
    }
    acts.push(chord("Alt+F", true, false, KeyName::Letter('f')));
    for _ in 0..4 {
        acts.push(tap("Right", KeyName::Right));
    }
    for _ in 0..6 {
        acts.push(tap("Down", KeyName::Down));
    }
    acts.push(tap("Enter", KeyName::Enter));
    acts.push(settle(
        "set to ground",
        WAIT_SNAP,
        SettleKind::Snap,
        Duration::from_secs(600),
    ));
    // File -> Save selection to file, paste the full path, Save.
    acts.push(chord("Alt+F", true, false, KeyName::Letter('f')));
    for _ in 0..3 {
        acts.push(tap("S", KeyName::Letter('s')));
    }
    acts.push(tap("Enter", KeyName::Enter));
    acts.push(wait(WAIT_SAVE_DIALOG, "save dialog"));
    acts.push(Act {
        label: "delete previous batch_out.Group".to_string(),
        gap: Duration::ZERO,
        body: ActBody::DeleteExport,
    });
    acts.push(press(
        &format!("paste export {export}"),
        KeyKind::Text(export.to_string()),
    ));
    acts.push(chord("Alt+S", true, false, KeyName::Letter('s')));
    acts.push(Act {
        label: format!("wait {}ms: disk write", WAIT_WRITE.as_millis()),
        gap: WAIT_WRITE,
        body: ActBody::Poll {
            timeout: EXPORT_TIMEOUT,
        },
    });
    // The save dialog still has focus, and this click clears the selection.
    // One Ctrl+A restores it. Do not click again before Delete.
    acts.push(Act {
        label: "click editor view".to_string(),
        gap: WAIT_VIEW,
        body: ActBody::FocusView,
    });
    acts.push(chord("Ctrl+A", false, true, KeyName::Letter('a')));
    acts.push(wait(WAIT_VIEW, "reselect"));
    acts.push(tap("Delete", KeyName::Delete));
    acts.push(wait(WAIT_CLEAR, "scene clear"));
    acts
}

/// Planned sleeps for one batch, including the inter-key gap.
fn script_span() -> Duration {
    script("in", "out")
        .iter()
        .fold(Duration::ZERO, |acc, a| acc + a.gap)
}

/// How many batches a dry run finishes before `budget`. Matches `play`.
fn batches_in_budget(available: usize, budget: Option<Duration>) -> usize {
    let Some(budget) = budget else {
        return available;
    };
    let span = script_span();
    if span.is_zero() {
        return available;
    }
    let mut n = 0;
    let mut elapsed = Duration::ZERO;
    while n < available && elapsed < budget {
        n += 1;
        elapsed += span;
    }
    n
}

fn fail_step(host: &mut dyn Host, batch_id: usize, err: String) -> String {
    host.log(&format!("FAILED batch {batch_id}  {err}"));
    format!("batch {batch_id}: {err}")
}

fn play(
    batches: &[Batch],
    import: &str,
    export: &str,
    cfg: &PlayCfg,
    host: &mut dyn Host,
) -> Result<PlayReport, String> {
    let mut played = 0usize;
    for batch in batches {
        if let Some(budget) = cfg.budget {
            if host.elapsed() >= budget {
                host.log(&format!(
                    "time budget {:.0}s reached after {played} batches",
                    budget.as_secs_f64()
                ));
                return Ok(PlayReport {
                    played,
                    stop: Stop::Budget,
                });
            }
        }
        host.note(played, batch);
        host.log(&format!(
            "batch {}  {}  {} probes  write batch_in.Group",
            batch.id, batch.sector, batch.count
        ));
        if let Err(e) = host.prepare(batch) {
            return Err(fail_step(host, batch.id, e));
        }
        let mut dialog_t = None;
        let mut load_t = None;
        let mut snap_t = None;
        for act in script(import, export) {
            host.log(&format!("batch {}  {}", batch.id, act.label));
            match &act.body {
                ActBody::Key(kind) => {
                    if cfg.live {
                        if let Err(e) = host.send_key(kind) {
                            return Err(fail_step(host, batch.id, e));
                        }
                    }
                    host.sleep(act.gap);
                }
                ActBody::FocusView => {
                    if cfg.live {
                        if let Err(e) = host.focus_view() {
                            return Err(fail_step(host, batch.id, e));
                        }
                    }
                    host.sleep(act.gap);
                }
                ActBody::Wait => host.sleep(act.gap),
                ActBody::DeleteExport => {
                    if let Err(e) = host.delete_export() {
                        return Err(fail_step(host, batch.id, e));
                    }
                }
                ActBody::Poll { timeout } => {
                    if cfg.live {
                        if let Err(e) = host.poll_export(*timeout) {
                            return Err(fail_step(host, batch.id, e));
                        }
                    } else {
                        host.log(&format!(
                            "batch {}  dry-run: editor export skipped",
                            batch.id
                        ));
                        host.sleep(act.gap);
                    }
                }
                ActBody::Settle { kind, timeout } => {
                    if cfg.live {
                        let took = match host.settle(*kind, *timeout) {
                            Ok(d) => d,
                            Err(e) => return Err(fail_step(host, batch.id, e)),
                        };
                        host.log(&format!(
                            "batch {}  {} took {:.3}s",
                            batch.id,
                            kind.name(),
                            took.as_secs_f64()
                        ));
                        match kind {
                            SettleKind::Dialog => dialog_t = Some(took),
                            SettleKind::Import => load_t = Some(took),
                            SettleKind::Select => {}
                            SettleKind::Snap => snap_t = Some(took),
                        }
                    } else {
                        host.log(&format!(
                            "batch {}  dry-run: planned wait, editor not timed",
                            batch.id
                        ));
                        host.sleep(act.gap);
                    }
                }
            }
        }
        if cfg.live {
            if let (Some(dialog), Some(load), Some(snap)) = (dialog_t, load_t, snap_t) {
                let measured = dialog.as_secs_f64() + load.as_secs_f64() + snap.as_secs_f64();
                host.log(&format!(
                    "LIMIT batch {}  {} probes  dialog {:.3}s  load {:.3}s  snap {:.3}s",
                    batch.id,
                    batch.count,
                    dialog.as_secs_f64(),
                    load.as_secs_f64(),
                    snap.as_secs_f64()
                ));
                if cfg.pass_probes > 0 && batch.count > 0 {
                    let batches = cfg.pass_probes.div_ceil(batch.count);
                    let hours = measured * batches as f64 / 3600.0;
                    let parallel = hours / cfg.machines.max(1) as f64;
                    host.log(&format!(
                        "LIMIT if this scales  {} probes/file  {} files  {:.2} h one machine  {:.2} h on {} machines",
                        batch.count, batches, hours, parallel, cfg.machines
                    ));
                }
            }
        }
        if cfg.live {
            if let Err(e) = host.finish(batch) {
                return Err(fail_step(host, batch.id, e));
            }
        }
        played += 1;
        host.mark(played);
    }
    host.log(&format!("shard finished, {played} batches"));
    Ok(PlayReport {
        played,
        stop: Stop::Finished,
    })
}

fn wall_local() -> String {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLocalTime(st: *mut SystemTime);
        }
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            dow: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            ms: u16,
        }
        let mut st = SystemTime {
            year: 0,
            month: 0,
            dow: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            ms: 0,
        };
        unsafe { GetLocalTime(&mut st) };
        let _ = st.dow;
        return format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            st.year, st.month, st.day, st.hour, st.minute, st.second, st.ms
        );
    }
    #[cfg(not(windows))]
    {
        let dur = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = dur.as_secs();
        let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
        format!("utc {:02}:{:02}:{:02}.{:03}", h, m, s, dur.subsec_millis())
    }
}

fn read_done(path: &Path) -> Result<Vec<usize>, String> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut ids = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
        let Some(word) = line.split_whitespace().next() else {
            continue;
        };
        let id: usize = word
            .parse()
            .map_err(|_| format!("{}: bad batch id {word}", path.display()))?;
        ids.push(id);
    }
    Ok(ids)
}

fn append_done(path: &Path, id: usize) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    writeln!(file, "{id}").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

struct RealHost {
    live: bool,
    start: Instant,
    work: PathBuf,
    import_path: PathBuf,
    export_path: PathBuf,
    pass_name: String,
    nodes: Vec<(usize, usize)>,
    editor_hwnd: isize,
    window: String,
    logf: File,
    bar: ProgressBar,
    store: HeightStore,
    store_path: PathBuf,
    done_path: PathBuf,
}

impl RealHost {
    fn write_log(&mut self, msg: &str) {
        let line = format!(
            "{}  +{:.3}s  {msg}",
            wall_local(),
            self.start.elapsed().as_secs_f64()
        );
        let _ = writeln!(self.logf, "{line}");
        let _ = self.logf.flush();
        self.bar.println(line);
    }
}

impl Host for RealHost {
    fn elapsed(&mut self) -> Duration {
        self.start.elapsed()
    }
    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
    fn log(&mut self, msg: &str) {
        self.write_log(msg);
    }
    fn note(&mut self, played: usize, batch: &Batch) {
        self.bar.set_position(played as u64);
        self.bar
            .set_message(format!("#{}  {}", batch.id, batch.sector));
    }
    fn mark(&mut self, played: usize) {
        self.bar.set_position(played as u64);
    }
    fn prepare(&mut self, batch: &Batch) -> Result<(), String> {
        if self.live {
            let (title, hwnd) = focus_editor(&self.window)?;
            self.editor_hwnd = hwnd;
            self.write_log(&format!("batch {}  foreground \"{title}\"", batch.id));
        }
        let end = batch
            .start
            .checked_add(batch.count)
            .filter(|end| *end <= self.nodes.len())
            .ok_or_else(|| format!("batch {} is outside the probe list", batch.id))?;
        let group = heightprobe::probe_group(
            &format!("{}-{:05}", self.pass_name, batch.id),
            &self.nodes[batch.start..end],
        );
        let text = serialize_group(&group);
        if let Some(dir) = self.import_path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::write(&self.import_path, &text)
            .map_err(|e| format!("{}: {e}", self.import_path.display()))?;
        let keep = self.work.join("in");
        fs::create_dir_all(&keep).map_err(|e| format!("{}: {e}", keep.display()))?;
        fs::write(keep.join(format!("b{:05}.Group", batch.id)), text)
            .map_err(|e| format!("{}: {e}", keep.display()))?;
        self.delete_export()?;
        Ok(())
    }
    fn send_key(&mut self, kind: &KeyKind) -> Result<(), String> {
        if !self.live {
            return Err("keys were requested on a dry run".into());
        }
        dispatch_key(kind)
    }
    fn focus_view(&mut self) -> Result<(), String> {
        if self.editor_hwnd == 0 {
            return Err("the Mission Editor window is not focused".into());
        }
        let (x, y) = click_editor_view(self.editor_hwnd)?;
        self.write_log(&format!("editor view click at {x},{y}"));
        Ok(())
    }
    fn delete_export(&mut self) -> Result<(), String> {
        match fs::remove_file(&self.export_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", self.export_path.display())),
        }
    }
    fn settle(&mut self, kind: SettleKind, timeout: Duration) -> Result<Duration, String> {
        if self.editor_hwnd == 0 {
            return Err("the Mission Editor window is not focused".into());
        }
        wait_for_editor(self.editor_hwnd, kind, timeout)
    }
    fn poll_export(&mut self, timeout: Duration) -> Result<(), String> {
        let started = Instant::now();
        loop {
            if export_stable(&self.export_path) {
                self.write_log(&format!(
                    "batch_out.Group ready after {:.1}s ({})",
                    started.elapsed().as_secs_f64(),
                    self.export_path.display()
                ));
                return Ok(());
            }
            if started.elapsed() >= timeout {
                return Err(format!(
                    "batch_out.Group was not written within {:.0}s ({})",
                    timeout.as_secs_f64(),
                    self.export_path.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    fn finish(&mut self, batch: &Batch) -> Result<(), String> {
        let text = fs::read_to_string(&self.export_path)
            .map_err(|e| format!("{}: {e}", self.export_path.display()))?;
        let snapped = self.work.join("snapped");
        fs::create_dir_all(&snapped).map_err(|e| format!("{}: {e}", snapped.display()))?;
        let copy = snapped.join(format!("b{:05}.Group", batch.id));
        fs::write(&copy, &text).map_err(|e| format!("{}: {e}", copy.display()))?;
        let root = crate::parser::parse_il2_document(&text)
            .map_err(|e| format!("{}: {e}", copy.display()))?;
        let rep = heightprobe::ingest(&root, &mut self.store, false);
        if !rep.merged {
            let rejected = self.work.join("rejected");
            fs::create_dir_all(&rejected).map_err(|e| format!("{}: {e}", rejected.display()))?;
            let dest = rejected.join(format!("b{:05}.Group", batch.id));
            let _ = fs::rename(&copy, &dest);
            return Err(format!(
                "not merged ({} above 0 m, {} water, {} unsnapped). Left at {}",
                rep.above_zero,
                rep.water,
                rep.unsnapped,
                dest.display()
            ));
        }
        self.store.save(&self.store_path)?;
        append_done(&self.done_path, batch.id)?;
        self.write_log(&format!(
            "batch {} stored  {} heights, {} water, {} unsnapped  -> {}",
            batch.id,
            rep.lattice,
            rep.water,
            rep.unsnapped,
            self.store_path.display()
        ));
        Ok(())
    }
}

fn export_stable(path: &Path) -> bool {
    let Ok(first) = fs::metadata(path) else {
        return false;
    };
    if first.len() == 0 {
        return false;
    }
    std::thread::sleep(Duration::from_millis(200));
    fs::metadata(path).is_ok_and(|m| m.len() == first.len() && m.len() > 0)
}

const VK_CONTROL: u8 = 0x11;
const VK_MENU: u8 = 0x12;
/// English Delete. Backspace is 0x08 and does not clear the scene.
const VK_DELETE: u8 = 0x2E;
const VK_V: u8 = 0x56;
const KEYEVENTF_KEYUP: u32 = 0x0002;

fn keybd(vk: u8, up: bool) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    }
    unsafe {
        keybd_event(vk, 0, if up { KEYEVENTF_KEYUP } else { 0 }, 0);
    }
}

fn tap_vk(vk: u8) {
    keybd(vk, false);
    std::thread::sleep(Duration::from_millis(15));
    keybd(vk, true);
}

fn vk_for(name: &KeyName) -> Result<u8, String> {
    match name {
        KeyName::Letter(c) => {
            let upper = c.to_ascii_uppercase();
            if upper.is_ascii_uppercase() {
                Ok(upper as u8)
            } else {
                Err(format!("no English letter key for {c:?}"))
            }
        }
        KeyName::Right => Ok(0x27),
        KeyName::Down => Ok(0x28),
        KeyName::Enter => Ok(0x0D),
        KeyName::Delete => Ok(VK_DELETE),
    }
}

fn chord(alt: bool, ctrl: bool, vk: u8) {
    if alt {
        keybd(VK_MENU, false);
    }
    if ctrl {
        keybd(VK_CONTROL, false);
    }
    tap_vk(vk);
    if ctrl {
        keybd(VK_CONTROL, true);
    }
    if alt {
        keybd(VK_MENU, true);
    }
}

/// Copy `text` and paste it with Control held, V tapped, Control released.
fn paste_text(text: &str) -> Result<(), String> {
    set_clipboard(text)?;
    chord(false, true, VK_V);
    Ok(())
}

fn set_clipboard(text: &str) -> Result<(), String> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(hwnd: isize) -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, mem: isize) -> isize;
        fn CloseClipboard() -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalLock(mem: isize) -> *mut u16;
        fn GlobalUnlock(mem: isize) -> i32;
        fn GlobalFree(mem: isize) -> isize;
    }
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;

    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len() * std::mem::size_of::<u16>();

    let mut opened = false;
    for _ in 0..8 {
        if unsafe { OpenClipboard(0) } != 0 {
            opened = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if !opened {
        return Err("could not open the clipboard".to_string());
    }

    let result = unsafe {
        EmptyClipboard();
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if mem == 0 {
            Err("could not allocate clipboard memory".to_string())
        } else {
            let ptr = GlobalLock(mem);
            if ptr.is_null() {
                GlobalFree(mem);
                Err("could not lock clipboard memory".to_string())
            } else {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                GlobalUnlock(mem);
                if SetClipboardData(CF_UNICODETEXT, mem) == 0 {
                    GlobalFree(mem);
                    Err("could not put the path on the clipboard".to_string())
                } else {
                    Ok(())
                }
            }
        }
    };
    unsafe {
        CloseClipboard();
    }
    result
}

fn dispatch_key(kind: &KeyKind) -> Result<(), String> {
    match kind {
        KeyKind::Chord { alt, ctrl, key } => chord(*alt, *ctrl, vk_for(key)?),
        KeyKind::Tap(name) => tap_vk(vk_for(name)?),
        KeyKind::Text(text) => paste_text(text)?,
    }
    Ok(())
}

fn focus_editor(needle: &str) -> Result<(String, isize), String> {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn EnumWindows(cb: extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
            fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
            fn IsWindowVisible(hwnd: isize) -> i32;
            fn SetForegroundWindow(hwnd: isize) -> i32;
            fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
            fn GetForegroundWindow() -> isize;
            fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
            fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
            fn AttachThreadInput(from: u32, to: u32, attach: i32) -> i32;
            fn BringWindowToTop(hwnd: isize) -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThreadId() -> u32;
        }
        struct Find {
            needle: String,
            hwnd: isize,
            title: String,
        }
        extern "system" fn enum_cb(hwnd: isize, lp: isize) -> i32 {
            let find = unsafe { &mut *(lp as *mut Find) };
            unsafe {
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                let mut buf = [0u16; 512];
                let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                if n <= 0 {
                    return 1;
                }
                let title = OsString::from_wide(&buf[..n as usize])
                    .to_string_lossy()
                    .into_owned();
                if title
                    .to_ascii_lowercase()
                    .contains(&find.needle.to_ascii_lowercase())
                {
                    find.hwnd = hwnd;
                    find.title = title;
                    return 0;
                }
            }
            1
        }
        let mut find = Find {
            needle: needle.to_string(),
            hwnd: 0,
            title: String::new(),
        };
        unsafe { EnumWindows(enum_cb, &mut find as *mut Find as isize) };
        if find.hwnd == 0 {
            return Err(format!(
                "no visible window title contains \"{needle}\". Open an empty Korea mission and leave the editor in front."
            ));
        }
        unsafe {
            let fg = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg, std::ptr::null_mut());
            let target_thread = GetWindowThreadProcessId(find.hwnd, std::ptr::null_mut());
            let current = GetCurrentThreadId();
            if fg_thread != 0 && fg_thread != current {
                AttachThreadInput(current, fg_thread, 1);
            }
            if target_thread != 0 && target_thread != current {
                AttachThreadInput(current, target_thread, 1);
            }
            ShowWindow(find.hwnd, 9);
            BringWindowToTop(find.hwnd);
            keybd_event(0x12, 0, 0, 0);
            SetForegroundWindow(find.hwnd);
            keybd_event(0x12, 0, 2, 0);
            if fg_thread != 0 && fg_thread != current {
                AttachThreadInput(current, fg_thread, 0);
            }
            if target_thread != 0 && target_thread != current {
                AttachThreadInput(current, target_thread, 0);
            }
        }
        let mut focused = false;
        for _ in 0..20 {
            if unsafe { GetForegroundWindow() } == find.hwnd {
                focused = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !focused {
            return Err(format!(
                "could not bring \"{}\" to the front. Click that window, then run again.",
                find.title
            ));
        }
        std::thread::sleep(Duration::from_millis(150));
        Ok((find.title, find.hwnd))
    }
    #[cfg(not(windows))]
    {
        let _ = needle;
        Err("HeightHelper drives the Mission Editor on Windows.".into())
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ScreenRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

fn point_in_rect(x: i32, y: i32, rect: &ScreenRect) -> bool {
    x >= rect.left && y >= rect.top && x < rect.right && y < rect.bottom
}

/// Client points to try, left-center first. Later points are used only when
/// an earlier one would land on Mission Properties.
fn editor_view_click_candidates(width: i32, height: i32) -> Vec<(i32, i32)> {
    if width < 64 || height < 64 {
        return Vec::new();
    }
    let margin = 16;
    let raw = [
        (width / 4, height / 2),
        (width / 8, height / 2),
        (margin, height / 2),
        (width / 4, height / 4),
        (width / 4, (height * 3) / 4),
        (width / 2, height / 2),
        ((width * 3) / 4, height / 2),
    ];
    let mut out = Vec::new();
    for (x, y) in raw {
        let point = (
            x.clamp(margin, width - margin - 1),
            y.clamp(margin, height - margin - 1),
        );
        if !out.contains(&point) {
            out.push(point);
        }
    }
    out
}

fn first_clear_point(points: &[(i32, i32)], blocked: &[ScreenRect]) -> Option<(i32, i32)> {
    points
        .iter()
        .copied()
        .find(|&(x, y)| blocked.iter().all(|rect| !point_in_rect(x, y, rect)))
}

/// Left-click the editor client area, outside Mission Properties, and return
/// the screen point. The dialog staying open is normal.
fn click_editor_view(editor: isize) -> Result<(i32, i32), String> {
    #[cfg(not(windows))]
    {
        let _ = editor;
        return Err("HeightHelper drives the Mission Editor on Windows.".into());
    }
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetClientRect(hwnd: isize, rect: *mut ScreenRect) -> i32;
            fn ClientToScreen(hwnd: isize, pt: *mut WinPoint) -> i32;
            fn GetWindowRect(hwnd: isize, rect: *mut ScreenRect) -> i32;
            fn IsWindowVisible(hwnd: isize) -> i32;
            fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
            fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
            fn EnumWindows(cb: extern "system" fn(isize, isize) -> i32, lp: isize) -> i32;
            fn EnumChildWindows(
                parent: isize,
                cb: extern "system" fn(isize, isize) -> i32,
                lp: isize,
            ) -> i32;
            fn SetForegroundWindow(hwnd: isize) -> i32;
            fn SetCursorPos(x: i32, y: i32) -> i32;
            fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
        }
        #[repr(C)]
        struct WinPoint {
            x: i32,
            y: i32,
        }
        struct MpFind {
            editor: isize,
            pid: u32,
            check_pid: bool,
            rects: Vec<ScreenRect>,
        }
        extern "system" fn enum_mp(hwnd: isize, lp: isize) -> i32 {
            let find = unsafe { &mut *(lp as *mut MpFind) };
            unsafe {
                if hwnd == find.editor || IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                if find.check_pid {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(hwnd, &mut pid);
                    if pid == 0 || pid != find.pid {
                        return 1;
                    }
                }
                let mut buf = [0u16; 512];
                let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                if n <= 0 {
                    return 1;
                }
                let title = String::from_utf16_lossy(&buf[..n as usize]);
                if !title.to_ascii_lowercase().contains("mission properties") {
                    return 1;
                }
                let mut rect = ScreenRect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                if GetWindowRect(hwnd, &mut rect) != 0 {
                    let pad = 8;
                    find.rects.push(ScreenRect {
                        left: rect.left.saturating_sub(pad),
                        top: rect.top.saturating_sub(pad),
                        right: rect.right.saturating_add(pad),
                        bottom: rect.bottom.saturating_add(pad),
                    });
                }
            }
            1
        }

        let mut client = ScreenRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { GetClientRect(editor, &mut client) } == 0 {
            return Err("could not read the editor client area".into());
        }
        let width = client.right - client.left;
        let height = client.bottom - client.top;
        let candidates = editor_view_click_candidates(width, height);
        if candidates.is_empty() {
            return Err(format!(
                "editor client area is {width}x{height}, too small to click"
            ));
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(editor, &mut pid) };
        let mut find = MpFind {
            editor,
            pid,
            check_pid: true,
            rects: Vec::new(),
        };
        unsafe {
            EnumWindows(enum_mp, &mut find as *mut MpFind as isize);
            find.check_pid = false;
            EnumChildWindows(editor, enum_mp, &mut find as *mut MpFind as isize);
        }
        let mut screen_points = Vec::with_capacity(candidates.len());
        for (x, y) in candidates {
            let mut pt = WinPoint { x, y };
            if unsafe { ClientToScreen(editor, &mut pt) } == 0 {
                return Err("could not map the editor client point to the screen".into());
            }
            screen_points.push((pt.x, pt.y));
        }
        let Some((x, y)) = first_clear_point(&screen_points, &find.rects) else {
            return Err(
                "could not find a point in the editor view outside Mission Properties".into(),
            );
        };
        unsafe { SetForegroundWindow(editor) };
        if unsafe { SetCursorPos(x, y) } == 0 {
            return Err("could not move the pointer onto the editor view".into());
        }
        std::thread::sleep(Duration::from_millis(40));
        unsafe {
            mouse_event(0x0002, 0, 0, 0, 0);
            mouse_event(0x0004, 0, 0, 0, 0);
        }
        Ok((x, y))
    }
}

/// Wait until the editor reaches `kind`, and return how long that took.
fn wait_for_editor(editor: isize, kind: SettleKind, timeout: Duration) -> Result<Duration, String> {
    #[cfg(windows)]
    {
        let started = Instant::now();
        let mut saw_dialog = false;
        let mut saw_busy = false;
        loop {
            let dialog = editor_dialog_open(editor);
            let ready = editor_responds(editor);
            let elapsed = started.elapsed();
            match kind {
                SettleKind::Dialog => {
                    if dialog {
                        return Ok(elapsed);
                    }
                    if elapsed >= timeout {
                        return Err(
                            "the import dialog did not open within 5s. The File menu shortcut may have missed."
                                .into(),
                        );
                    }
                }
                SettleKind::Import => {
                    if dialog {
                        saw_dialog = true;
                    }
                    let paced = elapsed >= LOAD_MIN;
                    if paced && saw_dialog && !dialog && ready {
                        return Ok(elapsed);
                    }
                    // The dialog can close before a poll sees it.
                    if paced && !saw_dialog && ready && !dialog {
                        return Ok(elapsed);
                    }
                    if elapsed >= timeout {
                        return Err(format!(
                            "the editor did not finish importing within {:.0}s",
                            timeout.as_secs_f64()
                        ));
                    }
                }
                SettleKind::Select => {
                    if !ready {
                        saw_busy = true;
                    }
                    let paced = elapsed >= SELECT_MIN;
                    if saw_busy && ready && paced {
                        return Ok(elapsed);
                    }
                    // Never saw the thread go busy: still wait out the minimum
                    // so a fast select is not skipped. Do not send keys here.
                    if !saw_busy && paced && ready {
                        return Ok(elapsed);
                    }
                    if elapsed >= timeout {
                        return Err(format!(
                            "select all did not finish within {:.0}s",
                            timeout.as_secs_f64()
                        ));
                    }
                }
                SettleKind::Snap => {
                    if !ready {
                        saw_busy = true;
                    }
                    if saw_busy && ready {
                        return Ok(elapsed);
                    }
                    // Never looked busy: do not treat a sub-second idle poll as
                    // a finished snap. Continue once the short pause has elapsed.
                    if !saw_busy && elapsed >= WAIT_SNAP && ready {
                        return Ok(elapsed);
                    }
                    if elapsed >= timeout {
                        return Err(format!(
                            "set to ground did not finish within {:.0}s",
                            timeout.as_secs_f64()
                        ));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (editor, kind, timeout);
        Err("HeightHelper drives the Mission Editor on Windows.".into())
    }
}

#[cfg(windows)]
fn editor_responds(hwnd: isize) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageTimeoutW(
            hwnd: isize,
            msg: u32,
            wparam: usize,
            lparam: isize,
            flags: u32,
            timeout_ms: u32,
            result: *mut usize,
        ) -> isize;
    }
    let mut result = 0usize;
    // WM_NULL, SMTO_ABORTIFHUNG. A busy editor UI thread does not answer.
    unsafe { SendMessageTimeoutW(hwnd, 0, 0, 0, 0x0002, 80, &mut result) != 0 }
}

/// A visible editor `#32770` is the import or save file dialog. Mission
/// Properties is the same class and stays open for the whole session, so a
/// title containing "Mission Properties" is not that dialog.
fn counts_as_file_dialog(class: &str, title: &str) -> bool {
    class == "#32770"
        && !title
            .to_ascii_lowercase()
            .contains("mission properties")
}

#[cfg(windows)]
fn editor_dialog_open(editor: isize) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(cb: extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetClassNameW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }
    struct Find {
        editor: isize,
        open: bool,
    }
    extern "system" fn enum_cb(hwnd: isize, lp: isize) -> i32 {
        let find = unsafe { &mut *(lp as *mut Find) };
        unsafe {
            if hwnd == find.editor || IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut editor_pid = 0u32;
            let mut pid = 0u32;
            GetWindowThreadProcessId(find.editor, &mut editor_pid);
            GetWindowThreadProcessId(hwnd, &mut pid);
            if editor_pid == 0 || editor_pid != pid {
                return 1;
            }
            let mut buf = [0u16; 32];
            let n = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if n > 0 {
                let class = String::from_utf16_lossy(&buf[..n as usize]);
                let mut title_buf = [0u16; 512];
                let tn = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
                let title = if tn > 0 {
                    String::from_utf16_lossy(&title_buf[..tn as usize])
                } else {
                    String::new()
                };
                if counts_as_file_dialog(&class, &title) {
                    find.open = true;
                    return 0;
                }
            }
        }
        1
    }
    let mut find = Find {
        editor,
        open: false,
    };
    unsafe { EnumWindows(enum_cb, &mut find as *mut Find as isize) };
    find.open
}

fn merge_files(inputs: &[PathBuf], out: &Path) -> Result<String, String> {
    if inputs.is_empty() {
        return Err("--height-merge needs at least one .hgt file".into());
    }
    for p in inputs {
        if !p.is_file() {
            return Err(format!("{} does not exist", p.display()));
        }
    }
    let mut acc = HeightStore::load(&inputs[0], terrain::KOREA_MAP_ID)?;
    let mut lines = vec![format!(
        "{}: {} measured nodes",
        inputs[0].display(),
        acc.measured_nodes()
    )];
    for p in &inputs[1..] {
        let next = HeightStore::load(p, terrain::KOREA_MAP_ID)?;
        let before = next.measured_nodes();
        let rep = acc
            .merge_from(&next)
            .map_err(|e| format!("{}: {e}", p.display()))?;
        lines.push(format!(
            "{}: {} measured nodes, added {}, matched {}",
            p.display(),
            before,
            rep.added,
            rep.same
        ));
    }
    acc.save(out)?;
    lines.push(format!(
        "{}: {} measured nodes",
        out.display(),
        acc.measured_nodes()
    ));
    Ok(lines.join("\n"))
}

struct RunOpts {
    minutes: Option<u64>,
    shard: usize,
    machines: usize,
    pass: Pass,
    live: bool,
    work: PathBuf,
    window: String,
    max_batches: Option<usize>,
    chunk: usize,
    limit_test: bool,
}

fn parse_shard(text: &str) -> Result<(usize, usize), String> {
    let Some((k, n)) = text.split_once('/') else {
        return Err(format!("--shard needs K/N, for example 0/4 (got {text})"));
    };
    let shard: usize = k
        .parse()
        .map_err(|_| format!("--shard needs K/N, for example 0/4 (got {text})"))?;
    let machines: usize = n
        .parse()
        .map_err(|_| format!("--shard needs K/N, for example 0/4 (got {text})"))?;
    if machines == 0 || shard >= machines {
        return Err(format!("--shard {text} is outside 0..{machines}"));
    }
    Ok((shard, machines))
}

fn take_val(
    flag: &str,
    rest: &mut std::iter::Peekable<std::slice::Iter<String>>,
) -> Result<String, String> {
    rest.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_run(args: &[String]) -> Result<RunOpts, String> {
    let mut opts = RunOpts {
        minutes: Some(5),
        shard: 0,
        machines: 4,
        pass: Pass::Survey,
        live: false,
        work: std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("HeightHelper"),
        window: "Mission Editor".to_string(),
        max_batches: None,
        chunk: PROBE_CHUNK,
        limit_test: false,
    };
    let mut rest = args.iter().peekable();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--help" | "-h" => return Err(HELP.to_string()),
            "--live" => opts.live = true,
            "--minutes" => {
                let v: u64 = take_val("--minutes", &mut rest)?
                    .parse()
                    .map_err(|_| "--minutes needs a number".to_string())?;
                opts.minutes = (v > 0).then_some(v);
            }
            "--shard" => {
                let (k, n) = parse_shard(&take_val("--shard", &mut rest)?)?;
                opts.shard = k;
                opts.machines = n;
            }
            "--pass" => {
                opts.pass = match take_val("--pass", &mut rest)?.as_str() {
                    "survey" => Pass::Survey,
                    "land" => Pass::Land,
                    other => return Err(format!("--pass is survey or land (got {other})")),
                };
            }
            "--work" => opts.work = PathBuf::from(take_val("--work", &mut rest)?),
            "--window" => opts.window = take_val("--window", &mut rest)?,
            "--max-batches" => {
                let v: usize = take_val("--max-batches", &mut rest)?
                    .parse()
                    .map_err(|_| "--max-batches needs a number".to_string())?;
                opts.max_batches = Some(v);
            }
            "--chunk" => {
                opts.chunk = take_val("--chunk", &mut rest)?
                    .parse()
                    .map_err(|_| "--chunk needs a probe count, or 0 for one file per machine".to_string())?;
            }
            "--limit-test" => opts.limit_test = true,
            other => return Err(format!("unknown option {other}\n\n{HELP}")),
        }
    }
    if opts.limit_test {
        opts.live = true;
        opts.minutes = None;
        if opts.max_batches.is_none() {
            opts.max_batches = Some(1);
        }
    }
    Ok(opts)
}

fn load_nodes(pass: Pass) -> Result<Vec<(usize, usize)>, String> {
    match pass {
        Pass::Survey => Ok(heightprobe::survey_nodes()),
        Pass::Land => {
            println!("HeightHelper: listing land probe nodes (this can take a minute)...");
            let mut nodes = Vec::new();
            for (ti, tj, _) in heightprobe::probe_tiles() {
                nodes.extend(heightprobe::tile_probe_nodes(ti, tj));
            }
            if nodes.is_empty() {
                return Err(
                    "no land probe nodes. assets/combined_terrain.bin is missing or empty.".into(),
                );
            }
            Ok(nodes)
        }
    }
}

fn cmd_run(args: &[String]) -> i32 {
    let opts = match parse_run(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let nodes = match load_nodes(opts.pass) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let pass_probes = nodes.len();
    let mut batches = batches_from(&nodes, opts.machines, opts.shard, opts.chunk);
    let file_chunk = if opts.chunk == 0 {
        nodes.len().div_ceil(opts.machines.max(1))
    } else {
        opts.chunk
    };
    let total_parts = nodes.len().div_ceil(file_chunk.max(1));
    if opts.live {
        let done_path = opts.work.join(format!("done_m{}.txt", opts.shard));
        match read_done(&done_path) {
            Ok(done) => {
                let before = batches.len();
                batches.retain(|b| !done.contains(&b.id));
                if before != batches.len() {
                    println!(
                        "HeightHelper: skipping {} batches already in {}",
                        before - batches.len(),
                        done_path.display()
                    );
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    if let Some(max) = opts.max_batches {
        batches.truncate(max);
    }
    let budget = opts.minutes.map(|m| Duration::from_secs(m * 60));
    let planned = batches_in_budget(batches.len(), budget);
    let span = script_span();
    let mode = if opts.live { "live" } else { "dry-run" };
    let pc = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "pc".into());
    let header = format!(
        "HeightHelper  {mode}  {pc}  shard {}/{}  pass {}  chunk {}  {} batches on this shard, about {planned} in this run\n\
         planned sleeps {:.3}s per batch (a live run times the editor instead)  budget {}\n\
         {} files in the full pass  work {}",
        opts.shard,
        opts.machines,
        opts.pass.name(),
        if opts.chunk == 0 {
            "one file per machine".to_string()
        } else {
            opts.chunk.to_string()
        },
        batches.len(),
        span.as_secs_f64(),
        opts.minutes
            .map(|m| format!("{m} min"))
            .unwrap_or_else(|| "until the shard is done".into()),
        total_parts,
        opts.work.display()
    );
    println!("{header}");
    if batches.is_empty() {
        println!("Nothing to do on this shard.");
        return 0;
    }
    if let Err(e) = fs::create_dir_all(&opts.work) {
        eprintln!("{}: {e}", opts.work.display());
        return 1;
    }
    let log_path = opts.work.join(format!("height_m{}.log", opts.shard));
    let mut logf = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {e}", log_path.display());
            return 1;
        }
    };
    let _ = writeln!(logf, "\n=== {header}");
    let _ = logf.flush();
    let plan_path = opts.work.join(format!("plan_m{}.txt", opts.shard));
    if let Err(e) = write_plan(&plan_path, &batches[..planned.min(batches.len())]) {
        eprintln!("{e}");
        return 1;
    }
    println!("log  {}", log_path.display());
    println!("plan {}", plan_path.display());

    let bar = ProgressBar::new(planned as u64);
    bar.set_style(
        ProgressStyle::with_template("{elapsed_precise} [{bar:40}] {pos}/{len} {msg}  ETA {eta}")
            .expect("progress template")
            .progress_chars("=>-"),
    );
    bar.enable_steady_tick(Duration::from_millis(200));

    let import_path = opts.work.join("import").join("batch_in.Group");
    let export_path = opts.work.join("batch_out.Group");
    let store_path = opts.work.join(format!("korea_m{}.hgt", opts.shard));
    let store = if opts.live {
        match HeightStore::load(&store_path, terrain::KOREA_MAP_ID) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    } else {
        HeightStore::new(terrain::KOREA_MAP_ID)
    };
    let mut host = RealHost {
        live: opts.live,
        start: Instant::now(),
        work: opts.work.clone(),
        import_path: import_path.clone(),
        export_path: export_path.clone(),
        pass_name: opts.pass.name().to_string(),
        nodes,
        editor_hwnd: 0,
        window: opts.window,
        logf,
        bar: bar.clone(),
        store,
        store_path: store_path.clone(),
        done_path: opts.work.join(format!("done_m{}.txt", opts.shard)),
    };
    let cfg = PlayCfg {
        live: opts.live,
        budget,
        pass_probes,
        machines: opts.machines,
    };
    let import = typed_path(&import_path);
    let export = typed_path(&export_path);
    let result = play(&batches, &import, &export, &cfg, &mut host);
    bar.finish_and_clear();
    match result {
        Ok(rep) => {
            let tail = match rep.stop {
                Stop::Budget => format!("time budget reached after {} batches", rep.played),
                Stop::Finished => format!("shard finished, {} batches", rep.played),
            };
            println!("HeightHelper: {tail}");
            println!("timestamps: {}", log_path.display());
            if opts.live {
                println!("this machine's store: {}", store_path.display());
                println!(
                    "when the other machines have stopped, copy every korea_mK.hgt together and run:\n  \
                     il2_mission_utility.exe --height-merge --out korea_100m.hgt korea_m0.hgt korea_m1.hgt korea_m2.hgt korea_m3.hgt"
                );
            } else {
                println!(
                    "dry run sent no keys. Add --live when the Mission Editor is in front on an empty Korea mission."
                );
            }
            0
        }
        Err(e) => {
            eprintln!("HeightHelper: {e}");
            eprintln!("timestamps: {}", log_path.display());
            eprintln!(
                "stopped. Fix the editor, then run the same command again. Completed live batches are skipped."
            );
            1
        }
    }
}

/// Path typed into the editor's file dialog. Absolute, without the `\\?\` prefix.
fn typed_path(path: &Path) -> String {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

fn write_plan(path: &Path, batches: &[Batch]) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    writeln!(file, "batch\tprobes\tsector").map_err(|e| format!("{}: {e}", path.display()))?;
    for b in batches {
        writeln!(file, "{}\t{}\t{}", b.id, b.count, b.sector)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn cmd_merge(args: &[String]) -> i32 {
    let mut out: Option<PathBuf> = None;
    let mut inputs = Vec::new();
    let mut rest = args.iter().peekable();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return 0;
            }
            "--out" => match rest.next() {
                Some(p) => out = Some(PathBuf::from(p)),
                None => {
                    eprintln!("--out needs a path");
                    return 1;
                }
            },
            other if other.starts_with("--") => {
                eprintln!("unknown option {other}\n\n{HELP}");
                return 1;
            }
            path => inputs.push(PathBuf::from(path)),
        }
    }
    let Some(out) = out else {
        eprintln!("--height-merge needs --out <file.hgt>\n\n{HELP}");
        return 1;
    };
    match merge_files(&inputs, &out) {
        Ok(report) => {
            println!("{report}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

pub fn run_cli(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("--height-run") => Some(cmd_run(&args[1..])),
        Some("--height-merge") => Some(cmd_merge(&args[1..])),
        Some("--height-help") => {
            println!("{HELP}");
            Some(0)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        t: Duration,
        logs: Vec<String>,
        keys: usize,
        prepared: Vec<usize>,
        finished: Vec<usize>,
        deleted: usize,
        fail_poll: bool,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                t: Duration::ZERO,
                logs: Vec::new(),
                keys: 0,
                prepared: Vec::new(),
                finished: Vec::new(),
                deleted: 0,
                fail_poll: false,
            }
        }
    }

    impl Host for Fake {
        fn elapsed(&mut self) -> Duration {
            self.t
        }
        fn sleep(&mut self, d: Duration) {
            self.t += d;
        }
        fn log(&mut self, msg: &str) {
            self.logs
                .push(format!("+{:.3} {msg}", self.t.as_secs_f64()));
        }
        fn note(&mut self, _: usize, _: &Batch) {}
        fn mark(&mut self, _: usize) {}
        fn prepare(&mut self, batch: &Batch) -> Result<(), String> {
            self.prepared.push(batch.id);
            Ok(())
        }
        fn send_key(&mut self, _: &KeyKind) -> Result<(), String> {
            self.keys += 1;
            Ok(())
        }
        fn focus_view(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn delete_export(&mut self) -> Result<(), String> {
            self.deleted += 1;
            Ok(())
        }
        fn settle(&mut self, _: SettleKind, _: Duration) -> Result<Duration, String> {
            Ok(Duration::from_millis(1))
        }
        fn poll_export(&mut self, _: Duration) -> Result<(), String> {
            if self.fail_poll {
                Err("batch_out.Group was not written".into())
            } else {
                Ok(())
            }
        }
        fn finish(&mut self, batch: &Batch) -> Result<(), String> {
            self.finished.push(batch.id);
            Ok(())
        }
    }

    fn sample(id: usize) -> Batch {
        Batch {
            id,
            start: 0,
            count: 3,
            sector: format!("sector {id}"),
        }
    }

    #[test]
    fn mission_properties_is_not_the_file_dialog() {
        assert!(!counts_as_file_dialog("#32770", "Mission Properties"));
        assert!(!counts_as_file_dialog("#32770", "mission properties"));
        assert!(!counts_as_file_dialog(
            "#32770",
            "IL-2 Mission Properties"
        ));
        assert!(counts_as_file_dialog("#32770", "Open"));
        assert!(counts_as_file_dialog("#32770", ""));
        assert!(!counts_as_file_dialog("IL2Editor", "Open"));
    }

    #[test]
    fn delete_and_paste_use_english_virtual_keys() {
        assert_eq!(vk_for(&KeyName::Delete).unwrap(), 0x2E);
        assert_ne!(vk_for(&KeyName::Delete).unwrap(), 0x08);
        assert_eq!(vk_for(&KeyName::Letter('v')).unwrap(), 0x56);
        assert_eq!(vk_for(&KeyName::Letter('a')).unwrap(), 0x41);
        assert_eq!(VK_V, 0x56);
        assert_eq!(VK_DELETE, 0x2E);
    }

    #[test]
    fn four_shards_cover_every_batch_once() {
        let total = 100;
        let mut seen = vec![0u8; total];
        for shard in 0..4 {
            let (start, end) = shard_bounds(total, 4, shard);
            assert!(start < end, "shard {shard} is empty");
            for id in start..end {
                seen[id] += 1;
            }
        }
        assert!(seen.iter().all(|&c| c == 1));
        assert_eq!(shard_bounds(100, 4, 0), (0, 25));
        assert_eq!(shard_bounds(100, 4, 3), (75, 100));
    }

    #[test]
    fn shard_batches_are_contiguous_slices() {
        let nodes: Vec<(usize, usize)> = (0..PROBE_CHUNK * 4).map(|k| (k, 1)).collect();
        let ids: Vec<Vec<usize>> = (0..4)
            .map(|s| {
                batches_from(&nodes, 4, s, PROBE_CHUNK)
                    .into_iter()
                    .map(|b| b.id)
                    .collect()
            })
            .collect();
        assert_eq!(ids, vec![vec![0], vec![1], vec![2], vec![3]]);
        assert_eq!(batches_from(&nodes, 4, 0, PROBE_CHUNK)[0].count, PROBE_CHUNK);
        assert!(batches_from(&nodes, 4, 0, PROBE_CHUNK)[0].sector.contains("X "));
        let nodes: Vec<(usize, usize)> = (0..1000).map(|k| (k, 0)).collect();
        let one = batches_from(&nodes, 4, 0, 0);
        assert_eq!(one.len(), 1);
        assert_eq!((one[0].start, one[0].count), (0, 250));
        assert_eq!(batches_from(&nodes, 4, 3, 0)[0].start, 750);
    }

    #[test]
    fn view_click_prefers_left_center_and_misses_mission_properties() {
        let points = editor_view_click_candidates(800, 600);
        assert_eq!(points[0], (200, 300));
        let on_the_right = ScreenRect {
            left: 500,
            top: 40,
            right: 780,
            bottom: 560,
        };
        assert_eq!(first_clear_point(&points, &[on_the_right]), Some((200, 300)));
        let over_left_center = ScreenRect {
            left: 180,
            top: 280,
            right: 240,
            bottom: 340,
        };
        let picked = first_clear_point(&points, &[over_left_center]).unwrap();
        assert_ne!(picked, (200, 300));
        assert!(!point_in_rect(picked.0, picked.1, &over_left_center));
        assert!(first_clear_point(&points, &[ScreenRect {
            left: -10,
            top: -10,
            right: 900,
            bottom: 700,
        }])
        .is_none());
        assert!(editor_view_click_candidates(40, 40).is_empty());
    }

    #[test]
    fn key_sequence_matches_the_editor_shortcuts() {
        let labels: Vec<_> = script(r"C:\work\batch_in.Group", r"C:\work\batch_out.Group")
            .into_iter()
            .map(|a| a.label)
            .filter(|l| !l.starts_with("wait"))
            .collect();
        assert_eq!(
            labels,
            vec![
                "Alt+F",
                "I",
                r"paste import C:\work\batch_in.Group",
                "Alt+O",
                "click editor view",
                "Ctrl+A",
                "Ctrl+A",
                "Ctrl+A",
                "Alt+F",
                "Right",
                "Right",
                "Right",
                "Right",
                "Down",
                "Down",
                "Down",
                "Down",
                "Down",
                "Down",
                "Enter",
                "Alt+F",
                "S",
                "S",
                "S",
                "Enter",
                "delete previous batch_out.Group",
                r"paste export C:\work\batch_out.Group",
                "Alt+S",
                "click editor view",
                "Ctrl+A",
                "Delete",
            ]
        );
    }

    #[test]
    fn five_minute_budget_matches_the_planned_sequence() {
        let span = script_span();
        let n = batches_in_budget(10_000, Some(Duration::from_secs(5 * 60)));
        assert!((26..=30).contains(&n), "{n} batches, span {span:?}");
        assert_eq!(batches_in_budget(10_000, Some(span * 2)), 2);
        assert_eq!(batches_in_budget(1, Some(Duration::from_secs(5 * 60))), 1);
    }

    #[test]
    fn dry_run_stamps_the_clock_and_sends_no_keys() {
        let batches: Vec<_> = (0..10).map(sample).collect();
        let mut host = Fake::new();
        let budget = script_span() * 2;
        let rep = play(
            &batches,
            "in.Group",
            "out.Group",
            &PlayCfg {
                live: false,
                budget: Some(budget),
                pass_probes: 0,
                machines: 1,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(rep.played, 2);
        assert!(matches!(rep.stop, Stop::Budget));
        assert_eq!(host.prepared, vec![0, 1]);
        assert_eq!(host.keys, 0);
        assert_eq!(host.finished, Vec::<usize>::new());
        assert!(host.deleted >= 2);
        assert_eq!(host.t, script_span() * 2);
        assert!(
            host.logs
                .iter()
                .any(|l| l.contains("batch 0") && l.contains("Alt+F"))
        );
        assert!(
            host.logs
                .iter()
                .any(|l| l.starts_with("+0.000") && l.contains("write batch_in.Group"))
        );
        assert!(host.logs.last().unwrap().contains("time budget"));
        let times: Vec<f64> = host
            .logs
            .iter()
            .filter_map(|l| l.split_whitespace().next())
            .filter_map(|w| w.trim_start_matches('+').parse().ok())
            .collect();
        assert!(
            times.windows(2).all(|w| w[1] + 1e-9 >= w[0]),
            "timestamps go backwards: {times:?}"
        );
    }

    #[test]
    fn live_stops_when_the_export_is_missing() {
        let batches: Vec<_> = (0..4).map(sample).collect();
        let mut host = Fake::new();
        host.fail_poll = true;
        let err = play(
            &batches,
            "in",
            "out",
            &PlayCfg {
                live: true,
                budget: None,
                pass_probes: 12,
                machines: 4,
            },
            &mut host,
        )
        .unwrap_err();
        assert!(err.contains("batch 0"), "{err}");
        assert!(host.logs.iter().any(|l| l.contains("FAILED batch 0")));
        assert!(host.finished.is_empty());
        assert_eq!(host.prepared, vec![0]);
        assert!(host.keys > 0);
    }

    #[test]
    fn merge_writes_one_store_and_leaves_it_unwritten_on_conflict() {
        let dir = std::env::temp_dir().join(format!("il2_heighthelper_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut a = HeightStore::new(terrain::KOREA_MAP_ID);
        let mut b = HeightStore::new(terrain::KOREA_MAP_ID);
        a.set_node(100, 200, 40.0);
        b.set_node(101, 200, 80.0);
        let pa = dir.join("korea_m0.hgt");
        let pb = dir.join("korea_m1.hgt");
        let out = dir.join("korea_100m.hgt");
        a.save(&pa).unwrap();
        b.save(&pb).unwrap();
        let report = merge_files(&[pa, pb], &out).unwrap();
        assert!(report.contains("2 measured nodes"), "{report}");
        let joined = HeightStore::load(&out, terrain::KOREA_MAP_ID).unwrap();
        assert_eq!(joined.node(100, 200), Some(40.0));
        assert_eq!(joined.node(101, 200), Some(80.0));

        let mut clash = HeightStore::new(terrain::KOREA_MAP_ID);
        clash.set_node(100, 200, 99.0);
        let pc = dir.join("korea_m2.hgt");
        clash.save(&pc).unwrap();
        let bad = dir.join("should_not_exist.hgt");
        let err = merge_files(&[out.clone(), pc], &bad).unwrap_err();
        assert!(err.contains("node (100,200)"), "{err}");
        assert!(!bad.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
