//! Namco 163 (mapper 19): 8KB PRG and 1KB CHR banks, nametables that
//! can come from CHR-ROM, a 15-bit CPU-cycle IRQ counter and up to eight channels of wavetable sound.
//! https://www.nesdev.org/wiki/Namco_163, https://www.nesdev.org/wiki/Namco_163_audio

use super::{bank_index, mapper_state, Chr, Mapper, PrgRam};
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
const CHR_1K: usize = 1024;
/// The sound chip works on one channel every 15 CPU cycles.
const CYCLES_PER_CHANNEL: u8 = 15;
/// Scale of a channel's signed sample times volume (at most +-120) to the APU mixer's range.
const AUDIO_SCALE: f32 = 0.0012;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Namco163 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,
    /// The 2KB of nametable RAM the board brings along (it also serves as CHR for banks $E0-$FF).
    ciram: Vec<u8>,

    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    nametable_banks: [u8; 4],
    /// Bit 6 / bit 7 of $E800: the lower / upper pattern table uses CHR-ROM even for banks $E0-$FF.
    chr_rom_only: [bool; 2],

    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,

    #[serde(with = "serde_big_array::BigArray")]
    sound_ram: [u8; 128],
    sound_address: u8,
    sound_auto_increment: bool,
    sound_disabled: bool,
    divider: u8,
    /// Which of the active channels is updated next (0 = the first of them).
    next_channel: u8,
    channel_output: [i16; 8],
    output: f32,
}

impl Namco163 {
    pub fn new(file: INesFile) -> Self {
        Namco163 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(8 * 1024, file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            ciram: vec![0; 2048],
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            nametable_banks: [0; 4],
            chr_rom_only: [false; 2],
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            sound_ram: [0; 128],
            sound_address: 0,
            sound_auto_increment: false,
            sound_disabled: false,
            divider: 0,
            next_channel: 0,
            channel_output: [0; 8],
            output: 0.0,
        }
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

    fn step_sound_address(&mut self) {
        if self.sound_auto_increment {
            self.sound_address = (self.sound_address + 1) & 0x7F;
        }
    }

    fn channel_count(&self) -> u8 {
        ((self.sound_ram[0x7F] >> 4) & 7) + 1
    }

    /// Advances the oscillator of the next active channel (the chip time-multiplexes them) and refreshes the mix.
    fn update_channel(&mut self) {
        let channels = self.channel_count();
        let channel = (8 - channels + self.next_channel) as usize;
        self.next_channel = (self.next_channel + 1) % channels;

        let base = 0x40 + channel * 8;
        let ram = &mut self.sound_ram;
        let frequency = ram[base] as u32 | (ram[base + 2] as u32) << 8 | (ram[base + 4] as u32 & 3) << 16;
        let phase = ram[base + 1] as u32 | (ram[base + 3] as u32) << 8 | (ram[base + 5] as u32) << 16;
        let length = 256 - (ram[base + 4] as u32 & 0xFC);
        let start = ram[base + 6] as u32;
        let volume = (ram[base + 7] & 0x0F) as i32;

        let phase = (phase + frequency) % (length << 16);
        ram[base + 1] = phase as u8;
        ram[base + 3] = (phase >> 8) as u8;
        ram[base + 5] = (phase >> 16) as u8;

        let nibble = (((phase >> 16) + start) & 0xFF) as usize;
        let byte = ram[nibble >> 1];
        let sample = if nibble & 1 != 0 { byte >> 4 } else { byte & 0x0F } as i32;
        self.channel_output[channel] = ((sample - 8) * volume) as i16;

        // every channel gets a turn, so a channel is heard for 1/n of the time
        let first = 8 - channels as usize;
        let sum: i32 = self.channel_output[first..].iter().map(|&level| level as i32).sum();
        self.output = sum as f32 / channels as f32 * AUDIO_SCALE;
    }

    /// CHR byte behind a pattern table address, honouring the banks that map the nametable RAM.
    fn chr_byte(&self, addr: u16) -> u8 {
        let slot = (addr as usize >> 10) & 7;
        let bank = self.chr_banks[slot];
        let offset = addr as usize & 0x3FF;

        if bank >= 0xE0 && !self.chr_rom_only[slot / 4] {
            self.ciram[(bank as usize & 1) * 0x400 + offset]
        } else {
            self.chr.read_banked(CHR_1K, bank as usize, offset)
        }
    }
}

impl Mapper for Namco163 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x4800..=0x4FFF => {
                let value = self.sound_ram[self.sound_address as usize];
                self.step_sound_address();
                Some(value)
            }
            0x5000..=0x57FF => Some(self.irq_counter as u8),
            0x5800..=0x5FFF => Some((self.irq_counter >> 8) as u8 | (self.irq_enabled as u8) << 7),
            0x6000..=0x7FFF => self.prg_ram.read(addr as usize - 0x6000),
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x4800..=0x4FFF => {
                self.sound_ram[self.sound_address as usize] = data;
                self.step_sound_address();
            }
            0x5000..=0x57FF => {
                self.irq_counter = (self.irq_counter & 0x7F00) | data as u16;
                self.irq_pending = false;
            }
            0x5800..=0x5FFF => {
                self.irq_counter = (self.irq_counter & 0x00FF) | ((data as u16 & 0x7F) << 8);
                self.irq_enabled = data & 0x80 != 0;
                self.irq_pending = false;
            }
            0x6000..=0x7FFF => self.prg_ram.write(addr as usize - 0x6000, data),
            0x8000..=0xBFFF => self.chr_banks[((addr - 0x8000) >> 11) as usize] = data,
            0xC000..=0xDFFF => self.nametable_banks[((addr - 0xC000) >> 11) as usize & 3] = data,
            0xE000..=0xE7FF => {
                self.prg_banks[0] = data & 0x3F;
                self.sound_disabled = data & 0x40 != 0;
            }
            0xE800..=0xEFFF => {
                self.prg_banks[1] = data & 0x3F;
                self.chr_rom_only = [data & 0x40 != 0, data & 0x80 != 0];
            }
            0xF000..=0xF7FF => self.prg_banks[2] = data & 0x3F,
            0xF800..=0xFFFF => {
                self.sound_address = data & 0x7F;
                self.sound_auto_increment = data & 0x80 != 0;
            }
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr_byte(addr)
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let slot = (addr as usize >> 10) & 7;
        let bank = self.chr_banks[slot];
        let offset = addr as usize & 0x3FF;

        if bank >= 0xE0 && !self.chr_rom_only[slot / 4] {
            self.ciram[(bank as usize & 1) * 0x400 + offset] = data;
        } else {
            self.chr.write_banked(CHR_1K, bank as usize, offset, data);
        }
    }

    fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        let bank = self.nametable_banks[(addr as usize >> 10) & 3];
        let offset = addr as usize & 0x3FF;

        Some(if bank >= 0xE0 {
            self.ciram[(bank as usize & 1) * 0x400 + offset]
        } else {
            self.chr.read_banked(CHR_1K, bank as usize, offset)
        })
    }

    fn nametable_write(&mut self, addr: u16, data: u8) -> bool {
        let bank = self.nametable_banks[(addr as usize >> 10) & 3];
        if bank >= 0xE0 {
            self.ciram[(bank as usize & 1) * 0x400 + (addr as usize & 0x3FF)] = data;
        }
        true
    }

    fn mirroring(&self) -> Mirroring {
        // not used: all nametable accesses are answered by `nametable_read` / `nametable_write`
        Mirroring::Horizontal
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if self.irq_enabled && self.irq_counter < 0x7FFF {
            self.irq_counter += 1;
            if self.irq_counter == 0x7FFF {
                self.irq_pending = true;
            }
        }

        if !self.sound_disabled {
            self.divider += 1;
            if self.divider == CYCLES_PER_CHANNEL {
                self.divider = 0;
                self.update_channel();
            }
        }
    }

    fn audio_output(&self) -> f32 {
        if self.sound_disabled {
            0.0
        } else {
            self.output
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

    fn namco() -> Namco163 {
        Namco163::new(file(19, 16, 16)) // 32 PRG banks of 8KB, 128 CHR banks of 1KB
    }

    fn write_sound(m: &mut Namco163, address: u8, bytes: &[u8]) {
        m.cpu_write(0xF800, 0x80 | address); // auto-increment
        for &byte in bytes {
            m.cpu_write(0x4800, byte);
        }
    }

    #[test]
    fn prg_banks_and_the_fixed_last_bank() {
        let mut m = namco();
        m.cpu_write(0xE000, 3);
        m.cpu_write(0xE800, 4);
        m.cpu_write(0xF000, 5);

        assert_eq!(m.cpu_read(0x8000), Some(3));
        assert_eq!(m.cpu_read(0xA000), Some(4));
        assert_eq!(m.cpu_read(0xC000), Some(5));
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn chr_banks_come_from_rom_below_e0_and_from_nametable_ram_above() {
        let mut m = namco();
        m.cpu_write(0x8000, 7);
        m.cpu_write(0x8800, 0xE1);
        assert_eq!(m.ppu_read(0x0000), 7);

        m.ppu_write(0x0400, 0x5A);
        assert_eq!(m.ppu_read(0x0400), 0x5A, "bank $E1 is page 1 of the board's RAM");
        m.cpu_write(0x8800, 0xE0);
        assert_eq!(m.ppu_read(0x0400), 0, "page 0 is a different page");
        m.cpu_write(0x8800, 0xE1);

        m.cpu_write(0xE800, 0x40); // lower pattern table: CHR-ROM only
        assert_eq!(m.ppu_read(0x0400), 0xE1 % 128, "ROM wraps: bank $E1 of a 128KB chip");
    }

    #[test]
    fn nametables_can_be_read_from_chr_rom_or_use_the_ram_pages() {
        let mut m = namco();
        m.cpu_write(0xC000, 9); // NT0 from CHR-ROM bank 9
        m.cpu_write(0xC800, 0xE0); // NT1 = RAM page 0
        m.cpu_write(0xD000, 0xE1); // NT2 = RAM page 1
        m.cpu_write(0xD800, 0xE1); // NT3 = RAM page 1

        assert_eq!(m.nametable_read(0x2005), Some(9));
        assert!(m.nametable_write(0x2005, 0xFF), "the write is swallowed by the ROM nametable");
        assert_eq!(m.nametable_read(0x2005), Some(9));

        assert!(m.nametable_write(0x2405, 0x11));
        assert!(m.nametable_write(0x2805, 0x22));
        assert_eq!(m.nametable_read(0x2405), Some(0x11));
        assert_eq!(m.nametable_read(0x2805), Some(0x22));
        assert_eq!(m.nametable_read(0x2C05), Some(0x22), "NT3 shares page 1 with NT2");
    }

    #[test]
    fn the_irq_counter_counts_up_to_7fff_and_stops() {
        let mut m = namco();
        m.cpu_write(0x5000, 0xFD);
        m.cpu_write(0x5800, 0x80 | 0x7F); // $7FFD, enabled

        m.cpu_cycle();
        assert!(!m.irq_pending());
        m.cpu_cycle();
        assert!(m.irq_pending());
        assert_eq!(m.cpu_read(0x5000), Some(0xFF));
        assert_eq!(m.cpu_read(0x5800), Some(0xFF));

        m.cpu_write(0x5000, 0);
        assert!(!m.irq_pending(), "writing acknowledges");
    }

    #[test]
    fn the_sound_ram_is_reached_through_an_auto_incrementing_port() {
        let mut m = namco();
        write_sound(&mut m, 0x10, &[1, 2, 3]);

        m.cpu_write(0xF800, 0x80 | 0x10);
        assert_eq!(m.cpu_read(0x4800), Some(1));
        assert_eq!(m.cpu_read(0x4800), Some(2));
        m.cpu_write(0xF800, 0x11); // no auto-increment
        assert_eq!(m.cpu_read(0x4800), Some(2));
        assert_eq!(m.cpu_read(0x4800), Some(2));
    }

    /// Channel 7 (the only active one) plays a 16-sample wave that is high in its first half and low in the second.
    fn play_square(volume: u8) -> Namco163 {
        let mut m = namco();
        // waveform: 8 nibbles of 15 then 8 nibbles of 0 = 8 bytes at $00-$07 (low nibble first)
        write_sound(&mut m, 0x00, &[0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00]);
        // channel 7 at $78-$7F: frequency $10000 (one sample per update), length 16 (252 -> 256 - 240), address 0
        write_sound(&mut m, 0x78, &[0x00, 0x00, 0x00, 0x00, 0x01 | (240 & 0xFC), 0x00, 0x00, volume]);
        m
    }

    #[test]
    fn a_wavetable_channel_swings_between_positive_and_negative_levels() {
        let mut m = play_square(0x0F);
        let mut levels = Vec::new();
        for _ in 0..(15 * 40) {
            m.cpu_cycle();
            levels.push(m.audio_output());
        }

        let high = levels.iter().cloned().fold(f32::MIN, f32::max);
        let low = levels.iter().cloned().fold(f32::MAX, f32::min);
        assert!((high - 7.0 * 15.0 * AUDIO_SCALE).abs() < 1e-6, "{high}");
        assert!((low + 8.0 * 15.0 * AUDIO_SCALE).abs() < 1e-6, "{low}");
    }

    #[test]
    fn a_silent_channel_or_disabled_sound_outputs_nothing() {
        let mut quiet = play_square(0x00);
        for _ in 0..(15 * 40) {
            quiet.cpu_cycle();
        }
        assert_eq!(quiet.audio_output(), 0.0);

        let mut off = play_square(0x0F);
        off.cpu_write(0xE000, 0x40); // disable the sound
        for _ in 0..(15 * 40) {
            off.cpu_cycle();
        }
        assert_eq!(off.audio_output(), 0.0);
    }

    #[test]
    fn several_channels_share_the_output_so_each_one_is_quieter() {
        let mut one = play_square(0x0F);
        let mut two = play_square(0x0F);
        // two channels: channel 6 gets the same setup as channel 7
        write_sound(&mut two, 0x70, &[0x00, 0x00, 0x00, 0x00, 0x01 | (240 & 0xFC), 0x00, 0x00, 0x0F]);
        write_sound(&mut two, 0x7F, &[0x10 | 0x0F]);

        let peak = |m: &mut Namco163| {
            (0..(15 * 80))
                .map(|_| {
                    m.cpu_cycle();
                    m.audio_output()
                })
                .fold(f32::MIN, f32::max)
        };

        let single = peak(&mut one);
        let double = peak(&mut two);
        assert!(
            double < single * 1.01,
            "two identical channels average to the same peak at most: {double} vs {single}"
        );
    }
}
