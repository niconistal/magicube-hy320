//! Runs ON the projector. Grabs frames from the built-in UVC camera (640x480 YUYV only)
//! and writes upright greyscale PGMs.
//!   mc-grab <out.pgm> [exposure] [count] [gap_ms]
//! With count > 1, writes <out>.0.pgm, <out>.1.pgm ... `gap_ms` apart.
//! Auto exposure gives blank frames on this sensor, so exposure is always manual.
//! The sensor is mounted 90 degrees off: frames are rotated counter-clockwise (480x640 out).
use std::{ffi::CString, fs::File, io::Write, ptr, thread, time::Duration};

const W: usize = 640;
const H: usize = 480;
const NBUF: u32 = 4;

// <linux/videodev2.h>, 32-bit ARM layouts
#[repr(C)]
struct PixFormat { width: u32, height: u32, pixelformat: u32, field: u32, bytesperline: u32,
    sizeimage: u32, colorspace: u32, priv_: u32, flags: u32, ycbcr_enc: u32, quantization: u32, xfer_func: u32 }
#[repr(C)]
struct Format { type_: u32, pix: PixFormat, _pad: [u8; 200 - std::mem::size_of::<PixFormat>()] }
#[repr(C)]
struct RequestBuffers { count: u32, type_: u32, memory: u32, capabilities: u32, flags: u8, _r: [u8; 3] }
#[repr(C)]
struct Timeval { sec: libc::c_long, usec: libc::c_long }
#[repr(C)]
struct Timecode { type_: u32, flags: u32, frames: u8, seconds: u8, minutes: u8, hours: u8, userbits: [u8; 4] }
#[repr(C)]
struct Buffer { index: u32, type_: u32, bytesused: u32, flags: u32, field: u32, timestamp: Timeval,
    timecode: Timecode, sequence: u32, memory: u32, offset: u32, length: u32, reserved2: u32, request_fd: i32 }
#[repr(C)]
struct Control { id: u32, value: i32 }

const fn iowr(nr: u32, size: usize) -> u32 { (3 << 30) | ((size as u32) << 16) | ((b'V' as u32) << 8) | nr }
const fn iow(nr: u32, size: usize) -> u32 { (1 << 30) | ((size as u32) << 16) | ((b'V' as u32) << 8) | nr }
const VIDIOC_S_FMT: u32 = iowr(5, std::mem::size_of::<Format>());
const VIDIOC_REQBUFS: u32 = iowr(8, std::mem::size_of::<RequestBuffers>());
const VIDIOC_QUERYBUF: u32 = iowr(9, std::mem::size_of::<Buffer>());
const VIDIOC_QBUF: u32 = iowr(15, std::mem::size_of::<Buffer>());
const VIDIOC_DQBUF: u32 = iowr(17, std::mem::size_of::<Buffer>());
const VIDIOC_STREAMON: u32 = iow(18, 4);
const VIDIOC_STREAMOFF: u32 = iow(19, 4);
const VIDIOC_S_CTRL: u32 = iowr(28, std::mem::size_of::<Control>());
const CAPTURE: u32 = 1;
const MMAP: u32 = 1;
const YUYV: u32 = u32::from_le_bytes(*b"YUYV");
const CID_EXPOSURE_AUTO: u32 = 0x009a0901;
const CID_EXPOSURE_ABSOLUTE: u32 = 0x009a0902;

fn ioctl<T>(fd: i32, req: u32, arg: &mut T) -> Result<(), String> {
    loop {
        let r = unsafe { libc::ioctl(fd, req as _, arg as *mut T) };
        if r == 0 { return Ok(()); }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINTR) { return Err(format!("ioctl {req:#x}: {e}")); }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).ok_or("usage: mc-grab <out.pgm> [exposure] [count] [gap_ms]")?;
    let exposure: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(130);
    let count: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
    let gap = Duration::from_millis(args.get(4).and_then(|s| s.parse().ok()).unwrap_or(500));

    let dev = CString::new("/dev/video0").unwrap();
    let fd = unsafe { libc::open(dev.as_ptr(), libc::O_RDWR) };
    if fd < 0 { return Err(format!("/dev/video0: {}", std::io::Error::last_os_error())); }

    let mut f: Format = unsafe { std::mem::zeroed() };
    f.type_ = CAPTURE;
    f.pix.width = W as u32; f.pix.height = H as u32; f.pix.pixelformat = YUYV;
    ioctl(fd, VIDIOC_S_FMT, &mut f)?;
    if f.pix.width as usize != W || f.pix.height as usize != H {
        return Err(format!("camera gave {}x{}, expected {W}x{H}", f.pix.width, f.pix.height));
    }
    // failures here are not fatal: the frame will just be dark or bright
    let _ = ioctl(fd, VIDIOC_S_CTRL, &mut Control { id: CID_EXPOSURE_AUTO, value: 1 });
    let _ = ioctl(fd, VIDIOC_S_CTRL, &mut Control { id: CID_EXPOSURE_ABSOLUTE, value: exposure });

    let mut rb = RequestBuffers { count: NBUF, type_: CAPTURE, memory: MMAP, capabilities: 0, flags: 0, _r: [0; 3] };
    ioctl(fd, VIDIOC_REQBUFS, &mut rb)?;
    let mut maps = Vec::new();
    for i in 0..rb.count.min(NBUF) {
        let mut b: Buffer = unsafe { std::mem::zeroed() };
        b.type_ = CAPTURE; b.memory = MMAP; b.index = i;
        ioctl(fd, VIDIOC_QUERYBUF, &mut b)?;
        let p = unsafe { libc::mmap(ptr::null_mut(), b.length as usize, libc::PROT_READ | libc::PROT_WRITE,
                                    libc::MAP_SHARED, fd, b.offset as libc::off_t) };
        if p == libc::MAP_FAILED { return Err("mmap failed".into()); }
        maps.push(p as *const u8);
        ioctl(fd, VIDIOC_QBUF, &mut b)?;
    }
    let mut t = CAPTURE as i32;
    ioctl(fd, VIDIOC_STREAMON, &mut t)?;

    // one frame from the queue; the camera keeps streaming while we wait between shots
    let dq = |settle: usize| -> Result<Vec<u8>, String> {
        let mut img = vec![0u8; W * H];
        for k in 0..=settle {
            let mut b: Buffer = unsafe { std::mem::zeroed() };
            b.type_ = CAPTURE; b.memory = MMAP;
            ioctl(fd, VIDIOC_DQBUF, &mut b)?;
            if k == settle {
                let p = unsafe { std::slice::from_raw_parts(maps[b.index as usize], W * H * 2) };
                // rotate CCW: out(x, y) = in(row = x, col = W-1-y); out is H wide, W tall
                for oy in 0..W { for ox in 0..H { img[oy * H + ox] = p[(ox * W + (W - 1 - oy)) * 2]; } }
            }
            ioctl(fd, VIDIOC_QBUF, &mut b)?;
        }
        Ok(img)
    };

    let mut first = true;
    for n in 0..count {
        if !first { thread::sleep(gap); }
        // first frame: let exposure settle; later ones: skip stale queued frames
        let img = dq(if first { 30 } else { NBUF as usize })?;
        first = false;
        let path = if count == 1 { out.clone() } else { format!("{out}.{n}.pgm") };
        let mut o = File::create(&path).map_err(|e| format!("{path}: {e}"))?;
        write!(o, "P5\n{H} {W}\n255\n").and_then(|_| o.write_all(&img)).map_err(|e| e.to_string())?;
        let mean = img.iter().map(|&v| v as u64).sum::<u64>() / img.len() as u64;
        println!("{path} mean={mean}");
    }
    let _ = ioctl(fd, VIDIOC_STREAMOFF, &mut t);
    unsafe { libc::close(fd) };
    Ok(())
}

fn main() {
    if let Err(e) = run() { eprintln!("mc-grab: {e}"); std::process::exit(1); }
}
