//! Mapper 3 (CNROM): fixed PRG (16KB mirrored or 32KB), switchable 8KB CHR bank.
//! https://www.nesdev.org/wiki/CNROM

use super::{Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const CHR_BANK_SIZE: usize = 8 * 1024;

pub struct Cnrom {
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    chr_bank: usize,
}

impl Cnrom {
    pub fn new(file: INesFile) -> Self {
        Cnrom {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            chr_bank: 0,
        }
    }
}

impl Mapper for Cnrom {
    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x8000..=0xFFFF => Some(self.prg_rom[(addr as usize - 0x8000) % self.prg_rom.len()]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if addr >= 0x8000 {
            self.chr_bank = data as usize;
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr.read_banked(CHR_BANK_SIZE, self.chr_bank, addr as usize & 0x1FFF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.chr.write_banked(CHR_BANK_SIZE, self.chr_bank, addr as usize & 0x1FFF, data);
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
    fn switches_the_whole_8kb_chr_bank() {
        let mut mapper = Cnrom::new(file(3, 2, 4));

        assert_eq!(mapper.ppu_read(0x0000), 0);
        assert_eq!(mapper.ppu_read(0x1FFF), 7);

        mapper.cpu_write(0x8000, 2);

        assert_eq!(mapper.ppu_read(0x0000), 16);
        assert_eq!(mapper.ppu_read(0x1FFF), 23);
    }

    #[test]
    fn chr_bank_wraps_and_prg_stays_fixed() {
        let mut mapper = Cnrom::new(file(3, 1, 2));
        mapper.cpu_write(0xC000, 5); // 5 % 2 = 1

        assert_eq!(mapper.ppu_read(0x0000), 8);
        assert_eq!(mapper.cpu_read(0x8000), Some(0));
        assert_eq!(mapper.cpu_read(0xFFFF), Some(1));
    }
}
