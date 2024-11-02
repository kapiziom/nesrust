pub trait CpuBus {
    fn read(&self, addr: u16) -> u8;
    fn read_u16(&self, addr: u16) -> u16;
    fn write(&mut self, addr: u16, value: u8);
    fn write_u16(&mut self, addr: u16, value: u16);
}

pub (super) fn read_u16_core(bus: &dyn CpuBus, addr: u16) -> u16 {
    let low = bus.read(addr) as u16;
    let high = bus.read(addr.wrapping_add(1)) as u16;
    (high << 8) | low
}

pub (super) fn write_u16_core(bus: &mut dyn CpuBus, addr: u16, value: u16) {
    let low = (value & 0xFF) as u8;
    let high = ((value >> 8) & 0xFF) as u8;
    bus.write(addr, low);
    bus.write(addr.wrapping_add(1), high);
}