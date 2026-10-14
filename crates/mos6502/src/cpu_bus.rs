pub trait CpuBus {
    /// Reads can have side effects on real hardware (PPU status, controller shift registers), hence `&mut self`.
    fn read(&mut self, addr: u16) -> u8;

    fn read_u16(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    fn write(&mut self, addr: u16, data: u8);

    fn write_u16(&mut self, addr: u16, data: u16) {
        let lo = (data & 0xFF) as u8;
        let hi = (data >> 8) as u8;
        self.write(addr, lo);
        self.write(addr.wrapping_add(1), hi);
    }

    /// Advances time by whole CPU cycles in which the bus is not used.
    fn tick(&mut self, cycles: u8);

    /// A CPU cycle with a bus access is split in two: the part of the cycle before the access and the rest.
    /// Chips that run in parallel (the PPU) are advanced accordingly, so a register read sees the PPU at the
    /// right dot. The defaults treat the whole cycle as happening before the access.
    fn tick_before_access(&mut self, _write: bool) {
        self.tick(1);
    }

    fn tick_after_access(&mut self, _write: bool) {}

    fn fetch_nmi(&mut self) -> Option<u8>;

    /// Level of the IRQ line (APU frame counter/DMC, mappers). Stays true until the source is acknowledged.
    fn irq_pending(&self) -> bool {
        false
    }

    /// Address of a sample byte the APU's DMC is waiting for. The CPU performs the read (stalling for a few cycles)
    /// and hands the byte back through `dmc_dma_done`.
    fn take_dmc_dma(&mut self) -> Option<u16> {
        None
    }

    /// The fetch returned by `take_dmc_dma` was started by a $4015 write (3 cycles, no alignment cycle).
    fn dmc_dma_is_load(&self) -> bool {
        false
    }

    fn dmc_dma_done(&mut self, _value: u8) {}

    /// Source page of an OAM DMA started by a write to $4014, if one is waiting to be run (taking it clears it).
    fn take_oam_dma(&mut self) -> Option<u8> {
        None
    }
}
