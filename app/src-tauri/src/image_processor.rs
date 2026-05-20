use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use std::path::Path;

const CELL_WIDTH: u32 = 192;
const CELL_HEIGHT: u32 = 208;
const ATLAS_COLS: u32 = 8;
const ATLAS_ROWS: u32 = 9;
const ATLAS_WIDTH: u32 = CELL_WIDTH * ATLAS_COLS;
const ATLAS_HEIGHT: u32 = CELL_HEIGHT * ATLAS_ROWS;

const CHROMA_THRESHOLD: f64 = 100.0;

/// Remove chroma key background from image, replacing it with transparency.
/// First detects the actual background color by sampling corner pixels,
/// then removes pixels close to that color. Falls back to the configured chroma hex.
pub fn remove_chroma_key(img: &DynamicImage, chroma_hex: &str) -> RgbaImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let detected = detect_background_color(&rgba);
    let (cr, cg, cb) = detected.unwrap_or_else(|| parse_hex_color(chroma_hex));

    let mut output = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let pixel = rgba.get_pixel(x, y);
            let dist = color_distance(pixel[0], pixel[1], pixel[2], cr, cg, cb);
            if dist < CHROMA_THRESHOLD {
                output.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            } else {
                output.put_pixel(x, y, *pixel);
            }
        }
    }
    output
}

/// Sample the four corners and edges to detect the dominant background color.
/// Returns Some((r,g,b)) if a consistent background is found, None otherwise.
fn detect_background_color(img: &RgbaImage) -> Option<(u8, u8, u8)> {
    let (w, h) = img.dimensions();
    if w < 4 || h < 4 {
        return None;
    }

    let sample_points: Vec<(u32, u32)> = vec![
        (0, 0), (1, 0), (0, 1),
        (w - 1, 0), (w - 2, 0), (w - 1, 1),
        (0, h - 1), (1, h - 1), (0, h - 2),
        (w - 1, h - 1), (w - 2, h - 1), (w - 1, h - 2),
        (w / 2, 0), (w / 2, h - 1),
        (0, h / 2), (w - 1, h / 2),
    ];

    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;

    let first = img.get_pixel(0, 0);
    for &(x, y) in &sample_points {
        let p = img.get_pixel(x, y);
        if p[3] < 128 {
            continue;
        }
        if color_distance(p[0], p[1], p[2], first[0], first[1], first[2]) < 80.0 {
            r_sum += p[0] as u64;
            g_sum += p[1] as u64;
            b_sum += p[2] as u64;
            count += 1;
        }
    }

    if count < 8 {
        return None;
    }

    Some((
        (r_sum / count) as u8,
        (g_sum / count) as u8,
        (b_sum / count) as u8,
    ))
}

/// Extract individual frames from a horizontal strip by dividing into equal slots.
/// After slicing, each frame's visible content is re-centered within the 192×208 cell
/// to compensate for inconsistent positioning across batch-generated strips.
pub fn extract_frames(strip: &RgbaImage, frame_count: u32) -> Vec<RgbaImage> {
    let mut frames = Vec::with_capacity(frame_count as usize);
    let strip_height = strip.height();
    let slot_width = strip.width() / frame_count;

    for i in 0..frame_count {
        let x_offset = i * slot_width;

        let copy_w = slot_width.min(CELL_WIDTH);
        let copy_h = strip_height.min(CELL_HEIGHT);
        let src_x_start = if slot_width > CELL_WIDTH { (slot_width - CELL_WIDTH) / 2 } else { 0 };
        let src_y_start = if strip_height > CELL_HEIGHT { (strip_height - CELL_HEIGHT) / 2 } else { 0 };

        let mut raw_frame: RgbaImage = ImageBuffer::new(copy_w, copy_h);
        for y in 0..copy_h {
            for x in 0..copy_w {
                let sx = x_offset + src_x_start + x;
                let sy = src_y_start + y;
                if sx < strip.width() && sy < strip.height() {
                    raw_frame.put_pixel(x, y, *strip.get_pixel(sx, sy));
                }
            }
        }

        let frame = center_content_in_cell(&raw_frame);
        frames.push(frame);
    }
    frames
}

/// Find the bounding box of non-transparent pixels and re-center the content
/// within a CELL_WIDTH×CELL_HEIGHT frame.
fn center_content_in_cell(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..h {
        for x in 0..w {
            if src.get_pixel(x, y)[3] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    let mut cell: RgbaImage = ImageBuffer::new(CELL_WIDTH, CELL_HEIGHT);

    if max_x < min_x || max_y < min_y {
        return cell;
    }

    let content_w = max_x - min_x + 1;
    let content_h = max_y - min_y + 1;
    let dst_x = (CELL_WIDTH.saturating_sub(content_w)) / 2;
    let dst_y = (CELL_HEIGHT.saturating_sub(content_h)) / 2;

    for cy in 0..content_h.min(CELL_HEIGHT) {
        for cx in 0..content_w.min(CELL_WIDTH) {
            let sx = min_x + cx;
            let sy = min_y + cy;
            if sx < w && sy < h {
                cell.put_pixel(dst_x + cx, dst_y + cy, *src.get_pixel(sx, sy));
            }
        }
    }

    cell
}

/// Compose all frames into a single atlas image (8 columns x 9 rows).
pub fn compose_atlas(all_rows: &[Vec<RgbaImage>]) -> RgbaImage {
    let mut atlas: RgbaImage = ImageBuffer::new(ATLAS_WIDTH, ATLAS_HEIGHT);

    for (row_idx, row_frames) in all_rows.iter().enumerate() {
        let y_offset = row_idx as u32 * CELL_HEIGHT;
        for (col_idx, frame) in row_frames.iter().enumerate() {
            if col_idx >= ATLAS_COLS as usize {
                break;
            }
            let x_offset = col_idx as u32 * CELL_WIDTH;
            for y in 0..frame.height().min(CELL_HEIGHT) {
                for x in 0..frame.width().min(CELL_WIDTH) {
                    let pixel = frame.get_pixel(x, y);
                    atlas.put_pixel(x_offset + x, y_offset + y, *pixel);
                }
            }
        }
    }

    clear_transparent_rgb(&mut atlas);
    atlas
}

/// Save atlas as WebP (lossless).
pub fn save_atlas_webp(atlas: &RgbaImage, path: &Path) -> Result<(), String> {
    let (w, h) = atlas.dimensions();
    let encoder = webp::Encoder::from_rgba(atlas.as_raw(), w, h);
    let webp_data = encoder.encode_lossless();
    std::fs::write(path, &*webp_data).map_err(|e| format!("写入 WebP 失败: {e}"))
}

/// Save atlas as PNG.
pub fn save_atlas_png(atlas: &RgbaImage, path: &Path) -> Result<(), String> {
    atlas
        .save(path)
        .map_err(|e| format!("写入 PNG 失败: {e}"))
}

/// Ensure fully transparent pixels have RGB = (0,0,0).
fn clear_transparent_rgb(img: &mut RgbaImage) {
    let (w, h) = img.dimensions();
    for y in 0..h {
        for x in 0..w {
            let pixel = img.get_pixel(x, y);
            if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
}

fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return (255, 0, 255); // fallback magenta
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    (r, g, b)
}

fn color_distance(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8) -> f64 {
    let dr = r1 as f64 - r2 as f64;
    let dg = g1 as f64 - g2 as f64;
    let db = b1 as f64 - b2 as f64;
    (dr * dr + dg * dg + db * db).sqrt()
}

/// Load image bytes (PNG) into a DynamicImage.
pub fn load_image_from_bytes(data: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(data).map_err(|e| format!("加载图片失败: {e}"))
}
