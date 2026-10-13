//! Loads the real nestest.nes through the public cartridge API.

use std::fs;

use nesrs::emulator::cartridge::{Cartridge, INesFile, Mirroring};

const ROM_PATH: &str = "test_rom/nestest.nes";

#[test]
fn nestest_loads_as_nrom() {
    let data = fs::read(ROM_PATH).expect("missing test_rom/nestest.nes");
    let file = INesFile::parse(&data).unwrap();

    assert_eq!(file.mapper, 0);
    assert!(!file.nes2);
    assert_eq!(file.prg_rom.len(), 16 * 1024);
    assert_eq!(file.chr_rom.len(), 8 * 1024);

    let mut cartridge = Cartridge::from_bytes(&data).unwrap();

    assert_eq!(cartridge.mapper_id(), 0);
    assert_eq!(cartridge.mirroring(), Mirroring::Horizontal);

    // PRG is visible at $8000 and mirrored at $C000; vectors are the last six bytes of PRG
    for offset in [0usize, 1, 0x1234, 0x3FFA, 0x3FFC, 0x3FFF] {
        assert_eq!(cartridge.cpu_read(0x8000 + offset as u16), Some(file.prg_rom[offset]));
        assert_eq!(cartridge.cpu_read(0xC000 + offset as u16), Some(file.prg_rom[offset]));
    }

    // CHR is visible to the PPU
    for offset in [0usize, 0x10, 0x1FFF] {
        assert_eq!(cartridge.ppu_read(offset as u16), file.chr_rom[offset]);
    }
}
