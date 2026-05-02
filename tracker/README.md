# tracker

A simple command line program to help keep track of work time by storing data in a simple per-week text file. It is designed for the use case where you have flexible work hours, but wish to keep track that you work a certain number of hours per week. By default, it assumes that you work 8 hours per day, 5 days per week, but this can be configured.

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
You have worked 3 h 12 m today.
You have worked 3 h 12 m this week.
Balance: -4 h 48 m
```

The balance tells you that you have 4 hours and 48 minutes left to work this day in order to be in balance. 

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

## Specifying a start time

If you forgot to run `tracker start` when you started working, you can specify a start time when you run the command:

```
$ tracker start 08:30
[monday 2024-01-08]
* 08:30-
```

The time should be in `HH:MM` format (24-hour format).

## Transferring balance

Tracker will only look at the current week file when stating your report. If you wish to transfer a balance from a previous week, it can be done by adding a line like this to the top of the current week file: 

```
* balance 3h 12m
```

This will add 3 hours and 12 minutes to the balance for the current week. 

(Under development: Such as balance shift will be added automatically when you start a new week.)

## Installation

This program is, as far as I'm aware, only used by myself. Please file an issue if this is no longer the case, I would love to know! If you wish to install it, you would have to set up a Rust development environment and run `cargo install` in the root of the repository. I would also recommend setting up the shell completions – take a look in the file `install.sh` for how to do this – and setting up some nice aliases, for example `work` for `tracker start`.

## Configuration

`tracker` can be customized with a configuration file. Currently, you can set the number of hours in a work day and the number of work days in a week. The location of the file is system-dependent; on Linux, it is expected to be located at `~/.config/tracker/config.toml`. Here is an example of what it might look like: 

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
