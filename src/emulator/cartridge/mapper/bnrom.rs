//! Mapper 34: BNROM (32KB PRG bank register, CHR-RAM) and NINA-001 (PRG-RAM plus registers at $7FFD-$7FFF, CHR-ROM).
//! https://www.nesdev.org/wiki/INES_Mapper_034

use super::{mapper_state, bank_index, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_32K: usize = 32 * 1024;
const CHR_4K: usize = 4 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Bnrom {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
    /// NINA-001 boards have CHR-ROM and the registers in PRG-RAM space; BNROM boards have CHR-RAM.
    nina_001: bool,
    prg_bank: usize,
    chr_banks: [usize; 2],
}

impl Bnrom {
    pub fn new(file: INesFile) -> Self {
        let nina_001 = !file.chr_rom.is_empty();
        Bnrom {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(if nina_001 { file.prg_ram_size } else { 0 }, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            nina_001,
            prg_bank: 0,
            chr_banks: [0, 1],
        }
    }
}

impl Mapper for Bnrom {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => {
                Some(self.prg_rom[bank_index(self.prg_rom.len(), PRG_32K, self.prg_bank, addr as usize & 0x7FFF)])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if self.nina_001 {
            if let 0x6000..=0x7FFF = addr {
                self.prg_ram.write(addr as usize - 0x6000, data);
            }
            match addr {
                0x7FFD => self.prg_bank = data as usize,
                0x7FFE => self.chr_banks[0] = data as usize,
                0x7FFF => self.chr_banks[1] = data as usize,
                _ => {}
            }
        } else if addr >= 0x8000 {
            self.prg_bank = data as usize;
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        if self.nina_001 {
            let window = (addr as usize >> 12) & 1;
            self.chr.read_banked(CHR_4K, self.chr_banks[window], addr as usize & 0x0FFF)
        } else {
            self.chr.read(addr as usize)
        }
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        if self.nina_001 {
            let window = (addr as usize >> 12) & 1;
            self.chr.write_banked(CHR_4K, self.chr_banks[window], addr as usize & 0x0FFF, data);
        } else {
            self.chr.write(addr as usize, data);
        }
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

    #[test]
    fn bnrom_switches_32kb_prg_with_any_write_to_rom_space() {
        let mut data = file(34, 8, 0);
        data.chr_rom.clear();
        let mut m = Bnrom::new(data);

        m.cpu_write(0x8000, 2);

        assert_eq!(m.cpu_read(0x8000), Some(8));
        assert_eq!(m.cpu_read(0xFFFF), Some(11));
    }

    #[test]
    fn bnrom_has_chr_ram_and_no_prg_ram() {
        let mut data = file(34, 4, 0);
        data.chr_rom.clear();
        let mut m = Bnrom::new(data);

        m.ppu_write(0x0123, 0x77);

        assert_eq!(m.ppu_read(0x0123), 0x77);
        m.cpu_write(0x6000, 1);
        assert_eq!(m.cpu_read(0x6000), None);
    }

    #[test]
    fn nina_001_registers_are_in_the_prg_ram_area() {
        let mut m = Bnrom::new(file(34, 4, 4)); // CHR-ROM: NINA-001

        m.cpu_write(0x7FFD, 1);
        m.cpu_write(0x7FFE, 3); // CHR 4KB bank 3 at $0000
        m.cpu_write(0x7FFF, 5); // CHR 4KB bank 5 at $1000

        assert_eq!(m.cpu_read(0x8000), Some(4));
        assert_eq!(m.ppu_read(0x0000), 12);
        assert_eq!(m.ppu_read(0x1000), 20);
        assert_eq!(m.ppu_read(0x1FFF), 23);
    }

    #[test]
    fn nina_001_has_working_prg_ram_and_ignores_writes_to_rom_space() {
        let mut m = Bnrom::new(file(34, 4, 4));
        m.cpu_write(0x6000, 0x42);
        m.cpu_write(0x8000, 3);

        assert_eq!(m.cpu_read(0x6000), Some(0x42));
        assert_eq!(m.cpu_read(0x8000), Some(0));
    }
}
