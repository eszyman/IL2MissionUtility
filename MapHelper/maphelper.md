content = """# MapHelper

MapHelper is a fast, lightweight desktop utility designed for IL-2 Sturmovik: Great Battles mission mapping and map management. Built in Rust and powered by the `egui` framework, it provides an efficient and responsive interface for handling map data and mission files.
This utility is designed to provide a data set for the Mission Utility to use when placing units either on land or at sea.

## Features

* **egui-Powered Interface:** Clean, highly responsive desktop UI built with Rust's `egui`.
* **IL-2 Sturmovik Integration:** Tailored specifically for handling IL-2 map configurations and mission data.
* **Fast & Lightweight:** Minimal resource footprint thanks to Rust's performance optimizations.
* **Cross-Platform:** Can be compiled for Windows, Linux, and macOS.

## Prerequisites

To build and run MapHelper from source, you will need to install the Rust toolchain:

* [Rust & Cargo](https://rustup.rs/) (latest stable release recommended)

## Installation & Building

Clone the repository and build the project using Cargo:

```bash
git clone [https://github.com/your-username/maphelper.git](https://github.com/your-username/maphelper.git)
cd maphelper
cargo build --release# Terrain Mask Extractor

A Rust command-line utility for preprocessing IL-2 Korea raster map images into a highly optimized, bit-packed binary format. This tool extracts waterways, road networks, and open areas from standard image exports, applies configurable morphological filtering to remove map grid artifacts, and outputs a single `$O(1)$` lookup file for mission generation utilities.

## Prerequisites
* **Rust Toolchain:** Requires `cargo` and `rustc` (Edition 2021).
* **Input Assets:** The utility requires the following grayscale/black-and-white images in the root execution directory:
  * `KoreaIL2Map_Waterways_2.jpg`
  * `KoreaIL2Map_Roads.jpg`
  * `KoreaIL2Map_Open.jpg`

## Build and Execution

Compile the application using Cargo:
```bash
cargo build --release
```

cargo run --release -- --remove-grid --open-size 15


## Outputs

The utility generates two files per run:

### 1. `combined_preview.png`
A full-color RGB composite image for visually verifying the morphological filtering and layer alignment before ingesting the binary data.
* **Blue Channel:** Waterways
* **Red Channel:** Roads
* **Green Channel:** Open Areas

 ![Composite Terrain Preview](MapHelper/combined_preview.png)

### 2. `combined_terrain.bin`
A flattened, bit-packed binary file containing the merged terrain data. 

**File Structure:**
* **Header (12 bytes):**
  * Bytes 0-3: Magic signature `WMAP` (ASCII)
  * Bytes 4-7: Image Width (`u32`, Little Endian)
  * Bytes 8-11: Image Height (`u32`, Little Endian)
* **Payload:** 
  * `width * height` bytes. 
  * Array index mapping: `index = y * width + x`.

**Bitmask Decoding (For the Mission Generator Engineer):**
Each byte in the payload represents a single pixel coordinate and holds up to 8 boolean terrain states. Use a bitwise `AND` (`&`) operator to evaluate occupancy:
* **Water:** `byte & 1 != 0`
* **Road:** `byte & 2 != 0`
* **Open Area:** `byte & 4 != 0`

## Architecture Notes: Roads & Railways
This section is deprecated in the code as graphical svg data is directly available for use to determine road and rail locations and geometry.
This utility outputs rasterized road data suitable for fast $O(1)$ collision checks (e.g., preventing static emplacements from spawning on roads). 

For dynamic routing—such as orienting vehicle convoys and tracing continuous paths—the mission generation utility should parse SVG vector paths directly. Vector data provides sequential vertices and mathematical tangents necessary for accurate unit heading alignment, whereas this raster binary provides absolute territorial boolean states.
...and this is exactly what is used for the data and placement of units along roads.
```eof
