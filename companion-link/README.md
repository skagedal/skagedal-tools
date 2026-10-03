# companion-link

The parts of Apple's Companion link protocol needed to pair with an Apple TV and send it requests, such as opening a URL in an app. Ported from [pyatv](https://github.com/postlund/pyatv), which is where the protocol was worked out.

The library is IO-free in the style of [Pimalaya](https://github.com/pimalaya/.github/blob/master/ARCHITECTURE.md): pair setup, pair verify and requests are coroutines that say what they want written to the connection and read from it, and are resumed with what happened. They never touch a socket, a clock or a source of randomness. The `client` feature adds a blocking driver over `std::net::TcpStream`.

```rust
let mut client = Client::connect(address, Duration::from_secs(5))?;
client.verify(&credentials)?;
client.open_url("vlc://192.168.1.23:8010/movie.mkv")?;
```

OPACK comes from [apple-opack](https://crates.io/crates/apple-opack), TLV8 from [hap-tlv8](https://crates.io/crates/hap-tlv8), and the pair setup state machine and crypto helpers from [hap-crypto](https://crates.io/crates/hap-crypto). Pair verify is written here, since Companion derives its session keys differently from HomeKit.

Used by [appletv-vlc](../appletv-vlc/).
