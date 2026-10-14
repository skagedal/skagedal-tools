# chrome

Opens URLs in the Chrome profile signed in as a given account.

```sh
chrome me@example.com https://github.com/login
chrome me@example.com                  # just a window in that profile
chrome --profile-dir me@example.com    # print the profile directory
chrome --list                          # every profile and who is signed in
```

Chrome names its profile directories (`Default`, `Profile 1`, …) in the order
the profiles were created, so the same account lives in a different directory
on each machine. `chrome` looks the directory up in Chrome's `Local State` by
the address signed into it, so nothing has to hardcode them.

It exits with status 3 when no profile is signed in as the account, which
setup scripts can check for.

The accounts themselves belong to whoever calls it: the `chrome-personal` and
`chrome-work` wrappers live in dotfiles.
