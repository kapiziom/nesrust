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

## 🛠️ To-Do Features

- **PPU**: registers, rendering, scrolling, sprites (currently a stub)
- **APU**: sound
- **Controllers**
- More mappers, save states, frontend

## 📂 Project Structure

- **`src/emulator/cpu/`**: MOS 6502 core (`operation_codes.rs` opcode table, `addressing.rs`, `instructions.rs`, `interrupts.rs`).
- **`src/emulator/bus/`**: `CpuBus` trait, the real `Bus`, and `MockBus` for tests.
- **`src/emulator/cartridge/`**: iNES parser and the `Mapper` trait with its implementations.
- **`src/emulator/ppu/`, `apu/`, `ram/`**: PPU (stub), APU (empty), 2KB RAM.
- **`tests/`**: integration tests (nestest, cartridge loading); reference ROMs live in `test_rom/`.
- **`src/main.rs`**: throw-away runner (`cargo run -- <rom.nes>`).

## 🧪 Tests

```
cargo test
```
