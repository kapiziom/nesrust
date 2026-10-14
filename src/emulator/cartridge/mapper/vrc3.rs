//! Konami VRC3 (mapper 73): a 16KB PRG bank, CHR-RAM and a 16-bit CPU-cycle IRQ counter.
//! https://www.nesdev.org/wiki/VRC3

use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_16K: usize = 16 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vrc3 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
    prg_bank: usize,

    latch: u16,
    counter: u16,
    /// Count only the low byte (8-bit mode).
    byte_mode: bool,
    enabled: bool,
    enable_after_ack: bool,
    pending: bool,
}

impl Vrc3 {
    pub fn new(file: INesFile) -> Self {
        Vrc3 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_bank: 0,
            latch: 0,
            counter: 0,
            byte_mode: false,
            enabled: false,
            enable_after_ack: false,
            pending: false,
        }
    }
}

impl Mapper for Vrc3 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        let len = self.prg_rom.len();
        match addr {
            0x6000..=0x7FFF => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xBFFF => Some(self.prg_rom[bank_index(len, PRG_16K, self.prg_bank, addr as usize & 0x3FFF)]),
            0xC000..=0xFFFF => {
                let last = (len / PRG_16K).max(1) - 1;
                Some(self.prg_rom[bank_index(len, PRG_16K, last, addr as usize & 0x3FFF)])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        let nibble = (data & 0x0F) as u16;

        match addr & 0xF000 {
            0x6000 | 0x7000 => self.prg_ram.write(addr as usize - 0x6000, data),
            0x8000 => self.latch = (self.latch & 0xFFF0) | nibble,
            0x9000 => self.latch = (self.latch & 0xFF0F) | (nibble << 4),
            0xA000 => self.latch = (self.latch & 0xF0FF) | (nibble << 8),
            0xB000 => self.latch = (self.latch & 0x0FFF) | (nibble << 12),
            0xC000 => {
                self.enable_after_ack = data & 0x01 != 0;
                self.enabled = data & 0x02 != 0;
                self.byte_mode = data & 0x04 != 0;
                self.pending = false;
                if self.enabled {
                    self.counter = self.latch;
                }
            }
            0xD000 => {
                self.pending = false;
                self.enabled = self.enable_after_ack;
            }
            0xF000 => self.prg_bank = (data & 0x07) as usize,
            _ => {}
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

    fn irq_pending(&self) -> bool {
        self.pending
    }

    fn cpu_cycle(&mut self) {
        if !self.enabled {
            return;
        }

        if self.byte_mode {
            if self.counter & 0xFF == 0xFF {
                self.counter = (self.counter & 0xFF00) | (self.latch & 0x00FF);
                self.pending = true;
            } else {
                self.counter += 1;
            }
        } else if self.counter == 0xFFFF {
            self.counter = self.latch;
            self.pending = true;
        } else {
            self.counter += 1;
        }
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

    fn vrc3() -> Vrc3 {
        Vrc3::new(file(73, 8, 0))
    }

    fn set_latch(m: &mut Vrc3, value: u16) {
        for (base, shift) in [(0x8000u16, 0), (0x9000, 4), (0xA000, 8), (0xB000, 12)] {
            m.cpu_write(base, (value >> shift) as u8 & 0x0F);
        }
    }

    #[test]
    fn a_16kb_prg_bank_is_switched_at_8000_and_the_last_one_is_fixed() {
        let mut m = vrc3();
        m.cpu_write(0xF000, 3);

        assert_eq!(m.cpu_read(0x8000), Some(6));
        assert_eq!(m.cpu_read(0xC000), Some(14));
    }

    #[test]
    fn chr_is_ram() {
        let mut m = vrc3();
        m.ppu_write(0x0123, 0x55);
        assert_eq!(m.ppu_read(0x0123), 0x55);
    }

    #[test]
    fn the_16_bit_counter_reloads_from_the_latch_and_interrupts_when_it_wraps() {
        let mut m = vrc3();
        set_latch(&mut m, 0xFFFD);
        m.cpu_write(0xC000, 0b010); // enabled, 16-bit mode: counter = $FFFD

        m.cpu_cycle(); // FFFE
        m.cpu_cycle(); // FFFF
        assert!(!m.irq_pending());
        m.cpu_cycle(); // wrap
        assert!(m.irq_pending());
        assert_eq!(m.counter, 0xFFFD);

        m.cpu_write(0xD000, 0);
        assert!(!m.irq_pending());
    }

    #[test]
    fn the_8_bit_mode_wraps_only_the_low_byte() {
        let mut m = vrc3();
        set_latch(&mut m, 0x12FE);
        m.cpu_write(0xC000, 0b110);

        m.cpu_cycle(); // low byte FF
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());
        assert_eq!(m.counter, 0x12FE);
    }

    #[test]
    fn the_acknowledge_register_re_enables_when_the_a_bit_was_set() {
        let mut m = vrc3();
        set_latch(&mut m, 0xFFFF);
        m.cpu_write(0xC000, 0b011); // enabled, enable after ack
        m.cpu_cycle();
        assert!(m.irq_pending());

        m.cpu_write(0xD000, 0);
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());
    }
}
