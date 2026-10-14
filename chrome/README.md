# chrome

Opens URLs in Chrome, optionally in the profile signed in as a given account.

```sh
chrome --account me@example.com https://github.com/login
chrome --account me@example.com              # just a window in that profile
chrome https://example.com                   # whichever profile Chrome picks
chrome --new-window https://a.example https://b.example
chrome --incognito
chrome --app https://linear.app              # its own window, no tabs or address bar
chrome --profile-dir --account me@example.com  # print the profile directory
chrome --list                                # every profile and who is signed in
```

Chrome names its profile directories (`Default`, `Profile 1`, …) in the order
the profiles were created, so the same account lives in a different directory
on each machine. `chrome` looks the directory up in Chrome's `Local State` by
the address signed into it, so nothing has to hardcode them.

It exits with status 3 when no profile is signed in as the account, which
setup scripts can check for.

When Chrome is already running, the process `open` starts hands its command
line to the running one and exits, and that one only acts on a few switches —
which is why these are the ones offered. Startup-only switches such as
`--remote-debugging-port` would be silently ignored.

The accounts themselves belong to whoever calls it: the `chrome-personal` and
`chrome-work` wrappers live in dotfiles.
