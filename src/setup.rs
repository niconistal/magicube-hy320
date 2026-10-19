//! `magicube setup`: the whole clean-up as resumable steps, remembered per projector serial.
use crate::config::{cache_dir, State};
use crate::device::{Device, TMP};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    io::{self, BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
};

pub const STEPS: &[(&str, &str)] = &[
    ("check", "identify the projector"),
    ("backup", "full eMMC image to this computer (root)"),
    ("debloat", "switch off vendor OTA, store, telemetry, casting"),
    ("launcher", "FLauncher as the home screen (root)"),
    ("tailscale", "Tailscale as always-on VPN (optional)"),
    ("adb-tcp", "adb over Wi-Fi after every boot (root)"),
    ("verify", "reboot and check everything stuck"),
];

/// Vendor packages to switch off (`pm disable-user`, reversible). Kept on purpose:
/// com.android.toofifi (HDMI in), com.hysd.vafocus (autofocus), awtvinputservice, and the
/// keystone pages inside com.htc.magcubicos.
pub const BLOAT: &[(&str, &str)] = &[
    ("com.hy.hyotaupdate", "vendor OTA updater (phones home)"),
    ("com.hy.storeos", "vendor app store"),
    ("com.mphotool.dlna", "DLNA receiver"),
    ("com.mphotool.airplay", "AirPlay receiver"),
    ("com.mphotool.usbcastserver", "USB cast"),
    ("com.huawei.connection", "Huawei cast"),
    ("com.softwinner.miracastReceiver", "Miracast receiver"),
    ("com.htc.samescreen", "screen mirroring"),
    ("com.htc.closedialog", "vendor popup"),
    ("com.htc.clearmemory", "vendor task killer"),
    ("com.htc.screensaver", "vendor screensaver"),
    ("com.htc.hyk_test", "factory test"),
    ("com.htc.hyk_test_pcba", "factory test"),
    ("com.mobisystems.office", "preinstalled office suite"),
    ("com.google.android.katniss", "Google TV voice/search"),
    ("com.google.android.backdrop", "Google screensaver"),
    ("com.android.nfx", "vendor service"),
];

pub struct Apk { pub name: &'static str, pub pkg: &'static str, pub url: &'static str, pub sha256: &'static str }

pub const FLAUNCHER: Apk = Apk {
    name: "flauncher-0.18.0.apk",
    pkg: "me.efesser.flauncher",
    url: "https://gitlab.com/api/v4/projects/26632151/packages/generic/flauncher/0.18.0/flauncher-0.18.0.apk",
    sha256: "376fc528c880cdf7c7c04b2d3708c69f911144498218babb6300808b772af32e",
};
pub const TAILSCALE: Apk = Apk {
    name: "tailscale-android-universal-1.102.4.apk",
    pkg: "com.tailscale.ipn",
    url: "https://github.com/tailscale/tailscale-android/releases/download/1.102.4-t3caf7d9e7-g8fbef364a/tailscale-android-universal-1.102.4.apk",
    sha256: "7ecfb863e08f5fbd1ecd70235d8c34ba135a4124bdd8e166b9d6fb962782e0b5",
};

const VENDOR_MAIN: &str = "com.htc.magcubicos/com.htc.magcubicos.activity.MainActivity";
const ADB_TCP_SCRIPT: &str = include_str!("../device/adb-tcp.sh");

pub struct Opts {
    pub skip: Vec<String>,
    pub redo: Vec<String>,
    pub backup_dir: Option<PathBuf>,
    pub yes: bool,
}

pub fn run(mut d: Device, o: &Opts) -> Result<()> {
    let serial = d.prop("ro.serialno")?;
    let mut st = State::load(&serial);
    for r in &o.redo { st.clear(&serial, r)?; }
    println!("projector {serial} at {}", d.addr);
    for (step, what) in STEPS {
        if o.skip.iter().any(|s| s == step) { println!("·  {step:<10} skipped (--skip)"); continue; }
        if let Some(note) = st.done.get(*step) { println!("✓  {step:<10} {note}  (done earlier)"); continue; }
        println!("…  {step:<10} {what}");
        let note = match *step {
            "check" => check(&mut d, false)?,
            "backup" => backup(&mut d, o.backup_dir.clone(), &serial)?,
            "debloat" => debloat(&mut d, false)?,
            "launcher" => launcher(&mut d, None, false)?,
            "tailscale" => match tailscale(&mut d, o.yes)? { Some(n) => n, None => return Ok(()) },
            "adb-tcp" => adb_tcp(&mut d, false)?,
            "verify" => {
                if !o.yes && !confirm("Reboot the projector now to verify? (it may play a boot sound)")? {
                    println!("   stopped before the reboot. Run `magicube setup` again when ready.");
                    return Ok(());
                }
                d = d.reboot_and_wait()?;
                let fails = verify(&mut d)?;
                if fails > 0 { bail!("{fails} check(s) failed — fix and run `magicube setup --redo verify`"); }
                "all checks pass after reboot".into()
            }
            _ => unreachable!(),
        };
        println!("✓  {step:<10} {note}");
        st.mark(&serial, step, &note)?;
    }
    println!("\ndone. This projector is set up.");
    Ok(())
}

fn confirm(q: &str) -> Result<bool> {
    if !io::stdin().is_terminal() { return Ok(false); }
    print!("{q} [y/N] ");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().lock().read_line(&mut s)?;
    Ok(s.trim().eq_ignore_ascii_case("y") || s.trim().eq_ignore_ascii_case("yes"))
}

pub fn check(d: &mut Device, verbose: bool) -> Result<String> {
    let model = d.prop("ro.product.model")?;
    let plat = d.prop("ro.board.platform")?;
    let android = d.prop("ro.build.version.release")?;
    let root = d.has_root();
    if verbose {
        for p in ["ro.product.model", "ro.board.platform", "ro.serialno", "ro.build.version.release", "ro.build.type"] {
            println!("  {p:<26} {}", d.prop(p)?);
        }
        println!("  {:<26} {}", "root", if root { "yes" } else { "no" });
        println!("  {:<26} {}", "home screen", d.home_activity()?);
        println!("  {:<26} {}", "wifi ip", d.wlan_ip().unwrap_or_else(|| "-".into()));
    }
    if !plat.starts_with("sun50iw15") {
        bail!("platform is '{plat}', not Allwinner H726 (sun50iw15p1). magicube is written for the HY320 Pro; stopping.");
    }
    Ok(format!("{model} · H726 · Android {android} · {}", if root { "root" } else { "no root" }))
}

/// Full eMMC image, streamed and compressed on the fly, checked against the device size.
pub fn backup(d: &mut Device, dir: Option<PathBuf>, serial: &str) -> Result<String> {
    d.need_root()?;
    let date = d.sh("date +%F")?.trim().to_string();
    let dir = dir.unwrap_or_else(|| PathBuf::from(format!("backup-{serial}-{date}")));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("getprop.txt"), d.sh("getprop")?)?;
    std::fs::write(dir.join("partitions.txt"), d.su("ls -l /dev/block/by-name/")?)?;
    let size: u64 = d.su("blockdev --getsize64 /dev/block/mmcblk0")?.trim().parse().context("eMMC size")?;
    println!("   dumping {} MiB — keep the projector on (15-40 min over Wi-Fi)", size >> 20);

    struct Tee<W: Write> { z: W, h: Sha256, n: u64, total: u64, last: u64 }
    impl<W: Write> Write for Tee<W> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.z.write_all(b)?;
            self.h.update(b);
            self.n += b.len() as u64;
            if self.n - self.last > 64 << 20 {
                self.last = self.n;
                eprint!("\r   {:>3}%  {} / {} MiB", self.n * 100 / self.total.max(1), self.n >> 20, self.total >> 20);
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> { self.z.flush() }
    }
    let zpath = dir.join("emmc-full.img.zst");
    let f = std::fs::File::create(&zpath)?;
    let mut z = zstd::Encoder::new(f, 3)?;
    z.long_distance_matching(true)?;
    z.window_log(27)?;
    let mut t = Tee { z, h: Sha256::new(), n: 0, total: size, last: 0 };
    d.push_bytes(b"cat /dev/block/mmcblk0\n", &format!("{TMP}/mc-dump.sh"))?;
    d.sh_stream(&format!("su -c 'sh {TMP}/mc-dump.sh'"), &mut t)?;
    eprintln!();
    let (n, hash) = (t.n, hex(&t.h.finalize_reset()));
    t.z.finish()?;
    if n != size {
        bail!("got {n} bytes, expected {size}. The connection dropped; retry (Wi-Fi is steadier than the USB cable).");
    }
    std::fs::write(dir.join("emmc-full.img.sha256"), format!("{hash}  emmc-full.img\n"))?;
    std::fs::write(dir.join("RESTORE.txt"), format!(
        "Full eMMC image of {serial}, {date}.\n\
         Decompress: zstd -d --long=27 emmc-full.img.zst -o emmc-full.img\n\
         Check:      sha256sum emmc-full.img  ->  {hash}\n\
         Partition offsets: sgdisk -p emmc-full.img (names in partitions.txt).\n\
         Write single partitions back from Android's userspace fastbootd (adb reboot fastboot),\n\
         NEVER from the bootloader (adb reboot bootloader leaves this box stuck on the logo).\n"))?;
    let mb = std::fs::metadata(&zpath)?.len() >> 20;
    Ok(format!("{} ({mb} MiB, sha256 {}…) — copy it off this machine too", dir.display(), &hash[..12]))
}

pub fn debloat(d: &mut Device, undo: bool) -> Result<String> {
    let root = d.has_root();
    let (verb, want) = if undo { ("enable --user 0", "enabled") } else { ("disable-user --user 0", "disabled") };
    let (mut n, mut fail) = (0, Vec::new());
    for (p, what) in BLOAT {
        if !d.pkg_present(p)? { continue; }
        let mut r = d.sh(&format!("pm {verb} {p} 2>&1"))?;
        // some vendor packages refuse the shell user; root always works
        if !r.contains("new state") && root { r = d.su(&format!("pm {verb} {p} 2>&1"))?; }
        if r.contains("new state") { n += 1; println!("     {want:<8} {p}  ({what})"); } else { fail.push(*p); }
    }
    if !fail.is_empty() { println!("   couldn't change: {} (needs root?)", fail.join(", ")); }
    Ok(format!("{n} packages {want}"))
}

pub fn launcher(d: &mut Device, apk: Option<&Path>, undo: bool) -> Result<String> {
    d.need_root()?;
    if undo {
        d.su(&format!("pm enable {VENDOR_MAIN}\ncmd package set-home-activity {VENDOR_MAIN}"))?;
        d.key("KEYCODE_HOME")?;
        return Ok("vendor launcher restored".into());
    }
    if !d.pkg_present(FLAUNCHER.pkg)? {
        let path = match apk { Some(p) => p.to_path_buf(), None => fetch(&FLAUNCHER)? };
        d.install(&path)?;
    }
    d.su("cmd package set-home-activity me.efesser.flauncher/.MainActivity")?;
    // The vendor launcher is started directly at boot, not as HOME. Disable only its main screen
    // so the keystone/alignment pages inside the same app keep working.
    d.su(&format!("pm disable {VENDOR_MAIN}"))?;
    d.key("KEYCODE_HOME")?;
    let home = d.home_activity()?;
    if !home.contains("flauncher") { bail!("home is still {home}"); }
    Ok("FLauncher 0.18.0 is HOME".into())
}

/// Returns None when it stopped for the manual step.
pub fn tailscale(d: &mut Device, yes: bool) -> Result<Option<String>> {
    if !d.pkg_present(TAILSCALE.pkg)? {
        if !yes && !confirm("   Install Tailscale (reach the projector from anywhere on your tailnet)?")? {
            return Ok(Some("not wanted".into()));
        }
        let p = fetch(&TAILSCALE)?;
        d.install(&p)?;
    }
    let set = "settings put global always_on_vpn_app com.tailscale.ipn";
    if d.has_root() { d.su(set)?; } else { d.sh(set)?; }
    // lockdown blocks ALL traffic while the VPN is down — including your adb. Keep it off.
    if d.sh("settings get global always_on_vpn_lockdown")?.trim() == "1" {
        d.su("settings put global always_on_vpn_lockdown 0")?;
    }
    if d.sh("ps -A -o NAME")?.contains("tailscale") && d.sh("dumpsys connectivity | grep -c 'VPN CONNECTED' || true")?.trim() != "0" {
        return Ok(Some("Tailscale installed, always-on, connected".into()));
    }
    d.sh("monkey -p com.tailscale.ipn -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1")?;
    println!("   On the projector, with the remote:");
    println!("     1. sign in to Tailscale in the app that just opened");
    println!("     2. Settings > Network > VPN > Tailscale (gear) > Always-on VPN: ON");
    println!("        (the setting this tool wrote isn't enough; the toggle must be flipped once)");
    if io::stdin().is_terminal() {
        print!("   Press Enter when both are done (or type skip): ");
        io::stdout().flush()?;
        let mut s = String::new();
        io::stdin().lock().read_line(&mut s)?;
        if s.trim() == "skip" { return Ok(Some("installed; sign-in skipped".into())); }
        Ok(Some("Tailscale installed, always-on".into()))
    } else {
        println!("   Run `magicube setup` again afterwards; it continues from here.");
        Ok(None)
    }
}

pub fn adb_tcp(d: &mut Device, undo: bool) -> Result<String> {
    d.need_root()?;
    if undo {
        d.su("rm -f /data/adb/service.d/adb-tcp.sh")?;
        return Ok("boot script removed".into());
    }
    d.push_bytes(ADB_TCP_SCRIPT.as_bytes(), &format!("{TMP}/adb-tcp.sh"))?;
    d.su(&format!("mkdir -p /data/adb/service.d\ncp {TMP}/adb-tcp.sh /data/adb/service.d/adb-tcp.sh\nchmod 755 /data/adb/service.d/adb-tcp.sh"))?;
    let ip = d.wlan_ip().unwrap_or_else(|| "<ip>".into());
    Ok(format!("boot script installed; reach it at {ip}:5555"))
}

pub fn verify(d: &mut Device) -> Result<usize> {
    let mut fails = 0;
    let mut check = |name: &str, ok: bool| {
        println!("     {}  {name}", if ok { "PASS" } else { "FAIL" });
        if !ok { fails += 1; }
    };
    check("booted", d.prop("sys.boot_completed")? == "1");
    check("root (su)", d.has_root());
    check("adb over Wi-Fi on 5555", d.prop("service.adb.tcp.port")? == "5555");
    check("FLauncher is HOME", d.home_activity()?.contains("flauncher"));
    check("vendor OTA disabled", d.pkg_disabled("com.hy.hyotaupdate")?);
    check("vendor store disabled", d.pkg_disabled("com.hy.storeos")?);
    check("HDMI-in app still on", !d.pkg_disabled("com.android.toofifi")?);
    check("autofocus app still on", !d.pkg_disabled("com.hysd.vafocus")?);
    if d.pkg_present(TAILSCALE.pkg)? {
        check("Tailscale always-on", d.sh("settings get global always_on_vpn_app")?.contains("com.tailscale.ipn"));
        check("Tailscale running", d.sh("ps -A -o NAME")?.contains("tailscale"));
    }
    check("internet", d.sh_rc("ping -c 1 -W 3 1.1.1.1 >/dev/null")?.1 == 0);
    Ok(fails)
}

/// Download a pinned APK once into the cache, refusing anything whose sha256 doesn't match.
pub fn fetch(a: &Apk) -> Result<PathBuf> {
    let dir = cache_dir().join("apks");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(a.name);
    if path.exists() && hex(&Sha256::digest(std::fs::read(&path)?)) == a.sha256 { return Ok(path); }
    println!("   downloading {}", a.name);
    let mut body = Vec::new();
    io::copy(&mut ureq::get(a.url).call()?.into_reader(), &mut body)?;
    let got = hex(&Sha256::digest(&body));
    if got != a.sha256 { bail!("{}: checksum mismatch (got {got}); not installing", a.name); }
    std::fs::write(&path, body)?;
    Ok(path)
}

pub fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
