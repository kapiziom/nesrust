//! Konami VRC2 and VRC4 (mappers 21, 22, 23, 25): 8KB PRG banks, 1KB CHR banks and (VRC4) a scanline/cycle IRQ.
//! The four mapper numbers are the same chip wired with its two register-select lines connected to different CPU
//! address lines. https://www.nesdev.org/wiki/VRC2_and_VRC4

use super::{mapper_state, bank_index, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;
/// PPU dots per scanline: the IRQ prescaler counts CPU cycles x 3 against this.
const PRESCALER_RELOAD: i16 = 341;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vrc2_4 {
    mapper: u16,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,

    prg_banks: [u8; 2],
    /// 9-bit CHR bank numbers, assembled from a low nibble and a high part.
    chr_banks: [u16; 8],
    swap_prg: bool,
    ram_enabled: bool,

    irq_latch: u8,
    irq_counter: u8,
    irq_cycle_mode: bool,
    irq_enabled: bool,
    irq_enable_after_ack: bool,
    irq_pending: bool,
    prescaler: i16,
}

impl Vrc2_4 {
    pub fn new(file: INesFile) -> Self {
        Vrc2_4 {
            mapper: file.mapper,
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_banks: [0; 2],
            chr_banks: [0; 8],
            swap_prg: false,
            ram_enabled: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_cycle_mode: false,
            irq_enabled: false,
            irq_enable_after_ack: false,
            irq_pending: false,
            prescaler: PRESCALER_RELOAD,
        }
    }

    /// The register (0-3) a write address selects, derived from the two address lines this board uses for it.
    /// Variants of one mapper number are wired differently, so all of their lines are combined.
    fn register(&self, addr: u16) -> u16 {
        let line = |bit: u32| (addr >> bit) & 1;
        let (low, high) = match self.mapper {
            21 => (line(1) | line(6), line(2) | line(7)),
            22 => (line(1), line(0)),
            23 => (line(0) | line(2), line(1) | line(3)),
            _ => (line(1) | line(3), line(0) | line(2)), // 25
        };
        low | (high << 1)
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let banks = (len / PRG_8K).max(1);
        let second_last = banks.saturating_sub(2);
        let bank = match (addr, self.swap_prg) {
            (0x8000..=0x9FFF, false) => self.prg_banks[0] as usize,
            (0x8000..=0x9FFF, true) => second_last,
            (0xA000..=0xBFFF, _) => self.prg_banks[1] as usize,
            (0xC000..=0xDFFF, false) => second_last,
            (0xC000..=0xDFFF, true) => self.prg_banks[0] as usize,
            _ => banks - 1,
        };
        bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)
    }

    fn chr_bank(&self, addr: u16) -> usize {
        let bank = self.chr_banks[(addr as usize >> 10) & 7];
        // VRC2a (mapper 22) wires the CHR bank register one bit lower
        if self.mapper == 22 { (bank >> 1) as usize } else { bank as usize }
    }

    fn write_chr_register(&mut self, addr: u16, data: u8) {
        let register = self.register(addr);
        let pair = ((addr >> 12) as usize - 0xB) * 2 + (register as usize >> 1);
        let bank = &mut self.chr_banks[pair];

        if register & 1 == 0 {
            *bank = (*bank & 0x1F0) | (data as u16 & 0x0F);
        } else {
            *bank = (*bank & 0x00F) | ((data as u16 & 0x1F) << 4);
        }
    }

    fn write_irq_register(&mut self, register: u16, data: u8) {
        match register {
            0 => self.irq_latch = (self.irq_latch & 0xF0) | (data & 0x0F),
            1 => self.irq_latch = (self.irq_latch & 0x0F) | (data << 4),
            2 => {
                self.irq_enable_after_ack = data & 0x01 != 0;
                self.irq_enabled = data & 0x02 != 0;
                self.irq_cycle_mode = data & 0x04 != 0;
                self.irq_pending = false;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                    self.prescaler = PRESCALER_RELOAD;
                }
            }
            _ => {
                self.irq_pending = false;
                self.irq_enabled = self.irq_enable_after_ack;
            }
        }
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0xFF {
            self.irq_counter = self.irq_latch;
            self.irq_pending = true;
        } else {
            self.irq_counter += 1;
        }
    }
}

impl Mapper for Vrc2_4 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if self.ram_enabled || self.mapper == 22 || self.mapper == 23 => {
                self.prg_ram.read(addr as usize - 0x6000)
            }
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF if self.ram_enabled || self.mapper == 22 || self.mapper == 23 => {
                self.prg_ram.write(addr as usize - 0x6000, data)
            }
            0x8000..=0x8FFF => self.prg_banks[0] = data & 0x1F,
            0xA000..=0xAFFF => self.prg_banks[1] = data & 0x1F,
            0x9000..=0x9FFF => match self.register(addr) {
                0 | 1 => {
                    self.mirroring = match data & 3 {
                        0 => Mirroring::Vertical,
                        1 => Mirroring::Horizontal,
                        2 => Mirroring::SingleScreenLower,
                        _ => Mirroring::SingleScreenUpper,
                    };
                }
                2 => {
                    self.ram_enabled = data & 0x01 != 0;
                    self.swap_prg = data & 0x02 != 0;
                }
                _ => {}
            },
            0xB000..=0xEFFF => self.write_chr_register(addr, data),
            0xF000..=0xFFFF => {
                let register = self.register(addr);
                self.write_irq_register(register, data);
            }
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr.read_banked(CHR_1K, self.chr_bank(addr), addr as usize & 0x3FF)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        self.chr.write_banked(CHR_1K, self.chr_bank(addr), addr as usize & 0x3FF, data);
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

        if self.irq_cycle_mode {
            self.clock_irq_counter();
        } else {
            // scanline mode: one count per 341 PPU dots = 113.67 CPU cycles
            self.prescaler -= 3;
            if self.prescaler <= 0 {
                self.prescaler += PRESCALER_RELOAD;
                self.clock_irq_counter();
            }
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

    fn board(mapper: u16) -> Vrc2_4 {
        Vrc2_4::new(file(mapper, 16, 32)) // 32 PRG banks of 8KB, 256 CHR banks of 1KB
    }

    /// Address of register `register` (0-3) inside the 4KB region starting at `base`, for each wiring.
    fn at(mapper: u16, base: u16, register: u16) -> u16 {
        let (low, high) = (register & 1, register >> 1);
        base + match mapper {
            21 => (low << 1) | (high << 2),
            22 => (low << 1) | high,
            23 => low | (high << 1),
            _ => (low << 1) | high, // 25: A1 selects the low bit, A0 the high bit
        }
    }

    #[test]
    fn prg_banks_and_the_swap_mode() {
        let mut m = board(23);
        m.cpu_write(0x8000, 3);
        m.cpu_write(0xA000, 4);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(30), "second-to-last");
        assert_eq!(m.cpu_read(0xE000), Some(31));

        m.cpu_write(at(23, 0x9000, 2), 0x02); // swap mode
        assert_eq!(m.cpu_read(0x8000), Some(30));
        assert_eq!(m.cpu_read(0xC000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
    }

    #[test]
    fn chr_banks_are_assembled_from_low_and_high_registers_for_every_wiring() {
        for mapper in [21u16, 23, 25] {
            let mut m = board(mapper);

            // CHR bank 5 (second register pair, first bank of it): value $0A7 = high 0x0A, low 0x7
            m.cpu_write(at(mapper, 0xC000, 0), 0x07);
            m.cpu_write(at(mapper, 0xC000, 1), 0x0A);
            // and bank 3 (second bank of the $B000 pair)
            m.cpu_write(at(mapper, 0xB000, 2), 0x02);
            m.cpu_write(at(mapper, 0xB000, 3), 0x01);

            assert_eq!(m.ppu_read(0x0800), (0x0A7u16 % 256) as u8, "mapper {mapper}: CHR bank $0A7 wraps to 167");
            assert_eq!(m.ppu_read(0x0400), 0x12, "mapper {mapper}: bank $12 is 1KB unit 18");
        }
    }

    #[test]
    fn vrc2a_shifts_the_chr_bank_number_right_by_one() {
        let mut m = board(22);
        m.cpu_write(at(22, 0xB000, 0), 0x08); // register value 8 -> bank 4

        assert_eq!(m.ppu_read(0x0000), 4);
    }

    #[test]
    fn mirroring_register_of_the_vrc4() {
        let mut m = board(25);
        for (value, expected) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleScreenLower),
            (3, Mirroring::SingleScreenUpper),
        ] {
            m.cpu_write(at(25, 0x9000, 0), value);
            assert_eq!(m.mirroring(), expected);
        }
    }

    #[test]
    fn prg_ram_on_the_vrc4_needs_enabling() {
        let mut m = board(21);
        m.cpu_write(0x6000, 1);
        assert_eq!(m.cpu_read(0x6000), None);

        m.cpu_write(at(21, 0x9000, 2), 0x01);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), Some(0x42));
    }

    #[test]
    fn irq_in_cycle_mode_counts_every_cpu_cycle_and_reloads_from_the_latch() {
        let mut m = board(23);
        m.cpu_write(at(23, 0xF000, 0), 0x0D); // latch low nibble
        m.cpu_write(at(23, 0xF000, 1), 0x0F); // latch high nibble: latch = $FD
        m.cpu_write(at(23, 0xF000, 2), 0b110); // enabled, cycle mode: counter = $FD

        m.cpu_cycle(); // FE
        m.cpu_cycle(); // FF
        assert!(!m.irq_pending());
        m.cpu_cycle(); // wraps: reload $FD and fire
        assert!(m.irq_pending());
        assert_eq!(m.irq_counter, 0xFD);
    }

    #[test]
    fn irq_in_scanline_mode_counts_once_per_113_67_cycles() {
        let mut m = board(21);
        m.cpu_write(at(21, 0xF000, 0), 0x0E);
        m.cpu_write(at(21, 0xF000, 1), 0x0F); // latch = $FE: two counts until the IRQ
        m.cpu_write(at(21, 0xF000, 2), 0b010); // enabled, scanline mode

        let mut cycles_until_irq = 0;
        while !m.irq_pending() && cycles_until_irq < 1000 {
            m.cpu_cycle();
            cycles_until_irq += 1;
        }

        // 341 dots per count, 3 dots per cycle: 114 + 114 cycles for two counts (the prescaler rounds up)
        assert!((225..=230).contains(&cycles_until_irq), "took {cycles_until_irq} cycles");
    }

    #[test]
    fn the_irq_is_acknowledged_by_the_ack_register_and_can_re_enable_itself() {
        let mut m = board(23);
        m.cpu_write(at(23, 0xF000, 0), 0x0F);
        m.cpu_write(at(23, 0xF000, 1), 0x0F); // latch $FF
        m.cpu_write(at(23, 0xF000, 2), 0b111); // enabled, cycle mode, enable after ack
        m.cpu_cycle();
        assert!(m.irq_pending());

        m.cpu_write(at(23, 0xF000, 3), 0);

        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending(), "the A bit kept the IRQ enabled after the acknowledge");
    }

    #[test]
    fn a_disabled_irq_never_fires() {
        let mut m = board(25);
        m.cpu_write(at(25, 0xF000, 2), 0b000);
        for _ in 0..1000 {
            m.cpu_cycle();
        }

        assert!(!m.irq_pending());
    }
}
