//! Jaleco SS 88006 (mapper 18): 8KB PRG and 1KB CHR banks written one nibble at a
//! time, a variable-width CPU-cycle IRQ countdown and mirroring control. https://www.nesdev.org/wiki/INES_Mapper_018

use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct JalecoSs88006 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,

    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    ram_enabled: bool,
    ram_writable: bool,

    irq_latch: u16,
    irq_counter: u16,
    /// Which bits of the counter take part in counting: 16, 12, 8 or 4.
    irq_mask: u16,
    irq_enabled: bool,
    irq_pending: bool,
}

impl JalecoSs88006 {
    pub fn new(file: INesFile) -> Self {
        JalecoSs88006 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            ram_enabled: false,
            ram_writable: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_mask: 0xFFFF,
            irq_enabled: false,
            irq_pending: false,
        }
    }

    /// Writes one nibble of a register that is split over two addresses (low nibble first).
    fn set_nibble(value: &mut u8, high: bool, data: u8) {
        *value = if high { (*value & 0x0F) | (data << 4) } else { (*value & 0xF0) | (data & 0x0F) };
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_banks[0] as usize,
            0xA000..=0xBFFF => self.prg_banks[1] as usize,
            0xC000..=0xDFFF => self.prg_banks[2] as usize,
            _ => (len / PRG_8K).max(1) - 1,
        };
        bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)
    }
}

impl Mapper for JalecoSs88006 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if self.ram_enabled => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        let high_nibble = addr & 1 != 0;
        let pair = ((addr >> 1) & 1) as usize;

        if (0x6000..0x8000).contains(&addr) {
            if self.ram_enabled && self.ram_writable {
                self.prg_ram.write(addr as usize - 0x6000, data);
            }
            return;
        }

        match addr & 0xF003 {
            0x8000..=0x8003 => Self::set_nibble(&mut self.prg_banks[pair], high_nibble, data),
            0x9000 | 0x9001 => Self::set_nibble(&mut self.prg_banks[2], high_nibble, data),
            0x9002 => {
                self.ram_enabled = data & 1 != 0;
                self.ram_writable = data & 2 != 0;
            }
            0xA000..=0xD003 => {
                let index = (((addr >> 12) - 0xA) * 2) as usize + pair;
                Self::set_nibble(&mut self.chr_banks[index], high_nibble, data);
            }
            0xE000..=0xE003 => {
                let shift = (addr & 3) * 4;
                self.irq_latch = (self.irq_latch & !(0xF << shift)) | ((data as u16 & 0x0F) << shift);
            }
            0xF000 => {
                self.irq_counter = self.irq_latch;
                self.irq_pending = false;
            }
            0xF001 => {
                self.irq_enabled = data & 1 != 0;
                self.irq_mask = if data & 8 != 0 {
                    0x000F
                } else if data & 4 != 0 {
                    0x00FF
                } else if data & 2 != 0 {
                    0x0FFF
                } else {
                    0xFFFF
                };
                self.irq_pending = false;
            }
            0xF002 => {
                self.mirroring = match data & 3 {
                    0 => Mirroring::Horizontal,
                    1 => Mirroring::Vertical,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let bank = self.chr_banks[(addr as usize >> 10) & 7] as usize;
        self.chr.read_banked(CHR_1K, bank, addr as usize & 0x3FF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let bank = self.chr_banks[(addr as usize >> 10) & 7] as usize;
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

        // only the masked low bits count; the others keep their value
        let kept = self.irq_counter & !self.irq_mask;
        let counting = (self.irq_counter & self.irq_mask).wrapping_sub(1) & self.irq_mask;
        self.irq_counter = kept | counting;
        if counting == 0 {
            self.irq_pending = true;
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

    fn jaleco() -> JalecoSs88006 {
        JalecoSs88006::new(file(18, 16, 16)) // 32 PRG banks of 8KB, 128 CHR banks of 1KB
    }

    #[test]
    fn prg_banks_are_written_a_nibble_at_a_time() {
        let mut m = jaleco();
        m.cpu_write(0x8000, 0x05);
        m.cpu_write(0x8001, 0x01); // bank $15 = 21
        m.cpu_write(0x8002, 0x07);
        m.cpu_write(0x9000, 0x09);

        assert_eq!(m.cpu_read(0x8000), Some(21));
        assert_eq!(m.cpu_read(0xA000), Some(7));
        assert_eq!(m.cpu_read(0xC000), Some(9));
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn chr_banks_are_written_a_nibble_at_a_time_in_eight_registers() {
        let mut m = jaleco();
        for slot in 0..8u16 {
            let base = 0xA000 + (slot / 2) * 0x1000;
            let low = base + (slot & 1) * 2;
            m.cpu_write(low, (slot as u8) & 0x0F);
            m.cpu_write(low + 1, 0x01);
        }

        for slot in 0..8u16 {
            assert_eq!(m.ppu_read(slot * 0x400), 16 + slot as u8, "slot {slot}");
        }
    }

    #[test]
    fn prg_ram_needs_enable_and_write_enable_bits() {
        let mut m = jaleco();
        m.cpu_write(0x6000, 1);
        assert_eq!(m.cpu_read(0x6000), None);

        m.cpu_write(0x9002, 0x01); // readable only
        m.cpu_write(0x6000, 0x11);
        assert_eq!(m.cpu_read(0x6000), Some(0));
        m.cpu_write(0x9002, 0x03);
        m.cpu_write(0x6000, 0x22);
        assert_eq!(m.cpu_read(0x6000), Some(0x22));
    }

    #[test]
    fn mirroring_register() {
        let mut m = jaleco();
        for (value, expected) in [
            (0, Mirroring::Horizontal),
            (1, Mirroring::Vertical),
            (2, Mirroring::SingleScreenLower),
            (3, Mirroring::SingleScreenUpper),
        ] {
            m.cpu_write(0xF002, value);
            assert_eq!(m.mirroring(), expected);
        }
    }

    #[test]
    fn the_irq_counts_down_cpu_cycles_and_fires_at_zero() {
        let mut m = jaleco();
        for (i, nibble) in [3u8, 0, 0, 0].iter().enumerate() {
            m.cpu_write(0xE000 + i as u16, *nibble); // latch = 3
        }
        m.cpu_write(0xF000, 0); // reload
        m.cpu_write(0xF001, 0x01); // enable, 16 bits

        for _ in 0..2 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());

        m.cpu_write(0xF000, 0);
        assert!(!m.irq_pending());
    }

    #[test]
    fn a_narrow_counter_only_counts_its_low_bits() {
        let mut m = jaleco();
        m.cpu_write(0xE000, 0x02);
        m.cpu_write(0xE001, 0x01); // latch $0012
        m.cpu_write(0xF000, 0);
        m.cpu_write(0xF001, 0x01 | 0x04); // 8-bit counter

        for _ in 0..0x11 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());
        assert_eq!(m.irq_counter, 0x0000);
    }
}
