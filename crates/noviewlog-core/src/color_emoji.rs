//! Color emoji via system Noto Color Emoji (CBDT/CBLC PNG strikes).
//!
//! Noto Color Emoji is bitmap-only; fontdue cannot rasterize it. We parse the
//! font with `ttf-parser`, pull embedded PNG bitmaps from CBDT, decode them,
//! and blit RGBA into the viewport.
//!
//! Bundling is intentionally skipped: the font is ~10MB+. Prefer the system
//! install (Linux packages / user fonts). Windows Segoe UI Emoji is typically
//! COLR/CPAL, not CBDT — only Noto Color Emoji (or another CBDT font) works here.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Mutex;

use ttf_parser::{Face, GlyphId, RasterImageFormat};

/// Decoded RGBA glyph ready to scale and blit.
#[derive(Clone)]
pub struct ColorEmojiGlyph {
    pub width: u32,
    pub height: u32,
    /// Horizontal bearing from the CBDT/raster image header (pixels at strike size).
    #[allow(dead_code)]
    pub x_offset: i16,
    /// Vertical bearing (PositiveYDown-friendly offset from baseline area).
    #[allow(dead_code)]
    pub y_offset: i16,
    /// Tight RGBA8 buffer (`width * height * 4`).
    pub rgba: Vec<u8>,
}

/// Lazy-loading CBDT color-emoji atlas keyed by Unicode scalar.
pub struct ColorEmojiAtlas {
    font_data: Vec<u8>,
    cache: Mutex<HashMap<char, Option<ColorEmojiGlyph>>>,
}

impl ColorEmojiAtlas {
    /// Load from the first readable candidate path. Returns `None` if missing.
    pub fn load() -> Option<Self> {
        let data = load_color_emoji_bytes()?;
        // Validate face parses and exposes at least one CBDT strike path.
        let face = Face::parse(&data, 0).ok()?;
        let probe = face.glyph_index('\u{1F680}').or_else(|| face.glyph_index('😀'))?;
        let _ = face.glyph_raster_image(probe, u16::MAX)?;
        Some(Self {
            font_data: data,
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn has_glyph(&self, ch: char) -> bool {
        self.glyph(ch).is_some()
    }

    /// Cached decode of the CBDT PNG for `ch`. ZWJ sequences are not handled (v1).
    pub fn glyph(&self, ch: char) -> Option<ColorEmojiGlyph> {
        {
            let cache = self.cache.lock().ok()?;
            if let Some(entry) = cache.get(&ch) {
                return entry.clone();
            }
        }
        let decoded = decode_glyph(&self.font_data, ch);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(ch, decoded.clone());
        }
        decoded
    }
}

fn load_color_emoji_bytes() -> Option<Vec<u8>> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy();
        for name in ["NotoColorEmoji.ttf", "NotoColorEmoji.ttc"] {
            candidates.push(format!("{home}/.local/share/fonts/{name}"));
            candidates.push(format!("{home}/.fonts/{name}"));
            candidates.push(format!(
                "{home}/AppData/Local/Microsoft/Windows/Fonts/{name}"
            ));
        }
    }
    for path in [
        // Linux
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/google-noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto-color-emoji/NotoColorEmoji.ttf",
        // Windows (only if a CBDT Noto build is installed)
        "C:\\Windows\\Fonts\\NotoColorEmoji.ttf",
        "C:\\Windows\\Fonts\\NotoColorEmoji.ttc",
    ] {
        candidates.push(path.to_string());
    }
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            if Face::parse(&data, 0).is_ok() {
                return Some(data);
            }
        }
    }
    None
}

fn decode_glyph(font_data: &[u8], ch: char) -> Option<ColorEmojiGlyph> {
    let face = Face::parse(font_data, 0).ok()?;
    let gid: GlyphId = face.glyph_index(ch)?;
    let img = face.glyph_raster_image(gid, u16::MAX)?;
    if img.format != RasterImageFormat::PNG {
        // Noto Color Emoji uses PNG in CBDT; skip raw/bitpacked formats for v1.
        return None;
    }
    let rgba = decode_png_rgba(img.data)?;
    Some(ColorEmojiGlyph {
        width: img.width as u32,
        height: img.height as u32,
        x_offset: img.x,
        y_offset: img.y,
        rgba,
    })
}

fn decode_png_rgba(png_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    match info.color_type {
        png::ColorType::Rgba => {
            buf.truncate(info.buffer_size());
            Some(buf)
        }
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in buf[..info.buffer_size()].chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            Some(rgba)
        }
        _ => None,
    }
}

/// Nearest-neighbor scale + alpha blit of a color emoji into an RGBA viewport buffer.
pub fn blit_color_emoji(
    out: &mut [u8],
    buf_width: u32,
    buf_height: u32,
    cell_x: i32,
    row_top: f32,
    row_height: f32,
    cell_width: u32,
    glyph: &ColorEmojiGlyph,
    clip: Option<(f32, f32)>,
) {
    if glyph.width == 0 || glyph.height == 0 || glyph.rgba.len() < 4 {
        return;
    }
    let cell_w = cell_width.max(1) as f32;
    // Fit strike into ~1 row tall and at most ~2 cells wide.
    let target_h = (row_height * 0.92).max(1.0);
    let max_w = cell_w * 1.85;
    let scale = (target_h / glyph.height as f32).min(max_w / glyph.width as f32);
    let dest_w = (glyph.width as f32 * scale).round().max(1.0) as i32;
    let dest_h = (glyph.height as f32 * scale).round().max(1.0) as i32;

    // Center in the cell horizontally; vertically center within the row.
    let dest_x = cell_x + ((cell_w - dest_w as f32) * 0.5).round() as i32;
    let dest_y = (row_top + (row_height - dest_h as f32) * 0.5).round() as i32;

    let clip_top = clip.map(|c| c.0.floor() as i32).unwrap_or(0);
    let clip_bottom = clip.map(|c| c.1.ceil() as i32).unwrap_or(buf_height as i32);
    let buf_w = buf_width as i32;
    let buf_h = buf_height as i32;

    for dy in 0..dest_h {
        let py = dest_y + dy;
        if py < 0 || py >= buf_h || py < clip_top || py >= clip_bottom {
            continue;
        }
        let src_y = ((dy as f32 + 0.5) / dest_h as f32 * glyph.height as f32).floor() as u32;
        let src_y = src_y.min(glyph.height - 1);
        for dx in 0..dest_w {
            let px = dest_x + dx;
            if px < 0 || px >= buf_w {
                continue;
            }
            let src_x = ((dx as f32 + 0.5) / dest_w as f32 * glyph.width as f32).floor() as u32;
            let src_x = src_x.min(glyph.width - 1);
            let si = ((src_y * glyph.width + src_x) * 4) as usize;
            let src = [
                glyph.rgba[si],
                glyph.rgba[si + 1],
                glyph.rgba[si + 2],
                glyph.rgba[si + 3],
            ];
            if src[3] == 0 {
                continue;
            }
            let di = ((py as u32 * buf_width + px as u32) * 4) as usize;
            blend_rgba(&mut out[di..di + 4], src);
        }
    }
}

fn blend_rgba(dst: &mut [u8], src: [u8; 4]) {
    let a = src[3] as f32 / 255.0;
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * a) + (dst[i] as f32 * (1.0 - a))) as u8;
    }
    dst[3] = 255;
}

/// True for codepoints we prefer to paint from the color-emoji font when present.
///
/// Includes misc-technical / misc-symbol blocks (⏱ U+23F1, ⚡, …) as well as
/// the main emoji planes. The draw path still requires a CBDT raster image, so
/// expanding these ranges is safe: missing glyphs fall through to fontdue.
pub fn is_color_emoji_candidate(ch: char) -> bool {
    matches!(
        ch,
        '\u{2300}'..='\u{23FF}' // Miscellaneous Technical (e.g. ⏱ ⌚️ ⌛)
            | '\u{2600}'..='\u{26FF}' // Miscellaneous Symbols (e.g. ⚡ ⚠)
            | '\u{2700}'..='\u{27BF}' // Dingbats
            | '\u{2B00}'..='\u{2BFF}' // Miscellaneous Symbols and Arrows (e.g. ⭐)
            | '\u{1F300}'..='\u{1FAFF}' // Misc. Symbols and Pictographs … Extended-A
            | '\u{1F1E6}'..='\u{1F1FF}' // Regional indicator symbols (flags; best-effort)
    )
}

/// Variation selectors / ZWJ: non-spacing, never rasterized as their own cell.
///
/// Sample apps often emit `⏱️` as U+23F1 + U+FE0F; without this, FE0F becomes tofu.
pub fn is_zero_width_emoji_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{200D}' // Zero Width Joiner (ZWJ sequences: skip for v1 display)
            | '\u{FE00}'..='\u{FE0F}' // Variation Selectors 1–16 (incl. text/emoji VS)
    )
}

/// Display cell count: Unicode scalars minus zero-width emoji marks.
pub fn display_cell_count(text: &str) -> usize {
    text.chars().filter(|ch| !is_zero_width_emoji_mark(*ch)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_system_noto_color_emoji_when_present() {
        let Some(atlas) = ColorEmojiAtlas::load() else {
            eprintln!("skip: Noto Color Emoji not installed");
            return;
        };
        assert!(
            atlas.has_glyph('\u{1F680}'),
            "expected rocket emoji in color font"
        );
    }

    #[test]
    fn rocket_png_decodes_with_colored_ink() {
        let Some(atlas) = ColorEmojiAtlas::load() else {
            eprintln!("skip: Noto Color Emoji not installed");
            return;
        };
        let glyph = atlas.glyph('\u{1F680}').expect("rocket glyph");
        assert!(glyph.width > 8 && glyph.height > 8);
        let colored = glyph
            .rgba
            .chunks_exact(4)
            .filter(|p| p[3] > 20 && (p[0] > 30 || p[1] > 30 || p[2] > 30))
            .count();
        assert!(colored > 100, "expected colored ink in rocket PNG, got {colored}");
    }

    #[test]
    fn stopwatch_is_color_candidate_and_has_cbdt() {
        assert!(
            is_color_emoji_candidate('\u{23F1}'),
            "⏱ (U+23F1) must be eligible for the color-emoji path"
        );
        let Some(atlas) = ColorEmojiAtlas::load() else {
            eprintln!("skip: Noto Color Emoji not installed");
            return;
        };
        assert!(
            atlas.has_glyph('\u{23F1}'),
            "expected stopwatch in Noto Color Emoji CBDT"
        );
    }

    #[test]
    fn variation_selectors_are_zero_width() {
        assert!(is_zero_width_emoji_mark('\u{FE0F}'));
        assert!(is_zero_width_emoji_mark('\u{FE0E}'));
        assert!(is_zero_width_emoji_mark('\u{200D}'));
        assert!(!is_zero_width_emoji_mark('\u{23F1}'));
        assert_eq!(display_cell_count("\u{23F1}\u{FE0F}"), 1);
        assert_eq!(display_cell_count("\u{FE0F}"), 0);
    }
}
