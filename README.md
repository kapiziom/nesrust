# nesrust: NES Emulator in Rust 🎮

**NESRust** is a Nintendo Entertainment System (NES) emulator written in Rust, focusing on modularity, accuracy and code clarity.

## 🚀 Features Implemented

### CPU
- **6502 CPU** (official and the stable unofficial opcodes), cycle-accurate: every bus access, including dummy reads/writes, is one CPU cycle.
- NMI, IRQ, BRK and RESET with correct stack/flag behaviour.
- Passes **nestest** instruction by instruction, including cycle counts (`cargo test --test nestest`).

### Cartridge
- **iNES and NES 2.0** parsing (malformed files return errors, never panic).
- Mappers: **0 (NROM), 1 (MMC1), 2 (UxROM), 3 (CNROM), 4 (MMC3), 7 (AxROM)**.
- PRG-RAM, CHR-RAM, battery-backed RAM access for saves, mapper-controlled mirroring and IRQ.

### Bus
- A **bus** connecting the CPU, RAM, PPU and cartridge, with open-bus behaviour for unmapped addresses.

### Input
- Two standard controllers (`$4016`/`$4017`): strobe/latch/shift register, open-bus upper bits; `Nes::set_buttons(port, Buttons)`.

### APU
- All five channels (2 pulse, triangle, noise, DMC), frame counter with IRQ, nonlinear mixer and output filters.
  Passes Blargg's `apu_test` and `blargg_apu_2005` suites and the DMC DMA tests.

### Regions
- **NTSC and PAL** timing, chosen from the cartridge header (60.1 / 50.0 frames per second).

### PPU
- Dot-accurate **2C02**: registers, scrolling, background and sprites (8x8/8x16, sprite 0 hit, overflow), NMI/VBL timing,
  RGBA framebuffer. Passes Blargg's `ppu_vbl_nmi` and `sprite_hit_tests`; 110 of 121 judged Blargg ROMs pass in total
  (`cargo test --test blargg -- --nocapture`).
- OAM DMA, `Nes::run_frame()` / `framebuffer()` as the entry point for a frontend.

## 🛠️ To-Do Features

- More mappers, save states, frontend

## 📂 Project Structure

- **`src/emulator/cpu/`**: MOS 6502 core (`operation_codes.rs` opcode table, `addressing.rs`, `instructions.rs`, `interrupts.rs`).
- **`src/emulator/bus/`**: `CpuBus` trait, the real `Bus`, and `MockBus` for tests.
- **`src/emulator/cartridge/`**: iNES parser and the `Mapper` trait with its implementations.
- **`src/emulator/ppu/`, `apu/`, `ram/`, `nes.rs`**: PPU, APU (empty), 2KB RAM, the `Nes` console facade.
- **`tests/`**: integration tests (nestest, Blargg ROMs, whole-console programs, cartridge loading); reference ROMs live in `test_rom/`.
- **`examples/viewer.rs`**: a minimal window to try ROMs by hand (see below).

## 🎮 Trying a ROM

```
cargo run --release --example viewer -- path/to/game.nes
```

Arrows = D-pad, X = A, Z = B, Enter = Start, Right Shift / Backspace = Select, P = pause, R = reset,
Tab (hold) = 4x speed, F12 = screenshot (PNG), Esc = quit. Battery RAM is stored in `<rom>.sav`. There is sound; the audio buffer paces the emulation.
For a headless screenshot: `... -- game.nes --screenshot out.png --frames 120 --press start`.

## 🧪 Tests

```
cargo test
```

### Test ROMs

`test_rom/nestest.nes` (with its reference log) is included in the repository. The much larger Blargg suite is **not**:
download it once to run the PPU/CPU/mapper ROM tests (without it `tests/blargg.rs` does nothing).

- Source: <https://github.com/christopherpow/nes-test-roms> (Blargg's tests, documented on
  <https://www.nesdev.org/wiki/Emulator_tests>)
- Copy these directories into `test_rom/blargg/`:

```
git clone --depth 1 https://github.com/christopherpow/nes-test-roms.git ../nes-test-roms
mkdir -p test_rom/blargg
for d in blargg_ppu_tests_2005.09.15b branch_timing_tests cpu_dummy_reads cpu_dummy_writes cpu_interrupts_v2          cpu_timing_test6 instr_misc instr_test-v5 instr_timing mmc3_test mmc3_test_2 oam_read oam_stress          ppu_open_bus ppu_read_buffer ppu_vbl_nmi sprite_hit_tests_2005.10.05 sprite_overflow_tests \
         apu_test blargg_apu_2005.07.30 apu_reset cpu_reset dmc_dma_during_read4 sprdma_and_dmc_dma vbl_nmi_timing; do
  cp -r ../nes-test-roms/$d test_rom/blargg/
done
```

Then `cargo test --test blargg -- --nocapture` lists every ROM with PASS/FAIL (`BLARGG_FILTER=ppu_vbl_nmi` runs a subset).
The ROMs that currently pass are listed in `MUST_PASS` in `tests/blargg.rs`.
