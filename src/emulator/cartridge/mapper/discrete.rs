//! Simple discrete-logic boards: a latch or two that selects PRG and/or CHR banks.
//! Color Dreams (11), GxROM (66), NINA-003/006 (79), Mapper 87, Jaleco JF-11/14 (140), Bandai 74161 (152) and
//! reversed UxROM (180). Bus conflicts are not emulated.

use super::{mapper_state, bank_index, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_32K: usize = 32 * 1024;
const PRG_16K: usize = 16 * 1024;
const CHR_8K: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Board {
    /// $8000-$FFFF: bits 0-1 select the 32KB PRG bank, bits 4-7 the 8KB CHR bank.
    ColorDreams,
    /// $8000-$FFFF: bits 4-5 select the 32KB PRG bank, bits 0-1 the 8KB CHR bank.
    GxRom,
    /// $4100-$5FFF (address & $E100 = $4100): bit 3 selects the 32KB PRG bank, bits 0-2 the 8KB CHR bank.
    Nina003,
    /// $6000-$7FFF: the CHR bank is the data with bits 0 and 1 swapped.
    Mapper87,
    /// $6000-$7FFF: bits 4-5 select the 32KB PRG bank, bits 0-3 the 8KB CHR bank.
    Jaleco140,
    /// $8000-$FFFF: bits 4-6 select the 16KB PRG bank at $8000, bits 0-3 the CHR bank, bit 7 the single screen.
    Mapper152,
    /// UxROM with the fixed bank at $8000 and the switchable one at $C000.
    Mapper180,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Discrete {
    board: Board,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    prg_bank: usize,
    chr_bank: usize,
}

impl Discrete {
    pub fn new(file: INesFile) -> Self {
        let board = match file.mapper {
            11 => Board::ColorDreams,
            66 => Board::GxRom,
            79 => Board::Nina003,
            87 => Board::Mapper87,
            140 => Board::Jaleco140,
            152 => Board::Mapper152,
            180 => Board::Mapper180,
            other => unreachable!("mapper {other} is not a discrete board"),
        };

        Discrete {
            board,
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_bank: 0,
            chr_bank: 0,
        }
    }

    fn write_register(&mut self, addr: u16, data: u8) {
        match (self.board, addr) {
            (Board::ColorDreams, 0x8000..=0xFFFF) => {
                self.prg_bank = (data & 0x03) as usize;
                self.chr_bank = (data >> 4) as usize;
            }
            (Board::GxRom, 0x8000..=0xFFFF) => {
                self.prg_bank = ((data >> 4) & 0x03) as usize;
                self.chr_bank = (data & 0x03) as usize;
            }
            (Board::Nina003, 0x4100..=0x5FFF) if addr & 0xE100 == 0x4100 => {
                self.prg_bank = ((data >> 3) & 0x01) as usize;
                self.chr_bank = (data & 0x07) as usize;
            }
            (Board::Mapper87, 0x6000..=0x7FFF) => {
                self.chr_bank = (((data & 1) << 1) | ((data >> 1) & 1)) as usize;
            }
            (Board::Jaleco140, 0x6000..=0x7FFF) => {
                self.prg_bank = ((data >> 4) & 0x03) as usize;
                self.chr_bank = (data & 0x0F) as usize;
            }
            (Board::Mapper152, 0x8000..=0xFFFF) => {
                self.prg_bank = ((data >> 4) & 0x07) as usize;
                self.chr_bank = (data & 0x0F) as usize;
                self.mirroring = if data & 0x80 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
            }
            (Board::Mapper180, 0x8000..=0xFFFF) => self.prg_bank = data as usize,
            _ => {}
        }
    }
}

impl Mapper for Discrete {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        if addr < 0x8000 {
            return None;
        }

        let len = self.prg_rom.len();
        let offset = addr as usize & 0x7FFF;
        let index = match self.board {
            Board::Mapper152 => {
                if addr < 0xC000 {
                    bank_index(len, PRG_16K, self.prg_bank, offset)
                } else {
                    let last = (len / PRG_16K).max(1) - 1;
                    bank_index(len, PRG_16K, last, offset & 0x3FFF)
                }
            }
            Board::Mapper180 => {
                if addr < 0xC000 {
                    bank_index(len, PRG_16K, 0, offset)
                } else {
                    bank_index(len, PRG_16K, self.prg_bank, offset & 0x3FFF)
                }
            }
            Board::Mapper87 => offset % len,
            _ => bank_index(len, PRG_32K, self.prg_bank, offset),
        };

        Some(self.prg_rom[index])
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        self.write_register(addr, data);
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr.read_banked(CHR_8K, self.chr_bank, addr as usize & 0x1FFF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.chr.write_banked(CHR_8K, self.chr_bank, addr as usize & 0x1FFF, data);
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    fn board(mapper: u16, prg_16k: usize, chr_8k: usize) -> Discrete {
        Discrete::new(file(mapper, prg_16k, chr_8k))
    }

    #[test]
    fn color_dreams_selects_32kb_prg_and_8kb_chr_from_one_register() {
        let mut m = board(11, 8, 16); // 128KB PRG = 4 banks of 32KB, 128KB CHR = 16 banks of 8KB

        m.cpu_write(0x8000, 0x52); // PRG 2, CHR 5

        assert_eq!(m.cpu_read(0x8000), Some(8), "bank 2 of 32KB starts at 8KB unit 8");
        assert_eq!(m.cpu_read(0xFFFF), Some(11));
        assert_eq!(m.ppu_read(0x0000), 40, "CHR bank 5 starts at 1KB unit 40");
    }

    #[test]
    fn gxrom_uses_the_opposite_bit_layout() {
        let mut m = board(66, 8, 4);

        m.cpu_write(0xFFFF, 0x31); // PRG 3, CHR 1

        assert_eq!(m.cpu_read(0x8000), Some(12));
        assert_eq!(m.ppu_read(0x0000), 8);
    }

    #[test]
    fn nina_003_is_written_through_the_4100_area_only() {
        let mut m = board(79, 4, 8);

        m.cpu_write(0x4100, 0b0000_1011); // PRG 1, CHR 3
        assert_eq!(m.cpu_read(0x8000), Some(4));
        assert_eq!(m.ppu_read(0x0000), 24);

        m.cpu_write(0x4000, 0); // not a register address
        m.cpu_write(0x8000, 0);
        assert_eq!(m.cpu_read(0x8000), Some(4));
    }

    #[test]
    fn mapper_87_swaps_the_two_chr_bits_and_keeps_prg_fixed() {
        let mut m = board(87, 2, 4);

        m.cpu_write(0x6000, 0b01); // -> CHR bank 2

        assert_eq!(m.ppu_read(0x0000), 16);
        assert_eq!(m.cpu_read(0x8000), Some(0));
        m.cpu_write(0x8000, 0xFF);
        assert_eq!(m.ppu_read(0x0000), 16, "writes to the ROM area do nothing");
    }

    #[test]
    fn jaleco_140_registers_live_in_6000() {
        let mut m = board(140, 8, 16);

        m.cpu_write(0x7FFF, 0x25); // PRG 2, CHR 5

        assert_eq!(m.cpu_read(0x8000), Some(8));
        assert_eq!(m.ppu_read(0x0000), 40);
    }

    #[test]
    fn mapper_152_has_16kb_prg_banking_and_single_screen_control() {
        let mut m = board(152, 8, 16);
        assert_eq!(m.cpu_read(0xC000), Some(14), "last 16KB bank fixed at $C000");

        m.cpu_write(0x8000, 0b1011_0100); // single screen upper, PRG 3, CHR 4

        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.cpu_read(0xC000), Some(14));
        assert_eq!(m.ppu_read(0x0000), 32);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
    }

    #[test]
    fn mapper_180_fixes_the_first_bank_and_switches_the_upper_one() {
        let mut m = board(180, 8, 0);

        m.cpu_write(0x8000, 3);

        assert_eq!(m.cpu_read(0x8000), Some(0));
        assert_eq!(m.cpu_read(0xC000), Some(6));
    }

    #[test]
    fn bank_numbers_wrap_around_small_roms() {
        let mut m = board(66, 2, 1); // one 32KB PRG bank, one 8KB CHR bank

        m.cpu_write(0x8000, 0x33);

        assert_eq!(m.cpu_read(0x8000), Some(0));
        assert_eq!(m.ppu_read(0x0400), 1);
    }

    #[test]
    fn rom_boards_without_prg_ram_report_open_bus_below_8000() {
        let mut m = board(11, 2, 1);

        assert_eq!(m.cpu_read(0x6000), None);
        assert_eq!(m.cpu_read(0x4020), None);
    }
}
