# test_rom

Freely redistributable test ROMs used by the emulator tests. Do not add commercial game ROMs here.

| File | Source | Purpose |
|---|---|---|
| `nestest.nes`, `nestest.log` | Kevin Horton's nestest, copied from https://github.com/christopherpow/nes-test-roms (`other/`); see also https://www.nesdev.org/wiki/Emulator_tests | CPU test: run from `$C000` (automation mode) and compare each executed instruction with `nestest.log` (PC, A, X, Y, P, SP, CYC; the log's PPU columns can be ignored until the PPU exists). |
