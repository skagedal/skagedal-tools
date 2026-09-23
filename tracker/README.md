# tracker

A command line program to help keep track of work time by storing data in a simple per-week text file. It is designed for the use case where you have flexible work hours, but wish to keep track that you work a certain number of hours per week. By default, it assumes that you work 8 hours per day, 5 days per week, but this can be configured.

## Usage

Run `tracker start` to start working. It might look like this (I use `$` here to represent your shell prompt): 

```
$ tracker start
[monday 2024-01-08]
* 08:28-
```

As you later end a shift, run `tracker stop`. 

```
$ tracker stop
[monday 2024-01-08]
* 08:28-11:40
```

Use `tracker report` to show your progress. 

```
$ tracker report
You have worked 3:12h today.
You have worked 3:12h this week.
Balance: -4:48h
```

The balance tells you that you have 4 hours and 48 minutes left to work this day in order to be in balance. For a fuller picture, see [looking closer, and looking back](#looking-closer-and-looking-back) below.

While the normal mode of operation is to use `tracker start` and `tracker stop` to track your shifts, you may find that you sometimes forget to start your shift, or otherwise make an error that you wish to correct. Instead of offering a specific user interface to do such edits, `tracker` lets you open the data file for the current week in your text editor of choice (following the `EDITOR` environment variable) by using `tracker edit`.

Here is an example of what a file might look like after two days of tracking: 

```
[monday 2024-01-08]
* 08:28-11:40
* 12:30-17:00

[tuesday 2024-01-09]
* 08:13-12:00
* 13:06-14:40
* 15:01-16:34
```

Each day starts with the week day and ISO-formatted date in square brackets. (The duplication in information is intentional, to make it easier to read the file.) Each shift is represented by a line starting with an asterisk, followed by the start and end time in 24-hour format, separated by a hyphen.

Comments can be written in the file using lines starting with `#`.

## Looking closer, and looking back

`tracker report --verbose` prints the whole week alongside the numbers, laid out the way `tracker edit` shows it, with the length of every shift and the sum of every day written beside it in colour:

```
$ tracker --week=-1 report --verbose
* balance 5:19h

[monday 2026-09-14]     7:57h
* 08:16-08:40             24m
* 09:57-17:00           7:03h
* 20:10-20:40             30m

[tuesday 2026-09-15]    8:10h
* 07:37-08:17             40m
* 08:30-12:00           3:30h
* 13:00-17:00              4h

[wednesday 2026-09-16]  9:33h
* 07:36-11:40           4:04h
* 12:25-17:10           4:45h
* 17:46-18:30             44m

[thursday 2026-09-17]   9:11h
* 07:49-17:00           9:11h

[friday 2026-09-18]        8h
* vacation                 8h

You worked 42:51h this week.
Balance: +8:10h
```

The annotations are not part of the file – they are what `tracker` makes of it – which is what makes them worth having: a day that is short of hours, or a whole day that never got recorded, is visible at a glance instead of having to be added up by hand. A shift left open on an earlier day is marked `not closed` rather than given a duration, since that is exactly what it counts as.

As in the example, `--week` points any command at another week, counted relative to this one, so `tracker --week=-1 report` reports on last week. A week that is not the current one has no "today" in it to report on, and is spoken of in the past tense.

## Durations

Wherever a duration is written – in the report output, or in the week file – the same format is used.

The normal form is *compound*: a single term with an optional sign, where hours and minutes are separated by a colon.

| Written    | Means                            |
|------------|----------------------------------|
| `8h`       | 8 hours                          |
| `1:26h`    | 1 hour and 26 minutes            |
| `+1:26h`   | the same, sign made explicit     |
| `-1:26h`   | minus 1 hour and 26 minutes      |
| `45m`      | 45 minutes                       |
| `-45m`     | minus 45 minutes                 |

The minutes after the colon must always be two digits, so `1:05h` – never `1:5h`, which would be ambiguous.

When writing a duration yourself, you may also use the *separated* form, where hours and minutes are given as separate terms, each with its own sign:

```
1h 30m
+1h -30m       # the same as +30m
-2h -15m
```

Output always uses the compound form: whole hours are written as `8h`, durations shorter than an hour as `30m`, everything else as `1:26h`, and zero as `0h`. Negative durations always carry a `-`; a balance also carries an explicit `+` when it is positive.

## Specifying a start time

If you forgot to run `tracker start` when you started working, you can specify a start time when you run the command:

```
$ tracker start 08:30
[monday 2024-01-08]
* 08:30-
```

The time should be in `HH:MM` format (24-hour format).

## Transferring balance

Tracker only looks at the current week file when stating your report, so the balance a week ends with is carried into the next one as a line at the top of the new file:

```
# balance carried over from 2024-W03
* balance 3:12h

[monday 2024-01-22]
* 08:28-
```

This happens once, when the week file is created – by whichever command touches the new week first. The carried balance is the one the previous week *ended* with: every expected work day of that week counts, whether or not it was worked, and a shift that was never closed counts as nothing.

"Previous week" means the latest earlier week that has a file. A week without one – a holiday you never ran `tracker` in – is skipped rather than counted as a week of missed work. No balance is carried into a future week (`tracker -w 1`), since the week before it is not over, or into a file given with `-f`.

The line is an ordinary part of the file, so if the carried balance is wrong, or you want to start over at zero, edit or delete it. The same line can be written by hand, and a negative balance is written with a minus sign: `* balance -3:12h`. Any duration format described above works here – the separated form `* balance 3h 12m` is still read, and is rewritten in the compound form the next time `tracker` writes the file.

## Installation

This program is, as far as I'm aware, only used by myself. Please file an issue if this is no longer the case, I would love to know!

`tracker` is a crate in the [skagedal-tools](https://github.com/skagedal/skagedal-tools) workspace, so installing it needs a Rust toolchain and, from the workspace root, either:

```shell
./install tracker          # builds, checks and installs just this one
cargo install --path tracker
```

Shell completions are generated by the program itself, rather than by any install script – write them somewhere on your `fpath` (zsh) or equivalent:

```shell
tracker completions zsh > ~/local/zsh-functions/_tracker
```

Some short aliases are worth setting up too; `work` for `tracker start` and `wstop` for `tracker stop` are the two I use constantly.

## Configuration

`tracker` is configured with a file that currently has to exist: the program exits with an error if it is missing, rather than falling back to the defaults below. Create it even if you want the defaults. It sets the number of hours in a work day and the number of work days in a week. It lives at `~/.config/skagedal-tools/tracker/config.toml` on every platform, macOS included (override the root with `$XDG_CONFIG_HOME`). Week files are stored alongside, under `~/.local/share/skagedal-tools/tracker/week-files/`. Here is an example of what the config might look like: 

```toml
[workweek]
days_per_week = 4       # Defaults to 5
hours_per_day = 6       # Defaults to 8
```

## Alternatives

There are many time tracking tools out there. Here are some open source alternatives.

CLI-based:
* [Timewarrior](https://timewarrior.net/)
* [Watson](https://tailordev.github.io/Watson/)
* [timetrace](https://github.com/dominikbraun/timetrace)

Browser-based:
* [Activity Watch](https://activitywatch.net)
