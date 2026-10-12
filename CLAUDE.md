# CLAUDE.md

Guidance for AI agents working in this repository.

## What this project is

`nesrs` is a NES emulator in Rust. It is the first system of a planned **multi-console emulator desktop app**
(Windows / Linux / macOS, most likely Tauri; NES first, then other retro consoles). So:

- **Emulation cores must be frontend-agnostic**: no windowing, audio device, filesystem or threading inside core code.
  A core takes bytes in (ROM, input) and produces bytes out (RGBA framebuffer, audio samples). It should compile for `wasm32` too.
- **Correctness first, then speed.** Every behaviour change should be backed by a test (unit test, or a reference-ROM/log comparison).
- Roadmap and status: [.plans/PLAN.md](.plans/PLAN.md) (local only — `.plans/` is gitignored). Read it before starting non-trivial work and update its checkboxes when finishing a task.

## Current state (keep this section up to date)

Single crate `nesrs` (lib + bin). Module layout under `src/emulator/`:

| Module | State |
|---|---|
| `cpu/` | 6502 official opcodes + stable unofficial ones (NOP variants, SBC $EB, LAX, SAX, DCP, ISB, SLO, RLA, SRE, RRA, ANC, ALR, ARR, AXS; `OperationCode::official` marks them), interpreter-style, cycles ticked **after** each instruction. Several known bugs (see PLAN.md §1). Unstable unofficial ops (XAA, AHX, SHX, SHY, TAS, LAS, LAX #imm) and KIL/JAM not implemented. No page-cross penalty, no IRQ line. |
| `bus/` | `CpuBus` trait (`read(&self)`, `write`, `tick`, `fetch_nmi`), real `Bus` (RAM+PPU+ROM), `MockBus` (flat 64KB for CPU tests). APU/controller registers `panic!`. |
| `rom/` | iNES parser (no NES 2.0, no bounds checks), mapper logic hard-coded as `match` (only NROM). PPU cannot access CHR yet. |
| `ppu/` | Stub: timing counters + NMI flag only. Registers are **not** implemented. |
| `apu/` | Empty. |
| `main.rs` | Throw-away runner: `cargo run -- <rom.nes>` starts the CPU at `$8000` with `println!` tracing. Most real ROMs panic on unimplemented APU/controller registers. |

Target architecture (not yet created) is a Cargo workspace — see PLAN.md §3. When refactoring toward it, move code, don't rewrite working code.

## Commands

Shell on this machine is Windows (PowerShell / Git Bash); commands are plain cargo.

```
cargo test                    # all unit + integration tests (must stay green)
cargo test cpu::              # a subset by path
cargo clippy --all-targets    # lint; new code should not add warnings
cargo fmt                     # format (rustfmt defaults)
cargo clean                   # run when target/ grows to several GB (check with: du -sh target); next build is a full rebuild
cargo run --release           # run the demo binary
```

## Conventions

- Rust edition 2021 today (migration to the latest edition is a PLAN.md task). No `unsafe` unless there is a measured reason and a comment.
- **No `println!` / `eprintln!` in emulation code.** Use the `log` crate (`trace!` for per-instruction tracing, which must be compiled out/cheap when disabled) or a dedicated trace callback. Per-instruction printing destroys performance.
- **No `panic!` for emulated-hardware behaviour.** Unmapped reads return open-bus/last-bus value, unmapped writes are ignored. Panics are acceptable only for programmer errors (broken invariants). Malformed ROM files return `Result::Err`, never panic.
- Hot paths (CPU step, bus read/write, PPU dot) must not allocate, hash (`HashMap`), or use `lazy_static` lookups. Use `static`/`const` arrays indexed by opcode.
- Prefer static dispatch (`CPU<B: Bus>`) over `Box<dyn Trait>` in per-cycle paths. Trait objects are fine at the *outermost* boundary (frontend ↔ console).
- Hardware reads can have side effects (PPU `$2002`, `$2007`, controller shift registers) — bus `read` takes `&mut self`.
- Arithmetic on emulated registers/addresses uses explicit `wrapping_*` ops; emulated overflow must never trigger a Rust debug-mode panic.
- Keep emulated component state in plain structs that are `Clone`able and later `serde`-serializable (save states, rewind). Don't hide state in `static`s or thread-locals.
- Tests live next to the code (`#[cfg(test)] mod tests`) for units; ROM-based/integration tests in `tests/`. Use `MockBus` for CPU unit tests.
- Naming: hardware terms as in nesdev wiki (`PPUCTRL`, `OAMADDR`, `v/t/x/w` loopy registers). Cite the nesdev.org page in a comment when implementing a non-obvious quirk.
- Comments explain *why* (hardware quirk, source), not what.

## Testing & reference material

- Primary CPU oracle: **nestest** (`nestest.nes` + `nestest.log`, run from `$C000` with PC override, compare per-instruction log incl. cycles). Then Klaus Dormann 6502 functional test and SingleStepTests/65x02 JSON tests for per-cycle bus traces.
- Blargg test ROMs (`instr_test-v5`, `cpu_interrupts_v2`, `ppu_vbl_nmi`, `ppu_sprite_hit`, `apu_test`, …) report status via `$6000` — a headless runner should poll that.
- Test ROMs go in `test_rom/` (only freely redistributable ones). **Never commit commercial game ROMs.**
- Spec: https://www.nesdev.org/wiki/ (CPU, PPU registers/rendering/scrolling, APU, mappers, iNES/NES 2.0).

## Working agreements

- Small, reviewable changes: one concern per commit (e.g. "fix indirect-indexed zero-page wrap" separate from "add opcode table").
- Before changing CPU/PPU timing, state which hardware behaviour the change targets and which test proves it.
- If the plan and the code disagree, say so and propose an update to PLAN.md rather than silently diverging.
- Don't add dependencies without a reason; unused ones (`log`, `lazy_static`) should be removed or put to use.
- Commit/push only when asked.
