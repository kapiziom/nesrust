pub mod cpu_bus;
pub use mos6502::mock_bus;

use crate::emulator::apu::{self, Apu};
use crate::emulator::cartridge::Cartridge;
use crate::emulator::controller::{Buttons, Controller};
use crate::emulator::ppu::PPU;
use crate::emulator::ram::RAM;
use crate::emulator::region::Region;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Bus {
    ram: RAM,
    ppu: PPU,
    apu: Apu,
    /// Not part of the state: ROM and mapper stay with the running console (see `Nes::load_state`).
    #[serde(skip, default = "Cartridge::placeholder")]
    pub cartridge: Cartridge,
    pub nmi_interrupt: Option<u8>,
    cycles: usize,
    /// Last value that travelled over the data bus; returned by reads of unmapped addresses (open bus).
    last_bus_value: u8,
    /// Source page of a pending $4014 OAM DMA; the CPU picks it up after the write instruction.
    oam_dma: Option<u8>,
    frame_complete: bool,
    region: Region,
    /// PPU dots left over from the fractional dots-per-cycle ratio (PAL: 3.2), in 1/5 dot units.
    dot_remainder: u8,
    /// Dots of the current CPU cycle that run after the bus access.
    dots_after_access: u8,
    /// Controller ports 1 ($4016) and 2 ($4017).
    controllers: [Controller; 2],
    // todo
    // apu: APU,
}

impl Bus {
    /// After a save state was deserialized into `self`: takes over what is not part of the state from the console
    /// that was running (cartridge with its ROM, palette, picture, audio output rate).
    pub(crate) fn take_runtime_from(&mut self, running: &mut Bus) {
        std::mem::swap(&mut self.cartridge, &mut running.cartridge);
        self.ppu.take_output_from(&mut running.ppu);
        // changing the rate restarts the resampler, so only do it when the host's rate differs from the saved one
        if self.apu.sample_rate() != running.apu.sample_rate() {
            self.apu.set_sample_rate(running.apu.sample_rate());
        }
    }

    pub fn new(cartridge: Cartridge) -> Self {
        let region = cartridge.region();
        Self {
            ram: RAM::new(),
            ppu: PPU::with_region(region),
            apu: Apu::with_region(apu::DEFAULT_SAMPLE_RATE, region),
            cartridge,
            nmi_interrupt: None,
            cycles: 0,
            last_bus_value: 0,
            oam_dma: None,
            frame_complete: false,
            region,
            dot_remainder: 0,
            dots_after_access: 0,
            controllers: [Controller::new(), Controller::new()],
        }
    }

    /// Sets the buttons held on controller `port` (0 or 1). Out-of-range ports are ignored.
    /// The buttons currently held on controller `port`.
    pub fn buttons(&self, port: usize) -> Buttons {
        self.controllers.get(port).map(Controller::buttons).unwrap_or_default()
    }

    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        if let Some(controller) = self.controllers.get_mut(port) {
            controller.set_buttons(buttons);
        }
    }

    pub fn ppu(&self) -> &PPU {
        &self.ppu
    }

    pub fn ppu_mut(&mut self) -> &mut PPU {
        &mut self.ppu
    }

    pub fn region(&self) -> Region {
        self.region
    }

    pub fn apu(&self) -> &Apu {
        &self.apu
    }

    pub fn apu_mut(&mut self) -> &mut Apu {
        &mut self.apu
    }

    /// True once per frame (the PPU reached vblank). Reading clears it.
    pub fn take_frame_complete(&mut self) -> bool {
        std::mem::take(&mut self.frame_complete)
    }

    /// PPU dots that have run when the bus access of a CPU cycle happens (counted from the start of the cycle,
    /// out of 3 on NTSC or 3-4 on PAL). Calibrated against Blargg's ppu_vbl_nmi tests on NTSC, which check VBL/NMI
    /// timing to one PPU clock; the PAL values are the same and have not been verified.
    pub(crate) const READ_DOTS_BEFORE: u8 = 2;
    pub(crate) const WRITE_DOTS_BEFORE: u8 = 3;

    /// Advances idle CPU cycles (no bus access).
    pub fn tick(&mut self, cycles: u16) {
        for _ in 0..cycles {
            self.start_cycle(u8::MAX);
            self.end_cycle();
        }
    }

    /// Starts a CPU cycle, running the PPU up to the point of the bus access (`dots_before` of this cycle's dots).
    fn start_cycle(&mut self, dots_before: u8) {
        self.cycles += 1;
        self.cartridge.cpu_cycle();

        let (dots, cycles) = self.region.dots_per_cycles();
        self.dot_remainder += dots;
        let total = self.dot_remainder / cycles;
        self.dot_remainder %= cycles;

        let before = dots_before.min(total);
        self.dots_after_access = total - before;
        self.run_ppu(before);
    }

    fn end_cycle(&mut self) {
        let after = std::mem::take(&mut self.dots_after_access);
        self.run_ppu(after);
        // the APU works at the end of the cycle, after the CPU's access: a register write is seen by it in that
        // same cycle
        self.apu.tick_with_expansion(self.cartridge.audio_output());

        // the CPU's NMI edge detector looks at the line at the end of every cycle
        if self.ppu.take_nmi() {
            self.nmi_interrupt = Some(0xFF);
        }
    }

    fn run_ppu(&mut self, dots: u8) {
        for _ in 0..dots {
            self.ppu.tick(&mut self.cartridge);

            if self.ppu.take_frame_complete() {
                self.frame_complete = true;
            }
        }
    }
}
