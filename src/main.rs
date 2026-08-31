mod aircraft;
mod airfield;
mod ast;
mod bombers;
mod duplicate;
mod flights;
mod frontlines;
mod geo;
mod help;
mod locale;
mod mapclip;
mod mapfighters;
mod mapground;
mod mapload;
mod mapnet;
mod mapshipping;
mod pack;
mod placement;
mod parser;
mod recon;
mod serialize;
mod template;
mod ui;
mod watermap;
mod weapon_range;

fn main() -> eframe::Result {
    ui::run()
}
