//! Find projectors on the local network: anything answering adb on :5555 that reports an H726.
use crate::device::Device;
use std::io::{Read, Write};
use std::{
    net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
    sync::mpsc,
    thread,
    time::Duration,
};

pub struct Found { pub addr: String, pub model: String, pub serial: String }

/// Our own LAN address, found by asking the OS which interface routes to the internet.
fn local_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:80").ok()?;
    match s.local_addr().ok()? { SocketAddr::V4(a) => Some(*a.ip()), _ => None }
}

/// Scan a /24 (default: ours) for open :5555, then ask each one what it is.
pub fn scan(subnet: Option<Ipv4Addr>, any: bool) -> Vec<Found> {
    let base = match subnet.or_else(local_ip) { Some(ip) => ip.octets(), None => return vec![] };
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for chunk in (1u8..=254).collect::<Vec<_>>().chunks(32) {
        let chunk = chunk.to_vec();
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            for h in chunk {
                let ip = Ipv4Addr::new(base[0], base[1], base[2], h);
                let sa = SocketAddr::from((ip, 5555));
                if TcpStream::connect_timeout(&sa, Duration::from_millis(300)).is_ok() {
                    let _ = tx.send(sa.to_string());
                }
            }
        }));
    }
    drop(tx);
    for h in handles { let _ = h.join(); }
    let mut open: Vec<String> = rx.into_iter().collect();
    open.sort_by_key(|a| a.split(['.', ':']).nth(3).and_then(|x| x.parse::<u8>().ok()).unwrap_or(0));
    // Ask each one what it is, in parallel, with a deadline per device.
    let (tx, rx) = mpsc::channel();
    let n = open.len();
    for addr in open {
        let tx = tx.clone();
        thread::spawn(move || { let _ = tx.send((addr.clone(), identify(&addr, any))); });
    }
    drop(tx);
    let mut found = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    for _ in 0..n {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let Ok((addr, r)) = rx.recv_timeout(left) else { break };
        match r {
            Probe::Projector { model, serial } => found.push(Found { addr, model, serial }),
            Probe::NotApproved => found.push(Found { addr, model: "?".into(), serial: "adb device, this computer not approved yet".into() }),
            Probe::Other => {}
        }
    }
    found.sort_by_key(|f| f.addr.split(['.', ':']).nth(3).and_then(|x| x.parse::<u8>().ok()).unwrap_or(0));
    found
}

enum Probe { Projector { model: String, serial: String }, NotApproved, Other }

/// Only a fully approved device gets a real session. For the rest we do the adb handshake by
/// hand and stop before offering our public key: offering it makes ANY adb device on the
/// network (TVs, phones) pop up "Allow debugging?", which a scan must never do.
fn identify(addr: &str, any: bool) -> Probe {
    match handshake(addr) {
        Ok(true) => {
            let Ok(mut d) = Device::connect(addr) else { return Probe::Other };
            match d.prop("ro.board.platform") {
                Ok(p) if any || p.starts_with("sun50iw15") => Probe::Projector {
                    model: d.prop("ro.product.model").unwrap_or_default(),
                    serial: d.prop("ro.serialno").unwrap_or_default(),
                },
                _ => Probe::Other,
            }
        }
        Ok(false) => Probe::NotApproved,
        Err(_) => Probe::Other,
    }
}

const CNXN: u32 = 0x4e58_4e43;
const AUTH: u32 = 0x4854_5541;

fn send(s: &mut TcpStream, cmd: u32, a0: u32, a1: u32, data: &[u8]) -> std::io::Result<()> {
    let sum: u32 = data.iter().map(|&b| b as u32).sum();
    let mut m = Vec::with_capacity(24 + data.len());
    for v in [cmd, a0, a1, data.len() as u32, sum, !cmd] { m.extend_from_slice(&v.to_le_bytes()); }
    m.extend_from_slice(data);
    s.write_all(&m)
}

fn recv(s: &mut TcpStream) -> std::io::Result<(u32, u32, Vec<u8>)> {
    let mut h = [0u8; 24];
    s.read_exact(&mut h)?;
    let w = |i: usize| u32::from_le_bytes(h[i * 4..i * 4 + 4].try_into().unwrap());
    let len = w(3) as usize;
    if len > 1 << 20 { return Err(std::io::ErrorKind::InvalidData.into()); }
    let mut data = vec![0u8; len];
    s.read_exact(&mut data)?;
    Ok((w(0), w(1), data))
}

/// Ok(true) = our key is accepted, Ok(false) = adb but not approved, Err = not adb / no answer.
fn handshake(addr: &str) -> anyhow::Result<bool> {
    use rsa::{pkcs8::DecodePrivateKey, Pkcs1v15Sign, RsaPrivateKey};
    let sa: SocketAddr = addr.parse()?;
    let mut s = TcpStream::connect_timeout(&sa, Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    send(&mut s, CNXN, 0x0100_0000, 256 * 1024, b"host::\0")?;
    let (cmd, a0, token) = recv(&mut s)?;
    if cmd == CNXN { return Ok(true); }
    if cmd != AUTH || a0 != 1 { anyhow::bail!("not adb"); }
    let key = RsaPrivateKey::from_pkcs8_pem(&std::fs::read_to_string(crate::device::key_path()?)?)?;
    let sig = key.sign(Pkcs1v15Sign::new::<sha1::Sha1>(), &token)?;
    send(&mut s, AUTH, 2, 0, &sig)?;
    let (cmd, _, _) = recv(&mut s)?;
    Ok(cmd == CNXN)
}
