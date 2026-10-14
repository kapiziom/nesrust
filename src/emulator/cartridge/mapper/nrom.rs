//! Mapper 0 (NROM): no bank switching. 16KB PRG is mirrored into both halves.
//! https://www.nesdev.org/wiki/NROM

use super::{mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Nrom {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
}

impl Nrom {
    pub fn new(file: INesFile) -> Self {
        Nrom {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
        }
    }
}

impl Mapper for Nrom {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[(addr as usize - 0x8000) % self.prg_rom.len()]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        if let 0x6000..=0x7FFF = addr {
            self.prg_ram.write(addr as usize - 0x6000, data);
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
    fn nrom_256_maps_both_banks() {
        let mut mapper = Nrom::new(file(0, 2, 1));

        assert_eq!(mapper.cpu_read(0x8000), Some(0));
        assert_eq!(mapper.cpu_read(0xC000), Some(2)); // second 16KB bank = 8KB units 2,3
        assert_eq!(mapper.cpu_read(0xFFFF), Some(3));
    }

    #[test]
    fn nrom_128_mirrors_the_single_bank() {
        let mut mapper = Nrom::new(file(0, 1, 1));

        assert_eq!(mapper.cpu_read(0x8000), mapper.cpu_read(0xC000));
        assert_eq!(mapper.cpu_read(0xBFFF), mapper.cpu_read(0xFFFF));
        assert_eq!(mapper.cpu_read(0xFFFF), Some(1));
    }

    #[test]
    fn rom_writes_are_ignored() {
        let mut mapper = Nrom::new(file(0, 1, 1));
        mapper.cpu_write(0x8000, 0xFF);

        assert_eq!(mapper.cpu_read(0x8000), Some(0));
    }

    #[test]
    fn prg_ram_is_readable_and_writable() {
        let mut mapper = Nrom::new(file(0, 1, 1));
        mapper.cpu_write(0x6000, 0x42);
        mapper.cpu_write(0x7FFF, 0xFF);

        assert_eq!(mapper.cpu_read(0x6000), Some(0x42));
        assert_eq!(mapper.cpu_read(0x7FFF), Some(0xFF));
    }

    #[test]
    fn no_prg_ram_means_open_bus() {
        let mut data = file(0, 1, 1);
        data.prg_ram_size = 0;
        let mut mapper = Nrom::new(data);
        mapper.cpu_write(0x6000, 1);

        assert_eq!(mapper.cpu_read(0x6000), None);
        assert_eq!(mapper.cpu_read(0x5000), None);
    }

    #[test]
    fn battery_ram_is_exposed_only_for_battery_boards() {
        let mut data = file(0, 1, 1);
        assert!(Nrom::new(data.clone()).battery_ram().is_none());

        data.battery = true;
        let mut mapper = Nrom::new(data);
        mapper.cpu_write(0x6001, 7);
        assert_eq!(mapper.battery_ram().unwrap()[1], 7);

        mapper.load_battery_ram(&[5, 6]);
        assert_eq!(mapper.cpu_read(0x6000), Some(5));
    }

    #[test]
    fn chr_rom_is_read_only_and_chr_ram_is_writable() {
        let mut rom_board = Nrom::new(file(0, 1, 1));
        rom_board.ppu_write(0x0010, 0xAA);
        assert_eq!(rom_board.ppu_read(0x0010), 0);
        assert_eq!(rom_board.ppu_read(0x1C00), 7);

        let mut data = file(0, 1, 0);
        data.chr_rom.clear();
        let mut ram_board = Nrom::new(data);
        ram_board.ppu_write(0x0010, 0xAA);
        assert_eq!(ram_board.ppu_read(0x0010), 0xAA);
    }
}
