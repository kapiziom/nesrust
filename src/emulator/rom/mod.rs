use crate::emulator::rom::mirroring::Mirroring;
pub mod mirroring;

#[derive(Debug)]
pub struct ROM {
    pub prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    sram: Vec<u8>,
    expansion: Vec<u8>,
    mapper: u8,
    mirroring: Mirroring,
    battery: bool,
}


impl ROM {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mapper: u8, mirroring: Mirroring, battery: bool) -> Self {
        ROM {
            prg_rom,
            chr_rom,
            sram: vec![0; 8192], // 8KB
            expansion: Vec::new(),
            mapper,
            mirroring,
            battery,
        }
    }

    pub fn read_prg(&self, addr: u16) -> u8 {
        let prg_addr = (addr - 0x8000) as usize;
        match self.mapper {
            0 => { // NROM
                if self.prg_rom.len() == 16384 && prg_addr >= 16384 {
                    self.prg_rom[prg_addr % 16384]
                } else {
                    self.prg_rom[prg_addr % self.prg_rom.len()]
                }
            }
            // todo other mappers
            _ => panic!("Unsupported mapper: {}", self.mapper)
        }
    }

    pub fn write_prg(&mut self, address: u16, data: u8) {
        match self.mapper {
            0 => {}, // NROM - read only
            1 => {   // MMC1
            },
            // todo other mappers
            _ => panic!("Unsupported mapper: {}", self.mapper)
        }
    }

    pub fn read_sram(&self, addr: u16) -> u8 {
        if self.battery {
            self.sram[(addr - 0x6000) as usize]
        } else {
            0
        }
    }

    pub fn write_sram(&mut self, addr: u16, data: u8) {
        if self.battery {
            self.sram[(addr - 0x6000) as usize] = data;
        }
    }

    pub fn read_expansion(&self, addr: u16) -> u8 {
        if !self.expansion.is_empty() {
            self.expansion[(addr - 0x4020) as usize]
        } else {
            0
        }
    }

    pub fn write_expansion(&mut self, addr: u16, data: u8) {
        if !self.expansion.is_empty() {
            self.expansion[(addr - 0x4020) as usize] = data;
        }
    }

    pub fn from_nes_file(data: &[u8]) -> Result<Self, String> {
        if &data[0..4] != b"NES\x1A" {
            return Err("Invalid NES header".into());
        }

        let prg_rom_size = data[4] as usize * 16384;
        let chr_rom_size = data[5] as usize * 8192;

        let flags6 = data[6];
        let flags7 = data[7];

        let mapper = (flags7 & 0xF0) | (flags6 >> 4);
        let mirroring = if flags6 & 0x08 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };
        let battery = flags6 & 0x02 != 0;

        let prg_rom = data[16..16 + prg_rom_size].to_vec();
        let chr_rom = data[16 + prg_rom_size..16 + prg_rom_size + chr_rom_size].to_vec();

        Ok(Self::new(prg_rom, chr_rom, mapper, mirroring, battery))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_rom() -> Vec<u8> {
        let mut rom = vec![0; 16 + 32768 + 8192]; // Header + 32KB PRG + 8KB CHR
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 2;  // 2 * 16KB PRG ROM
        rom[5] = 1;  // 1 * 8KB CHR ROM
        rom[6] = 0;  // Mapper 0, horizontal mirroring
        rom[7] = 0;  // Mapper 0 (upper bits)

        for i in 0..32768 {
            rom[16 + i] = (i % 256) as u8;
        }

        for i in 0..8192 {
            rom[16 + 32768 + i] = (i % 256) as u8;
        }

        rom
    }

    #[test]
    fn test_rom_loading() {
        let test_data = create_test_rom();
        let rom = ROM::from_nes_file(&test_data).unwrap();

        assert_eq!(rom.prg_rom.len(), 32768);
        assert_eq!(rom.chr_rom.len(), 8192);
        assert_eq!(rom.mapper, 0);
        assert_eq!(rom.mirroring, Mirroring::Horizontal);
        assert_eq!(rom.battery, false);
    }

    #[test]
    fn test_invalid_rom_header() {
        let mut bad_data = create_test_rom();
        bad_data[0] = b'X';

        let result = ROM::from_nes_file(&bad_data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid NES header");
    }

    #[test]
    fn test_prg_rom_reading() {
        let test_data = create_test_rom();
        let rom = ROM::from_nes_file(&test_data).unwrap();

        assert_eq!(rom.read_prg(0x8000), test_data[16]);

        assert_eq!(rom.read_prg(0xBFFF), test_data[16 + 0x3FFF]);

        assert_eq!(rom.read_prg(0xC000), test_data[16 + 0x4000]);
    }

    #[test]
    fn test_prg_rom_mirroring() {
        let mut test_data = create_test_rom();
        test_data[4] = 1; // 1 * 16KB PRG ROM
        let rom = ROM::from_nes_file(&test_data).unwrap();

        assert_eq!(rom.read_prg(0x8000), rom.read_prg(0xC000));
        assert_eq!(rom.read_prg(0x9FFF), rom.read_prg(0xDFFF));
    }

    #[test]
    fn test_sram_operations() {
        let test_data = create_test_rom();
        let mut rom = ROM::from_nes_file(&test_data).unwrap();
        rom.battery = true;

        rom.write_sram(0x6000, 0x42);
        assert_eq!(rom.read_sram(0x6000), 0x42);

        rom.write_sram(0x7FFF, 0xFF);
        assert_eq!(rom.read_sram(0x7FFF), 0xFF);
    }

    #[test]
    fn test_battery_backed_ram() {
        let mut test_data = create_test_rom();
        test_data[6] |= 0x02;
        let mut rom = ROM::from_nes_file(&test_data).unwrap();

        assert!(rom.battery);

        rom.write_sram(0x6000, 0x42);
        assert_eq!(rom.read_sram(0x6000), 0x42);
    }
}