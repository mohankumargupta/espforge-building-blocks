# A gentle intro to Wi-Fi (and to how `esp-emu` pretends to be an access point)

This is the long version of [001-wifi-basic](.) for someone who has not done
Wi-Fi on an ESP before. It answers three questions in order: what the hardware
does, what the software has to do, and how the emulator stands in for the
access point so the whole thing runs on your laptop.

Everything here is ESP32-C3 only, and every `espforge` / `esp-emu` call goes
through a recipe in the root `justfile` (`just check`, `just build`,
`just esp-emu`, `just wifi-basic-*`). Do not call either binary directly.

---

## 1. Two halves: the radio and the station

An ESP32-C3 has one 2.4 GHz Wi-Fi radio. In espforge that is a **peripheral**:

```
# peripherals
  <wifi id="radio" />
```

A peripheral is a piece of silicon. It does not know your network's name, it
cannot do DHCP, and on its own it does nothing at all. What makes it useful is
a **service** that owns the software side — credentials, association, the
TCP/IP stack, DNS, DHCP:

```
# services
  <wifi_station id="home" wifi="$radio" ssid="MyNetwork" password="MyPassword" />
```

`wifi="$radio"` is the link between the two. One radio carries one station;
espforge rejects a second `<wifi_station>` on the same `<wifi>`, because the
driver owns the radio exclusively.

Two things follow from this split, and both are ordinary espforge rules:

- **`<wifi_station>` forces `<runtime type="embassy" />`.** esp-hal's Wi-Fi
  driver is async, and the DHCP client is a task that has to keep being polled.
  A blocking runtime has no way to do that.
- **`<wifi>` alone is not "networking".** It gets you a radio and nothing
  else. Every network client (`<http_client>`, `<ntp>`, …) names a provider
  with `network="$home"`, and `<wifi_station>` is the provider.

### Credential rules (checked by `just check`, before any compilation)

| What you write | What it means |
|---|---|
| `ssid="MyNetwork"` | 1–32 bytes. Must match the AP you are joining. |
| `password="MyPassword"` | 8–63 characters (WPA2-PSK). |
| `auth="open"` | An open network: **no** password attribute allowed. |
| `password="env:WIFI_PASSWORD"` | Read from the build environment, so a secret need not live in the project file. |

An open AP with a password, or a password that is too short, is an espforge
error, not a runtime failure. That is deliberate: a wrong-length PSK can never
work, so it fails at check time.

---

## 2. What "connected" actually means

Joining a network is four exchanges, and they happen in this order:

1. **Scan** — the station listens for beacons / probe responses and finds an AP
   advertising your SSID.
2. **Authenticate** — Open System for an open network, or the SAE exchange for
   WPA3.
3. **Associate** — the AP assigns an association ID. At this point frames can
   flow, but the device still has no IP address.
4. **4-way handshake** — with a password, the EAPOL exchange derives the
   session keys and encryption starts. (Skipped on an open network.)
5. **DHCP** — the station broadcasts a DHCP DISCOVER, the AP's DHCP server
   answers, and the device gets an IPv4 address, a gateway and a DNS server.

The distinction between step 3 and step 5 is the one that bites people: a
device can be *associated* and still have **no IP address**, which means every
socket call fails. That is why this example waits on a single call:

```rust
station.wait_connected().await;
```

`wait_connected()` waits for the link (`wait_link_up`, after step 4) **and**
for the configuration (`wait_config_up`, after step 5). Only then is it
meaningful to ask for the address:

```rust
match station.stack().config_v4() {
    Some(config) => logger.info(format_args!("TEST: ip {}", config.address.address())),
    None => logger.info("TEST: ip none"),
}
```

- `station.stack()` is the embassy-net stack the station built — the same
  object HTTP, NTP and DNS rides on. You never construct it yourself.
- `config_v4()` is `None` until DHCP has a lease. It returns
  `Ipv4Cidr` + gateway + DNS, so `config.address` is the address *with* the
  prefix (`192.168.4.2/24`) and `config.address.address()` is just the address.

That "none" line is worth keeping: if it ever prints, the problem is DHCP or
association, not your socket code.

---

## 3. What the emulator does

`esp-emu` contains a **soft AP**: a small Wi-Fi access point implemented inside
the emulator. Your firmware's station driver talks to it over emulated 802.11
frames, and it is a complete AP for the purposes of the association:

| Step | Emulated by `esp-emu` |
|---|---|
| Scan | beacons and probe responses for the configured SSID |
| Authenticate | Open System, WPA2-PSK, or WPA3-SAE (commit/confirm) |
| Associate | AP assigns AID = 1 |
| 4-way handshake | full EAPOL with the PSK/SAE key, including the IGTK for PMF |
| DHCP | built-in DHCP server, leases **192.168.4.2**, gateway **192.168.4.1** |

So there is nothing to install: no hostapd, no dnsmasq, no `sudo`, no TAP
device. Two rules decide whether it works:

1. **The SSID and password must match on both sides.** Firmware asks for
   `MyNetwork` / `MyPassword`; the emulator's defaults are `myssid` /
   `mypassword`. Pass the flags:

   ```
   esp-emu --chip esp32c3 --firmware ... --net user \
           --wifi-ssid MyNetwork --wifi-password MyPassword
   ```

   (The `wifi-basic-run` recipe does exactly this, with `ssid=` / `password=`
   just variables you can override: `just wifi-basic-run 60s myssid mypassword`.)

2. **A password means WPA2 or WPA3, not open.** With `--wifi-password` set the
   AP advertises WPA2/WPA3 transition mode; your firmware picks PSK or SAE
   according to what it supports. For an open network, either pass
   `--wifi-auth open --wifi-password ""` and set `auth="open"` in the project
   file, or just give both sides a password.

### Addressing: `--net user` (the default here)

`--net user` is a QEMU-style user-mode network: the guest's traffic is proxied
through host sockets, with no host setup. The addresses are the same as the
soft AP's:

```
guest     192.168.4.2     <- what "TEST: ip" must print
gateway   192.168.4.1     <- also the DNS server; TCP/UDP to it is
                             redirected to 127.0.0.1 on the host
DNS       forwarded from the host's /etc/resolv.conf
```

That gateway rule is the useful trick for later examples: a server you run on
your laptop at `127.0.0.1:8000` is reachable from the firmware as
`http://192.168.4.1:8000/...`. That is how block 002 fetches its OTA image, and
how you would serve a page to an `<http_client>` in the next example.

Limits worth knowing now: you cannot `ping 192.168.4.2` from the host (the
address only exists inside the emulator process), and `tcpdump` sees nothing
because frames never leave user space. For real LAN visibility you would use
`--net tap,ifname=tap0` after `sudo ./tools/setup-tap.sh`, which needs root and
is not required for this example.

---

## 4. Running it

```
just wifi-basic-check      # schema + resource graph, no compilation
just wifi-basic-image      # -> build/wifi-basic/wifi-basic.bin (+ .elf)
just wifi-basic-test       # boots the emulator and asserts the lease
just wifi-basic-run        # same run, interactive, prints the console
```

`wifi-basic-test` is the gate. It starts the emulator, waits up to 60 s for the
firmware's own line, and fails unless it is exactly the emulator's lease:

```
TEST: wifi boot
wifi-basic: waiting for association and DHCP...
TEST: ip 192.168.4.2
PASS: associated and acquired 192.168.4.2
```

The emulator's own log (ROM, bootloader, driver) is appended to
`build/wifi-basic/emu.log`; the firmware console is the captured stdout in
`build/wifi-basic/test.stdout`. `RUST_LOG` selects emulator verbosity if you
run through `just esp-emu` yourself:

```
RUST_LOG=esp_emu::host::net_user=trace just esp-emu "..."
```

---

## 5. When there is no address

Work down this list; each item maps to one of the steps above.

| Symptom | Likely cause |
|---|---|
| `connect failed: ...` repeating in `emu.log` | SSID or password mismatch between the project file and `--wifi-ssid` / `--wifi-password`. |
| Associated, `TEST: ip none` | DHCP did not complete — usually a password/auth mismatch (the AP drops the station after three failed handshakes) or `<wifi_station>` not being started. |
| Nothing at all on the console | The image did not boot: check `--chip esp32c3` and that you ran the recipe, not the binary. |
| Check fails with "ssid must be 1 to 32 bytes" / "password must be 8 to 63" | A typo in the project file; the rule is enforced before compilation. |
| It works, but a later HTTP example fails to connect | That is not Wi-Fi: the address is fine, so look at the client's URL and `network=` reference. |

---

## 6. Where this goes next

- **002-wifi-ota** (`blocks/002-wifi-ota`) — the same station plus `<ota />`,
  an `<http_client>` and a real download from the host loopback.
- **011-network-time** (`blocks/011-network-time`) — `<ntp network="$home">`:
  the first client that *uses* the stack built here.
- **012-captive-portal** — the other direction: the firmware *is* the AP. The
  emulator can join a firmware soft AP and `hostfwd` a port to it, which is how
  provisioning gets tested.
