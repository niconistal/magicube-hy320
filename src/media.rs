//! Pictures, videos, loops, slideshows and the off timer.
use crate::cam;
use crate::device::{q, Device, TMP};
use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command, thread, time::Duration};

const VIEWER: &str = "com.softwinner.imageParser";
const PLAYER: &str = "com.softwinner.TvdVideo";
const PIC_DIR: &str = "/sdcard/Pictures/magicube";
const MOV_DIR: &str = "/sdcard/Movies";

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

fn file_name(p: &Path) -> Result<String> {
    Ok(p.file_name().context("not a file")?.to_string_lossy().to_string())
}

/// "30m", "90s", "2h", "45" (minutes) -> seconds
pub fn parse_dur(s: &str) -> Result<u64> {
    let (n, unit) = s.trim().split_at(s.trim().find(|c: char| !c.is_ascii_digit()).unwrap_or(s.trim().len()));
    let n: u64 = n.parse().with_context(|| format!("bad duration {s:?} (try 30m, 90s, 2h)"))?;
    Ok(match unit { "s" => n, "" | "m" | "min" => n * 60, "h" => n * 3600, _ => bail!("bad duration unit in {s:?}") })
}

/// `am start` exits 0 even when the activity doesn't exist; read what it says instead.
pub fn am_start(d: &mut Device, args: &str) -> Result<()> {
    let out = d.sh(&format!("am start {args} 2>&1"))?;
    if out.lines().any(|l| l.starts_with("Error")) {
        let msg: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("Error: ")).collect();
        let msg = if msg.is_empty() { out.trim().to_string() } else { msg.join(" ") };
        bail!("couldn't open it: {}", msg.trim());
    }
    Ok(())
}

fn open_picture(d: &mut Device, remote: &str, mime: &str) -> Result<()> {
    // the viewer is singleTask: it ignores a new picture unless it is closed first
    d.sh(&format!("am force-stop {VIEWER}"))?;
    am_start(d, &format!("-a android.intent.action.VIEW -d {} -t {mime} -n {VIEWER}/.MainActivity", q(&format!("file://{remote}"))))
}

pub fn show(d: &mut Device, img: &Path, verify: bool) -> Result<()> {
    let before = if verify { Some(cam::grab(d, 130, 1, 0)?.remove(0)) } else { None };
    let remote = format!("{PIC_DIR}/{}", file_name(img)?);
    d.sh(&format!("mkdir -p {PIC_DIR}"))?;
    d.push_file(img, &remote)?;
    open_picture(d, &remote, mime(img))?;
    if let Some(a) = before {
        thread::sleep(Duration::from_millis(2500));
        let b = cam::grab(d, 130, 1, 0)?.remove(0);
        let pct = cam::changed_pct(&a, &b);
        if pct > cam::CHANGED { println!("✓ camera: picture changed ({pct:.0}% of the frame)"); }
        else { bail!("camera saw no change ({pct:.1}%). Same picture already up, or the viewer didn't open"); }
    }
    Ok(())
}

pub struct PlayOpts { pub looped: bool, pub for_: Option<u64>, pub sound: bool, pub off_in: Option<u64>, pub verify: bool }

pub fn play(d: &mut Device, video: &Path, o: &PlayOpts) -> Result<()> {
    let mut src = video.to_path_buf();
    let _tmp;
    if o.looped {
        // The built-in player can't loop. Repeat the clip into one long file (no re-encode).
        let total = o.for_.or(o.off_in).unwrap_or(3600);
        let dir = std::env::temp_dir().join(format!("magicube-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let out = dir.join(format!("loop-{}", file_name(video)?));
        make_loop(video, &out, total, o.sound)?;
        src = out;
        _tmp = TmpDir(dir);
    }
    let remote = format!("{MOV_DIR}/{}", file_name(&src)?);
    d.push_file(&src, &remote)?;
    // Restarting the player straight away freezes the hardware video layer on the previous clip
    // while screenshots show the new one. Go HOME and let it settle first.
    d.sh(&format!("am force-stop {PLAYER}; am force-stop {VIEWER}; input keyevent 3"))?;
    thread::sleep(Duration::from_secs(3));
    am_start(d, &format!("-a android.intent.action.VIEW -d {} -t video/mp4 -n {PLAYER}/.TvdVideoActivity", q(&format!("file://{remote}"))))?;
    if let Some(s) = o.off_in { off_in(d, s)?; }
    if o.verify { verify_motion(d)?; }
    Ok(())
}

pub fn verify_motion(d: &mut Device) -> Result<()> {
    thread::sleep(Duration::from_secs(4));
    let f = cam::grab(d, 130, 2, 800)?;
    let pct = cam::changed_pct(&f[0], &f[1]);
    if pct > cam::CHANGED { println!("✓ camera: video is moving ({pct:.0}% of the frame changed in 0.8 s)"); Ok(()) }
    else { bail!("camera saw no motion ({pct:.1}%). Frozen video layer? Try `magicube stop` then play again") }
}

struct TmpDir(std::path::PathBuf);
impl Drop for TmpDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn make_loop(input: &Path, out: &Path, total_s: u64, sound: bool) -> Result<()> {
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(input).output()
        .context("--loop needs ffmpeg/ffprobe on this computer")?;
    let clip: f64 = String::from_utf8_lossy(&probe.stdout).trim().parse().context("couldn't read the video's length")?;
    let reps = ((total_s as f64 / clip.max(0.5)).ceil() as u64).max(1);
    println!("looping {:.0}s clip {reps}x (~{} min){}", clip, total_s / 60, if sound { "" } else { ", silent" });
    let mut c = Command::new("ffmpeg");
    c.args(["-v", "error", "-y", "-stream_loop", &(reps - 1).to_string(), "-i"]).arg(input).args(["-c", "copy"]);
    if !sound { c.arg("-an"); }
    if !c.arg(out).status()?.success() { bail!("ffmpeg failed to build the loop"); }
    Ok(())
}

/// Device-side background job: survives this command exiting. `magicube stop` ends it.
fn background(d: &mut Device, name: &str, script: &str) -> Result<()> {
    let path = format!("{TMP}/{name}.sh");
    kill_job(d, name)?;
    d.push_bytes(script.as_bytes(), &path)?;
    // Own session (so its pid is also its process group), and a moment to detach: if the adb
    // shell closes first, the job dies with it.
    d.sh(&format!("setsid sh {path} >/dev/null 2>&1 </dev/null & echo $! > {path}.pid; sleep 1"))?;
    Ok(())
}

/// Kill a background job and whatever it's running (e.g. its `sleep`).
fn kill_job(d: &mut Device, name: &str) -> Result<()> {
    let pid = format!("{TMP}/{name}.sh.pid");
    d.sh(&format!("[ -f {pid} ] && kill -- -$(cat {pid}) 2>/dev/null; rm -f {pid}; true"))?;
    Ok(())
}

pub fn off_in(d: &mut Device, secs: u64) -> Result<()> {
    background(d, "mc-off", &format!("sleep {secs}\nam force-stop {PLAYER}\nam force-stop {VIEWER}\ninput keyevent KEYCODE_SLEEP\n"))?;
    println!("picture goes off in {} min", secs.div_ceil(60));
    Ok(())
}

pub fn cancel_off(d: &mut Device) -> Result<()> {
    kill_job(d, "mc-off")
}

pub fn slideshow(d: &mut Device, dir: &Path, every_s: u64, off: Option<u64>) -> Result<()> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "webp")))
        .collect();
    files.sort();
    if files.is_empty() { bail!("no pictures in {}", dir.display()); }
    let rdir = format!("{PIC_DIR}/slideshow");
    d.sh(&format!("rm -rf {rdir}; mkdir -p {rdir}"))?;
    for f in &files { d.push_file(f, &format!("{rdir}/{}", file_name(f)?))?; }
    println!("{} pictures, every {every_s}s", files.len());
    background(d, "mc-slideshow", &format!(
        "while true; do for f in {rdir}/*; do\n\
         am force-stop {VIEWER}\n\
         case \"$f\" in *.jpg|*.jpeg|*.JPG) t=image/jpeg;; *) t=image/png;; esac\n\
         am start -a android.intent.action.VIEW -d \"file://$f\" -t $t -n {VIEWER}/.MainActivity >/dev/null 2>&1\n\
         sleep {every_s}\ndone; done\n"))?;
    if let Some(s) = off { off_in(d, s)?; }
    Ok(())
}

pub fn stop(d: &mut Device) -> Result<()> {
    kill_job(d, "mc-slideshow")?;
    d.sh(&format!("am force-stop {PLAYER}; am force-stop {VIEWER}; input keyevent 3"))?;
    Ok(())
}
