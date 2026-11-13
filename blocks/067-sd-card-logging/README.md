# 067 — Removable SPI SD Card & FAT32 Telemetry Logging

**Status:** ❌ not possible yet
**ESPHome lego:** no `sd_card:` element; ESPHome projects do this in a lambda

## ESPHome

```yaml
spi:
  clk_pin: GPIO6
  mosi_pin: GPIO7
  miso_pin: GPIO2

sd_card:
  cs_pin: GPIO10
  id: sd_logger

interval:
  - interval: 10s
    then:
      - lambda: |-
          FILE *f = fopen("/sd/telemetry.csv", "a");
          if (f != NULL) {
            fprintf(f, "%d,%.2f\n", id(sntp_time).now().timestamp, id(room_temp).state);
            fclose(f);
          }
```

The card is not memory: it answers sector reads and writes over SPI, and a
filesystem layer on top provides `open`, `write` and `close`. A log that
survives a power cut needs `fsync` semantics, or the last records are lost
in a stalled cache.

## espforge

`sd-card-logging.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `spi:` bus | `<spi id="spi2" sclk="6" mosi="7" miso="2" mode="0" frequency="4000kHz" />` is real |
| `sd_card: cs_pin` | proposed `<sd_card id="sd_logger" bus="$spi2" cs="10" />` |
| SPI mode 0 initialisation at 400 kHz | none: no block device driver |
| `fopen("/sd/...", "a")` | proposed `open`/`write`/`close` on the mount |
| FAT32 directory and cluster allocation | none: no filesystem |
| `fsync` on every record | none: no flush control, which an append log needs |
| timestamped record | `<ntp>` is real, and the float formatting is Rust |

Two layers are missing, not one: the card's SPI protocol (init, CMD17/24,
CRC16) and a FAT filesystem on top. Writing one without the other logs
nothing, which is why the bus being real does not make this 🟡.

### What is missing

An `<sd_card>` block device plus a filesystem layer with append and flush
guarantees. Block 099's cross-check is block 019's NVS, which has the same
"persist across resets" role with no card.
