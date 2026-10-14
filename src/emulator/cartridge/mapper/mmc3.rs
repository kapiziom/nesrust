//! Mapper 4 (MMC3, TxROM): 8KB PRG banks, 1KB/2KB CHR banks and a scanline counter clocked by PPU address line A12.
//! https://www.nesdev.org/wiki/MMC3

use super::{mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;
/// A12 must have been low for this many CPU cycles before a rising edge counts (filters the PPU's rapid toggling).
const A12_LOW_CYCLES_REQUIRED: u8 = 3;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Mmc3 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    board_mirroring: Mirroring,
    /// Mapper 206 (Namco 108 family): MMC3 banking without PRG/CHR swap modes, mirroring control, PRG-RAM or IRQ.
    simple: bool,

    bank_select: u8,
    registers: [u8; 8],
    horizontal_mirroring: bool,
    ram_enabled: bool,
    ram_write_protected: bool,

    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_pending: bool,
    last_a12: bool,
    a12_low_cycles: u8,
}

impl Mmc3 {
    pub fn new(file: INesFile) -> Self {
        let simple = file.mapper == 206;
        Mmc3 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(if simple { 0 } else { file.prg_ram_size }, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            board_mirroring: file.mirroring,
            simple,
            bank_select: 0,
            registers: [0, 2, 4, 5, 6, 7, 0, 1],
            horizontal_mirroring: file.mirroring != Mirroring::Vertical,
            ram_enabled: true,
            ram_write_protected: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_pending: false,
            last_a12: false,
            a12_low_cycles: 0,
        }
    }

    fn prg_banks(&self) -> usize {
        (self.prg_rom.len() / PRG_BANK_SIZE).max(1)
    }

    fn prg_index(&self, addr: u16) -> usize {
        let banks = self.prg_banks();
        let swapped = !self.simple && self.bank_select & 0x40 != 0;
        let mask = if self.simple { 0x0F } else { 0x3F };

        let bank = match ((addr as usize - 0x8000) / PRG_BANK_SIZE, swapped) {
            (0, false) | (2, true) => self.registers[6] as usize & mask,
            (0, true) | (2, false) => banks.saturating_sub(2),
            (1, _) => self.registers[7] as usize & mask,
            _ => banks - 1,
        };

        ((bank % banks) * PRG_BANK_SIZE + (addr as usize & 0x1FFF)) % self.prg_rom.len()
    }

    /// Byte offset in CHR memory of a PPU address ($0000-$1FFF).
    fn chr_offset(&self, addr: u16) -> usize {
        let mut addr = addr as usize & 0x1FFF;

        // CHR inversion swaps the two 4KB halves
        if !self.simple && self.bank_select & 0x80 != 0 {
            addr ^= 0x1000;
        }

        // the Namco 108 only has six CHR address lines
        let mask = if self.simple { 0x3F } else { 0xFF };

        let bank = if addr < 0x1000 {
            // R0 and R1 are 2KB banks: the low bit is ignored and the odd half follows the even one
            let register = self.registers[addr / 0x800] as usize & mask & !1;
            register + (addr % 0x800) / CHR_BANK_SIZE
        } else {
            self.registers[2 + (addr - 0x1000) / CHR_BANK_SIZE] as usize & mask
        };

        bank * CHR_BANK_SIZE + (addr % CHR_BANK_SIZE)
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }

        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }

    /// The counter is clocked by a filtered rising edge of PPU A12 (sprites at $1000, background at $0000).
    fn watch_a12(&mut self, addr: u16) {
        if self.simple {
            return;
        }

        let a12 = addr & 0x1000 != 0;

        if a12 && !self.last_a12 && self.a12_low_cycles >= A12_LOW_CYCLES_REQUIRED {
            self.clock_irq_counter();
        }

        if a12 {
            self.a12_low_cycles = 0;
        }
        self.last_a12 = a12;
    }
}

impl Mapper for Mmc3 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if self.ram_enabled => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        let even = addr & 1 == 0;

        if self.simple && !(0x8000..=0x9FFF).contains(&addr) {
            return;
        }

        match addr {
            0x6000..=0x7FFF => {
                if self.ram_enabled && !self.ram_write_protected {
                    self.prg_ram.write(addr as usize - 0x6000, data);
                }
            }
            0x8000..=0x9FFF if even => self.bank_select = data,
            0x8000..=0x9FFF => self.registers[(self.bank_select & 0x07) as usize] = data,
            0xA000..=0xBFFF if even => self.horizontal_mirroring = data & 1 != 0,
            0xA000..=0xBFFF => {
                self.ram_enabled = data & 0x80 != 0;
                self.ram_write_protected = data & 0x40 != 0;
            }
            0xC000..=0xDFFF if even => self.irq_latch = data,
            0xC000..=0xDFFF => {
                self.irq_counter = 0;
                self.irq_reload = true;
            }
            0xE000..=0xFFFF if even => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE000..=0xFFFF => self.irq_enabled = true,
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.watch_a12(addr);
        self.chr.read(self.chr_offset(addr))
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.watch_a12(addr);
        self.chr.write(self.chr_offset(addr), data);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.watch_a12(addr);
    }

    fn mirroring(&self) -> Mirroring {
        if self.simple || self.board_mirroring == Mirroring::FourScreen {
            self.board_mirroring
        } else if self.horizontal_mirroring {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if !self.last_a12 {
            self.a12_low_cycles = self.a12_low_cycles.saturating_add(1);
        }
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        self.prg_ram.battery_ram()
    }

    fn load_battery_ram(&mut self, data: &[u8]) {
        self.prg_ram.load(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    // test_util: PRG byte = 8KB unit number, CHR byte = 1KB unit number

    fn mapper() -> Mmc3 {
        Mmc3::new(file(4, 16, 16)) // 256KB PRG = 32 units of 8KB, 128KB CHR = 128 units of 1KB
    }

    fn select(m: &mut Mmc3, register: u8, bank_select_flags: u8, value: u8) {
        m.cpu_write(0x8000, register | bank_select_flags);
        m.cpu_write(0x8001, value);
    }

    fn namco108() -> Mmc3 {
        Mmc3::new(file(206, 16, 8)) // 32 PRG units of 8KB, 64 CHR units of 1KB
    }

    #[test]
    fn namco_108_has_no_swap_modes_and_masks_the_bank_numbers() {
        let mut m = namco108();
        select(&mut m, 6, 0x40, 0x15); // swap flag ignored, bank number limited to 4 bits
        select(&mut m, 7, 0x80, 9);
        select(&mut m, 0, 0x80, 0x45); // CHR inversion ignored, 6-bit 2KB bank: 0x05 & !1 = 4

        assert_eq!(m.cpu_read(0x8000), Some(5));
        assert_eq!(m.cpu_read(0xA000), Some(9));
        assert_eq!(m.cpu_read(0xC000), Some(30));
        assert_eq!(m.ppu_read(0x0000), 4);
        assert_eq!(m.ppu_read(0x0400), 5);
    }

    #[test]
    fn namco_108_ignores_mirroring_ram_and_irq_registers() {
        let mut m = namco108();
        m.cpu_write(0xA000, 1);
        m.cpu_write(0x6000, 0x42);
        m.cpu_write(0xC000, 0);
        m.cpu_write(0xC001, 0);
        m.cpu_write(0xE001, 0);

        assert_eq!(m.mirroring(), Mirroring::Vertical, "fixed by the board");
        assert_eq!(m.cpu_read(0x6000), None);
        for _ in 0..10 {
            m.ppu_read(0x0000);
            m.ppu_read(0x1000);
        }
        assert!(!m.irq_pending());
    }

    #[test]
    fn prg_mode_0_switches_8000_and_a000() {
        let mut m = mapper();
        select(&mut m, 6, 0, 5);
        select(&mut m, 7, 0, 9);

        assert_eq!(m.cpu_read(0x8000), Some(5));
        assert_eq!(m.cpu_read(0xA000), Some(9));
        assert_eq!(m.cpu_read(0xC000), Some(30), "second-to-last bank");
        assert_eq!(m.cpu_read(0xE000), Some(31), "last bank");
    }

    #[test]
    fn prg_mode_1_swaps_the_8000_and_c000_banks() {
        let mut m = mapper();
        select(&mut m, 6, 0x40, 5);
        select(&mut m, 7, 0x40, 9);

        assert_eq!(m.cpu_read(0x8000), Some(30));
        assert_eq!(m.cpu_read(0xA000), Some(9));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn chr_without_inversion() {
        let mut m = mapper();
        select(&mut m, 0, 0, 10); // 2KB at $0000: banks 10,11
        select(&mut m, 1, 0, 20); // 2KB at $0800: banks 20,21
        for (register, bank) in [(2, 30), (3, 31), (4, 32), (5, 33)] {
            select(&mut m, register, 0, bank);
        }

        assert_eq!(m.ppu_read(0x0000), 10);
        assert_eq!(m.ppu_read(0x0400), 11);
        assert_eq!(m.ppu_read(0x0800), 20);
        assert_eq!(m.ppu_read(0x0C00), 21);
        assert_eq!(m.ppu_read(0x1000), 30);
        assert_eq!(m.ppu_read(0x1400), 31);
        assert_eq!(m.ppu_read(0x1800), 32);
        assert_eq!(m.ppu_read(0x1C00), 33);
    }

    #[test]
    fn chr_inversion_swaps_the_halves() {
        let mut m = mapper();
        select(&mut m, 0, 0x80, 10);
        select(&mut m, 2, 0x80, 30);

        assert_eq!(m.ppu_read(0x1000), 10);
        assert_eq!(m.ppu_read(0x1400), 11);
        assert_eq!(m.ppu_read(0x0000), 30);
    }

    #[test]
    fn two_kb_chr_registers_ignore_the_low_bit() {
        let mut m = mapper();
        select(&mut m, 0, 0, 11);

        assert_eq!(m.ppu_read(0x0000), 10);
        assert_eq!(m.ppu_read(0x0400), 11);
    }

    #[test]
    fn mirroring_register() {
        let mut m = mapper();
        m.cpu_write(0xA000, 0);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
        m.cpu_write(0xA000, 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn four_screen_boards_ignore_the_mirroring_register() {
        let mut data = file(4, 16, 16);
        data.mirroring = Mirroring::FourScreen;
        let mut m = Mmc3::new(data);
        m.cpu_write(0xA000, 1);

        assert_eq!(m.mirroring(), Mirroring::FourScreen);
    }

    #[test]
    fn prg_ram_enable_and_write_protect() {
        let mut m = mapper();
        m.cpu_write(0x6000, 0x11);
        assert_eq!(m.cpu_read(0x6000), Some(0x11));

        m.cpu_write(0xA001, 0xC0); // enabled + write protected
        m.cpu_write(0x6000, 0x22);
        assert_eq!(m.cpu_read(0x6000), Some(0x11));

        m.cpu_write(0xA001, 0x00); // disabled
        assert_eq!(m.cpu_read(0x6000), None);
    }

    /// One scanline's worth of A12 activity: background pattern fetches low, then sprite fetches high.
    fn scanline(m: &mut Mmc3) {
        m.ppu_read(0x0000);
        for _ in 0..4 {
            m.cpu_cycle();
        }
        m.ppu_read(0x1000);
    }

    #[test]
    fn irq_fires_after_latch_plus_one_scanlines() {
        let mut m = mapper();
        m.cpu_write(0xC000, 2); // latch
        m.cpu_write(0xC001, 0); // reload
        m.cpu_write(0xE001, 0); // enable

        scanline(&mut m); // reload to 2
        scanline(&mut m); // 1
        assert!(!m.irq_pending());
        scanline(&mut m); // 0 -> IRQ
        assert!(m.irq_pending());
    }

    #[test]
    fn irq_is_acknowledged_by_the_disable_register() {
        let mut m = mapper();
        m.cpu_write(0xC000, 0);
        m.cpu_write(0xE001, 0);
        scanline(&mut m);
        assert!(m.irq_pending());

        m.cpu_write(0xE000, 0);
        assert!(!m.irq_pending());

        scanline(&mut m);
        assert!(!m.irq_pending(), "disabled counters do not raise IRQ");
    }

    #[test]
    fn a12_glitches_shorter_than_three_cpu_cycles_do_not_clock() {
        let mut m = mapper();
        m.cpu_write(0xC000, 0);
        m.cpu_write(0xE001, 0);

        m.cpu_cycle();
        m.ppu_read(0x0000);
        m.ppu_read(0x1000); // low for only 1 cycle

        assert!(!m.irq_pending());
    }

    #[test]
    fn counter_reloads_when_it_reaches_zero() {
        let mut m = mapper();
        m.cpu_write(0xC000, 1);
        m.cpu_write(0xC001, 0);
        m.cpu_write(0xE001, 0);

        scanline(&mut m); // reload to 1
        scanline(&mut m); // 0 -> IRQ
        assert!(m.irq_pending());

        m.cpu_write(0xE000, 0);
        m.cpu_write(0xE001, 0);
        scanline(&mut m); // counter was 0: reload to 1
        assert!(!m.irq_pending());
        scanline(&mut m); // 0 -> IRQ again
        assert!(m.irq_pending());
    }
}
