//! Bandai FCG-1/2 and LZ93D50 (mapper 16) and the LZ93D50 with a
//! 24C01 EEPROM (mapper 159): 1KB CHR banks, a 16KB PRG bank, mirroring and a CPU-cycle IRQ
//! countdown. The serial EEPROM that stores the saves is not emulated.
//! https://www.nesdev.org/wiki/INES_Mapper_016, https://www.nesdev.org/wiki/INES_Mapper_159

use super::{bank_index, mapper_state, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_16K: usize = 16 * 1024;
const CHR_1K: usize = 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct BandaiFcg {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    prg_bank: usize,
    chr_banks: [usize; 8],

    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
}

impl BandaiFcg {
    pub fn new(file: INesFile) -> Self {
        BandaiFcg {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_bank: 0,
            chr_banks: [0; 8],
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
        }
    }
}

impl Mapper for BandaiFcg {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        let len = self.prg_rom.len();
        match addr {
            0x8000..=0xBFFF => Some(self.prg_rom[bank_index(len, PRG_16K, self.prg_bank, addr as usize & 0x3FFF)]),
            0xC000..=0xFFFF => {
                let last = (len / PRG_16K).max(1) - 1;
                Some(self.prg_rom[bank_index(len, PRG_16K, last, addr as usize & 0x3FFF)])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        // the FCG-1/2 board decodes $6000-$7FFF, the LZ93D50 board $8000-$FFFF; both are accepted
        if addr < 0x6000 {
            return;
        }

        match addr & 0x000F {
            register @ 0..=7 => self.chr_banks[register as usize] = data as usize,
            8 => self.prg_bank = (data & 0x0F) as usize,
            9 => {
                self.mirroring = match data & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            0xA => {
                self.irq_pending = false;
                self.irq_enabled = data & 1 != 0;
                self.irq_counter = self.irq_latch;
            }
            0xB => self.irq_latch = (self.irq_latch & 0xFF00) | data as u16,
            0xC => self.irq_latch = (self.irq_latch & 0x00FF) | ((data as u16) << 8),
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let bank = self.chr_banks[(addr as usize >> 10) & 7];
        self.chr.read_banked(CHR_1K, bank, addr as usize & 0x3FF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let bank = self.chr_banks[(addr as usize >> 10) & 7];
        self.chr.write_banked(CHR_1K, bank, addr as usize & 0x3FF, data);
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }

        self.irq_counter = self.irq_counter.wrapping_sub(1);
        if self.irq_counter == 0 {
            self.irq_pending = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    fn bandai() -> BandaiFcg {
        BandaiFcg::new(file(16, 8, 8))
    }

    #[test]
    fn registers_work_at_6000_and_at_8000() {
        for base in [0x6000u16, 0x8000] {
            let mut m = bandai();
            m.cpu_write(base + 8, 3);
            m.cpu_write(base + 2, 40);

            assert_eq!(m.cpu_read(0x8000), Some(6), "base {base:#06X}");
            assert_eq!(m.cpu_read(0xC000), Some(14));
            assert_eq!(m.ppu_read(0x0800), 40);
        }
    }

    #[test]
    fn mirroring_register() {
        let mut m = bandai();
        for (value, expected) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleScreenLower),
            (3, Mirroring::SingleScreenUpper),
        ] {
            m.cpu_write(0x8009, value);
            assert_eq!(m.mirroring(), expected);
        }
    }

    #[test]
    fn the_irq_counts_cpu_cycles_down_from_the_latch() {
        let mut m = bandai();
        m.cpu_write(0x800B, 3);
        m.cpu_write(0x800C, 0);
        m.cpu_write(0x800A, 1); // load the counter and enable

        m.cpu_cycle();
        m.cpu_cycle();
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());

        m.cpu_write(0x800A, 0);
        assert!(!m.irq_pending());
        for _ in 0..10 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending(), "disabled");
    }
}
