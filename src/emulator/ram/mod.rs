pub struct RAM {
    memory: [u8; 0x800] // 2KB RAM
}

impl RAM {
    pub fn new() -> Self {
        RAM { memory: [0; 0x800] }
    }

    pub fn read(&self, address: u16) -> u8 {
        self.memory[(address & 0x07FF) as usize]
    }

    pub fn write(&mut self, address: u16, data: u8) {
        self.memory[(address & 0x07FF) as usize] = data;
    }
}