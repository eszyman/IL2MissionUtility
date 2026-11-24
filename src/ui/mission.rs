//! # mission.rs — Mission mode
//!
//! The last rail tab. It keeps three packages (single-player, co-op,
//! deathmatch). The center shows the captured cloud type and an altitude
//! scale from sea level to the highest measured ground in Korea. **Create
//! Mission** on this tab and on Map writes the `.mission` and the description
//! sidecar around the current base map.
//!
//! `ui.rs` is the anchor. It owns `GroupGeneratorApp::mission`, the rail,
//! and the shortcuts, and it calls the hooks below.
//!
//! Hooks `ui.rs` calls (keep these `pub(super)`):
//! * `mission_page` — the tab
//! * `create_mission` — Create Mission / Ctrl G
//! * `load_mission` — Load mission… / Ctrl O
//!
//! Presentation only. The file text is built by [`crate::mission`]. The base
//! map group is built by Map (`build_base_map`) and embedded unchanged.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::locale::{decode_locale_bytes, parse_locale, write_sidecars};
use crate::mission::{self, CloudPreset, MissionKind, MissionScene, Precip, StyleBit};
use crate::parser::{parse_group_file, parse_il2_document};
use crate::shell;
use crate::theme::{self, c};

use super::{Status, dialog, group_digits, labeled_slider, section_heading, side_panel};

const LEFT_W: f32 = 300.0;
const RIGHT_W: f32 = 304.0;

impl super::GroupGeneratorApp {
    pub(super) fn mission_page(&mut self, ctx: &egui::Context) {
        let scene = self.mission_scene();
        for package in &mut self.mission.packages {
            package.follow_season(scene);
        }
        side_panel(ctx, "mission_left", true, LEFT_W, |ui| {
            self.mission_packages(ui);
        });
        side_panel(ctx, "mission_right", false, RIGHT_W, |ui| {
            self.mission_briefing(ui);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            self.mission_center(ui);
        });
    }

    /// Write the active package and the current base map as a `.mission`.
    pub(super) fn create_mission(&mut self) {
        let kind = self.mission.active;
        let map_scene = self.mission_scene();
        self.mission.packages[kind.index()].follow_season(map_scene);
        let scene = self.mission_scene_dated();
        let package = self.mission.packages[kind.index()].clone();
        if package.title.trim().is_empty() {
            self.status = Status::Error("Give the mission a title.".into());
            return;
        }
        let built = match self.build_base_map() {
            Ok(built) => built,
            Err(err) => {
                self.status = Status::Error(err);
                return;
            }
        };
        let html = mission::with_ndb_chart(&mission::describe(&package, &scene), &built.text);
        let mut tables = built.tables;
        let group = mission::uniquify_locale(&built.text, &mut tables);
        let (name_id, desc_id, author_id) =
            mission::stamp_briefing(&mut tables, package.title.trim(), &html);
        let body =
            mission::render_mission(kind, &package, &scene, name_id, desc_id, author_id, &group);
        let stem = mission::file_stem(package.title.trim());
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Mission", &["mission"])
            .set_file_name(format!("{stem}.mission"))
            .save_file()
        else {
            return;
        };
        if let Err(err) = std::fs::write(&path, &body) {
            self.status = Status::Error(format!("Could not write file: {err}"));
            return;
        }
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("mission");
        match write_sidecars(&path, &tables) {
            Ok(exts) => {
                let langs = if exts.is_empty() {
                    "no language files".to_string()
                } else {
                    exts.join("/")
                };
                self.status = Status::Info(format!(
                    "Wrote {file} ({}, {}, {}). {langs}.",
                    kind.label(),
                    scene.date(),
                    package.clock()
                ));
                let mut notes = built.strike_notes;
                notes.extend(built.spawn_notes);
                if !notes.is_empty()
                    && let Status::Info(lead) = std::mem::replace(&mut self.status, Status::Idle)
                {
                    self.status = Status::Warn { lead, items: notes };
                }
            }
            Err(err) => {
                self.status = Status::Error(format!(
                    "Wrote the mission, but language files failed: {err}"
                ));
            }
        }
        self.add_terrain_note(built.terrain_note);
    }

    /// Read a `.mission` this utility wrote into the package of that type,
    /// and put the group after the header onto the Map tab.
    pub(super) fn load_mission(&mut self) {
        let Some(path) = dialog::FileDialog::new()
            .add_filter("IL-2 Mission", &["mission"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    self.status = Status::Error("This file is not text the utility wrote.".into());
                    return;
                }
            },
            Err(err) => {
                self.status = Status::Error(format!("Could not read the mission: {err}"));
                return;
            }
        };
        let english = match std::fs::read(path.with_extension("eng")) {
            Ok(bytes) => match decode_locale_bytes(&bytes) {
                Ok(text) => parse_locale(&text),
                Err(err) => {
                    self.status = Status::Error(format!("Could not read the English file: {err}"));
                    return;
                }
            },
            Err(_) => {
                self.status = Status::Error(
                    "This mission has no English file next to it, so the title and briefing cannot be read.".into(),
                );
                return;
            }
        };
        match mission::load_utility_mission(&text, &english) {
            Ok(loaded) => {
                let kind = loaded.kind;
                self.mission.active = kind;
                self.mission.packages[kind.index()] = loaded.package;
                self.mission.sel = None;
                self.mission.dragging_clouds = false;
                if let Some(date) = mission::parse_file_date(&loaded.date) {
                    self.mission.date_override = Some(date);
                }
                let file = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("mission");
                let group_text = match mission::embedded_group(&text) {
                    Ok(group) => group,
                    Err(err) => {
                        self.status = Status::Error(err);
                        return;
                    }
                };
                let entity = match parse_group_file(group_text)
                    .or_else(|_| parse_il2_document(group_text))
                {
                    Ok(entity) => entity,
                    Err(err) => {
                        self.status = Status::Error(format!(
                            "The group in the mission could not be read: {err}"
                        ));
                        return;
                    }
                };
                let summary = match self.install_base_map(entity, &path) {
                    Ok(summary) => summary,
                    Err(err) => {
                        self.status = Status::Error(err);
                        return;
                    }
                };
                self.status = Status::Info(format!(
                    "Loaded {file} ({}, {}). {summary}",
                    kind.label(),
                    self.mission.package().clock(),
                ));
            }
            Err(err) => self.status = Status::Error(err),
        }
    }

    /// Into-wind heading for the active package, in degrees. Wind from 200
    /// means takeoff heading 200. Calm wind returns `None`, and the ramp
    /// stays at the end nearer the buildings.
    pub(in crate::ui) fn mission_takeoff_heading(&mut self) -> Option<f64> {
        let scene = self.mission_scene();
        self.mission.package_mut().follow_season(scene);
        let pkg = self.mission.package();
        if pkg.wind_speed_mps == 0 {
            None
        } else {
            Some(f64::from(pkg.wind_from_deg.rem_euclid(360)))
        }
    }

    fn mission_scene(&self) -> MissionScene {
        let mark = self.current_mark();
        MissionScene {
            season: mark.season,
            year: mark.year,
            month: mark.month,
            day: mark.day,
        }
    }

    /// Map season, with the briefing date when one has been set.
    fn mission_scene_dated(&self) -> MissionScene {
        let scene = self.mission_scene();
        match self.mission.date_override {
            Some((day, month, year)) => scene.with_date(day, month, year),
            None => scene,
        }
    }

    fn mission_packages(&mut self, ui: &mut egui::Ui) {
        ui.label(section_heading("Package"));
        shell::hint(
            ui,
            "Each type keeps its own title, time, weather, and briefing.",
            false,
        );
        ui.add_space(4.0);
        let active = self.mission.active;
        for kind in MissionKind::ALL {
            let on = active == kind;
            if ui
                .add_sized(
                    Vec2::new(ui.available_width(), 28.0),
                    egui::Button::selectable(on, kind.label()),
                )
                .on_hover_text("Show this package. The others stay as you left them.")
                .clicked()
                && !on
            {
                self.mission.active = kind;
                self.mission.sel = None;
                self.mission.dragging_clouds = false;
            }
        }
        ui.add_space(8.0);
        ui.separator();
        self.mission_date(ui);
        ui.add_space(6.0);

        let scene = self.mission_scene();
        let pkg = self.mission.package_mut();
        pkg.clamp_weather();
        ui.label(section_heading("Time"));
        labeled_slider(ui, "Hour", &mut pkg.hour, 0..=23);
        labeled_slider(ui, "Minute", &mut pkg.minute, 0..=59);
        ui.add_space(6.0);
        ui.separator();

        ui.label(section_heading("Clouds"));
        ui.label(pkg.clouds.name());
        let mut index = pkg.clouds.index() as u32;
        ui.add(
            egui::Slider::new(&mut index, 0..=49)
                .show_value(false)
                .trailing_fill(true),
        );
        if index as usize != pkg.clouds.index() {
            let preset = CloudPreset::new(index as u8);
            pkg.clouds = preset;
            pkg.cloud_base_m = preset.suggested_base_m();
        }
        ui.add_space(4.0);
        let base = pkg.cloud_base_m;
        let mut base_edit = base;
        ui.label("Cloud base");
        ui.add(
            egui::Slider::new(&mut base_edit, 0.0..=8_000.0)
                .step_by(10.0)
                .suffix(" m")
                .trailing_fill(true),
        );
        if (base_edit - base).abs() > 0.5 {
            pkg.cloud_base_m = base_edit;
        }
        shell::hint(
            ui,
            "Scroll the type. Drag the cloud line on the scale to move the base.",
            false,
        );
        ui.add_space(6.0);
        ui.separator();

        ui.label(section_heading("Precipitation"));
        let mut precip = pkg.precip;
        ui.horizontal(|ui| {
            for kind in [Precip::None, Precip::Rain, Precip::Snow] {
                if ui
                    .add_sized(
                        Vec2::new(72.0, 28.0),
                        egui::Button::selectable(precip == kind, kind.label()),
                    )
                    .clicked()
                {
                    precip = kind;
                }
            }
        });
        if precip != pkg.precip {
            pkg.precip = precip;
            if precip == Precip::None {
                pkg.prec_level = 0;
            } else if pkg.prec_level == 0 {
                pkg.prec_level = 4;
            }
        }
        if pkg.precip != Precip::None {
            labeled_slider(ui, "Level", &mut pkg.prec_level, 1..=10);
        }
        ui.add_space(4.0);
        ui.label("Haze");
        let mut haze = pkg.haze;
        ui.add(
            egui::Slider::new(&mut haze, 0.0..=1.0)
                .step_by(0.01)
                .fixed_decimals(2)
                .trailing_fill(true),
        );
        if (haze - pkg.haze).abs() > 0.001 {
            pkg.haze = haze;
        }
        ui.add_space(6.0);
        ui.separator();

        ui.label(section_heading("Sea state"));
        ui.label(mission::sea_state_name(pkg.sea_state));
        labeled_slider(ui, "State", &mut pkg.sea_state, 0..=6);
        ui.add_space(6.0);
        ui.separator();

        ui.label(section_heading("Season air"));
        let mut wind = pkg.wind_from_deg;
        let mut speed = pkg.wind_speed_mps;
        let mut temp = pkg.temperature_c;
        let mut pressure = pkg.pressure_mmhg;
        ui.label("Wind from");
        ui.add(
            egui::Slider::new(&mut wind, 0..=359)
                .suffix(" deg")
                .trailing_fill(true),
        );
        ui.label("Wind speed");
        ui.add(
            egui::Slider::new(&mut speed, 0..=25)
                .suffix(" m/s")
                .trailing_fill(true),
        );
        ui.label("Temperature");
        ui.add(
            egui::Slider::new(&mut temp, -40..=45)
                .suffix(" C")
                .trailing_fill(true),
        );
        ui.label("Pressure");
        ui.add(
            egui::Slider::new(&mut pressure, 700..=790)
                .suffix(" mmHg")
                .trailing_fill(true),
        );
        if wind != pkg.wind_from_deg
            || speed != pkg.wind_speed_mps
            || temp != pkg.temperature_c
            || pressure != pkg.pressure_mmhg
        {
            pkg.wind_from_deg = wind;
            pkg.wind_speed_mps = speed;
            pkg.temperature_c = temp;
            pkg.pressure_mmhg = pressure;
            pkg.climate_custom = true;
        }
        if ui
            .add_sized(
                Vec2::new(ui.available_width(), 28.0),
                egui::Button::new("Season typical"),
            )
            .on_hover_text("Wind direction, speed, temperature, and pressure for the Map season.")
            .clicked()
        {
            pkg.use_typical(scene);
        }
        shell::hint(
            ui,
            "These start from the Map season. Move a slider to keep your own values.",
            false,
        );
    }

    fn mission_date(&mut self, ui: &mut egui::Ui) {
        ui.label(section_heading("Date"));
        let scene = self.mission_scene();
        let shown = self
            .mission
            .date_override
            .unwrap_or((scene.day, scene.month, scene.year));
        ui.label(
            mission::MissionScene {
                season: scene.season,
                year: shown.2,
                month: shown.1,
                day: shown.0,
            }
            .long_date(),
        );
        let max_day = u32::from(mission::days_in_month(shown.1, shown.2));
        let mut day = u32::from(shown.0).min(max_day);
        let mut month = shown.1;
        let mut year = shown.2;
        let day_before = day;
        labeled_slider(ui, "Day", &mut day, 1..=max_day);
        let mut month_changed = false;
        egui::ComboBox::from_id_salt("mission_month")
            .selected_text(mission::month_name(month))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for value in 1..=12 {
                    if ui
                        .selectable_label(month == value, mission::month_name(value))
                        .clicked()
                    {
                        month = value;
                        month_changed = true;
                    }
                }
            });
        let mut year_changed = false;
        egui::ComboBox::from_id_salt("mission_year")
            .selected_text(year.to_string())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for value in crate::frontlines::YEARS {
                    if ui
                        .selectable_label(year == value, value.to_string())
                        .clicked()
                    {
                        year = value;
                        year_changed = true;
                    }
                }
            });
        if day != day_before || month_changed || year_changed {
            let max_day = mission::days_in_month(month, year);
            let day = (day as u8).clamp(1, max_day);
            self.mission.date_override = Some((day, month, year));
        }
        if self.mission.date_override.is_some()
            && ui
                .add_sized(
                    Vec2::new(ui.available_width(), 28.0),
                    egui::Button::new("Map date"),
                )
                .on_hover_text("Use the date on the Map strip again.")
                .clicked()
        {
            self.mission.date_override = None;
        }
        shell::hint(
            ui,
            "Season still follows the Map strip. The date starts there. Change it to keep your own in the briefing and the file.",
            false,
        );
    }

    fn mission_briefing(&mut self, ui: &mut egui::Ui) {
        ui.label(section_heading("Briefing"));
        shell::hint(ui, "Select words, then a style. The entry shows it.", false);
        ui.add_space(4.0);
        let mut action = None;
        ui.horizontal(|ui| {
            for (bit, label) in [
                (StyleBit::Bold, "Bold"),
                (StyleBit::Italic, "Italic"),
                (StyleBit::Underline, "Underline"),
            ] {
                if ui
                    .add_sized(Vec2::new(84.0, 28.0), egui::Button::new(label))
                    .clicked()
                {
                    action = Some(BriefAction::Bit(bit));
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_sized(Vec2::new(84.0, 28.0), egui::Button::new("Mono"))
                .clicked()
            {
                action = Some(BriefAction::Bit(StyleBit::Mono));
            }
        });
        if let Some(action) = action {
            self.apply_brief_action(action);
        }
        ui.add_space(8.0);
        ui.label(section_heading("Your text"));
        let kind = self.mission.active.index();
        self.edit_brief(ui, ("mission_brief", kind), 8, "Briefing");
        ui.add_space(8.0);
        ui.separator();
        ui.label(section_heading("In the file"));
        shell::hint(
            ui,
            "The date and weather, the balloon report, your text, then the airfield chart.",
            false,
        );
        self.preview_mission_brief(ui);
    }

    fn apply_brief_action(&mut self, action: BriefAction) {
        let Some((a, b)) = self.mission.sel else {
            self.status = Status::Info("Click the briefing, then a style.".into());
            return;
        };
        if a == b {
            self.status =
                Status::Info("Select briefing text, then Bold, Italic, Underline, or Mono.".into());
            return;
        }
        let index = self.mission.active.index();
        let brief = &mut self.mission.packages[index].briefing;
        let (start, end) = (a.min(b), a.max(b));
        let BriefAction::Bit(bit) = action;
        brief.toggle(start, end, bit);
    }

    fn preview_mission_brief(&self, ui: &mut egui::Ui) {
        let scene = self.mission_scene_dated();
        let weather = mission::weather_brief(self.mission.package(), &scene);
        let body = self.mission.package().briefing.preview_lines();
        paint_underlined_lead(ui, "Date:", &format!(" {}", weather.date));
        paint_underlined_lead(ui, "Weather:", &format!(" {}", weather.weather));
        ui.label(weather.wind);
        paint_underlined_lead(ui, "Temperature:", &format!(" {}", weather.temperature));
        ui.add_space(8.0);
        paint_underlined_lead(ui, "Weather balloon report:", "");
        for line in &weather.balloon {
            ui.label(line);
        }
        ui.label(weather.turbulence);
        ui.add_space(8.0);
        paint_brief_lines(ui, &body);
        ui.add_space(8.0);
        let names = self.briefing_field_names();
        let chart = mission::ndb_lines_for_names(&names);
        if chart.is_empty() {
            shell::hint(
                ui,
                "No airfield in this area, so the radio chart is left out.",
                false,
            );
        } else {
            paint_underlined_lead(ui, "Airfield NDB List:", "");
            ui.add_space(4.0);
            egui::ScrollArea::horizontal()
                .id_salt("mission_ndb_preview")
                .show(ui, |ui| {
                    for line in chart {
                        ui.label(RichText::new(line).font(FontId::monospace(12.0)));
                    }
                });
        }
    }

    fn briefing_field_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for slot in &self.airbases {
            if !self.airbase_in_play(slot) {
                continue;
            }
            if let Some(stem) = slot.path.file_stem().and_then(|stem| stem.to_str()) {
                names.push(stem.to_string());
            }
            if !slot.info.name.eq_ignore_ascii_case("group")
                && !slot.info.name.eq_ignore_ascii_case("airfield")
                && !slot.info.name.is_empty()
            {
                names.push(slot.info.name.clone());
            }
        }
        names
    }

    fn mission_center(&mut self, ui: &mut egui::Ui) {
        let scene = self.mission_scene_dated();
        let mut title = self.mission.package().title.clone();
        ui.horizontal(|ui| {
            ui.label(section_heading("Title"));
            let edit = egui::TextEdit::singleline(&mut title)
                .id(egui::Id::new("mission_title"))
                .desired_width(ui.available_width())
                .min_size(Vec2::new(120.0, 28.0))
                .font(FontId::new(20.0, theme::heading_family()))
                .hint_text("Mission title");
            ui.add(edit);
        });
        if title != self.mission.package().title {
            self.mission.package_mut().title = title;
        }
        ui.add_space(4.0);
        let preset = self.mission.package().clouds;
        let hour = self.mission.package().hour;
        ui.label(
            RichText::new(format!(
                "{}    landscape_korea_{}    {}    {}    {}",
                preset.name(),
                scene.prefix(),
                scene.season.label(),
                scene.long_date(),
                self.mission.package().clock()
            ))
            .color(c::NEUTRAL_700),
        );
        ui.add_space(6.0);
        show_cloud_type(ui, preset, hour);
        ui.add_space(8.0);
        let height = (ui.available_height() - 4.0).max(180.0);
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
        let base = self.mission.package().cloud_base_m;
        let grabbed = paint_scale(
            ui,
            rect,
            &response,
            base,
            &mut self.mission.dragging_clouds,
            &mut self.mission.cloud_grab_m,
        );
        if let Some(next) = grabbed {
            self.mission.package_mut().cloud_base_m = next;
        }
    }

    fn edit_brief(&mut self, ui: &mut egui::Ui, id: impl std::hash::Hash, rows: usize, hint: &str) {
        let snapshot = self.mission.package().briefing.clone();
        let mut text = snapshot.text.clone();
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let mut styled = snapshot.clone();
            styled.rebase(buf.as_str());
            brief_galley(ui, &styled, wrap)
        };
        let output = egui::TextEdit::multiline(&mut text)
            .id(egui::Id::new(id))
            .desired_width(f32::INFINITY)
            .desired_rows(rows)
            .hint_text(hint)
            .layouter(&mut layouter)
            .show(ui);
        if output.response.changed() {
            self.mission.package_mut().briefing.rebase(&text);
        }
        if output.response.has_focus()
            && let Some(range) = output.cursor_range
        {
            let cursors = range.sorted_cursors();
            let text_now = &self.mission.package().briefing.text;
            let a = byte_at(text_now, cursors[0].index);
            let b = byte_at(text_now, cursors[1].index);
            self.mission.sel = Some((a, b));
        }
    }
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

enum BriefAction {
    Bit(StyleBit),
}

fn brief_galley(
    ui: &egui::Ui,
    brief: &mission::Briefing,
    wrap: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    let ink = ui.visuals().text_color();
    let pieces = brief.pieces();
    if pieces.is_empty() {
        job.append(
            "",
            0.0,
            egui::TextFormat {
                font_id: FontId::proportional(13.0),
                color: ink,
                ..Default::default()
            },
        );
    }
    for piece in &pieces {
        let font_id = if piece.mono {
            FontId::monospace(13.0)
        } else if piece.bold {
            FontId::new(13.0, theme::bold_family())
        } else {
            FontId::proportional(13.0)
        };
        job.append(
            &piece.text,
            0.0,
            egui::TextFormat {
                font_id,
                color: ink,
                italics: piece.italic,
                underline: if piece.underline {
                    Stroke::new(1.0_f32, ink)
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            },
        );
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn paint_underlined_lead(ui: &mut egui::Ui, lead: &str, rest: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new(lead).underline());
        if !rest.is_empty() {
            ui.label(rest);
        }
    });
}

fn paint_brief_lines(ui: &mut egui::Ui, lines: &[mission::BriefLine]) {
    for line in lines {
        if line.pieces.is_empty() {
            ui.add_space(8.0);
            continue;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for piece in &line.pieces {
                ui.label(piece_rich(piece));
            }
        });
    }
}

fn piece_rich(piece: &mission::BriefPiece) -> RichText {
    let mut rich = RichText::new(&piece.text);
    if piece.mono {
        rich = rich.font(FontId::monospace(13.0));
    } else if piece.bold {
        rich = rich.font(FontId::new(13.0, theme::bold_family()));
    }
    if piece.italic {
        rich = rich.italics();
    }
    if piece.underline {
        rich = rich.underline();
    }
    rich
}

fn show_cloud_type(ui: &mut egui::Ui, preset: CloudPreset, hour: u32) {
    let tex = cloud_frame(ui.ctx(), preset.index(), hour);
    let size = tex.size_vec2();
    let width = ui.available_width();
    let height = (width * size.y / size.x).clamp(120.0, 280.0);
    ui.add(
        egui::Image::new((tex.id(), size)).fit_to_exact_size(Vec2::new(width, height.max(28.0))),
    );
}

/// The frame on screen only. Other hours and types stay on disk until chosen.
fn cloud_frame(ctx: &egui::Context, index: usize, hour: u32) -> egui::TextureHandle {
    let id = egui::Id::new(("mission-cloud-frame", hour, index));
    if let Some(tex) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return tex;
    }
    let bytes = super::cloud_preview::jpeg(index, hour);
    let image = image::load_from_memory(bytes).expect("cloud preview");
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let tex = ctx.load_texture(
        format!("mission-cloud-{hour}-{index}"),
        color,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, tex.clone()));
    tex
}

fn paint_scale(
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    base: f32,
    dragging: &mut bool,
    grab_m: &mut f32,
) -> Option<f32> {
    let painter = ui.painter();
    let peak = mission::KOREA_HIGHEST_GROUND_M;
    let top_m = base.max(peak).max(3_000.0) * 1.12;
    let gutter = 56.0;
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + gutter, rect.top() + 8.0),
        Pos2::new(rect.right() - 8.0, rect.bottom() - 8.0),
    );
    painter.rect_filled(rect, 0.0, Color32::from_rgb(244, 242, 236));
    let y_of = |alt: f32| {
        let t = (alt / top_m).clamp(0.0, 1.0);
        plot.bottom() - t * plot.height()
    };
    let alt_of = |y: f32| {
        let t = ((plot.bottom() - y) / plot.height()).clamp(0.0, 1.0);
        t * top_m
    };
    let font = FontId::new(12.0, egui::FontFamily::Proportional);
    let ink = Color32::from_rgb(40, 40, 40);
    let mut tick = 0.0;
    while tick <= top_m {
        let y = y_of(tick);
        painter.hline(
            plot.left()..=plot.right(),
            y,
            Stroke::new(1.0_f32, Color32::from_rgb(210, 206, 196)),
        );
        painter.text(
            Pos2::new(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{} m", group_digits(f64::from(tick as i32))),
            font.clone(),
            ink,
        );
        tick += 500.0;
    }
    let sea = y_of(0.0);
    painter.hline(
        plot.left()..=plot.right(),
        sea,
        Stroke::new(2.0_f32, Color32::from_rgb(40, 90, 130)),
    );
    painter.text(
        Pos2::new(plot.left() + 8.0, sea - 8.0),
        Align2::LEFT_BOTTOM,
        "Sea level",
        font.clone(),
        Color32::from_rgb(40, 90, 130),
    );
    let y_peak = y_of(peak);
    painter.hline(
        plot.left()..=plot.right(),
        y_peak,
        Stroke::new(1.5_f32, Color32::from_rgb(120, 72, 40)),
    );
    painter.text(
        Pos2::new(plot.right() - 8.0, y_peak - 8.0),
        Align2::RIGHT_BOTTOM,
        format!("Highest ground {} m", group_digits(f64::from(peak as i32))),
        font.clone(),
        Color32::from_rgb(120, 72, 40),
    );
    let y_base = y_of(base);
    painter.hline(
        plot.left()..=plot.right(),
        y_base,
        Stroke::new(2.0_f32, Color32::from_rgb(70, 90, 110)),
    );
    painter.text(
        Pos2::new(plot.left() + 8.0, y_base - 8.0),
        Align2::LEFT_BOTTOM,
        format!(
            "Cloud base {} m",
            group_digits(f64::from(base.round() as i32))
        ),
        font,
        ink,
    );

    let band = Rect::from_min_max(
        Pos2::new(plot.left(), y_base - 8.0),
        Pos2::new(plot.right(), y_base + 8.0),
    );
    let mut moved = None;
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
        && band.contains(pointer)
    {
        *dragging = true;
        *grab_m = base - alt_of(pointer.y);
    }
    if *dragging
        && response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let mut next = alt_of(pointer.y) + *grab_m;
        next = (next / 10.0).round() * 10.0;
        moved = Some(next.clamp(0.0, 8_000.0));
    }
    if response.drag_stopped() {
        *dragging = false;
    }
    moved
}
