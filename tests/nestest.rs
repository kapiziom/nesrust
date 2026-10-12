//! Runs Kevin Horton's nestest in "automation mode" (start at $C000) and compares every
//! executed instruction with the reference `nestest.log`: PC, A, X, Y, P, SP and CYC.
//! The log's PPU columns are ignored until the PPU exists.

use std::fs;

use nesrs::emulator::bus::mock_bus::MockBus;
use nesrs::emulator::cpu::{CpuState, CPU};
use nesrs::emulator::rom::ROM;

const ROM_PATH: &str = "test_rom/nestest.nes";
const LOG_PATH: &str = "test_rom/nestest.log";

/// Initial state of the reference log (RESET already took 7 cycles).
const START: CpuState = CpuState { pc: 0xC000, a: 0, x: 0, y: 0, p: 0x24, sp: 0xFD };
const START_CYCLES: u64 = 7;

/// BREAK is not a real register bit: the reference emulator keeps it cleared while
/// bit 5 is always set, so normalise our P before comparing.
fn normalize_p(p: u8) -> u8 {
    (p | 0x20) & !0x10
}

fn hex_field(line: &str, marker: &str, digits: usize) -> u32 {
    let start = line.find(marker).unwrap_or_else(|| panic!("no '{marker}' in log line: {line}")) + marker.len();
    u32::from_str_radix(&line[start..start + digits], 16).unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct Expected {
    state: CpuState,
    cycles: u64,
}

fn parse_log_line(line: &str) -> Expected {
    let cycles_start = line.find("CYC:").expect("no CYC in log line") + 4;

    Expected {
        state: CpuState {
            pc: u16::from_str_radix(&line[0..4], 16).unwrap(),
            a: hex_field(line, " A:", 2) as u8,
            x: hex_field(line, " X:", 2) as u8,
            y: hex_field(line, " Y:", 2) as u8,
            p: hex_field(line, " P:", 2) as u8,
            sp: hex_field(line, " SP:", 2) as u8,
        },
        cycles: line[cycles_start..].trim().parse().unwrap(),
    }
}

#[test]
fn nestest_matches_reference_log() {
    let rom = ROM::from_nes_file(&fs::read(ROM_PATH).expect("missing test_rom/nestest.nes"))
        .expect("nestest.nes should parse");
    assert_eq!(rom.prg_rom.len(), 16384, "nestest is a single 16KB PRG bank (NROM-128)");

    // flat memory is enough: automation mode only touches RAM and PRG
    let mut bus = MockBus::new();
    bus.load_program(&rom.prg_rom, 0x8000);
    bus.load_program(&rom.prg_rom, 0xC000);

    let mut cpu = CPU::new(bus);
    cpu.set_state(START);
    cpu.cycles = START_CYCLES;

    let log = fs::read_to_string(LOG_PATH).expect("missing test_rom/nestest.log");

    for (index, line) in log.lines().enumerate() {
        let expected = parse_log_line(line);

        let mut actual = cpu.state();
        actual.p = normalize_p(actual.p);

        assert_eq!(
            (actual, cpu.cycles),
            (expected.state, expected.cycles),
            "mismatch before executing log line {} (left = ours, right = reference)\n  {}",
            index + 1,
            line
        );

        assert!(cpu.step(), "CPU stopped at log line {}: {}", index + 1, line);
    }

    // nestest stores its result codes at $0002/$0003; 0 means every test passed
    assert_eq!(cpu.peek(0x0002), 0x00, "nestest official-opcode result code");
    assert_eq!(cpu.peek(0x0003), 0x00, "nestest unofficial-opcode result code");
}
