use super::*;

/// A register write; the APU commits it at the end of that CPU cycle.
fn write(apu: &mut Apu, address: u16, data: u8) {
    apu.write_register(address, data);
    apu.tick();
}

fn run(apu: &mut Apu, cycles: usize) {
    for _ in 0..cycles {
        apu.tick();
    }
}

fn drain(apu: &mut Apu) -> Vec<f32> {
    let mut out = Vec::new();
    apu.drain_samples(&mut out);
    out
}

/// Pulse 1 at constant volume 15, 50% duty, halted length counter, period `t` (f = 1789773 / (16 * (t + 1))).
fn start_pulse1(apu: &mut Apu, period: u16) {
    write(apu, 0x4015, 0x01);
    write(apu, 0x4000, 0b1011_1111);
    write(apu, 0x4001, 0x08); // sweep off (negate set so it never mutes)
    write(apu, 0x4002, period as u8);
    write(apu, 0x4003, 0x08 | (period >> 8) as u8);
}

/// Rising zero crossings per second of `samples` (the frequency of a clean tone).
fn rising_crossings(samples: &[f32], sample_rate: u32) -> f32 {
    let crossings = samples.windows(2).filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0).count();
    crossings as f32 * sample_rate as f32 / samples.len() as f32
}

#[test]
fn status_reports_which_length_counters_are_running() {
    let mut apu = Apu::default();
    assert_eq!(apu.read_status(), 0);

    write(&mut apu, 0x4015, 0b0000_1111);
    write(&mut apu, 0x4003, 0x08); // pulse 1
    write(&mut apu, 0x400B, 0x08); // triangle
    write(&mut apu, 0x400F, 0x08); // noise

    assert_eq!(apu.read_status() & 0x1F, 0b0000_1101);

    write(&mut apu, 0x4007, 0x08); // pulse 2
    assert_eq!(apu.read_status() & 0x1F, 0b0000_1111);
}

#[test]
fn clearing_an_enable_bit_clears_that_channels_length_counter() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x0F);
    write(&mut apu, 0x4003, 0x08);
    write(&mut apu, 0x4007, 0x08);

    write(&mut apu, 0x4015, 0b0000_0010);

    assert_eq!(apu.read_status() & 0x03, 0b10, "only pulse 2 is still running");
}

#[test]
fn a_disabled_channel_ignores_length_loads() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x00);
    write(&mut apu, 0x4003, 0x08);

    assert_eq!(apu.read_status() & 0x01, 0);
}

#[test]
fn length_counters_expire_on_half_frame_clocks() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x01);
    write(&mut apu, 0x4000, 0x00); // not halted
    write(&mut apu, 0x4003, 0x18); // length index 3 = 2 half-frame clocks

    run(&mut apu, 14_000);
    assert_eq!(apu.read_status() & 1, 1, "before the first half-frame clock (cycle 14913)");

    run(&mut apu, 1_000);
    assert_eq!(apu.read_status() & 1, 1, "one clock done, one to go");

    run(&mut apu, 15_000);
    assert_eq!(apu.read_status() & 1, 0, "the second half-frame clock (cycle 29829) silenced it");
}

#[test]
fn the_halt_flag_stops_the_length_counter() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x01);
    write(&mut apu, 0x4000, 0x20); // halt
    write(&mut apu, 0x4003, 0x18);

    run(&mut apu, 100_000);

    assert_eq!(apu.read_status() & 1, 1);
}

#[test]
fn the_frame_irq_is_raised_every_frame_and_cleared_by_reading_4015() {
    let mut apu = Apu::default();
    assert!(!apu.irq_pending());

    run(&mut apu, 29_830);
    assert!(apu.irq_pending());
    assert_ne!(apu.read_status() & 0x40, 0);
    apu.tick(); // the IRQ line follows the flag one cycle later
    assert!(!apu.irq_pending(), "reading $4015 acknowledges it");
    assert_eq!(apu.read_status() & 0x40, 0);

    run(&mut apu, 29_830);
    assert!(apu.irq_pending(), "and it comes back next frame");
}

#[test]
fn the_irq_inhibit_bit_and_5_step_mode_prevent_the_frame_irq() {
    let mut inhibited = Apu::default();
    write(&mut inhibited, 0x4017, 0x40);
    run(&mut inhibited, 100_000);
    assert!(!inhibited.irq_pending());

    let mut five_step = Apu::default();
    write(&mut five_step, 0x4017, 0x80);
    run(&mut five_step, 100_000);
    assert!(!five_step.irq_pending());
}

#[test]
fn writing_4017_with_the_inhibit_bit_acknowledges_a_pending_irq() {
    let mut apu = Apu::default();
    run(&mut apu, 29_830);
    assert!(apu.irq_pending());

    write(&mut apu, 0x4017, 0x40);
    apu.tick();

    assert!(!apu.irq_pending());
}

#[test]
fn the_right_number_of_samples_is_produced() {
    let mut apu = Apu::new(48_000);
    run(&mut apu, NTSC_CPU_CLOCK_HZ as usize);

    let samples = drain(&mut apu);

    assert!((47_999..=48_001).contains(&samples.len()), "{} samples in one second", samples.len());
}

#[test]
fn the_sample_rate_can_be_changed() {
    let mut apu = Apu::new(48_000);
    apu.set_sample_rate(44_100);
    run(&mut apu, NTSC_CPU_CLOCK_HZ as usize);

    assert_eq!(apu.sample_rate(), 44_100);
    assert!((44_099..=44_101).contains(&drain(&mut apu).len()));
}

#[test]
fn silence_is_silent() {
    let mut apu = Apu::default();
    // the triangle idles at level 15, which the high-pass filters remove after a short start-up thump
    run(&mut apu, 200_000);
    drain(&mut apu);

    run(&mut apu, 200_000);

    assert!(drain(&mut apu).iter().all(|sample| sample.abs() < 1e-4));
}

#[test]
fn a_pulse_channel_plays_a_tone_at_the_programmed_frequency() {
    let mut apu = Apu::new(48_000);
    start_pulse1(&mut apu, 253); // 1789773 / (16 * 254) = 440.4 Hz
    run(&mut apu, 100_000); // let the filters settle
    drain(&mut apu);

    run(&mut apu, NTSC_CPU_CLOCK_HZ as usize);
    let samples = drain(&mut apu);

    let loudest = samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    assert!(loudest > 0.05, "the tone is audible: {loudest}");
    let frequency = rising_crossings(&samples, 48_000);
    assert!((frequency - 440.4).abs() < 3.0, "measured {frequency} Hz");
}

#[test]
fn a_higher_period_register_gives_a_lower_note() {
    let measure = |period: u16| {
        let mut apu = Apu::new(48_000);
        start_pulse1(&mut apu, period);
        run(&mut apu, 100_000);
        drain(&mut apu);
        run(&mut apu, NTSC_CPU_CLOCK_HZ as usize / 2);
        rising_crossings(&drain(&mut apu), 48_000)
    };

    assert!(measure(0x1FC) < measure(0x0FE) * 0.6);
}

#[test]
fn the_triangle_channel_is_audible() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x04);
    write(&mut apu, 0x4008, 0xFF);
    write(&mut apu, 0x400A, 0x7F);
    write(&mut apu, 0x400B, 0x08);
    // the linear counter loads on the first quarter-frame clock
    run(&mut apu, 100_000);
    drain(&mut apu);

    run(&mut apu, 100_000);
    let samples = drain(&mut apu);

    assert!(samples.iter().any(|sample| sample.abs() > 0.02));
}

#[test]
fn the_noise_channel_is_audible_and_not_periodic_like_a_tone() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x08);
    write(&mut apu, 0x400C, 0x3F);
    write(&mut apu, 0x400E, 0x04);
    write(&mut apu, 0x400F, 0x08);
    run(&mut apu, 50_000);
    drain(&mut apu);

    run(&mut apu, 200_000);
    let samples = drain(&mut apu);

    assert!(samples.iter().any(|sample| sample.abs() > 0.02));
    assert!(samples.iter().any(|sample| *sample > 0.0) && samples.iter().any(|sample| *sample < 0.0));
}

#[test]
fn the_dmc_hands_out_dma_requests_and_raises_its_irq_at_the_end_of_the_sample() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4010, 0x80 | 0x0F); // IRQ on, fastest rate
    write(&mut apu, 0x4012, 0x00); // $C000
    write(&mut apu, 0x4013, 0x00); // 1 byte
    write(&mut apu, 0x4015, 0x10);
    assert_ne!(apu.read_status() & 0x10, 0, "sample playback is active");

    let request = (0..10).find_map(|_| {
        apu.tick();
        apu.take_dmc_dma_request()
    });
    assert_eq!(request, Some(0xC000));
    apu.load_dmc_sample_byte(0xFF);
    apu.tick();

    assert!(apu.irq_pending());
    assert_ne!(apu.read_status() & 0x80, 0);
    assert_eq!(apu.read_status() & 0x10, 0, "the sample has been fully fetched");

    write(&mut apu, 0x4015, 0x00); // any $4015 write acknowledges the DMC interrupt
    apu.tick();
    assert!(!apu.irq_pending());
}

#[test]
fn the_dmc_output_level_reaches_the_mixer() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4011, 0x7F);
    run(&mut apu, 20_000);
    let samples = drain(&mut apu);

    // a sudden DC step: the high-pass filters turn it into a decaying spike
    let peak = samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > 0.05, "peak {peak}");
}

#[test]
fn unused_and_unknown_registers_are_ignored() {
    let mut apu = Apu::default();
    for address in [0x4009, 0x400D, 0x4014, 0x4016, 0x4018] {
        write(&mut apu, address, 0xFF);
    }

    assert_eq!(apu.read_status(), 0);
}

#[test]
fn reset_clears_the_length_counters_but_keeps_the_triangles_and_the_output_settings() {
    let mut apu = Apu::new(44_100);
    write(&mut apu, 0x4015, 0x0F);
    write(&mut apu, 0x4003, 0x08);
    write(&mut apu, 0x4008, 0x80); // triangle halted, so its counter does not run down
    write(&mut apu, 0x400B, 0x08);
    run(&mut apu, 10_000);
    let pending = apu.buffered_samples();
    assert!(pending > 0);
    assert_eq!(apu.read_status() & 0x0F, 0b0101);

    apu.reset();

    assert_eq!(apu.read_status() & 0x0F, 0b0100, "pulse 1 was cleared, the triangle counter was left alone");
    assert_eq!(apu.sample_rate(), 44_100);
    assert_eq!(apu.buffered_samples(), pending);
}

#[test]
fn after_reset_the_length_counters_are_disabled_until_4015_is_written_again() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4015, 0x0F);
    apu.reset();

    write(&mut apu, 0x4003, 0x08);
    assert_eq!(apu.read_status() & 1, 0);

    write(&mut apu, 0x4015, 0x01);
    write(&mut apu, 0x4003, 0x08);
    assert_eq!(apu.read_status() & 1, 1);
}

#[test]
fn reset_rewrites_4017_with_the_last_value_so_the_mode_survives() {
    let mut apu = Apu::default();
    write(&mut apu, 0x4017, 0x80); // 5-step mode: no frame IRQ
    apu.reset();
    run(&mut apu, 100_000);

    assert!(!apu.irq_pending());
}

#[test]
fn reset_acknowledges_the_frame_irq() {
    let mut apu = Apu::default();
    run(&mut apu, 29_830);
    assert!(apu.irq_pending());

    apu.reset();

    assert!(!apu.irq_pending());
}

#[test]
fn the_sample_buffer_does_not_grow_without_bound() {
    let mut apu = Apu::new(48_000);

    run(&mut apu, 20 * NTSC_CPU_CLOCK_HZ as usize);

    assert!(apu.buffered_samples() <= MAX_BUFFERED_SAMPLES);
}

#[test]
fn a_pal_apu_makes_the_same_number_of_samples_per_second_from_its_slower_clock() {
    let mut apu = Apu::with_region(48_000, Region::Pal);
    run(&mut apu, Region::Pal.cpu_clock_hz() as usize);

    let samples = drain(&mut apu);

    assert!((47_999..=48_001).contains(&samples.len()), "{} samples in one PAL second", samples.len());
}

#[test]
fn a_pal_pulse_channel_plays_a_lower_note_for_the_same_register_value() {
    let measure = |region: Region| {
        let mut apu = Apu::with_region(48_000, region);
        start_pulse1(&mut apu, 253);
        run(&mut apu, 100_000);
        drain(&mut apu);
        run(&mut apu, region.cpu_clock_hz() as usize);
        rising_crossings(&drain(&mut apu), 48_000)
    };

    let ntsc = measure(Region::Ntsc);
    let pal = measure(Region::Pal);

    assert!((ntsc - 440.4).abs() < 3.0, "NTSC {ntsc}");
    assert!((pal - 409.1).abs() < 3.0, "PAL: 1662607 / (16 * 254) = 409.1 Hz, measured {pal}");
}

#[test]
fn expansion_audio_from_the_cartridge_is_added_to_the_mix() {
    let mut silent = Apu::new(48_000);
    let mut with_expansion = Apu::new(48_000);
    for _ in 0..20_000 {
        silent.tick();
        with_expansion.tick_with_expansion(0.2);
    }

    // the output filters remove the constant (DC) part over time, but the first samples show the added level
    let first_silent = drain(&mut silent);
    let first_expansion = drain(&mut with_expansion);

    let peak = |samples: &[f32]| samples.iter().cloned().fold(f32::MIN, f32::max);
    assert!(peak(&first_expansion) - peak(&first_silent) > 0.1);
}
