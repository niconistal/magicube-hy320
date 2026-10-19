//! One connection to a projector, speaking the adb protocol directly (no adb install).
use adb_client::{tcp::ADBTcpDevice, ADBDeviceExt, RebootType};
use anyhow::{anyhow, bail, Context, Result};
use std::{
    io::Write,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub const TMP: &str = "/data/local/tmp";

pub struct Device {
    pub addr: String,
    inner: Box<dyn ADBDeviceExt>,
}

/// "192.168.1.50" -> 192.168.1.50:5555; "usb" stays "usb".
pub fn normalize(addr: &str) -> String {
    if addr == "usb" || addr.contains(':') { addr.to_string() } else { format!("{addr}:5555") }
}

/// The adb key this computer signs in with (~/.android/adbkey). Created once if missing, so the
/// projector only asks "Allow USB debugging?" the first time, not on every command.
pub fn key_path() -> Result<PathBuf> {
    let p = match std::env::var_os("ANDROID_USER_HOME") {
        Some(h) => PathBuf::from(h).join("android"),
        None => std::env::home_dir().context("no home directory")?.join(".android"),
    }
    .join("adbkey");
    if !p.exists() {
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        std::fs::create_dir_all(p.parent().unwrap())?;
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048)?;
        std::fs::write(&p, key.to_pkcs8_pem(LineEnding::LF)?.as_bytes())?;
        eprintln!("created adb key {}", p.display());
    }
    Ok(p)
}

impl Device {
    pub fn connect(addr: &str) -> Result<Self> {
        let addr = normalize(addr);
        let key = key_path()?;
        let inner: Box<dyn ADBDeviceExt> = if addr == "usb" {
            usb(&key)?
        } else {
            let sa: SocketAddr = addr.to_socket_addrs()?.next().ok_or_else(|| anyhow!("bad address {addr}"))?;
            // fail fast with a clear message instead of a long protocol timeout
            TcpStream::connect_timeout(&sa, Duration::from_secs(3)).with_context(|| {
                format!("projector {addr} not reachable (on? same network? adb over Wi-Fi on?)")
            })?;
            Box::new(ADBTcpDevice::new_with_custom_private_key(sa, &key).with_context(|| {
                format!("{addr}: adb handshake failed. If the projector shows \"Allow debugging?\", accept it (tick Always allow) and retry")
            })?)
        };
        Ok(Self { addr, inner })
    }

    /// Run a shell command, return stdout+stderr as text.
    pub fn sh(&mut self, cmd: &str) -> Result<String> {
        let mut out = Vec::new();
        self.inner.shell_command(&cmd, Some(&mut out), None).with_context(|| format!("shell: {cmd}"))?;
        Ok(String::from_utf8_lossy(&out).replace('\r', ""))
    }

    /// Run a shell command and return (output, exit code).
    pub fn sh_rc(&mut self, cmd: &str) -> Result<(String, i32)> {
        let out = self.sh(&format!("{cmd}\necho \"__rc=$?\""))?;
        let (body, rc) = out.rsplit_once("__rc=").unwrap_or((&out, "1"));
        Ok((body.to_string(), rc.trim().parse().unwrap_or(1)))
    }

    /// Stream a command's raw output into a writer (binary-safe).
    pub fn sh_stream(&mut self, cmd: &str, w: &mut dyn Write) -> Result<()> {
        self.inner.shell_command(&cmd, Some(w), None).with_context(|| format!("shell: {cmd}"))?;
        Ok(())
    }

    pub fn prop(&mut self, name: &str) -> Result<String> {
        Ok(self.sh(&format!("getprop {name}"))?.trim().to_string())
    }

    pub fn has_root(&mut self) -> bool {
        self.sh("su -c id 2>/dev/null").map(|o| o.contains("uid=0")).unwrap_or(false)
    }

    pub fn need_root(&mut self) -> Result<()> {
        if !self.has_root() {
            bail!("this needs root: Magisk installed, Superuser > Shell allowed (see README > Root)");
        }
        Ok(())
    }

    /// Run a script as root. Pushed as a file so quoting and `&&` survive intact.
    pub fn su(&mut self, script: &str) -> Result<String> {
        self.push_bytes(script.as_bytes(), &format!("{TMP}/mc-su.sh"))?;
        self.sh(&format!("su -c 'sh {TMP}/mc-su.sh'"))
    }

    pub fn push_bytes(&mut self, data: &[u8], remote: &str) -> Result<()> {
        self.inner.push(&mut std::io::Cursor::new(data), &remote).with_context(|| format!("push {remote}"))?;
        Ok(())
    }

    pub fn push_file(&mut self, local: &Path, remote: &str) -> Result<()> {
        let mut f = std::fs::File::open(local).with_context(|| format!("{}", local.display()))?;
        self.inner.push(&mut f, &remote).with_context(|| format!("push {remote}"))?;
        Ok(())
    }

    pub fn pull_bytes(&mut self, remote: &str) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        self.inner.pull(&remote, &mut v).with_context(|| format!("pull {remote}"))?;
        Ok(v)
    }

    /// Push, then `pm install` (adb_client's streamed install hangs on some builds).
    pub fn install(&mut self, apk: &Path) -> Result<()> {
        let remote = format!("{TMP}/mc-install.apk");
        self.push_file(apk, &remote)?;
        let out = self.sh(&format!("pm install -r {remote} 2>&1; rm -f {remote}"))?;
        if !out.contains("Success") { bail!("install {}: {}", apk.display(), out.trim()); }
        Ok(())
    }

    pub fn pkg_present(&mut self, pkg: &str) -> Result<bool> {
        Ok(self.sh(&format!("pm list packages {pkg}"))?.lines().any(|l| l.trim() == format!("package:{pkg}")))
    }

    pub fn pkg_disabled(&mut self, pkg: &str) -> Result<bool> {
        Ok(self.sh("pm list packages -d")?.lines().any(|l| l.trim() == format!("package:{pkg}")))
    }

    pub fn home_activity(&mut self) -> Result<String> {
        let o = self.sh("cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME")?;
        Ok(o.lines().last().unwrap_or("").trim().to_string())
    }

    pub fn wlan_ip(&mut self) -> Option<String> {
        let o = self.sh("ip -4 addr show wlan0").ok()?;
        o.split_whitespace().skip_while(|w| *w != "inet").nth(1).map(|s| s.split('/').next().unwrap().to_string())
    }

    pub fn key(&mut self, k: &str) -> Result<()> {
        self.sh(&format!("input keyevent {k}"))?;
        Ok(())
    }

    /// Normal reboot, then wait until Android is up again. Never to the bootloader: on this
    /// box that leaves it stuck on the logo until the power is pulled.
    pub fn reboot_and_wait(self) -> Result<Device> {
        let addr = self.addr.clone();
        let mut me = self;
        let _ = me.inner.reboot(RebootType::System);
        drop(me);
        thread::sleep(Duration::from_secs(10));
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(180) {
            if let Ok(mut d) = Device::connect(&addr) {
                if d.prop("sys.boot_completed").map(|v| v == "1").unwrap_or(false) {
                    eprintln!("back up after ~{}s", start.elapsed().as_secs() + 10);
                    return Ok(d);
                }
            }
            thread::sleep(Duration::from_secs(3));
        }
        bail!("{addr} did not come back within 3 minutes")
    }
}

#[cfg(feature = "usb")]
fn usb(key: &Path) -> Result<Box<dyn ADBDeviceExt>> {
    let d = adb_client::usb::ADBUSBDevice::autodetect_with_custom_private_key(key.to_path_buf())
        .context("no single adb device on USB (cable in? another adb server running? try `adb kill-server`)")?;
    Ok(Box::new(d))
}

#[cfg(not(feature = "usb"))]
fn usb(_: &Path) -> Result<Box<dyn ADBDeviceExt>> {
    bail!("this build has no USB support; use the projector's IP")
}

/// Quote a string for the device's sh.
pub fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
