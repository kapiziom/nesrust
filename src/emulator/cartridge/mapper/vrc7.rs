//! Konami VRC7 (mapper 85): three 8KB PRG banks, eight 1KB CHR banks, the VRC
//! IRQ counter and six channels of FM sound. The two board versions wire the register-select line to A4 (VRC7a) or
//! A3 (VRC7b); both are accepted. https://www.nesdev.org/wiki/VRC7

use super::vrc7_fm::Opll;
use super::vrc_irq::VrcIrq;
use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vrc7 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
    prg_banks: [usize; 3],
    chr_banks: [usize; 8],
    ram_enabled: bool,
    audio_muted: bool,
    irq: VrcIrq,
    fm: Opll,
}

impl Vrc7 {
    pub fn new(file: INesFile) -> Self {
        Vrc7 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            ram_enabled: false,
            audio_muted: false,
            irq: VrcIrq::default(),
            fm: Opll::default(),
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_banks[0],
            0xA000..=0xBFFF => self.prg_banks[1],
            0xC000..=0xDFFF => self.prg_banks[2],
            _ => (len / PRG_8K).max(1) - 1,
        };
        bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)
    }
}

impl Mapper for Vrc7 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if self.ram_enabled => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        // the second register of each pair sits on A4 (VRC7a) or A3 (VRC7b)
        let second = addr & 0x18 != 0;

        match addr & 0xF000 {
            0x6000 | 0x7000 if self.ram_enabled => self.prg_ram.write(addr as usize - 0x6000, data),
            0x8000 if !second => self.prg_banks[0] = (data & 0x3F) as usize,
            0x8000 => self.prg_banks[1] = (data & 0x3F) as usize,
            0x9000 if addr & 0x20 != 0 => self.fm.write(data),
            0x9000 if second => self.fm.select(data),
            0x9000 => self.prg_banks[2] = (data & 0x3F) as usize,
            0xA000 => self.chr_banks[second as usize] = data as usize,
            0xB000 => self.chr_banks[2 + second as usize] = data as usize,
            0xC000 => self.chr_banks[4 + second as usize] = data as usize,
            0xD000 => self.chr_banks[6 + second as usize] = data as usize,
            0xE000 if !second => {
                self.mirroring = match data & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
                self.audio_muted = data & 0x40 != 0;
                self.ram_enabled = data & 0x80 != 0;
            }
            0xE000 => self.irq.write_latch(data),
            0xF000 if !second => self.irq.write(2, data),
            0xF000 => self.irq.write(3, data),
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
        self.irq.pending()
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
        self.fm.tick();
    }

    fn audio_output(&self) -> f32 {
        if self.audio_muted {
            0.0
        } else {
            self.fm.output()
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

    fn vrc7() -> Vrc7 {
        Vrc7::new(file(85, 16, 16)) // 32 PRG banks of 8KB, 128 CHR banks of 1KB
    }

    #[test]
    fn prg_banks_and_the_fixed_last_bank() {
        let mut m = vrc7();
        m.cpu_write(0x8000, 3);
        m.cpu_write(0x8010, 4);
        m.cpu_write(0x9000, 5);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn both_register_wirings_work() {
        for second in [0x10u16, 0x08] {
            let mut m = vrc7();
            m.cpu_write(0x8000 + second, 7);
            m.cpu_write(0xA000 + second, 50);
            m.cpu_write(0xB000, 51);

            assert_eq!(m.cpu_read(0xA000), Some(7), "second register at {second:#X}");
            assert_eq!(m.ppu_read(0x0400), 50);
            assert_eq!(m.ppu_read(0x0800), 51);
        }
    }

    #[test]
    fn chr_has_eight_1kb_banks() {
        let mut m = vrc7();
        for (slot, base) in [0xA000u16, 0xB000, 0xC000, 0xD000].into_iter().enumerate() {
            m.cpu_write(base, 20 + 2 * slot as u8);
            m.cpu_write(base + 0x10, 21 + 2 * slot as u8);
        }

        for slot in 0..8u16 {
            assert_eq!(m.ppu_read(slot * 0x400), 20 + slot as u8);
        }
    }

    #[test]
    fn e000_controls_mirroring_ram_and_the_audio_mute() {
        let mut m = vrc7();
        m.cpu_write(0x6000, 1);
        assert_eq!(m.cpu_read(0x6000), None);

        m.cpu_write(0xE000, 0x80 | 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), Some(0x42));
        m.cpu_write(0xE000, 3);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
        assert_eq!(m.cpu_read(0x6000), None);
    }

    #[test]
    fn the_irq_uses_a_full_byte_latch() {
        let mut m = vrc7();
        m.cpu_write(0xE010, 0xFE);
        m.cpu_write(0xF000, 0b110);

        m.cpu_cycle();
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());
        m.cpu_write(0xF010, 0);
        assert!(!m.irq_pending());
    }

    /// Plays a note on FM channel 0 through the CPU-visible registers.
    fn play_a_note(m: &mut Vrc7) {
        for (register, value) in [(0x10u8, 34u8), (0x30, 0x60), (0x20, 0x10 | (4 << 1) | 1)] {
            m.cpu_write(0x9010, register);
            m.cpu_write(0x9030, value);
        }
    }

    #[test]
    fn the_fm_chip_is_reached_through_9010_and_9030() {
        let mut m = vrc7();
        play_a_note(&mut m);

        let mut peak = 0.0f32;
        for _ in 0..(36 * 3000) {
            m.cpu_cycle();
            peak = peak.max(m.audio_output().abs());
        }

        assert!(peak > 0.01, "{peak}");
    }

    #[test]
    fn the_mute_bit_silences_the_chip() {
        let mut m = vrc7();
        play_a_note(&mut m);
        m.cpu_write(0xE000, 0x40);

        for _ in 0..(36 * 3000) {
            m.cpu_cycle();
            assert_eq!(m.audio_output(), 0.0);
        }
    }
}
