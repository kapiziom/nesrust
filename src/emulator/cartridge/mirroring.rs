/// How the PPU's four logical nametables map onto its 2KB of VRAM.
/// Mappers can change it at runtime (MMC1, MMC3, AxROM), so the PPU must ask the cartridge every time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    /// All four nametables show the lower 1KB of VRAM.
    SingleScreenLower,
    /// All four nametables show the upper 1KB of VRAM.
    SingleScreenUpper,
    /// The cartridge supplies extra VRAM, giving four independent nametables.
    FourScreen,
}
