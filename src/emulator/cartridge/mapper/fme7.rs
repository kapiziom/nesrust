//! Mapper 69 (Sunsoft FME-7 / 5A / 5B): a command/parameter register pair selects
//! 1KB CHR banks, 8KB PRG banks (also at $6000, optionally RAM), mirroring and a CPU-cycle IRQ counter.
//! The 5B variant also has expansion sound (see `sunsoft5b.rs`). https://www.nesdev.org/wiki/Sunsoft_FME-7

use super::sunsoft5b::Sunsoft5bAudio;
use super::{mapper_state, bank_index, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Fme7 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,

    command: u8,
    chr_banks: [u8; 8],
    /// Banks for $6000, $8000, $A000 and $C000; $E000 is fixed to the last bank.
    prg_banks: [u8; 4],

    irq_counter: u16,
    irq_enabled: bool,
    counter_enabled: bool,
    irq_pending: bool,

    audio: Sunsoft5bAudio,
}

impl Fme7 {
    pub fn new(file: INesFile) -> Self {
        Fme7 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            command: 0,
            chr_banks: [0; 8],
            prg_banks: [0; 4],
            irq_counter: 0,
            irq_enabled: false,
            counter_enabled: false,
            irq_pending: false,
            audio: Sunsoft5bAudio::default(),
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_banks[1] as usize & 0x3F,
            0xA000..=0xBFFF => self.prg_banks[2] as usize & 0x3F,
            0xC000..=0xDFFF => self.prg_banks[3] as usize & 0x3F,
            _ => (len / PRG_8K).max(1) - 1,
        };
        bank_index(len, PRG_8K, bank, addr as usize & 0x1FFF)
    }

    fn write_parameter(&mut self, data: u8) {
        match self.command {
            0..=7 => self.chr_banks[self.command as usize] = data,
            8..=11 => self.prg_banks[self.command as usize - 8] = data,
            12 => {
                self.mirroring = match data & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            13 => {
                self.irq_enabled = data & 0x01 != 0;
                self.counter_enabled = data & 0x80 != 0;
                self.irq_pending = false; // any write acknowledges
            }
            14 => self.irq_counter = (self.irq_counter & 0xFF00) | data as u16,
            15 => self.irq_counter = (self.irq_counter & 0x00FF) | (data as u16) << 8,
            _ => {}
        }
    }
}

impl Mapper for Fme7 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF => {
                let register = self.prg_banks[0];
                if register & 0x40 != 0 {
                    // RAM is selected: only visible when its enable bit is set
                    if register & 0x80 != 0 { self.prg_ram.read(addr as usize - 0x6000) } else { None }
                } else {
                    let bank = register as usize & 0x3F;
                    Some(self.prg_rom[bank_index(self.prg_rom.len(), PRG_8K, bank, addr as usize & 0x1FFF)])
                }
            }
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let register = self.prg_banks[0];
                if register & 0xC0 == 0xC0 {
                    self.prg_ram.write(addr as usize - 0x6000, data);
                }
            }
            0x8000..=0x9FFF => self.command = data & 0x0F,
            0xA000..=0xBFFF => self.write_parameter(data),
            0xC000..=0xDFFF => self.audio.select(data),
            0xE000..=0xFFFF => self.audio.write(data),
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
        self.audio.tick();

        if !self.counter_enabled {
            return;
        }

        // the counter counts down and raises the IRQ when it wraps from $0000 to $FFFF
        let (next, wrapped) = self.irq_counter.overflowing_sub(1);
        self.irq_counter = next;
        if wrapped && self.irq_enabled {
            self.irq_pending = true;
        }
    }

    fn audio_output(&self) -> f32 {
        self.audio.output()
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

    fn command(m: &mut Fme7, command: u8, parameter: u8) {
        m.cpu_write(0x8000, command);
        m.cpu_write(0xA000, parameter);
    }

    fn fme7() -> Fme7 {
        Fme7::new(file(69, 16, 16)) // 256KB PRG = 32 banks of 8KB, 128KB CHR = 128 banks of 1KB
    }

    #[test]
    fn chr_has_eight_independent_1kb_banks() {
        let mut m = fme7();
        for slot in 0..8u8 {
            command(&mut m, slot, 10 + slot);
        }

        for slot in 0..8u16 {
            assert_eq!(m.ppu_read(slot * 0x400), 10 + slot as u8);
        }
    }

    #[test]
    fn prg_has_three_switchable_banks_and_a_fixed_last_one() {
        let mut m = fme7();
        command(&mut m, 9, 3);
        command(&mut m, 10, 4);
        command(&mut m, 11, 5);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn the_6000_window_can_map_rom_or_ram() {
        let mut m = fme7();

        command(&mut m, 8, 7); // ROM bank 7
        assert_eq!(m.cpu_read(0x6000), Some(7));

        command(&mut m, 8, 0x40); // RAM selected but not enabled
        assert_eq!(m.cpu_read(0x6000), None);
        m.cpu_write(0x6000, 0x11);

        command(&mut m, 8, 0xC0); // RAM selected and enabled
        assert_eq!(m.cpu_read(0x6000), Some(0), "the write while disabled did not stick");
        m.cpu_write(0x6000, 0x22);
        assert_eq!(m.cpu_read(0x6000), Some(0x22));
    }

    #[test]
    fn mirroring_command() {
        let mut m = fme7();
        for (value, expected) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleScreenLower),
            (3, Mirroring::SingleScreenUpper),
        ] {
            command(&mut m, 12, value);
            assert_eq!(m.mirroring(), expected);
        }
    }

    #[test]
    fn the_irq_counter_counts_cpu_cycles_down_and_fires_on_wrap_around() {
        let mut m = fme7();
        command(&mut m, 14, 0x03); // counter = 3
        command(&mut m, 15, 0x00);
        command(&mut m, 13, 0x81); // counting and IRQ enabled

        for _ in 0..3 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending(), "the counter has reached 0 but not wrapped");

        m.cpu_cycle(); // $0000 -> $FFFF
        assert!(m.irq_pending());
    }

    #[test]
    fn writing_the_irq_control_register_acknowledges_the_interrupt() {
        let mut m = fme7();
        command(&mut m, 14, 0);
        command(&mut m, 15, 0);
        command(&mut m, 13, 0x81);
        m.cpu_cycle();
        assert!(m.irq_pending());

        command(&mut m, 13, 0x80);

        assert!(!m.irq_pending());
    }

    #[test]
    fn a_stopped_counter_or_disabled_irq_does_not_interrupt() {
        let mut stopped = fme7();
        command(&mut stopped, 13, 0x01); // IRQ enabled, counter stopped
        for _ in 0..70_000 {
            stopped.cpu_cycle();
        }
        assert!(!stopped.irq_pending());

        let mut disabled = fme7();
        command(&mut disabled, 13, 0x80); // counting, IRQ disabled
        for _ in 0..70_000 {
            disabled.cpu_cycle();
        }
        assert!(!disabled.irq_pending());
    }
}
