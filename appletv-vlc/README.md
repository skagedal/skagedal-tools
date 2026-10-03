# appletv-vlc

Plays a local movie file on an Apple TV, in [VLC for tvOS](https://apps.apple.com/app/vlc-media-player/id650377962).

It serves the file over HTTP from this machine and asks the Apple TV to open it in VLC through VLC's `vlc://` URL scheme. Nothing is transcoded: VLC on tvOS decodes MKV, AC3, DTS and so on natively, which is the whole reason to route around AirPlay, which only accepts what the Apple TV itself can decode.

## Usage

Once, to find the Apple TV and pair with it (the TV shows a PIN):

```
$ appletv-vlc scan
Vardagsrum   AppleTV14,1   192.168.1.40:49153      not paired
$ appletv-vlc pair
Enter the PIN shown on Vardagsrum: 1234
Paired with Vardagsrum.
```

Then, to play something:

```
$ appletv-vlc The.Movie.2019.mkv
Serving: The.Movie.2019.mkv
URL:     http://192.168.1.23:8010/3f9c0a…/The.Movie.2019.mkv
Opening in VLC on Vardagsrum ...

Press Ctrl-C to stop serving.
```

The movie is served until Ctrl-C. The server speaks byte ranges, so seeking in VLC works, and the URL has a random path segment so the open port does not expose the file to the rest of the network.

With several Apple TVs paired, pick one with `--device <name>` or `$APPLETV_VLC_DEVICE`. `--port` changes the port (8010 by default). `--url-only` skips the Apple TV and just prints the URL, to enter under Network Stream in VLC; that is also what to do if launching fails, and the URL is printed then too.

Pairings are kept in `~/.local/share/skagedal-tools/appletv-vlc/pairings.toml`, which holds a private key and is readable only by you. If the pairing is removed on the TV (Settings → Remotes and Devices), run `appletv-vlc pair` again.

The Apple TV side goes through [companion-link](../companion-link/).
