//! The projector's built-in camera: grab frames (via the embedded mc-grab), save PNGs, and
//! tell whether the picture on the wall changed or is moving. Screenshots can't do that:
//! video plays on a hardware layer that screencap doesn't see.
use crate::device::{Device, TMP};
use anyhow::{bail, Context, Result};
use std::path::Path;

static GRAB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mc-grab"));

pub const CW: usize = 480; // upright frame
pub const CH: usize = 640;

pub struct Frame { pub px: Vec<u8> }

impl Frame {
    pub fn mean(&self) -> f32 { self.px.iter().map(|&v| v as u32).sum::<u32>() as f32 / self.px.len() as f32 }
    pub fn at(&self, x: usize, y: usize) -> u8 { self.px[y * CW + x] }
}

fn ensure_grabber(d: &mut Device) -> Result<()> {
    if GRAB.is_empty() {
        bail!("this magicube was built without the camera grabber (rustup target add armv7-unknown-linux-musleabihf, rebuild)");
    }
    let remote = format!("{TMP}/mc-grab");
    let size = d.sh(&format!("stat -c %s {remote} 2>/dev/null"))?;
    if size.trim() != GRAB.len().to_string() {
        d.push_bytes(GRAB, &remote)?;
        d.sh(&format!("chmod 755 {remote}"))?;
    }
    Ok(())
}

fn parse_pgm(b: &[u8]) -> Result<Frame> {
    // "P5\n480 640\n255\n" + pixels
    let mut fields = 0;
    let mut i = 0;
    while fields < 4 && i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() { i += 1; }
        while i < b.len() && !b[i].is_ascii_whitespace() { i += 1; }
        fields += 1;
    }
    let px = b.get(i + 1..).context("short PGM")?.to_vec();
    if px.len() != CW * CH { bail!("camera frame has {} bytes, expected {}", px.len(), CW * CH); }
    Ok(Frame { px })
}

/// Grab `count` frames `gap_ms` apart. Needs root (the camera node is root-only).
/// Each grab shows a small Magisk toast on the picture; that's expected.
pub fn grab(d: &mut Device, exposure: u32, count: usize, gap_ms: u32) -> Result<Vec<Frame>> {
    d.need_root()?;
    ensure_grabber(d)?;
    let out = format!("{TMP}/mc-cam");
    let base = if count == 1 { format!("{out}.pgm") } else { out.clone() };
    let r = d.su(&format!("{TMP}/mc-grab {base} {exposure} {count} {gap_ms} 2>&1"))?;
    if r.contains("mc-grab:") { bail!("camera: {}", r.trim()); }
    (0..count).map(|n| {
        let p = if count == 1 { base.clone() } else { format!("{out}.{n}.pgm") };
        parse_pgm(&d.pull_bytes(&p)?)
    }).collect()
}

pub fn save_png(path: &Path, w: usize, h: usize, grey: &[u8]) -> Result<()> {
    let f = std::fs::File::create(path).with_context(|| path.display().to_string())?;
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Grayscale);
    e.write_header()?.write_image_data(grey)?;
    Ok(())
}

/// Share of pixels (0-100) whose brightness moved by more than a noise margin.
pub fn changed_pct(a: &Frame, b: &Frame) -> f32 {
    let n = a.px.iter().zip(&b.px).filter(|(x, y)| (**x as i16 - **y as i16).abs() > 18).count();
    n as f32 * 100.0 / a.px.len() as f32
}

/// Threshold for "something changed": above camera noise, below a toast's footprint.
pub const CHANGED: f32 = 1.5;
