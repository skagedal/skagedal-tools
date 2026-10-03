# appletv-vlc in Rust, with its own Companion client

No issue; this replaces `bin/atv-vlc.sh` in the dotfiles repository.

`atv-vlc.sh` plays a local movie on the Apple TV through VLC for tvOS. It
serves the file over HTTP with byte ranges, and asks the Apple TV to open
`vlc://<host>:<port>/<token>/<name>`, which VLC turns back into an HTTP
URL and streams without transcoding. The second half is done by pyatv's
`atvremote launch_app=…`, over Apple's Companion link protocol, and the
first half by a Python HTTP server embedded in the script as a heredoc.

So the script needs a Python interpreter, pyatv installed through `uv`,
and pyatv's pairing file in `~/.pyatv.conf`, for what amounts to five
messages on one TCP connection. This moves it here as a Rust tool, and
moves the part of pyatv it uses into a library crate of its own, written
in the IO-free style of [Pimalaya's
architecture](https://github.com/pimalaya/.github/blob/master/ARCHITECTURE.md):
the protocol is a set of state machines that say what they want written
and read, and a thin blocking client does the actual socket work.

## Functionality

### Playing a movie

    appletv-vlc The.Movie.2019.mkv

finds the Apple TV, serves the file, and opens it in VLC on the TV. What
the terminal shows is what the script showed:

    Serving: The.Movie.2019.mkv
    URL:     http://192.168.1.23:8010/3f9c0a…/The.Movie.2019.mkv
    Opening in VLC on Vardagsrum ...

    Press Ctrl-C to stop serving.
      [http] serving The.Movie.2019.mkv (2318.4 MiB) on port 8010
      [http] GET /3f9c0a…/The.Movie.2019.mkv 206 bytes=0-

and keeps serving until Ctrl-C. Every behaviour of the old HTTP server is
kept: a 64-bit random path token so the open port does not expose the
file to the rest of the network, single and suffix byte ranges with `206`
and `416`, `HEAD`, keep-alive, the content type picked from the file
extension, and silence about clients that hang up mid-transfer, which
VLC does on every seek.

`--port <n>` changes the port (default 8010). `--url-only` skips the
Apple TV entirely and prints the URL with the hint to enter it under
Network Stream, as before. A file that does not exist, or a second
positional argument, is an error before anything is started.

The URL uses the address of the interface that routes to the Apple TV,
found by asking the operating system which local address it would use to
reach it. The script took `en0`, then `en1`, which picks the wrong one on
a machine whose Ethernet and Wi-Fi are on different networks, and finds
nothing at all on a Mac whose Wi-Fi is `en1` and whose `en0` is down.
With `--url-only`, or when the TV does not answer, there is nothing to
route to, and it is the first private IPv4 address on an `en*` interface,
much as before. The default route is no substitute: with a VPN up it
leads into the tunnel, and the TV cannot reach that address.

### Which Apple TV

Devices are named by their name on the network: the one set under
Settings → General → Name on the TV, which is what Bonjour advertises.
`--device <name>` (or `$APPLETV_VLC_DEVICE`) picks one, compared without
regard to case. Without either, the one paired Apple TV is used; with
none paired, or several, it is an error that says to pass `--device` and
mentions `appletv-vlc scan`.

The old `--id`/`$ATV_ID` named a pyatv identifier, which is meaningless
to this tool, so they are gone rather than kept as aliases.

    appletv-vlc scan

browses the network for three seconds and lists every Apple TV it hears:

    Vardagsrum   AppleTV14,1   192.168.1.40:49153   paired
    Sovrum       AppleTV6,2    192.168.1.41:49153   not paired

Other Apple devices advertise the Companion service too — Macs, iPhones,
HomePods — and are left out, by their model string.

### Pairing

    appletv-vlc pair [--device <name>]

connects, asks the TV to show a PIN, prompts for it on the terminal, and
stores the result. Without `--device` it pairs with the only Apple TV on
the network, and with several it says to pick one. Pairing again with a
TV already paired replaces the old entry.

When playing, an Apple TV that is not paired is an error saying to run
`appletv-vlc pair`. A TV that refuses the stored pairing — it was removed
under Settings → Remotes and Devices, or the TV was reset — gets the same
advice, with the protocol error underneath.

Pairings made by pyatv are not imported. Pairing takes one PIN, and
reading `~/.pyatv.conf` would keep a dependency on a file format this
change is otherwise getting rid of.

### When launching fails

As in the script, a failed launch does not stop the server: it prints why
(not paired, VLC not installed, the TV asleep and not answering, no TV by
that name on the network), and the URL to enter by hand, and keeps
serving. An Apple TV that does not answer within five seconds counts as a
failure.

## Implementation

### Two new crates

`companion-link/` is the library: the parts of Apple's Companion link
protocol needed to pair with an Apple TV and send it requests. Its
modules:

- `frame` — the four-byte frame header (type, 24-bit big-endian length),
  the frame types, and a buffer that turns bytes read into frames.
- `cipher` — ChaCha20-Poly1305 for the session, with 96-bit little-endian
  counters as nonces and the frame header as associated data.
- `session` — the connection state shared by the coroutines: the frame
  buffer, the cipher once there is one, and the next transaction id.
- `pair_setup` — pair setup as two coroutines, `PairSetupStart` (M1, M2)
  and `PairSetupFinish` (M3–M6), split where the PIN appears on the TV,
  so the caller can ask for it in between. Their result is `Credentials`.
- `pair_verify` — the pair verify coroutine, which turns `Credentials`
  into an encrypted `Session`.
- `request` — a coroutine sending one OPACK request (`_i`, `_t`, `_c`,
  `_x`) and returning the response content, or the device's `_em` error.
- `client` — behind the `client` feature: a blocking driver over
  `std::net::TcpStream` that runs the coroutines, with
  `Client::connect`, `pair_start`/`pair_finish`, `verify` and `request`,
  and helpers for the few requests used (`_systemInfo`, `_sessionStart`,
  `_launchApp`, `_sessionStop`).

A coroutine is resumed with `resume(arg)` and either yields what it wants
— bytes written, or more bytes read — or completes with its result. It
never touches a socket, a clock or a file, which is what lets the tests
drive whole exchanges from recorded bytes.

The Pimalaya rules are followed where they cost nothing here; where they
don't fit, these crates follow this repository. In particular the crate
is not `no_std`: `apple-opack` and `hap-crypto` use `std`, and nothing
here runs anywhere without it.

`appletv-vlc/` is the tool, a binary crate using `companion-link` with the
`client` feature: `main.rs` with the clap surface, `discovery.rs` over
mDNS, `pairings.rs` for the stored credentials, `server.rs` for the HTTP
server, and `play.rs` putting them together.

### What the libraries on crates.io do

- **[apple-opack](https://crates.io/crates/apple-opack)** — OPACK, the
  binary serialization Companion messages are written in. A port of
  pyatv's, tested against it.
- **[hap-tlv8](https://crates.io/crates/hap-tlv8)** — TLV8, which the
  pairing messages are written in inside the OPACK `_pd` field.
- **[hap-crypto](https://crates.io/crates/hap-crypto)** — its
  `PairSetupClient` is an IO-free HomeKit pair setup state machine (SRP-6a
  with the 3072-bit group and SHA-512, then the Ed25519 exchange), and
  Companion's pair setup is the same exchange in different framing. Its
  `ControllerKeypair`, `EphemeralKeypair`, Ed25519 verification and
  ChaCha20-Poly1305 wrappers are used for pair verify too.
- **[hkdf](https://crates.io/crates/hkdf)** and
  **[sha2](https://crates.io/crates/sha2)** — HKDF-SHA512 for the pair
  verify keys.
- **[mdns-sd](https://crates.io/crates/mdns-sd)** — browsing
  `_companion-link._tcp.local.`, in the tool.
- **[tiny_http](https://crates.io/crates/tiny_http)** — the HTTP server,
  in the tool. It handles keep-alive and `HEAD`; the range logic is ours,
  being twenty lines that decide which bytes of the file to send. Its
  habit of switching to chunked encoding for large bodies is turned off,
  since VLC learns the file's size from `Content-Length`.
- **[if-addrs](https://crates.io/crates/if-addrs)** — the interface list
  for `--url-only`, in the tool. `mdns-sd` depends on it already.

`hap-crypto`'s `PairVerifyClient` is not used. Companion runs the same
pair verify messages, but derives its session keys from the shared secret
with an empty salt and `ClientEncrypt-main`/`ServerEncrypt-main`, where
HomeKit uses `Control-Salt` and `Control-*-Encryption-Key`, and the
client does not give the shared secret out. Writing pair verify is about
eighty lines on the primitives above.

`PairSetupClient` takes the PIN in its constructor, before M1, but the
Apple TV only shows the PIN in response to M1. `PairSetupStart` therefore
sends M1 itself and keeps M2, and `PairSetupFinish` builds the client
from the PIN, calls `start` to put it in the state of awaiting M2 (the M1
it returns is dropped, being identical to the one sent), and hands it M2.
Its M5 has no `Name` item, which pyatv adds as an OPACK
`{"name": "pyatv"}`; see the open questions.

### On disk

Pairings are stored in
`~/.local/share/skagedal-tools/appletv-vlc/pairings.toml`, created with mode
`0600` since it holds a private key:

```toml
[[device]]
name = "Vardagsrum"
device_id = "E3E25DF5-AB48-40F0-B61D-2AADADAB0FBD"  # the TV's pairing identifier
device_public_key = "…"                              # hex, Ed25519
client_id = "01c71d5d-14b3-4486-877a-60d9304510d6"  # ours, a fresh UUID per pairing
client_secret_key = "…"                              # hex, Ed25519 seed
```

### Repository changes

- `Cargo.toml`: both crates as workspace members; `apple-opack`,
  `hap-crypto`, `hap-tlv8`, `hkdf`, `sha2`, `mdns-sd`, `tiny_http`,
  `if-addrs`, `rand` and `companion-link` (by path) in `[workspace.dependencies]`.
- `shared.sh`: `appletv-vlc` in `INSTALLED_RUST_TOOLS`. `companion-link` is a
  library and, like `skagedal-dirs`, is checked by the workspace pass.
- `README.md`: rows for both.

### Testing

The library's tests drive pair verify and requests against a fake Apple
TV written in the test, all in memory, which is what being IO-free buys.
Pair setup needs the device's half of SRP, which `hap-crypto` does not
expose, so it is exercised by hand against pyatv's own fake device
(`scripts/fake_device.py --companion` in the pyatv repository, PIN 1111),
which runs the whole flow — scan, pair, verify, `_systemInfo`,
`_launchApp` — against pyatv's server-side implementation of it.

### Outside this repository

In the dotfiles:

- `bin/atv-vlc.sh` is deleted.
- `other-package-systems.sh` loses `uv tool install pyatv`, and
  `setup/30-other-packages.sh` the `~/.local/bin/atvremote` it checks for.
- `README.md` and `migrate.sh` lose `~/.pyatv.conf`, and gain
  `~/.local/share/skagedal-tools/appletv-vlc/` if the latter lists state
  worth carrying to a new machine.

## Open questions

- **The pairing name.** pyatv sends a name in M5 and `hap-crypto` does
  not. The Apple TV accepts the pairing without one. If it lists the
  pairing without a name under Remotes and Devices, and that matters,
  pair setup M5 can be built here instead, from the same primitives.
- **What the Apple TV needs before `_launchApp`.** pyatv sends
  `_systemInfo`, `_touchStart`, `_sessionStart`, `TVRCSessionStart`,
  `_tiStart` and an `_interest` subscription on every connect, because it
  sets up for everything. This sends `_systemInfo` alone, and that is
  enough for a real Apple TV to open the URL in VLC.
- **Naming devices by name.** A renamed TV needs pairing again, or editing
  the name in `pairings.toml`. The TV's pairing identifier is stable but
  is not in its Bonjour record, so matching by it means a connection to
  every Apple TV on the network.
