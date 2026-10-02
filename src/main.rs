//! IL-2 Group Generator — binary crate for IL-2 Sturmovik: Great Battles
//! (Korea map).
//!
//! Declares every `src/` module and boots the egui app via [`ui::run`].
//! `--height-run` / `--height-merge` run HeightHelper, and `--probe-*`
//! arguments run the terrain probe tools; see `heighthelper` and `heightprobe`.
//! There is no `lib.rs`. See `docs/src-guide.md` for the per-file map.
//! Parsing lives in [`parser`] → [`ast`]; writing in [`serialize`].

mod aircraft;
mod airfield;
mod airstart;
mod ast;
mod bombers;
mod duplicate;
mod flights;
mod frontlines;
mod geo;
mod harvest;
mod heighthelper;
mod heightprobe;
mod help;
mod locale;
mod mapclip;
mod mapfighters;
mod mapground;
mod mapload;
mod mapnet;
mod mapshipping;
mod model_spec;
mod pack;
mod placement;
mod parser;
mod payloads;
mod recon;
mod serialize;
mod shell;
mod template;
mod terrain;
mod terrain_apply;
mod theme;
mod ui;
mod watermap;
mod weapon_range;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = heighthelper::run_cli(&args) {
        std::process::exit(code);
    }
    if let Some(code) = heightprobe::run_cli(&args) {
        std::process::exit(code);
    }
    ui::run()
}
