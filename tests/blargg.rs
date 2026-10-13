//! Runs Blargg's test ROMs headless and reports their results.
//!
//! Newer Blargg ROMs write their result to cartridge RAM: $6001-$6003 hold the signature DE B0 61, $6000 the
//! status (0x80 while running, 0 = passed, anything else = failure code) and $6004.. a zero-terminated text.
//! https://www.nesdev.org/wiki/Emulator_tests
//!
//! The ROMs are not part of this repository: download them into test_rom/blargg as described in README.md
//! (source: github.com/christopherpow/nes-test-roms). If the directory is missing the test does nothing. ROMs listed in `MUST_PASS` are a regression gate; the rest are only reported
//! (run with `cargo test --test blargg -- --nocapture` to see the table).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;

const ROM_DIR: &str = "test_rom/blargg";
const MAX_FRAMES: usize = 2400; // 40 seconds of console time (ROMs that report early stop sooner)

/// Combined multi-test ROMs; the single-test ROMs in rom_singles/ cover the same ground with clearer results.
const SKIPPED: &[&str] = &[
    "ppu_vbl_nmi/ppu_vbl_nmi.nes",
    "instr_test-v5/all_instrs.nes",
    "instr_test-v5/official_only.nes",
    "cpu_interrupts_v2/cpu_interrupts.nes",
    "instr_misc/instr_misc.nes",
    "instr_timing/instr_timing.nes",
];

/// ROMs (path relative to ROM_DIR) that currently pass and must keep passing.
const MUST_PASS: &[&str] = &[
    "blargg_ppu_tests_2005.09.15b/palette_ram.nes",
    "blargg_ppu_tests_2005.09.15b/sprite_ram.nes",
    "blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes",
    "blargg_ppu_tests_2005.09.15b/vram_access.nes",
    "branch_timing_tests/1.Branch_Basics.nes",
    "branch_timing_tests/2.Backward_Branch.nes",
    "branch_timing_tests/3.Forward_Branch.nes",
    "cpu_dummy_reads/cpu_dummy_reads.nes",
    "cpu_dummy_writes/cpu_dummy_writes_oam.nes",
    "cpu_dummy_writes/cpu_dummy_writes_ppumem.nes",
    "cpu_timing_test6/cpu_timing_test.nes",
    "instr_misc/rom_singles/01-abs_x_wrap.nes",
    "instr_misc/rom_singles/02-branch_wrap.nes",
    "instr_misc/rom_singles/03-dummy_reads.nes",
    "instr_test-v5/rom_singles/01-basics.nes",
    "instr_test-v5/rom_singles/02-implied.nes",
    "instr_test-v5/rom_singles/03-immediate.nes",
    "instr_test-v5/rom_singles/04-zero_page.nes",
    "instr_test-v5/rom_singles/05-zp_xy.nes",
    "instr_test-v5/rom_singles/06-absolute.nes",
    "instr_test-v5/rom_singles/07-abs_xy.nes",
    "instr_test-v5/rom_singles/08-ind_x.nes",
    "instr_test-v5/rom_singles/09-ind_y.nes",
    "instr_test-v5/rom_singles/10-branches.nes",
    "instr_test-v5/rom_singles/11-stack.nes",
    "instr_test-v5/rom_singles/12-jmp_jsr.nes",
    "instr_test-v5/rom_singles/13-rts.nes",
    "instr_test-v5/rom_singles/14-rti.nes",
    "instr_test-v5/rom_singles/15-brk.nes",
    "instr_test-v5/rom_singles/16-special.nes",
    "mmc3_test/1-clocking.nes",
    "mmc3_test/2-details.nes",
    "mmc3_test/3-A12_clocking.nes",
    "mmc3_test/4-scanline_timing.nes",
    "mmc3_test/5-MMC3.nes",
    "mmc3_test_2/rom_singles/1-clocking.nes",
    "mmc3_test_2/rom_singles/2-details.nes",
    "mmc3_test_2/rom_singles/3-A12_clocking.nes",
    "mmc3_test_2/rom_singles/4-scanline_timing.nes",
    "mmc3_test_2/rom_singles/5-MMC3.nes",
    "oam_read/oam_read.nes",
    "oam_stress/oam_stress.nes",
    "ppu_vbl_nmi/rom_singles/01-vbl_basics.nes",
    "ppu_vbl_nmi/rom_singles/02-vbl_set_time.nes",
    "ppu_vbl_nmi/rom_singles/03-vbl_clear_time.nes",
    "ppu_vbl_nmi/rom_singles/04-nmi_control.nes",
    "ppu_vbl_nmi/rom_singles/05-nmi_timing.nes",
    "ppu_vbl_nmi/rom_singles/06-suppression.nes",
    "ppu_vbl_nmi/rom_singles/07-nmi_on_timing.nes",
    "ppu_vbl_nmi/rom_singles/08-nmi_off_timing.nes",
    "ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes",
    "ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes",
    "sprite_hit_tests_2005.10.05/01.basics.nes",
    "sprite_hit_tests_2005.10.05/02.alignment.nes",
    "sprite_hit_tests_2005.10.05/03.corners.nes",
    "sprite_hit_tests_2005.10.05/04.flip.nes",
    "sprite_hit_tests_2005.10.05/05.left_clip.nes",
    "sprite_hit_tests_2005.10.05/06.right_edge.nes",
    "sprite_hit_tests_2005.10.05/07.screen_bottom.nes",
    "sprite_hit_tests_2005.10.05/08.double_height.nes",
    "sprite_hit_tests_2005.10.05/09.timing_basics.nes",
    "sprite_hit_tests_2005.10.05/10.timing_order.nes",
    "sprite_hit_tests_2005.10.05/11.edge_timing.nes",
    "sprite_overflow_tests/1.Basics.nes",
    "sprite_overflow_tests/2.Details.nes",
    "sprite_overflow_tests/4.Obscure.nes",
    "sprite_overflow_tests/5.Emulator.nes",
];

#[derive(Debug, PartialEq)]
enum Outcome {
    Passed,
    Failed { code: u8, text: String },
    /// The ROM never wrote the $6000 signature (older ROMs report on screen instead); holds the on-screen text.
    NoResult { screen: String },
    CpuStopped { pc: u16, opcode: u8 },
    NotLoadable(String),
}

fn find_roms(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_roms(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "nes") {
            found.push(path);
        }
    }
}

fn read_text(nes: &mut Nes) -> String {
    let mut text = String::new();
    for address in 0x6004..0x7000u16 {
        match nes.cpu.peek(address) {
            0 => break,
            byte => text.push(byte as char),
        }
    }
    text.trim().to_string()
}

fn run_rom(path: &Path) -> Outcome {
    let data = fs::read(path).expect("readable ROM");
    let cartridge = match Cartridge::from_bytes(&data) {
        Ok(cartridge) => cartridge,
        Err(error) => return Outcome::NotLoadable(error.to_string()),
    };

    let mut nes = Nes::new(cartridge);

    for _ in 0..MAX_FRAMES {
        if !nes.run_frame() {
            let pc = nes.cpu.program_counter;
            return Outcome::CpuStopped { pc, opcode: nes.cpu.peek(pc.wrapping_sub(1)) };
        }

        let signature = [nes.cpu.peek(0x6001), nes.cpu.peek(0x6002), nes.cpu.peek(0x6003)];
        let status = nes.cpu.peek(0x6000);

        // check the screen twice a second; verdict text appears once the ROM is done
        if nes.bus().ppu().frame() % 30 == 0 {
            let screen = screen_text(&nes);
            match screen_verdict(&screen) {
                Some(true) => return Outcome::Passed,
                Some(false) => return Outcome::Failed { code: 0, text: screen },
                None => {}
            }
        }

        if signature == [0xDE, 0xB0, 0x61] && status != 0x80 && status != 0x81 {
            return if status == 0 {
                Outcome::Passed
            } else {
                Outcome::Failed { code: status, text: read_text(&mut nes) }
            };
        }
    }

    Outcome::NoResult { screen: screen_text(&nes) }
}

/// Older ROMs print their verdict on screen: "PASSED"/"FAILED" text, or a result code like "$01" (1 = passed).
fn screen_verdict(screen: &str) -> Option<bool> {
    let upper = screen.to_uppercase();
    if upper.contains("PASSED") || screen.trim() == "$01" {
        Some(true)
    } else if upper.contains("FAILED") || (screen.trim().starts_with('$') && screen.trim().len() <= 3) {
        Some(false)
    } else {
        None
    }
}

/// Old Blargg ROMs print ASCII tile indices into the first nametable; reconstruct the visible text.
fn screen_text(nes: &Nes) -> String {
    let nametable = &nes.bus().ppu().nametable_ram()[..960];
    let lines: Vec<String> = nametable
        .chunks(32)
        .map(|row| row.iter().map(|&tile| if (0x20..0x7F).contains(&tile) { tile as char } else { ' ' }).collect::<String>())
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    lines.join(" | ")
}

#[test]
fn blargg_roms() {
    let root = Path::new(ROM_DIR);
    let mut roms = Vec::new();
    find_roms(root, &mut roms);
    roms.sort();
    roms.retain(|rom| {
        let name = rom.strip_prefix(root).unwrap().to_string_lossy().replace("\\", "/");
        !SKIPPED.contains(&name.as_str())
    });

    // BLARGG_FILTER=ppu_vbl_nmi cargo test --test blargg -- --nocapture   runs only matching ROMs
    if let Ok(filter) = std::env::var("BLARGG_FILTER") {
        roms.retain(|rom| rom.to_string_lossy().replace("\\", "/").contains(&filter));
    }

    if roms.is_empty() {
        eprintln!("{ROM_DIR} not found, skipping Blargg tests");
        return;
    }

    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(rom) = roms.get(index) else { break };
                let outcome = run_rom(rom);
                results.lock().unwrap().push((index, outcome));
            });
        }
    });

    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(index, _)| *index);

    let mut regressions = Vec::new();
    let mut passed = 0;

    for (index, outcome) in &results {
        let name = roms[*index].strip_prefix(root).unwrap().to_string_lossy().replace("\\", "/");
        let must_pass = MUST_PASS.contains(&name.as_str());

        match outcome {
            Outcome::Passed => {
                passed += 1;
                println!("PASS  {name}{}", if must_pass { "" } else { "   <- not in MUST_PASS yet" });
            }
            Outcome::Failed { code, text } => println!("FAIL  {name} (code {code}) {}", text.replace("\n", " | ")),
            other => println!("----  {name}: {other:?}"),
        }

        if must_pass && *outcome != Outcome::Passed {
            regressions.push(name);
        }
    }

    println!("{passed}/{} Blargg ROMs pass", roms.len());
    assert!(regressions.is_empty(), "regressions: {regressions:?}");
}
