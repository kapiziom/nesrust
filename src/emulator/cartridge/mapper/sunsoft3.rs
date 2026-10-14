//! Sunsoft 3 (mapper 67): four 2KB CHR banks, a 16KB PRG bank and a 16-bit CPU-cycle IRQ countdown.
//! https://www.nesdev.org/wiki/INES_Mapper_067

use super::{bank_index, mapper_state, Chr, Mapper};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_16K: usize = 16 * 1024;
const CHR_2K: usize = 2 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Sunsoft3 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    chr: Chr,
    mirroring: Mirroring,
    prg_bank: usize,
    chr_banks: [usize; 4],

    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    /// The IRQ counter is loaded with two writes to the same register: high byte first.
    irq_second_write: bool,
}

impl Sunsoft3 {
    pub fn new(file: INesFile) -> Self {
        Sunsoft3 {
            prg_rom: file.prg_rom,
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_bank: 0,
            chr_banks: [0; 4],
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_second_write: false,
        }
    }
}

impl Mapper for Sunsoft3 {
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
        match addr & 0xF800 {
            0x8800 => self.chr_banks[0] = data as usize,
            0x9800 => self.chr_banks[1] = data as usize,
            0xA800 => self.chr_banks[2] = data as usize,
            0xB800 => self.chr_banks[3] = data as usize,
            0xC800 => {
                self.irq_counter = if self.irq_second_write {
                    (self.irq_counter & 0xFF00) | data as u16
                } else {
                    (self.irq_counter & 0x00FF) | ((data as u16) << 8)
                };
                self.irq_second_write = !self.irq_second_write;
            }
            0xD800 => {
                self.irq_second_write = false;
                self.irq_enabled = data & 0x10 != 0;
                self.irq_pending = false;
            }
            0xE800 => {
                self.mirroring = match data & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            0xF800 => self.prg_bank = data as usize,
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let bank = self.chr_banks[(addr as usize >> 11) & 3];
        self.chr.read_banked(CHR_2K, bank, addr as usize & 0x7FF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let bank = self.chr_banks[(addr as usize >> 11) & 3];
        self.chr.write_banked(CHR_2K, bank, addr as usize & 0x7FF, data);
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

        let (next, wrapped) = self.irq_counter.overflowing_sub(1);
        self.irq_counter = next;
        if wrapped {
            self.irq_pending = true;
            self.irq_enabled = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::mapper::test_util::file;

    fn sunsoft3() -> Sunsoft3 {
        Sunsoft3::new(file(67, 8, 8)) // 8 PRG banks of 16KB, 64 CHR banks of 2KB... 64 of 1KB
    }

    #[test]
    fn four_2kb_chr_banks_and_a_16kb_prg_bank() {
        let mut m = sunsoft3();
        m.cpu_write(0x8800, 1);
        m.cpu_write(0x9800, 2);
        m.cpu_write(0xA800, 3);
        m.cpu_write(0xB800, 4);
        m.cpu_write(0xF800, 3);

        assert_eq!(m.ppu_read(0x0000), 2);
        assert_eq!(m.ppu_read(0x0800), 4);
        assert_eq!(m.ppu_read(0x1000), 6);
        assert_eq!(m.ppu_read(0x1800), 8);
        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.cpu_read(0xC000), Some(14));
    }

    #[test]
    fn mirroring_register() {
        let mut m = sunsoft3();
        for (value, expected) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleScreenLower),
            (3, Mirroring::SingleScreenUpper),
        ] {
            m.cpu_write(0xE800, value);
            assert_eq!(m.mirroring(), expected);
        }
    }

    #[test]
    fn the_irq_counter_takes_two_writes_and_fires_when_it_wraps_once() {
        let mut m = sunsoft3();
        m.cpu_write(0xC800, 0x00); // high
        m.cpu_write(0xC800, 0x02); // low: counter = 2
        m.cpu_write(0xD800, 0x10);

        m.cpu_cycle(); // 1
        m.cpu_cycle(); // 0
        assert!(!m.irq_pending());
        m.cpu_cycle(); // wraps
        assert!(m.irq_pending());

        m.cpu_write(0xD800, 0x10);
        assert!(!m.irq_pending());
        for _ in 0..10 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending(), "it only fires again after another 65536 cycles");
    }

    #[test]
    fn writing_the_enable_register_resets_the_write_toggle() {
        let mut m = sunsoft3();
        m.cpu_write(0xC800, 0x12); // high
        m.cpu_write(0xD800, 0x00); // resets the toggle
        m.cpu_write(0xC800, 0x34); // high again

        assert_eq!(m.irq_counter, 0x3400);
    }
}
