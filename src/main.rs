//! magicube — set up and control Magcubic HY320 Pro projectors (Allwinner H726, Android 14).
//! Speaks the adb protocol itself: no adb install needed.
mod cam;
mod config;
mod device;
mod map;
mod media;
mod scan;
mod setup;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use device::Device;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "magicube", version, about = "Set up and control Magcubic HY320 Pro projectors over adb")]
struct Cli {
    /// Projector to use: a saved name, an IP, IP:port or "usb". Default: the saved default.
    #[arg(short, long, global = true, env = "MAGICUBE")]
    projector: Option<String>,
    /// Run the command on every saved projector
    #[arg(long, global = true)]
    all: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Find projectors on your network
    Scan {
        /// Scan this /24 instead of your own (any address in it, e.g. 192.168.1.0)
        #[arg(long)]
        subnet: Option<std::net::Ipv4Addr>,
        /// List every approved adb device, not just HY320 projectors
        #[arg(long)]
        any: bool,
    },
    /// Save a projector under a name (and make it the default if it's the first)
    Name { address: String, name: String },
    /// Forget a saved name
    Forget { name: String },
    /// Show or set the default projector
    Default { target: Option<String> },
    /// List saved projectors
    List,
    /// Power, foreground app, IP, volume, root, home screen
    Status,

    /// Set up a projector from scratch: backup, debloat, launcher, Tailscale, adb over Wi-Fi, verify.
    /// Resumable: finished steps are remembered per projector and skipped.
    Setup {
        /// Skip steps (check backup debloat launcher tailscale adb-tcp verify)
        #[arg(long, value_delimiter = ',')]
        skip: Vec<String>,
        /// Run these steps again even if done
        #[arg(long, value_delimiter = ',')]
        redo: Vec<String>,
        /// Where to write the eMMC backup (default ./backup-<serial>-<date>)
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        /// Don't ask (installs Tailscale, reboots for verify)
        #[arg(short, long)]
        yes: bool,
        /// List the steps and what's done on this projector
        #[arg(long)]
        plan: bool,
    },
    /// Run one setup step on its own (with --undo where it can be undone)
    Step {
        /// check backup debloat launcher tailscale adb-tcp verify
        step: String,
        #[arg(long)]
        undo: bool,
        /// Use this APK instead of downloading (launcher)
        #[arg(long)]
        apk: Option<PathBuf>,
        #[arg(long)]
        backup_dir: Option<PathBuf>,
    },

    /// Show a picture full-screen (1920x1080 looks best)
    Show {
        image: PathBuf,
        /// Use the camera to confirm the picture changed
        #[arg(long)]
        verify: bool,
    },
    /// Play a video in the built-in player
    Play {
        video: PathBuf,
        /// Repeat it (needs ffmpeg here; silent unless --sound)
        #[arg(long = "loop")]
        looped: bool,
        /// How long the loop lasts (default 60m, or --off-in)
        #[arg(long = "for")]
        for_: Option<String>,
        /// Keep the sound when looping
        #[arg(long)]
        sound: bool,
        /// Turn the picture off after this long (30m, 2h, 90s)
        #[arg(long)]
        off_in: Option<String>,
        /// Use the camera to confirm the video is moving on the wall
        #[arg(long)]
        verify: bool,
    },
    /// Cycle through a folder of pictures (runs on the projector; `stop` ends it)
    Slideshow {
        dir: PathBuf,
        /// Seconds per picture (20s, 1m)
        #[arg(long, default_value = "20s")]
        every: String,
        #[arg(long)]
        off_in: Option<String>,
    },
    /// Turn the picture off after a while ("cancel" to cancel)
    OffIn { duration: String },
    /// Close the player/viewer/slideshow and go home
    Stop,
    /// Save a camera frame as PNG (what's really on the wall)
    See {
        out: PathBuf,
        /// Camera exposure (raise in a bright room is wrong: raise it in a DARK room)
        #[arg(long, default_value_t = 130)]
        exposure: u32,
    },
    /// Learn where the picture lands in the camera view (projection mapping, ~2-3 min)
    Map {
        #[arg(long, default_value_t = 130)]
        exposure: u32,
    },
    /// Pick the object to light, in camera coordinates of the view.png that `map` saved
    Mask {
        /// x,y,w,h
        #[arg(long, conflicts_with = "quad")]
        r#box: Option<String>,
        /// x1,y1,x2,y2,x3,y3,x4,y4
        #[arg(long)]
        quad: Option<String>,
    },
    /// Light only the masked object in a colour (red, warm, #ff8800 …)
    Light {
        #[arg(default_value = "white")]
        color: String,
        #[arg(long)]
        mask: Option<PathBuf>,
    },

    /// Get or set media volume (setting needs root)
    Vol { level: Option<u32> },
    /// Send a key (HOME, BACK, DPAD_UP, MEDIA_PLAY_PAUSE, or a number)
    Key { key: String },
    /// Type text into the focused field
    Text { text: String },
    /// Picture on
    Wake,
    /// Picture off (standby)
    Sleep,
    /// Built-in autofocus + auto keystone
    Focus,
    /// Save or load the keystone corners
    Keystone {
        /// save | load
        action: String,
        file: PathBuf,
    },
    /// Switch to the HDMI input
    Hdmi,
    /// Launch an app by package name
    App { package: String },
    /// List installed (non-system) apps
    Apps,
    /// Screenshot of the UI layer. NOT proof of what's projected; use `see`
    Shot { out: PathBuf },
    /// Run a shell command
    Shell { cmd: Vec<String> },
    /// Run a command as root (quoting kept intact)
    Su { cmd: Vec<String> },
    /// Normal reboot and wait until it's back (never to the bootloader)
    Reboot {
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("magicube: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // exit quietly when piped into `head` instead of panicking
    #[cfg(unix)]
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL); }
    let cli = Cli::parse();
    let mut cfg = Config::load()?;

    // commands that don't talk to one projector
    match &cli.cmd {
        Cmd::Scan { subnet, any } => {
            let found = scan::scan(*subnet, *any);
            if found.is_empty() { println!("no projectors answering adb on :5555 (adb over Wi-Fi on? same network?)"); }
            for f in found {
                let name = cfg.name_of(&f.addr).map(|n| n.to_string()).unwrap_or_else(|| "(new)".into());
                println!("  {:<22} {:<12} {:<18} {name}", f.addr, f.model, f.serial);
            }
            return Ok(());
        }
        Cmd::Name { address, name } => {
            let addr = device::normalize(address);
            Device::connect(&addr)?;
            cfg.names.insert(name.clone(), addr.clone());
            if cfg.default.is_none() { cfg.default = Some(name.clone()); }
            cfg.save()?;
            println!("{name} = {addr}");
            return Ok(());
        }
        Cmd::Forget { name } => {
            if cfg.names.remove(name).is_none() { bail!("no projector named {name}"); }
            if cfg.default.as_deref() == Some(name) { cfg.default = None; }
            return cfg.save();
        }
        Cmd::Default { target: None } => {
            println!("{}", cfg.default.as_deref().unwrap_or("(none) — magicube default <name|ip>"));
            return Ok(());
        }
        Cmd::Default { target: Some(t) } => {
            cfg.default = Some(t.clone());
            return cfg.save();
        }
        Cmd::List => {
            for (n, a) in &cfg.names {
                println!("  {}{n:<12} {a}", if cfg.default.as_deref() == Some(n) { "*" } else { " " });
            }
            return Ok(());
        }
        _ => {}
    }

    let targets: Vec<String> = if cli.all {
        if cfg.names.is_empty() { bail!("no saved projectors (magicube name <ip> <name>)"); }
        cfg.names.values().cloned().collect()
    } else {
        let t = cli.projector.clone().or_else(|| cfg.default.clone())
            .context("which projector? use -p <ip|name>, or save one: magicube name <ip> <name>")?;
        vec![cfg.resolve(&t)]
    };
    let mut failed = 0;
    for t in &targets {
        if targets.len() > 1 { println!("== {} ({t})", cfg.name_of(t).unwrap_or("")); }
        let r = Device::connect(t).and_then(|d| one(d, &cli.cmd));
        if let Err(e) = r {
            if targets.len() == 1 { return Err(e); }
            eprintln!("   {e:#}");
            failed += 1;
        }
    }
    if failed > 0 { bail!("{failed} of {} projectors failed", targets.len()); }
    Ok(())
}

fn one(mut d: Device, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Status => {
            let ver = d.sh("cmd media_session volume --stream 3 --get 2>/dev/null")?;
            let vol = ver.split("volume is ").nth(1).and_then(|s| s.split_whitespace().next()).unwrap_or("?").to_string();
            let power = d.sh("dumpsys power 2>/dev/null | grep -m1 mWakefulness=")?;
            let focus = d.sh("dumpsys window 2>/dev/null | grep -m1 mCurrentFocus")?;
            println!("address     {}", d.addr);
            println!("model       {} (Android {}) {}", d.prop("ro.product.model")?, d.prop("ro.build.version.release")?, d.prop("ro.serialno")?);
            println!("power       {}", power.trim().trim_start_matches("mWakefulness="));
            println!("on screen   {}", focus.trim().rsplit(' ').next().unwrap_or("").trim_end_matches('}'));
            println!("uptime      {}", d.sh("uptime")?.trim());
            println!("wifi ip     {}", d.wlan_ip().unwrap_or_else(|| "-".into()));
            println!("volume      {vol}");
            println!("root        {}", if d.has_root() { "yes" } else { "no" });
            println!("home screen {}", d.home_activity()?);
        }
        Cmd::Setup { skip, redo, backup_dir, yes, plan } => {
            if *plan {
                let serial = d.prop("ro.serialno")?;
                let st = config::State::load(&serial);
                for (s, what) in setup::STEPS {
                    match st.done.get(*s) {
                        Some(n) => println!("✓  {s:<10} {n}"),
                        None => println!("   {s:<10} {what}"),
                    }
                }
                return Ok(());
            }
            setup::run(d, &setup::Opts { skip: skip.clone(), redo: redo.clone(), backup_dir: backup_dir.clone(), yes: *yes })?;
        }
        Cmd::Step { step, undo, apk, backup_dir } => {
            let note = match step.as_str() {
                "check" => setup::check(&mut d, true)?,
                "backup" => { let s = d.prop("ro.serialno")?; setup::backup(&mut d, backup_dir.clone(), &s)? }
                "debloat" => setup::debloat(&mut d, *undo)?,
                "launcher" => setup::launcher(&mut d, apk.as_deref(), *undo)?,
                "tailscale" => setup::tailscale(&mut d, true)?.unwrap_or_default(),
                "adb-tcp" => setup::adb_tcp(&mut d, *undo)?,
                "verify" => { let f = setup::verify(&mut d)?; if f > 0 { bail!("{f} check(s) failed"); } "all pass".into() }
                s => bail!("unknown step {s}; one of: check backup debloat launcher tailscale adb-tcp verify"),
            };
            println!("{note}");
        }
        Cmd::Show { image, verify } => media::show(&mut d, image, *verify)?,
        Cmd::Play { video, looped, for_, sound, off_in, verify } => {
            let o = media::PlayOpts {
                looped: *looped,
                for_: for_.as_deref().map(media::parse_dur).transpose()?,
                sound: *sound,
                off_in: off_in.as_deref().map(media::parse_dur).transpose()?,
                verify: *verify,
            };
            media::play(&mut d, video, &o)?;
        }
        Cmd::Slideshow { dir, every, off_in } => {
            let every = media::parse_dur(every.strip_suffix('s').map(|n| format!("{n}s")).as_deref().unwrap_or(every))?;
            media::slideshow(&mut d, dir, every, off_in.as_deref().map(media::parse_dur).transpose()?)?;
        }
        Cmd::OffIn { duration } => {
            if duration == "cancel" { media::cancel_off(&mut d)?; println!("timer cancelled"); }
            else { media::off_in(&mut d, media::parse_dur(duration)?)?; }
        }
        Cmd::Stop => media::stop(&mut d)?,
        Cmd::See { out, exposure } => {
            let f = cam::grab(&mut d, *exposure, 1, 0)?.remove(0);
            cam::save_png(out, cam::CW, cam::CH, &f.px)?;
            println!("wrote {} ({}x{}, brightness {:.0})", out.display(), cam::CW, cam::CH, f.mean());
        }
        Cmd::Map { exposure } => map::map(&mut d, *exposure)?,
        Cmd::Mask { r#box, quad } => {
            let nums = |s: &str| -> Result<Vec<f32>> { s.split(',').map(|v| v.trim().parse::<f32>().context("numbers, comma-separated")).collect() };
            let pts = match (r#box, quad) {
                (Some(b), _) => {
                    let v = nums(b)?;
                    if v.len() != 4 { bail!("--box x,y,w,h"); }
                    vec![(v[0], v[1]), (v[0] + v[2], v[1]), (v[0] + v[2], v[1] + v[3]), (v[0], v[1] + v[3])]
                }
                (None, Some(q)) => {
                    let v = nums(q)?;
                    if v.len() != 8 { bail!("--quad x1,y1,x2,y2,x3,y3,x4,y4"); }
                    v.chunks(2).map(|c| (c[0], c[1])).collect()
                }
                _ => bail!("give --box x,y,w,h or --quad x1,y1,...,x4,y4 (camera coordinates in view.png)"),
            };
            map::mask(&mut d, &pts)?;
        }
        Cmd::Light { color, mask } => map::light(&mut d, map::color(color)?, mask.clone())?,
        Cmd::Vol { level: None } => print!("{}", d.sh("cmd media_session volume --stream 3 --get")?),
        Cmd::Vol { level: Some(l) } => {
            // without root this returns OK but changes nothing on this firmware
            d.need_root()?;
            d.su(&format!("cmd media_session volume --stream 3 --set {l}"))?;
            print!("{}", d.sh("cmd media_session volume --stream 3 --get")?);
        }
        Cmd::Key { key } => {
            let k = if key.chars().all(|c| c.is_ascii_digit()) || key.starts_with("KEYCODE_") { key.clone() }
                    else { format!("KEYCODE_{}", key.to_ascii_uppercase()) };
            d.key(&k)?;
        }
        Cmd::Text { text } => { d.sh(&format!("input text {}", device::q(&text.replace(' ', "%s"))))?; }
        Cmd::Wake => d.key("KEYCODE_WAKEUP")?,
        Cmd::Sleep => d.key("KEYCODE_SLEEP")?,
        Cmd::Focus => {
            let (_, rc) = d.sh_rc("am start -n com.hysd.vafocus/.VajzActivity >/dev/null 2>&1")?;
            if rc != 0 { d.su("am start -n com.hysd.vafocus/.VajzActivity")?; }
        }
        Cmd::Keystone { action, file } => match action.as_str() {
            "save" => {
                let lines: Vec<String> = d.sh("getprop")?.lines().filter(|l| l.starts_with("[persist.display.keystone"))
                    .filter_map(|l| { let (k, v) = l.split_once("]: [")?; Some(format!("{}={}", &k[1..], v.trim_end_matches(']'))) })
                    .collect();
                std::fs::write(file, lines.join("\n") + "\n")?;
                println!("saved {} values to {}", lines.len(), file.display());
            }
            "load" => {
                d.need_root()?;
                let mut script = String::new();
                for l in std::fs::read_to_string(file)?.lines() {
                    let Some((k, v)) = l.split_once('=') else { continue };
                    if !k.starts_with("persist.display.keystone") || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') { continue; }
                    script += &format!("setprop {k} {}\n", device::q(v));
                }
                d.su(&script)?;
                println!("restored. Reboot if the picture doesn't update.");
            }
            _ => bail!("keystone save|load <file>"),
        },
        Cmd::Hdmi => {
            let o = d.sh("monkey -p com.android.toofifi -c android.intent.category.LAUNCHER 1 2>&1")?;
            if !o.contains("Events injected") { bail!("HDMI-in app (com.android.toofifi) didn't start — is it disabled?"); }
        }
        Cmd::App { package } => {
            let o = d.sh(&format!("monkey -p {} -c android.intent.category.LAUNCHER 1 2>&1", device::q(package)))?;
            if !o.contains("Events injected") {
                let o = d.sh(&format!("monkey -p {} -c android.intent.category.LEANBACK_LAUNCHER 1 2>&1", device::q(package)))?;
                if !o.contains("Events injected") { bail!("couldn't launch {package}"); }
            }
        }
        Cmd::Apps => {
            let mut v: Vec<String> = d.sh("pm list packages -3")?.lines().filter_map(|l| l.strip_prefix("package:")).map(|s| s.trim().to_string()).collect();
            v.sort();
            println!("{}", v.join("\n"));
        }
        Cmd::Shot { out } => {
            let png = { d.sh("screencap -p /data/local/tmp/mc-shot.png")?; d.pull_bytes("/data/local/tmp/mc-shot.png")? };
            std::fs::write(out, png)?;
            println!("wrote {} (UI layer only — videos may not match the wall; use `see`)", out.display());
        }
        Cmd::Shell { cmd } => {
            if cmd.is_empty() { bail!("magicube shell <command>  (interactive shells: use adb)"); }
            print!("{}", d.sh(&cmd.join(" "))?);
        }
        Cmd::Su { cmd } => { d.need_root()?; print!("{}", d.su(&cmd.join(" "))?); }
        Cmd::Reboot { yes } => {
            if !yes {
                use std::io::{BufRead, Write};
                print!("Reboot {}? It may play a boot sound. [y/N] ", d.addr);
                std::io::stdout().flush()?;
                let mut s = String::new();
                std::io::stdin().lock().read_line(&mut s)?;
                if !s.trim().eq_ignore_ascii_case("y") { return Ok(()); }
            }
            d.reboot_and_wait()?;
        }
        Cmd::Scan { .. } | Cmd::Name { .. } | Cmd::Forget { .. } | Cmd::Default { .. } | Cmd::List => unreachable!(),
    }
    Ok(())
}
