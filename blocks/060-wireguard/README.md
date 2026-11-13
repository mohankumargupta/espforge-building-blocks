# 060 — WireGuard Encrypted VPN Overlay Tunnel

**Status:** ❌ not possible yet
**ESPHome lego:** no ESPHome equivalent (this is the roadmap's extension)

## ESPHome

```yaml
wireguard:
  address: 10.0.0.15
  private_key: "CLIENT_PRIVATE_KEY_HERE="
  peer_endpoint: "vpn.myhome.net"
  peer_port: 51820
  peer_public_key: "SERVER_PUBLIC_KEY_HERE="
  peer_allowed_ips:
    - 10.0.0.0/24
```

WireGuard is a user-space tunnel over UDP: a Curve25519 handshake establishes
session keys, ChaCha20-Poly1305 seals each packet, and the peer's endpoint can
change at any time. The node keeps its own overlay address (`10.0.0.15`) while
the underlying Wi-Fi address roams.

## espforge

`wireguard.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `wireguard:` | proposed `<wireguard id="tunnel" network="$home" address="10.0.0.15" />` service |
| `private_key` / `peer_public_key` | proposed attributes; key handling would be `env:`-sourced like `password` |
| `peer_endpoint` / `peer_port` | one peer per element, so `peer="vpn.myhome.net:51820"` |
| `peer_allowed_ips` | proposed `allowed_ips="10.0.0.0/24"`, an overlay route |
| the tunnel as a network provider | proposed `provides "network"`, so `<http_client>` could ride it |
| raw UDP sockets | none: the runtime has TCP clients only |
| crypto | none: no Curve25519/ChaCha20-Poly1305 in the build |

Two separate gaps: there is no UDP socket to send a handshake from, and no
crypto. A public-key handshake also pulls in a random number source, which the
runtime does not expose today.

### What is missing

A UDP socket API, a `provides "network"` interface abstraction that can wrap a
tunnel, and a crypto dependency. This is the biggest single block in the
roadmap.
