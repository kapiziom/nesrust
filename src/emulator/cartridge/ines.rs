//! iNES and NES 2.0 file parsing.
//! https://www.nesdev.org/wiki/INES, https://www.nesdev.org/wiki/NES_2.0

use std::fmt;

use super::mirroring::Mirroring;
use crate::emulator::region::Region;

const HEADER_SIZE: usize = 16;
const TRAINER_SIZE: usize = 512;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const DEFAULT_PRG_RAM_SIZE: usize = 8 * 1024;
const DEFAULT_CHR_RAM_SIZE: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    /// Fewer than 16 bytes.
    TooShort,
    /// Does not start with `NES\x1A`.
    InvalidMagic,
    /// The header announces more data than the file contains.
    Truncated {
        expected: usize,
        actual: usize,
    },
    /// The header has no PRG ROM, or a size that cannot be represented.
    InvalidSize,
    UnsupportedMapper(u16),
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CartridgeError::TooShort => write!(f, "file is shorter than the 16-byte iNES header"),
            CartridgeError::InvalidMagic => write!(f, "invalid NES header"),
            CartridgeError::Truncated { expected, actual } => {
                write!(f, "file is truncated: header announces {expected} bytes, file has {actual}")
            }
            CartridgeError::InvalidSize => write!(f, "header describes an empty or invalid PRG/CHR size"),
            CartridgeError::UnsupportedMapper(id) => write!(f, "unsupported mapper {id}"),
        }
    }
}

impl std::error::Error for CartridgeError {}

/// The contents of an iNES / NES 2.0 file, before a mapper is attached.
#[derive(Debug, Clone)]
pub struct INesFile {
    pub mapper: u16,
    pub submapper: u8,
    pub nes2: bool,
    /// Mirroring wired on the board (some mappers override it at runtime).
    pub mirroring: Mirroring,
    pub battery: bool,
    /// Console timing the game was made for (NES 2.0 byte 12, or the iNES 1 flag in byte 9).
    pub region: Region,
    pub prg_rom: Vec<u8>,
    /// Empty when the board has CHR-RAM instead.
    pub chr_rom: Vec<u8>,
    /// Total PRG-RAM (volatile + battery-backed) at $6000-$7FFF.
    pub prg_ram_size: usize,
    /// CHR-RAM size, used only when `chr_rom` is empty.
    pub chr_ram_size: usize,
}

impl INesFile {
    pub fn parse(data: &[u8]) -> Result<Self, CartridgeError> {
        if data.len() < HEADER_SIZE {
            return Err(CartridgeError::TooShort);
        }
        if &data[0..4] != b"NES\x1A" {
            return Err(CartridgeError::InvalidMagic);
        }

        let flags6 = data[6];
        let flags7 = data[7];
        let nes2 = flags7 & 0x0C == 0x08;

        let (mapper, submapper) = if nes2 {
            let id = (flags6 >> 4) as u16 | (flags7 & 0xF0) as u16 | ((data[8] & 0x0F) as u16) << 8;
            (id, data[8] >> 4)
        } else if data[12..16].iter().any(|&byte| byte != 0) {
            // old dumps (e.g. "DiskDude!") left garbage in the unused bytes, including flags 7
            ((flags6 >> 4) as u16, 0)
        } else {
            (((flags6 >> 4) | (flags7 & 0xF0)) as u16, 0)
        };

        let (prg_rom_size, chr_rom_size) = if nes2 {
            (
                nes2_rom_size(data[4], data[9] & 0x0F, PRG_BANK_SIZE).ok_or(CartridgeError::InvalidSize)?,
                nes2_rom_size(data[5], data[9] >> 4, CHR_BANK_SIZE).ok_or(CartridgeError::InvalidSize)?,
            )
        } else {
            (data[4] as usize * PRG_BANK_SIZE, data[5] as usize * CHR_BANK_SIZE)
        };

        if prg_rom_size == 0 {
            return Err(CartridgeError::InvalidSize);
        }

        let trainer_size = if flags6 & 0x04 != 0 { TRAINER_SIZE } else { 0 };
        let prg_start = HEADER_SIZE + trainer_size;
        let chr_start = prg_start.checked_add(prg_rom_size).ok_or(CartridgeError::InvalidSize)?;
        let expected = chr_start.checked_add(chr_rom_size).ok_or(CartridgeError::InvalidSize)?;

        if data.len() < expected {
            return Err(CartridgeError::Truncated { expected, actual: data.len() });
        }

        // NES 2.0: 0 = NTSC, 1 = PAL, 2 = works on both (NTSC is used), 3 = Dendy.
        // iNES 1: bit 0 of byte 9 marks PAL, though many dumps never set it.
        let region = if nes2 {
            match data[12] & 0x03 {
                1 => Region::Pal,
                3 => Region::Dendy,
                _ => Region::Ntsc,
            }
        } else if data[9] & 0x01 != 0 {
            Region::Pal
        } else {
            Region::Ntsc
        };

        let (prg_ram_size, chr_ram_size) = if nes2 {
            (
                nes2_ram_size(data[10] & 0x0F) + nes2_ram_size(data[10] >> 4),
                nes2_ram_size(data[11] & 0x0F) + nes2_ram_size(data[11] >> 4),
            )
        } else {
            // iNES 1 does not say whether PRG-RAM exists; byte 8 of 0 means 8KB for compatibility
            let banks = if data[8] == 0 { 1 } else { data[8] as usize };
            (banks * DEFAULT_PRG_RAM_SIZE, 0)
        };

        let chr_ram_size = if chr_rom_size == 0 && chr_ram_size == 0 { DEFAULT_CHR_RAM_SIZE } else { chr_ram_size };

        let mirroring = if flags6 & 0x08 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        Ok(INesFile {
            mapper,
            submapper,
            nes2,
            mirroring,
            battery: flags6 & 0x02 != 0,
            region,
            prg_rom: data[prg_start..chr_start].to_vec(),
            chr_rom: data[chr_start..expected].to_vec(),
            prg_ram_size,
            chr_ram_size,
        })
    }
}

/// NES 2.0 ROM size: a 12-bit bank count, or (when the high nibble is 0xF) exponent-multiplier notation.
fn nes2_rom_size(lsb: u8, msb_nibble: u8, bank_size: usize) -> Option<usize> {
    if msb_nibble == 0x0F {
        let exponent = (lsb >> 2) as u32;
        let multiplier = ((lsb & 0x03) as usize) * 2 + 1;
        1usize.checked_shl(exponent)?.checked_mul(multiplier)
    } else {
        (((msb_nibble as usize) << 8) | lsb as usize).checked_mul(bank_size)
    }
}

/// NES 2.0 RAM size field: 0 means none, otherwise 64 << shift bytes.
fn nes2_ram_size(shift: u8) -> usize {
    if shift == 0 {
        0
    } else {
        64usize << shift
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(prg_banks: u8, chr_banks: u8, flags6: u8, flags7: u8) -> Vec<u8> {
        let mut data = vec![0; HEADER_SIZE];
        data[0..4].copy_from_slice(b"NES\x1A");
        data[4] = prg_banks;
        data[5] = chr_banks;
        data[6] = flags6;
        data[7] = flags7;
        data
    }

    fn rom_file(prg_banks: u8, chr_banks: u8, flags6: u8, flags7: u8) -> Vec<u8> {
        let mut data = header(prg_banks, chr_banks, flags6, flags7);
        data.extend((0..prg_banks as usize * PRG_BANK_SIZE).map(|i| (i % 251) as u8));
        data.extend((0..chr_banks as usize * CHR_BANK_SIZE).map(|i| (i % 241) as u8));
        data
    }

    #[test]
    fn parses_ines1_header() {
        let file = INesFile::parse(&rom_file(2, 1, 0b0001_0011, 0x20)).unwrap();

        assert!(!file.nes2);
        assert_eq!(file.mapper, 0x21);
        assert_eq!(file.mirroring, Mirroring::Vertical);
        assert!(file.battery);
        assert_eq!(file.prg_rom.len(), 2 * PRG_BANK_SIZE);
        assert_eq!(file.chr_rom.len(), CHR_BANK_SIZE);
        assert_eq!(file.prg_ram_size, DEFAULT_PRG_RAM_SIZE);
        assert_eq!(file.chr_ram_size, 0);
        assert_eq!(file.prg_rom[300], (300 % 251) as u8);
        assert_eq!(file.chr_rom[300], (300 % 241) as u8);
    }

    #[test]
    fn four_screen_overrides_vertical_bit() {
        let file = INesFile::parse(&rom_file(1, 1, 0b0000_1001, 0)).unwrap();
        assert_eq!(file.mirroring, Mirroring::FourScreen);
    }

    #[test]
    fn zero_chr_banks_means_chr_ram() {
        let file = INesFile::parse(&rom_file(1, 0, 0, 0)).unwrap();

        assert!(file.chr_rom.is_empty());
        assert_eq!(file.chr_ram_size, DEFAULT_CHR_RAM_SIZE);
    }

    #[test]
    fn trainer_is_skipped() {
        let mut data = header(1, 0, 0b0000_0100, 0);
        data.extend(vec![0xEE; TRAINER_SIZE]);
        data.extend(vec![0xAB; PRG_BANK_SIZE]);

        let file = INesFile::parse(&data).unwrap();

        assert_eq!(file.prg_rom.len(), PRG_BANK_SIZE);
        assert_eq!(file.prg_rom[0], 0xAB);
    }

    #[test]
    fn archaic_header_garbage_is_ignored() {
        let mut data = rom_file(1, 1, 0x10, 0x40);
        data[12..16].copy_from_slice(b"Disk");

        assert_eq!(INesFile::parse(&data).unwrap().mapper, 1);
    }

    #[test]
    fn parses_nes2_header() {
        let mut data = rom_file(1, 1, 0x10, 0x28);
        data[8] = 0x31; // submapper 3, mapper bits 8-11 = 1
        data[10] = 0x71; // PRG-RAM 128B, PRG-NVRAM 8KB
        data[11] = 0x07; // CHR-RAM 8KB

        let file = INesFile::parse(&data).unwrap();

        assert!(file.nes2);
        assert_eq!(file.mapper, 0x121);
        assert_eq!(file.submapper, 3);
        assert_eq!(file.prg_ram_size, 128 + 8192);
        assert_eq!(file.chr_ram_size, 8192);
    }

    #[test]
    fn nes2_exponent_notation() {
        // PRG: exponent 14, multiplier 1 -> 16KB (lsb = 14 << 2 | 0), msb nibble 0xF
        let mut data = header(0, 0, 0, 0x08);
        data[4] = 14 << 2;
        data[9] = 0x0F;
        data.extend(vec![0; PRG_BANK_SIZE]);

        assert_eq!(INesFile::parse(&data).unwrap().prg_rom.len(), PRG_BANK_SIZE);
    }

    #[test]
    fn nes2_absurd_sizes_are_errors_not_panics() {
        let mut data = header(0, 0, 0, 0x08);
        data[4] = 63 << 2 | 3; // 2^63 * 7 overflows
        data[9] = 0x0F;

        assert_eq!(INesFile::parse(&data).unwrap_err(), CartridgeError::InvalidSize);
    }

    #[test]
    fn region_comes_from_nes2_byte_12_or_the_ines1_pal_flag() {
        assert_eq!(INesFile::parse(&rom_file(1, 1, 0, 0)).unwrap().region, Region::Ntsc);

        let mut nes2_pal = rom_file(1, 1, 0, 0x08);
        nes2_pal[12] = 1;
        assert_eq!(INesFile::parse(&nes2_pal).unwrap().region, Region::Pal);

        let mut nes2_dendy = rom_file(1, 1, 0, 0x08);
        nes2_dendy[12] = 3;
        assert_eq!(INesFile::parse(&nes2_dendy).unwrap().region, Region::Dendy);

        let mut nes2_multi = rom_file(1, 1, 0, 0x08);
        nes2_multi[12] = 2;
        assert_eq!(INesFile::parse(&nes2_multi).unwrap().region, Region::Ntsc);

        let mut ines1_pal = rom_file(1, 1, 0, 0);
        ines1_pal[9] = 1;
        assert_eq!(INesFile::parse(&ines1_pal).unwrap().region, Region::Pal);

        let mut ines1_garbage = rom_file(1, 1, 0, 0);
        ines1_garbage[12] = 1; // byte 12 means nothing in iNES 1
        assert_eq!(INesFile::parse(&ines1_garbage).unwrap().region, Region::Ntsc);
    }

    #[test]
    fn rejects_bad_files() {
        assert_eq!(INesFile::parse(&[]).unwrap_err(), CartridgeError::TooShort);
        assert_eq!(INesFile::parse(&[0; 15]).unwrap_err(), CartridgeError::TooShort);

        let mut bad_magic = rom_file(1, 1, 0, 0);
        bad_magic[0] = b'X';
        assert_eq!(INesFile::parse(&bad_magic).unwrap_err(), CartridgeError::InvalidMagic);

        assert_eq!(INesFile::parse(&rom_file(0, 1, 0, 0)).unwrap_err(), CartridgeError::InvalidSize);

        let mut truncated = rom_file(2, 1, 0, 0);
        truncated.truncate(100);
        assert!(matches!(INesFile::parse(&truncated).unwrap_err(), CartridgeError::Truncated { .. }));
    }

    #[test]
    fn error_messages_are_readable() {
        assert_eq!(CartridgeError::InvalidMagic.to_string(), "invalid NES header");
        assert_eq!(CartridgeError::UnsupportedMapper(5).to_string(), "unsupported mapper 5");
    }

    /// Arbitrary bytes must produce an error or a file, never a panic.
    #[test]
    fn random_input_never_panics() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for round in 0..20_000 {
            let length = (next() % 3_000) as usize;
            let mut data: Vec<u8> = (0..length.min(64)).map(|_| next() as u8).collect();
            data.resize(length, 0);

            // most rounds start with a valid magic so the size logic is exercised
            if round % 4 != 0 && data.len() >= 4 {
                data[0..4].copy_from_slice(b"NES\x1A");
            }

            let _ = INesFile::parse(&data);
        }
    }
}
