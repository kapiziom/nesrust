// Picture Processing Unit
pub struct PPU {
    vram: [u8; 2048],
    oam: [u8; 256],
    palette: [u8; 32],
    scanline: i16,
    pub cycles: u16,
    frame_complete: bool,
    nmi_flag: bool,
}

impl PPU {
    pub fn new() -> Self {
        PPU {
            vram: [0; 2048],
            oam: [0; 256],
            palette: [0; 32],
            scanline: 0,
            cycles: 0,
            frame_complete: false,
            nmi_flag: false,
        }
    }

    pub fn read(&self, address: u16) -> u8 {
        match address & 0x2007 {
            0x2000..=0x2FFF => self.vram[((address - 0x2000) & 0x0FFF) as usize],
            0x3000..=0x3EFF => self.vram[((address - 0x3000) & 0x0FFF) as usize], // Mirror of 0x2000-0x2EFF
            0x3F00..=0x3FFF => self.palette[((address - 0x3F00) & 0x1F) as usize],
            _ => panic!("Disallowed address read PPU: {:04X}", address),
        }
    }

    pub fn write(&mut self, address: u16, data: u8) {
        match address & 0x2007 {
            0x2000..=0x2FFF => self.vram[((address - 0x2000) & 0x0FFF) as usize] = data,
            0x3000..=0x3EFF => self.vram[((address - 0x3000) & 0x0FFF) as usize] = data,
            0x3F00..=0x3FFF => self.palette[((address - 0x3F00) & 0x1F) as usize] = data,
            _ => panic!("Disallowed address write PPU: {:04X}", address),
        }
    }

    pub fn tick(&mut self) {
        self.cycles += 1;

        if self.cycles >= 341 {
            self.cycles = 0;
            self.scanline += 1;

            if self.scanline == 241 {
                self.nmi_flag = true;
            } else if self.scanline >= 261 {
                self.scanline = 0;
                self.frame_complete = true;
            }
        }
    }

    pub fn is_frame_complete(&mut self) -> bool {
        if self.frame_complete {
            self.frame_complete = false;
            true
        } else {
            false
        }
    }

    pub fn fetch_nmi(&mut self) -> bool {
        if self.nmi_flag {
            self.nmi_flag = false;
            true
        } else {
            false
        }
    }
}