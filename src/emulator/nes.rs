use crate::emulator::bus::Bus;
use crate::emulator::cartridge::Cartridge;
use crate::emulator::cpu::CPU;

/// The whole console: a CPU wired to the bus (RAM, PPU, cartridge).
/// This is the entry point a frontend or a test runner uses.
pub struct Nes {
    pub cpu: CPU<Bus>,
}

impl Nes {
    /// Creates the console and runs the reset sequence, so execution starts at the cartridge's reset vector.
    pub fn new(cartridge: Cartridge) -> Self {
        let mut cpu = CPU::new(Bus::new(cartridge));
        cpu.reset();
        Nes { cpu }
    }

    pub fn reset(&mut self) {
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

    /// RGBA, 256x240.
    pub fn framebuffer(&self) -> &[u8] {
        self.cpu.bus.ppu().framebuffer()
    }

    pub fn bus(&self) -> &Bus {
        &self.cpu.bus
    }

    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.cpu.bus
    }
}
