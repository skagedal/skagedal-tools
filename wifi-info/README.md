# wifi-info

Prints the current Wi-Fi network's name and BSSID.

```sh
wifi-info              # ssid<TAB>name, bssid<TAB>access point
wifi-info --ssid       # just the name
wifi-info --bssid      # just the access point's MAC address
wifi-info --json       # {"bssid":"…","ssid":"…"}, bssid null when unknown
wifi-info --authorize  # ask for Location Services permission (once)
```

It exits non-zero when not on Wi-Fi, or when it has no permission.

## Why this needs an app

Since macOS 14.4 the Wi-Fi network name and BSSID count as location data.
`networksetup`, `ipconfig getsummary`, `system_profiler` and CoreWLAN all
answer `<redacted>` or nothing unless the asking process has Location
Services permission, and that can only be granted to an app bundle with a
location usage description in its `Info.plist`.

So `install` wraps the binary in `wifi-info.app`, ad-hoc signed, under
`~/.local/share/skagedal-tools/wifi-info/`, and links `~/.local/bin/wifi-info`
to the binary inside it. Notarization is not needed; that is for software
that is downloaded, not built here.

Two things make it more roundabout than that:

- Permission is checked against the *responsible* process. Run from a
  terminal, that is the terminal, which has no permission. So the binary,
  when run directly, relaunches itself through `open`, where LaunchServices
  makes the app responsible for itself, and relays what it printed. That
  costs about 0.2 s.
- An ad-hoc signature identifies the exact binary, so a permission does not
  survive a rebuild. `install` leaves the bundle alone when the build has not
  changed; when it has, run `wifi-info --authorize` again.
