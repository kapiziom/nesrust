//! A minimal window to try games by hand. No audio yet.
//!
//!   cargo run --release --example viewer -- path/to/game.nes
//!   cargo run --release --example viewer -- path/to/game.nes --screenshot out.png [--frames 120] [--press start]   (no window)
//!
//! Keys:  arrows = D-pad   X = A   Z = B   Enter = Start   Right Shift / Backspace = Select
//!        P = pause   R = reset   Tab (hold) = 4x speed   F12 = screenshot (PNG in the current directory)   Esc = quit
//!
//! The window can be resized by dragging its edges.
//! Battery-backed RAM is loaded from / saved to `<rom>.sav` next to the ROM.

use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, fs, process};

use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::controller::Buttons;
use nesrs::emulator::nes::Nes;
use nesrs::emulator::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};

const KEY_MAP: [(Key, Buttons); 9] = [
    (Key::X, Buttons::A),
    (Key::Z, Buttons::B),
    (Key::RightShift, Buttons::SELECT),
    (Key::Backspace, Buttons::SELECT),
    (Key::Enter, Buttons::START),
    (Key::Up, Buttons::UP),
    (Key::Down, Buttons::DOWN),
    (Key::Left, Buttons::LEFT),
    (Key::Right, Buttons::RIGHT),
];

/// The NTSC frame rate is 60.0988 Hz; 60 is close enough without audio to sync to.
const TARGET_FPS: usize = 60;
const FAST_FORWARD_FRAMES: usize = 4;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = args.first().cloned() else {
        eprintln!("usage: viewer <rom.nes> [--screenshot out.png [--frames N]]");
        process::exit(1);
    };
    let option = |name: &str| args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned();

    let data = fs::read(&path).unwrap_or_else(|error| fail(&format!("cannot read {path}: {error}")));
    let cartridge = Cartridge::from_bytes(&data).unwrap_or_else(|error| fail(&format!("cannot load {path}: {error}")));
    let mapper = cartridge.mapper_id();

    let mut nes = Nes::new(cartridge);
    let save_path = PathBuf::from(&path).with_extension("sav");
    if let Ok(saved) = fs::read(&save_path) {
        nes.cpu.bus.cartridge.load_battery_ram(&saved);
    }

    if let Some(screenshot) = option("--screenshot") {
        let frames = option("--frames").and_then(|n| n.parse().ok()).unwrap_or(120);
        // --press start|a|b|select|up|down|left|right : hold that button for a few frames after the first half second
        let pressed = option("--press").and_then(|name| match name.to_lowercase().as_str() {
            "a" => Some(Buttons::A),
            "b" => Some(Buttons::B),
            "select" => Some(Buttons::SELECT),
            "start" => Some(Buttons::START),
            "up" => Some(Buttons::UP),
            "down" => Some(Buttons::DOWN),
            "left" => Some(Buttons::LEFT),
            "right" => Some(Buttons::RIGHT),
            _ => None,
        });
        for frame in 0..frames {
            let held = pressed.filter(|_| (30..35).contains(&frame)).unwrap_or(Buttons::empty());
            nes.set_buttons(0, held);
            if !nes.run_frame() {
                eprintln!("the CPU stopped on an unimplemented opcode, PC = {:#06X}", nes.cpu.program_counter);
                break;
            }
        }
        save_png(&screenshot, nes.framebuffer());
        println!("saved {screenshot} after {frames} frames");
        return;
    }

    let name = PathBuf::from(&path).file_stem().map_or("nes".to_string(), |stem| stem.to_string_lossy().into_owned());
    let options = WindowOptions { scale: Scale::X2, resize: true, ..WindowOptions::default() };
    let mut window = Window::new(&name, SCREEN_WIDTH, SCREEN_HEIGHT, options)
        .unwrap_or_else(|error| fail(&format!("cannot open a window: {error}")));
    window.set_target_fps(TARGET_FPS);

    let mut pixels = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let mut paused = false;
    let mut stopped = false;
    let mut frames_counted = 0u32;
    let mut fps_timer = Instant::now();
    let mut fps = 0.0;
    let mut screenshots = 0;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::F12, KeyRepeat::No) {
            screenshots += 1;
            save_png(&format!("{name}-{screenshots}.png"), nes.framebuffer());
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            nes.reset();
            stopped = false;
        }

        let buttons = KEY_MAP.iter().filter(|(key, _)| window.is_key_down(*key)).fold(Buttons::empty(), |all, (_, button)| all | *button);
        nes.set_buttons(0, buttons);

        if !paused && !stopped {
            let frames = if window.is_key_down(Key::Tab) { FAST_FORWARD_FRAMES } else { 1 };
            for _ in 0..frames {
                if !nes.run_frame() {
                    stopped = true;
                    eprintln!("the CPU stopped on an unimplemented opcode, PC = {:#06X}", nes.cpu.program_counter);
                    break;
                }
                frames_counted += 1;
            }
        }

        for (pixel, rgba) in pixels.iter_mut().zip(nes.framebuffer().chunks_exact(4)) {
            *pixel = u32::from_be_bytes([0, rgba[0], rgba[1], rgba[2]]);
        }
        if let Err(error) = window.update_with_buffer(&pixels, SCREEN_WIDTH, SCREEN_HEIGHT) {
            fail(&format!("cannot update the window: {error}"));
        }

        if fps_timer.elapsed() >= Duration::from_secs(1) {
            fps = frames_counted as f64 / fps_timer.elapsed().as_secs_f64();
            frames_counted = 0;
            fps_timer = Instant::now();
        }
        let state = if stopped { " - CPU stopped" } else if paused { " - paused" } else { "" };
        window.set_title(&format!("{name} (mapper {mapper}) - {fps:.0} fps{state}"));
    }

    if let Some(battery_ram) = nes.cpu.bus.cartridge.battery_ram() {
        if let Err(error) = fs::write(&save_path, battery_ram) {
            eprintln!("cannot write {}: {error}", save_path.display());
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(1);
}

/// Writes the RGBA framebuffer as a PNG (uncompressed deflate blocks, so no compression crate is needed).
fn save_png(path: &str, rgba: &[u8]) {
    let mut raw = Vec::with_capacity((SCREEN_WIDTH * 4 + 1) * SCREEN_HEIGHT);
    for row in rgba.chunks_exact(SCREEN_WIDTH * 4) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }

    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(blocks.peek().is_none() as u8); // BFINAL, BTYPE = stored
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut header = Vec::new();
    header.extend_from_slice(&(SCREEN_WIDTH as u32).to_be_bytes());
    header.extend_from_slice(&(SCREEN_HEIGHT as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits per channel, RGBA

    let mut png = PNG_SIGNATURE.to_vec();
    for (kind, data) in [(b"IHDR", &header[..]), (b"IDAT", &zlib[..]), (b"IEND", &[][..])] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }

    if let Err(error) = fs::write(path, png) {
        eprintln!("cannot write {path}: {error}");
    }
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
