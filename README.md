Todo: 
- Update this landing page to provide better instructions as to what this is.
- Update the user manual to human readable text (not AI babble) to provide a clear understanding of how this utility is supposed to work.
- Provide for localization

How to build:
- Download & install [Rust](https://rust-lang.org/)
- Click the green <>Code button and download the zip from this github page
- Unzip the downloaded source code into a known folder location, ie: C:\IL2MissionUtil
- open the source code folder in a terminal
- enter *cargo build --release* into the terminal to build the exe
- double click the newly built *Il2MissionUtility.exe* file now found in the /target/release folder to run the app.

Source code updated on 09/04/2026

Added [MapHelper](MapHelper/maphelper.md) - a fast, lightweight desktop utility designed for IL-2 Sturmovik: Great Battles mission mapping and map management. Built in Rust and powered by the `egui` framework, it provides an efficient and responsive interface for handling map data and mission files.
This utility is designed to provide a data set for the Mission Utility to use when placing units either on land or at sea.
