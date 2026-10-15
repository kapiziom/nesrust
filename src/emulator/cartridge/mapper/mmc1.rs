//! Mapper 1 (MMC1, SxROM): five serial writes load one of four 5-bit registers.
//! https://www.nesdev.org/wiki/MMC1

use super::{mapper_state, wrap, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 4 * 1024;
/// SUROM boards (512KB PRG) use a CHR bank bit to choose between two 256KB halves.
const PRG_WINDOW: usize = 256 * 1024;
/// A set bit 4 marks the start of the shift register; when it reaches bit 0 the fifth write has arrived.
const SHIFT_RESET: u8 = 0x10;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Mmc1 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    shift: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
    cycles: u64,
    last_write_cycle: Option<u64>,
}

impl Mmc1 {
    pub fn new(file: INesFile) -> Self {
        Mmc1 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            shift: SHIFT_RESET,
            control: 0x0C, // power-on: last PRG bank fixed at $C000
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
            cycles: 0,
            last_write_cycle: None,
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let window = self.prg_rom.len().min(PRG_WINDOW);
        let banks = (window / PRG_BANK_SIZE).max(1);
        let outer = if self.prg_rom.len() > PRG_WINDOW && self.chr_bank_0 & 0x10 != 0 { PRG_WINDOW } else { 0 };

        let upper_half = addr >= 0xC000;
        let prg_bank = (self.prg_bank & 0x0F) as usize;

        let bank = match (self.control >> 2) & 0x03 {
            // 32KB mode: low bit of the bank number is ignored
            0 | 1 => (prg_bank & !1) | upper_half as usize,
            // first bank fixed at $8000, switchable at $C000
            2 => {
                if upper_half {
                    prg_bank
                } else {
                    0
                }
            }
            // switchable at $8000, last bank fixed at $C000
            _ => {
                if upper_half {
                    banks - 1
                } else {
                    prg_bank
                }
            }
        };

        wrap(outer + wrap(bank, banks) * PRG_BANK_SIZE + (addr as usize & 0x3FFF), self.prg_rom.len())
    }

    /// Byte offset in CHR memory of a PPU address.
    fn chr_offset(&self, addr: u16) -> usize {
        let addr = addr as usize & 0x1FFF;

        if self.control & 0x10 == 0 {
            // 8KB mode: bank 0 selects an even/odd pair, bit 0 ignored
            (self.chr_bank_0 as usize & !1) * CHR_BANK_SIZE + addr
        } else if addr < 0x1000 {
            self.chr_bank_0 as usize * CHR_BANK_SIZE + addr
        } else {
            self.chr_bank_1 as usize * CHR_BANK_SIZE + (addr - 0x1000)
        }
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        match (addr >> 13) & 0x03 {
            0 => self.control = value,
            1 => self.chr_bank_0 = value,
            2 => self.chr_bank_1 = value,
            _ => self.prg_bank = value,
        }
    }

    fn write_serial(&mut self, addr: u16, data: u8) {
        if data & 0x80 != 0 {
            self.shift = SHIFT_RESET;
            self.control |= 0x0C;
            return;
        }

        let complete = self.shift & 1 != 0;
        self.shift = (self.shift >> 1) | ((data & 1) << 4);

        if complete {
            let value = self.shift;
            self.write_register(addr, value);
            self.shift = SHIFT_RESET;
        }
    }
}

impl Mapper for Mmc1 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if self.prg_bank & 0x10 == 0 => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF if self.prg_bank & 0x10 == 0 => self.prg_ram.write(addr as usize - 0x6000, data),
            0x8000..=0xFFFF => {
                // read-modify-write instructions write twice in consecutive cycles; the chip only sees the first
                let consecutive = self.last_write_cycle.is_some_and(|last| self.cycles.wrapping_sub(last) == 1);
                self.last_write_cycle = Some(self.cycles);

                if !consecutive {
                    self.write_serial(addr, data);
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr.read(self.chr_offset(addr))
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.chr.write(self.chr_offset(addr), data);
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            _ => Mirroring::Horizontal,
        }
    }

    fn cpu_cycle(&mut self) {
        self.cycles += 1;
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

    /// Loads a 5-bit value into the register selected by `addr`, one bit per write (LSB first).
    fn load(mapper: &mut Mmc1, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_cycle();
            mapper.cpu_cycle();
            mapper.cpu_cycle();
            mapper.cpu_write(addr, (value >> bit) & 1);
        }
    }

    fn mapper(prg_banks: usize, chr_banks: usize) -> Mmc1 {
        Mmc1::new(file(1, prg_banks, chr_banks))
    }

    #[test]
    fn power_on_fixes_the_last_prg_bank() {
        let mut m = mapper(16, 8);

        assert_eq!(m.cpu_read(0x8000), Some(0));
        assert_eq!(m.cpu_read(0xC000), Some(30)); // bank 15 = 8KB units 30,31
        assert_eq!(m.cpu_read(0xFFFF), Some(31));
    }

    #[test]
    fn prg_mode_3_switches_the_low_bank() {
        let mut m = mapper(16, 8);
        load(&mut m, 0xE000, 3);

        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.cpu_read(0xC000), Some(30));
    }

    #[test]
    fn prg_mode_2_fixes_the_first_bank() {
        let mut m = mapper(16, 8);
        load(&mut m, 0x8000, 0b01000); // PRG mode 2
        load(&mut m, 0xE000, 5);

        assert_eq!(m.cpu_read(0x8000), Some(0));
        assert_eq!(m.cpu_read(0xC000), Some(10));
    }

    #[test]
    fn prg_mode_0_switches_32kb_ignoring_the_low_bit() {
        let mut m = mapper(16, 8);
        load(&mut m, 0x8000, 0b00000);
        load(&mut m, 0xE000, 5); // banks 4,5

        assert_eq!(m.cpu_read(0x8000), Some(8));
        assert_eq!(m.cpu_read(0xC000), Some(10));
    }

    #[test]
    fn chr_4kb_mode_has_two_independent_banks() {
        let mut m = mapper(2, 8);
        load(&mut m, 0x8000, 0b10000 | 0b01100 | 0b10);
        load(&mut m, 0xA000, 3);
        load(&mut m, 0xC000, 5);

        assert_eq!(m.ppu_read(0x0000), 12);
        assert_eq!(m.ppu_read(0x0FFF), 15);
        assert_eq!(m.ppu_read(0x1000), 20);
    }

    #[test]
    fn chr_8kb_mode_uses_bank_0_and_ignores_its_low_bit() {
        let mut m = mapper(2, 8);
        load(&mut m, 0xA000, 5); // 4KB banks 4,5 as one 8KB bank

        assert_eq!(m.ppu_read(0x0000), 16);
        assert_eq!(m.ppu_read(0x1000), 20);
    }

    #[test]
    fn control_register_selects_mirroring() {
        let mut m = mapper(2, 2);
        for (value, expected) in [
            (0b01100, Mirroring::SingleScreenLower),
            (0b01101, Mirroring::SingleScreenUpper),
            (0b01110, Mirroring::Vertical),
            (0b01111, Mirroring::Horizontal),
        ] {
            load(&mut m, 0x8000, value);
            assert_eq!(m.mirroring(), expected);
        }
    }

    #[test]
    fn reset_bit_clears_the_shift_register_and_fixes_the_last_bank() {
        let mut m = mapper(16, 8);
        load(&mut m, 0x8000, 0b01000); // mode 2
        m.cpu_cycle();
        m.cpu_cycle();
        m.cpu_cycle();
        m.cpu_write(0xE000, 1); // two stray bits in the shift register
        m.cpu_cycle();
        m.cpu_cycle();
        m.cpu_cycle();
        m.cpu_write(0xE000, 1);
        m.cpu_cycle();
        m.cpu_cycle();
        m.cpu_cycle();
        m.cpu_write(0x8000, 0x80); // reset

        assert_eq!(m.cpu_read(0xC000), Some(30), "reset forces PRG mode 3");

        load(&mut m, 0xE000, 2); // a clean five-write sequence still works
        assert_eq!(m.cpu_read(0x8000), Some(4));
    }

    #[test]
    fn writes_on_consecutive_cycles_are_ignored() {
        let mut m = mapper(16, 8);

        // like an INC on a register: the second write comes one cycle after the first
        for _ in 0..5 {
            m.cpu_cycle();
            m.cpu_write(0xE000, 1);
            m.cpu_cycle();
            m.cpu_write(0xE000, 1);
            m.cpu_cycle();
            m.cpu_cycle();
            m.cpu_cycle();
        }

        // each pair counted once: five writes of 1 -> 0b11111 = bank 15
        assert_eq!(m.cpu_read(0x8000), Some(30));
    }

    #[test]
    fn surom_chr_bit_selects_the_256kb_half() {
        let mut m = mapper(32, 0); // 512KB PRG, CHR-RAM
        assert_eq!(m.cpu_read(0x8000), Some(0));
        assert_eq!(m.cpu_read(0xC000), Some(30));

        load(&mut m, 0xA000, 0x10);

        assert_eq!(m.cpu_read(0x8000), Some(32), "second half starts at 8KB unit 32");
        assert_eq!(m.cpu_read(0xC000), Some(62));
    }

    #[test]
    fn prg_ram_can_be_disabled() {
        let mut m = mapper(2, 2);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), Some(0x42));

        load(&mut m, 0xE000, 0x10);
        assert_eq!(m.cpu_read(0x6000), None);
    }

    #[test]
    fn small_roms_wrap_bank_numbers() {
        let mut m = mapper(2, 2); // 32KB
        load(&mut m, 0xE000, 7); // 7 % 2 = 1

        assert_eq!(m.cpu_read(0x8000), Some(2));
    }
}
