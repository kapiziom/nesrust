//! Mapper 5 (MMC5, ExROM): four PRG modes with RAM and ROM windows,
//! four CHR modes with separate bank sets for sprites and background, a scanline IRQ, a multiplier, 1KB of
//! extension RAM (extra nametable / extended attributes / plain RAM), per-nametable sources including a fill mode,
//! and two pulse channels plus a PCM channel of expansion audio.
//! Not emulated: the vertical split screen ($5200-$5202) and the PCM channel's read mode IRQ.
//! https://www.nesdev.org/wiki/MMC5

use super::{bank_index, mapper_state, Chr, Mapper, PpuPhase, PrgRam};
use crate::emulator::apu::pulse::Pulse;
use crate::emulator::cartridge::ines::INesFile;
use crate::emulator::cartridge::mirroring::Mirroring;

const PRG_8K: usize = 8 * 1024;
/// The sound hardware clocks its envelopes and length counters at 240 Hz.
const FRAME_CYCLES: u16 = 7457;
const PULSE_SCALE: f32 = 0.0085;
const PCM_SCALE: f32 = 0.0016;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Mmc5Audio {
    pulse1: Pulse,
    pulse2: Pulse,
    pcm: u8,
    frame_cycles: u16,
}

impl Mmc5Audio {
    fn new() -> Self {
        Mmc5Audio { pulse1: Pulse::new(true), pulse2: Pulse::new(false), pcm: 0, frame_cycles: 0 }
    }

    fn tick(&mut self) {
        self.frame_cycles += 1;
        if self.frame_cycles == FRAME_CYCLES {
            self.frame_cycles = 0;
            for pulse in [&mut self.pulse1, &mut self.pulse2] {
                pulse.clock_quarter_frame();
                pulse.clock_half_frame();
            }
        }

        self.pulse1.tick();
        self.pulse2.tick();
        self.pulse1.end_cycle();
        self.pulse2.end_cycle();
    }

    fn output(&self) -> f32 {
        (self.pulse1.output() as f32 + self.pulse2.output() as f32) * PULSE_SCALE + self.pcm as f32 * PCM_SCALE
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Mmc5 {
    #[serde(skip)]
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr: Chr,

    prg_mode: u8,
    /// $5113 (RAM at $6000) and $5114-$5117.
    prg_registers: [u8; 5],
    ram_protect: [u8; 2],

    chr_mode: u8,
    /// Bank set A ($5120-$5127) and B ($5128-$512B), including the upper bits of $5130.
    chr_a: [u16; 8],
    chr_b: [u16; 4],
    chr_upper: u16,
    /// The CPU's $2007 accesses use the set that was written last.
    last_written_set_b: bool,
    sprites_8x16: bool,
    phase: PpuPhase,

    exram_mode: u8,
    #[serde(with = "serde_big_array::BigArray")]
    exram: [u8; 1024],
    nametable_sources: u8,
    fill_tile: u8,
    fill_attribute: u8,
    /// The background tile whose nametable byte was fetched last (extended attribute mode looks it up in ExRAM).
    extended_tile: usize,

    irq_target: u8,
    irq_enabled: bool,
    irq_pending: bool,
    in_frame: bool,
    scanline: u8,

    multiplicand: u8,
    multiplier: u8,

    audio: Mmc5Audio,
}

impl Mmc5 {
    pub fn new(file: INesFile) -> Self {
        Mmc5 {
            prg_rom: file.prg_rom,
            prg_ram: PrgRam::new(file.prg_ram_size.max(8 * 1024), file.battery),
            chr: Chr::new(file.chr_rom, file.chr_ram_size),
            prg_mode: 3,
            // the last ROM bank must be mapped at $E000 for the reset vector
            prg_registers: [0, 0xFF, 0xFF, 0xFF, 0xFF],
            ram_protect: [0; 2],
            chr_mode: 0,
            chr_a: [0; 8],
            chr_b: [0; 4],
            chr_upper: 0,
            last_written_set_b: false,
            sprites_8x16: false,
            phase: PpuPhase::Idle,
            exram_mode: 0,
            exram: [0; 1024],
            nametable_sources: 0,
            fill_tile: 0,
            fill_attribute: 0,
            extended_tile: 0,
            irq_target: 0,
            irq_enabled: false,
            irq_pending: false,
            in_frame: false,
            scanline: 0,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            audio: Mmc5Audio::new(),
        }
    }

    /// Whether the PRG-RAM accepts writes ($5102 = 2 and $5103 = 1 unlock it).
    fn ram_writable(&self) -> bool {
        self.ram_protect[0] & 3 == 2 && self.ram_protect[1] & 3 == 1
    }

    /// The 8KB window of the CPU address, as (is ROM, 8KB bank number).
    fn prg_source(&self, addr: u16) -> (bool, usize) {
        let window = ((addr - 0x8000) >> 13) as usize; // 0-3 for $8000, $A000, $C000, $E000
        let register = |index: usize| self.prg_registers[index];
        let rom = |value: u8| value & 0x80 != 0;

        match (self.prg_mode, window) {
            (0, _) => (true, (register(4) & 0x7C) as usize + window),
            (1, 0 | 1) => (rom(register(2)), (register(2) & 0x7E) as usize + (window & 1)),
            (1, _) => (true, (register(4) & 0x7E) as usize + (window & 1)),
            (2, 0 | 1) => (rom(register(2)), (register(2) & 0x7E) as usize + (window & 1)),
            (2, 2) => (rom(register(3)), (register(3) & 0x7F) as usize),
            (2, _) => (true, (register(4) & 0x7F) as usize),
            (_, 3) => (true, (register(4) & 0x7F) as usize),
            (_, window) => {
                let value = register(1 + window);
                (rom(value), (value & 0x7F) as usize)
            }
        }
    }

    fn ram_offset(&self, bank: usize, addr: u16) -> usize {
        (bank & 0x0F) * PRG_8K + (addr as usize & 0x1FFF)
    }

    /// The CHR bank (in units of `size`) for a pattern address, and the size of the bank.
    fn chr_bank(&self, addr: u16) -> (usize, usize) {
        let use_b = match self.phase {
            PpuPhase::Idle => self.last_written_set_b,
            PpuPhase::Background => self.sprites_8x16,
            PpuPhase::Sprites => false,
        };
        let addr = addr as usize & 0x1FFF;

        if self.exram_mode == 1 && self.phase == PpuPhase::Background {
            // extended attribute mode: the tile's own 4KB bank
            let bank = (self.chr_upper as usize) << 6 | (self.exram[self.extended_tile] & 0x3F) as usize;
            return (bank, 4 * 1024);
        }

        match (self.chr_mode, use_b) {
            (0, false) => (self.chr_a[7] as usize, 8 * 1024),
            (0, true) => (self.chr_b[3] as usize, 8 * 1024),
            (1, false) => (self.chr_a[3 + 4 * (addr >> 12)] as usize, 4 * 1024),
            (1, true) => (self.chr_b[3] as usize, 4 * 1024),
            (2, false) => (self.chr_a[1 + 2 * (addr >> 11)] as usize, 2 * 1024),
            (2, true) => (self.chr_b[1 + 2 * ((addr >> 11) & 1)] as usize, 2 * 1024),
            (_, false) => (self.chr_a[addr >> 10] as usize, 1024),
            (_, true) => (self.chr_b[(addr >> 10) & 3] as usize, 1024),
        }
    }

    fn nametable_source(&self, addr: u16) -> u8 {
        (self.nametable_sources >> (((addr >> 10) & 3) * 2)) & 3
    }

    fn write_register(&mut self, addr: u16, data: u8) {
        match addr {
            0x5000..=0x5003 => self.audio.pulse1.write(addr - 0x5000, data),
            0x5004..=0x5007 => self.audio.pulse2.write(addr - 0x5004, data),
            0x5010 => {}
            0x5011 => {
                // a write of zero would be the IRQ trick of the read mode; the level is only changed otherwise
                if data != 0 {
                    self.audio.pcm = data;
                }
            }
            0x5015 => {
                self.audio.pulse1.length.set_enabled(data & 1 != 0);
                self.audio.pulse2.length.set_enabled(data & 2 != 0);
            }
            0x5100 => self.prg_mode = data & 3,
            0x5101 => self.chr_mode = data & 3,
            0x5102 => self.ram_protect[0] = data,
            0x5103 => self.ram_protect[1] = data,
            0x5104 => self.exram_mode = data & 3,
            0x5105 => self.nametable_sources = data,
            0x5106 => self.fill_tile = data,
            0x5107 => self.fill_attribute = data & 3,
            0x5113 => self.prg_registers[0] = data,
            0x5114..=0x5117 => self.prg_registers[(addr - 0x5113) as usize] = data,
            0x5120..=0x5127 => {
                self.chr_a[(addr - 0x5120) as usize] = data as u16 | self.chr_upper << 8;
                self.last_written_set_b = false;
            }
            0x5128..=0x512B => {
                self.chr_b[(addr - 0x5128) as usize] = data as u16 | self.chr_upper << 8;
                self.last_written_set_b = true;
            }
            0x5130 => self.chr_upper = (data & 3) as u16,
            0x5203 => self.irq_target = data,
            0x5204 => self.irq_enabled = data & 0x80 != 0,
            0x5205 => self.multiplicand = data,
            0x5206 => self.multiplier = data,
            0x5C00..=0x5FFF if self.exram_mode != 3 => self.exram[(addr - 0x5C00) as usize] = data,
            _ => {}
        }
    }
}

impl Mapper for Mmc5 {
    mapper_state!();

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x5015 => Some(self.audio.pulse1.length.active() as u8 | (self.audio.pulse2.length.active() as u8) << 1),
            0x5204 => {
                let status = (self.irq_pending as u8) << 7 | (self.in_frame as u8) << 6;
                self.irq_pending = false;
                Some(status)
            }
            0x5205 => Some((self.multiplicand as u16 * self.multiplier as u16) as u8),
            0x5206 => Some(((self.multiplicand as u16 * self.multiplier as u16) >> 8) as u8),
            0x5C00..=0x5FFF if self.exram_mode >= 2 => Some(self.exram[(addr - 0x5C00) as usize]),
            0x6000..=0x7FFF => {
                let offset = self.ram_offset(self.prg_registers[0] as usize, addr);
                self.prg_ram.read(offset)
            }
            0x8000..=0xFFFF => {
                let (is_rom, bank) = self.prg_source(addr);
                if is_rom {
                    Some(self.prg_rom[bank_index(self.prg_rom.len(), PRG_8K, bank, addr as usize & 0x1FFF)])
                } else {
                    self.prg_ram.read(self.ram_offset(bank, addr))
                }
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x5000..=0x5FFF => self.write_register(addr, data),
            0x6000..=0x7FFF if self.ram_writable() => {
                let offset = self.ram_offset(self.prg_registers[0] as usize, addr);
                self.prg_ram.write(offset, data);
            }
            0x8000..=0xDFFF if self.ram_writable() => {
                let (is_rom, bank) = self.prg_source(addr);
                if !is_rom {
                    let offset = self.ram_offset(bank, addr);
                    self.prg_ram.write(offset, data);
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let (bank, size) = self.chr_bank(addr);
        self.chr.read_banked(size, bank, addr as usize & (size - 1))
    }

    fn ppu_write(&mut self, addr: u16, data: u8) {
        let (bank, size) = self.chr_bank(addr);
        self.chr.write_banked(size, bank, addr as usize & (size - 1), data);
    }

    fn ppu_phase(&mut self, phase: PpuPhase) {
        self.phase = phase;
    }

    fn ppu_scanline_end(&mut self, _scanline: u16) {
        if !self.in_frame {
            self.in_frame = true;
            self.scanline = 0;
            self.irq_pending = false;
        } else {
            self.scanline = self.scanline.wrapping_add(1);
            if self.scanline == self.irq_target {
                self.irq_pending = true;
            }
        }
    }

    fn ppu_vblank(&mut self) {
        self.in_frame = false;
        self.scanline = 0;
    }

    fn ppu_register_write(&mut self, register: u16, data: u8) {
        match register {
            0 => self.sprites_8x16 = data & 0x20 != 0,
            1 if data & 0x18 == 0 => {
                // rendering switched off
                self.in_frame = false;
                self.phase = PpuPhase::Idle;
            }
            _ => {}
        }
    }

    fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        let offset = addr as usize & 0x3FF;
        let attribute_fetch = offset >= 0x3C0;

        if !attribute_fetch && self.phase == PpuPhase::Background {
            self.extended_tile = offset;
        }

        if self.exram_mode == 1 && attribute_fetch && self.phase == PpuPhase::Background {
            // extended attribute mode: every tile has its own palette in the top bits of its ExRAM byte
            return Some((self.exram[self.extended_tile] >> 6) * 0x55);
        }

        match self.nametable_source(addr) {
            0 | 1 => None,
            2 => Some(if self.exram_mode <= 1 { self.exram[offset] } else { 0 }),
            _ => Some(if attribute_fetch { self.fill_attribute * 0x55 } else { self.fill_tile }),
        }
    }

    fn nametable_write(&mut self, addr: u16, data: u8) -> bool {
        match self.nametable_source(addr) {
            0 | 1 => false,
            2 => {
                if self.exram_mode <= 1 {
                    self.exram[addr as usize & 0x3FF] = data;
                }
                true
            }
            _ => true,
        }
    }

    fn mirroring(&self) -> Mirroring {
        let page = |nametable: u8| ((self.nametable_sources >> (nametable * 2)) & 3 == 1) as u8;
        Mirroring::Custom([page(0), page(1), page(2), page(3)])
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending && self.irq_enabled
    }

    fn cpu_cycle(&mut self) {
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

    fn mmc5() -> Mmc5 {
        Mmc5::new(INesFile { prg_ram_size: 32 * 1024, ..file(5, 16, 16) }) // 32 PRG banks of 8KB, 128 CHR banks of 1KB
    }

    fn unlock_ram(m: &mut Mmc5) {
        m.cpu_write(0x5102, 2);
        m.cpu_write(0x5103, 1);
    }

    #[test]
    fn power_up_maps_the_last_bank_at_e000() {
        let mut m = mmc5();
        assert_eq!(m.cpu_read(0xE000), Some(31));
    }

    #[test]
    fn prg_mode_3_has_four_8kb_banks_and_mode_0_one_32kb_bank() {
        let mut m = mmc5();
        m.cpu_write(0x5100, 3);
        m.cpu_write(0x5114, 0x80 | 3);
        m.cpu_write(0x5115, 0x80 | 4);
        m.cpu_write(0x5116, 0x80 | 5);
        m.cpu_write(0x5117, 0x80 | 6);
        assert_eq!(
            [m.cpu_read(0x8000), m.cpu_read(0xA000), m.cpu_read(0xC000), m.cpu_read(0xE000)],
            [Some(3), Some(4), Some(5), Some(6)]
        );

        m.cpu_write(0x5100, 0);
        m.cpu_write(0x5117, 0x80 | 8);
        assert_eq!(m.cpu_read(0x8000), Some(8));
        assert_eq!(m.cpu_read(0xE000), Some(11));
    }

    #[test]
    fn prg_modes_1_and_2_use_16kb_and_8kb_windows() {
        let mut m = mmc5();
        m.cpu_write(0x5100, 1);
        m.cpu_write(0x5115, 0x80 | 6);
        m.cpu_write(0x5117, 0x80 | 10);
        assert_eq!([m.cpu_read(0x8000), m.cpu_read(0xA000)], [Some(6), Some(7)]);
        assert_eq!([m.cpu_read(0xC000), m.cpu_read(0xE000)], [Some(10), Some(11)]);

        m.cpu_write(0x5100, 2);
        m.cpu_write(0x5116, 0x80 | 20);
        m.cpu_write(0x5117, 0x80 | 21);
        assert_eq!([m.cpu_read(0x8000), m.cpu_read(0xA000)], [Some(6), Some(7)]);
        assert_eq!([m.cpu_read(0xC000), m.cpu_read(0xE000)], [Some(20), Some(21)]);
    }

    #[test]
    fn a_prg_window_without_the_rom_bit_maps_ram_and_needs_the_protect_keys_to_be_written() {
        let mut m = mmc5();
        m.cpu_write(0x5100, 3);
        m.cpu_write(0x5114, 0x01); // RAM bank 1 at $8000
        m.cpu_write(0x8000, 0x11);
        assert_eq!(m.cpu_read(0x8000), Some(0), "protected");

        unlock_ram(&mut m);
        m.cpu_write(0x8000, 0x22);
        assert_eq!(m.cpu_read(0x8000), Some(0x22));

        m.cpu_write(0x5113, 1); // $6000 shows the same bank
        assert_eq!(m.cpu_read(0x6000), Some(0x22));
        m.cpu_write(0x5113, 0);
        assert_eq!(m.cpu_read(0x6000), Some(0));
    }

    #[test]
    fn chr_modes_scale_the_bank_size() {
        let mut m = mmc5();
        m.cpu_write(0x5101, 3); // 1KB banks
        for slot in 0..8u16 {
            m.cpu_write(0x5120 + slot, 20 + slot as u8);
        }
        assert_eq!(m.ppu_read(0x0000), 20);
        assert_eq!(m.ppu_read(0x1C00), 27);

        m.cpu_write(0x5101, 2); // 2KB banks: $5121, $5123, $5125, $5127 (in 2KB units)
        m.cpu_write(0x5121, 3);
        m.cpu_write(0x5127, 5);
        assert_eq!(m.ppu_read(0x0000), 6);
        assert_eq!(m.ppu_read(0x0400), 7);
        assert_eq!(m.ppu_read(0x1800), 10);

        m.cpu_write(0x5101, 1); // 4KB banks: $5123 and $5127
        m.cpu_write(0x5123, 2);
        m.cpu_write(0x5127, 3);
        assert_eq!(m.ppu_read(0x0000), 8);
        assert_eq!(m.ppu_read(0x1000), 12);

        m.cpu_write(0x5101, 0); // 8KB: $5127
        m.cpu_write(0x5127, 1);
        assert_eq!(m.ppu_read(0x0000), 8);
    }

    #[test]
    fn with_8x16_sprites_the_background_uses_set_b_and_sprites_set_a() {
        let mut m = mmc5();
        m.cpu_write(0x5101, 3);
        for slot in 0..8u16 {
            m.cpu_write(0x5120 + slot, 20 + slot as u8);
        }
        for slot in 0..4u16 {
            m.cpu_write(0x5128 + slot, 60 + slot as u8);
        }
        m.ppu_register_write(0, 0x20); // 8x16 sprites

        m.ppu_phase(PpuPhase::Background);
        assert_eq!(m.ppu_read(0x0000), 60);
        assert_eq!(m.ppu_read(0x1400), 61, "set B repeats for the upper half");
        m.ppu_phase(PpuPhase::Sprites);
        assert_eq!(m.ppu_read(0x0000), 20);

        m.ppu_register_write(0, 0x00); // 8x8: set A everywhere
        m.ppu_phase(PpuPhase::Background);
        assert_eq!(m.ppu_read(0x0000), 20);

        m.ppu_phase(PpuPhase::Idle);
        m.cpu_write(0x5128, 61); // set B was written last: $2007 accesses use it
        assert_eq!(m.ppu_read(0x0000), 61);
        m.cpu_write(0x5120, 22);
        assert_eq!(m.ppu_read(0x0000), 22);
    }

    #[test]
    fn the_upper_chr_bits_extend_the_bank_numbers() {
        let mut m = Mmc5::new(file(5, 16, 64)); // 512 CHR banks of 1KB
        m.cpu_write(0x5101, 3);
        m.cpu_write(0x5130, 1);
        m.cpu_write(0x5120, 4);

        assert_eq!(m.ppu_read(0x0000), (260 % 256) as u8);
    }

    #[test]
    fn nametables_can_come_from_ciram_exram_or_the_fill_registers() {
        let mut m = mmc5();
        // NT0 = CIRAM 0, NT1 = CIRAM 1, NT2 = ExRAM, NT3 = fill
        m.cpu_write(0x5105, 0b11_10_01_00);
        m.cpu_write(0x5106, 0x42);
        m.cpu_write(0x5107, 0b10);

        assert_eq!(m.mirroring(), Mirroring::Custom([0, 1, 0, 0]));
        assert_eq!(m.nametable_read(0x2005), None);

        assert!(m.nametable_write(0x2805, 0x99));
        assert_eq!(m.nametable_read(0x2805), Some(0x99));
        assert_eq!(m.cpu_read(0x5C05), None, "ExRAM mode 0 is not readable by the CPU");

        assert_eq!(m.nametable_read(0x2C05), Some(0x42));
        assert_eq!(m.nametable_read(0x2FC5), Some(0b10_10_10_10));
        assert!(m.nametable_write(0x2C05, 1), "fill mode ignores writes");
    }

    #[test]
    fn exram_is_plain_cpu_ram_in_mode_2_and_read_only_in_mode_3() {
        let mut m = mmc5();
        m.cpu_write(0x5104, 2);
        m.cpu_write(0x5C10, 0x77);
        assert_eq!(m.cpu_read(0x5C10), Some(0x77));

        m.cpu_write(0x5104, 3);
        m.cpu_write(0x5C10, 0x11);
        assert_eq!(m.cpu_read(0x5C10), Some(0x77));
    }

    #[test]
    fn extended_attribute_mode_takes_palette_and_chr_bank_from_exram() {
        let mut m = Mmc5::new(file(5, 16, 64));
        m.cpu_write(0x5104, 1);
        m.cpu_write(0x5C00 + 7, 0b10_000011); // tile 7: palette 2, 4KB CHR bank 3
        m.ppu_phase(PpuPhase::Background);

        m.nametable_read(0x2007); // the tile fetch
        assert_eq!(m.nametable_read(0x23C0), Some(0b10_10_10_10), "the attribute fetch gets tile 7's palette");
        assert_eq!(m.ppu_read(0x0000), 12, "4KB bank 3 starts at 1KB unit 12");
        assert_eq!(m.ppu_read(0x1000), 12, "both halves show the tile's bank");
    }

    #[test]
    fn the_scanline_irq_fires_when_the_counter_reaches_the_target() {
        let mut m = mmc5();
        m.cpu_write(0x5203, 3);
        m.cpu_write(0x5204, 0x80);

        m.ppu_scanline_end(261); // first line of the frame: counter = 0
        for line in 0..2 {
            m.ppu_scanline_end(line);
            assert!(!m.irq_pending(), "line {line}");
        }
        m.ppu_scanline_end(2); // counter = 3
        assert!(m.irq_pending());

        assert_eq!(m.cpu_read(0x5204), Some(0x80 | 0x40), "pending and in frame");
        assert!(!m.irq_pending(), "reading acknowledges");

        m.ppu_vblank();
        assert_eq!(m.cpu_read(0x5204), Some(0));
    }

    #[test]
    fn a_disabled_irq_sets_the_flag_but_not_the_line() {
        let mut m = mmc5();
        m.cpu_write(0x5203, 1);
        m.ppu_scanline_end(261);
        m.ppu_scanline_end(0);

        assert!(!m.irq_pending());
        assert_eq!(m.cpu_read(0x5204).unwrap() & 0x80, 0x80);
    }

    #[test]
    fn switching_rendering_off_ends_the_frame() {
        let mut m = mmc5();
        m.ppu_scanline_end(261);
        m.ppu_register_write(1, 0x00);

        assert_eq!(m.cpu_read(0x5204).unwrap() & 0x40, 0);
    }

    #[test]
    fn the_hardware_multiplier() {
        let mut m = mmc5();
        m.cpu_write(0x5205, 200);
        m.cpu_write(0x5206, 100);

        assert_eq!(m.cpu_read(0x5205), Some((20_000u16 & 0xFF) as u8));
        assert_eq!(m.cpu_read(0x5206), Some((20_000u16 >> 8) as u8));
    }

    #[test]
    fn the_pcm_channel_and_a_pulse_channel_make_sound() {
        let mut m = mmc5();
        m.cpu_write(0x5011, 100);
        assert!((m.audio_output() - 100.0 * PCM_SCALE).abs() < 1e-6);

        m.cpu_write(0x5015, 0x01);
        m.cpu_write(0x5000, 0b1011_1111); // 50% duty, constant volume 15
        m.cpu_write(0x5002, 0x20);
        m.cpu_write(0x5003, 0x08);
        let mut peak = 0.0f32;
        for _ in 0..2000 {
            m.cpu_cycle();
            peak = peak.max(m.audio_output());
        }
        assert!(peak > 100.0 * PCM_SCALE + 10.0 * PULSE_SCALE, "{peak}");
        assert_eq!(m.cpu_read(0x5015), Some(1), "the pulse's length counter is running");
    }
}
