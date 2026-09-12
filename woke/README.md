# woke

Keeps this Mac awake with the lid closed, and puts the setting back when it exits.

```sh
woke                      # hold until Ctrl-C
woke ./serve.sh           # hold only while the command runs
woke --status             # what is set right now
woke --install-sudoers    # stop it asking for a password (once)
woke --uninstall-sudoers  # undo that
```

The display is untouched and still sleeps normally; only system sleep is
disabled. Processes keep running and the machine keeps answering the network.

## Why `caffeinate` cannot do this

`caffeinate` takes out power *assertions*, and those only suppress sleep
triggered by the idle timer. Closing the lid is a separate, forced path in the
kernel that ignores userspace assertions — the power log calls it
`Clamshell Sleep` — which is why a service stops answering a few minutes after
you close the machine, whichever of `-i`, `-d` and `-s` you passed.

Apple documents the limitation in IOKit's public `IOPMLib.h`, where
`kIOPMAssertPreventUserIdleSystemSleep` is described as leaving the system free
to "sleep for lid close, Apple menu, low battery, or other sleep reasons".

## What it does instead

It sets `pmset -a disablesleep 1`, an undocumented setting backed by
`SleepDisabled` in `IOPMrootDomain`. That covers idle sleep as well as the lid.

The setting is global and survives reboots, which is the awkward part: set it
and forget it, and the laptop will happily cook itself in a bag some week later.
So `woke` holds it only while it runs and restores it on the way out — on
Ctrl-C, on `SIGTERM`/`SIGHUP`, or when the wrapped command finishes. If the
restore ever fails it says so loudly, with the command to run by hand.

There is no unprivileged way to do this. Underneath, `pmset` calls
`IOPMSetSystemPowerSetting(CFSTR("SleepDisabled"), …)`, which is exported from
IOKit but declared only in `IOPMLibPrivate.h`; calling it directly buys nothing,
since it returns `kIOReturnNotPrivileged` without root exactly as `pmset` does.
So `woke` shells out to `sudo pmset`.

## `--install-sudoers`

Writes `/etc/sudoers.d/woke`, granting your user these two commands — and
nothing else — without a password:

```
/usr/bin/pmset -a disablesleep 1
/usr/bin/pmset -a disablesleep 0
```

`sudo` matches specified arguments literally, so no other `pmset` invocation is
reachable through it. The capability it hands out is "can toggle the sleep
setting", so the worst it can be abused for is flattening your battery.

The file is validated with `visudo -c` before installing and the whole ruleset
re-checked afterwards, with the file removed again if anything is wrong. A valid
file in `sudoers.d` still does nothing if `/etc/sudoers` has no `@includedir`
line for it, so the install then confirms the rule actually takes effect rather
than assuming it.
