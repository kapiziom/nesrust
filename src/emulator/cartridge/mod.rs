//! A cartridge: the iNES file contents plus the mapper that wires them to the CPU and PPU buses.

pub mod ines;
pub mod mapper;
pub mod mirroring;

pub use ines::{CartridgeError, INesFile};
pub use mirroring::Mirroring;

use mapper::Mapper;

use crate::emulator::region::Region;

pub struct Cartridge {
    mapper: Box<dyn Mapper>,
    mapper_id: u16,
    region: Region,
}

impl Cartridge {
    /// Parses an iNES / NES 2.0 file and attaches its mapper.
    pub fn from_bytes(data: &[u8]) -> Result<Self, CartridgeError> {
        Self::from_ines(INesFile::parse(data)?)
    }

    pub fn from_ines(file: INesFile) -> Result<Self, CartridgeError> {
        let mapper_id = file.mapper;
        let region = file.region;
        Ok(Cartridge { mapper: mapper::create(file)?, mapper_id, region })
    }

    pub fn mapper_id(&self) -> u16 {
        self.mapper_id
    }

    /// The console timing (NTSC or PAL) the cartridge was made for.
    pub fn region(&self) -> Region {
        self.region
    }

    /// CPU read in $4020-$FFFF; `None` means open bus.
    pub fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        self.mapper.cpu_read(addr)
    }

    pub fn cpu_write(&mut self, addr: u16, data: u8) {
        self.mapper.cpu_write(addr, data);
    }

    /// PPU pattern table access ($0000-$1FFF).
    pub fn ppu_read(&mut self, addr: u16) -> u8 {
        self.mapper.ppu_read(addr)
    }

    pub fn ppu_write(&mut self, addr: u16, data: u8) {
        self.mapper.ppu_write(addr, data);
    }

    /// The PPU drove `addr` onto its address bus outside of a pattern table access.
    pub fn ppu_address_changed(&mut self, addr: u16) {
        self.mapper.ppu_address_changed(addr);
    }

    pub fn mirroring(&self) -> Mirroring {
        self.mapper.mirroring()
    }

    pub fn irq_pending(&self) -> bool {
        self.mapper.irq_pending()
    }

    /// Must be called once per CPU cycle.
    pub fn cpu_cycle(&mut self) {
        self.mapper.cpu_cycle();
    }

    /// Battery-backed RAM to save to disk (the frontend does the I/O), if the cartridge has any.
    pub fn battery_ram(&self) -> Option<&[u8]> {
        self.mapper.battery_ram()
    }

    pub fn load_battery_ram(&mut self, data: &[u8]) {
        self.mapper.load_battery_ram(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nes_file(mapper: u16) -> Vec<u8> {
        let mut data = vec![0; 16 + 16384 + 8192];
        data[0..4].copy_from_slice(b"NES\x1A");
        data[4] = 1;
        data[5] = 1;
        data[6] = (mapper as u8) << 4;
        data[7] = mapper as u8 & 0xF0;
        data[16] = 0xA9; // first PRG byte
        data
    }

    #[test]
    fn loads_an_nrom_file() {
        let mut cartridge = Cartridge::from_bytes(&nes_file(0)).unwrap();

        assert_eq!(cartridge.mapper_id(), 0);
        assert_eq!(cartridge.cpu_read(0x8000), Some(0xA9));
        assert_eq!(cartridge.cpu_read(0xC000), Some(0xA9), "16KB PRG is mirrored");
        assert_eq!(cartridge.cpu_read(0x4020), None);
        assert_eq!(cartridge.mirroring(), Mirroring::Horizontal);
        assert!(!cartridge.irq_pending());
    }

    #[test]
    fn supported_mappers_load() {
        for mapper in [0, 1, 2, 3, 4, 7, 9, 10, 11, 21, 22, 23, 25, 34, 66, 69, 71, 79, 87, 140, 152, 180, 206] {
            assert!(Cartridge::from_bytes(&nes_file(mapper)).is_ok(), "mapper {mapper}");
        }
    }

    #[test]
    fn unsupported_mapper_is_reported() {
        let error = Cartridge::from_bytes(&nes_file(5)).err().unwrap();

        assert_eq!(error, CartridgeError::UnsupportedMapper(5));
    }

    #[test]
    fn malformed_files_are_errors() {
        assert_eq!(Cartridge::from_bytes(&[]).err(), Some(CartridgeError::TooShort));

        let mut bad = nes_file(0);
        bad[0] = b'X';
        assert_eq!(Cartridge::from_bytes(&bad).err(), Some(CartridgeError::InvalidMagic));
    }
}
