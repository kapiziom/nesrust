//! Taito's boards: TC0190 (mapper 33), TC0190 + TC0690 with an A12 IRQ (mapper 48) and X1-005
//! (mappers 80 and 207).
//! https://www.nesdev.org/wiki/INES_Mapper_033, https://www.nesdev.org/wiki/INES_Mapper_048,
//! https://www.nesdev.org/wiki/INES_Mapper_080, https://www.nesdev.org/wiki/INES_Mapper_207

use super::a12_counter::A12Counter;
use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Chip {
    /// Mapper 33.
    Tc0190,
    /// Mapper 48: the TC0190 registers, a mirroring register in $E000 and an IRQ.
    Tc0690,
    /// Mapper 80.
    X1005,
    /// Mapper 207: the X1-005 with the nametable pages chosen by bit 7 of the two 2KB CHR registers.
    X1005Pages,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Taito {
    chip: Chip,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,

    prg_banks: [usize; 3],
    /// Two 2KB banks followed by four 1KB banks (as 1KB numbers for the 2KB ones: the stored value is doubled).
    chr_banks: [usize; 6],
    /// X1-005: $7EF8/$7EF9 must hold $A3 to enable the 128 bytes of RAM.
    ram_enable: [u8; 2],
    /// X1-005 207: bit 7 of the two 2KB CHR registers.
    pages: [u8; 2],
    irq: A12Counter,
}

impl Taito {
    pub fn new(file: INesFile) -> Self {
        let chip = match file.mapper {
            33 => Chip::Tc0190,
            48 => Chip::Tc0690,
            207 => Chip::X1005Pages,
            _ => Chip::X1005,
        };
        Taito {
            chip,
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(if matches!(chip, Chip::X1005 | Chip::X1005Pages) { 128 } else { 0 }, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_banks: [0; 3],
            chr_banks: [0; 6],
            ram_enable: [0; 2],
            pages: [0; 2],
            irq: A12Counter::default(),
        }
    }

    fn is_x1005(&self) -> bool {
        matches!(self.chip, Chip::X1005 | Chip::X1005Pages)
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let banks = (len / PRG_8K).max(1);
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_banks[0],
            0xA000..=0xBFFF => self.prg_banks[1],
            0xC000..=0xDFFF if self.is_x1005() => self.prg_banks[2],
            0xC000..=0xDFFF => banks.saturating_sub(2),
            _ => banks - 1,
        };
        bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)
    }

    fn write_tc0190(&mut self, addr: u16, data: u8) {
        match addr & 0xE003 {
            0x8000 => {
                self.prg_banks[0] = (data & 0x3F) as usize;
                if self.chip == Chip::Tc0190 {
                    self.mirroring = if data & 0x40 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
                }
            }
            0x8001 => self.prg_banks[1] = (data & 0x3F) as usize,
            0x8002 => self.chr_banks[0] = data as usize * 2,
            0x8003 => self.chr_banks[1] = data as usize * 2,
            0xA000..=0xA003 => self.chr_banks[2 + (addr & 3) as usize] = data as usize,
            _ if self.chip != Chip::Tc0690 => {}
            // the IRQ of the TC0690 is an MMC3 counter with an inverted latch
            0xC000 => self.irq.set_latch(data ^ 0xFF),
            0xC001 => self.irq.request_reload(),
            0xC002 => self.irq.set_enabled(true),
            0xC003 => self.irq.set_enabled(false),
            0xE000 => self.mirroring = if data & 0x40 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical },
            _ => {}
        }
    }

    fn write_x1005(&mut self, addr: u16, data: u8) {
        match addr {
            0x7EF0 => {
                self.chr_banks[0] = (data & 0xFE) as usize;
                self.pages[0] = data >> 7;
            }
            0x7EF1 => {
                self.chr_banks[1] = (data & 0xFE) as usize;
                self.pages[1] = data >> 7;
            }
            0x7EF2..=0x7EF5 => self.chr_banks[2 + (addr - 0x7EF2) as usize] = data as usize,
            0x7EF6 | 0x7EF7 => self.mirroring = if data & 1 != 0 { Mirroring::Vertical } else { Mirroring::Horizontal },
            0x7EF8 | 0x7EF9 => self.ram_enable[(addr - 0x7EF8) as usize] = data,
            0x7EFA | 0x7EFB => self.prg_banks[0] = data as usize,
            0x7EFC | 0x7EFD => self.prg_banks[1] = data as usize,
            0x7EFE | 0x7EFF => self.prg_banks[2] = data as usize,
            0x7F00..=0x7FFF if self.ram_enable == [0xA3, 0xA3] => self.prg_ram.write(addr as usize & 0x7F, data),
            _ => {}
        }
    }

    /// The (1KB bank, offset) behind a PPU address; the chip with the 2KB banks in the lower half.
    fn chr_bank(&self, addr: u16) -> (usize, usize) {
        let addr = addr as usize & 0x1FFF;
        let slot = addr / CHR_1K;
        let bank = if addr < 0x1000 {
            // the 2KB registers address pairs of 1KB banks
            self.chr_banks[slot / 2] + (slot & 1)
        } else {
            self.chr_banks[2 + slot - 4]
        };
        (if self.chip == Chip::X1005Pages { bank & 0x7F } else { bank }, addr % CHR_1K)
    }
}

impl Mapper for Taito {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x7F00..=0x7FFF if self.is_x1005() && self.ram_enable == [0xA3, 0xA3] => {
                self.prg_ram.read(addr as usize & 0x7F)
            }
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if self.is_x1005() {
            self.write_x1005(addr, data);
        } else if addr >= 0x8000 {
            self.write_tc0190(addr, data);
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        if self.chip == Chip::Tc0690 {
            self.irq.watch(addr);
        }
        let (bank, offset) = self.chr_bank(addr);
        self.chr.read_banked(CHR_1K, bank, offset)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        if self.chip == Chip::Tc0690 {
            self.irq.watch(addr);
        }
        let (bank, offset) = self.chr_bank(addr);
        self.chr.write_banked(CHR_1K, bank, offset, data);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.chip == Chip::Tc0690 {
            self.irq.watch(addr);
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.chip {
            Chip::X1005Pages => Mirroring::Custom([self.pages[0], self.pages[0], self.pages[1], self.pages[1]]),
            _ => self.mirroring,
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

    fn taito(mapper: u16) -> Taito {
        Taito::new(file(mapper, 8, 16)) // 16 PRG banks of 8KB, 128 CHR banks of 1KB
    }

    /// One scanline's worth of A12 activity: low for a while, then the sprite fetches at $1000.
    fn scanline(m: &mut Taito) {
        for _ in 0..6 {
            m.cpu_cycle();
        }
        m.ppu_read(0x0000);
        for _ in 0..6 {
            m.cpu_cycle();
        }
        m.ppu_read(0x1000);
    }

    #[test]
    fn tc0190_prg_and_chr_banking() {
        let mut m = taito(33);
        m.cpu_write(0x8000, 3);
        m.cpu_write(0x8001, 4);
        m.cpu_write(0x8002, 5); // 2KB bank 5 = 1KB banks 10, 11
        m.cpu_write(0x8003, 6);
        for (slot, value) in [(0u16, 40u8), (1, 41), (2, 42), (3, 43)] {
            m.cpu_write(0xA000 + slot, value);
        }

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(14));
        assert_eq!(m.cpu_read(0xE000), Some(15));
        assert_eq!(m.ppu_read(0x0000), 10);
        assert_eq!(m.ppu_read(0x0400), 11);
        assert_eq!(m.ppu_read(0x0800), 12);
        assert_eq!(m.ppu_read(0x1000), 40);
        assert_eq!(m.ppu_read(0x1C00), 43);
    }

    #[test]
    fn mapper_33_takes_the_mirroring_from_bit_6_of_the_first_prg_register() {
        let mut m = taito(33);
        m.cpu_write(0x8000, 0x40);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
        m.cpu_write(0x8000, 0x00);
        assert_eq!(m.mirroring(), Mirroring::Vertical);

        let mut m = taito(48);
        m.cpu_write(0x8000, 0x40);
        assert_eq!(m.mirroring(), Mirroring::Vertical, "mapper 48 uses $E000");
        m.cpu_write(0xE000, 0x40);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn mapper_48_has_an_a12_irq_with_an_inverted_latch() {
        let mut m = taito(48);
        m.cpu_write(0xC000, 0xFD); // latch = !$FD = 2
        m.cpu_write(0xC001, 0);
        m.cpu_write(0xC002, 0);

        scanline(&mut m); // reload to 2
        scanline(&mut m); // 1
        assert!(!m.irq_pending());
        scanline(&mut m); // 0
        assert!(m.irq_pending());

        m.cpu_write(0xC003, 0);
        assert!(!m.irq_pending());
    }

    #[test]
    fn x1005_registers_live_in_7ef0() {
        let mut m = taito(80);
        m.cpu_write(0x7EFA, 3);
        m.cpu_write(0x7EFC, 4);
        m.cpu_write(0x7EFE, 5);
        m.cpu_write(0x7EF0, 0x05); // low bit ignored for 2KB banks: 4, 5
        m.cpu_write(0x7EF2, 60);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(15));
        assert_eq!(m.ppu_read(0x0000), 4);
        assert_eq!(m.ppu_read(0x0400), 5);
        assert_eq!(m.ppu_read(0x1000), 60);
    }

    #[test]
    fn the_x1005_ram_needs_both_enable_registers() {
        let mut m = taito(80);
        m.cpu_write(0x7F00, 1);
        assert_eq!(m.cpu_read(0x7F00), None);

        m.cpu_write(0x7EF8, 0xA3);
        m.cpu_write(0x7EF9, 0xA3);
        m.cpu_write(0x7F05, 0x42);
        assert_eq!(m.cpu_read(0x7F05), Some(0x42));
        assert_eq!(m.cpu_read(0x7F85), Some(0x42), "128 bytes, mirrored");
    }

    #[test]
    fn mapper_207_chooses_the_nametable_pages_with_bit_7_of_the_2kb_chr_registers() {
        let mut m = taito(207);
        m.cpu_write(0x7EF0, 0x84);
        m.cpu_write(0x7EF1, 0x02);

        assert_eq!(m.mirroring(), Mirroring::Custom([1, 1, 0, 0]));
        assert_eq!(m.ppu_read(0x0000), 4, "the page bit is not part of the bank number");
    }

    #[test]
    fn mapper_80_mirroring_register() {
        let mut m = taito(80);
        m.cpu_write(0x7EF6, 1);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
        m.cpu_write(0x7EF6, 0);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
    }
}
