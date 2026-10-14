//! Cartridge mappers: the logic on the board that sits between the CPU/PPU buses and the ROM/RAM chips.
//! https://www.nesdev.org/wiki/Mapper

mod a12_counter;
mod axrom;
mod bnrom;
mod camerica;
mod cnrom;
mod discrete;
mod fme7;
mod mmc1;
mod mmc2;
mod mmc3;
mod nrom;
mod sunsoft5b;
mod uxrom;
mod vrc2_4;
mod vrc6;
mod vrc_irq;

use super::ines::{CartridgeError, INesFile};
use super::mirroring::Mirroring;
#[cfg(test)]
use crate::emulator::region::Region;

pub trait Mapper {
    /// CPU read in $4020-$FFFF. `None` means nothing drives the bus (open bus).
    fn cpu_read(&mut self, addr: u16) -> Option<u8>;

    /// CPU write in $4020-$FFFF (registers, PRG-RAM).
    fn cpu_write(&mut self, addr: u16, data: u8);

    /// PPU pattern table read in $0000-$1FFF. Takes `&mut self` because some mappers (MMC3) watch the address lines.
    fn ppu_read(&mut self, addr: u16) -> u8;

    /// PPU pattern table write; only has an effect on CHR-RAM.
    fn ppu_write(&mut self, addr: u16, data: u8);

    /// The PPU put an address on its bus without a pattern table access (PPUADDR/PPUDATA while not fetching).
    /// MMC3 watches address line A12 and counts rising edges, wherever they come from.
    fn ppu_address_changed(&mut self, _addr: u16) {}

    /// Current nametable mirroring (may change at runtime).
    fn mirroring(&self) -> Mirroring;

    /// Level of the mapper's IRQ output.
    fn irq_pending(&self) -> bool {
        false
    }

    /// Called once per CPU cycle (MMC1 ignores back-to-back writes, MMC3 filters A12 glitches).
    fn cpu_cycle(&mut self) {}

    /// Level of the mapper's own sound hardware, to be added to the APU mixer output (roughly 0.0 to 0.5, where
    /// the two APU pulse channels at full volume give 0.26). Clocked from `cpu_cycle`.
    fn audio_output(&self) -> f32 {
        0.0
    }

    /// Battery-backed PRG-RAM contents to persist, if the board has any.
    fn battery_ram(&self) -> Option<&[u8]> {
        None
    }

    /// Restores battery-backed PRG-RAM (e.g. from a `.sav` file); extra or missing bytes are ignored.
    fn load_battery_ram(&mut self, _data: &[u8]) {}

    /// The mapper's registers and RAM (not the ROM, which the cartridge file provides) for save states.
    fn save_state(&self) -> Vec<u8>;

    /// Restores what `save_state` returned for the same cartridge.
    fn load_state(&mut self, data: &[u8]) -> Result<(), StateError>;
}

/// A save state could not be restored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    /// The bytes are not a state of this console (truncated or damaged).
    Corrupt,
    /// Saved by an incompatible version of the emulator.
    WrongVersion,
    /// Saved while running another cartridge.
    WrongCartridge,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::Corrupt => write!(f, "the save state is corrupt"),
            StateError::WrongVersion => write!(f, "the save state was made by another version of the emulator"),
            StateError::WrongCartridge => write!(f, "the save state belongs to another cartridge"),
        }
    }
}

impl std::error::Error for StateError {}

/// Implements `save_state`/`load_state` for a mapper struct that derives serde and keeps its ROM in `#[serde(skip)]`
/// fields: the CHR-ROM (inside `chr`) and `prg_rom` are taken over from the running mapper.
macro_rules! mapper_state {
    () => {
        fn save_state(&self) -> Vec<u8> {
            postcard::to_allocvec(self).expect("serializing into a Vec cannot fail")
        }

        fn load_state(&mut self, data: &[u8]) -> Result<(), $crate::emulator::cartridge::mapper::StateError> {
            let mut restored: Self =
                postcard::from_bytes(data).map_err(|_| $crate::emulator::cartridge::mapper::StateError::Corrupt)?;
            restored.prg_rom = std::mem::take(&mut self.prg_rom);
            restored.chr.rom = std::mem::take(&mut self.chr.rom);
            *self = restored;
            Ok(())
        }
    };
}
pub(crate) use mapper_state;

pub fn create(file: INesFile) -> Result<Box<dyn Mapper>, CartridgeError> {
    Ok(match file.mapper {
        0 => Box::new(nrom::Nrom::new(file)),
        1 => Box::new(mmc1::Mmc1::new(file)),
        2 => Box::new(uxrom::Uxrom::new(file)),
        3 => Box::new(cnrom::Cnrom::new(file)),
        4 | 76 | 88 | 95 | 118 | 119 | 154 | 206 => Box::new(mmc3::Mmc3::new(file)),
        7 => Box::new(axrom::Axrom::new(file)),
        9 | 10 => Box::new(mmc2::Mmc2::new(file)),
        11 | 66 | 72 | 78 | 79 | 87 | 89 | 92 | 93 | 94 | 97 | 140 | 152 | 180 | 184 | 185 => {
            Box::new(discrete::Discrete::new(file))
        }
        21 | 22 | 23 | 25 => Box::new(vrc2_4::Vrc2_4::new(file)),
        24 | 26 => Box::new(vrc6::Vrc6::new(file)),
        34 => Box::new(bnrom::Bnrom::new(file)),
        69 => Box::new(fme7::Fme7::new(file)),
        71 => Box::new(camerica::Camerica::new(file)),
        id => return Err(CartridgeError::UnsupportedMapper(id)),
    })
}

/// Offset of `offset` inside bank number `bank`, wrapping the bank number around the chip size
/// (so a 16KB ROM works with a mapper that addresses 256KB).
fn bank_index(len: usize, bank_size: usize, bank: usize, offset: usize) -> usize {
    let banks = (len / bank_size).max(1);
    ((bank % banks) * bank_size + offset) % len
}

/// Pattern table memory: CHR-ROM, or CHR-RAM when the cartridge has no CHR-ROM. Only the RAM is part of a save state.
#[derive(serde::Serialize, serde::Deserialize)]
struct Chr {
    #[serde(skip)]
    rom: Vec<u8>,
    ram: Vec<u8>,
}

impl Chr {
    fn new(chr_rom: Vec<u8>, chr_ram_size: usize) -> Self {
        if chr_rom.is_empty() {
            Chr { rom: Vec::new(), ram: vec![0; chr_ram_size.max(1)] }
        } else {
            Chr { rom: chr_rom, ram: Vec::new() }
        }
    }

    fn data(&self) -> &[u8] {
        if self.rom.is_empty() {
            &self.ram
        } else {
            &self.rom
        }
    }

    fn read(&self, index: usize) -> u8 {
        let data = self.data();
        data[index % data.len()]
    }

    fn read_banked(&self, bank_size: usize, bank: usize, offset: usize) -> u8 {
        let data = self.data();
        data[bank_index(data.len(), bank_size, bank, offset)]
    }

    fn write(&mut self, index: usize, value: u8) {
        if self.rom.is_empty() {
            let len = self.ram.len();
            self.ram[index % len] = value;
        }
    }

    fn write_banked(&mut self, bank_size: usize, bank: usize, offset: usize, value: u8) {
        if self.rom.is_empty() {
            let index = bank_index(self.ram.len(), bank_size, bank, offset);
            self.ram[index] = value;
        }
    }
}

/// PRG-RAM at $6000-$7FFF.
#[derive(serde::Serialize, serde::Deserialize)]
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
            region: Region::Ntsc,
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

    /// Every mapper number `create` accepts; add new ones here so they get the save state round trip test.
    const ALL_MAPPERS: &[u16] = &[
        0, 1, 2, 3, 4, 7, 9, 10, 11, 21, 22, 23, 24, 25, 26, 34, 66, 69, 71, 72, 76, 78, 79, 87, 88, 89, 92, 93, 94,
        95, 97, 118, 119, 140, 152, 154, 180, 184, 185, 206,
    ];

    /// Reads that change nothing but show the banking, the RAM and the latches.
    fn fingerprint(mapper: &mut dyn Mapper) -> Vec<u8> {
        let mut seen = Vec::new();
        for addr in (0x6000u16..=0xFFFF).step_by(0x207) {
            seen.push(mapper.cpu_read(addr).unwrap_or(0xFF));
        }
        for addr in (0u16..0x2000).step_by(0x1D3) {
            seen.push(mapper.ppu_read(addr));
        }
        seen.extend(format!("{:?}", mapper.mirroring()).bytes());
        seen.push(mapper.irq_pending() as u8);
        seen
    }

    #[test]
    fn every_mapper_restores_its_registers_and_ram_from_a_save_state() {
        for &id in ALL_MAPPERS {
            let mut original = create(file(id, 16, 16)).unwrap();
            // a mix of register writes, RAM writes and pattern accesses that move the mapper away from power-up
            for (step, addr) in (0x6000u16..=0xFFFF).step_by(0x111).enumerate() {
                original.cpu_write(addr, (step * 7 + 3) as u8);
                original.ppu_read(((step * 0x91) & 0x1FFF) as u16);
                original.cpu_cycle();
            }
            let saved = original.save_state();

            let mut restored = create(file(id, 16, 16)).unwrap();
            restored.load_state(&saved).unwrap_or_else(|e| panic!("mapper {id}: {e}"));

            assert_eq!(fingerprint(&mut *restored), fingerprint(&mut *original), "mapper {id}");
            assert_eq!(restored.save_state(), saved, "mapper {id}: the state round-trips");
            assert!(restored.load_state(&saved[..saved.len() / 2]).is_err(), "mapper {id}: truncated state");
        }
    }

    #[test]
    fn chr_ram_contents_are_part_of_the_state_but_chr_rom_is_not() {
        let mut ram = create(file(2, 4, 0)).unwrap(); // UxROM with CHR-RAM
        ram.ppu_write(0x0123, 0x77);
        let state = ram.save_state();
        let mut restored = create(file(2, 4, 0)).unwrap();
        restored.load_state(&state).unwrap();
        assert_eq!(restored.ppu_read(0x0123), 0x77);

        let rom = create(file(3, 2, 2)).unwrap(); // CNROM with CHR-ROM
        assert!(rom.save_state().len() < 16, "the 16KB of CHR-ROM is not saved");
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
