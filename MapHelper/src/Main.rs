use image::{GrayImage, Luma, Rgb, RgbImage};
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

// Input files for each map layer
const WATER_PATH: &str = "KoreaIL2Map_Waterways.jpg";
const ROAD_PATH: &str = "KoreaIL2Map_Roads.jpg";
const OPEN_PATH: &str = "KoreaIL2Map_Open.jpg";
const OUTPUT_BIN_PATH: &str = "combined_terrain.bin";
const OUTPUT_PREVIEW_PATH: &str = "combined_preview.png";

// Modifiable kernel sizes (in pixels) for morphological filtering
const WATER_KERNEL_SIZE: usize = 3;
const OPEN_AREA_KERNEL_SIZE: usize = 4;

// Bitwise flags for each terrain type
const FLAG_WATER: u8 = 1 << 0; // 0b0000_0001 (1)
const FLAG_ROAD: u8 = 1 << 1;  // 0b0000_0010 (2)
const FLAG_OPEN: u8 = 1 << 2;  // 0b0000_0100 (4)

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let remove_grid = args.iter().any(|arg| arg == "--remove-grid");

    // Water uses the 3x3 kernel only if the grid removal flag is passed
    let water_kernel = if remove_grid { Some(WATER_KERNEL_SIZE) } else { None };
    println!("Processing Waterways layer (Kernel: {:?})...", water_kernel);
    let (width, height, water_mask) = process_layer(WATER_PATH, water_kernel)?;

    // Roads use no morphology
    println!("Processing Roads layer (Kernel: None)...");
    let (r_w, r_h, road_mask) = process_layer(ROAD_PATH, None)?;
    assert_eq!((width, height), (r_w, r_h), "Dimensions for Roads layer mismatch!");

    // Open Areas use the modifiable 10x10 kernel by default
    println!("Processing Open Areas layer (Kernel: Some({}))...", OPEN_AREA_KERNEL_SIZE);
    let (o_w, o_h, open_mask) = process_layer(OPEN_PATH, Some(OPEN_AREA_KERNEL_SIZE))?;
    assert_eq!((width, height), (o_w, o_h), "Dimensions for Open Areas layer mismatch!");

    let total_pixels = (width * height) as usize;
    let mut combined_grid = vec![0u8; total_pixels];

    for i in 0..total_pixels {
        let mut packed_byte = 0u8;
        if water_mask[i] == 1 {
            packed_byte |= FLAG_WATER;
        }
        if road_mask[i] == 1 {
            packed_byte |= FLAG_ROAD;
        }
        if open_mask[i] == 1 {
            packed_byte |= FLAG_OPEN;
        }
        combined_grid[i] = packed_byte;
    }

    println!("Writing composite preview PNG to: {}", OUTPUT_PREVIEW_PATH);
    write_composite_preview(
        OUTPUT_PREVIEW_PATH,
        width,
        height,
        &water_mask,
        &road_mask,
        &open_mask,
    )?;

    println!("Writing combined binary map to: {}", OUTPUT_BIN_PATH);
    write_binary_file(OUTPUT_BIN_PATH, width, height, &combined_grid)?;

    println!("Process complete.");
    Ok(())
}

fn process_layer(
    path: &str,
    kernel_size: Option<usize>,
) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let path_obj = Path::new(path);
    if !path_obj.exists() {
        return Err(format!("Required input file missing: {}", path).into());
    }

    let img = image::open(path_obj)?.into_luma8();
    let (width, height) = img.dimensions();

    let mut binary_grid = vec![0u8; (width * height) as usize];
    for (x, y, pixel) in img.enumerate_pixels() {
        if pixel[0] >= 128 {
            binary_grid[(y * width + x) as usize] = 1;
        }
    }

    let final_grid = match kernel_size {
        Some(size) if size > 1 => {
            let eroded = erode(&binary_grid, width, height, size);
            let mut opened = dilate(&eroded, width, height, size);
            
            // Enforce strict boundaries against the original mask
            for i in 0..opened.len() {
                if opened[i] == 1 && binary_grid[i] == 0 {
                    opened[i] = 0;
                }
            }
            opened
        }
        _ => binary_grid,
    };

    Ok((width, height, final_grid))
}

/// Generalized erosion for an arbitrary N x N kernel
fn erode(grid: &[u8], width: u32, height: u32, kernel_size: usize) -> Vec<u8> {
    let mut output = vec![0u8; (width * height) as usize];
    let offset = (kernel_size as isize) / 2;

    for y in 0..(height as isize) {
        for x in 0..(width as isize) {
            let mut keep = true;
            'kernel: for dy in 0..(kernel_size as isize) {
                for dx in 0..(kernel_size as isize) {
                    let nx = x + dx - offset;
                    let ny = y + dy - offset;
                    
                    if nx < 0 || nx >= (width as isize) || ny < 0 || ny >= (height as isize) {
                        keep = false;
                        break 'kernel;
                    }
                    if grid[(ny * (width as isize) + nx) as usize] == 0 {
                        keep = false;
                        break 'kernel;
                    }
                }
            }
            if keep {
                output[(y * (width as isize) + x) as usize] = 1;
            }
        }
    }
    output
}

/// Generalized dilation for an arbitrary N x N kernel
fn dilate(grid: &[u8], width: u32, height: u32, kernel_size: usize) -> Vec<u8> {
    let mut output = vec![0u8; (width * height) as usize];
    let offset = (kernel_size as isize) / 2;

    for y in 0..(height as isize) {
        for x in 0..(width as isize) {
            if grid[(y * (width as isize) + x) as usize] == 1 {
                for dy in 0..(kernel_size as isize) {
                    for dx in 0..(kernel_size as isize) {
                        let nx = x + dx - offset;
                        let ny = y + dy - offset;
                        
                        if nx >= 0 && nx < (width as isize) && ny >= 0 && ny < (height as isize) {
                            output[(ny * (width as isize) + nx) as usize] = 1;
                        }
                    }
                }
            }
        }
    }
    output
}

fn write_binary_file(path: &str, width: u32, height: u32, grid: &[u8]) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b"WMAP")?;
    writer.write_all(&width.to_le_bytes())?;
    writer.write_all(&height.to_le_bytes())?;
    writer.write_all(grid)?;
    writer.flush()?;
    Ok(())
}

fn write_composite_preview(
    path: &str,
    width: u32,
    height: u32,
    water: &[u8],
    road: &[u8],
    open: &[u8],
) -> Result<(), image::ImageError> {
    let mut preview = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            
            let r = if road[idx] == 1 { 255 } else { 0 };
            let g = if open[idx] == 1 { 255 } else { 0 };
            let b = if water[idx] == 1 { 255 } else { 0 };
            
            preview.put_pixel(x, y, Rgb([r, g, b]));
        }
    }
    preview.save(path)
}