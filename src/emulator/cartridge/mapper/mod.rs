//! Cartridge mappers: the logic on the board that sits between the CPU/PPU buses and the ROM/RAM chips.
//! https://www.nesdev.org/wiki/Mapper

mod axrom;
mod cnrom;
mod mmc1;
mod mmc3;
mod nrom;
mod uxrom;

use super::ines::{CartridgeError, INesFile};
use super::mirroring::Mirroring;

pub trait Mapper {
    /// CPU read in $4020-$FFFF. `None` means nothing drives the bus (open bus).
    fn cpu_read(&mut self, addr: u16) -> Option<u8>;

    /// CPU write in $4020-$FFFF (registers, PRG-RAM).
    fn cpu_write(&mut self, addr: u16, data: u8);

    /// PPU pattern table read in $0000-$1FFF. Takes `&mut self` because some mappers (MMC3) watch the address lines.
    fn ppu_read(&mut self, addr: u16) -> u8;

    /// PPU pattern table write; only has an effect on CHR-RAM.
    fn ppu_write(&mut self, addr: u16, data: u8);

    /// Current nametable mirroring (may change at runtime).
    fn mirroring(&self) -> Mirroring;

    /// Level of the mapper's IRQ output.
    fn irq_pending(&self) -> bool {
        false
    }

    /// Called once per CPU cycle (MMC1 ignores back-to-back writes, MMC3 filters A12 glitches).
    fn cpu_cycle(&mut self) {}

    /// Battery-backed PRG-RAM contents to persist, if the board has any.
    fn battery_ram(&self) -> Option<&[u8]> {
        None
    }

    /// Restores battery-backed PRG-RAM (e.g. from a `.sav` file); extra or missing bytes are ignored.
    fn load_battery_ram(&mut self, _data: &[u8]) {}
}

pub fn create(file: INesFile) -> Result<Box<dyn Mapper>, CartridgeError> {
    Ok(match file.mapper {
        0 => Box::new(nrom::Nrom::new(file)),
        1 => Box::new(mmc1::Mmc1::new(file)),
        2 => Box::new(uxrom::Uxrom::new(file)),
        3 => Box::new(cnrom::Cnrom::new(file)),
        4 => Box::new(mmc3::Mmc3::new(file)),
        7 => Box::new(axrom::Axrom::new(file)),
        id => return Err(CartridgeError::UnsupportedMapper(id)),
    })
}

/// Offset of `offset` inside bank number `bank`, wrapping the bank number around the chip size
/// (so a 16KB ROM works with a mapper that addresses 256KB).
fn bank_index(len: usize, bank_size: usize, bank: usize, offset: usize) -> usize {
    let banks = (len / bank_size).max(1);
    ((bank % banks) * bank_size + offset) % len
}

/// Pattern table memory: CHR-ROM, or CHR-RAM when the cartridge has no CHR-ROM.
struct Chr {
    data: Vec<u8>,
    writable: bool,
}

impl Chr {
    fn new(chr_rom: Vec<u8>, chr_ram_size: usize) -> Self {
        if chr_rom.is_empty() {
            Chr { data: vec![0; chr_ram_size.max(1)], writable: true }
        } else {
            Chr { data: chr_rom, writable: false }
        }
    }

    fn read(&self, index: usize) -> u8 {
        self.data[index % self.data.len()]
    }

    fn read_banked(&self, bank_size: usize, bank: usize, offset: usize) -> u8 {
        self.data[bank_index(self.data.len(), bank_size, bank, offset)]
    }

    fn write(&mut self, index: usize, value: u8) {
        if self.writable {
            let len = self.data.len();
            self.data[index % len] = value;
        }
    }

    fn write_banked(&mut self, bank_size: usize, bank: usize, offset: usize, value: u8) {
        if self.writable {
            let index = bank_index(self.data.len(), bank_size, bank, offset);
            self.data[index] = value;
        }
    }
}

/// PRG-RAM at $6000-$7FFF.
struct PrgRam {
    data: Vec<u8>,
    battery: bool,
}

impl PrgRam {
    fn new(size: usize, battery: bool) -> Self {
        PrgRam { data: vec![0; size], battery }
    }

    /// `None` when the board has no RAM (open bus).
    fn read(&self, offset: usize) -> Option<u8> {
        if self.data.is_empty() {
            None
        } else {
            Some(self.data[offset % self.data.len()])
        }
    }

    fn write(&mut self, offset: usize, value: u8) {
        if !self.data.is_empty() {
            let len = self.data.len();
            self.data[offset % len] = value;
        }
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        (self.battery && !self.data.is_empty()).then_some(&self.data[..])
    }

    fn load(&mut self, saved: &[u8]) {
        if self.battery {
            let len = self.data.len().min(saved.len());
            self.data[..len].copy_from_slice(&saved[..len]);
        }
    }
}

#[cfg(test)]
mod test_util {
    use super::*;

    /// A file whose PRG byte at offset `o` is `o / 8192` (the 8KB unit number) and whose CHR byte is the 1KB unit number.
    /// Bank switching tests can then read a single byte and know which bank is mapped.
    pub fn file(mapper: u16, prg_banks_16k: usize, chr_banks_8k: usize) -> INesFile {
        INesFile {
            mapper,
            submapper: 0,
            nes2: false,
            mirroring: Mirroring::Vertical,
            battery: false,
            prg_rom: (0..prg_banks_16k * 16384).map(|o| (o / 8192) as u8).collect(),
            chr_rom: (0..chr_banks_8k * 8192).map(|o| (o / 1024) as u8).collect(),
            prg_ram_size: 8192,
            chr_ram_size: 8192,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::file;
    use super::*;

    #[test]
    fn unsupported_mapper_is_an_error() {
        assert!(matches!(create(file(5, 2, 1)), Err(CartridgeError::UnsupportedMapper(5))));
    }

    #[test]
    fn bank_index_wraps_around_small_chips() {
        assert_eq!(bank_index(16384, 16384, 5, 10), 10);
        assert_eq!(bank_index(65536, 16384, 5, 10), 16384 + 10);
        assert_eq!(bank_index(8192, 16384, 0, 100), 100);
    }

    #[test]
    fn prg_ram_battery_roundtrip() {
        let mut ram = PrgRam::new(4, true);
        ram.write(1, 0x42);
        assert_eq!(ram.battery_ram(), Some(&[0, 0x42, 0, 0][..]));

        let mut restored = PrgRam::new(4, true);
        restored.load(&[9, 8, 7, 6, 5, 4]);
        assert_eq!(restored.read(2), Some(7));

        assert_eq!(PrgRam::new(4, false).battery_ram(), None);
        assert_eq!(PrgRam::new(0, true).read(0), None);
    }

    #[test]
    fn chr_rom_is_read_only_and_chr_ram_is_writable() {
        let mut rom = Chr::new(vec![1, 2, 3], 8192);
        rom.write(0, 9);
        assert_eq!(rom.read(0), 1);

        let mut ram = Chr::new(Vec::new(), 8192);
        ram.write(5, 9);
        assert_eq!(ram.read(5), 9);
    }
}
