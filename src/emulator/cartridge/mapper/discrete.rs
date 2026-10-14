//! Simple discrete-logic boards: a latch or two that selects PRG and/or CHR banks.
//! Color Dreams (11), GxROM (66), NINA-003 (79), Mapper 87, Jaleco JF-11/14 (140), Bandai 74161 (152), reversed
//! UxROM (180), Irem 74HC161 (78), Sunsoft 2 (89, 93), UN1ROM (94), Irem TAM-S1 (97), Jaleco JF-17/19 (72, 92),
//! Sunsoft 1 (184) and CNROM with copy protection (185). Bus conflicts are not emulated.

use super::{bank_index, mapper_state, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_32K: usize = 32 * 1024;
const PRG_16K: usize = 16 * 1024;
const CHR_8K: usize = 8 * 1024;
const CHR_4K: usize = 4 * 1024;

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
    /// $8000-$FFFF: bits 0-2 select the 16KB PRG bank, bits 4-7 the CHR bank, bit 3 the mirroring.
    Mapper78,
    /// $8000-$FFFF: bits 4-6 select the 16KB PRG bank, bits 0-2 and 7 the CHR bank, bit 3 the single screen.
    Mapper89,
    /// $8000-$FFFF: bits 4-6 select the 16KB PRG bank (CHR is RAM).
    Mapper93,
    /// $8000-$FFFF: bits 2-4 select the 16KB PRG bank (CHR is RAM).
    Mapper94,
    /// $8000-$FFFF: bits 0-3 select the 16KB PRG bank at $C000 ($8000 is the last bank), bits 6-7 the mirroring.
    Mapper97,
    /// $8000-$FFFF: bit 7 / bit 6 latch the PRG (bits 0-2) / CHR (bits 0-3) bank on their rising edge.
    Mapper72,
    /// Like 72 but the first PRG bank is fixed at $8000 and the switchable one is at $C000.
    Mapper92,
    /// $6000-$7FFF: two 4KB CHR banks; PRG is fixed.
    Mapper184,
    /// CNROM whose CHR-ROM only answers after the right value was written to $8000-$FFFF.
    Mapper185,
}

/// Where the switchable PRG bank sits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PrgLayout {
    /// One 32KB bank (or all of it when the ROM has just one).
    Whole32,
    /// The switchable 16KB bank at $8000, the last bank at $C000.
    SwitchLow,
    /// The first bank at $8000, the switchable one at $C000.
    SwitchHigh,
    /// The last bank at $8000, the switchable one at $C000.
    SwitchHighLastLow,
}

impl Board {
    fn prg_layout(self) -> PrgLayout {
        match self {
            Board::ColorDreams | Board::GxRom | Board::Nina003 | Board::Jaleco140 => PrgLayout::Whole32,
            Board::Mapper87 | Board::Mapper184 | Board::Mapper185 => PrgLayout::Whole32,
            Board::Mapper152
            | Board::Mapper78
            | Board::Mapper89
            | Board::Mapper93
            | Board::Mapper94
            | Board::Mapper72 => PrgLayout::SwitchLow,
            Board::Mapper180 | Board::Mapper92 => PrgLayout::SwitchHigh,
            Board::Mapper97 => PrgLayout::SwitchHighLastLow,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Discrete {
    board: Board,
    /// Mapper 78 comes in two wirings of its mirroring bit (NES 2.0 submapper 1 = single-screen select, 3 = horizontal/vertical select).
    single_screen_select: bool,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    prg_bank: usize,
    chr_bank: usize,
    /// The second 4KB CHR bank of mapper 184.
    chr_bank_high: usize,
    /// Mapper 185: whether the CHR-ROM is currently connected.
    chr_enabled: bool,
    /// Mapper 72 / 92: the last value written, to find the rising edges of its two load bits.
    previous_write: u8,
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
            78 => Board::Mapper78,
            89 => Board::Mapper89,
            93 => Board::Mapper93,
            94 => Board::Mapper94,
            97 => Board::Mapper97,
            72 => Board::Mapper72,
            92 => Board::Mapper92,
            184 => Board::Mapper184,
            185 => Board::Mapper185,
            other => unreachable!("mapper {other} is not a discrete board"),
        };

        Discrete {
            board,
            single_screen_select: file.mapper == 78 && file.submapper == 1,
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_bank: 0,
            chr_bank: 0,
            chr_bank_high: if board == Board::Mapper184 { 4 } else { 0 },
            chr_enabled: board != Board::Mapper185,
            previous_write: 0,
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
                self.mirroring =
                    if data & 0x80 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
            }
            (Board::Mapper180, 0x8000..=0xFFFF) => self.prg_bank = data as usize,
            (Board::Mapper78, 0x8000..=0xFFFF) => {
                self.prg_bank = (data & 0x07) as usize;
                self.chr_bank = (data >> 4) as usize;
                self.mirroring = match (self.single_screen_select, data & 0x08 != 0) {
                    (true, false) => Mirroring::SingleScreenLower,
                    (true, true) => Mirroring::SingleScreenUpper,
                    (false, false) => Mirroring::Horizontal,
                    (false, true) => Mirroring::Vertical,
                };
            }
            (Board::Mapper89, 0x8000..=0xFFFF) => {
                self.prg_bank = ((data >> 4) & 0x07) as usize;
                self.chr_bank = ((data & 0x07) | ((data >> 4) & 0x08)) as usize;
                self.mirroring =
                    if data & 0x08 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
            }
            (Board::Mapper93, 0x8000..=0xFFFF) => self.prg_bank = ((data >> 4) & 0x07) as usize,
            (Board::Mapper94, 0x8000..=0xFFFF) => self.prg_bank = ((data >> 2) & 0x07) as usize,
            (Board::Mapper97, 0x8000..=0xFFFF) => {
                self.prg_bank = (data & 0x0F) as usize;
                self.mirroring = match data >> 6 {
                    0 => Mirroring::SingleScreenLower,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::Vertical,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            (Board::Mapper72 | Board::Mapper92, 0x8000..=0xFFFF) => {
                let rising = data & !self.previous_write;
                if rising & 0x80 != 0 {
                    self.prg_bank = (data & 0x0F) as usize;
                }
                if rising & 0x40 != 0 {
                    self.chr_bank = (data & 0x0F) as usize;
                }
                self.previous_write = data;
            }
            (Board::Mapper184, 0x6000..=0x7FFF) => {
                self.chr_bank = (data & 0x07) as usize;
                self.chr_bank_high = 4 | ((data >> 4) & 0x07) as usize;
            }
            (Board::Mapper185, 0x8000..=0xFFFF) => {
                // the usual protection: any value with a non-zero low nibble except $13 connects the CHR-ROM
                self.chr_enabled = data & 0x0F != 0 && data != 0x13;
            }
            _ => {}
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let offset = addr as usize & 0x7FFF;
        let last = (len / PRG_16K).max(1) - 1;

        match self.board.prg_layout() {
            PrgLayout::Whole32 if self.board == Board::Mapper87 => offset % len,
            PrgLayout::Whole32 => bank_index(len, PRG_32K, self.prg_bank, offset),
            PrgLayout::SwitchLow if addr < 0xC000 => bank_index(len, PRG_16K, self.prg_bank, offset),
            PrgLayout::SwitchLow => bank_index(len, PRG_16K, last, offset & 0x3FFF),
            PrgLayout::SwitchHigh if addr < 0xC000 => bank_index(len, PRG_16K, 0, offset),
            PrgLayout::SwitchHigh => bank_index(len, PRG_16K, self.prg_bank, offset & 0x3FFF),
            PrgLayout::SwitchHighLastLow if addr < 0xC000 => bank_index(len, PRG_16K, last, offset),
            PrgLayout::SwitchHighLastLow => bank_index(len, PRG_16K, self.prg_bank, offset & 0x3FFF),
        }
    }

    fn chr_bank_for(&self, addr: u16) -> (usize, usize, usize) {
        if self.board == Board::Mapper184 {
            let bank = if addr < 0x1000 { self.chr_bank } else { self.chr_bank_high };
            (CHR_4K, bank, addr as usize & 0x0FFF)
        } else {
            (CHR_8K, self.chr_bank, addr as usize & 0x1FFF)
        }
    }
}

impl Mapper for Discrete {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        if addr < 0x8000 {
            return None;
        }

        Some(self.prg_rom[self.prg_index(addr)])
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        self.write_register(addr, data);
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        if !self.chr_enabled {
            return 0xFF;
        }
        let (size, bank, offset) = self.chr_bank_for(addr);
        self.chr.read_banked(size, bank, offset)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let (size, bank, offset) = self.chr_bank_for(addr);
        self.chr.write_banked(size, bank, offset, data);
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

    #[test]
    fn mapper_78_picks_the_mirroring_meaning_from_the_submapper() {
        let mut holy_diver = Discrete::new(INesFile { submapper: 3, ..file(78, 8, 16) });
        holy_diver.cpu_write(0x8000, 0x4A); // PRG 2, CHR 4, bit 3 set
        assert_eq!(holy_diver.cpu_read(0x8000), Some(4));
        assert_eq!(holy_diver.cpu_read(0xC000), Some(14));
        assert_eq!(holy_diver.ppu_read(0x0000), 32);
        assert_eq!(holy_diver.mirroring(), Mirroring::Vertical);
        holy_diver.cpu_write(0x8000, 0x02);
        assert_eq!(holy_diver.mirroring(), Mirroring::Horizontal);

        let mut cosmo = Discrete::new(INesFile { submapper: 1, ..file(78, 8, 16) });
        cosmo.cpu_write(0x8000, 0x08);
        assert_eq!(cosmo.mirroring(), Mirroring::SingleScreenUpper);
    }

    #[test]
    fn mapper_89_assembles_the_chr_bank_from_bits_0_2_and_7() {
        let mut m = board(89, 8, 16);

        m.cpu_write(0x8000, 0b1011_1101); // CHR = 5 | 8, PRG 3, single screen upper

        assert_eq!(m.ppu_read(0x0000), 13 * 8);
        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
    }

    #[test]
    fn mapper_93_and_94_take_the_prg_bank_from_different_bits() {
        let mut sunsoft = board(93, 8, 0);
        sunsoft.cpu_write(0x8000, 0x30);
        assert_eq!(sunsoft.cpu_read(0x8000), Some(6));
        assert_eq!(sunsoft.cpu_read(0xC000), Some(14));

        let mut un1rom = board(94, 8, 0);
        un1rom.cpu_write(0x8000, 0b0000_1100);
        assert_eq!(un1rom.cpu_read(0x8000), Some(6));
    }

    #[test]
    fn mapper_97_switches_the_high_bank_and_fixes_the_last_one_low() {
        let mut m = board(97, 8, 0);

        m.cpu_write(0x8000, 0b1000_0010); // PRG 2, vertical

        assert_eq!(m.cpu_read(0x8000), Some(14));
        assert_eq!(m.cpu_read(0xC000), Some(4));
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mappers_72_and_92_load_a_bank_only_on_the_rising_edge_of_their_load_bit() {
        let mut m = board(72, 8, 16);

        m.cpu_write(0x8000, 0x83); // load PRG 3
        assert_eq!(m.cpu_read(0x8000), Some(6));
        m.cpu_write(0x8000, 0x85); // bit 7 still high: no new load
        assert_eq!(m.cpu_read(0x8000), Some(6));
        m.cpu_write(0x8000, 0x05); // bit 7 low
        m.cpu_write(0x8000, 0x85); // rising again
        assert_eq!(m.cpu_read(0x8000), Some(10));
        m.cpu_write(0x8000, 0x00);
        m.cpu_write(0x8000, 0x47); // load CHR 7
        assert_eq!(m.ppu_read(0x0000), 56);

        let mut m = board(92, 8, 16);
        m.cpu_write(0x8000, 0x83);
        assert_eq!(m.cpu_read(0x8000), Some(0), "the first bank is fixed");
        assert_eq!(m.cpu_read(0xC000), Some(6));
    }

    #[test]
    fn mapper_184_has_two_4kb_chr_banks_and_the_high_one_is_in_the_upper_half() {
        let mut m = board(184, 2, 8);

        m.cpu_write(0x6000, 0b0010_0011); // low bank 3, high bank 4 | 2 = 6

        assert_eq!(m.ppu_read(0x0000), 12);
        assert_eq!(m.ppu_read(0x1000), 24);
    }

    #[test]
    fn mapper_185_hides_the_chr_until_the_protection_value_arrives() {
        let mut m = board(185, 2, 1);
        assert_eq!(m.ppu_read(0x0000), 0xFF, "disconnected at power-up");

        m.cpu_write(0x8000, 0x13);
        assert_eq!(m.ppu_read(0x0000), 0xFF);
        m.cpu_write(0x8000, 0x21);
        assert_eq!(m.ppu_read(0x0400), 1);
        m.cpu_write(0x8000, 0x10);
        assert_eq!(m.ppu_read(0x0400), 0xFF);
    }
}
