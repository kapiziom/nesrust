# CLAUDE.md

Guidance for AI agents working in this repository.

## What this project is

`nesrs` is a NES emulator in Rust.

- **Emulation code does no I/O**: no windowing, audio device, filesystem or threading. It takes bytes in (ROM, input)
  and produces bytes out (RGBA framebuffer, audio samples).
- **Correctness first, then speed.** Every behaviour change should be backed by a test (unit test, or a reference-ROM/log comparison).
- Roadmap and status: [.plans/PLAN.md](.plans/PLAN.md) (local only — `.plans/` is gitignored). Read it before starting non-trivial work and update its checkboxes when finishing a task.

## Current state (keep this section up to date)

Single library crate `nesrs` (plus `examples/viewer.rs`). Module layout under `src/emulator/`:

| Module | State |
|---|---|
| `cpu/` | 6502 official opcodes + stable unofficial ones (NOP variants, SBC $EB, LAX, SAX, DCP, ISB, SLO, RLA, SRE, RRA, ANC, ALR, ARR, AXS; `OperationCode::official` marks them), `CPU<B: CpuBus>` with static dispatch; **every bus access is one CPU cycle** (`CPU::read/write` tick the bus; dummy reads/writes included), opcode table is a `const` array, dispatch is by `Mnemonic`. **Passes nestest** (`tests/nestest.rs`, compares PC/A/X/Y/P/SP/CYC with the reference log). NMI (edge), IRQ (level, via `CpuBus::irq_pending`, masked by I) and BRK are handled with correct B/U bits and return addresses. `reset()` implemented. Remaining: CLI/SEI/PLP interrupt-poll delay, exact poll timing, SingleStepTests (see PLAN.md). Unstable unofficial ops (XAA, AHX, SHX, SHY, TAS, LAS, LAX #imm) and KIL/JAM not implemented.  |
| `bus/` | `CpuBus` trait (`read(&mut self)`, `write`, `tick`, `fetch_nmi`, `irq_pending`), real `Bus` (RAM+PPU+cartridge, open-bus value for unmapped reads), `MockBus` (flat 64KB for CPU tests). APU registers are routed to `Apu`, controllers to `controller.rs`; an OAM DMA or DMC DMA stalls the CPU (`CPU::run_oam_dma` is a cycle-by-cycle get/put simulation in which a DMC fetch shares its halt/dummy/alignment cycles with the copy; `run_dmc_dma` for fetches while the CPU runs; get cycles are the odd ones for both). |
| `cartridge/` | `ines.rs`: iNES + NES 2.0 parser (`Result`, never panics). `mapper/`: `trait Mapper` + NROM(0), MMC1(1), UxROM(2), CNROM(3), MMC3(4, and 206 as its simple Namco 108 variant), AxROM(7), MMC2/MMC4(9, 10), discrete boards in `discrete.rs` (11, 66, 79, 87, 140, 152, 180), BNROM/NINA-001(34), VRC2/VRC4(21, 22, 23, 25), FME-7(69, no expansion audio), Camerica(71); mapper-controlled mirroring, PRG-RAM/battery RAM, CHR-RAM, IRQ line. `Cartridge` wraps a boxed mapper. The PPU reads CHR through `Cartridge::ppu_read` and mirroring through `mirroring()`. |
| `ppu/` | Dot-by-dot 2C02: registers, VRAM/palette, loopy scrolling, background + sprite pipelines, sprite 0 hit/overflow, NMI/vblank timing, RGBA framebuffer + palette-index buffer. `sprites.rs`, `palette.rs`. Sprite evaluation runs in one step at dot 65 (the overflow flag is set at the dot the hardware would find the 9th sprite); the I/O latch decays after 36 frames. |
| `apu/` | 2A03 APU: pulse x2 (sweep, envelope), triangle, noise, DMC (with DMA through the CPU), frame counter (4/5-step, IRQ), nonlinear mixer + the console's analog filters, box-filter resampling to the output rate (`Nes::set_sample_rate`, `drain_audio`). Written files: `units.rs` (timer, length counter, envelope), one file per channel, `frame_counter.rs`, `mixer.rs`. The APU ticks at the END of each CPU cycle; length counter writes are committed at the end of the cycle (see `LengthCounter`). |
| `region.rs` | `Region` (NTSC / PAL): CPU clock, scanlines per frame (262 / 312), PPU dots per CPU cycle (3 / 3.2), odd-frame dot skip (NTSC only). Taken from the cartridge header (NES 2.0 byte 12, or the iNES 1 PAL flag) and applied by `Bus`, `PPU` and `Apu` (APU tables for noise periods, DMC rates, frame counter steps). Dendy is not emulated (treated as NTSC). PAL timing is verified only by unit tests, not by hardware test ROMs. |
| `controller.rs` | Standard controller (`Controller`, `Buttons`): strobe latch, 8-bit shift register, ones after the 8th read. Bus routes `$4016` (strobe, shared) and `$4016`/`$4017` reads; open-bus upper bits. No Zapper / Four Score. |
| `nes.rs` | `Nes` facade: CPU + bus; `new` (runs reset), `reset`, `run_frame`, `framebuffer`, `set_buttons(port, Buttons)`, `save_state` / `load_state` (serde + postcard; the ROM, palette, picture and audio rate stay with the running console, a state only loads into the same game; bump `STATE_VERSION` when a serialized struct changes; every mapper derives serde, keeps its ROM in `#[serde(skip)]` fields and uses `mapper_state!()`). Entry point for frontends and test runners. |
| `examples/viewer.rs` | Minimal windowed viewer for manual testing (minifb + cpal, dev-dependencies so the library stays I/O-free): keyboard input, sound (the audio buffer paces the emulation), pause/reset/4x speed, PNG screenshots (also headless: `--screenshot out.png --frames N --press start`), `.sav` battery files, F5/F9 save/load state (`.state`). `VIEWER_DEBUG=1` prints the real emulation and audio device rates; headless mode can record a `--wav`. |

## Commands

Shell on this machine is Windows (PowerShell / Git Bash); commands are plain cargo.

```
cargo test                    # all unit + integration tests (must stay green)
cargo test cpu::              # a subset by path
cargo clippy --all-targets    # lint; new code should not add warnings
cargo fmt                     # format (rustfmt defaults)
cargo clean                   # run when target/ grows to several GB (check with: du -sh target); next build is a full rebuild
cargo run --release --example viewer -- game.nes   # play / inspect a ROM in a window
```

## Conventions

- Rust edition 2021 today (migration to the latest edition is a PLAN.md task). No `unsafe` unless there is a measured reason and a comment.
- **No `println!` / `eprintln!` in emulation code.** Use the `log` crate (`trace!` for per-instruction tracing, which must be compiled out/cheap when disabled) or a dedicated trace callback. Per-instruction printing destroys performance.
- **No `panic!` for emulated-hardware behaviour.** Unmapped reads return open-bus/last-bus value, unmapped writes are ignored. Panics are acceptable only for programmer errors (broken invariants). Malformed ROM files return `Result::Err`, never panic.
- Hot paths (CPU step, bus read/write, PPU dot) must not allocate, hash (`HashMap`), or use `lazy_static` lookups. Use `static`/`const` arrays indexed by opcode.
- Prefer static dispatch (`CPU<B: CpuBus>`) over `Box<dyn Trait>` in per-cycle paths. Trait objects are fine at the outermost boundary.
- Hardware reads can have side effects (PPU `$2002`, `$2007`, controller shift registers) — bus `read` takes `&mut self`.
- Arithmetic on emulated registers/addresses uses explicit `wrapping_*` ops; emulated overflow must never trigger a Rust debug-mode panic.
- Keep emulated component state in plain structs that are `serde`-serializable (save states, rewind); anything that is not console state (ROM, palette, output buffers) is `#[serde(skip)]` and carried over in `load_state`. Don't hide state in `static`s or thread-locals.
- Tests live next to the code (`#[cfg(test)] mod tests`) for units; ROM-based/integration tests in `tests/`. Use `MockBus` for CPU unit tests; in tests set up/inspect memory with `cpu.poke/peek` (no cycle cost), not `read/write`.
- Naming: hardware terms as in nesdev wiki (`PPUCTRL`, `OAMADDR`, `v/t/x/w` loopy registers). Cite the nesdev.org page in a comment when implementing a non-obvious quirk.
- Comments explain *why* (hardware quirk, source), not what.

## Testing & reference material

- Primary CPU oracle: **nestest** (`nestest.nes` + `nestest.log`, run from `$C000` with PC override, compare per-instruction log incl. cycles). Then Klaus Dormann 6502 functional test and SingleStepTests/65x02 JSON tests for per-cycle bus traces.
- Blargg test ROMs (`instr_test-v5`, `cpu_interrupts_v2`, `ppu_vbl_nmi`, `ppu_sprite_hit`, `apu_test`, …) report status via `$6000` — a headless runner should poll that.
- Test ROMs go in `test_rom/` (only freely redistributable ones). **Never commit commercial game ROMs.**
- `tests/blargg.rs` runs Blargg's ROMs from `test_rom/blargg/` headless (119/121 pass; the passing ones are the `MUST_PASS` regression gate). `BLARGG_FILTER=ppu_vbl_nmi cargo test --test blargg -- --nocapture` runs a subset and prints each result. After a change that makes more ROMs pass, add them to `MUST_PASS`. See PLAN.md for what the failing ones need.
- Timing between CPU and PPU is calibrated, not guessed: `READ_DOTS_BEFORE` / `WRITE_DOTS_BEFORE` in `bus/mod.rs` were chosen so ppu_vbl_nmi passes 10/10. Re-run it after touching PPU or bus timing.
- Spec: https://www.nesdev.org/wiki/ (CPU, PPU registers/rendering/scrolling, APU, mappers, iNES/NES 2.0).

## Working agreements

- Small, reviewable changes: one concern per commit (e.g. "fix indirect-indexed zero-page wrap" separate from "add opcode table").
- Before changing CPU/PPU timing, state which hardware behaviour the change targets and which test proves it.
- If the plan and the code disagree, say so and propose an update to PLAN.md rather than silently diverging.
- Don't add dependencies without a reason.
- Commit/push only when asked.
