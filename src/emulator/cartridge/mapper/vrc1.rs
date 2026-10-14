//! Konami VRC1 (mapper 75): three switchable 8KB PRG banks and two 4KB CHR banks whose
//! upper bit lives in the mirroring register. https://www.nesdev.org/wiki/VRC1

use super::{bank_index, mapper_state, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_4K: usize = 4 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vrc1 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    board_mirroring: Mirroring,
    horizontal: bool,
    prg_banks: [usize; 3],
    chr_low: [usize; 2],
    chr_high: [usize; 2],
}

impl Vrc1 {
    pub fn new(file: INesFile) -> Self {
        Vrc1 {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            board_mirroring: file.mirroring,
            horizontal: file.mirroring == Mirroring::Horizontal,
            prg_banks: [0; 3],
            chr_low: [0; 2],
            chr_high: [0; 2],
        }
    }

    fn chr_bank(&self, window: usize) -> usize {
        self.chr_low[window] | (self.chr_high[window] << 4)
    }
}

impl Mapper for Vrc1 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        if addr < 0x8000 {
            return None;
        }

        let len = self.prg_rom.len();
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_banks[0],
            0xA000..=0xBFFF => self.prg_banks[1],
            0xC000..=0xDFFF => self.prg_banks[2],
            _ => (len / PRG_8K).max(1) - 1,
        };
        Some(self.prg_rom[bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)])
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr & 0xF000 {
            0x8000 => self.prg_banks[0] = (data & 0x0F) as usize,
            0x9000 => {
                self.horizontal = data & 1 != 0;
                self.chr_high = [((data >> 1) & 1) as usize, ((data >> 2) & 1) as usize];
            }
            0xA000 => self.prg_banks[1] = (data & 0x0F) as usize,
            0xC000 => self.prg_banks[2] = (data & 0x0F) as usize,
            0xE000 => self.chr_low[0] = (data & 0x0F) as usize,
            0xF000 => self.chr_low[1] = (data & 0x0F) as usize,
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let window = (addr as usize >> 12) & 1;
        self.chr.read_banked(CHR_4K, self.chr_bank(window), addr as usize & 0x0FFF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let window = (addr as usize >> 12) & 1;
        self.chr.write_banked(CHR_4K, self.chr_bank(window), addr as usize & 0x0FFF, data);
    }

    fn mirroring(&self) -> Mirroring {
        if self.board_mirroring == Mirroring::FourScreen {
            Mirroring::FourScreen
        } else if self.horizontal {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    #[test]
    fn three_8kb_prg_banks_and_a_fixed_last_one() {
        let mut m = Vrc1::new(file(75, 8, 8)); // 16 PRG banks of 8KB
        m.cpu_write(0x8000, 3);
        m.cpu_write(0xA000, 4);
        m.cpu_write(0xC000, 5);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(15));
    }

    #[test]
    fn the_chr_banks_get_their_fifth_bit_from_the_mirroring_register() {
        let mut m = Vrc1::new(file(75, 8, 32)); // 64 CHR units of 4KB... 256 of 1KB
        m.cpu_write(0xE000, 0x03);
        m.cpu_write(0xF000, 0x05);
        assert_eq!(m.ppu_read(0x0000), 12);
        assert_eq!(m.ppu_read(0x1000), 20);

        m.cpu_write(0x9000, 0b110); // both high bits
        assert_eq!(m.ppu_read(0x0000), 19 * 4);
        assert_eq!(m.ppu_read(0x1000), 21 * 4);
    }

    #[test]
    fn bit_0_of_9000_selects_horizontal_mirroring() {
        let mut m = Vrc1::new(file(75, 8, 8));
        m.cpu_write(0x9000, 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
        m.cpu_write(0x9000, 0);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }
}
