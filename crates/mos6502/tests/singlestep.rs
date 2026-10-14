//! SingleStepTests/65x02 (the nes6502 set): every opcode is run on random initial states and compared with the
//! reference, including each bus access (address, value, read or write) and so the exact cycle count.
//! The test files are not in the repository: run `powershell -File test_rom/download_singlestep.ps1` once
//! (without them this test does nothing). `SINGLESTEP_FILTER=a9 cargo test --test singlestep -- --nocapture` runs
//! one opcode and prints the first failures in detail.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use mos6502::cpu_bus::CpuBus;
use mos6502::{CpuState, CPU};
use serde::Deserialize;

const TEST_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test_rom/singlestep");

/// Opcodes whose reference results are not matched. Everything else must pass.
/// $AB (LXA) is analog-dependent: the reference ORs $EE into A, Blargg's instr_test (real NES) expects $FF.
const KNOWN_FAILURES: &[&str] = &["ab"];

#[derive(Deserialize)]
struct Snapshot {
    pc: u16,
    s: u8,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    ram: Vec<(u16, u8)>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    initial: Snapshot,
    #[serde(rename = "final")]
    expected: Snapshot,
    cycles: Vec<(u16, u8, String)>,
}

/// 64KB of RAM that records every access.
struct RecordingBus {
    memory: Vec<u8>,
    accesses: Vec<(u16, u8, bool)>,
}

impl CpuBus for RecordingBus {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.memory[addr as usize];
        self.accesses.push((addr, value, false));
        value
    }

    fn write(&mut self, addr: u16, data: u8) {
        self.memory[addr as usize] = data;
        self.accesses.push((addr, data, true));
    }

    fn tick(&mut self, _cycles: u8) {}

    fn fetch_nmi(&mut self) -> Option<u8> {
        None
    }
}

/// Runs one case; returns a description of the first difference.
fn run_case(cpu: &mut CPU<RecordingBus>, case: &Case) -> Result<(), String> {
    for &(address, value) in &case.initial.ram {
        cpu.bus.memory[address as usize] = value;
    }
    cpu.bus.accesses.clear();
    let initial = &case.initial;
    cpu.set_state(CpuState { pc: initial.pc, a: initial.a, x: initial.x, y: initial.y, p: initial.p, sp: initial.s });

    let ran = cpu.step();

    let mut problems = String::new();
    let state = cpu.state();
    let expected = &case.expected;
    // the JAM opcodes freeze the processor: `step` reports that it did not run on
    let jams = matches!(
        case.name.get(..2),
        Some("02" | "12" | "22" | "32" | "42" | "52" | "62" | "72" | "92" | "b2" | "d2" | "f2")
    );
    if ran == jams {
        writeln!(problems, "step returned {ran} for an opcode that {} jam", if jams { "should" } else { "should not" })
            .unwrap();
    }
    for (name, got, want) in [
        ("pc", state.pc as u32, expected.pc as u32),
        ("a", state.a as u32, expected.a as u32),
        ("x", state.x as u32, expected.x as u32),
        ("y", state.y as u32, expected.y as u32),
        ("sp", state.sp as u32, expected.s as u32),
        // the B flag only exists on the stack: ignore bit 4, and bit 5 is always set
        ("p", (state.p | 0x20) as u32 & 0xEF, (expected.p | 0x20) as u32 & 0xEF),
    ] {
        if got != want {
            writeln!(problems, "{name}: got {got:#06X}, expected {want:#06X}").unwrap();
        }
    }

    for &(address, want) in &expected.ram {
        let got = cpu.bus.memory[address as usize];
        if got != want {
            writeln!(problems, "ram[{address:#06X}]: got {got:#04X}, expected {want:#04X}").unwrap();
        }
    }

    let wanted: Vec<(u16, u8, bool)> = case.cycles.iter().map(|(a, v, kind)| (*a, *v, kind == "write")).collect();
    if cpu.bus.accesses != wanted {
        let render = |list: &[(u16, u8, bool)]| {
            list.iter()
                .map(|(a, v, w)| format!("{}{a:04X}={v:02X}", if *w { "W" } else { "R" }))
                .collect::<Vec<_>>()
                .join(" ")
        };
        writeln!(problems, "bus:\n    got      {}\n    expected {}", render(&cpu.bus.accesses), render(&wanted))
            .unwrap();
    }

    if jams {
        cpu.reset(); // only a reset unfreezes the processor
    }

    // leave the memory clean for the next case
    for &(address, _) in case.initial.ram.iter().chain(&case.expected.ram) {
        cpu.bus.memory[address as usize] = 0;
    }
    for &(address, _, _) in &cpu.bus.accesses {
        cpu.bus.memory[address as usize] = 0;
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{}\n  {}", case.name, problems.replace('\n', "\n  ")))
    }
}

#[test]
fn single_step_tests() {
    if !Path::new(TEST_DIR).is_dir() {
        eprintln!("{TEST_DIR} not found, skipping (see test_rom/download_singlestep.ps1)");
        return;
    }
    let filter = std::env::var("SINGLESTEP_FILTER").ok();
    let verbose = filter.is_some();

    let mut files: Vec<_> = fs::read_dir(TEST_DIR)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    files.sort();

    let mut failed_opcodes = Vec::new();
    let mut total = 0;

    for path in files {
        let opcode = path.file_stem().unwrap().to_string_lossy().to_string();
        if filter.as_ref().is_some_and(|wanted| !opcode.contains(wanted.as_str())) {
            continue;
        }

        let cases: Vec<Case> = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut cpu = CPU::new(RecordingBus { memory: vec![0; 0x10000], accesses: Vec::new() });
        let mut failures = Vec::new();
        for case in &cases {
            if let Err(description) = run_case(&mut cpu, case) {
                failures.push(description);
            }
        }
        total += cases.len();

        if !failures.is_empty() {
            if verbose {
                eprintln!("opcode {opcode}: {} of {} failed", failures.len(), cases.len());
                for failure in failures.iter().take(3) {
                    eprintln!("{failure}");
                }
            }
            failed_opcodes.push((opcode, failures.len(), cases.len()));
        }
    }

    for (opcode, failed, cases) in &failed_opcodes {
        eprintln!("opcode {opcode}: {failed} of {cases} cases failed");
    }
    let unexpected: Vec<_> =
        failed_opcodes.iter().filter(|(opcode, _, _)| !KNOWN_FAILURES.contains(&opcode.as_str())).collect();
    eprintln!("{total} cases, {} opcodes with failures", failed_opcodes.len());
    assert!(unexpected.is_empty(), "{} opcodes fail (see above)", unexpected.len());
}
