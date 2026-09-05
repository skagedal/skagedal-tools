# assistant

**assistant** is a command line application that performs recurring tasks and tells you what to do next. I use it as a "driver" of my daily routine when I'm at the computer. Whenever I don't know what else to do, I type the command `n` (for "next") into my shell, which is a thin shell wrapper around the `assistant` program. It will then follow the script in your configuration file and run the tasks that are due.     

## Installation

Currently, there is no binary distribution available, so you will have to have a Rust toolchain installed. 

Run `cargo install` from the root of this repository. This installs the `assistant` binary in the default location where cargo installs binaries; typically it will be  `~/.cargo/bin/assistant`.

You can use the `assistant` tool directly. However, if you would like some tighter integration with your shell, specifically allow certain commands to change the current directory of your shell, you need to execute it with a shell function wrapper. This lives in `shell/assistant.zsh`. You can source this file from your shell startup script.

## Configuration

Create a file called ~/.config/skagedal-tools/assistant/tasks.yml. This should be an YML array of tasks that `assistant` checks for you. Example:

```yaml
- id: check-github-notifications # Needs to be something unique
  shell: ./check-github-notifications.sh # Script to start
  directory: ~/kry/code/simon-kry/scripts 
  when: daily
```

The `when` field can be one of the following:
* `hourly`
* `every N hours`, where N is an integer
* `daily`
* `every N days`, where N is an integer
* `weekly`

## Useful tools to handle subtasks

* [simons-assistant](https://github.com/skagedal/simons-assistant) for git
* [gmail-inbox-status](https://github.com/skagedal/gmail-inbox-status) for gmail
