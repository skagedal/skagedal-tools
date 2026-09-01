# disky

Move big directories between this machine and a remote, and reclaim the local
space once the remote copy has been proved correct.

```
disky list                 what exists locally, remotely, or both, plus free space
disky offload <name>...    push local -> remote  (--delete to reclaim the space)
disky onload  <name>...    pull remote -> local  (no name: list what's available)
disky status  [<name>]     per-project detail, including exactly what differs
disky pairs                the configured pairs
```

Commands find their pair from the current directory, so inside `~/studio` a bare
`disky list` already knows which remote it means. `--pair`/`-p` overrides that.
`offload` with no name uses the project the current directory is in, so you can
`cd` into one and just say `disky offload --delete`.

## Configuration

`~/.skagedal-tools/disky.toml` (override with `$DISKY_CONFIG`):

```toml
[remotes.hetzner]
host = "u656759.your-storagebox.de"
user = "u656759"
port = 23
identity_file = "~/.ssh/hetzner-key"

[pairs.studio]
remote = "hetzner"
local = "~/studio"
path = "studio"
```

`[remotes.*]` are ssh endpoints; `[pairs.*]` bind a local directory to a path on
one of them. Port and key live in the config rather than in `~/.ssh/config`, so
the tool is self-contained.

Omit `remote` from a pair and `path` is read as a plain path on this machine —
useful for an external disk, and what the tests use:

```toml
[pairs.usb]
local = "~/studio"
path = "/Volumes/Backup/studio"
```

`rsync_extra` takes additional rsync flags, at top level (all pairs) or inside a
pair.

## Disk space

`list` and `offload` both report how full the remote disk is:

```
u656759.your-storagebox.de:studio: 1.2T used, 3.8T free of 5.0T (24% full)
```

Nothing in the rsync protocol carries this, so `disky` runs `df -Pk` on the far
side — over the pair's own ssh settings for a remote pair, locally for a pair
without one. The percentage counts used against used-plus-available, the way
`df`'s own Capacity column does, so it agrees with what every other tool on the
machine says.

The reading is never fatal. A remote whose restricted shell has no `df` still
lists and offloads fine; the line is replaced by a note on stderr.

If the directory itself isn't there — nothing offloaded yet, or an external disk
that isn't mounted — `df` is asked about the nearest enclosing directory that
does exist, and the line says so:

```
/Volumes/Backup/studio: 409.1G used, 22.7G free of 460.4G (95% full) (measured at /Volumes)
```

That suffix is worth reading. `/Volumes` is on the *internal* disk, so a reading
annotated that way is telling you the backup drive is not plugged in.

## How `--delete` decides it is safe

1. `rsync --delete` copies the project up.
2. A second pass runs `rsync --dry-run --itemize-changes --checksum --delete`,
   which re-reads **both** sides and compares content hashes rather than size
   and mtime.
3. Only if that reports zero differences is the local copy removed — and only
   after re-resolving the path to confirm it is really inside the pair root.

Any difference aborts the delete and prints the offending files. The point is
silent corruption on the remote: a flipped bit that kept the same size and mtime
is invisible to an ordinary transfer, and the checksum pass is what catches it.
There is a test for exactly that.

Directory-mtime-only differences (`.d..t......`) are filtered out — they change
whenever any file inside changes, carry no content, and the Storage Box does not
reliably round-trip them.

## Why shell out to rsync

No Rust crate speaks rsync's remote protocol; the librsync bindings only
implement the delta algorithm over local files. Meanwhile the Hetzner Storage
Box runs a real rsync on port 23, which is by far the fastest option for the
many-small-file trees that Logic projects are. So `disky` builds argument
vectors and runs the real thing.

The alternatives were weighed and rejected: **rclone** over SFTP has no delta
transfer and pays a round-trip per file, and the Storage Box's restricted shell
gives it no remote hash command, so `rclone check` degrades to size+mtime —
exactly the guarantee that needed to be stronger. **git-annex** fits the verb
pair but replaces files with symlinks, which Logic Pro won't open.
**restic/borg/kopia** are backup tools: snapshots and dedup, not "move this
folder there and back by name".

## Caveats

**Extended attributes are not synced.** `-X` is deliberately not passed: the
Storage Box does not reliably store xattrs, and turning it on makes every
verification pass report spurious differences, which would defeat the delete
gate. In practice this loses Finder tags and colour labels. Logic's own project
data lives in real files, so projects round-trip intact. Add `--xattrs` via
`rsync_extra` if you need it and your remote supports it.

**Owner and group are not preserved** (`-rlptD`, not `-a`): we are not root, and
the Storage Box maps everything to the one account anyway.

**`.DS_Store` and Spotlight metadata are excluded**, both to keep transfers clean
and because they otherwise produce endless phantom diffs.

## Hetzner Storage Box setup

1. Enable SSH support for the box in the Hetzner Robot panel.
2. Install the key — the box's restricted shell has no writable `~/.ssh`, so it
   provides a command for this:

   ```
   ssh-keygen -t ed25519 -f ~/.ssh/hetzner-key -C hetzner
   cat ~/.ssh/hetzner-key.pub \
     | ssh -p23 u656759@u656759.your-storagebox.de install-ssh-key
   ```

3. `disky pairs` should then list the pair, and `disky list` work inside it.

## Getting things out of iCloud first

`~/Documents` on this Mac is managed by iCloud Desktop & Documents sync, so
files there can be dataless placeholders. `icloud-offload` (in the dotfiles repo) materialises a
tree and moves it out; `disky` takes it from there.
