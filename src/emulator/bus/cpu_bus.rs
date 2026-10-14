pub use mos6502::cpu_bus::CpuBus;

use crate::emulator::bus::Bus;

impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let value = match addr {
            // Internal RAM + mirroring
            0x0000..=0x1FFF => self.ram.read(addr & 0x07FF),
            // PPU registers + mirroring
            0x2000..=0x3FFF => self.ppu.read_register(addr & 0x7, &mut self.cartridge),
            // APU status. Bit 5 is open bus, and reading it does not disturb the CPU's data bus latch.
            0x4015 => {
                return (self.apu.read_status() & !0x20) | (self.last_bus_value & 0x20);
            }
            // other APU registers are write-only
            0x4000..=0x4014 => self.last_bus_value,
            // Controller registers: the data is on bit 0, the upper bits are not driven (open bus, usually $40)
            0x4016..=0x4017 => {
                let port = (addr & 1) as usize;
                (self.last_bus_value & 0xE0) | self.controllers[port].read()
            }
            // Cartridge: expansion area, PRG-RAM and PRG-ROM; nothing driving the bus means open bus
            0x4020..=0xFFFF => self.cartridge.cpu_read(addr).unwrap_or(self.last_bus_value),
            // $4018-$401F: disabled test registers
            _ => self.last_bus_value,
        };

        self.last_bus_value = value;
        value
    }

    fn write(&mut self, addr: u16, data: u8) {
        self.last_bus_value = data;

        match addr {
            // Internal RAM + mirroring
            0x0000..=0x1FFF => {
                self.ram.write(addr & 0x07FF, data);
            }
            // PPU registers + mirroring
            0x2000..=0x3FFF => {
                self.ppu.write_register(addr & 0x7, data, &mut self.cartridge);
                self.cartridge.ppu_register_write(addr & 0x7, data);
            }
            // OAM DMA
            0x4014 => {
                self.oam_dma = Some(data);
            }
            // APU registers
            0x4000..=0x4013 | 0x4015 => {
                self.apu.write_register(addr, data);
            }
            // Controller strobe: bit 0 is shared by both ports
            0x4016 => {
                for controller in &mut self.controllers {
                    controller.write_strobe(data & 1 != 0);
                }
            }
            // $4017 writes go to the APU frame counter
            0x4017 => {
                self.apu.write_register(addr, data);
            }
            // Cartridge: expansion area, PRG-RAM and mapper registers
            0x4020..=0xFFFF => self.cartridge.cpu_write(addr, data),
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u8) {
        self.tick(cycles as u16)
    }

    fn tick_before_access(&mut self, write: bool) {
        self.start_cycle(if write { Bus::WRITE_DOTS_BEFORE } else { Bus::READ_DOTS_BEFORE });
    }

    fn tick_after_access(&mut self, _write: bool) {
        self.end_cycle();
    }

    fn fetch_nmi(&mut self) -> Option<u8> {
        self.nmi_interrupt.take()
    }

    fn irq_pending(&self) -> bool {
        self.cartridge.irq_pending() || self.apu.irq_pending()
    }

    fn take_dmc_dma(&mut self) -> Option<u16> {
        self.apu.take_dmc_dma_request()
    }

    fn dmc_dma_is_load(&self) -> bool {
        self.apu.dmc_dma_is_load()
    }

    fn dmc_dma_done(&mut self, value: u8) {
        self.apu.load_dmc_sample_byte(value);
    }

    fn take_oam_dma(&mut self) -> Option<u8> {
        self.oam_dma.take()
    }
}
