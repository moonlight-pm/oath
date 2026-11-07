//! Decode `brand/logo-white.png` to a raw alpha mask for the EFI app, and
//! pack `src/font.txt`, the splash's font.

use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let png_path = manifest.join("../../brand/logo-white.png");
    println!("cargo:rerun-if-changed={}", png_path.display());
    let bytes = fs::read(&png_path).expect("brand/logo-white.png");
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("png info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let w = info.width;
    let h = info.height;
    let alpha: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].chunks(4).map(|p| p[3]).collect(),
        png::ColorType::GrayscaleAlpha => {
            buf[..info.buffer_size()].chunks(2).map(|p| p[1]).collect()
        }
        other => panic!("logo color {other:?}"),
    };
    assert_eq!(alpha.len(), (w * h) as usize);
    let mut bin = Vec::with_capacity(8 + alpha.len());
    bin.extend_from_slice(&w.to_le_bytes());
    bin.extend_from_slice(&h.to_le_bytes());
    bin.extend_from_slice(&alpha);
    fs::write(out.join("logo.bin"), bin).unwrap();
    // The mark's size without its pixels, for code that only lays out
    // around it (PID 1).
    fs::write(out.join("logo_size.rs"), format!("pub const LOGO_SIZE: u32 = {w};\n")).unwrap();

    let font_path = manifest.join("src/font.txt");
    println!("cargo:rerun-if-changed={}", font_path.display());
    let text = fs::read_to_string(&font_path).expect("src/font.txt");
    fs::write(out.join("font.bin"), pack_font(&text)).unwrap();
}

const GLYPHS: usize = 96;
const PER_BAND: usize = 16;
const ROWS: usize = 9;

/// Each glyph as nine bytes, one per row, its leftmost pixel in bit 4.
fn pack_font(text: &str) -> Vec<u8> {
    let rows: Vec<&str> = text.lines().filter(|l| !l.is_empty() && !l.starts_with(';')).collect();
    assert_eq!(rows.len(), GLYPHS / PER_BAND * ROWS, "font.txt: rows");
    let mut out = vec![0u8; GLYPHS * ROWS];
    for (i, row) in rows.iter().enumerate() {
        let (band, r) = (i / ROWS, i % ROWS);
        let cells: Vec<&str> = row.split(' ').collect();
        assert_eq!(cells.len(), PER_BAND, "font.txt row {i}: {row:?}");
        for (col, cell) in cells.iter().enumerate() {
            assert_eq!(cell.len(), 5, "font.txt row {i}: {cell:?}");
            let mut bits = 0u8;
            for c in cell.chars() {
                bits = bits << 1 | (c == '#') as u8;
                assert!(c == '#' || c == '.', "font.txt row {i}: {c:?}");
            }
            out[(band * PER_BAND + col) * ROWS + r] = bits;
        }
    }
    out
}
