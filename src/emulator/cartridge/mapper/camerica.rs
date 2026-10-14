//! Mapper 71 (Camerica/Codemasters BF9093 and BF9097): a switchable 16KB bank at $8000, the last bank fixed at $C000.
//! The BF9097 variant also has a single-screen select in $8000-$9FFF.
//! https://www.nesdev.org/wiki/INES_Mapper_071

use super::{mapper_state, bank_index, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_16K: usize = 16 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Camerica {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    bank: usize,
}

impl Camerica {
    pub fn new(file: INesFile) -> Self {
        Camerica {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            bank: 0,
        }
    }
}

impl Mapper for Camerica {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        let len = self.prg_rom.len();
        match addr {
            0x8000..=0xBFFF => Some(self.prg_rom[bank_index(len, PRG_16K, self.bank, addr as usize - 0x8000)]),
            0xC000..=0xFFFF => {
                let last = (len / PRG_16K).max(1) - 1;
                Some(self.prg_rom[bank_index(len, PRG_16K, last, addr as usize - 0xC000)])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            // BF9097: bit 4 chooses which nametable all four screens show
            0x9000..=0x9FFF => {
                self.mirroring = if data & 0x10 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
            }
            0xC000..=0xFFFF => self.bank = data as usize,
            _ => {}
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
    fn switches_the_low_bank_with_writes_to_c000_and_up() {
        let mut m = Camerica::new(file(71, 8, 0));
        assert_eq!(m.cpu_read(0xC000), Some(14));

        m.cpu_write(0xC000, 3);

        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.cpu_read(0xC000), Some(14), "fixed bank stays");

        m.cpu_write(0x8000, 5); // not the bank register on this board
        assert_eq!(m.cpu_read(0x8000), Some(6));
    }

    #[test]
    fn fire_hawk_selects_a_single_screen_through_9000() {
        let mut m = Camerica::new(file(71, 4, 0));
        assert_eq!(m.mirroring(), Mirroring::Vertical, "header value until the game writes");

        m.cpu_write(0x9000, 0x10);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);

        m.cpu_write(0x9000, 0x00);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn chr_is_ram() {
        let mut data = file(71, 4, 0);
        data.chr_rom.clear();
        let mut m = Camerica::new(data);
        m.ppu_write(0x0040, 0x5A);

        assert_eq!(m.ppu_read(0x0040), 0x5A);
    }
}
