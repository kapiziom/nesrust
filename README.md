# nesrust: NES Emulator in Rust 🎮

**NESRust** is a work-in-progress Nintendo Entertainment System (NES) emulator written in Rust, with a focus on **modularity, code clarity, and learning low-level system design**.

## 🚀 Features Implemented

### CPU
- **6502 CPU Implementation**
  - Support for **official opcodes**.
  - Handles instructions, following the original 6502 specification.
  - Stack operations and branching logic are implemented.

### ROM Loading
- **iNES ROM Support**
  - Parsing of ROM headers to load PRG and CHR data.
  - Support for **Mapper 0 (NROM)**.

### Bus
- A **bus** system that connects the CPU, RAM, ROM.

## 🛠️ To-Do Features

- [ ] **PPU (graphics)** — render NES graphics
- [ ] **APU (sound)** — audio emulation
- [ ] **Additional mappers** — extended ROM compatibility
- [ ] **Controller input** — simulate gamepad controls

## 📂 Project Structure

- **`src/emulator/cpu.rs`**: Implements the MOS 6502 CPU, including opcodes and instruction handling.
- **`src/emulator/bus.rs`**: The bus system that connects CPU, memory, and ROM.
- **`src/emulator/rom.rs`**: Handles loading and parsing of NES ROM files.
- **`src/main.rs`**: The main entry point for the emulator.
