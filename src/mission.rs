//! mission.rs — a whole Korea `.mission` (weather, time, briefing, type)
//!
//! Map mode writes a `.Group`. This module wraps that group in the mission
//! file the editor opens: Options (map, season, clock, clouds, precipitation,
//! sea state, mission type) plus the language sidecar for the title and the
//! description. The description is basic HTML (`b`, `i`, `u`, `br`), which
//! is what the in-game briefing renders. The first use of a group label
//! keeps its index. A later object that points at the same index gets a
//! copy, because the editor refuses a mission where two objects share a
//! localization index. The title and description are appended after that.
//!
//! It does not parse the embedded `.Group` and it does not place units.
//! Load reads the Options header and the English sidecar only. The UI draws
//! the sky; this module describes it, writes it, and reads it back.
//!
//! ## Public API
//! * `MissionKind` / `MissionPackage` / `MissionWork` — three packages
//! * `CloudPreset` / `Precip` / `MissionScene` / `SeasonClimate`
//! * `Briefing` — formatted text and `to_html`
//! * `fn sample_slice` — west-east Korea heights
//! * `KOREA_HIGHEST_GROUND_M` — highest measured ground, for the altitude scale
//! * `fn render_mission` / `fn stamp_briefing` / `fn uniquify_locale` / `fn describe`
//! * `fn load_utility_mission` — a file this utility wrote
//! * `fn ndb_chart` — mono NDB table for airfields used in the group
//!
//! ## Used by
//! * ui/mission.rs — the Mission tab and Create Mission

use std::collections::{HashMap, HashSet};

use nom::bytes::complete::{tag, take_while, take_while1};
use nom::character::complete::{char, digit1, multispace0};
use nom::combinator::{map_res, opt, recognize};
use nom::error::{Error, ErrorKind};
use nom::sequence::preceded;
use nom::{Err, IResult, Parser};

use crate::frontlines::Season;
use crate::geo::{self, MAP_MAX, MAP_MIN};
use crate::locale::LocaleTable;
use crate::terrain::HeightStore;

const NEWLINE: &str = "\r\n";
/// Written into `LCAuthor`. Load accepts a mission only when the English
/// sidecar still has this name.
pub const UTILITY_AUTHOR: &str = "IL-2 Mission Utility";

/// Single-player, co-op, or deathmatch. The integer is what `MissionType`
/// stores. A freeflight `_gen.mission` uses a different code; an authored
/// mission uses these (co-op converters write `1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionKind {
    Single,
    Coop,
    Deathmatch,
}

impl MissionKind {
    pub const ALL: [MissionKind; 3] = [
        MissionKind::Single,
        MissionKind::Coop,
        MissionKind::Deathmatch,
    ];

    pub fn index(self) -> usize {
        match self {
            MissionKind::Single => 0,
            MissionKind::Coop => 1,
            MissionKind::Deathmatch => 2,
        }
    }

    pub fn code(self) -> i32 {
        self.index() as i32
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(MissionKind::Single),
            1 => Some(MissionKind::Coop),
            2 => Some(MissionKind::Deathmatch),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MissionKind::Single => "Single-player",
            MissionKind::Coop => "Co-op",
            MissionKind::Deathmatch => "Deathmatch",
        }
    }
}

/// Editor cloud configs, in the order the atmosphere list shows them.
/// The folder name is also the `CloudConfig` path under the season word.
const CLOUD_PRESETS: [&str; 50] = [
    "00_clear_00",
    "00_clear_01",
    "00_clear_02",
    "00_clear_03",
    "00_clear_04",
    "00_clear_05",
    "00_clear_06",
    "00_clear_07",
    "00_clear_08",
    "00_clear_09",
    "01_light_00",
    "01_light_01",
    "01_light_02",
    "01_light_03",
    "01_light_04",
    "01_light_05",
    "01_light_06",
    "01_light_07",
    "01_light_08",
    "01_light_09",
    "02_medium_00",
    "02_medium_01",
    "02_medium_02",
    "02_medium_03",
    "02_medium_04",
    "02_medium_05",
    "02_medium_06",
    "02_medium_07",
    "02_medium_08",
    "02_medium_09",
    "03_heavy_00",
    "03_heavy_01",
    "03_heavy_02",
    "03_heavy_03",
    "03_heavy_04",
    "03_heavy_05",
    "03_heavy_06",
    "03_heavy_07",
    "03_heavy_08",
    "03_heavy_09",
    "04_overcast_00",
    "04_overcast_01",
    "04_overcast_02",
    "04_overcast_03",
    "04_overcast_04",
    "04_overcast_05",
    "04_overcast_06",
    "04_overcast_07",
    "04_overcast_08",
    "04_overcast_09",
];

/// One entry from the editor's Clouds and Haze list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudPreset(u8);

impl CloudPreset {
    pub const COUNT: usize = 50;
    /// `01_light_00`, the preset a new package starts on.
    pub const LIGHT: CloudPreset = CloudPreset(10);

    pub fn new(index: u8) -> Self {
        Self(index.min((Self::COUNT - 1) as u8))
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn name(self) -> &'static str {
        CLOUD_PRESETS[self.index()]
    }

    /// Folder under the season word. The editor stores `01_Light_00`, not the
    /// lowercase name shown in the atmosphere list.
    pub fn config_folder(self) -> String {
        let name = self.name();
        let mut parts = name.splitn(3, '_');
        let Some(prefix) = parts.next() else {
            return name.to_string();
        };
        let Some(word) = parts.next() else {
            return name.to_string();
        };
        let Some(suffix) = parts.next() else {
            return name.to_string();
        };
        let word = match word {
            "clear" => "Clear",
            "light" => "Light",
            "medium" => "Medium",
            "heavy" => "Heavy",
            "overcast" => "Overcast",
            other => other,
        };
        format!("{prefix}_{word}_{suffix}")
    }

    /// `01_Light_00` or `01_light_00`, the folder in a `CloudConfig` path.
    pub fn from_folder(folder: &str) -> Option<Self> {
        let folder = folder.to_ascii_lowercase();
        CLOUD_PRESETS
            .iter()
            .position(|name| *name == folder)
            .map(|index| Self(index as u8))
    }

    /// Cloud base the editor filled in for this preset on the captured view.
    pub fn suggested_base_m(self) -> f32 {
        match &self.name()[..2] {
            "00" => 500.0,
            "01" => 700.0,
            "02" => 600.0,
            "03" => 800.0,
            _ => 500.0,
        }
    }

    /// `CloudHeight` written for this family. The user does not edit it.
    pub fn file_height_m(self) -> i32 {
        match &self.name()[..2] {
            "00" => 300,
            "01" => 500,
            "02" => 750,
            "03" => 900,
            _ => 400,
        }
    }
}

/// Highest measured ground on the Korea 100 m lattice, meters above the sea.
/// The node is X 457200, Z 346900.
pub const KOREA_HIGHEST_GROUND_M: f32 = 2509.0;

/// `PrecType` in the mission file.
///
/// The harvested Korea freeflight (`References/_gen.Mission`) writes
/// `PrecType = 1` and its English sidecar calls that weather "heavy rain".
/// Snow is the other value. A 2019 note from the developer listed the
/// opposite pair; this exporter follows the file the current game wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precip {
    None,
    Rain,
    Snow,
}

impl Precip {
    pub fn label(self) -> &'static str {
        match self {
            Precip::None => "None",
            Precip::Rain => "Rain",
            Precip::Snow => "Snow",
        }
    }

    pub fn code(self) -> i32 {
        match self {
            Precip::Rain | Precip::None => 1,
            Precip::Snow => 0,
        }
    }
}

/// Douglas sea state 0–6, the range the atmosphere dialog edits.
pub fn sea_state_name(state: u32) -> &'static str {
    match state {
        0 => "Calm",
        1 => "Smooth",
        2 => "Slight",
        3 => "Moderate",
        4 => "Rough",
        5 => "Very rough",
        _ => "High",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleBit {
    Bold,
    Italic,
    Underline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Mark {
    start: usize,
    end: usize,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Mark {
    fn has(&self, bit: StyleBit) -> bool {
        match bit {
            StyleBit::Bold => self.bold,
            StyleBit::Italic => self.italic,
            StyleBit::Underline => self.underline,
        }
    }

    fn set(&mut self, bit: StyleBit, on: bool) {
        match bit {
            StyleBit::Bold => self.bold = on,
            StyleBit::Italic => self.italic = on,
            StyleBit::Underline => self.underline = on,
        }
    }

    fn any(&self) -> bool {
        self.bold || self.italic || self.underline
    }

    fn same_style(&self, other: &Mark) -> bool {
        self.bold == other.bold && self.italic == other.italic && self.underline == other.underline
    }
}

/// One styled piece of a briefing, in reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BriefRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub line_break: bool,
}

/// Briefing text. The editor shows `text`; Create Mission writes [`Briefing::to_html`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Briefing {
    pub text: String,
    marks: Vec<Mark>,
}

impl Default for Briefing {
    fn default() -> Self {
        Self {
            text: "Primary objective\n\nDescribe the mission.".to_string(),
            marks: Vec::new(),
        }
    }
}

impl Briefing {
    /// Turn `bit` on for the byte range, or off when the whole range already has it.
    pub fn toggle(&mut self, start: usize, end: usize, bit: StyleBit) {
        if start >= end || end > self.text.len() {
            return;
        }
        let on = !self.range_has(start, end, bit);
        self.paint(start, end, bit, on);
    }

    /// Styled pieces in order, for the on-screen briefing. A run with
    /// `line_break` is only the break; `text` is empty.
    pub fn runs(&self) -> Vec<BriefRun> {
        let mut cuts = vec![0, self.text.len()];
        for mark in &self.marks {
            cuts.push(mark.start);
            cuts.push(mark.end);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::new();
        for pair in cuts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a >= b {
                continue;
            }
            let (bold, italic, underline) = self.style_at(a);
            let chunk = &self.text[a..b];
            let mut start = 0usize;
            for (offset, ch) in chunk.char_indices() {
                if ch == '\n' {
                    if offset > start {
                        out.push(BriefRun {
                            text: chunk[start..offset].to_string(),
                            bold,
                            italic,
                            underline,
                            line_break: false,
                        });
                    }
                    out.push(BriefRun {
                        text: String::new(),
                        bold: false,
                        italic: false,
                        underline: false,
                        line_break: true,
                    });
                    start = offset + ch.len_utf8();
                }
            }
            if start < chunk.len() {
                out.push(BriefRun {
                    text: chunk[start..].replace('\r', ""),
                    bold,
                    italic,
                    underline,
                    line_break: false,
                });
            }
        }
        out
    }

    /// The user's briefing from the description HTML. The weather preamble
    /// is not part of this text. Tags are `b`, `i`, `u`, and `br`.
    pub fn from_html(html: &str) -> Result<Self, String> {
        let mut text = String::new();
        let mut marks = Vec::new();
        let mut bold = false;
        let mut italic = false;
        let mut underline = false;
        let mut mark_at: Option<usize> = None;
        let mut rest = html;
        while !rest.is_empty() {
            if rest.starts_with('<') {
                let (tag, next) = html_tag(rest)?;
                match tag {
                    "br" => text.push('\n'),
                    "b" | "i" | "u" | "/b" | "/i" | "/u" => {
                        close_mark(&text, &mut marks, &mut mark_at, bold, italic, underline);
                        match tag {
                            "b" if !bold => bold = true,
                            "/b" if bold => bold = false,
                            "i" if !italic => italic = true,
                            "/i" if italic => italic = false,
                            "u" if !underline => underline = true,
                            "/u" if underline => underline = false,
                            _ => {
                                return Err(
                                    "The briefing HTML opens or closes a style twice.".into()
                                );
                            }
                        }
                        if bold || italic || underline {
                            mark_at = Some(text.len());
                        }
                    }
                    other => {
                        return Err(format!(
                            "The briefing has a tag this utility does not write: <{other}>."
                        ));
                    }
                }
                rest = next;
                continue;
            }
            if rest.starts_with('&') {
                let (ch, next) = html_entity(rest)?;
                text.push(ch);
                rest = next;
                continue;
            }
            let ch = rest.chars().next().expect("rest is not empty");
            text.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
        if bold || italic || underline {
            return Err("The briefing HTML leaves a style open.".into());
        }
        close_mark(&text, &mut marks, &mut mark_at, false, false, false);
        Ok(Briefing { text, marks })
    }

    pub fn to_html(&self) -> String {
        let mut cuts = vec![0, self.text.len()];
        for mark in &self.marks {
            cuts.push(mark.start);
            cuts.push(mark.end);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = String::new();
        for pair in cuts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a >= b {
                continue;
            }
            let piece = html_escape_breaks(&self.text[a..b]);
            if piece.is_empty() {
                continue;
            }
            out.push_str(&wrap_style(&piece, self.style_at(a)));
        }
        out
    }

    /// Keep marks aligned when the text edit replaces `text`.
    pub fn rebase(&mut self, new_text: &str) {
        if new_text == self.text {
            return;
        }
        let old = self.text.clone();
        let mut prefix = old
            .bytes()
            .zip(new_text.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while prefix > 0 && (!old.is_char_boundary(prefix) || !new_text.is_char_boundary(prefix)) {
            prefix -= 1;
        }
        let mut suffix = 0usize;
        let old_b = old.as_bytes();
        let new_b = new_text.as_bytes();
        while suffix < old.len() - prefix
            && suffix < new_text.len() - prefix
            && old_b[old.len() - 1 - suffix] == new_b[new_text.len() - 1 - suffix]
        {
            suffix += 1;
        }
        while suffix > 0
            && (!old.is_char_boundary(old.len() - suffix)
                || !new_text.is_char_boundary(new_text.len() - suffix))
        {
            suffix -= 1;
        }
        let old_end = old.len() - suffix;
        let new_end = new_text.len() - suffix;
        let delta = new_end as isize - old_end as isize;
        let mut kept = Vec::new();
        for mark in &self.marks {
            if mark.end <= prefix {
                kept.push(mark.clone());
            } else if mark.start >= old_end {
                let mut next = mark.clone();
                next.start = shift(mark.start, delta);
                next.end = shift(mark.end, delta);
                kept.push(next);
            } else if mark.start <= prefix && mark.end >= old_end {
                let mut next = mark.clone();
                next.end = shift(mark.end, delta);
                kept.push(next);
            } else if mark.start < prefix && mark.end <= old_end {
                let mut next = mark.clone();
                next.end = prefix;
                if next.start < next.end {
                    kept.push(next);
                }
            } else if mark.start >= prefix && mark.start < old_end && mark.end > old_end {
                let mut next = mark.clone();
                next.start = new_end;
                next.end = shift(mark.end, delta);
                if next.start < next.end {
                    kept.push(next);
                }
            }
        }
        self.text = new_text.to_string();
        self.marks = kept;
        self.merge_marks();
    }

    fn style_at(&self, at: usize) -> (bool, bool, bool) {
        self.marks
            .iter()
            .find(|mark| mark.start <= at && at < mark.end)
            .map(|mark| (mark.bold, mark.italic, mark.underline))
            .unwrap_or((false, false, false))
    }

    fn range_has(&self, start: usize, end: usize, bit: StyleBit) -> bool {
        let mut at = start;
        let mut marks: Vec<&Mark> = self
            .marks
            .iter()
            .filter(|mark| mark.end > start && mark.start < end)
            .collect();
        marks.sort_by_key(|mark| mark.start);
        for mark in marks {
            if mark.start > at || !mark.has(bit) {
                return false;
            }
            at = at.max(mark.end);
            if at >= end {
                return true;
            }
        }
        false
    }

    fn paint(&mut self, start: usize, end: usize, bit: StyleBit, on: bool) {
        let mut next = Vec::new();
        for mark in self.marks.drain(..) {
            if mark.end <= start || mark.start >= end {
                next.push(mark);
                continue;
            }
            if mark.start < start {
                next.push(Mark {
                    end: start,
                    ..mark.clone()
                });
            }
            if mark.end > end {
                next.push(Mark {
                    start: end,
                    ..mark.clone()
                });
            }
            let a = mark.start.max(start);
            let b = mark.end.min(end);
            if a >= b {
                continue;
            }
            let mut mid = Mark {
                start: a,
                end: b,
                ..mark
            };
            mid.set(bit, on);
            if mid.any() {
                next.push(mid);
            }
        }
        if on {
            let mut covered: Vec<(usize, usize)> = next
                .iter()
                .filter(|mark| mark.end > start && mark.start < end)
                .map(|mark| (mark.start.max(start), mark.end.min(end)))
                .collect();
            covered.sort_unstable();
            let mut at = start;
            let mut gaps = Vec::new();
            for (a, b) in covered {
                if a > at {
                    gaps.push((at, a));
                }
                at = at.max(b);
            }
            if at < end {
                gaps.push((at, end));
            }
            for (a, b) in gaps {
                let mut mark = Mark {
                    start: a,
                    end: b,
                    bold: false,
                    italic: false,
                    underline: false,
                };
                mark.set(bit, true);
                next.push(mark);
            }
        }
        self.marks = next;
        self.merge_marks();
    }

    fn merge_marks(&mut self) {
        self.marks
            .retain(|mark| mark.start < mark.end && mark.end <= self.text.len());
        self.marks.sort_by_key(|mark| mark.start);
        let mut merged: Vec<Mark> = Vec::new();
        for mark in self.marks.drain(..) {
            if let Some(prev) = merged.last_mut()
                && prev.end == mark.start
                && prev.same_style(&mark)
            {
                prev.end = mark.end;
            } else {
                merged.push(mark);
            }
        }
        self.marks = merged;
    }
}

fn close_mark(
    text: &str,
    marks: &mut Vec<Mark>,
    mark_at: &mut Option<usize>,
    bold: bool,
    italic: bool,
    underline: bool,
) {
    let Some(start) = mark_at.take() else {
        return;
    };
    if start < text.len() && (bold || italic || underline) {
        marks.push(Mark {
            start,
            end: text.len(),
            bold,
            italic,
            underline,
        });
    }
}

fn html_tag(input: &str) -> Result<(&str, &str), String> {
    let rest = input
        .strip_prefix('<')
        .ok_or_else(|| "The briefing HTML has an unclosed tag.".to_string())?;
    let end = rest
        .find('>')
        .ok_or_else(|| "The briefing HTML has an unclosed tag.".to_string())?;
    let raw = rest[..end].trim();
    let tag = raw.strip_suffix('/').unwrap_or(raw).trim();
    if tag.is_empty() {
        return Err("The briefing HTML has an empty tag.".into());
    }
    Ok((tag, &rest[end + 1..]))
}

fn html_entity(input: &str) -> Result<(char, &str), String> {
    let rest = input
        .strip_prefix('&')
        .ok_or_else(|| "The briefing HTML has an unfinished character code.".to_string())?;
    let end = rest
        .find(';')
        .ok_or_else(|| "The briefing HTML has an unfinished character code.".to_string())?;
    let ch = match &rest[..end] {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        other => {
            return Err(format!(
                "The briefing HTML has an unknown character code: &{other};."
            ));
        }
    };
    Ok((ch, &rest[end + 1..]))
}

fn shift(index: usize, delta: isize) -> usize {
    index.saturating_add_signed(delta)
}

fn html_escape_breaks(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\n' => out.push_str("<br>"),
            '\r' => {}
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn wrap_style(piece: &str, style: (bool, bool, bool)) -> String {
    let (bold, italic, underline) = style;
    let mut out = piece.to_string();
    if underline {
        out = format!("<u>{out}</u>");
    }
    if italic {
        out = format!("<i>{out}</i>");
    }
    if bold {
        out = format!("<b>{out}</b>");
    }
    out
}

/// One saved mission: title, clock, weather, briefing.
#[derive(Clone, Debug, PartialEq)]
pub struct MissionPackage {
    pub title: String,
    pub hour: u32,
    pub minute: u32,
    pub clouds: CloudPreset,
    /// Cloud base, meters above sea level (`CloudLevel`).
    pub cloud_base_m: f32,
    pub precip: Precip,
    /// 0–10. Zero is written when precipitation is None.
    pub prec_level: u32,
    /// 0–6.
    pub sea_state: u32,
    /// Direction the wind blows from, degrees. Upper layers veer from this.
    pub wind_from_deg: i32,
    /// Surface speed, meters per second. Upper layers add to this, up to 25.
    pub wind_speed_mps: u32,
    pub temperature_c: i32,
    /// Millimeters of mercury, the unit the atmosphere dialog uses.
    pub pressure_mmhg: i32,
    /// Set when the user edits wind, temperature, or pressure.
    /// While this is false, those three follow the Map season.
    pub climate_custom: bool,
    pub briefing: Briefing,
}

impl MissionPackage {
    pub fn for_kind(kind: MissionKind) -> Self {
        let mut package = Self {
            title: String::new(),
            hour: 8,
            minute: 0,
            clouds: CloudPreset::LIGHT,
            cloud_base_m: CloudPreset::LIGHT.suggested_base_m(),
            precip: Precip::None,
            prec_level: 0,
            sea_state: 2,
            wind_from_deg: 200,
            wind_speed_mps: 5,
            temperature_c: 24,
            pressure_mmhg: 752,
            climate_custom: false,
            briefing: Briefing::default(),
        };
        package.title = match kind {
            MissionKind::Single => "Korea".to_string(),
            MissionKind::Coop => "Korea co-op".to_string(),
            MissionKind::Deathmatch => "Korea deathmatch".to_string(),
        };
        package
    }

    pub fn clamp_weather(&mut self) {
        self.cloud_base_m = (self.cloud_base_m / 10.0).round() * 10.0;
        self.cloud_base_m = self.cloud_base_m.clamp(0.0, 8_000.0);
        self.temperature_c = self.temperature_c.clamp(-40, 45);
        self.pressure_mmhg = self.pressure_mmhg.clamp(700, 790);
        self.wind_from_deg = self.wind_from_deg.rem_euclid(360);
        self.wind_speed_mps = self.wind_speed_mps.min(25);
        self.clouds = CloudPreset::new(self.clouds.0);
    }

    /// Put wind direction, speed, temperature, and pressure back on the
    /// season, and keep following it until the user edits one of them.
    pub fn use_typical(&mut self, scene: MissionScene) {
        let climate = scene.typical();
        self.temperature_c = climate.temperature_c;
        self.pressure_mmhg = climate.pressure_mmhg;
        self.wind_from_deg = climate.wind_from_deg;
        self.wind_speed_mps = climate.wind_speed_mps;
        self.climate_custom = false;
    }

    /// Season defaults, unless the user has edited them.
    pub fn follow_season(&mut self, scene: MissionScene) {
        if !self.climate_custom {
            let climate = scene.typical();
            self.temperature_c = climate.temperature_c;
            self.pressure_mmhg = climate.pressure_mmhg;
            self.wind_from_deg = climate.wind_from_deg;
            self.wind_speed_mps = climate.wind_speed_mps;
        }
    }

    pub fn clock(&self) -> String {
        format!("{}:{}:0", self.hour.min(23), self.minute.min(59))
    }
}

/// The three packages and which one is on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct MissionWork {
    pub active: MissionKind,
    pub packages: [MissionPackage; 3],
    /// Byte selection in the briefing, remembered so Bold can use it.
    pub sel: Option<(usize, usize)>,
    pub dragging_clouds: bool,
    /// Meters between the pointer and the cloud base when a drag starts.
    pub cloud_grab_m: f32,
}

impl Default for MissionWork {
    fn default() -> Self {
        Self {
            active: MissionKind::Single,
            packages: [
                MissionPackage::for_kind(MissionKind::Single),
                MissionPackage::for_kind(MissionKind::Coop),
                MissionPackage::for_kind(MissionKind::Deathmatch),
            ],
            sel: None,
            dragging_clouds: false,
            cloud_grab_m: 0.0,
        }
    }
}

impl MissionWork {
    pub fn package(&self) -> &MissionPackage {
        &self.packages[self.active.index()]
    }

    pub fn package_mut(&mut self) -> &mut MissionPackage {
        let index = self.active.index();
        &mut self.packages[index]
    }
}

/// Season and date from the Map strip. The landscape prefix is what the
/// mission file loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionScene {
    pub season: Season,
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl MissionScene {
    pub fn prefix(self) -> &'static str {
        match self.season {
            Season::EarlySpring | Season::LateSpring => "sp",
            Season::Summer => "su",
            Season::Fall => "au",
            Season::Winter => "wi",
        }
    }

    /// Cloud-config folder. This editor stores every Korea sky under
    /// `summer\`, including a spring landscape. A March mission that loads
    /// (`MiG-15 Orbit`) uses `summer\02_Medium_03\sky.ini`. `spring\` does not.
    pub fn sky_word(self) -> &'static str {
        let _ = self;
        "summer"
    }

    pub fn date(self) -> String {
        format!("{}.{}.{}", self.day, self.month, self.year)
    }

    /// Wind, temperature, and pressure typical of this season on the Korea map.
    /// Winter is the northwest monsoon and a high. Summer is a southerly
    /// monsoon and a lower pressure.
    pub fn typical(self) -> SeasonClimate {
        match self.season {
            Season::Winter => SeasonClimate {
                temperature_c: -8,
                pressure_mmhg: 768,
                wind_from_deg: 330,
                wind_speed_mps: 8,
            },
            Season::EarlySpring => SeasonClimate {
                temperature_c: 6,
                pressure_mmhg: 762,
                wind_from_deg: 320,
                wind_speed_mps: 6,
            },
            Season::LateSpring => SeasonClimate {
                temperature_c: 14,
                pressure_mmhg: 758,
                wind_from_deg: 250,
                wind_speed_mps: 4,
            },
            Season::Summer => SeasonClimate {
                temperature_c: 24,
                pressure_mmhg: 752,
                wind_from_deg: 200,
                wind_speed_mps: 5,
            },
            Season::Fall => SeasonClimate {
                temperature_c: 10,
                pressure_mmhg: 764,
                wind_from_deg: 340,
                wind_speed_mps: 6,
            },
        }
    }
}

/// Wind from, temperature, and pressure for one season.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonClimate {
    pub temperature_c: i32,
    pub pressure_mmhg: i32,
    pub wind_from_deg: i32,
    /// Surface wind, meters per second.
    pub wind_speed_mps: u32,
}

/// Heading the wind blows toward. The file stores this; the briefing says
/// the direction it blows from.
pub fn wind_to_deg(from_deg: i32) -> i32 {
    (from_deg + 180).rem_euclid(360)
}

fn wind_layers(package: &MissionPackage) -> String {
    let base = package.wind_speed_mps.min(25);
    let to = wind_to_deg(package.wind_from_deg);
    let rows = [
        (0, to, base),
        (500, to + 10, (base + 2).min(25)),
        (1000, to + 15, (base + 4).min(25)),
        (2000, to + 25, (base + 6).min(25)),
        (5000, to + 30, (base + 8).min(25)),
    ];
    let mut out = String::from("  WindLayers");
    out.push_str(NEWLINE);
    out.push_str("  {");
    out.push_str(NEWLINE);
    for (alt, dir, speed) in rows {
        let dir = dir.rem_euclid(360);
        out.push_str(&format!("    {alt} :     {dir} :     {speed};"));
        out.push_str(NEWLINE);
    }
    out.push_str("  }");
    out.push_str(NEWLINE);
    out
}

fn precip_phrase(package: &MissionPackage) -> String {
    let level = package.prec_level.min(10);
    let word = match package.precip {
        Precip::None => return "no precipitation".to_string(),
        Precip::Rain => "rain",
        Precip::Snow => "snow",
    };
    if level == 0 {
        return "no precipitation".to_string();
    }
    let weight = if level <= 3 {
        "light "
    } else if level <= 7 {
        ""
    } else {
        "heavy "
    };
    format!("{weight}{word}")
}

/// Weather, map, season, and clock, then the user's briefing. One line, HTML.
pub fn describe(package: &MissionPackage, scene: &MissionScene) -> String {
    let prefix = scene.prefix();
    let base = package.cloud_base_m.round().max(0.0) as i32;
    let speed = package.wind_speed_mps.min(25);
    let weather = format!(
        "<u>landscape_korea_{prefix}</u> &middot; {season} &middot; {date} &middot; {time}<br><u>Weather</u>: {clouds} at <m-ft-v>{base}</m-ft-v> <m-ft-u/>, {precip}, sea state {sea}<br>Wind at ground level: From {from} deg, <ms-mph-v>{speed}</ms-mph-v> <ms-mph-u/><br>Temperature {temp} C. Pressure {pressure} mmHg.<br><br>",
        season = scene.season.label(),
        date = scene.date(),
        time = package.clock(),
        clouds = package.clouds.name(),
        precip = precip_phrase(package),
        sea = sea_state_name(package.sea_state).to_lowercase(),
        from = package.wind_from_deg.rem_euclid(360),
        temp = package.temperature_c,
        pressure = package.pressure_mmhg,
    );
    let body = package.briefing.to_html();
    if body.is_empty() {
        weather
    } else {
        format!("{weather}{body}")
    }
}

/// Description HTML plus the NDB table for airfields named in `group`.
/// The table is not part of the editable briefing.
pub fn with_ndb_chart(html: &str, group: &str) -> String {
    let chart = ndb_chart(group);
    if chart.is_empty() {
        html.to_string()
    } else {
        format!("{html}<br><br>{chart}")
    }
}

/// In-game mono table of NDB idents for airfields whose names appear in the
/// group. Ident codes are Latin. An empty string means none of the chart's
/// fields are in the group.
pub fn ndb_chart(group: &str) -> String {
    let names = group_name_keys(group);
    let rows: Vec<&Ndb> = NDB
        .iter()
        .filter(|row| names.iter().any(|name| name.contains(row.key)))
        .collect();
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "<style style=\"Style.RichText.Mono\">|Airfield          | frq | cn | cd   |ltr1|ltr2|on map|",
    );
    for row in rows {
        out.push_str("<br>");
        out.push_str(&format!(
            "|{name:<18}| {frq:<3} | {cn:<2} | {cd:<4} |{ltr1}|{ltr2}| {map} |",
            name = row.name,
            frq = row.frq,
            cn = row.cn,
            cd = row.cd,
            ltr1 = row.ltr1,
            ltr2 = row.ltr2,
            map = row.map,
        ));
    }
    out.push_str("</style>");
    out
}

/// `name` is the label in the mono row. `key` is that label with spaces and
/// punctuation removed, for matching `K-14_Kimpo_AF` and `K-47 Chuncheon`.
struct Ndb {
    key: &'static str,
    name: &'static str,
    frq: &'static str,
    cn: &'static str,
    /// Latin ident. Cyrillic pairs from the game chart are transliterated
    /// (ОЖ → OZh, Х → Kh, Я → Ya).
    cd: &'static str,
    ltr1: &'static str,
    ltr2: &'static str,
    map: &'static str,
}

/// Frequencies, channels, Morse, and map squares are the rows the game
/// stores. DPRK idents were Cyrillic there; here they are Latin.
const NDB: &[Ndb] = &[
    Ndb {
        key: "k17ongjin",
        name: "K-17_Ongjin",
        frq: "280",
        cn: "13",
        cd: "OZh",
        ltr1: "--- ",
        ltr2: "...-",
        map: "3312",
    },
    Ndb {
        key: "k19haeju",
        name: "K-19_Haeju",
        frq: "290",
        cn: "14",
        cd: "KhZh",
        ltr1: "....",
        ltr2: "...-",
        map: "3216",
    },
    Ndb {
        key: "k20sinmak",
        name: "K-20_Sinmak",
        frq: "300",
        cn: "15",
        cd: "SKh",
        ltr1: "... ",
        ltr2: "....",
        map: "2720",
    },
    Ndb {
        key: "k21pyonggang",
        name: "K-21_Pyonggang",
        frq: "310",
        cn: "16",
        cd: "PG",
        ltr1: ".--.",
        ltr2: "--. ",
        map: "2729",
    },
    Ndb {
        key: "k22onchon",
        name: "K-22_Onchon",
        frq: "320",
        cn: "17",
        cd: "OKh",
        ltr1: "--- ",
        ltr2: "....",
        map: "2211",
    },
    Ndb {
        key: "k23pyongyang",
        name: "K-23_Pyongyang",
        frq: "330",
        cn: "18",
        cd: "PYa",
        ltr1: ".--.",
        ltr2: ".-.-",
        map: "2015",
    },
    Ndb {
        key: "k24pyongyang",
        name: "K-24_Pyongyang",
        frq: "340",
        cn: "19",
        cd: "PN",
        ltr1: ".--.",
        ltr2: "-.  ",
        map: "2116",
    },
    Ndb {
        key: "k25wonsan",
        name: "K-25_Wonsan",
        frq: "350",
        cn: "20",
        cd: "VS",
        ltr1: ".-- ",
        ltr2: "... ",
        map: "1930",
    },
    Ndb {
        key: "k26sondok",
        name: "K-26_Sondok",
        frq: "360",
        cn: "21",
        cd: "SD",
        ltr1: "... ",
        ltr2: "-.. ",
        map: "1230",
    },
    Ndb {
        key: "k27yonpo",
        name: "K-27_Yonpo",
        frq: "370",
        cn: "22",
        cd: "YP",
        ltr1: ".---",
        ltr2: ".--.",
        map: "1231",
    },
    Ndb {
        key: "k28hamhungwest",
        name: "K-28_Hamhung West",
        frq: "380",
        cn: "23",
        cd: "KhKh",
        ltr1: "....",
        ltr2: "....",
        map: "1030",
    },
    Ndb {
        key: "k29sinanju",
        name: "K-29_Sinanju",
        frq: "390",
        cn: "24",
        cd: "SN",
        ltr1: "... ",
        ltr2: "-.  ",
        map: "1311",
    },
    Ndb {
        key: "k30sinuiju",
        name: "K-30_Sinuiju",
        frq: "400",
        cn: "25",
        cd: "SY",
        ltr1: "... ",
        ltr2: "-.--",
        map: "0904",
    },
    Ndb {
        key: "antung",
        name: "Antung",
        frq: "500",
        cn: "35",
        cd: "AN",
        ltr1: ".-  ",
        ltr2: "-.  ",
        map: "0903",
    },
    Ndb {
        key: "tatungkao",
        name: "Tatung-kao",
        frq: "510",
        cn: "36",
        cd: "MU",
        ltr1: "--  ",
        ltr2: "..- ",
        map: "1002",
    },
    Ndb {
        key: "dapu",
        name: "Dapu",
        frq: "520",
        cn: "37",
        cd: "DP",
        ltr1: "-.. ",
        ltr2: ".--.",
        map: "0402",
    },
    Ndb {
        key: "kuandian",
        name: "Kuandian",
        frq: "530",
        cn: "38",
        cd: "KD",
        ltr1: "-.- ",
        ltr2: "-.. ",
        map: "0206",
    },
    Ndb {
        key: "uiju",
        name: "Uiju",
        frq: "540",
        cn: "39",
        cd: "YD",
        ltr1: "-.--",
        ltr2: "-.. ",
        map: "0805",
    },
    Ndb {
        key: "sariwon",
        name: "Sariwon",
        frq: "550",
        cn: "40",
        cd: "SV",
        ltr1: "... ",
        ltr2: ".-- ",
        map: "2526",
    },
    Ndb {
        key: "iwon",
        name: "Iwon",
        frq: "560",
        cn: "41",
        cd: "IV",
        ltr1: "..  ",
        ltr2: ".-- ",
        map: "0540",
    },
    Ndb {
        key: "kangwon",
        name: "Kangwon",
        frq: "570",
        cn: "42",
        cd: "KV",
        ltr1: "-.- ",
        ltr2: ".-- ",
        map: "2532",
    },
    Ndb {
        key: "k55osan",
        name: "K-55_Osan",
        frq: "200",
        cn: "05",
        cd: "OS",
        ltr1: "--- ",
        ltr2: "... ",
        map: "4227",
    },
    Ndb {
        key: "k54chodo",
        name: "K-54_Cho-do",
        frq: "210",
        cn: "06",
        cd: "CD",
        ltr1: "-.-.",
        ltr2: "-.. ",
        map: "2607",
    },
    Ndb {
        key: "k53paengyongdo",
        name: "K-53_Paengyong-do",
        frq: "220",
        cn: "07",
        cd: "PD",
        ltr1: ".--.",
        ltr2: "-.. ",
        map: "3206",
    },
    Ndb {
        key: "k6pyongtaek",
        name: "K-6_Pyongtaek",
        frq: "230",
        cn: "08",
        cd: "PT",
        ltr1: ".--.",
        ltr2: "-   ",
        map: "4427",
    },
    Ndb {
        key: "k13suwon",
        name: "K-13_Suwon",
        frq: "240",
        cn: "09",
        cd: "SU",
        ltr1: "... ",
        ltr2: "..- ",
        map: "4026",
    },
    Ndb {
        key: "k14kimpo",
        name: "K-14_Kimpo",
        frq: "250",
        cn: "10",
        cd: "KP",
        ltr1: "-.- ",
        ltr2: ".--.",
        map: "3724",
    },
    Ndb {
        key: "k16seoul",
        name: "K-16_Seoul",
        frq: "260",
        cn: "11",
        cd: "SL",
        ltr1: "... ",
        ltr2: ".-..",
        map: "3726",
    },
    Ndb {
        key: "k18gangneung",
        name: "K-18_Gangneung",
        frq: "270",
        cn: "12",
        cd: "KN",
        ltr1: "-.- ",
        ltr2: "-.  ",
        map: "3443",
    },
    Ndb {
        key: "k41chungju",
        name: "K-41_Chungju",
        frq: "410",
        cn: "26",
        cd: "CG",
        ltr1: "-.-.",
        ltr2: "--. ",
        map: "4334",
    },
    Ndb {
        key: "k44changhowonni",
        name: "K-44_Changhowon-ni",
        frq: "420",
        cn: "27",
        cd: "CW",
        ltr1: "-.-.",
        ltr2: ".-- ",
        map: "4132",
    },
    Ndb {
        key: "k45yoju",
        name: "K-45_Yoju",
        frq: "430",
        cn: "28",
        cd: "YJ",
        ltr1: "-.--",
        ltr2: ".---",
        map: "4033",
    },
    Ndb {
        key: "k46hoengsong",
        name: "K-46_Hoengsong",
        frq: "440",
        cn: "29",
        cd: "HS",
        ltr1: "....",
        ltr2: "... ",
        map: "3835",
    },
    Ndb {
        key: "k47chuncheon",
        name: "K-47 Chuncheon",
        frq: "450",
        cn: "30",
        cd: "CH",
        ltr1: "-.-.",
        ltr2: "....",
        map: "3333",
    },
    Ndb {
        key: "k49yangsuri",
        name: "K-49_Yangsu-ri",
        frq: "460",
        cn: "31",
        cd: "YS",
        ltr1: "-.--",
        ltr2: "... ",
        map: "3729",
    },
    Ndb {
        key: "k50sokchori",
        name: "K-50_Sokcho-ri",
        frq: "470",
        cn: "32",
        cd: "SK",
        ltr1: "... ",
        ltr2: "-.- ",
        map: "3040",
    },
    Ndb {
        key: "k51inje",
        name: "K-51_Inje",
        frq: "480",
        cn: "33",
        cd: "IJ",
        ltr1: "..  ",
        ltr2: ".---",
        map: "3236",
    },
    Ndb {
        key: "k52yanggu",
        name: "K-52_Yanggu",
        frq: "490",
        cn: "34",
        cd: "YG",
        ltr1: "-.--",
        ltr2: "--. ",
        map: "3135",
    },
];

fn group_name_keys(group: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = group;
    while let Some(index) = rest.find("Name = \"") {
        rest = &rest[index + "Name = \"".len()..];
        let Some(end) = rest.find('"') else {
            break;
        };
        names.push(name_key(&rest[..end]));
        rest = &rest[end..];
    }
    names
}

fn name_key(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn strip_ndb_chart(html: &str) -> &str {
    let marker = "<br><br><style style=\"Style.RichText.Mono\">";
    if let Some(index) = html.find(marker) {
        return &html[..index];
    }
    html
}

/// Append title, description, and author past any group labels. Returns the
/// three LC indexes written into Options.
pub fn stamp_briefing(
    tables: &mut HashMap<String, LocaleTable>,
    title: &str,
    html: &str,
) -> (i32, i32, i32) {
    let title = one_line(title);
    let html = one_line(html);
    let author = UTILITY_AUTHOR;
    let max = tables
        .values()
        .filter_map(LocaleTable::max_id)
        .max()
        .unwrap_or(-1);
    let name_id = max + 1;
    let desc_id = max + 2;
    let author_id = max + 3;
    let eng = tables.entry("eng".to_string()).or_default();
    eng.insert(name_id, title.clone());
    eng.insert(desc_id, html.clone());
    eng.insert(author_id, author);
    let keys: Vec<String> = tables.keys().cloned().collect();
    for key in keys {
        if key == "eng" {
            continue;
        }
        let table = tables.get_mut(&key).expect("key just listed");
        table.insert(name_id, title.clone());
        table.insert(desc_id, html.clone());
        table.insert(author_id, author);
    }
    (name_id, desc_id, author_id)
}

/// The editor stops on "Found multiple objects pointing to the same
/// localization index". Index 0 means no string and stays shared. Every
/// later `LCName`, `LCDesc`, `LCAuthor`, or `LCText` that repeats an index gets a new
/// index and a copy of the string in each language table.
pub fn uniquify_locale(group_body: &str, tables: &mut HashMap<String, LocaleTable>) -> String {
    let mut seen = HashSet::new();
    let mut next = tables
        .values()
        .filter_map(LocaleTable::max_id)
        .max()
        .unwrap_or(-1);
    let mut out = String::with_capacity(group_body.len());
    for line in group_body.split_inclusive('\n') {
        let Some(id) = locale_assignment(line) else {
            out.push_str(line);
            continue;
        };
        if id <= 0 || seen.insert(id) {
            out.push_str(line);
            continue;
        }
        next += 1;
        copy_locale(tables, id, next);
        out.push_str(&line.replacen(&format!("= {id};"), &format!("= {next};"), 1));
    }
    out
}

fn locale_assignment(line: &str) -> Option<i32> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("LCName = ")
        .or_else(|| trimmed.strip_prefix("LCDesc = "))
        .or_else(|| trimmed.strip_prefix("LCAuthor = "))
        .or_else(|| trimmed.strip_prefix("LCText = "))?;
    rest.trim_end_matches(['\r', '\n'])
        .strip_suffix(';')?
        .trim()
        .parse()
        .ok()
}

fn copy_locale(tables: &mut HashMap<String, LocaleTable>, from: i32, to: i32) {
    let fallback = tables
        .get("eng")
        .and_then(|table| table.get(from))
        .unwrap_or("")
        .to_string();
    let keys: Vec<String> = tables.keys().cloned().collect();
    for key in keys {
        let text = tables
            .get(&key)
            .and_then(|table| table.get(from))
            .map(str::to_string)
            .unwrap_or_else(|| fallback.clone());
        tables
            .get_mut(&key)
            .expect("key just listed")
            .insert(to, text);
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], "")
}

/// The `.mission` text. `group_body` is the serialized base map after
/// duplicate localization indexes have been split.
pub fn render_mission(
    kind: MissionKind,
    package: &MissionPackage,
    scene: &MissionScene,
    name_id: i32,
    desc_id: i32,
    author_id: i32,
    group_body: &str,
) -> String {
    let prefix = scene.prefix();
    let sky = scene.sky_word();
    let base = package.cloud_base_m.round().max(0.0) as i32;
    let thick = package.clouds.file_height_m();
    let prec_level = if package.precip == Precip::None {
        0
    } else {
        package.prec_level.min(10)
    };
    let haze = (0.15 + f64::from(prec_level) * 0.06).min(1.0);
    let mut out = String::new();
    out.push_str("# Mission File Version = 1.0;");
    out.push_str(NEWLINE);
    out.push_str(NEWLINE);
    out.push_str("Options");
    out.push_str(NEWLINE);
    out.push('{');
    out.push_str(NEWLINE);
    for line in [
        format!("  LCName = {name_id};"),
        format!("  LCDesc = {desc_id};"),
        format!("  LCAuthor = {author_id};"),
        "  PlayerConfig = \"\";".to_string(),
        format!("  Time = {};", package.clock()),
        format!("  Date = {};", scene.date()),
        format!("  HMap = \"graphics/landscape_korea_{prefix}/height.hini\";"),
        format!("  Textures = \"graphics\\LANDSCAPE_Korea_{prefix}\\textures.tini\";"),
        format!("  Forests = \"graphics\\LANDSCAPE_Korea_{prefix}\\trees\\woods.wds\";"),
        "  Layers = \"\";".to_string(),
        format!("  GuiMap = \"landscape_korea_{prefix}\";"),
        format!("  SeasonPrefix = \"{prefix}\";"),
        format!("  MissionType = {};", kind.code()),
        "  AqmId = 0;".to_string(),
        format!("  CloudLevel = {base};"),
        format!("  CloudHeight = {thick};"),
        format!("  PrecLevel = {prec_level};"),
        format!("  PrecType = {};", package.precip.code()),
        format!(
            "  CloudConfig = \"{sky}\\{}\\sky.ini\";",
            package.clouds.config_folder()
        ),
        format!("  SeaState = {};", package.sea_state.min(6)),
        "  Turbulence = 0;".to_string(),
        "  TempPressLevel = 0;".to_string(),
        format!("  Temperature = {};", package.temperature_c),
        format!("  Pressure = {};", package.pressure_mmhg),
        format!("  Haze = {haze:.1};"),
        "  LayerFog = 0;".to_string(),
        "  CloudsShift = 0;".to_string(),
    ] {
        out.push_str(&line);
        out.push_str(NEWLINE);
    }
    out.push_str(&wind_layers(package));
    out.push_str("  Countries");
    out.push_str(NEWLINE);
    out.push_str("  {");
    out.push_str(NEWLINE);
    for line in [
        "    0 : 0;",
        "    501 : 1;",
        "    502 : 1;",
        "    503 : 1;",
        "    601 : 2;",
    ] {
        out.push_str(line);
        out.push_str(NEWLINE);
    }
    out.push_str("  }");
    out.push_str(NEWLINE);
    out.push('}');
    out.push_str(NEWLINE);
    out.push_str(NEWLINE);
    out.push_str(group_body);
    if !out.ends_with(NEWLINE) {
        out.push_str(NEWLINE);
    }
    // The editor stops reading without this trailer. Working missions end
    // with a blank line, then `# end of file` and no further newline.
    out.push_str(NEWLINE);
    out.push_str(NEWLINE);
    out.push_str("# end of file");
    out
}

/// A mission this utility wrote, restored to the package the Mission tab edits.
/// The embedded base map is not read. The date is reported so the Map strip
/// can stay where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedMission {
    pub kind: MissionKind,
    pub package: MissionPackage,
    /// `d.m.yyyy`, as stored in Options.
    pub date: String,
}

/// Read Options and the English sidecar. A file whose author is not
/// [`UTILITY_AUTHOR`] is refused. The group after Options is left unread.
pub fn load_utility_mission(text: &str, english: &LocaleTable) -> Result<LoadedMission, String> {
    let fields = match options_header(text) {
        Ok((_, fields)) => fields,
        Err(_) => {
            return Err("This file does not have the mission header the utility writes.".into());
        }
    };
    let author_id = fields.integer("LCAuthor")?;
    let author = english.get(author_id).unwrap_or("");
    if author != UTILITY_AUTHOR {
        return Err(
            "This mission was not written by the utility. Loading other missions is not supported yet."
                .into(),
        );
    }
    let kind = fields
        .integer("MissionType")
        .ok()
        .and_then(MissionKind::from_code)
        .ok_or_else(|| "This mission type is not one the utility writes.".to_string())?;
    let title = one_line(english.get(fields.integer("LCName")?).unwrap_or(""));
    if title.is_empty() {
        return Err("This mission has no title.".into());
    }
    let desc = english.get(fields.integer("LCDesc")?).unwrap_or("");
    let user_html = desc
        .split_once("mmHg.<br><br>")
        .map(|(_, rest)| rest)
        .ok_or_else(|| "The description is not a briefing this utility wrote.".to_string())?;
    let briefing = Briefing::from_html(strip_ndb_chart(user_html))?;
    let (hour, minute) = clock_hm(fields.text("Time")?)?;
    let clouds = cloud_from_config(fields.text("CloudConfig")?)?;
    let (wind_to, speed) = fields.surface_wind()?;
    let prec_level = fields.integer("PrecLevel")?.max(0) as u32;
    let prec_type = fields.integer("PrecType")?;
    let precip = if prec_level == 0 {
        Precip::None
    } else if prec_type == 0 {
        Precip::Snow
    } else {
        Precip::Rain
    };
    let mut package = MissionPackage::for_kind(kind);
    package.title = title;
    package.hour = hour;
    package.minute = minute;
    package.clouds = clouds;
    package.cloud_base_m = fields.integer("CloudLevel")? as f32;
    package.precip = precip;
    package.prec_level = if precip == Precip::None {
        0
    } else {
        prec_level
    };
    package.sea_state = fields.integer("SeaState")?.max(0) as u32;
    package.wind_from_deg = (wind_to + 180).rem_euclid(360);
    package.wind_speed_mps = speed.max(0) as u32;
    package.temperature_c = fields.integer("Temperature")?;
    package.pressure_mmhg = fields.integer("Pressure")?;
    package.climate_custom = true;
    package.briefing = briefing;
    package.clamp_weather();
    Ok(LoadedMission {
        kind,
        package,
        date: fields.text("Date")?.to_string(),
    })
}

struct OptionFields {
    values: HashMap<String, String>,
    wind: Vec<(i32, i32, i32)>,
}

impl OptionFields {
    fn text(&self, key: &str) -> Result<&str, String> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("The mission header is missing {key}."))
    }

    fn integer(&self, key: &str) -> Result<i32, String> {
        self.text(key)?
            .parse()
            .map_err(|_| format!("The mission header has a {key} this utility does not write."))
    }

    fn surface_wind(&self) -> Result<(i32, i32), String> {
        self.wind
            .iter()
            .find(|(alt, _, _)| *alt == 0)
            .map(|(_, dir, speed)| (*dir, *speed))
            .ok_or_else(|| "The mission header is missing the surface wind.".to_string())
    }
}

fn options_header(input: &str) -> IResult<&str, OptionFields> {
    let (input, _) = tag("# Mission File Version = 1.0;").parse(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("Options").parse(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, _) = multispace0(input)?;
    let mut values = HashMap::new();
    let mut wind = Vec::new();
    let mut rest = input;
    loop {
        rest = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        if rest.starts_with('}') {
            rest = &rest[1..];
            break;
        }
        if rest.is_empty() {
            return Err(Err::Error(Error::new(rest, ErrorKind::Eof)));
        }
        if let Ok((next, rows)) = wind_block(rest) {
            wind = rows;
            rest = next;
            continue;
        }
        if let Ok((next, ())) = named_block(rest) {
            rest = next;
            continue;
        }
        let (next, (key, value)) = field(rest)?;
        values.insert(key, value);
        rest = next;
    }
    Ok((rest, OptionFields { values, wind }))
}

fn field(input: &str) -> IResult<&str, (String, String)> {
    let (input, key) = ident(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('=').parse(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = alt_value(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((input, (key, value)))
}

fn alt_value(input: &str) -> IResult<&str, String> {
    if input.starts_with('"') {
        let (input, inner) = quoted(input)?;
        return Ok((input, inner));
    }
    let (input, raw) = take_while(|c: char| c != ';' && c != '\n' && c != '\r').parse(input)?;
    Ok((input, raw.trim().to_string()))
}

fn quoted(input: &str) -> IResult<&str, String> {
    let (input, _) = char('"').parse(input)?;
    let (input, inner) = take_while(|c: char| c != '"').parse(input)?;
    let (input, _) = char('"').parse(input)?;
    Ok((input, inner.replace("\"\"", "\"")))
}

fn ident(input: &str) -> IResult<&str, String> {
    recognize(preceded(
        take_while1(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_'),
    ))
    .parse(input)
    .map(|(rest, name)| (rest, name.to_string()))
}

fn wind_block(input: &str) -> IResult<&str, Vec<(i32, i32, i32)>> {
    let (input, _) = tag("WindLayers").parse(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, _) = multispace0(input)?;
    let mut rows = Vec::new();
    let mut rest = input;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        rest = trimmed;
        if rest.starts_with('}') {
            rest = &rest[1..];
            break;
        }
        let (next, row) = wind_row(rest)?;
        rows.push(row);
        rest = next;
    }
    Ok((rest, rows))
}

fn wind_row(input: &str) -> IResult<&str, (i32, i32, i32)> {
    let (input, alt) = signed(input)?;
    let (input, _) = (multispace0, char(':'), multispace0).parse(input)?;
    let (input, dir) = signed(input)?;
    let (input, _) = (multispace0, char(':'), multispace0).parse(input)?;
    let (input, speed) = signed(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((input, (alt, dir, speed)))
}

fn named_block(input: &str) -> IResult<&str, ()> {
    let (input, _) = ident(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('{').parse(input)?;
    let mut depth = 1;
    let mut end = None;
    for (index, ch) in input.char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end = Some(index);
                break;
            }
        }
    }
    let Some(end) = end else {
        return Err(Err::Error(Error::new(input, ErrorKind::Eof)));
    };
    Ok((&input[end + 1..], ()))
}

fn signed(input: &str) -> IResult<&str, i32> {
    map_res(recognize((opt(char('-')), digit1)), |s: &str| {
        s.parse::<i32>()
    })
    .parse(input)
}

fn clock_hm(text: &str) -> Result<(u32, u32), String> {
    let mut parts = text.split(':');
    let hour = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| "The mission clock is not one the utility writes.".to_string())?;
    let minute = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| "The mission clock is not one the utility writes.".to_string())?;
    Ok((hour, minute))
}

fn cloud_from_config(path: &str) -> Result<CloudPreset, String> {
    let folder = path
        .rsplit(['\\', '/'])
        .nth(1)
        .ok_or_else(|| "The cloud path is not one the utility writes.".to_string())?;
    CloudPreset::from_folder(folder)
        .ok_or_else(|| format!("The cloud type {folder} is not in the utility's list."))
}

/// Safe file stem from the mission title.
pub fn file_stem(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() && !out.ends_with('_') {
            out.push('_');
        }
        if out.len() >= 80 {
            break;
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "Mission".to_string()
    } else {
        out.to_string()
    }
}

/// A named summit. Tests check these against the height store. The Mission
/// page uses `KOREA_HIGHEST_GROUND_M` instead of drawing this cut.
#[allow(dead_code)]
struct Summit {
    name: &'static str,
    lat: f64,
    lon: f64,
}

#[allow(dead_code)]
const SUMMITS: &[Summit] = &[
    Summit {
        name: "Seoraksan",
        lat: 38.1193,
        lon: 128.4652,
    },
    Summit {
        name: "Kumgangsan",
        lat: 38.6567,
        lon: 128.1039,
    },
    Summit {
        name: "Odaesan",
        lat: 37.798,
        lon: 128.543,
    },
    Summit {
        name: "Myohyangsan",
        lat: 40.018,
        lon: 126.333,
    },
    Summit {
        name: "Taebaeksan",
        lat: 37.095,
        lon: 128.915,
    },
];

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct SliceSample {
    pub z: f64,
    pub y: f64,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct SummitMark {
    pub name: &'static str,
    pub z: f64,
    pub y: f64,
}

/// West-east heights at one northing. `peak_y` is the highest sample.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSlice {
    pub northing_m: f64,
    pub samples: Vec<SliceSample>,
    pub peak_z: f64,
    pub peak_y: f64,
    pub marks: Vec<SummitMark>,
}

#[allow(dead_code)]
const SLICE_STEP_M: f64 = 2_000.0;
/// A named summit is drawn when the cut passes within this of its northing.
#[allow(dead_code)]
const SUMMIT_NEAR_M: f64 = 30_000.0;

/// Sample the height store from the Yellow Sea to the East Sea.
#[allow(dead_code)]
pub fn sample_slice(store: &HeightStore, northing_m: f64) -> TerrainSlice {
    let x = northing_m.clamp(MAP_MIN, MAP_MAX);
    let mut samples = Vec::new();
    let mut z = MAP_MIN;
    let mut peak_z = 0.0;
    let mut peak_y = 0.0;
    while z <= MAP_MAX + 0.5 {
        let y = store.height_at(x, z).unwrap_or(0.0).max(0.0);
        if y >= peak_y {
            peak_y = y;
            peak_z = z;
        }
        samples.push(SliceSample { z, y });
        z += SLICE_STEP_M;
    }
    let mut marks = Vec::new();
    for summit in SUMMITS {
        let (sx, sz) = geo::latlon_to_xz(summit.lat, summit.lon);
        if !geo::on_map(sx, sz) || (sx - x).abs() > SUMMIT_NEAR_M {
            continue;
        }
        let Some(y) = store.height_at(sx, sz) else {
            continue;
        };
        marks.push(SummitMark {
            name: summit.name,
            z: sz,
            y: y.max(0.0),
        });
    }
    TerrainSlice {
        northing_m: x,
        samples,
        peak_z,
        peak_y,
        marks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{HeightStore, KOREA_MAP_ID};

    fn scene() -> MissionScene {
        MissionScene {
            season: Season::Summer,
            year: 1952,
            month: 6,
            day: 8,
        }
    }

    #[test]
    fn briefing_html_is_basic_and_one_line() {
        let mut briefing = Briefing {
            text: "Hold <the> line\nat dawn".to_string(),
            marks: Vec::new(),
        };
        let hold = "Hold ".len();
        briefing.toggle(0, hold - 1, StyleBit::Bold);
        briefing.toggle(hold, hold + "<the>".len(), StyleBit::Italic);
        let html = briefing.to_html();
        assert!(html.starts_with("<b>Hold</b>"), "{html}");
        assert!(html.contains("<i>&lt;the&gt;</i>"), "{html}");
        assert!(html.contains("<br>"), "{html}");
        assert!(!html.contains('\n'), "{html}");
        briefing.toggle(0, hold - 1, StyleBit::Bold);
        assert!(
            !briefing.to_html().contains("<b>"),
            "{}",
            briefing.to_html()
        );
    }

    #[test]
    fn typing_inside_a_mark_keeps_the_style() {
        let mut briefing = Briefing {
            text: "abcd".to_string(),
            marks: Vec::new(),
        };
        briefing.toggle(0, 4, StyleBit::Underline);
        briefing.rebase("abXcd");
        assert_eq!(briefing.to_html(), "<u>abXcd</u>");
    }

    #[test]
    fn rain_matches_the_harvested_freeflight() {
        assert_eq!(Precip::Rain.code(), 1);
        assert_eq!(Precip::Snow.code(), 0);
        assert_eq!(MissionKind::Single.code(), 0);
        assert_eq!(MissionKind::Coop.code(), 1);
        assert_eq!(MissionKind::Deathmatch.code(), 2);
    }

    #[test]
    fn mission_file_carries_korea_weather_and_the_group() {
        let package = MissionPackage::for_kind(MissionKind::Coop);
        let body = "Group\r\n{\r\n}\r\n";
        let text = render_mission(MissionKind::Coop, &package, &scene(), 0, 1, 2, body);
        assert!(text.starts_with("# Mission File Version = 1.0;\r\n"));
        assert!(text.contains("MissionType = 1;"));
        assert!(text.contains("SeasonPrefix = \"su\";"));
        assert!(text.contains("GuiMap = \"landscape_korea_su\";"));
        assert!(text.contains("HMap = \"graphics/landscape_korea_su/height.hini\";"));
        assert!(text.contains("CloudConfig = \"summer\\01_Light_00\\sky.ini\";"));
        assert!(text.contains("CloudLevel = 700;"));
        assert!(text.contains("CloudHeight = 500;"));
        assert!(text.contains("Pressure = 752;"));
        assert!(text.contains("0 :     20 :     5;"));
        assert!(text.contains("Time = 8:0:0;"));
        assert!(text.contains("Date = 8.6.1952;"));
        assert!(text.contains("SeaState = 2;"));
        assert!(text.contains("PrecLevel = 0;"));
        assert!(text.contains("PrecType = 1;"));
        assert!(text.contains("501 : 1;"));
        assert!(text.contains("601 : 2;"));
        assert!(text.contains("Temperature = 24;"));
        assert!(text.contains(body));
        assert!(text.ends_with("\r\n\r\n# end of file"));
        assert!(!text.contains('\n') || text.contains("\r\n"));
    }

    #[test]
    fn description_is_html_and_does_not_clobber_group_labels() {
        let package = MissionPackage::for_kind(MissionKind::Single);
        let html = describe(&package, &scene());
        assert!(html.contains("<u>Weather</u>"), "{html}");
        assert!(html.contains("<m-ft-v>700</m-ft-v>"), "{html}");
        assert!(html.contains("Pressure 752 mmHg"), "{html}");
        assert!(html.contains("<ms-mph-v>"), "{html}");
        assert!(html.contains("Primary objective"), "{html}");
        assert!(!html.contains('\n'));

        let mut tables = HashMap::new();
        let mut eng = LocaleTable::default();
        eng.insert(0, "Seoul");
        tables.insert("eng".to_string(), eng);
        let (name, desc, author) = stamp_briefing(&mut tables, "Dawn", &html);
        assert_eq!((name, desc, author), (1, 2, 3));
        assert_eq!(tables["eng"].get(0), Some("Seoul"));
        assert_eq!(tables["eng"].get(1), Some("Dawn"));
        assert!(tables["eng"].get(2).unwrap().contains("<u>Weather</u>"));
    }

    #[test]
    fn each_mission_type_keeps_its_own_package() {
        let mut work = MissionWork::default();
        work.package_mut().title = "Dawn".into();
        work.active = MissionKind::Deathmatch;
        work.package_mut().title = "Furball".into();
        work.package_mut().clouds = CloudPreset::new(40);
        work.active = MissionKind::Single;
        assert_eq!(work.package().title, "Dawn");
        assert_eq!(work.package().clouds, CloudPreset::LIGHT);
        work.active = MissionKind::Deathmatch;
        assert_eq!(work.package().title, "Furball");
        assert_eq!(work.package().clouds.name(), "04_overcast_00");
    }

    #[test]
    fn choosing_a_preset_offers_that_familys_base() {
        assert_eq!(CloudPreset::new(0).suggested_base_m(), 500.0);
        assert_eq!(CloudPreset::new(10).name(), "01_light_00");
        assert_eq!(CloudPreset::new(30).suggested_base_m(), 800.0);
        assert_eq!(CloudPreset::new(49).name(), "04_overcast_09");
        assert_eq!(CloudPreset::new(80).index(), 49);
    }

    #[test]
    fn a_custom_climate_does_not_follow_the_season() {
        let mut package = MissionPackage::for_kind(MissionKind::Single);
        let summer = scene();
        package.follow_season(summer);
        assert_eq!(package.wind_from_deg, 200);
        package.wind_from_deg = 90;
        package.climate_custom = true;
        let winter = MissionScene {
            season: Season::Winter,
            year: 1951,
            month: 1,
            day: 12,
        };
        package.follow_season(winter);
        assert_eq!(package.wind_from_deg, 90);
        package.use_typical(winter);
        assert_eq!(package.temperature_c, -8);
        assert_eq!(package.pressure_mmhg, 768);
        assert_eq!(package.wind_from_deg, 330);
        assert_eq!(package.wind_speed_mps, 8);
        assert!(!package.climate_custom);
    }

    #[test]
    fn named_summits_sit_on_the_korea_map() {
        for summit in SUMMITS {
            let (x, z) = geo::latlon_to_xz(summit.lat, summit.lon);
            assert!(geo::on_map(x, z), "{}", summit.name);
        }
    }

    fn fill_cell(store: &mut HeightStore, x: f64, z: f64, y: f64) {
        let i = (x / 100.0).floor() as usize;
        let j = (z / 100.0).floor() as usize;
        for (di, dj) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            store.set_node(i + di, j + dj, y);
        }
    }

    #[test]
    fn slice_peak_is_the_highest_sample_and_a_nearby_summit_is_marked() {
        let mut store = HeightStore::new(KOREA_MAP_ID);
        let (sx, sz) = geo::latlon_to_xz(SUMMITS[0].lat, SUMMITS[0].lon);
        fill_cell(&mut store, sx, sz, 1_640.0);
        let on_step = (sz / SLICE_STEP_M).round() * SLICE_STEP_M;
        fill_cell(&mut store, sx, on_step, 1_700.0);
        let slice = sample_slice(&store, sx);
        assert!(slice.peak_y >= 1_640.0, "{}", slice.peak_y);
        assert!(
            slice.marks.iter().any(|mark| mark.name == "Seoraksan"),
            "{:?}",
            slice.marks
        );
        let far = sample_slice(&store, 0.0);
        assert!(
            far.marks.iter().all(|mark| mark.name != "Seoraksan"),
            "a cut at the south edge should not label Seoraksan"
        );
    }

    #[test]
    fn heavy_preset_matches_the_editor_list() {
        assert_eq!(CloudPreset::new(37).name(), "03_heavy_07");
        let winter = MissionScene {
            season: Season::Winter,
            year: 1951,
            month: 1,
            day: 12,
        };
        assert_eq!(winter.prefix(), "wi");
        assert_eq!(winter.sky_word(), "summer");
        let spring = MissionScene {
            season: Season::EarlySpring,
            year: 1951,
            month: 4,
            day: 22,
        };
        assert_eq!(spring.sky_word(), "summer");
        assert_eq!(spring.prefix(), "sp");
        assert_eq!(winter.typical().temperature_c, -8);
        assert_eq!(winter.typical().wind_speed_mps, 8);
        assert_eq!(CloudPreset::new(10).config_folder(), "01_Light_00");
        assert_eq!(KOREA_HIGHEST_GROUND_M, 2509.0);
    }

    #[test]
    fn a_repeated_locale_index_is_copied() {
        let body = "MCU\r\n{\r\n  LCText = 0;\r\n  LCText = 5;\r\n}\r\nMCU\r\n{\r\n  LCText = 0;\r\n  LCText = 5;\r\n}\r\n";
        let mut tables = HashMap::new();
        let mut eng = LocaleTable::default();
        eng.insert(5, "Runway");
        tables.insert("eng".to_string(), eng);
        let mut ger = LocaleTable::default();
        ger.insert(5, "Bahn");
        tables.insert("ger".to_string(), ger);
        let out = uniquify_locale(body, &mut tables);
        assert_eq!(out.matches("LCText = 0;").count(), 2);
        assert_eq!(out.matches("LCText = 5;").count(), 1);
        assert!(out.contains("LCText = 6;"));
        assert_eq!(tables["eng"].get(6), Some("Runway"));
        assert_eq!(tables["ger"].get(6), Some("Bahn"));
    }

    #[test]
    fn a_utility_mission_loads_its_package_and_another_author_is_refused() {
        let mut package = MissionPackage::for_kind(MissionKind::Coop);
        package.title = "Dawn".into();
        package.hour = 9;
        package.minute = 15;
        package.briefing.rebase("Hold <the> line");
        let hold = "Hold ".len();
        package.briefing.toggle(0, hold - 1, StyleBit::Bold);
        package
            .briefing
            .toggle(hold, hold + "<the>".len(), StyleBit::Italic);
        let html = describe(&package, &scene());
        let mut tables = HashMap::new();
        let (name, desc, author) = stamp_briefing(&mut tables, "Dawn", &html);
        let text = render_mission(
            MissionKind::Coop,
            &package,
            &scene(),
            name,
            desc,
            author,
            "Group\r\n{\r\n}\r\n",
        );
        let loaded = load_utility_mission(&text, &tables["eng"]).expect("load");
        assert_eq!(loaded.kind, MissionKind::Coop);
        assert_eq!(loaded.package.title, "Dawn");
        assert_eq!((loaded.package.hour, loaded.package.minute), (9, 15));
        assert_eq!(loaded.package.wind_from_deg, 200);
        assert_eq!(loaded.package.wind_speed_mps, 5);
        assert_eq!(loaded.package.clouds, CloudPreset::LIGHT);
        assert_eq!(loaded.package.temperature_c, 24);
        assert_eq!(loaded.package.pressure_mmhg, 752);
        assert!(loaded.package.climate_custom);
        assert_eq!(loaded.date, "8.6.1952");
        assert_eq!(
            loaded.package.briefing.to_html(),
            package.briefing.to_html()
        );

        tables
            .get_mut("eng")
            .unwrap()
            .insert(author, "Someone else");
        let err = load_utility_mission(&text, &tables["eng"]).unwrap_err();
        assert!(err.contains("not written by the utility"), "{err}");
    }

    #[test]
    fn ndb_chart_lists_only_fields_in_the_group_in_latin() {
        let group = "Group\r\n{\r\n  Name = \"K-14_Kimpo_AF\";\r\n  Name = \"K-17_Ongjin\";\r\n  Name = \"K-47_Chuncheon_AF\";\r\n}\r\n";
        let chart = ndb_chart(group);
        assert!(chart.starts_with("<style style=\"Style.RichText.Mono\">"));
        assert!(chart.ends_with("</style>"));
        assert!(chart.contains("|K-17_Ongjin       | 280 | 13 | OZh  |--- |...-| 3312 |"));
        assert!(chart.contains("|K-14_Kimpo        | 250 | 10 | KP   |-.- |.--.| 3724 |"));
        assert!(chart.contains("|K-47 Chuncheon    | 450 | 30 | CH   |-.-.|....| 3333 |"));
        assert!(!chart.contains("K-55_Osan"));
        assert!(chart.is_ascii());
        assert!(ndb_chart("Group\r\n{\r\n  Name = \"Block\";\r\n}\r\n").is_empty());

        let mut package = MissionPackage::for_kind(MissionKind::Single);
        package.title = "Dawn".into();
        package.briefing.rebase("Hold the line");
        let html = with_ndb_chart(&describe(&package, &scene()), group);
        let mut tables = HashMap::new();
        let (name, desc, author) = stamp_briefing(&mut tables, "Dawn", &html);
        let text = render_mission(
            MissionKind::Single,
            &package,
            &scene(),
            name,
            desc,
            author,
            group,
        );
        let loaded = load_utility_mission(&text, &tables["eng"]).expect("load");
        assert_eq!(loaded.package.briefing.to_html(), "Hold the line");
        assert!(!loaded.package.briefing.to_html().contains("OZh"));
    }
}
