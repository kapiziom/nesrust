//! Konami VRC6 (mappers 24 and 26): 16KB + 8KB PRG banks, 1KB CHR banks,
//! the VRC IRQ counter and two pulse channels and a sawtooth channel of expansion audio. Mapper 26 is the same chip
//! with the register-select lines A0 and A1 swapped. https://www.nesdev.org/wiki/VRC6, https://www.nesdev.org/wiki/VRC6_audio

use super::vrc_irq::VrcIrq;
use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_16K: usize = 16 * 1024;
const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;
/// Scale of the summed channel levels (pulse 0-15 each, saw 0-31) to the APU mixer's range.
const AUDIO_SCALE: f32 = 0.0075;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Pulse {
    volume: u8,
    duty: u8,
    /// Digitized mode: the channel outputs its volume constantly.
    constant: bool,
    period: u16,
    enabled: bool,
    timer: u16,
    phase: u8,
}

impl Pulse {
    fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => {
                self.volume = data & 0x0F;
                self.duty = (data >> 4) & 7;
                self.constant = data & 0x80 != 0;
            }
            1 => self.period = (self.period & 0x0F00) | data as u16,
            _ => {
                self.period = (self.period & 0x00FF) | ((data as u16 & 0x0F) << 8);
                self.enabled = data & 0x80 != 0;
                if !self.enabled {
                    self.phase = 0;
                }
            }
        }
    }

    fn tick(&mut self, shift: u8) {
        if !self.enabled {
            return;
        }
        if self.timer == 0 {
            self.timer = self.period >> shift;
            self.phase = (self.phase + 1) & 15;
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.enabled && (self.constant || self.phase <= self.duty) {
            self.volume
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Saw {
    rate: u8,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
    accumulator: u8,
}

impl Saw {
    fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => self.rate = data & 0x3F,
            1 => self.period = (self.period & 0x0F00) | data as u16,
            _ => {
                self.period = (self.period & 0x00FF) | ((data as u16 & 0x0F) << 8);
                self.enabled = data & 0x80 != 0;
                if !self.enabled {
                    self.step = 0;
                    self.accumulator = 0;
                }
            }
        }
    }

    fn tick(&mut self, shift: u8) {
        if !self.enabled {
            return;
        }
        if self.timer == 0 {
            self.timer = self.period >> shift;
            // 14 steps; the accumulator grows on every even one and restarts with the cycle
            self.step = (self.step + 1) % 14;
            if self.step == 0 {
                self.accumulator = 0;
            } else if self.step % 2 == 0 {
                self.accumulator = self.accumulator.wrapping_add(self.rate);
            }
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.enabled {
            self.accumulator >> 3
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Vrc6Audio {
    pulse1: Pulse,
    pulse2: Pulse,
    saw: Saw,
    halted: bool,
    /// Frequency control: the timers' periods are divided by 1, 16 or 256 (stored as the shift).
    shift: u8,
}

impl Vrc6Audio {
    fn write_control(&mut self, data: u8) {
        self.halted = data & 1 != 0;
        self.shift = if data & 4 != 0 {
            8
        } else if data & 2 != 0 {
            4
        } else {
            0
        };
    }

    fn tick(&mut self) {
        if self.halted {
            return;
        }
        self.pulse1.tick(self.shift);
        self.pulse2.tick(self.shift);
        self.saw.tick(self.shift);
    }

    fn output(&self) -> f32 {
        (self.pulse1.output() + self.pulse2.output() + self.saw.output()) as f32 * AUDIO_SCALE
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vrc6 {
    /// Mapper 26 swaps the register-select address lines A0 and A1.
    swap_address_lines: bool,
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    mirroring: Mirroring,
    prg_16k_bank: usize,
    prg_8k_bank: usize,
    chr_banks: [usize; 8],
    ram_enabled: bool,
    irq: VrcIrq,
    audio: Vrc6Audio,
}

impl Vrc6 {
    pub fn new(file: INesFile) -> Self {
        Vrc6 {
            swap_address_lines: file.mapper == 26,
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            mirroring: file.mirroring,
            prg_16k_bank: 0,
            prg_8k_bank: 0,
            chr_banks: [0; 8],
            ram_enabled: false,
            irq: VrcIrq::default(),
            audio: Vrc6Audio::default(),
        }
    }

    /// The register (0-3) selected by A1/A0 of the address.
    fn register(&self, addr: u16) -> u16 {
        let register = addr & 3;
        if self.swap_address_lines {
            ((register & 1) << 1) | (register >> 1)
        } else {
            register
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let len = self.prg_rom.len();
        match addr {
            0x8000..=0xBFFF => bank_index(len, PRG_16K, self.prg_16k_bank, addr as usize & 0x3FFF),
            0xC000..=0xDFFF => bank_index(len, PRG_8K, self.prg_8k_bank, addr as usize & 0x1FFF),
            _ => bank_index(len, PRG_8K, (len / PRG_8K).max(1) - 1, addr as usize & 0x1FFF),
        }
    }
}

impl Mapper for Vrc6 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if self.ram_enabled => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        let register = self.register(addr);

        match addr & 0xF000 {
            0x6000 | 0x7000 if self.ram_enabled => self.prg_ram.write(addr as usize - 0x6000, data),
            0x8000 => self.prg_16k_bank = data as usize & 0x0F,
            0x9000 if register == 3 => self.audio.write_control(data),
            0x9000 => self.audio.pulse1.write(register, data),
            0xA000 => self.audio.pulse2.write(register, data),
            0xB000 if register == 3 => {
                self.ram_enabled = data & 0x80 != 0;
                self.mirroring = match (data >> 2) & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            0xB000 => self.audio.saw.write(register, data),
            0xC000 => self.prg_8k_bank = data as usize & 0x1F,
            0xD000 => self.chr_banks[register as usize] = data as usize,
            0xE000 => self.chr_banks[4 + register as usize] = data as usize,
            0xF000 => match register {
                0 => self.irq.write_latch(data),
                reg => self.irq.write(reg + 1, data),
            },
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
        self.audio.tick();
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

    fn vrc6() -> Vrc6 {
        Vrc6::new(file(24, 16, 16)) // 32 PRG banks of 8KB, 128 CHR banks of 1KB
    }

    fn run(m: &mut Vrc6, cycles: usize) -> Vec<f32> {
        (0..cycles)
            .map(|_| {
                m.cpu_cycle();
                m.audio_output()
            })
            .collect()
    }

    #[test]
    fn prg_has_a_16kb_bank_an_8kb_bank_and_a_fixed_last_bank() {
        let mut m = vrc6();
        m.cpu_write(0x8000, 3);
        m.cpu_write(0xC000, 9);

        assert_eq!(m.cpu_read(0x8000), Some(6), "16KB bank 3 = 8KB banks 6 and 7");
        assert_eq!(m.cpu_read(0xA000), Some(7));
        assert_eq!(m.cpu_read(0xC000), Some(9));
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn chr_has_eight_1kb_banks() {
        let mut m = vrc6();
        for slot in 0..4u16 {
            m.cpu_write(0xD000 + slot, 20 + slot as u8);
            m.cpu_write(0xE000 + slot, 40 + slot as u8);
        }

        for slot in 0..4u16 {
            assert_eq!(m.ppu_read(slot * 0x400), 20 + slot as u8);
            assert_eq!(m.ppu_read(0x1000 + slot * 0x400), 40 + slot as u8);
        }
    }

    #[test]
    fn mapper_26_swaps_the_register_select_lines() {
        let mut m = Vrc6::new(file(26, 16, 16));
        m.cpu_write(0xD001, 5); // register 2 on this board
        m.cpu_write(0xD002, 6); // register 1

        assert_eq!(m.ppu_read(0x0800), 5);
        assert_eq!(m.ppu_read(0x0400), 6);
    }

    #[test]
    fn b003_controls_mirroring_and_prg_ram() {
        let mut m = vrc6();
        m.cpu_write(0x6000, 1);
        assert_eq!(m.cpu_read(0x6000), None, "PRG-RAM starts disabled");

        m.cpu_write(0xB003, 0x80 | (1 << 2));
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), Some(0x42));

        m.cpu_write(0xB003, 0x80 | (2 << 2));
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);
        m.cpu_write(0xB003, 3 << 2);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
        assert_eq!(m.cpu_read(0x6000), None);
    }

    #[test]
    fn the_irq_latch_is_a_full_byte_and_control_and_acknowledge_follow() {
        let mut m = vrc6();
        m.cpu_write(0xF000, 0xFE); // latch
        m.cpu_write(0xF001, 0b110); // enabled, cycle mode: counter = $FE

        m.cpu_cycle(); // FF
        assert!(!m.irq_pending());
        m.cpu_cycle(); // wraps
        assert!(m.irq_pending());

        m.cpu_write(0xF002, 0);
        assert!(!m.irq_pending());
    }

    #[test]
    fn a_pulse_outputs_its_volume_for_duty_plus_one_sixteenths() {
        let mut m = vrc6();
        m.cpu_write(0x9000, (3 << 4) | 10); // duty 3, volume 10
        m.cpu_write(0x9001, 0); // period 0: one step per two cycles (timer 0 then reload)
        m.cpu_write(0x9002, 0x80);

        let samples = run(&mut m, 16 * 2);
        let high = samples.iter().filter(|&&s| s > 0.0).count();

        assert_eq!(high, 4 * 2, "4 of 16 steps are high (duty 3)");
        assert!((samples.iter().cloned().fold(0.0, f32::max) - 10.0 * AUDIO_SCALE).abs() < 1e-6);
    }

    #[test]
    fn digitized_mode_holds_the_volume_and_a_disabled_pulse_is_silent() {
        let mut m = vrc6();
        m.cpu_write(0xA000, 0x80 | 7);
        m.cpu_write(0xA002, 0x80);
        assert!(run(&mut m, 40).iter().all(|&s| (s - 7.0 * AUDIO_SCALE).abs() < 1e-6));

        m.cpu_write(0xA002, 0x00);
        assert_eq!(m.audio_output(), 0.0);
    }

    #[test]
    fn the_sawtooth_climbs_by_its_rate_every_other_step_and_restarts_after_14_steps() {
        let mut m = vrc6();
        m.cpu_write(0xB000, 8); // rate
        m.cpu_write(0xB001, 0);
        m.cpu_write(0xB002, 0x80); // enabled, period 0: a step every second cycle

        let levels: Vec<u8> = (0..14 * 2)
            .map(|_| {
                m.cpu_cycle();
                m.audio.saw.output()
            })
            .collect();
        let mut distinct: Vec<u8> = levels.clone();
        distinct.dedup();

        assert_eq!(
            &distinct[..8],
            &[0, 1, 2, 3, 4, 5, 6, 0],
            "6 climbs of rate/8 = 1, then back to 0 at the 14th step"
        );
    }

    #[test]
    fn frequency_control_scales_the_periods_and_halt_stops_everything() {
        let mut slow = vrc6();
        slow.cpu_write(0x9000, 0x0F);
        slow.cpu_write(0x9001, 0x20); // period 32
        slow.cpu_write(0x9002, 0x80);
        let normal = run(&mut slow, 600).windows(2).filter(|w| w[0] != w[1]).count();

        let mut fast = vrc6();
        fast.cpu_write(0x9000, 0x0F);
        fast.cpu_write(0x9001, 0x20);
        fast.cpu_write(0x9002, 0x80);
        fast.cpu_write(0x9003, 0x02); // periods / 16
        let faster = run(&mut fast, 600).windows(2).filter(|w| w[0] != w[1]).count();
        assert!(faster > normal * 4, "{faster} edges vs {normal}");

        fast.cpu_write(0x9003, 0x01); // halt
        let halted = run(&mut fast, 600);
        assert!(halted.windows(2).all(|w| w[0] == w[1]));
    }
}
