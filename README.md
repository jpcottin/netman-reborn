# Netman Reborn — Etherman + Interman

A modern recreation of two tools from the **Netman** suite (Curtin
University, 1993): **Etherman** (layer 2 conversations, MAC addresses) and
**Interman** (layer 3 conversations, IPv4 + IPv6), displayed **side by side**
in the browser and fed by **a single network capture**.

- **Rust** backend: packet capture, aggregation, HTTP/WebSocket server (axum).
- **sigma.js v3** + graphology frontend: Etherman lays its stations out on a
  circle (a flat L2 network); Interman draws one circle per network (classful
  IPv4, /64 for IPv6), with the networks spread over a ring.
- A **strictly passive** tool: it captures and displays, nothing is injected.

> **Why this project?** I used the original Netman tools (Etherman,
> Interman…) on a **Sun SPARC workstation running SunOS 4.1.3**, **in the
> early 1990s**, at **[Télécom Paris](https://www.telecom-paris.fr/)**, my
> engineering school. This repository is a modern recreation, faithful to
> their look and to their spirit.

![Netman Reborn — Etherman (L2) and Interman (L3) side by side, on live traffic](docs/screenshot.png)

## Prebuilt packages

The [releases](https://github.com/AlexandreFenyo/netman-reborn/releases)
carry a build for each platform:

| File | For | Install |
|---|---|---|
| `…-windows-x64.zip` | Windows 10/11 x64 | unpack anywhere |
| `…-freebsd15-amd64.pkg` | FreeBSD 15.x amd64 | `pkg add` |
| `…-freebsd14-amd64.pkg` | FreeBSD 14.x amd64 | `pkg add` |
| `…-macos-arm64.pkg` | macOS on Apple silicon | `installer -pkg … -target /` |

The FreeBSD and macOS packages share one layout: `netman` in
`/usr/local/bin`, the manual page in `/usr/local/share/man/man1`, and the
frontend in `/usr/local/share/netman/static` — which is the built-in default
for `--static-dir`, so the binary works from any directory.

Building from source is covered below.

## Requirements (Windows)

- **Windows** 10 or 11. Capture is native Windows through **Npcap** — do not
  run it from WSL2 (its virtualised network stack does not see the
  promiscuous traffic of the physical adapter).
- **[Npcap](https://npcap.com/#download)** installed with its default
  options.
  - The "WinPcap API-compatible" mode is **not** needed: the binary loads
    `wpcap.dll` from `C:\Windows\System32\Npcap` (delay-load +
    `SetDllDirectory`).
  - If "Restrict Npcap driver's access to Administrators only" was ticked
    when installing Npcap, run netman **as administrator**. With the default
    options no elevation is needed.
- To build: **Rust** (rustup, `stable-msvc` toolchain) + the Visual Studio
  Build Tools (C++) + a Windows SDK. The Npcap SDK is vendored in
  `third_party/`, so there is nothing to configure.

On FreeBSD and macOS, **Rust** is the only build requirement; see the
sections below.

## Build

```powershell
cargo build --release
```

The binary is `target\release\netman.exe`. It serves the `static\` directory
(configurable with `--static-dir`), which therefore has to travel with the
executable.

### FreeBSD

netman builds and runs on **FreeBSD** as well, where capture goes through
libpcap and the `bpf(4)` devices of the base system: no extra dependency, no
patching of the code.

```sh
cargo build --release
./target/release/netman --iface em0     # as root, for access to bpf(4)
```

A ready-to-use port is provided in **`freebsd-port/`** — see the README in
that directory. The `netman.1` manual page documents the options, the
capture privileges and the WebGL requirements of the browsers.

### macOS

netman builds and runs on **macOS** too, where capture goes through the
system libpcap and the `bpf(4)` devices: no extra dependency, no patching of
the code.

```sh
cargo build --release
./target/release/netman --iface en0
```

Access to `/dev/bpf*` is reserved to root by default: run netman with
`sudo`, or join the `access_bpf` group (created by Wireshark's ChmodBPF
package) to capture without elevation.

A Homebrew formula is available in the
[`AlexandreFenyo/homebrew-netman`](https://github.com/AlexandreFenyo/homebrew-netman)
tap — it builds from source and installs the frontend and the manual page in
their standard locations, with the default `--static-dir` path pointing at
the installed copy:

```sh
brew install alexandrefenyo/netman/netman
```

## Running

```powershell
# Interactive interface selection (numbered list):
.\target\release\netman.exe

# Or directly, by index or by substring of the interface name:
.\target\release\netman.exe --iface "Intel"
.\target\release\netman.exe --iface 12

# Replay a .pcap file (offline mode, no network adapter needed):
.\target\release\netman.exe --pcap-file capture.pcap

# Options:
#   --port <n>        HTTP/WebSocket port (default 8080)
#   --listen <addr>   listen address (default 127.0.0.1; 0.0.0.0 or :: serves
#                     every interface over both IPv4 and IPv6 — mind the
#                     firewall rule)
#   --fade <s>        initial delay before silent nodes disappear (default 60 s)
#   --static-dir <d>  frontend directory (default "static")
```

Then open **http://localhost:8080**. `Ctrl-C` shuts the capture and the
server down cleanly.

With a wildcard listen address, netman prints one URL per reachable local
address rather than the wildcard itself, which no browser can connect to.

> To see more than your own traffic plus broadcast/multicast on a switched
> network, connect the machine to a SPAN/mirror port or a TAP. That is a
> matter of infrastructure: the tool stays passive by design.

## The interface

- **Etherman (left)**: one node per MAC address (labelled with its vendor
  from the embedded Wireshark OUI database), one edge per L2 conversation.
  Nodes are laid out **on one large circle** — a layer 2 network is flat, all
  stations share the same segment; conversations cross the circle, as they
  did in the Etherman of 1993.
- **Interman (right)**: one node per IP address (v4/v6, remote networks
  included), renamed automatically as soon as the reverse DNS (PTR) lookup
  succeeds; one edge per L3 conversation. Hosts of the same **classful**
  network (class A → /8, class B → /16, class C → /24; multicast apart;
  IPv6 grouped by /64) form **one circle per network**, with the networks
  spread over a ring.
- **Visual mapping**: node size ∝ log(cumulative bytes); edge width ∝
  log(**observed rate**, smoothed over ~3 s, decaying when the traffic
  stops), with a "Link width" slider per panel to amplify or damp the
  effect; colour = dominant protocol (legend in the footer).
- **Controls**:
  - *Pause / Resume* — freezes both views (capture carries on; resuming
    resynchronises against the server state);
  - *Reset* — clears the node and link history on the server side (as if no
    packet had ever been received), keeping the DNS caches; a reset is also
    triggered automatically when the interface is switched;
  - *Rates* (on by default) — shows the average two-way rate on every link
    (with a suitable unit: bit/s, kbit/s, Mbit/s, Gbit/s), computed between
    the first and the last packet seen since the link has been displayed
    continuously (nothing is remembered once it has faded out);
  - *Protocol* — highlights one protocol (other edges are hidden, nodes
    dimmed);
  - *Fade* — the delay after which silent nodes and edges disappear
    (5 s → 10 min, synchronised across all open tabs).
- **Hovering a node**: Etherman shows the full MAC and its "Vendor xx:yy:zz"
  form; Interman shows the IP and the resolved name if it is known; both add
  `out:` (outbound rate) and `in:` (inbound rate), computed between the first
  and the last packet seen since the host has been displayed continuously.
- Wheel = zoom, drag = pan.

## Architecture (summary)

```
OS pcap thread (blocking, promiscuous, a single capture)
  → etherparse parsing → PacketMeta → channel (never blocks the capture side)
  → tokio aggregator: L2 table (MAC,MAC) + L3 table (IP,IP), 250 ms tick
  → atomic JSON deltas (upsert/remove node/edge) → WebSocket broadcast
  → browser: 2 × (graphology + sigma.js + ForceAtlas2 worker)
```

Resolutions (OUI, PTR) are best-effort, asynchronous and cached — never on
the packet path. See `RESEARCH.md` for the pinned versions and the API
choices, and `CLAUDE.md` for the invariants of the project.

## Tests

```powershell
cargo test
```

Includes the deterministic replay of a `.pcap` fixture
(`tests/fixtures/sample.pcap`, verified byte for byte against its generator).

## Updating the OUI database

```powershell
Invoke-WebRequest https://www.wireshark.org/download/automated/data/manuf `
  -OutFile src\resolve\data\manuf
cargo build --release
```
