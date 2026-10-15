use crate::emulator::bus::Bus;
use crate::emulator::cartridge::{Cartridge, StateError};
use crate::emulator::controller::Buttons;
use crate::emulator::cpu::{CpuState, CPU};
use crate::emulator::region::Region;
use emu_core::{ButtonInfo, Machine, VideoFrame};

const STATE_MAGIC: [u8; 4] = *b"NSST";
/// Bump when the layout of any serialized struct changes.
const STATE_VERSION: u16 = 3;

#[derive(serde::Serialize)]
struct SaveStateOut<'a> {
    magic: [u8; 4],
    version: u16,
    mapper_id: u16,
    rom_checksum: u32,
    cpu: &'a CPU<Bus>,
    mapper: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct SaveStateIn {
    magic: [u8; 4],
    version: u16,
    mapper_id: u16,
    rom_checksum: u32,
    cpu: CPU<Bus>,
    mapper: Vec<u8>,
}

/// The whole console: a CPU wired to the bus (RAM, PPU, cartridge).
/// This is the entry point a frontend or a test runner uses.
pub struct Nes {
    pub cpu: CPU<Bus>,
}

impl Nes {
    /// Creates the console and runs the reset sequence, so execution starts at the cartridge's reset vector.
    pub fn new(cartridge: Cartridge) -> Self {
        let mut cpu = CPU::new(Bus::new(cartridge));
        // power-up: SP is 0 and the reset sequence (which lowers it by 3) leaves $FD; A, X, Y are 0
        cpu.set_state(CpuState { pc: 0, a: 0, x: 0, y: 0, p: 0x24, sp: 0x00 });
        cpu.reset();
        Nes { cpu }
    }

    pub fn reset(&mut self) {
        self.cpu.bus.apu_mut().reset();
        self.cpu.reset();
    }

    /// Runs until the PPU has finished drawing a frame. Returns false if the CPU stopped on an unknown opcode.
    pub fn run_frame(&mut self) -> bool {
        loop {
            if !self.cpu.step() {
                return false;
            }
            if self.cpu.bus.take_frame_complete() {
                return true;
            }
        }
    }

    /// Everything that makes up the console's state (CPU, RAM, PPU, APU, controllers, mapper registers and RAM), but
    /// not the ROM: it can only be loaded back into the same game.
    pub fn save_state(&self) -> Vec<u8> {
        let cartridge = &self.cpu.bus.cartridge;
        let state = SaveStateOut {
            magic: STATE_MAGIC,
            version: STATE_VERSION,
            mapper_id: cartridge.mapper_id(),
            rom_checksum: cartridge.rom_checksum(),
            cpu: &self.cpu,
            mapper: cartridge.save_state(),
        };
        postcard::to_allocvec(&state).expect("serializing into a Vec cannot fail")
    }

    /// Restores a state from `save_state`. On error the console is left as it was.
    /// The audio output rate, the palette and the picture of the running console are kept.
    pub fn load_state(&mut self, data: &[u8]) -> Result<(), StateError> {
        let state: SaveStateIn = postcard::from_bytes(data).map_err(|_| StateError::Corrupt)?;
        let cartridge = &mut self.cpu.bus.cartridge;

        if state.magic != STATE_MAGIC {
            return Err(StateError::Corrupt);
        }
        if state.version != STATE_VERSION {
            return Err(StateError::WrongVersion);
        }
        if state.mapper_id != cartridge.mapper_id() || state.rom_checksum != cartridge.rom_checksum() {
            return Err(StateError::WrongCartridge);
        }

        // the mapper only replaces itself when its bytes are valid
        cartridge.load_state(&state.mapper)?;

        let mut cpu = state.cpu;
        cpu.bus.take_runtime_from(&mut self.cpu.bus);
        self.cpu = cpu;
        Ok(())
    }

    /// Sets the buttons held on controller `port` (0 = $4016, 1 = $4017); they stay held until changed.
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        self.cpu.bus.set_buttons(port, buttons);
    }

    /// Sets the audio output sample rate (use the sound device's). Samples produced so far are discarded.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.cpu.bus.apu_mut().set_sample_rate(sample_rate);
    }

    /// Moves the audio produced since the last call (mono `f32`, roughly -0.5..0.5) to `out`.
    pub fn drain_audio(&mut self, out: &mut Vec<f32>) {
        self.cpu.bus.apu_mut().drain_samples(out);
    }

    /// RGBA, 256x240.
    pub fn framebuffer(&self) -> &[u8] {
        self.cpu.bus.ppu().framebuffer()
    }

    /// The console timing in use (taken from the cartridge).
    pub fn region(&self) -> Region {
        self.cpu.bus.region()
    }

    pub fn bus(&self) -> &Bus {
        &self.cpu.bus
    }

    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.cpu.bus
    }
}

const BUTTONS: [ButtonInfo; 8] = [
    ButtonInfo { name: "A", bit: 0 },
    ButtonInfo { name: "B", bit: 1 },
    ButtonInfo { name: "Select", bit: 2 },
    ButtonInfo { name: "Start", bit: 3 },
    ButtonInfo { name: "Up", bit: 4 },
    ButtonInfo { name: "Down", bit: 5 },
    ButtonInfo { name: "Left", bit: 6 },
    ButtonInfo { name: "Right", bit: 7 },
];

impl Machine for Nes {
    fn reset(&mut self) {
        Nes::reset(self);
    }

    fn run_frame(&mut self) -> bool {
        Nes::run_frame(self)
    }

    fn video(&self) -> VideoFrame<'_> {
        VideoFrame {
            width: crate::emulator::ppu::SCREEN_WIDTH,
            height: crate::emulator::ppu::SCREEN_HEIGHT,
            rgba: self.framebuffer(),
        }
    }

    fn frame_rate(&self) -> f64 {
        self.region().frame_rate()
    }

    fn set_audio_sample_rate(&mut self, hz: u32) {
        self.set_sample_rate(hz);
    }

    fn drain_audio(&mut self, out: &mut Vec<f32>) {
        Nes::drain_audio(self, out);
    }

    fn buttons(&self) -> &'static [ButtonInfo] {
        &BUTTONS
    }

    fn set_input(&mut self, port: usize, pressed: u32) {
        self.set_buttons(port, Buttons::from_bits_truncate(pressed as u8));
    }

    fn save_state(&self) -> Vec<u8> {
        Nes::save_state(self)
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), StateError> {
        Nes::load_state(self, data)
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        self.cpu.bus.cartridge.battery_ram()
    }

    fn load_battery_ram(&mut self, data: &[u8]) {
        self.cpu.bus.cartridge.load_battery_ram(data);
    }
}
