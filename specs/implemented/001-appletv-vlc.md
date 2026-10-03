# appletv-vlc, with its own Companion client

`appletv-vlc` plays a local movie on an Apple TV through VLC for tvOS. It
serves the file over HTTP and asks the TV to open
`vlc://<host>:<port>/<token>/<name>`, which VLC turns back into an HTTP
URL and streams without transcoding. The Apple TV side goes through
`companion-link`, a library with the parts of Apple's Companion link
protocol needed for that, ported from pyatv.

## Functionality

    appletv-vlc scan                      list the Apple TVs on the network
    appletv-vlc pair [--device <name>]    pair, typing the PIN the TV shows
    appletv-vlc [--device <name>] [--port <n>] [--url-only] <file>

Playing serves the file until Ctrl-C, with byte ranges so VLC can seek,
behind a random path segment so the open port does not expose the file.
If the launch fails, the URL is printed to enter by hand under Network
Stream in VLC, and the server keeps running.

An Apple TV is named by its name on the network, the one set under
Settings → General → Name, which is what Bonjour advertises. Without
`--device` (or `$APPLETV_VLC_DEVICE`) the only paired TV is used. The TV
lists the tool as `skagedal-tools` under Remotes and Devices. A renamed
TV needs pairing again: its stable pairing identifier is not in its
Bonjour record, so matching by it would mean connecting to every Apple TV
on the network.

The served URL uses the local address that routes to the TV. With no TV
to route to (`--url-only`), it is the first private IPv4 address on an
`en*` interface; the default route will not do, since with a VPN up it
leads into the tunnel.

## The Apple TV integration

Apple TVs advertise `_companion-link._tcp.local.`, as Macs, iPhones and
HomePods do; the TXT record's model (`rpMd`) starting with `AppleTV` tells
them apart.

The protocol runs over one TCP connection, in frames of a type byte and a
24-bit length. Payloads are OPACK dictionaries, and pairing messages
carry HomeKit-style TLV8 in their `_pd` field. The tool needs four
exchanges:

- **Pair setup**, once. M1 makes the TV show a PIN, so the library splits
  it into `PairSetupStart` (M1, M2) and `PairSetupFinish` (M3 to M6),
  letting the caller ask for the PIN in between. M3 and M4 are SRP-6a over
  the PIN, with the 3072-bit group and SHA-512. M5 and M6 exchange Ed25519
  long-term keys, encrypted under the SRP session key. M5 also carries a
  `Name` item, an OPACK `{"name": …}`; without one the TV lists the
  pairing as an unknown device.
- **Pair verify**, on every connection. HomeKit's exchange, but Companion
  derives the session keys from the X25519 secret with an empty salt and
  `ClientEncrypt-main`/`ServerEncrypt-main`. From then on every payload is
  ChaCha20-Poly1305, with a little-endian message counter as the nonce
  and the frame header as associated data.
- **`_systemInfo`**, telling the TV who we are. pyatv also sends
  `_touchStart`, `_sessionStart`, `TVRCSessionStart`, `_tiStart` and an
  `_interest` subscription, but a real Apple TV opens the URL without them.
- **`_launchApp`** with `_urlS` set to the `vlc://` URL.

Requests are matched to responses by transaction id (`_x`); the TV may
send events in between, which are skipped.

The library is IO-free, in the style of [Pimalaya's
architecture](https://github.com/pimalaya/.github/blob/master/ARCHITECTURE.md):
each step is a coroutine that yields bytes to write or asks for bytes
read, and takes its randomness from the caller. A `client` feature drives
them over a blocking `TcpStream`. It is not `no_std`, since the crates it
builds on are not.

### Built on

- **apple-opack** for OPACK, itself a port of pyatv's.
- **hap-tlv8** for TLV8.
- **hap-crypto** for Ed25519 keys, X25519, and ChaCha20-Poly1305. Its
  pair setup and pair verify state machines are not used: the first has
  no room for the name in M5, and the second derives HomeKit's session
  keys and does not expose the shared secret.
- **srp** for the SRP-6a arithmetic, at 0.7.0-rc.3 since 0.6 has none of
  RFC 5054. Its own client hashes `PAD(g)` into the proof where HomeKit
  hashes `g`, so the hashes are done here, with every number padded to the
  length of N as `hap-crypto` does.
- **mdns-sd** for discovery and **tiny_http** for the server, in the tool.

### Pairings on disk

`~/.local/share/skagedal-tools/appletv-vlc/pairings.toml`, mode `0600`,
one `[[device]]` per TV: its name, its pairing identifier and Ed25519
public key, and our identifier and Ed25519 seed for that pairing.

### Testing

The library's tests run pair setup, pair verify and requests against a
fake Apple TV in memory, with `hap-crypto`'s device-side SRP standing in
for the TV's. The whole flow was also run against pyatv's fake device
(`scripts/fake_device.py --companion`, PIN 1111), and against a real
Apple TV.
