//! Decode `brand/logo-white.png` to a raw alpha mask for the EFI app.

use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
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
    let mut out = Vec::with_capacity(8 + alpha.len());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&alpha);
    let dest = PathBuf::from(env::var("OUT_DIR").unwrap()).join("logo.bin");
    fs::write(dest, out).unwrap();
}
