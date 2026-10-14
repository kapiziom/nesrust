//! Mapper 2 (UxROM): switchable 16KB bank at $8000, last bank fixed at $C000, usually CHR-RAM.
//! https://www.nesdev.org/wiki/UxROM

use super::{bank_index, mapper_state, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const BANK_SIZE: usize = 16 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Uxrom {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    bank: usize,
}

impl Uxrom {
    pub fn new(file: INesFile) -> Self {
        Uxrom {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            bank: 0,
        }
    }
}

impl Mapper for Uxrom {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        let len = self.prg_rom.len();
        match addr {
            0x8000..=0xBFFF => Some(self.prg_rom[bank_index(len, BANK_SIZE, self.bank, addr as usize - 0x8000)]),
            0xC000..=0xFFFF => {
                let last_bank = (len / BANK_SIZE).max(1) - 1;
                Some(self.prg_rom[bank_index(len, BANK_SIZE, last_bank, addr as usize - 0xC000)])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if addr >= 0x8000 {
            self.bank = data as usize;
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr.read(addr as usize)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.chr.write(addr as usize, data);
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    #[test]
    fn switches_the_low_bank_and_fixes_the_last() {
        let mut mapper = Uxrom::new(file(2, 8, 0));

        assert_eq!(mapper.cpu_read(0x8000), Some(0));
        assert_eq!(mapper.cpu_read(0xC000), Some(14)); // last 16KB bank = 8KB units 14,15

        mapper.cpu_write(0x8000, 3);

        assert_eq!(mapper.cpu_read(0x8000), Some(6));
        assert_eq!(mapper.cpu_read(0xBFFF), Some(7));
        assert_eq!(mapper.cpu_read(0xC000), Some(14), "fixed bank does not move");
    }

    #[test]
    fn bank_number_wraps_around_the_rom_size() {
        let mut mapper = Uxrom::new(file(2, 4, 0));
        mapper.cpu_write(0xFFFF, 0x0F); // 15 % 4 = 3

        assert_eq!(mapper.cpu_read(0x8000), Some(6));
    }

    #[test]
    fn uses_chr_ram() {
        let mut data = file(2, 4, 0);
        data.chr_rom.clear();
        let mut mapper = Uxrom::new(data);
        mapper.ppu_write(0x1234, 0x99);

        assert_eq!(mapper.ppu_read(0x1234), 0x99);
    }
}
