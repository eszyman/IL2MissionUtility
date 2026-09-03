//! In-app user manual. Source of truth is `USER_MANUAL.md` at the repo root.

use eframe::egui::{self, Color32, RichText, ViewportBuilder, ViewportId};

pub const MANUAL: &str = include_str!("../USER_MANUAL.md");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Overview,
    Template,
    Fighter,
    Exclusive,
    Recon,
    Airfield,
    Front,
    Language,
    Import,
    Troubleshooting,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 10] = [
        HelpTopic::Overview,
        HelpTopic::Template,
        HelpTopic::Recon,
        HelpTopic::Fighter,
        HelpTopic::Exclusive,
        HelpTopic::Airfield,
        HelpTopic::Front,
        HelpTopic::Language,
        HelpTopic::Import,
        HelpTopic::Troubleshooting,
    ];

    pub fn title(self) -> &'static str {
        match self {
            HelpTopic::Overview => "Overview",
            HelpTopic::Template => "Template Builder",
            HelpTopic::Fighter => "Fighter Pack",
            HelpTopic::Exclusive => "Exclusive Activation",
            HelpTopic::Recon => "Army Generator",
            HelpTopic::Airfield => "Airfield",
            HelpTopic::Front => "Map",
            HelpTopic::Language => "Language files",
            HelpTopic::Import => "Importing into the mission editor",
            HelpTopic::Troubleshooting => "Troubleshooting",
        }
    }
}

pub fn show_window(ctx: &egui::Context, open: &mut bool, topic: &mut HelpTopic) {
    if !*open {
        return;
    }
    let mut still_open = *open;
    let mut topic_now = *topic;
    ctx.show_viewport_immediate(
        ViewportId::from_hash_of("user-manual"),
        ViewportBuilder::default()
            .with_title("IL-2 Group Generator — Help")
            .with_inner_size([720.0, 820.0])
            .with_min_inner_size([480.0, 400.0])
            .with_decorations(true),
        |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Topic").strong());
                    egui::ComboBox::from_id_salt("help_topic")
                        .selected_text(topic_now.title())
                        .width(360.0)
                        .show_ui(ui, |ui| {
                            for t in HelpTopic::ALL {
                                ui.selectable_value(&mut topic_now, t, t.title());
                            }
                        });
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width());
                        show_markdown(ui, section_for(topic_now));
                        ui.add_space(16.0);
                    });
            });
            if ctx.input(|i| i.viewport().close_requested()) {
                still_open = false;
            }
        },
    );
    *open = still_open;
    *topic = topic_now;
}

pub fn section_for(topic: HelpTopic) -> &'static str {
    extract_section(MANUAL, topic.title())
}

fn extract_section<'a>(md: &'a str, heading: &str) -> &'a str {
    let needle = format!("## {heading}");
    let Some(start) = md.find(&needle) else {
        return md;
    };
    let after = start + needle.len();
    let rest = &md[after..];
    let end = rest
        .find("\n## ")
        .map(|i| after + i)
        .unwrap_or(md.len());
    md[start..end].trim()
}

fn show_markdown(ui: &mut egui::Ui, md: &str) {
    let mut lines = md.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim_end();
        if line.is_empty() {
            ui.add_space(6.0);
            continue;
        }
        if line.starts_with("---") {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            ui.add_space(10.0);
            ui.label(RichText::new(rest.trim()).strong().size(15.0));
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            ui.add_space(4.0);
            ui.heading(rest.trim());
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            ui.heading(rest.trim());
            continue;
        }
        if line.starts_with('|') {
            let mut rows = vec![line];
            while let Some(next) = lines.peek() {
                if next.trim_start().starts_with('|') {
                    rows.push(lines.next().unwrap());
                } else {
                    break;
                }
            }
            show_table(ui, &rows);
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            show_bullet(ui, rest, 0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("* ") {
            show_bullet(ui, rest, 0);
            continue;
        }
        let trimmed = line.trim_start();
        if line.starts_with("  ") && (trimmed.starts_with("- ") || trimmed.starts_with("* ")) {
            show_bullet(ui, &trimmed[2..], 1);
            continue;
        }
        if let Some((num, rest)) = numbered_item(line) {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(format!("{num}.")).strong());
                emit_spans(ui, rest);
            });
            continue;
        }
        show_inline(ui, line);
    }
}

fn numbered_item(line: &str) -> Option<(u32, &str)> {
    let (digits, rest) = line.split_once(". ")?;
    let n = digits.parse().ok()?;
    Some((n, rest))
}

fn show_bullet(ui: &mut egui::Ui, text: &str, indent: usize) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if indent > 0 {
            ui.add_space(18.0 * indent as f32);
        }
        ui.label("•");
        emit_spans(ui, text);
    });
}

fn show_table(ui: &mut egui::Ui, rows: &[&str]) {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .filter(|row| !is_table_rule(row))
        .map(|row| {
            row.trim()
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect()
        })
        .filter(|row: &Vec<String>| !row.is_empty())
        .collect();
    if cells.is_empty() {
        return;
    }
    let cols = cells.iter().map(|r| r.len()).max().unwrap_or(0);
    egui::Grid::new(format!("help-table-{}", rows[0]))
        .num_columns(cols)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (i, row) in cells.iter().enumerate() {
                for cell in row {
                    if i == 0 {
                        ui.label(RichText::new(cell).strong());
                    } else {
                        ui.label(cell.as_str());
                    }
                }
                ui.end_row();
            }
        });
}

fn is_table_rule(row: &str) -> bool {
    let inner = row.trim().trim_matches('|');
    !inner.is_empty() && inner.chars().all(|c| c == '-' || c == '|' || c == ':' || c == ' ')
}

fn show_inline(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        emit_spans(ui, text);
    });
}

fn emit_spans(ui: &mut egui::Ui, text: &str) {
    for (chunk, bold, code) in inline_spans(text) {
        let mut rich = RichText::new(chunk);
        if code {
            rich = rich.monospace().color(Color32::from_rgb(140, 190, 150));
        } else if bold {
            rich = rich.strong();
        }
        ui.label(rich);
    }
}

fn inline_spans(text: &str) -> Vec<(String, bool, bool)> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut bold = false;
    let mut code = false;
    while !rest.is_empty() {
        let bold_at = (!code).then(|| rest.find("**")).flatten();
        let code_at = (!bold).then(|| rest.find('`')).flatten();
        let next = match (bold_at, code_at) {
            (Some(b), Some(c)) if b <= c => Some((b, true)),
            (Some(b), None) => Some((b, true)),
            (_, Some(c)) => Some((c, false)),
            (None, None) => None,
        };
        match next {
            Some((0, is_bold)) if is_bold => {
                rest = &rest[2..];
                bold = !bold;
            }
            Some((0, _)) => {
                rest = &rest[1..];
                code = !code;
            }
            Some((i, _)) => {
                out.push((rest[..i].to_string(), bold, code));
                rest = &rest[i..];
            }
            None => {
                out.push((rest.to_string(), bold, code));
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_has_a_section() {
        for topic in HelpTopic::ALL {
            let section = section_for(topic);
            assert!(
                section.starts_with("## "),
                "{} section should start with its heading, got {:?}",
                topic.title(),
                section.lines().next()
            );
            assert!(
                section.contains(topic.title()),
                "{} missing from extracted section",
                topic.title()
            );
            assert!(
                section.len() > 80,
                "{} section looks empty",
                topic.title()
            );
        }
    }

    #[test]
    fn template_section_lists_checkzones() {
        let s = section_for(HelpTopic::Template);
        assert!(s.contains("Zone IN"));
        assert!(s.contains("ENABLE / PULSE IN"));
        assert!(s.contains("PULSE OUT"));
        assert!(s.contains("DeathCount"));
        assert!(s.contains("Reset Counter"));
        assert!(s.contains("Modifier Set Value"));
        assert!(s.contains("finger-four"));
        assert!(s.contains("Planes"));
        assert!(s.contains("light flak"));
        assert!(s.contains("cruise speed"));
        assert!(s.contains("MCU_TR_Entity"));
        assert!(s.contains("Carriages"));
    }

    #[test]
    fn fighter_section_lists_enable_pulse_in() {
        let s = section_for(HelpTopic::Fighter);
        assert!(s.contains("ENABLE / PULSE IN"));
        assert!(s.contains("NodeGates"));
        assert!(s.contains("Group 1"));
    }

    #[test]
    fn exclusive_section_lists_end_timer() {
        let s = section_for(HelpTopic::Exclusive);
        assert!(s.contains("MISSION END"));
        assert!(s.contains("Zone IN"));
        assert!(s.contains("Closer"));
        assert!(s.contains("Export in place"));
    }

    #[test]
    fn recon_section_lists_zone_in() {
        let s = section_for(HelpTopic::Recon);
        assert!(s.contains("ENABLE / PULSE IN"));
        assert!(s.contains("Zone IN"));
        assert!(s.contains("Mission Begin"));
        assert!(s.contains("Closer"));
        assert!(s.contains("Remove random logic"));
        assert!(s.contains("30"));
        assert!(s.contains("Train"));
    }

    #[test]
    fn map_section_describes_base_map_export() {
        let s = section_for(HelpTopic::Front);
        assert!(s.contains("Generate Base Map"));
        assert!(s.contains("Reference groups"));
        assert!(s.contains("10 km"));
        assert!(s.contains("5 km"));
        assert!(s.contains("AO outline"));
        assert!(s.contains("LineType` 11"));
        assert!(s.contains("Hungnam"));
        assert!(s.contains("Editor map"));
        assert!(s.contains("Legend"));
        assert!(s.contains("Fighter CAP"));
        assert!(s.contains("checkerboard"));
        assert!(s.contains("Zone IN"));
        assert!(s.contains("Wave"));
        assert!(s.contains("Shipping"));
        assert!(s.contains("combined_terrain"));
        assert!(s.contains("Objectives"));
        assert!(s.contains("Armor"));
        assert!(s.contains("15 km"));
        assert!(s.contains("AttackArea"));
        assert!(s.contains("4.5 km"));
        assert!(s.contains("direction.svg"));
        assert!(s.contains("EasternTrain.svg"));
        assert!(s.contains("another branch"));
        assert!(s.contains("Model"));
        assert!(s.contains("LinkTrId"));
        assert!(!s.contains("Generate Icons"));
        assert!(!s.contains("Buildings / towns"));
    }
}
