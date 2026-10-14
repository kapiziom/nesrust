//! Mapper 7 (AxROM): 32KB PRG banks, single-screen mirroring selected by a register, CHR-RAM.
//! https://www.nesdev.org/wiki/AxROM

use super::{mapper_state, bank_index, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const BANK_SIZE: usize = 32 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Axrom {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    bank: usize,
    upper_screen: bool,
}

impl Axrom {
    pub fn new(file: INesFile) -> Self {
        Axrom {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            bank: 0,
            upper_screen: false,
        }
    }
}

impl Mapper for Axrom {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x8000..=0xFFFF => Some(self.prg_rom[bank_index(self.prg_rom.len(), BANK_SIZE, self.bank, addr as usize - 0x8000)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if addr >= 0x8000 {
            self.bank = (data & 0x07) as usize;
            self.upper_screen = data & 0x10 != 0;
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr.read(addr as usize)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.chr.write(addr as usize, data);
    }

    fn mirroring(&self) -> Mirroring {
        if self.upper_screen { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    #[test]
    fn switches_32kb_banks() {
        let mut mapper = Axrom::new(file(7, 8, 0)); // 4 banks of 32KB

        assert_eq!(mapper.cpu_read(0x8000), Some(0));

        mapper.cpu_write(0x8000, 2);

        assert_eq!(mapper.cpu_read(0x8000), Some(8)); // 8KB unit 8
        assert_eq!(mapper.cpu_read(0xFFFF), Some(11));
    }

    #[test]
    fn bit_4_selects_the_single_screen() {
        let mut mapper = Axrom::new(file(7, 4, 0));
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);

        mapper.cpu_write(0x8000, 0x10);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenUpper);

        mapper.cpu_write(0x8000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    }
}
