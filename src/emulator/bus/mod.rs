pub mod cpu_bus;
pub mod mock_bus;

use crate::emulator::ppu::PPU;
use crate::emulator::cartridge::Cartridge;
use crate::emulator::ram::RAM;

pub struct Bus {
    ram: RAM,
    ppu: PPU,
    pub cartridge: Cartridge,
    pub nmi_interrupt: Option<u8>,
    cycles: usize,
    /// Last value that travelled over the data bus; returned by reads of unmapped addresses (open bus).
    last_bus_value: u8,
    /// Source page of a pending $4014 OAM DMA; the CPU picks it up after the write instruction.
    oam_dma: Option<u8>,
    frame_complete: bool,
    // todo
    // apu: APU,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            ram: RAM::new(),
            ppu: PPU::new(),
            cartridge,
            nmi_interrupt: None,
            cycles: 0,
            last_bus_value: 0,
            oam_dma: None,
            frame_complete: false,
        }
    }

    pub fn ppu(&self) -> &PPU {
        &self.ppu
    }

    pub fn ppu_mut(&mut self) -> &mut PPU {
        &mut self.ppu
    }

    /// True once per frame (the PPU reached vblank). Reading clears it.
    pub fn take_frame_complete(&mut self) -> bool {
        std::mem::take(&mut self.frame_complete)
    }

    /// PPU dots (of the 3 per CPU cycle) that have run when the bus access of a cycle happens. Calibrated against
    /// Blargg's ppu_vbl_nmi tests, which check VBL/NMI timing to one PPU clock.
    const READ_DOTS_BEFORE: u8 = 2;
    const WRITE_DOTS_BEFORE: u8 = 3;

    /// Advances idle CPU cycles (no bus access).
    pub fn tick(&mut self, cycles: u16) {
        for _ in 0..cycles {
            self.start_cycle(3);
            self.end_cycle(0);
        }
    }

    fn start_cycle(&mut self, dots: u8) {
        self.cycles += 1;
        self.cartridge.cpu_cycle();
        self.run_ppu(dots);
    }

    fn end_cycle(&mut self, dots: u8) {
        self.run_ppu(dots);

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
