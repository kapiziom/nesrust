//! Mapper 9 (MMC2) and mapper 10 (MMC4): two 4KB CHR windows, each with two banks that are
//! switched by latches which watch the PPU's pattern fetches of the "$FD" and "$FE" tiles.
//! https://www.nesdev.org/wiki/MMC2, https://www.nesdev.org/wiki/MMC4

use super::{bank_index, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const CHR_4K: usize = 4 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    /// 8KB switchable PRG bank at $8000, the last three 8KB banks fixed.
    Mmc2,
    /// 16KB switchable PRG bank at $8000, the last 16KB bank fixed; PRG-RAM at $6000.
    Mmc4,
}

pub struct Mmc2 {
    variant: Variant,
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
    prg_bank: usize,
    /// CHR banks selected by [window][latch]: latch 0 = the $FD tile was fetched last, 1 = the $FE tile.
    chr_banks: [[usize; 2]; 2],
    latches: [usize; 2],
}

impl Mmc2 {
    pub fn new(file: INesFile) -> Self {
        let variant = if file.mapper == 10 { Variant::Mmc4 } else { Variant::Mmc2 };
        Mmc2 {
            variant,
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(if variant == Variant::Mmc4 { file.prg_ram_size } else { 0 }, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_bank: 0,
            chr_banks: [[0; 2]; 2],
            latches: [1, 1],
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let offset = addr as usize - 0x8000;

        match self.variant {
            Variant::Mmc2 => {
                let banks = (len / 0x2000).max(1);
                let bank = if addr < 0xA000 { self.prg_bank } else { banks.saturating_sub(3) + (offset - 0x2000) / 0x2000 };
                bank_index(len, 0x2000, bank, offset & 0x1FFF)
            }
            Variant::Mmc4 => {
                if addr < 0xC000 {
                    bank_index(len, 0x4000, self.prg_bank, offset)
                } else {
                    let last = (len / 0x4000).max(1) - 1;
                    bank_index(len, 0x4000, last, offset & 0x3FFF)
                }
            }
        }
    }

    /// A fetch of the $FD or $FE tile row flips the latch of that window (after the data has been returned).
    fn update_latches(&mut self, addr: u16) {
        let addr = addr & 0x1FFF;
        let range = match self.variant {
            Variant::Mmc2 => 0, // only the exact address
            Variant::Mmc4 => 7, // the whole 8-byte row of the tile
        };

        for (window, base) in [(0usize, 0x0000u16), (1, 0x1000)] {
            let fd = base + 0x0FD8;
            let fe = base + 0x0FE8;
            if (fd..=fd + range).contains(&addr) {
                self.latches[window] = 0;
            } else if (fe..=fe + range).contains(&addr) {
                self.latches[window] = 1;
            }
        }
    }
}

impl Mapper for Mmc2 {
    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.write(addr as usize - 0x6000, data),
            0xA000..=0xAFFF => self.prg_bank = (data & 0x0F) as usize,
            0xB000..=0xBFFF => self.chr_banks[0][0] = (data & 0x1F) as usize,
            0xC000..=0xCFFF => self.chr_banks[0][1] = (data & 0x1F) as usize,
            0xD000..=0xDFFF => self.chr_banks[1][0] = (data & 0x1F) as usize,
            0xE000..=0xEFFF => self.chr_banks[1][1] = (data & 0x1F) as usize,
            0xF000..=0xFFFF => self.mirroring = if data & 1 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical },
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let window = (addr as usize >> 12) & 1;
        let bank = self.chr_banks[window][self.latches[window]];
        let value = self.chr.read_banked(CHR_4K, bank, addr as usize & 0x0FFF);
        self.update_latches(addr);
        value
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let window = (addr as usize >> 12) & 1;
        let bank = self.chr_banks[window][self.latches[window]];
        self.chr.write_banked(CHR_4K, bank, addr as usize & 0x0FFF, data);
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

    /// CHR byte = 1KB unit number; 4KB bank n therefore starts with the value 4n.
    fn mmc2() -> Mmc2 {
        Mmc2::new(file(9, 8, 8)) // 128KB PRG (16 banks of 8KB), 64KB CHR (16 banks of 4KB)
    }

    fn mmc4() -> Mmc2 {
        Mmc2::new(file(10, 8, 8))
    }

    #[test]
    fn mmc2_prg_has_one_switchable_8kb_bank_and_three_fixed() {
        let mut m = mmc2();
        m.cpu_write(0xA000, 5);

        assert_eq!(m.cpu_read(0x8000), Some(5));
        assert_eq!(m.cpu_read(0xA000), Some(13), "third from last of 16 banks");
        assert_eq!(m.cpu_read(0xC000), Some(14));
        assert_eq!(m.cpu_read(0xE000), Some(15));
    }

    #[test]
    fn mmc4_prg_has_a_16kb_switchable_bank_and_a_fixed_last_bank() {
        let mut m = mmc4();
        m.cpu_write(0xA000, 3);

        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.cpu_read(0xC000), Some(14));
    }

    #[test]
    fn chr_banks_follow_the_latches_after_the_trigger_tile_is_fetched() {
        let mut m = mmc2();
        m.cpu_write(0xB000, 2); // left window, latch FD
        m.cpu_write(0xC000, 3); // left window, latch FE
        m.cpu_write(0xD000, 4); // right window, latch FD
        m.cpu_write(0xE000, 5); // right window, latch FE

        // power-on latches select the $FE banks
        assert_eq!(m.ppu_read(0x0000), 12);
        assert_eq!(m.ppu_read(0x1000), 20);

        // reading $0FD8 returns the old bank's data and then switches the left window to its FD bank
        assert_eq!(m.ppu_read(0x0FD8), 15, "last byte area of the FE bank");
        assert_eq!(m.ppu_read(0x0000), 8, "now bank 2");
        assert_eq!(m.ppu_read(0x1000), 20, "the right window is independent");

        m.ppu_read(0x0FE8);
        assert_eq!(m.ppu_read(0x0000), 12);

        m.ppu_read(0x1FD8);
        assert_eq!(m.ppu_read(0x1000), 16);
        m.ppu_read(0x1FE8);
        assert_eq!(m.ppu_read(0x1000), 20);
    }

    #[test]
    fn mmc2_latches_react_to_the_exact_address_but_mmc4_to_the_whole_row() {
        let mut mmc2 = mmc2();
        mmc2.cpu_write(0xB000, 2);
        mmc2.cpu_write(0xC000, 3);
        mmc2.ppu_read(0x0FDA);
        assert_eq!(mmc2.ppu_read(0x0000), 12, "MMC2: $0FDA does not flip the latch");

        let mut mmc4 = mmc4();
        mmc4.cpu_write(0xB000, 2);
        mmc4.cpu_write(0xC000, 3);
        mmc4.ppu_read(0x0FDA);
        assert_eq!(mmc4.ppu_read(0x0000), 8, "MMC4: any of $0FD8-$0FDF does");
    }

    #[test]
    fn mirroring_register() {
        let mut m = mmc2();
        m.cpu_write(0xF000, 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
        m.cpu_write(0xF000, 0);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn only_the_mmc4_has_prg_ram() {
        let mut mmc2 = mmc2();
        mmc2.cpu_write(0x6000, 1);
        assert_eq!(mmc2.cpu_read(0x6000), None);

        let mut mmc4 = mmc4();
        mmc4.cpu_write(0x6000, 0x42);
        assert_eq!(mmc4.cpu_read(0x6000), Some(0x42));
    }
}
