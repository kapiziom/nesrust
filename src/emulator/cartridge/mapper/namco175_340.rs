//! Namco 175 and 340 (mapper 210): the Namco 163's banking
//! without its sound and IRQ. The 175 has PRG-RAM and fixed mirroring, the 340 controls mirroring.
//! https://www.nesdev.org/wiki/INES_Mapper_210

use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Namco175340 {
    /// Submapper 1 = Namco 175, 2 = Namco 340; without a submapper the mirroring register decides (the 175 never
    /// writes bits 6-7 of $E000).
    namco175: bool,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
    ram_enabled: bool,
    prg_banks: [usize; 3],
    chr_banks: [usize; 8],
}

impl Namco175340 {
    pub fn new(file: INesFile) -> Self {
        Namco175340 {
            namco175: file.submapper == 1,
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            ram_enabled: false,
            prg_banks: [0; 3],
            chr_banks: [0; 8],
        }
    }
}

impl Mapper for Namco175340 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        let len = self.prg_rom.len();
        match addr {
            0x6000..=0x7FFF if self.ram_enabled || !self.namco175 => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => {
                let bank = match addr {
                    0x8000..=0x9FFF => self.prg_banks[0],
                    0xA000..=0xBFFF => self.prg_banks[1],
                    0xC000..=0xDFFF => self.prg_banks[2],
                    _ => (len / PRG_8K).max(1) - 1,
                };
                Some(self.prg_rom[bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr & 0xF800 {
            0x6000 | 0x6800 | 0x7000 | 0x7800 if self.ram_enabled || !self.namco175 => {
                self.prg_ram.write(addr as usize - 0x6000, data)
            }
            0x8000 | 0x8800 | 0x9000 | 0x9800 | 0xA000 | 0xA800 | 0xB000 | 0xB800 => {
                self.chr_banks[((addr >> 11) & 7) as usize] = data as usize;
            }
            0xC000 => self.ram_enabled = data & 1 != 0,
            0xE000 => {
                self.prg_banks[0] = (data & 0x3F) as usize;
                if !self.namco175 {
                    self.mirroring = match data >> 6 {
                        0 => Mirroring::SingleScreenLower,
                        1 => Mirroring::Vertical,
                        2 => Mirroring::SingleScreenUpper,
                        _ => Mirroring::Horizontal,
                    };
                }
            }
            0xE800 => self.prg_banks[1] = (data & 0x3F) as usize,
            0xF000 => self.prg_banks[2] = (data & 0x3F) as usize,
            _ => {}
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

    fn namco(submapper: u8) -> Namco175340 {
        Namco175340::new(INesFile { submapper, ..file(210, 8, 8) })
    }

    #[test]
    fn prg_and_chr_banking() {
        let mut m = namco(2);
        m.cpu_write(0xE000, 3);
        m.cpu_write(0xE800, 4);
        m.cpu_write(0xF000, 5);
        for slot in 0..8u16 {
            m.cpu_write(0x8000 + slot * 0x800, 30 + slot as u8);
        }

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(15));
        for slot in 0..8u16 {
            assert_eq!(m.ppu_read(slot * 0x400), 30 + slot as u8);
        }
    }

    #[test]
    fn the_340_controls_mirroring_but_the_175_does_not() {
        let mut namco340 = namco(2);
        for (bits, expected) in [
            (0u8, Mirroring::SingleScreenLower),
            (1, Mirroring::Vertical),
            (2, Mirroring::SingleScreenUpper),
            (3, Mirroring::Horizontal),
        ] {
            namco340.cpu_write(0xE000, bits << 6);
            assert_eq!(namco340.mirroring(), expected);
        }

        let mut namco175 = namco(1);
        namco175.cpu_write(0xE000, 3 << 6);
        assert_eq!(namco175.mirroring(), Mirroring::Vertical, "header value");
    }

    #[test]
    fn the_175_ram_has_to_be_enabled() {
        let mut m = namco(1);
        m.cpu_write(0x6000, 0x11);
        assert_eq!(m.cpu_read(0x6000), None);

        m.cpu_write(0xC000, 1);
        m.cpu_write(0x6000, 0x22);
        assert_eq!(m.cpu_read(0x6000), Some(0x22));
    }
}
