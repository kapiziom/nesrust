//! Mapper 4 (MMC3, TxROM): 8KB PRG banks, 1KB/2KB CHR banks and a scanline counter clocked by PPU address line A12.
//! The same file serves the boards built from the same registers: the Namco 108 family (206, 76, 88, 95, 154: no
//! swap modes, IRQ or PRG-RAM), TxSROM (118: mirroring from CHR bank bit 7) and TQROM (119: CHR-ROM and CHR-RAM).
//! https://www.nesdev.org/wiki/MMC3, https://www.nesdev.org/wiki/INES_Mapper_118

use super::a12_counter::A12Counter;
use super::{mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Kind {
    Mmc3,
    /// Mapper 206.
    Namco108,
    /// CHR as four 2KB banks selected by R2-R5.
    Mapper76,
    /// CHR banks of the lower half only for the 2KB banks, of the upper half only for the 1KB banks.
    Mapper88,
    /// Namco 3425: bit 5 of R0/R1 chooses the nametable page.
    Mapper95,
    /// Like 88, plus a single-screen select in bit 6 of every write.
    Mapper154,
    /// Bit 7 of the CHR bank registers chooses the nametable page.
    TxSrom,
    /// Bit 6 of the CHR bank registers switches the bank to 8KB of CHR-RAM.
    TqRom,
}

impl Kind {
    /// The Namco 108 family has only the two bank registers pairs: no swap modes, mirroring control, RAM or IRQ.
    fn simple(self) -> bool {
        !matches!(self, Kind::Mmc3 | Kind::TxSrom | Kind::TqRom)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Mmc3 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    board_mirroring: Mirroring,
    kind: Kind,
    /// TQROM's 8KB of CHR-RAM next to the CHR-ROM.
    tqrom_ram: Vec<u8>,
    /// Mapper 154's single-screen select.
    single_screen_upper: bool,

    bank_select: u8,
    registers: [u8; 8],
    horizontal_mirroring: bool,
    ram_enabled: bool,
    ram_write_protected: bool,

    irq: A12Counter,
}

impl Mmc3 {
    pub fn new(file: INesFile) -> Self {
        let kind = match file.mapper {
            206 => Kind::Namco108,
            76 => Kind::Mapper76,
            88 => Kind::Mapper88,
            95 => Kind::Mapper95,
            154 => Kind::Mapper154,
            118 => Kind::TxSrom,
            119 => Kind::TqRom,
            _ => Kind::Mmc3,
        };
        let simple = kind.simple();
        Mmc3 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(if simple { 0 } else { file.prg_ram_size }, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            board_mirroring: file.mirroring,
            kind,
            tqrom_ram: if kind == Kind::TqRom { vec![0; 8 * 1024] } else { Vec::new() },
            single_screen_upper: false,
            bank_select: 0,
            registers: [0, 2, 4, 5, 6, 7, 0, 1],
            horizontal_mirroring: file.mirroring != Mirroring::Vertical,
            ram_enabled: true,
            ram_write_protected: false,
            irq: A12Counter::default(),
        }
    }

    fn prg_banks(&self) -> usize {
        (self.prg_rom.len() / PRG_BANK_SIZE).max(1)
    }

    fn prg_index(&self, addr: u16) -> usize {
        let banks = self.prg_banks();
        let swapped = !self.kind.simple() && self.bank_select & 0x40 != 0;
        let mask = if self.kind.simple() { 0x0F } else { 0x3F };

        let bank = match ((addr as usize - 0x8000) / PRG_BANK_SIZE, swapped) {
            (0, false) | (2, true) => self.registers[6] as usize & mask,
            (0, true) | (2, false) => banks.saturating_sub(2),
            (1, _) => self.registers[7] as usize & mask,
            _ => banks - 1,
        };

        ((bank % banks) * PRG_BANK_SIZE + (addr as usize & 0x1FFF)) % self.prg_rom.len()
    }

    /// The 1KB CHR bank (with the board's special bits removed) behind a PPU address in $0000-$1FFF, the offset in
    /// it, and whether TQROM selected its CHR-RAM.
    fn chr_bank(&self, addr: u16) -> (usize, usize, bool) {
        let mut addr = addr as usize & 0x1FFF;

        // CHR inversion swaps the two 4KB halves
        if !self.kind.simple() && self.bank_select & 0x80 != 0 {
            addr ^= 0x1000;
        }

        let slot = addr / CHR_BANK_SIZE;
        let raw = match self.kind {
            // four 2KB banks selected by R2-R5
            Kind::Mapper76 => (self.registers[2 + slot / 2] as usize & 0x3F) * 2 + (slot & 1),
            _ if addr < 0x1000 => {
                // R0 and R1 are 2KB banks: the low bit is ignored and the odd half follows the even one
                (self.registers[addr / 0x800] as usize & !1) + (slot & 1)
            }
            _ => self.registers[2 + slot - 4] as usize,
        };

        let offset = addr % CHR_BANK_SIZE;
        match self.kind {
            Kind::Mmc3 => (raw, offset, false),
            Kind::Namco108 | Kind::Mapper95 | Kind::Mapper76 => (raw & 0x3F, offset, false),
            Kind::Mapper88 | Kind::Mapper154 => {
                (if addr < 0x1000 { raw & 0x3F } else { (raw & 0x3F) | 0x40 }, offset, false)
            }
            Kind::TxSrom => (raw & 0x7F, offset, false),
            Kind::TqRom => (raw & 0x3F, offset, raw & 0x40 != 0),
        }
    }

    fn chr_read(&self, addr: u16) -> u8 {
        let (bank, offset, ram) = self.chr_bank(addr);
        if ram {
            self.tqrom_ram[(bank & 7) * CHR_BANK_SIZE + offset]
        } else {
            self.chr.read(bank * CHR_BANK_SIZE + offset)
        }
    }

    fn chr_write(&mut self, addr: u16, data: u8) {
        let (bank, offset, ram) = self.chr_bank(addr);
        if ram {
            self.tqrom_ram[(bank & 7) * CHR_BANK_SIZE + offset] = data;
        } else {
            self.chr.write(bank * CHR_BANK_SIZE + offset, data);
        }
    }

    /// The nametable page chosen by a CHR bank register's bit (TxSROM: bit 7, mapper 95: bit 5).
    fn nametable_pages(&self, bit: u8) -> [u8; 4] {
        let page = |register: usize| (self.registers[register] >> bit) & 1;
        if self.kind == Kind::TxSrom && self.bank_select & 0x80 != 0 {
            [page(2), page(3), page(4), page(5)]
        } else {
            [page(0), page(0), page(1), page(1)]
        }
    }

    /// The counter is clocked by a filtered rising edge of PPU A12 (sprites at $1000, background at $0000).
    fn watch_a12(&mut self, addr: u16) {
        if !self.kind.simple() {
            self.irq.watch(addr);
        }
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

        if self.kind == Kind::Mapper154 && addr >= 0x8000 {
            self.single_screen_upper = data & 0x40 != 0;
        }
        if self.kind.simple() && !(0x8000..=0x9FFF).contains(&addr) {
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
            0xC000..=0xDFFF if even => self.irq.set_latch(data),
            0xC000..=0xDFFF => self.irq.request_reload(),
            0xE000..=0xFFFF => self.irq.set_enabled(!even),
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.watch_a12(addr);
        self.chr_read(addr)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.watch_a12(addr);
        self.chr_write(addr, data);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.watch_a12(addr);
    }

    fn mirroring(&self) -> Mirroring {
        if self.board_mirroring == Mirroring::FourScreen {
            return Mirroring::FourScreen;
        }

        match self.kind {
            Kind::Namco108 | Kind::Mapper76 | Kind::Mapper88 => return self.board_mirroring,
            Kind::Mapper95 => return Mirroring::Custom(self.nametable_pages(5)),
            Kind::TxSrom => return Mirroring::Custom(self.nametable_pages(7)),
            Kind::Mapper154 => {
                return if self.single_screen_upper {
                    Mirroring::SingleScreenUpper
                } else {
                    Mirroring::SingleScreenLower
                };
            }
            Kind::Mmc3 | Kind::TqRom => {}
        }

        if self.horizontal_mirroring {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    fn cpu_cycle(&mut self) {
        self.irq.cpu_cycle();
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

    // ---- the boards built from the same registers ----

    fn variant(mapper: u16) -> Mmc3 {
        Mmc3::new(file(mapper, 16, 16)) // 128 CHR units of 1KB
    }

    #[test]
    fn mapper_76_has_four_2kb_chr_banks_selected_by_r2_to_r5() {
        let mut m = variant(76);
        for (register, value) in [(2u8, 3u8), (3, 4), (4, 5), (5, 6)] {
            select(&mut m, register, 0, value);
        }

        assert_eq!(m.ppu_read(0x0000), 6);
        assert_eq!(m.ppu_read(0x0400), 7);
        assert_eq!(m.ppu_read(0x0800), 8);
        assert_eq!(m.ppu_read(0x1000), 10);
        assert_eq!(m.ppu_read(0x1C00), 13);
    }

    #[test]
    fn mappers_88_and_154_keep_the_two_chr_halves_in_different_64kb() {
        for id in [88u16, 154] {
            let mut m = variant(id);
            select(&mut m, 0, 0, 0xC4); // 2KB bank in the lower 64KB: 4
            select(&mut m, 2, 0, 0x05); // 1KB bank in the upper 64KB: 0x45

            assert_eq!(m.ppu_read(0x0000), 4, "mapper {id}");
            assert_eq!(m.ppu_read(0x0400), 5);
            assert_eq!(m.ppu_read(0x1000), 0x45);
        }
    }

    #[test]
    fn mapper_154_selects_a_single_screen_with_bit_6_of_any_write() {
        let mut m = variant(154);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);

        m.cpu_write(0xE000, 0x40);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
        m.cpu_write(0x8000, 0x00);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn dragon_buster_picks_the_nametable_pages_from_bit_5_of_r0_and_r1() {
        let mut m = variant(95);
        select(&mut m, 0, 0, 0x20);
        select(&mut m, 1, 0, 0x00);

        assert_eq!(m.mirroring(), Mirroring::Custom([1, 1, 0, 0]));
        select(&mut m, 1, 0, 0x22);
        assert_eq!(m.mirroring(), Mirroring::Custom([1, 1, 1, 1]));
    }

    #[test]
    fn txsrom_takes_the_nametable_pages_from_bit_7_of_the_chr_banks() {
        let mut m = variant(118);
        select(&mut m, 0, 0, 0x80);
        select(&mut m, 1, 0, 0x02);
        assert_eq!(m.mirroring(), Mirroring::Custom([1, 1, 0, 0]), "no CHR inversion: R0 and R1");

        for (register, value) in [(2u8, 0x80u8), (3, 0), (4, 0x80), (5, 0)] {
            select(&mut m, register, 0x80, value); // CHR inversion: the 1KB registers
        }
        assert_eq!(m.mirroring(), Mirroring::Custom([1, 0, 1, 0]));
        assert_eq!(m.ppu_read(0x0000), 0, "the flag bit is not part of the CHR bank number");
    }

    #[test]
    fn tqrom_switches_single_chr_banks_to_ram() {
        let mut m = variant(119);
        select(&mut m, 2, 0, 0x40 | 3); // 1KB bank at $1000 is RAM page 3
        select(&mut m, 3, 0, 5);

        m.ppu_write(0x1000, 0x77);
        m.ppu_write(0x1400, 0x88); // ROM: ignored

        assert_eq!(m.ppu_read(0x1000), 0x77);
        assert_eq!(m.ppu_read(0x1400), 5);
        select(&mut m, 2, 0, 3);
        assert_eq!(m.ppu_read(0x1000), 3, "back to CHR-ROM");
        select(&mut m, 2, 0, 0x40 | 3);
        assert_eq!(m.ppu_read(0x1000), 0x77, "the RAM kept its contents");
    }
}
