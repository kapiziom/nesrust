//! Irem's G-101 (mapper 32) and H3001 (mapper 65):
//! 8KB PRG banks, 1KB CHR banks, and for the H3001 a CPU-cycle IRQ countdown.
//! https://www.nesdev.org/wiki/INES_Mapper_032, https://www.nesdev.org/wiki/INES_Mapper_065

use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Chip {
    G101,
    H3001,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Irem {
    chip: Chip,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,

    prg_banks: [usize; 3],
    chr_banks: [usize; 8],
    /// G-101: the swappable bank is at $C000 and $8000 is fixed to the second-to-last bank.
    swap_prg: bool,

    // H3001 IRQ
    irq_enabled: bool,
    irq_counter: u16,
    irq_latch: u16,
    irq_pending: bool,
}

impl Irem {
    pub fn new(file: INesFile) -> Self {
        Irem {
            chip: if file.mapper == 65 { Chip::H3001 } else { Chip::G101 },
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            swap_prg: false,
            irq_enabled: false,
            irq_counter: 0,
            irq_latch: 0,
            irq_pending: false,
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let banks = (len / PRG_8K).max(1);
        let second_last = banks.saturating_sub(2);

        let bank = match (addr, self.chip, self.swap_prg) {
            (0x8000..=0x9FFF, Chip::G101, true) => second_last,
            (0xC000..=0xDFFF, Chip::G101, true) => self.prg_banks[0],
            (0x8000..=0x9FFF, Chip::G101, false) => self.prg_banks[0],
            (0xC000..=0xDFFF, Chip::G101, false) => second_last,
            (0x8000..=0x9FFF, Chip::H3001, _) => self.prg_banks[0],
            (0xC000..=0xDFFF, Chip::H3001, _) => self.prg_banks[2],
            (0xA000..=0xBFFF, _, _) => self.prg_banks[1],
            _ => banks - 1,
        };
        bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)
    }

    fn write_g101(&mut self, addr: u16, data: u8) {
        match addr & 0xF000 {
            0x8000 => self.prg_banks[0] = (data & 0x1F) as usize,
            0x9000 => {
                self.mirroring = if data & 1 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
                self.swap_prg = data & 2 != 0;
            }
            0xA000 => self.prg_banks[1] = (data & 0x1F) as usize,
            0xB000 => self.chr_banks[(addr & 7) as usize] = data as usize,
            _ => {}
        }
    }

    fn write_h3001(&mut self, addr: u16, data: u8) {
        match addr {
            0x8000 => self.prg_banks[0] = data as usize,
            0xA000 => self.prg_banks[1] = data as usize,
            0xC000 => self.prg_banks[2] = data as usize,
            0x9001 => self.mirroring = if data & 0x80 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical },
            0x9003 => {
                self.irq_enabled = data & 0x80 != 0;
                self.irq_pending = false;
            }
            0x9004 => {
                self.irq_counter = self.irq_latch;
                self.irq_pending = false;
            }
            0x9005 => self.irq_latch = (self.irq_latch & 0x00FF) | ((data as u16) << 8),
            0x9006 => self.irq_latch = (self.irq_latch & 0xFF00) | data as u16,
            0xB000..=0xB007 => self.chr_banks[(addr & 7) as usize] = data as usize,
            _ => {}
        }
    }
}

impl Mapper for Irem {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if (0x6000..0x8000).contains(&addr) {
            self.prg_ram.write(addr as usize - 0x6000, data);
        } else if self.chip == Chip::G101 {
            self.write_g101(addr, data);
        } else {
            self.write_h3001(addr, data);
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let bank = self.chr_banks[(addr as usize >> 10) & 7];
        self.chr.read_banked(CHR_1K, bank, addr as usize & 0x3FF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let bank = self.chr_banks[(addr as usize >> 10) & 7];
        self.chr.write_banked(CHR_1K, bank, addr as usize & 0x3FF, data);
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if self.irq_enabled && self.irq_counter != 0 {
            self.irq_counter -= 1;
            if self.irq_counter == 0 {
                self.irq_pending = true;
                self.irq_enabled = false;
            }
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

    #[test]
    fn g101_has_two_switchable_prg_banks_and_a_mode_that_swaps_the_fixed_one() {
        let mut m = Irem::new(file(32, 8, 8)); // 16 PRG banks of 8KB
        m.cpu_write(0x8000, 3);
        m.cpu_write(0xA000, 4);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(14));
        assert_eq!(m.cpu_read(0xE000), Some(15));

        m.cpu_write(0x9000, 0b10);
        assert_eq!(m.cpu_read(0x8000), Some(14));
        assert_eq!(m.cpu_read(0xC000), Some(3));
    }

    #[test]
    fn g101_chr_banks_and_mirroring() {
        let mut m = Irem::new(file(32, 8, 8));
        for slot in 0..8u16 {
            m.cpu_write(0xB000 + slot, 20 + slot as u8);
        }
        for slot in 0..8u16 {
            assert_eq!(m.ppu_read(slot * 0x400), 20 + slot as u8);
        }

        m.cpu_write(0x9000, 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
        m.cpu_write(0x9000, 0);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn h3001_has_three_switchable_prg_banks() {
        let mut m = Irem::new(file(65, 8, 8));
        m.cpu_write(0x8000, 3);
        m.cpu_write(0xA000, 4);
        m.cpu_write(0xC000, 5);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(15));
    }

    #[test]
    fn h3001_irq_counts_down_cpu_cycles_once() {
        let mut m = Irem::new(file(65, 8, 8));
        m.cpu_write(0x9005, 0x00);
        m.cpu_write(0x9006, 0x03); // latch = 3
        m.cpu_write(0x9004, 0); // reload the counter
        m.cpu_write(0x9003, 0x80); // enable

        m.cpu_cycle();
        m.cpu_cycle();
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());

        m.cpu_write(0x9003, 0x80);
        assert!(!m.irq_pending(), "writing the enable register acknowledges");
        for _ in 0..10 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending(), "the counter stopped at zero");
    }
}
