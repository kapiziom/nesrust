//! A minimal window to try games by hand, with sound.
//!
//!   cargo run --release --example viewer -- path/to/game.nes
//!   cargo run --release --example viewer -- path/to/game.nes --screenshot out.png [--frames 120] [--press start [--press-frame 30]] [--wav out.wav]   (no window)
//!
//! Keys:  arrows = D-pad   X = A   Z = B   Enter = Start   Right Shift / Backspace = Select
//!        P = pause   R = reset   Tab (hold) = 4x speed   F12 = screenshot (PNG in the current directory)   Esc = quit
//!        Gamepad (any, first to send input wins): D-pad / left stick, East = A, South = B, Start, Select (Back)
//!        F5 = save state   F9 = load state (`<rom>.state` next to the ROM)   W (hold) = rewind (the last 10 seconds)
//!
//! The window can be resized by dragging its edges. The audio buffer paces the emulation (if there is no sound
//! device the window's refresh rate does).
//! Battery-backed RAM is loaded from / saved to `<rom>.sav` next to the ROM.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{env, fs, process};

use gilrs::{Axis, Button, EventType, Gilrs};
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::controller::Buttons;
use nesrs::emulator::nes::Nes;
use nesrs::emulator::rewind::Rewind;
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

/// Only used when there is no sound device; the NTSC frame rate is really 60.0988 Hz.
const TARGET_FPS: usize = 60;
const VOLUME: f32 = 0.8;
const FAST_FORWARD_FRAMES: usize = 4;

/// How far the left stick has to be pushed to count as a D-pad press.
const STICK_THRESHOLD: f32 = 0.5;

/// Controller state kept up to date from gilrs events; every connected pad can play.
struct Pad {
    gilrs: Option<Gilrs>,
    buttons: Buttons,
    stick: Buttons,
}

impl Pad {
    fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(error) => {
                eprintln!("no gamepad support: {error}");
                None
            }
        };
        Pad { gilrs, buttons: Buttons::empty(), stick: Buttons::empty() }
    }

    fn nes_button(button: Button) -> Option<Buttons> {
        Some(match button {
            Button::East => Buttons::A,
            Button::South => Buttons::B,
            Button::Start => Buttons::START,
            Button::Select => Buttons::SELECT,
            Button::DPadUp => Buttons::UP,
            Button::DPadDown => Buttons::DOWN,
            Button::DPadLeft => Buttons::LEFT,
            Button::DPadRight => Buttons::RIGHT,
            _ => return None,
        })
    }

    /// Processes the pending events and returns what is held now.
    fn poll(&mut self) -> Buttons {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return Buttons::empty();
        };

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(nes) = Self::nes_button(button) {
                        self.buttons.insert(nes);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(nes) = Self::nes_button(button) {
                        self.buttons.remove(nes);
                    }
                }
                EventType::AxisChanged(Axis::LeftStickX, value, _) => {
                    self.stick.set(Buttons::LEFT, value < -STICK_THRESHOLD);
                    self.stick.set(Buttons::RIGHT, value > STICK_THRESHOLD);
                }
                EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                    self.stick.set(Buttons::UP, value > STICK_THRESHOLD);
                    self.stick.set(Buttons::DOWN, value < -STICK_THRESHOLD);
                }
                EventType::Disconnected => {
                    self.buttons = Buttons::empty();
                    self.stick = Buttons::empty();
                }
                _ => {}
            }
        }

        self.buttons | self.stick
    }
}
/// Frames of history kept for rewinding (10 seconds at 60 Hz).
const REWIND_FRAMES: usize = 600;
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
    let region = cartridge.region();

    let mut nes = Nes::new(cartridge);
    let save_path = PathBuf::from(&path).with_extension("sav");
    let state_path = PathBuf::from(&path).with_extension("state");
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
        let press_frame = option("--press-frame").and_then(|n| n.parse().ok()).unwrap_or(30);
        let mut recorded = Vec::new();
        for frame in 0..frames {
            let held = pressed.filter(|_| (press_frame..press_frame + 5).contains(&frame)).unwrap_or(Buttons::empty());
            nes.set_buttons(0, held);
            if !nes.run_frame() {
                eprintln!("the CPU stopped on an unimplemented opcode, PC = {:#06X}", nes.cpu.program_counter);
                break;
            }
            nes.drain_audio(&mut recorded);
        }
        if let Some(wav) = option("--wav") {
            save_wav(&wav, &recorded, 48_000);
            println!("saved {wav}: {} samples", recorded.len());
        }
        save_png(&screenshot, nes.framebuffer());
        println!("saved {screenshot} after {frames} frames");
        return;
    }

    let name = PathBuf::from(&path).file_stem().map_or("nes".to_string(), |stem| stem.to_string_lossy().into_owned());
    let options = WindowOptions { scale: Scale::X2, resize: true, ..WindowOptions::default() };
    let mut window = Window::new(&name, SCREEN_WIDTH, SCREEN_HEIGHT, options)
        .unwrap_or_else(|error| fail(&format!("cannot open a window: {error}")));

    let audio = AudioOutput::new();
    match &audio {
        Some(audio) => {
            eprintln!("{region:?} timing, {:.2} frames/s", region.frame_rate());
            nes.set_sample_rate(audio.sample_rate)
        }
        None => {
            eprintln!("no usable audio device: running without sound");
            window.set_target_fps(TARGET_FPS);
        }
    }
    let mut audio_samples = Vec::new();

    let mut pixels = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let mut paused = false;
    let mut stopped = false;
    let mut rewind = Rewind::new(REWIND_FRAMES);
    let mut pad = Pad::new();
    let mut last_rewind_step = Instant::now();
    let mut frames_counted = 0u32;
    let mut fps_timer = Instant::now();
    let mut fps = 0.0;
    let mut screenshots = 0;
    let mut title = String::new();
    let debug = env::var_os("VIEWER_DEBUG").is_some();
    let mut debug_timer = Instant::now();
    let mut debug_consumed = 0u64;
    let mut debug_frames = 0u64;
    let mut total_frames = 0u64;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::F12, KeyRepeat::No) {
            screenshots += 1;
            save_png(&format!("{name}-{screenshots}.png"), nes.framebuffer());
        }
        if window.is_key_pressed(Key::F5, KeyRepeat::No) {
            match fs::write(&state_path, nes.save_state()) {
                Ok(()) => eprintln!("state saved to {}", state_path.display()),
                Err(error) => eprintln!("cannot save the state: {error}"),
            }
        }
        if window.is_key_pressed(Key::F9, KeyRepeat::No) {
            match fs::read(&state_path) {
                Ok(bytes) => match nes.load_state(&bytes) {
                    Ok(()) => {
                        rewind.clear();
                        stopped = false;
                    }
                    Err(error) => eprintln!("cannot load the state: {error}"),
                },
                Err(error) => eprintln!("cannot read {}: {error}", state_path.display()),
            }
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            nes.reset();
            rewind.clear();
            stopped = false;
        }

        let keyboard = KEY_MAP.iter().filter(|(key, _)| window.is_key_down(*key)).fold(Buttons::empty(), |all, (_, button)| all | *button);
        nes.set_buttons(0, keyboard | pad.poll());

        let fast_forward = window.is_key_down(Key::Tab);
        let mut ran_a_frame = false;

        if window.is_key_down(Key::W) && !paused {
            // one frame back per 1/60 s of real time; the sound is silent meanwhile
            if last_rewind_step.elapsed() >= Duration::from_micros(16_667) && rewind.step_back(&mut nes) {
                last_rewind_step = Instant::now();
                audio_samples.clear();
                nes.drain_audio(&mut audio_samples);
                ran_a_frame = true;
                stopped = false;
            }
        } else if !paused && !stopped {
            // with sound the audio buffer is the clock: emulate only while it is running low
            let wanted = fast_forward || audio.as_ref().map_or(true, |audio| audio.queued() < audio.target_queue);

            if wanted {
                let frames = if fast_forward { FAST_FORWARD_FRAMES } else { 1 };
                for _ in 0..frames {
                    rewind.push(&nes);
                    if !nes.run_frame() {
                        stopped = true;
                        eprintln!("the CPU stopped on an unimplemented opcode, PC = {:#06X}", nes.cpu.program_counter);
                        break;
                    }
                    frames_counted += 1;
                    total_frames += 1;
                    ran_a_frame = true;

                    audio_samples.clear();
                    nes.drain_audio(&mut audio_samples);
                    if let (Some(audio), false) = (&audio, fast_forward) {
                        audio.push(&audio_samples);
                    }
                }
            }
        }

        if !ran_a_frame {
            // nothing new to show: keep the window responsive without spinning
            let idle = if paused || stopped { Duration::from_millis(16) } else { Duration::from_millis(1) };
            std::thread::sleep(idle);
            if !paused && !stopped {
                window.update();
                continue;
            }
        }

        for (pixel, rgba) in pixels.iter_mut().zip(nes.framebuffer().chunks_exact(4)) {
            *pixel = u32::from_be_bytes([0, rgba[0], rgba[1], rgba[2]]);
        }
        if let Err(error) = window.update_with_buffer(&pixels, SCREEN_WIDTH, SCREEN_HEIGHT) {
            fail(&format!("cannot update the window: {error}"));
        }

        if debug && debug_timer.elapsed() >= Duration::from_secs(2) {
            let seconds = debug_timer.elapsed().as_secs_f64();
            let consumed = audio.as_ref().map_or(0, |audio| audio.consumed.load(Ordering::Relaxed));
            eprintln!(
                "debug: emulated {:.2} frames/s, device consumed {:.0} audio frames/s, queue {}",
                (total_frames - debug_frames) as f64 / seconds,
                (consumed - debug_consumed) as f64 / seconds,
                audio.as_ref().map_or(0, |audio| audio.queued()),
            );
            debug_timer = Instant::now();
            debug_consumed = consumed;
            debug_frames = total_frames;
        }

        if fps_timer.elapsed() >= Duration::from_secs(1) {
            fps = frames_counted as f64 / fps_timer.elapsed().as_secs_f64();
            frames_counted = 0;
            fps_timer = Instant::now();
        }
        let state = if stopped { " - CPU stopped" } else if paused { " - paused" } else { "" };
        let new_title = format!("{name} ({region:?}, mapper {mapper}) - {fps:.0} fps{state}");
        if new_title != title {
            window.set_title(&new_title);
            title = new_title;
        }
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

/// Sound output through the default audio device: the APU's samples are queued and a cpal stream plays them.
struct AudioOutput {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    /// Audio frames the device has asked for so far (to check its real playback rate).
    consumed: Arc<AtomicU64>,
    sample_rate: u32,
    /// The queue length (about 70 ms of sound) the emulation tries to keep up.
    target_queue: usize,
}

impl AudioOutput {
    fn new() -> Option<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config = supported.config();
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let consumed = Arc::new(AtomicU64::new(0));
        eprintln!("audio: {sample_rate} Hz, {channels} channels, {format:?}");

        let stream = match format {
            cpal::SampleFormat::F32 => Self::build_stream::<f32>(&device, config, channels, queue.clone(), consumed.clone()),
            cpal::SampleFormat::I16 => Self::build_stream::<i16>(&device, config, channels, queue.clone(), consumed.clone()),
            cpal::SampleFormat::U16 => Self::build_stream::<u16>(&device, config, channels, queue.clone(), consumed.clone()),
            other => {
                eprintln!("unsupported audio sample format {other:?}");
                None
            }
        }?;
        stream.play().ok()?;

        Some(AudioOutput { _stream: stream, queue, consumed, sample_rate, target_queue: sample_rate as usize / 15 })
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        channels: usize,
        queue: Arc<Mutex<VecDeque<f32>>>,
        consumed: Arc<AtomicU64>,
    ) -> Option<cpal::Stream>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        use cpal::traits::DeviceTrait;

        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    let mut queue = queue.lock().unwrap();
                    consumed.fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
                    for frame in data.chunks_mut(channels) {
                        // the sound is mono: every channel gets the same sample (silence if the queue ran dry)
                        frame.fill(T::from_sample(queue.pop_front().unwrap_or(0.0)));
                    }
                },
                |error| eprintln!("audio error: {error}"),
                None,
            )
            .map_err(|error| eprintln!("cannot open the audio stream: {error}"))
            .ok()
    }

    fn queued(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    fn push(&self, samples: &[f32]) {
        let mut queue = self.queue.lock().unwrap();
        queue.extend(samples.iter().map(|sample| sample * VOLUME));

        // never let the delay grow beyond a second
        let limit = self.sample_rate as usize;
        if queue.len() > limit {
            let excess = queue.len() - limit;
            queue.drain(..excess);
        }
    }
}

/// Writes mono 16-bit PCM.
fn save_wav(path: &str, samples: &[f32], sample_rate: u32) {
    let data_len = samples.len() as u32 * 2;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }

    if let Err(error) = fs::write(path, wav) {
        eprintln!("cannot write {path}: {error}");
    }
}
