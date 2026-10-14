use super::*;

const DOTS_PER_FRAME: u64 = 341 * 262;
const PRE_RENDER_LINE: u16 = 261;

/// NROM with CHR-RAM, so tests can draw their own tiles.
fn cartridge(vertical: bool) -> Cartridge {
    let mut data = vec![0u8; 16 + 16384];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1;
    data[6] = vertical as u8;
    Cartridge::from_bytes(&data).unwrap()
}

fn setup() -> (PPU, Cartridge) {
    (PPU::new(), cartridge(true))
}

fn write(ppu: &mut PPU, cart: &mut Cartridge, register: u16, data: u8) {
    ppu.write_register(register, data, cart);
}

fn read(ppu: &mut PPU, cart: &mut Cartridge, register: u16) -> u8 {
    ppu.read_register(register, cart)
}

fn set_address(ppu: &mut PPU, cart: &mut Cartridge, address: u16) {
    write(ppu, cart, 6, (address >> 8) as u8);
    write(ppu, cart, 6, address as u8);
}

/// Writes through PPUADDR/PPUDATA, the way a game would.
fn write_vram(ppu: &mut PPU, cart: &mut Cartridge, address: u16, bytes: &[u8]) {
    set_address(ppu, cart, address);
    for &byte in bytes {
        write(ppu, cart, 7, byte);
    }
}

fn run_frame(ppu: &mut PPU, cart: &mut Cartridge) {
    loop {
        ppu.tick(cart);
        if ppu.take_frame_complete() {
            return;
        }
    }
}

/// Runs two frames so the second one is drawn with a properly prefetched pipeline and t copied into v.
fn render(ppu: &mut PPU, cart: &mut Cartridge) {
    set_address(ppu, cart, 0); // leave v/t at zero scroll
    run_frame(ppu, cart);
    run_frame(ppu, cart);
}

fn pixel(ppu: &PPU, x: usize, y: usize) -> u8 {
    ppu.color_indices()[y * SCREEN_WIDTH + x]
}

fn tick_until(ppu: &mut PPU, cart: &mut Cartridge, condition: impl Fn(&PPU) -> bool) {
    for _ in 0..DOTS_PER_FRAME * 3 {
        if condition(ppu) {
            return;
        }
        ppu.tick(cart);
    }
    panic!("condition never became true");
}

// ---- registers and VRAM ---------------------------------------------------------------------------------------

#[test]
fn write_only_registers_read_back_the_io_latch() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, 0x55);

    assert_eq!(read(&mut ppu, &mut cart, 0), 0x55);
    assert_eq!(read(&mut ppu, &mut cart, 5), 0x55);
}

#[test]
fn registers_mirror_every_eight_bytes() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 0x0008 | 1, 0x1E);

    assert_eq!(ppu.mask, 0x1E);
}

#[test]
fn ppustatus_low_bits_are_open_bus_and_a_read_clears_vblank_and_the_latch() {
    let (mut ppu, mut cart) = setup();
    ppu.status = STATUS_VBLANK | STATUS_SPRITE_ZERO_HIT;
    write(&mut ppu, &mut cart, 5, 0x00); // w = 1
    ppu.io_latch = 0x1F;

    assert_eq!(read(&mut ppu, &mut cart, 2), 0xC0 | 0x1F);
    assert_eq!(read(&mut ppu, &mut cart, 2), 0x40 | 0x1F, "vblank flag was cleared by the first read");
    assert!(!ppu.write_latch);
}

fn wait_dots(ppu: &mut PPU, cart: &mut Cartridge, dots: u32) {
    for _ in 0..dots {
        ppu.tick(cart);
    }
}

#[test]
fn ppudata_reads_are_buffered_by_one() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x2000, &[0xAA, 0xBB, 0xCC]);

    set_address(&mut ppu, &mut cart, 0x2000);
    wait_dots(&mut ppu, &mut cart, 12);

    assert_eq!(read(&mut ppu, &mut cart, 7), 0x00, "first read returns the stale buffer");
    wait_dots(&mut ppu, &mut cart, 12);
    assert_eq!(read(&mut ppu, &mut cart, 7), 0xAA);
    wait_dots(&mut ppu, &mut cart, 12);
    assert_eq!(read(&mut ppu, &mut cart, 7), 0xBB);
    wait_dots(&mut ppu, &mut cart, 12);
    assert_eq!(read(&mut ppu, &mut cart, 7), 0xCC);
}

#[test]
fn a_second_ppudata_read_one_cpu_cycle_later_sees_the_old_buffer_but_still_advances() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x2000, &[0x11, 0x22, 0x33, 0x44]);
    set_address(&mut ppu, &mut cart, 0x2000);
    wait_dots(&mut ppu, &mut cart, 12);
    read(&mut ppu, &mut cart, 7); // returns the stale buffer; buffer = $11, v = $2001
    wait_dots(&mut ppu, &mut cart, 12);
    assert_eq!(read(&mut ppu, &mut cart, 7), 0x11); // buffer = $22, v = $2002

    wait_dots(&mut ppu, &mut cart, 3); // the next CPU cycle
    assert_eq!(read(&mut ppu, &mut cart, 7), 0x11, "the buffer has not been reloaded yet");

    wait_dots(&mut ppu, &mut cart, 12);
    assert_eq!(read(&mut ppu, &mut cart, 7), 0x33, "$2001's byte was skipped: the address advanced twice");
}

#[test]
fn ppudata_increment_is_1_or_32() {
    let (mut ppu, mut cart) = setup();

    set_address(&mut ppu, &mut cart, 0x2000);
    write(&mut ppu, &mut cart, 7, 1);
    assert_eq!(ppu.v, 0x2001);

    write(&mut ppu, &mut cart, 0, CTRL_VRAM_INCREMENT);
    write(&mut ppu, &mut cart, 7, 1);
    assert_eq!(ppu.v, 0x2021);
}

#[test]
fn palette_ram_mirrors_the_sprite_backdrop_entries() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x3F10, &[0x30]);
    write_vram(&mut ppu, &mut cart, 0x3F04, &[0x12]);
    write_vram(&mut ppu, &mut cart, 0x3F11, &[0x21]);

    assert_eq!(ppu.palette_ram[0x00], 0x30);
    assert_eq!(ppu.palette_ram[palette_index(0x3F14)], 0x12);
    assert_eq!(ppu.palette_ram[0x11], 0x21, "$3F11 is not a mirror of $3F01");
    assert_eq!(ppu.palette_ram[0x01], POWER_UP_PALETTE[0x01], "$3F11 does not touch $3F01");
    assert_eq!(palette_index(0x3F20), 0, "palette RAM repeats every 32 bytes");
}

#[test]
fn palette_reads_are_immediate_and_fill_the_buffer_from_the_nametable_underneath() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x2F01, &[0x77]);
    write_vram(&mut ppu, &mut cart, 0x3F01, &[0x15]);

    set_address(&mut ppu, &mut cart, 0x3F01);
    assert_eq!(read(&mut ppu, &mut cart, 7) & 0x3F, 0x15, "no priming read needed");

    set_address(&mut ppu, &mut cart, 0x2000);
    assert_eq!(read(&mut ppu, &mut cart, 7), 0x77, "buffer holds $2F01");
}

#[test]
fn nametable_mirroring_follows_the_cartridge() {
    let mut cart_v = cartridge(true);
    let mut ppu = PPU::new();
    ppu.bus_write(&mut cart_v, 0x2000, 0x11);
    assert_eq!(ppu.bus_read(&mut cart_v, 0x2800), 0x11, "vertical: $2000 = $2800");
    assert_eq!(ppu.bus_read(&mut cart_v, 0x2400), 0x00);

    let mut cart_h = cartridge(false);
    let mut ppu = PPU::new();
    ppu.bus_write(&mut cart_h, 0x2000, 0x22);
    assert_eq!(ppu.bus_read(&mut cart_h, 0x2400), 0x22, "horizontal: $2000 = $2400");
    assert_eq!(ppu.bus_read(&mut cart_h, 0x2800), 0x00);
    assert_eq!(ppu.bus_read(&mut cart_h, 0x3000), 0x22, "$3000-$3EFF mirrors $2000-$2EFF");
}

#[test]
fn pattern_tables_reach_chr_ram_through_the_cartridge() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x0010, &[1, 2, 3]);

    assert_eq!(cart.ppu_read(0x0011), 2);
}

#[test]
fn scroll_and_address_writes_update_t_x_and_w() {
    let (mut ppu, mut cart) = setup();

    write(&mut ppu, &mut cart, 5, 0b0111_1101);
    assert!(ppu.write_latch);
    assert_eq!(ppu.fine_x, 5);
    assert_eq!(ppu.t & 0x1F, 0b01111);

    write(&mut ppu, &mut cart, 5, 0b0101_1110);
    assert!(!ppu.write_latch);
    assert_eq!(ppu.t, (6 << 12) | (0b01011 << 5) | 0b01111);

    write(&mut ppu, &mut cart, 0, 0x03);
    assert_eq!(ppu.t & 0x0C00, 0x0C00, "PPUCTRL bits 0-1 become t bits 10-11");

    write(&mut ppu, &mut cart, 6, 0xFF);
    write(&mut ppu, &mut cart, 6, 0x34);
    assert_eq!(ppu.t, 0x3F34, "bits 14 and 15 of the high byte are dropped");
    assert_eq!(ppu.v, 0x3F34);
}

#[test]
fn oam_registers() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 3, 0x10);
    write(&mut ppu, &mut cart, 4, 0xAA);
    write(&mut ppu, &mut cart, 4, 0xBB);
    write(&mut ppu, &mut cart, 4, 0xFF); // attribute byte
    assert_eq!(ppu.oam_addr, 0x13);

    write(&mut ppu, &mut cart, 3, 0x11);
    assert_eq!(read(&mut ppu, &mut cart, 4), 0xBB, "reads do not increment the address");
    assert_eq!(ppu.oam_addr, 0x11);

    write(&mut ppu, &mut cart, 3, 0x12);
    assert_eq!(read(&mut ppu, &mut cart, 4), 0xE3, "attribute bits 2-4 read as zero");
}

#[test]
fn oam_writes_are_ignored_while_rendering() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND);
    write(&mut ppu, &mut cart, 3, 0x00);

    write(&mut ppu, &mut cart, 4, 0x42);

    assert_eq!(ppu.oam[0], 0);
    assert_eq!(ppu.oam_addr, 4);
}

// ---- timing ---------------------------------------------------------------------------------------------------

#[test]
fn vblank_starts_at_scanline_241_dot_1() {
    let (mut ppu, mut cart) = setup();

    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);

    assert_eq!((ppu.scanline(), ppu.dot()), (241, 2), "flag set while processing (241, 1)");
    assert!(ppu.take_frame_complete());
}

#[test]
fn nmi_fires_once_per_vblank_when_enabled() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 0, CTRL_NMI_ENABLE);

    assert!(!ppu.take_nmi());
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);

    assert!(ppu.take_nmi());
    assert!(!ppu.take_nmi(), "an edge is consumed once");
}

#[test]
fn no_nmi_when_disabled() {
    let (mut ppu, mut cart) = setup();
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);

    assert!(!ppu.take_nmi());
}

#[test]
fn enabling_nmi_during_vblank_triggers_it_and_toggling_retriggers() {
    let (mut ppu, mut cart) = setup();
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);

    write(&mut ppu, &mut cart, 0, CTRL_NMI_ENABLE);
    assert!(ppu.take_nmi());

    write(&mut ppu, &mut cart, 0, CTRL_NMI_ENABLE);
    assert!(!ppu.take_nmi(), "no new edge while already enabled");

    write(&mut ppu, &mut cart, 0, 0);
    write(&mut ppu, &mut cart, 0, CTRL_NMI_ENABLE);
    assert!(ppu.take_nmi());
}

#[test]
fn reading_ppustatus_before_enabling_nmi_prevents_it() {
    let (mut ppu, mut cart) = setup();
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);

    read(&mut ppu, &mut cart, 2);
    write(&mut ppu, &mut cart, 0, CTRL_NMI_ENABLE);

    assert!(!ppu.take_nmi());
}

#[test]
fn pre_render_line_clears_the_status_flags() {
    let (mut ppu, mut cart) = setup();
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);
    ppu.status |= STATUS_SPRITE_ZERO_HIT | STATUS_SPRITE_OVERFLOW;

    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == PRE_RENDER_LINE && ppu.dot() == 2);

    assert_eq!(ppu.status, 0);
}

#[test]
fn frame_is_89342_dots_and_odd_frames_are_one_dot_shorter_while_rendering() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND);

    let mut intervals = Vec::new();
    run_frame(&mut ppu, &mut cart);
    for _ in 0..4 {
        let mut dots = 0u64;
        loop {
            ppu.tick(&mut cart);
            dots += 1;
            if ppu.take_frame_complete() {
                break;
            }
        }
        intervals.push(dots);
    }

    assert_eq!(intervals, [DOTS_PER_FRAME, DOTS_PER_FRAME - 1, DOTS_PER_FRAME, DOTS_PER_FRAME - 1]);
}

#[test]
fn frames_are_not_shortened_when_rendering_is_off() {
    let (mut ppu, mut cart) = setup();

    run_frame(&mut ppu, &mut cart);
    for _ in 0..3 {
        let mut dots = 0u64;
        loop {
            ppu.tick(&mut cart);
            dots += 1;
            if ppu.take_frame_complete() {
                break;
            }
        }
        assert_eq!(dots, DOTS_PER_FRAME);
    }
}

#[test]
fn scroll_registers_are_reloaded_from_t_on_the_pre_render_line() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND);
    ppu.t = (3 << 12) | (5 << 5) | 7;
    ppu.v = 0;

    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == PRE_RENDER_LINE && ppu.dot() == 306);

    assert_eq!(ppu.v & 0x7BE0, ppu.t & 0x7BE0, "vertical bits copied during dots 280-304");
}

#[test]
fn coarse_x_and_y_increment_with_nametable_wrap() {
    let (mut ppu, _) = setup();

    ppu.v = 0x001F;
    ppu.increment_coarse_x();
    assert_eq!(ppu.v, 0x0400, "coarse X wraps and switches the horizontal nametable");

    ppu.v = 0x7000 | (29 << 5);
    ppu.increment_y();
    assert_eq!(ppu.v, 0x0800, "coarse Y 29 wraps and switches the vertical nametable");

    ppu.v = 0x7000 | (31 << 5);
    ppu.increment_y();
    assert_eq!(ppu.v, 0, "coarse Y 31 wraps without switching");

    ppu.v = 0x1000;
    ppu.increment_y();
    assert_eq!(ppu.v, 0x2000);
}

// ---- background rendering -------------------------------------------------------------------------------------

/// Tile `index` with every row equal to (plane 0, plane 1).
fn solid_tile(ppu: &mut PPU, cart: &mut Cartridge, table: u16, index: u16, plane0: u8, plane1: u8) {
    let address = table + index * 16;
    write_vram(ppu, cart, address, &[plane0; 8]);
    write_vram(ppu, cart, address + 8, &[plane1; 8]);
}

#[test]
fn background_tile_is_drawn_with_its_palette() {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xFF, 0x00); // pixel value 1
    write_vram(&mut ppu, &mut cart, 0x2000, &[1]);
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x30]);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT);

    render(&mut ppu, &mut cart);

    for x in 0..8 {
        assert_eq!(pixel(&ppu, x, 0), 0x30, "x={x}");
        assert_eq!(pixel(&ppu, x, 7), 0x30);
    }
    assert_eq!(pixel(&ppu, 8, 0), 0x0F, "next tile is tile 0: transparent");
    assert_eq!(pixel(&ppu, 0, 8), 0x0F);
}

#[test]
fn attribute_table_selects_the_palette_per_16_pixels() {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xFF, 0x00);
    write_vram(&mut ppu, &mut cart, 0x2000, &[1, 1, 1, 1]);
    write_vram(&mut ppu, &mut cart, 0x2020, &[1, 1, 1, 1]);
    write_vram(&mut ppu, &mut cart, 0x23C0, &[0b00_00_11_01]); // top-left 2x2 = palette 1, top-right = palette 3
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x01, 0, 0, 0x0F, 0x02, 0, 0, 0, 0, 0, 0, 0x0F, 0x04]);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT);

    render(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 0, 0), 0x02, "palette 1");
    assert_eq!(pixel(&ppu, 15, 15), 0x02);
    assert_eq!(pixel(&ppu, 16, 0), 0x04, "palette 3");
    assert_eq!(pixel(&ppu, 31, 15), 0x04);
}

#[test]
fn fine_x_scroll_shifts_the_picture() {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0b1010_1010, 0x00);
    write_vram(&mut ppu, &mut cart, 0x2000, &[1]);
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x30]);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT);
    set_address(&mut ppu, &mut cart, 0); // t also holds nametable bits left over from the VRAM writes
    write(&mut ppu, &mut cart, 5, 1); // fine x = 1
    write(&mut ppu, &mut cart, 5, 0);

    run_frame(&mut ppu, &mut cart);
    run_frame(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 0, 0), 0x0F, "pixel 1 of the tile is transparent");
    assert_eq!(pixel(&ppu, 1, 0), 0x30);
    assert_eq!(pixel(&ppu, 2, 0), 0x0F);
}

#[test]
fn coarse_x_scroll_moves_whole_tiles_and_wraps_into_the_next_nametable() {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xFF, 0x00);
    solid_tile(&mut ppu, &mut cart, 0, 2, 0xFF, 0xFF); // pixel value 3
    write_vram(&mut ppu, &mut cart, 0x2001, &[2]); // column 1 of nametable 0
    write_vram(&mut ppu, &mut cart, 0x2400, &[1]); // column 0 of nametable 1
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x11, 0x12, 0x13]);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT);
    set_address(&mut ppu, &mut cart, 0);
    write(&mut ppu, &mut cart, 5, 8); // coarse x = 1
    write(&mut ppu, &mut cart, 5, 0);

    run_frame(&mut ppu, &mut cart);
    run_frame(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 0, 0), 0x13, "column 1 is now at the left edge");
    assert_eq!(pixel(&ppu, 8, 0), 0x0F);
    assert_eq!(pixel(&ppu, 31 * 8 - 8, 0), 0x0F);
    assert_eq!(pixel(&ppu, 255 - 7, 0), 0x11, "column 0 of the second nametable appears at the right edge");
}

#[test]
fn background_pattern_table_select() {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0x1000, 1, 0xFF, 0x00);
    write_vram(&mut ppu, &mut cart, 0x2000, &[1]);
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x30]);
    write(&mut ppu, &mut cart, 0, CTRL_BACKGROUND_TABLE);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT);

    render(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 0, 0), 0x30);
}

#[test]
fn left_eight_pixels_can_be_clipped() {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xFF, 0x00);
    write_vram(&mut ppu, &mut cart, 0x2000, &[1, 1]);
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x30]);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND);

    render(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 7, 0), 0x0F);
    assert_eq!(pixel(&ppu, 8, 0), 0x30);
}

#[test]
fn rendering_off_shows_the_backdrop_or_the_palette_entry_v_points_at() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x21, 0x16]);

    set_address(&mut ppu, &mut cart, 0);
    run_frame(&mut ppu, &mut cart);
    assert_eq!(pixel(&ppu, 100, 100), 0x21);

    set_address(&mut ppu, &mut cart, 0x3F01);
    run_frame(&mut ppu, &mut cart);
    assert_eq!(pixel(&ppu, 100, 100), 0x16, "palette hack: v inside palette RAM");
}

#[test]
fn greyscale_masks_the_colour_to_the_grey_column() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x16]);
    write(&mut ppu, &mut cart, 1, MASK_GREYSCALE);

    set_address(&mut ppu, &mut cart, 0);
    run_frame(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 0, 0), 0x10);
}

#[test]
fn framebuffer_holds_rgba_from_the_palette() {
    let (mut ppu, mut cart) = setup();
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x30]);
    set_address(&mut ppu, &mut cart, 0);

    run_frame(&mut ppu, &mut cart);

    assert_eq!(&ppu.framebuffer()[0..4], &[0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(ppu.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT * 4);
}

// ---- sprites --------------------------------------------------------------------------------------------------

fn write_oam(ppu: &mut PPU, cart: &mut Cartridge, sprite: usize, y: u8, tile: u8, attributes: u8, x: u8) {
    write(ppu, cart, 3, (sprite * 4) as u8);
    for byte in [y, tile, attributes, x] {
        write(ppu, cart, 4, byte);
    }
}

fn sprite_setup() -> (PPU, Cartridge) {
    let (mut ppu, mut cart) = setup();
    solid_tile(&mut ppu, &mut cart, 0, 2, 0xFF, 0x00); // sprite pixel value 1
    solid_tile(&mut ppu, &mut cart, 0, 3, 0xFF, 0xFF); // value 3
    write_vram(&mut ppu, &mut cart, 0x3F00, &[0x0F, 0x01, 0x02, 0x03]);
    write_vram(&mut ppu, &mut cart, 0x3F10, &[0x0F, 0x2A, 0x2B, 0x2C]);
    write_vram(&mut ppu, &mut cart, 0x3F14, &[0x0F, 0x15, 0x16, 0x17]);
    (ppu, cart)
}

/// Turns sprite rendering on (after OAM is filled) and renders two frames.
fn render_sprites(ppu: &mut PPU, cart: &mut Cartridge) {
    write(ppu, cart, 1, MASK_SPRITES | MASK_SPRITES_LEFT);
    render(ppu, cart);
}

#[test]
fn sprite_appears_one_scanline_below_its_y_coordinate() {
    let (mut ppu, mut cart) = sprite_setup();
    write_oam(&mut ppu, &mut cart, 0, 9, 2, 0, 16);
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 16, 10), 0x2A);
    assert_eq!(pixel(&ppu, 23, 17), 0x2A);
    assert_eq!(pixel(&ppu, 15, 10), 0x0F);
    assert_eq!(pixel(&ppu, 24, 10), 0x0F);
    assert_eq!(pixel(&ppu, 16, 9), 0x0F, "not on its own Y line");
    assert_eq!(pixel(&ppu, 16, 18), 0x0F);
}

#[test]
fn sprite_palette_and_horizontal_flip() {
    let (mut ppu, mut cart) = sprite_setup();
    solid_tile(&mut ppu, &mut cart, 0, 4, 0b1000_0000, 0x00); // only the leftmost pixel
    write_oam(&mut ppu, &mut cart, 0, 19, 4, 0b0100_0001, 32); // flip H, palette 1
    write_oam(&mut ppu, &mut cart, 1, 19, 4, 0b0000_0001, 64); // no flip, palette 1
    for sprite in 2..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 39, 20), 0x15, "flipped: the pixel moves to the right edge");
    assert_eq!(pixel(&ppu, 32, 20), 0x0F);
    assert_eq!(pixel(&ppu, 64, 20), 0x15);
    assert_eq!(pixel(&ppu, 71, 20), 0x0F);
}

#[test]
fn vertical_flip_reverses_the_rows() {
    let (mut ppu, mut cart) = sprite_setup();
    // only the first row of tile 5 is lit
    write_vram(&mut ppu, &mut cart, 0x0050, &[0xFF, 0, 0, 0, 0, 0, 0, 0]);
    write_oam(&mut ppu, &mut cart, 0, 19, 5, 0b1000_0000, 32);
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 32, 20), 0x0F);
    assert_eq!(pixel(&ppu, 32, 27), 0x2A, "the lit row is now the bottom one");
}

#[test]
fn lower_numbered_sprites_have_priority() {
    let (mut ppu, mut cart) = sprite_setup();
    write_oam(&mut ppu, &mut cart, 0, 9, 3, 0, 16); // value 3 -> $2C
    write_oam(&mut ppu, &mut cart, 1, 9, 2, 0, 16); // value 1 -> $2A, same spot
    for sprite in 2..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 16, 10), 0x2C);
}

#[test]
fn sprites_behind_the_background_only_show_through_transparent_pixels() {
    let (mut ppu, mut cart) = sprite_setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xF0, 0x00); // left 4 pixels opaque
    write_vram(&mut ppu, &mut cart, 0x2000, &[1]);
    write_oam(&mut ppu, &mut cart, 0, 0, 2, 0b0010_0000, 0); // behind background
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT | MASK_SPRITES | MASK_SPRITES_LEFT);

    render(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 0, 1), 0x01, "background wins where it is opaque");
    assert_eq!(pixel(&ppu, 5, 1), 0x2A, "sprite shows where the background is transparent");
}

#[test]
fn eight_by_sixteen_sprites_use_two_tiles_and_the_table_from_bit_0() {
    let (mut ppu, mut cart) = sprite_setup();
    solid_tile(&mut ppu, &mut cart, 0x1000, 2, 0xFF, 0x00); // top half, value 1
    solid_tile(&mut ppu, &mut cart, 0x1000, 3, 0x00, 0xFF); // bottom half, value 2
    write(&mut ppu, &mut cart, 0, CTRL_SPRITE_SIZE);
    write_oam(&mut ppu, &mut cart, 0, 19, 3, 0, 32); // tile byte 3: table $1000, tiles 2 and 3
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 32, 20), 0x2A, "top tile");
    assert_eq!(pixel(&ppu, 32, 27), 0x2A);
    assert_eq!(pixel(&ppu, 32, 28), 0x2B, "bottom tile");
    assert_eq!(pixel(&ppu, 32, 35), 0x2B);
    assert_eq!(pixel(&ppu, 32, 36), 0x0F);
}

#[test]
fn only_eight_sprites_per_scanline_are_drawn_and_the_ninth_sets_overflow() {
    let (mut ppu, mut cart) = sprite_setup();
    for sprite in 0..9 {
        write_oam(&mut ppu, &mut cart, sprite, 19, 2, 0, (sprite * 16) as u8);
    }
    for sprite in 9..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);

    assert_eq!(pixel(&ppu, 7 * 16, 20), 0x2A, "eighth sprite");
    assert_eq!(pixel(&ppu, 8 * 16, 20), 0x0F, "ninth sprite is dropped");
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_SPRITE_OVERFLOW != 0);
}

#[test]
fn eight_sprites_do_not_set_overflow() {
    let (mut ppu, mut cart) = sprite_setup();
    for sprite in 0..8 {
        write_oam(&mut ppu, &mut cart, sprite, 19, 2, 0, (sprite * 16) as u8);
    }
    for sprite in 8..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }

    render_sprites(&mut ppu, &mut cart);
    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 250);

    assert_eq!(ppu.status & STATUS_SPRITE_OVERFLOW, 0);
}

#[test]
fn sprite_zero_hit_is_raised_at_the_overlapping_pixel() {
    let (mut ppu, mut cart) = sprite_setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xFF, 0x00);
    write_vram(&mut ppu, &mut cart, 0x2000, &[1, 1]);
    write_oam(&mut ppu, &mut cart, 0, 3, 2, 0, 4); // appears from y=4, x=4
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT | MASK_SPRITES | MASK_SPRITES_LEFT);
    set_address(&mut ppu, &mut cart, 0);

    run_frame(&mut ppu, &mut cart);
    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == PRE_RENDER_LINE && ppu.dot() == 2);
    assert_eq!(ppu.status & STATUS_SPRITE_ZERO_HIT, 0, "cleared on the pre-render line");

    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_SPRITE_ZERO_HIT != 0);

    assert_eq!((ppu.scanline(), ppu.dot()), (4, 6), "x=4 is dot 5; the flag is visible after that dot");
}

#[test]
fn no_sprite_zero_hit_over_a_transparent_background_or_at_x_255() {
    let (mut ppu, mut cart) = sprite_setup();
    write_oam(&mut ppu, &mut cart, 0, 3, 2, 0, 4);
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT | MASK_SPRITES | MASK_SPRITES_LEFT);
    render(&mut ppu, &mut cart);
    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 100);
    assert_eq!(ppu.status & STATUS_SPRITE_ZERO_HIT, 0, "background is transparent");

    // opaque background everywhere, sprite 0 only at the last column
    let (mut ppu, mut cart) = sprite_setup();
    solid_tile(&mut ppu, &mut cart, 0, 1, 0xFF, 0x00);
    write_vram(&mut ppu, &mut cart, 0x2000, &[1; 32]);
    write_oam(&mut ppu, &mut cart, 0, 3, 2, 0, 255);
    for sprite in 1..64 {
        write_oam(&mut ppu, &mut cart, sprite, 0xFF, 0, 0, 0);
    }
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND | MASK_BACKGROUND_LEFT | MASK_SPRITES | MASK_SPRITES_LEFT);
    render(&mut ppu, &mut cart);
    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 100);
    assert_eq!(ppu.status & STATUS_SPRITE_ZERO_HIT, 0, "never at x=255");
}

#[test]
fn dma_writes_go_through_oamdata() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 3, 0x20);

    ppu.dma_write(0x11);
    ppu.dma_write(0x22);

    assert_eq!(ppu.oam[0x20], 0x11);
    assert_eq!(ppu.oam[0x21], 0x22);
    assert_eq!(ppu.oam_addr, 0x22);
}

// ---- PAL ------------------------------------------------------------------------------------------------------

#[test]
fn a_pal_frame_has_312_scanlines_and_no_odd_frame_skip() {
    let mut cart = cartridge(true);
    let mut ppu = PPU::with_region(Region::Pal);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND);

    run_frame(&mut ppu, &mut cart);
    let mut intervals = Vec::new();
    for _ in 0..3 {
        let mut dots = 0u64;
        loop {
            ppu.tick(&mut cart);
            dots += 1;
            if ppu.take_frame_complete() {
                break;
            }
        }
        intervals.push(dots);
    }

    assert_eq!(intervals, [341 * 312; 3]);
}

#[test]
fn the_pal_pre_render_line_is_311() {
    let mut cart = cartridge(true);
    let mut ppu = PPU::with_region(Region::Pal);
    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);
    ppu.status |= STATUS_SPRITE_ZERO_HIT;

    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 261 && ppu.dot() == 5);
    assert_ne!(ppu.status & STATUS_VBLANK, 0, "vblank lasts 70 lines on PAL: still set on line 261");

    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 311 && ppu.dot() == 2);
    assert_eq!(ppu.status, 0);
    assert_eq!(ppu.region(), Region::Pal);
}

#[test]
fn dendy_has_51_post_render_lines_and_sets_vblank_on_line_291() {
    let mut cart = cartridge(true);
    let mut ppu = PPU::with_region(Region::Dendy);

    tick_until(&mut ppu, &mut cart, |ppu| ppu.status & STATUS_VBLANK != 0);
    assert_eq!((ppu.scanline(), ppu.dot()), (291, 2));

    ppu.status |= STATUS_SPRITE_ZERO_HIT;
    tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 311 && ppu.dot() == 2);
    assert_eq!(ppu.status, 0, "the pre-render line is 311, as on PAL");
}

#[test]
fn a_dendy_frame_has_312_lines_of_3_dots_per_cpu_cycle_and_no_odd_frame_skip() {
    let mut cart = cartridge(true);
    let mut ppu = PPU::with_region(Region::Dendy);
    write(&mut ppu, &mut cart, 1, MASK_BACKGROUND);

    let mut dots = 0u32;
    for _ in 0..2 {
        tick_until(&mut ppu, &mut cart, |ppu| ppu.scanline() == 0 && ppu.dot() == 10);
        for _ in 0..(312 * 341) {
            ppu.tick(&mut cart);
            dots += 1;
        }
    }

    assert_eq!(dots, 2 * 312 * 341);
    assert_eq!((ppu.scanline(), ppu.dot()), (0, 10), "every frame has exactly 312 * 341 dots");
}

// ---- I/O latch decay ------------------------------------------------------------------------------------------

#[test]
fn the_io_latch_keeps_its_bits_for_a_while_and_then_they_decay_to_zero() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, 0xFF);

    assert_eq!(read(&mut ppu, &mut cart, 0), 0xFF, "write-only registers read back the latch");

    ppu.frame += LATCH_DECAY_FRAMES - 1;
    assert_eq!(read(&mut ppu, &mut cart, 0), 0xFF, "not decayed yet");

    ppu.frame += 1;
    assert_eq!(read(&mut ppu, &mut cart, 0), 0x00, "36 frames after the last refresh every bit is gone");
}

#[test]
fn reading_a_register_refreshes_only_the_bits_it_drives() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, 0xFF);
    ppu.frame += LATCH_DECAY_FRAMES - 1;

    ppu.status = STATUS_VBLANK;
    read(&mut ppu, &mut cart, 2); // drives bits 7-5 only

    ppu.frame += 1;
    assert_eq!(
        read(&mut ppu, &mut cart, 0),
        0x80,
        "bit 7 was refreshed by the read; bits 6-5 were driven low; bits 4-0 decayed"
    );
}

#[test]
fn ppustatus_low_bits_come_from_the_decaying_latch() {
    let (mut ppu, mut cart) = setup();
    write(&mut ppu, &mut cart, 1, 0x1F);
    assert_eq!(read(&mut ppu, &mut cart, 2) & 0x1F, 0x1F);

    ppu.frame += LATCH_DECAY_FRAMES;
    assert_eq!(read(&mut ppu, &mut cart, 2) & 0x1F, 0x00);
}

#[test]
fn custom_mirroring_maps_each_nametable_to_its_own_page() {
    let mirroring = Mirroring::Custom([1, 0, 3, 1]);

    assert_eq!(nametable_offset(0x2005, mirroring), 0x400 + 5);
    assert_eq!(nametable_offset(0x2405, mirroring), 5);
    assert_eq!(nametable_offset(0x2805, mirroring), 3 * 0x400 + 5);
    assert_eq!(nametable_offset(0x2C05, mirroring), 0x400 + 5);
    assert_eq!(nametable_offset(0x3C05, mirroring), 0x400 + 5, "$3000-$3EFF mirror $2000-$2EFF");
}
