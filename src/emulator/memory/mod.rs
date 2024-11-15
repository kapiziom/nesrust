

pub struct Memory {
    pub (super) w_ram: [u8; 0x800], // 2KB internal RAM
    pub (super) prg_rom: Vec<u8>, // ROM
}

impl Memory {
    pub fn new() -> Self {
        Self {
            w_ram: [0; 0x800],
            prg_rom: Vec::new(),
        }
    }

    pub fn load_rom(&mut self, rom_data: Vec<u8>) {
        self.prg_rom = rom_data;
    }
}