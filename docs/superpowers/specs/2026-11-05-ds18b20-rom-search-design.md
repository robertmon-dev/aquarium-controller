# DS18B20 ROM search — design

## Problem

Two DS18B20 probes share one 1-Wire bus on GPIO4 (external VDD from 3.3 V, 4.7 kΩ
pull-up to the data line, common ground). Temperature reading could never work:

1. `settings.rs` carried placeholder ROM addresses
   (`[0x28, 0, 0, 0, 0, 0, 0, 0x00]` and `...0x01`). Their CRC-8 is 0x82 and 0xDC,
   so `validate_rom` rejects both and `Ds18b20Adapter::new` aborts the boot before
   any bus traffic. `git log -S` confirms these values were never real.
2. The only discovery path, `read_single_rom`, issues READ ROM (0x33), which the
   datasheet allows with exactly one device on the bus. With two probes wired in
   parallel both answer at once and the master reads the bitwise AND of the two
   ROMs — the CRC then fails. Discovery therefore demanded unplugging a probe,
   which the soldered topology makes awkward.

## Approach

Implement SEARCH ROM (0xF0, Maxim AN187) so discovery enumerates every device
without touching the wiring. The wiring does not change: DATA stays on GPIO4.

The whole 64-bit search loop lives in `aquarium-core` behind a four-method trait,
so it runs under host tests. Only the timing-critical bit primitives stay in the
firmware adapter, where they cannot be tested.

```
core/src/protocol/ds18b20.rs
    trait OneWireLine          reset / read_bit / write_bit
                               + default read_byte / write_byte (LSB first)
    struct RomSearch           next() -> Option<RomAddress>
    fn search_roms             fills a caller-owned buffer, returns the count
    fn convert_all             reset + SKIP ROM + CONVERT T
    fn read_scratchpad         reset + MATCH ROM + rom + READ SCRATCHPAD + 9 bytes
    fn temperature_from_scratchpad   role-free parse; parse_scratchpad delegates

src/adapters/outbound/ds18b20.rs
    impl OneWireLine for OneWireBus   the three bit-level primitives only
```

Rejected alternatives: keeping the loop in the adapter and exposing only a
decision function (leaves the most error-prone part untested); pulling in the
`one-wire-bus` crate (discards a working driver and the core/adapter split).

## Role identification

SEARCH ROM yields addresses, not roles. `RunMode::RomDiscovery` therefore logs
every ROM found and then streams a temperature per ROM every two seconds, so
warming one probe by hand shows which address belongs to which sensor.
`temperature_from_scratchpad` exists because discovery has no role to report in
an error; `parse_scratchpad` maps its CRC error back onto `CrcMismatch(role)`.

## Testing

A fake bus in `core/src/protocol/ds18b20/tests.rs` models an open-drain
multi-drop line: for each bit position, `id_bit` is the AND over all devices
still matching the path walked so far and the complement bit is the AND of their
inverses; with no candidates left both read 1. Command bytes are reassembled from
individual `write_bit` calls, so a wrong bit order would never reach search mode
and the tests would fail.

Covered: one device; two devices differing in the last byte; two differing in the
first byte; four devices without duplicates; a bus with no presence pulse; a
broken ROM CRC; a foreign family code; SEARCH ROM issued LSB first; `search_roms`
counting and buffer saturation; MATCH ROM addressing in `read_scratchpad`.

## Deliberate behaviour changes

- `read_temperature` previously yielded to the tokio scheduler between every
  byte. It now yields once per sensor read, blocking roughly 10 ms. Acceptable at
  a 30 s sampling interval, and each bit already masks interrupts for ~70 µs.
- `read_single_rom` and the adapter's byte-level loops are gone; byte assembly
  lives in the trait defaults and is host-tested.

## Out of scope

Verifying at boot that both configured ROMs are present; parasite power support;
heater outputs.
