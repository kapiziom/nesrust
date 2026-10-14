//! A cartridge: the iNES file contents plus the mapper that wires them to the CPU and PPU buses.

pub mod ines;
pub mod mapper;
pub mod mirroring;

pub use ines::{CartridgeError, INesFile};
pub use mirroring::Mirroring;

use mapper::Mapper;

use crate::emulator::region::Region;

pub use mapper::{PpuPhase, StateError};

pub struct Cartridge {
    mapper: Box<dyn Mapper>,
    mapper_id: u16,
    region: Region,
    /// Identifies the ROM contents so a save state is only loaded into the game it was made with.
    rom_checksum: u32,
}

fn checksum<'a>(bytes: impl Iterator<Item = &'a u8>) -> u32 {
    bytes.fold(0x811C_9DC5u32, |hash, &byte| (hash ^ byte as u32).wrapping_mul(0x0100_0193))
}

impl Cartridge {
    /// An empty NROM cartridge that stands in for the real one while a save state is being deserialized.
    pub(crate) fn placeholder() -> Self {
        let mut file = vec![0u8; 16 + 16 * 1024];
        file[..5].copy_from_slice(&[b'N', b'E', b'S', 0x1A, 1]);
        Self::from_bytes(&file).expect("a minimal NROM file is valid")
    }

    /// Parses an iNES / NES 2.0 file and attaches its mapper.
    pub fn from_bytes(data: &[u8]) -> Result<Self, CartridgeError> {
        Self::from_ines(INesFile::parse(data)?)
    }

    pub fn from_ines(file: INesFile) -> Result<Self, CartridgeError> {
        let mapper_id = file.mapper;
        let region = file.region;
        let rom_checksum = checksum(file.prg_rom.iter().chain(&file.chr_rom));
        Ok(Cartridge { mapper: mapper::create(file)?, mapper_id, region, rom_checksum })
    }

    /// FNV-1a hash of the PRG and CHR ROM; save states carry it to detect a different game.
    pub fn rom_checksum(&self) -> u32 {
        self.rom_checksum
    }

    /// The mapper's registers and RAM (see `Nes::save_state`).
    pub fn save_state(&self) -> Vec<u8> {
        self.mapper.save_state()
    }

    pub fn load_state(&mut self, data: &[u8]) -> Result<(), StateError> {
        self.mapper.load_state(data)
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

    pub fn ppu_phase(&mut self, phase: PpuPhase) {
        self.mapper.ppu_phase(phase);
    }

    pub fn ppu_scanline_end(&mut self, scanline: u16) {
        self.mapper.ppu_scanline_end(scanline);
    }

    pub fn ppu_vblank(&mut self) {
        self.mapper.ppu_vblank();
    }

    pub fn ppu_register_write(&mut self, register: u16, data: u8) {
        self.mapper.ppu_register_write(register, data);
    }

    /// Nametable memory provided by the board itself, if any (see `Mapper::nametable_read`).
    pub fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        self.mapper.nametable_read(addr)
    }

    /// Returns true when the board handled the write instead of the console's VRAM.
    pub fn nametable_write(&mut self, addr: u16, data: u8) -> bool {
        self.mapper.nametable_write(addr, data)
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

    /// Sound produced by the cartridge itself (VRC6, Sunsoft 5B, ...), added to the APU output.
    pub fn audio_output(&self) -> f32 {
        self.mapper.audio_output()
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
        for mapper in [
            0, 1, 2, 3, 4, 5, 7, 9, 10, 11, 16, 18, 19, 21, 22, 23, 24, 25, 26, 32, 33, 34, 48, 65, 66, 67, 69, 71, 72,
            73, 75, 85, 76, 78, 79, 80, 87, 88, 89, 92, 93, 94, 95, 97, 118, 119, 140, 152, 154, 159, 180, 184, 185,
            206, 207, 210,
        ] {
            assert!(Cartridge::from_bytes(&nes_file(mapper)).is_ok(), "mapper {mapper}");
        }
    }

    #[test]
    fn unsupported_mapper_is_reported() {
        let error = Cartridge::from_bytes(&nes_file(6)).err().unwrap();

        assert_eq!(error, CartridgeError::UnsupportedMapper(6));
    }

    #[test]
    fn malformed_files_are_errors() {
        assert_eq!(Cartridge::from_bytes(&[]).err(), Some(CartridgeError::TooShort));

        let mut bad = nes_file(0);
        bad[0] = b'X';
        assert_eq!(Cartridge::from_bytes(&bad).err(), Some(CartridgeError::InvalidMagic));
    }
}
