//! Projection mapping: learn which projector pixel lands on which camera pixel (Gray-code
//! patterns), then light up only an object you pick in the camera's view.
//!
//!   map        show ~34 stripe patterns, photograph each with the built-in camera, decode
//!   mask       pick the object as a box/quad in camera coordinates (open view.png to find them)
//!   light      project a colour onto the masked area only, black everywhere else
use crate::cam::{self, CH, CW};
use crate::config;
use crate::device::Device;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, thread, time::Duration};

const BLOCK: usize = 8; // projector pixels per code cell
const REMOTE: &str = "/sdcard/Pictures/magicube/map";
const NONE: u32 = u32::MAX;

#[derive(Serialize, Deserialize)]
struct Meta { proj_w: usize, proj_h: usize, block: usize, valid_pct: f32 }

fn dir(serial: &str) -> PathBuf { config::dir().join("map").join(serial) }

/// ceil(log2(n)): bits needed to number n cells
fn bits_for(n: usize) -> usize { (usize::BITS - (n - 1).leading_zeros()) as usize }

fn proj_size(d: &mut Device) -> Result<(usize, usize)> {
    // "Physical size: 1920x1080"
    let s = d.sh("wm size")?;
    let wh = s.split_whitespace().last().context("wm size")?;
    let (w, h) = wh.split_once('x').context("wm size")?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}

fn png_grey(w: usize, h: usize, px: &[u8]) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    {
        let mut e = png::Encoder::new(&mut v, w as u32, h as u32);
        e.set_color(png::ColorType::Grayscale);
        e.write_header()?.write_image_data(px)?;
    }
    Ok(v)
}

/// Pattern list: (name, pixels). Column bits, row bits, each as pattern + inverse, then white/black.
fn patterns(w: usize, h: usize) -> Vec<(String, Vec<u8>)> {
    let (cols, rows) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
    let mut out = Vec::new();
    for (axis, n) in [("x", cols), ("y", rows)] {
        for b in 0..bits_for(n) {
            for inv in [false, true] {
                let mut px = vec![0u8; w * h];
                for y in 0..h {
                    for x in 0..w {
                        let c = if axis == "x" { x / BLOCK } else { y / BLOCK };
                        let g = c ^ (c >> 1);
                        let on = ((g >> b) & 1 == 1) != inv;
                        px[y * w + x] = if on { 255 } else { 0 };
                    }
                }
                out.push((format!("{axis}{b}{}", if inv { "i" } else { "" }), px));
            }
        }
    }
    out.push(("white".into(), vec![255; w * h]));
    out.push(("black".into(), vec![0; w * h]));
    out
}

fn show_remote(d: &mut Device, name: &str) -> Result<()> {
    d.sh("am force-stop com.softwinner.imageParser")?;
    crate::media::am_start(d, &format!("-a android.intent.action.VIEW -d file://{REMOTE}/{name}.png -t image/png -n com.softwinner.imageParser/.MainActivity"))
}

pub fn map(d: &mut Device, exposure: u32) -> Result<()> {
    let serial = d.prop("ro.serialno")?;
    let (w, h) = proj_size(d)?;
    let pats = patterns(w, h);
    println!("pushing {} patterns ({w}x{h}, {BLOCK}px cells)", pats.len());
    d.sh(&format!("rm -rf {REMOTE}; mkdir -p {REMOTE}"))?;
    for (name, px) in &pats { d.push_bytes(&png_grey(w, h, px)?, &format!("{REMOTE}/{name}.png"))?; }
    println!("keep the room as it is and don't move the projector (~{} min)", pats.len() * 4 / 60 + 1);
    let mut frames = std::collections::HashMap::new();
    for (i, (name, _)) in pats.iter().enumerate() {
        show_remote(d, name)?;
        thread::sleep(Duration::from_millis(1200));
        frames.insert(name.clone(), cam::grab(d, exposure, 1, 0)?.remove(0));
        eprint!("\r  {}/{}", i + 1, pats.len());
    }
    eprintln!();
    d.key("KEYCODE_HOME")?;

    let (cols, rows) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
    let (white, black) = (&frames["white"], &frames["black"]);
    let decode = |axis: &str, n: usize, x: usize, y: usize| -> Option<usize> {
        let mut g = 0usize;
        for b in 0..bits_for(n) {
            let (p, ni) = (frames[&format!("{axis}{b}")].at(x, y) as i16, frames[&format!("{axis}{b}i")].at(x, y) as i16);
            if (p - ni).abs() < 8 { return None; }
            if p > ni { g |= 1 << b; }
        }
        // Gray -> binary
        let mut c = g;
        let mut s = g >> 1;
        while s > 0 { c ^= s; s >>= 1; }
        (c < n).then_some(c)
    };
    let mut m = vec![NONE; CW * CH];
    let mut valid = 0;
    for y in 0..CH {
        for x in 0..CW {
            if (white.at(x, y) as i16 - black.at(x, y) as i16) < 25 { continue; }
            if let (Some(c), Some(r)) = (decode("x", cols, x, y), decode("y", rows, x, y)) {
                m[y * CW + x] = ((c as u32) << 16) | r as u32;
                valid += 1;
            }
        }
    }
    let pct = valid as f32 * 100.0 / (CW * CH) as f32;
    let dd = dir(&serial);
    std::fs::create_dir_all(&dd)?;
    std::fs::write(dd.join("map.bin"), m.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())?;
    std::fs::write(dd.join("meta.json"), serde_json::to_string(&Meta { proj_w: w, proj_h: h, block: BLOCK, valid_pct: pct })?)?;
    cam::save_png(&dd.join("view.png"), CW, CH, &white.px)?;
    println!("mapped {pct:.0}% of the camera view (the rest is outside the picture or too dark)");
    println!("camera view: {}  — find your object's corners there, then `magicube mask --box x,y,w,h`", dd.join("view.png").display());
    if pct < 10.0 { println!("that's low: darken the room or raise --exposure"); }
    Ok(())
}

fn load(serial: &str) -> Result<(Meta, Vec<u32>)> {
    let dd = dir(serial);
    let meta: Meta = serde_json::from_str(&std::fs::read_to_string(dd.join("meta.json")).context("no map yet: run `magicube map` first")?)?;
    let raw = std::fs::read(dd.join("map.bin"))?;
    Ok((meta, raw.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect()))
}

/// Point-in-polygon (even-odd).
fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi { c = !c; }
        j = i;
    }
    c
}

/// `pts`: 4 corners (quad) in camera coordinates of view.png.
pub fn mask(d: &mut Device, pts: &[(f32, f32)]) -> Result<PathBuf> {
    let serial = d.prop("ro.serialno")?;
    let (meta, m) = load(&serial)?;
    let (cols, rows) = (meta.proj_w.div_ceil(meta.block), meta.proj_h.div_ceil(meta.block));
    let mut cells = vec![false; cols * rows];
    let mut hits = 0;
    for y in 0..CH {
        for x in 0..CW {
            let v = m[y * CW + x];
            if v == NONE || !inside(pts, x as f32 + 0.5, y as f32 + 0.5) { continue; }
            let (c, r) = ((v >> 16) as usize, (v & 0xffff) as usize);
            cells[r * cols + c] = true;
            hits += 1;
        }
    }
    if hits == 0 { bail!("no mapped camera pixels inside that area — check the coordinates against view.png"); }
    // close small gaps: the camera is lower-res than the projector, so neighbours go missing
    let grow = |src: &[bool], want: bool| -> Vec<bool> {
        let mut out = src.to_vec();
        for r in 0..rows { for c in 0..cols {
            let any = (r.saturating_sub(1)..=(r + 1).min(rows - 1))
                .any(|rr| (c.saturating_sub(1)..=(c + 1).min(cols - 1)).any(|cc| src[rr * cols + cc] == want));
            if any { out[r * cols + c] = want; }
        }}
        out
    };
    let cells = grow(&grow(&cells, true), false);
    let (w, h) = (meta.proj_w, meta.proj_h);
    let mut px = vec![0u8; w * h];
    for y in 0..h { for x in 0..w { if cells[(y / meta.block) * cols + x / meta.block] { px[y * w + x] = 255; } } }
    let path = dir(&serial).join("mask.png");
    std::fs::write(&path, png_grey(w, h, &px)?)?;
    println!("mask: {} cells lit ({hits} camera pixels) -> {}", cells.iter().filter(|c| **c).count(), path.display());
    Ok(path)
}

pub fn color(name: &str) -> Result<[u8; 3]> {
    Ok(match name {
        "red" => [255, 0, 0], "green" => [0, 255, 0], "blue" => [0, 0, 255], "white" => [255, 255, 255],
        "warm" => [255, 170, 80], "purple" => [160, 60, 255], "yellow" => [255, 220, 0], "cyan" => [0, 220, 255],
        h if h.len() == 7 && h.starts_with('#') => {
            let v = u32::from_str_radix(&h[1..], 16).context("bad hex colour")?;
            [(v >> 16) as u8, (v >> 8) as u8, v as u8]
        }
        _ => bail!("colour: red green blue white warm purple yellow cyan, or #rrggbb"),
    })
}

/// Render the mask in a colour and show it.
pub fn light(d: &mut Device, rgb: [u8; 3], mask: Option<PathBuf>) -> Result<()> {
    let serial = d.prop("ro.serialno")?;
    let path = mask.unwrap_or_else(|| dir(&serial).join("mask.png"));
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).with_context(|| format!("{}: run `magicube mask` first", path.display()))?));
    let mut r = dec.read_info()?;
    let mut buf = vec![0; r.output_buffer_size().context("mask size")?];
    let info = r.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    let step = info.line_size / w;
    let mut rgbpx = vec![0u8; w * h * 3];
    for y in 0..h { for x in 0..w {
        if buf[y * info.line_size + x * step] > 127 { rgbpx[(y * w + x) * 3..][..3].copy_from_slice(&rgb); }
    }}
    let mut v = Vec::new();
    {
        let mut e = png::Encoder::new(&mut v, w as u32, h as u32);
        e.set_color(png::ColorType::Rgb);
        e.write_header()?.write_image_data(&rgbpx)?;
    }
    let tmp = std::env::temp_dir().join("magicube-light.png");
    std::fs::write(&tmp, v)?;
    crate::media::show(d, &tmp, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_code_round_trips() {
        for c in 0..240usize {
            let g = c ^ (c >> 1);
            let (mut b, mut s) = (g, g >> 1);
            while s > 0 { b ^= s; s >>= 1; }
            assert_eq!(b, c);
        }
    }

    #[test]
    fn bits() {
        assert_eq!(bits_for(240), 8);
        assert_eq!(bits_for(135), 8);
        assert_eq!(bits_for(256), 8);
        assert_eq!(bits_for(257), 9);
    }

    #[test]
    fn polygon() {
        let sq = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(inside(&sq, 5.0, 5.0));
        assert!(!inside(&sq, 15.0, 5.0));
    }

    #[test]
    fn pattern_count() {
        // 1920x1080 at 8px: 8 column bits + 8 row bits, x2 for inverses, + white + black
        assert_eq!(patterns(64, 32).len(), 3 * 2 + 2 * 2 + 2);
    }
}
