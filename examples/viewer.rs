//! A minimal window to try games by hand, with sound.
//!
//!   cargo run --release --example viewer -- path/to/game.nes
//!   cargo run --release --example viewer -- path/to/game.nes --screenshot out.png [--frames 120] [--press start [--press-frame 30]] [--wav out.wav]   (no window)
//!
//! Keys:  arrows = D-pad   X = A   Z = B   Enter = Start   Right Shift / Backspace = Select
//!        player 2: I J K L = D-pad   O = A   U = B   9 = Start   8 = Select
//!        P = pause   R = reset   Tab (hold) = 4x speed   F12 = screenshot (PNG in the current directory)   Esc = quit
//!        M = mute   - / = (the key left of Backspace) = volume down / up
//!        Gamepads: the first one plays as player 1, the second as player 2; D-pad / left stick, East = A, South = B,
//!        Start, Select (Back)
//!        --record FILE writes your input to FILE when you quit, --replay FILE plays such a recording back
//!        (`gamecheck --bot script:FILE` replays it without a window; rewind, load state and reset are off meanwhile)
//!        F5 = save state   F9 = load state (`<rom>.state` next to the ROM)   1-4 = choose the state slot (`.state2` ...)
//!        W (hold) = rewind (the last 10 seconds)
//!        Options: --scale 1|2|3|4|8 (window size, default 2)   --palette FILE.pal (a 64 or 512 colour palette file)
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

use emu_core::{png, InputScript};
use gilrs::{Axis, Button, EventType, Gilrs};
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::controller::Buttons;
use nesrs::emulator::nes::Nes;
use nesrs::emulator::ppu::palette::Palette;
use nesrs::emulator::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};
use nesrs::emulator::rewind::Rewind;

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

const KEY_MAP_PLAYER_2: [(Key, Buttons); 7] = [
    (Key::O, Buttons::A),
    (Key::U, Buttons::B),
    (Key::Key8, Buttons::SELECT),
    (Key::Key9, Buttons::START),
    (Key::I, Buttons::UP),
    (Key::K, Buttons::DOWN),
    (Key::J, Buttons::LEFT),
];

/// Only used when there is no sound device; the NTSC frame rate is really 60.0988 Hz.
const TARGET_FPS: usize = 60;
const VOLUME: f32 = 0.8;
const FAST_FORWARD_FRAMES: usize = 4;

/// How far the left stick has to be pushed to count as a D-pad press.
const STICK_THRESHOLD: f32 = 0.5;
const VOLUME_STEP: f32 = 0.1;

/// What one gamepad is holding.
#[derive(Default)]
struct PadState {
    buttons: Buttons,
    stick: Buttons,
}

/// Controller state kept up to date from gilrs events: the first gamepad that sends anything plays as player 1, the
/// second one as player 2.
struct Pad {
    gilrs: Option<Gilrs>,
    pads: Vec<(gilrs::GamepadId, PadState)>,
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
        Pad { gilrs, pads: Vec::new() }
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

    /// Processes the pending events and returns what is held now on player 1's and player 2's gamepads.
    fn poll(&mut self) -> [Buttons; 2] {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return [Buttons::empty(); 2];
        };

        while let Some(event) = gilrs.next_event() {
            let index = match self.pads.iter().position(|(id, _)| *id == event.id) {
                Some(index) => index,
                None => {
                    self.pads.push((event.id, PadState::default()));
                    self.pads.len() - 1
                }
            };
            let state = &mut self.pads[index].1;

            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(nes) = Self::nes_button(button) {
                        state.buttons.insert(nes);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(nes) = Self::nes_button(button) {
                        state.buttons.remove(nes);
                    }
                }
                EventType::AxisChanged(Axis::LeftStickX, value, _) => {
                    state.stick.set(Buttons::LEFT, value < -STICK_THRESHOLD);
                    state.stick.set(Buttons::RIGHT, value > STICK_THRESHOLD);
                }
                EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                    state.stick.set(Buttons::UP, value > STICK_THRESHOLD);
                    state.stick.set(Buttons::DOWN, value < -STICK_THRESHOLD);
                }
                EventType::Disconnected => *state = PadState::default(),
                _ => {}
            }
        }

        let held =
            |index: usize| self.pads.get(index).map_or(Buttons::empty(), |(_, state)| state.buttons | state.stick);
        [held(0), held(1)]
    }
}

/// Frames of history kept for rewinding (10 seconds at 60 Hz).
const REWIND_FRAMES: usize = 600;

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
    if let Some(palette_path) = option("--palette") {
        let bytes =
            fs::read(&palette_path).unwrap_or_else(|error| fail(&format!("cannot read {palette_path}: {error}")));
        let palette = Palette::from_pal_bytes(&bytes)
            .unwrap_or_else(|| fail(&format!("{palette_path} is not a 64 or 512 colour .pal file")));
        nes.cpu.bus.ppu_mut().set_palette(palette);
    }
    let save_path = PathBuf::from(&path).with_extension("sav");
    // slot 1 is `<rom>.state`, the others `<rom>.state2` ...
    let state_path_for = |slot: u8| {
        PathBuf::from(&path).with_extension(if slot == 1 { "state".to_string() } else { format!("state{slot}") })
    };
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
    let scale = match option("--scale").as_deref() {
        Some("1") => Scale::X1,
        Some("3") => Scale::X4, // minifb has no 3x: the next size up
        Some("4") => Scale::X4,
        Some("8") => Scale::X8,
        _ => Scale::X2,
    };
    let options = WindowOptions { scale, resize: true, ..WindowOptions::default() };
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
    let mut slot = 1u8;
    let mut notice = String::new();
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
    let record_path = option("--record");
    let mut recording = record_path.as_ref().map(|_| InputScript::new());
    let replay = option("--replay").map(|path| {
        let text = fs::read_to_string(&path).unwrap_or_else(|error| fail(&format!("cannot read {path}: {error}")));
        InputScript::parse(&text).unwrap_or_else(|error| fail(&format!("{path}: {error}")))
    });
    // rewinding, loading a state and resetting would put the game out of step with the recorded frame numbers
    let timeline_locked = recording.is_some() || replay.is_some();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::F12, KeyRepeat::No) {
            screenshots += 1;
            save_png(&format!("{name}-{screenshots}.png"), nes.framebuffer());
        }
        for (key, number) in [(Key::Key1, 1u8), (Key::Key2, 2), (Key::Key3, 3), (Key::Key4, 4)] {
            if window.is_key_pressed(key, KeyRepeat::No) {
                slot = number;
                notice = format!("state slot {slot}");
            }
        }
        if window.is_key_pressed(Key::F5, KeyRepeat::No) {
            let state_path = state_path_for(slot);
            notice = match fs::write(&state_path, nes.save_state()) {
                Ok(()) => format!("saved to slot {slot}"),
                Err(error) => format!("cannot save the state: {error}"),
            };
            eprintln!("{notice} ({})", state_path.display());
        }
        if window.is_key_pressed(Key::F9, KeyRepeat::No) && !timeline_locked {
            let state_path = state_path_for(slot);
            notice = match fs::read(&state_path) {
                Ok(bytes) => match nes.load_state(&bytes) {
                    Ok(()) => {
                        rewind.clear();
                        stopped = false;
                        format!("loaded slot {slot}")
                    }
                    Err(error) => format!("cannot load slot {slot}: {error}"),
                },
                Err(_) => format!("slot {slot} is empty"),
            };
            eprintln!("{notice}");
        }
        if let Some(audio) = &audio {
            if window.is_key_pressed(Key::M, KeyRepeat::No) {
                audio.muted.set(!audio.muted.get());
            }
            for (key, step) in [(Key::Minus, -VOLUME_STEP), (Key::Equal, VOLUME_STEP)] {
                if window.is_key_pressed(key, KeyRepeat::Yes) {
                    audio.volume.set((audio.volume.get() + step).clamp(0.0, 1.0));
                    audio.muted.set(false);
                }
            }
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) && !timeline_locked {
            nes.reset();
            rewind.clear();
            stopped = false;
        }

        let keyboard = KEY_MAP
            .iter()
            .filter(|(key, _)| window.is_key_down(*key))
            .fold(Buttons::empty(), |all, (_, button)| all | *button);
        let keyboard_2 = KEY_MAP_PLAYER_2
            .iter()
            .filter(|(key, _)| window.is_key_down(*key))
            .fold(Buttons::empty(), |all, (_, button)| all | *button)
            | if window.is_key_down(Key::L) { Buttons::RIGHT } else { Buttons::empty() };
        let [pad_1, pad_2] = pad.poll();
        let live = keyboard | pad_1;
        nes.set_buttons(0, live);
        nes.set_buttons(1, keyboard_2 | pad_2);

        let fast_forward = window.is_key_down(Key::Tab);
        let mut ran_a_frame = false;

        if window.is_key_down(Key::W) && !paused && !timeline_locked {
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
            let wanted = fast_forward || audio.as_ref().is_none_or(|audio| audio.queued() < audio.target_queue);

            if wanted {
                let frames = if fast_forward { FAST_FORWARD_FRAMES } else { 1 };
                for _ in 0..frames {
                    rewind.push(&nes);
                    let held = match &replay {
                        Some(script) => Buttons::from_bits_truncate(script.at(total_frames) as u8),
                        None => live,
                    };
                    if let Some(script) = recording.as_mut() {
                        script.set(total_frames, held.bits() as u32);
                    }
                    nes.set_buttons(0, held);
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

        for (pixel, rgba) in pixels.iter_mut().zip(nes.framebuffer().as_chunks::<4>().0) {
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
        let state = if stopped {
            " - CPU stopped"
        } else if paused {
            " - paused"
        } else {
            ""
        };
        let sound = match &audio {
            Some(audio) if audio.muted.get() => " - muted".to_string(),
            Some(audio) if audio.volume.get() < VOLUME - 0.01 || audio.volume.get() > VOLUME + 0.01 => {
                format!(" - volume {:.0}%", audio.volume.get() * 100.0)
            }
            _ => String::new(),
        };
        let extra = if notice.is_empty() { format!(" - slot {slot}") } else { format!(" - {notice}") };
        let new_title = format!("{name} ({region:?}, mapper {mapper}) - {fps:.0} fps{state}{sound}{extra}");
        if new_title != title {
            window.set_title(&new_title);
            title = new_title;
        }
    }

    if let (Some(path), Some(mut script)) = (record_path, recording) {
        script.set(total_frames, 0);
        match fs::write(&path, script.to_text()) {
            Ok(()) => eprintln!("recorded {total_frames} frames of input to {path}"),
            Err(error) => eprintln!("cannot write {path}: {error}"),
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
    if let Err(error) = fs::write(path, png::encode_rgba(SCREEN_WIDTH, SCREEN_HEIGHT, rgba)) {
        eprintln!("cannot write {path}: {error}");
    }
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
    volume: std::cell::Cell<f32>,
    muted: std::cell::Cell<bool>,
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
            cpal::SampleFormat::F32 => {
                Self::build_stream::<f32>(&device, config, channels, queue.clone(), consumed.clone())
            }
            cpal::SampleFormat::I16 => {
                Self::build_stream::<i16>(&device, config, channels, queue.clone(), consumed.clone())
            }
            cpal::SampleFormat::U16 => {
                Self::build_stream::<u16>(&device, config, channels, queue.clone(), consumed.clone())
            }
            other => {
                eprintln!("unsupported audio sample format {other:?}");
                None
            }
        }?;
        stream.play().ok()?;

        Some(AudioOutput {
            _stream: stream,
            queue,
            consumed,
            sample_rate,
            target_queue: sample_rate as usize / 15,
            volume: std::cell::Cell::new(VOLUME),
            muted: std::cell::Cell::new(false),
        })
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
        let gain = if self.muted.get() { 0.0 } else { self.volume.get() };
        queue.extend(samples.iter().map(|sample| sample * gain));

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
