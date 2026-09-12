# git-assistant: one tool for the repositories under a directory

Implements [#84](https://github.com/skagedal/skagedal-tools/issues/84).

`git-branch-assistant` has grown from a branch cleaner into the tool that
walks a directory of repositories and reports on each one: `repos` for
branches, `activity` for the latest commit. `git-dirty-checker` does the
same walk to report uncommitted changes, with its own copy of the walk,
its own copy of the dirty check, and its own way of telling a shell
wrapper where to `cd`. This folds it in as a `dirty` subcommand and
renames the result `git-assistant`, since branches are now one of several
things it looks at.

Nothing about what the dirty check *does* changes. The point is one
implementation of each shared piece, and one command-line convention
across the subcommands.

## Functionality

### The commands

    git-assistant clean    [REPO]      sync one repository's branches
    git-assistant repos    [ROOT...]   the same, for every repository under the roots
    git-assistant activity [ROOT...]   repositories by latest commit, oldest first
    git-assistant dirty    [ROOT...]   repositories with uncommitted changes

`clean`, `repos` and `activity` behave exactly as they do today under
`git-branch-assistant`, apart from how their paths are given (below).

`dirty` is `git-dirty-checker` as it stands: without flags it prints the
path of every repository with uncommitted changes that is not snoozed,
one per line, sorted. `--interactive` opens the picker — `j`/`k` or the
arrows to move, `/` to search, `s` to snooze for an hour (again to add
another), `u` to unsnooze, Enter to pick — with snoozed repositories in
grey at the bottom. `--exit-if-no-active-dirty` keeps its meaning: with
`--interactive`, exit silently if every dirty repository is snoozed.

### Paths

Every subcommand takes its paths as positional arguments and defaults to
the current directory. The split today — `clean` and `repos` take
`--path`, `activity` and `git-dirty-checker` take positionals — is an
accident of which was written when, and positionals win because the
multi-root commands need them: `git-assistant dirty ~/aira ~/code` reads
as what it is.

`clean` takes at most one, since it works on a single repository. The
others take any number of roots and treat each root's child directories
as the candidates, as `activity` and `git-dirty-checker` already do.
That makes `repos` multi-root too, which it has not been; `repos --list
~/aira ~/code` is the obvious thing to want once `activity` can do it.

`--path`/`-p` is removed rather than kept as an alias. Every caller is in
the dotfiles repository and changes in the same pass (see below), and
none of them passes `--path` — the assistant tasks use `directory:`.

A root that does not exist or cannot be listed is reported on stderr and
skipped, and the command carries on with the rest and exits as it
otherwise would. `activity` fails outright on such a root today and
`git-dirty-checker` skips it silently; neither is right for a command the
daily assistant runs over `~/worktrees-aira`, which may well not exist on
a given machine. A child directory that is not a git repository is still
skipped without comment.

### Moving to the new name

The first run of `dirty` after the rename moves the existing snooze
records to the new tool's data directory, so a repository snoozed an hour
ago under `git-dirty-checker` is still snoozed. It does this only when the
old directory exists and the new one does not, and says nothing about it.

The branch-list cache under `~/.cache/skagedal-tools/git-branch-assistant/`
is not moved. It is a cache; the first `repos --list --interactive` under
the new name rescans, and the old directory can be deleted by hand.

`./install` removes the `git-branch-assistant` and `git-dirty-checker`
binaries it installed earlier, so the old names stop working rather than
quietly running a stale build.

## Implementation

### The crate

`git-branch-assistant/` is renamed `git-assistant/` with `git mv`, and
`Cargo.toml` changes `name` to match. The workspace `members` list in the
root `Cargo.toml` drops `git-dirty-checker` and renames the other.
`git-dirty-checker/` is deleted, `other-implementations/` with it: the
shell and Java versions were kept for comparison while the Rust one was
new, and git history still has them. With them goes the "Java Code
Formatting" section of the root `AGENTS.md`, which has nothing left to
apply to.

`clap`'s `#[command(name = ...)]` becomes `git-assistant`, and the `about`
text becomes "Helper commands for the git repositories under a directory".

### `dirty`

`git-dirty-checker/src/main.rs` splits along its existing seams:

- `src/commands/dirty.rs` — the command: collect roots, find dirty
  repositories, print or run the picker, write the suggested directory.
  Thin, like its siblings.
- `src/services/dirty_service.rs` — the parallel scan. It uses the root
  walk that `repo_activity_service::collect_entries` already has, pulled
  out into `fs_utils::child_directories(roots) -> Vec<PathBuf>` so that
  `activity`, `dirty` and multi-root `repos` share it, including its
  `is_globally_ignored` filter. The dirty check itself is
  `GitRepo::is_dirty`, which is the same `git status --porcelain` test
  `is_dirty_repository` makes. A repository whose `git status` fails is
  treated as clean, as now.
- `src/snooze.rs` — `is_snoozed`, `snooze_expiry`, `snooze`, `unsnooze`,
  and the one-time move. Storage is unchanged: one empty file per
  repository in `skagedal_dirs::data_dir("git-assistant").join("snoozed")`,
  named by the canonical path with `/` replaced by `__`, whose mtime is
  the expiry. The move is a `fs::rename` of
  `data_dir("git-dirty-checker")/snoozed` when that exists and the new one
  does not.
- `src/dirty_picker.rs` — the ratatui UI, moved as it is. `ratatui` and
  `crossterm` join the crate's dependencies; `filetime` too, for the
  mtimes.

`output_selected` goes away in favour of
`Repository::set_suggested_directory`, followed by exit code 10, which is
what `clean` already does. The one behavioural difference is in the
crate's favour: it expands a leading `~` in `SUGGESTED_CD_FILE` and
creates the file's parent directory.

### Paths in `main.rs`

`Clean { path: Option<PathBuf> }` becomes a positional `repo:
Option<PathBuf>`. `Repos` and `Activity` and the new `Dirty` take `roots:
Vec<PathBuf>`, resolved to `vec![env::current_dir()?]` when empty by one
helper in `main.rs` rather than in each command. `git_repos::run` takes
`&[PathBuf]` in place of its `Option<PathBuf>`, and the branch-list cache
key becomes the sorted, canonicalised roots joined with `\n` rather than
the invocation directory. `collect_activity` changes from failing on an
unreadable root to warning, via the shared walk.

### Install and check

In `shared.sh`, `INSTALLED_RUST_TOOLS` loses `git-branch-assistant` and
`git-dirty-checker` and gains `git-assistant`. A new list sits under it:

```bash
# Binaries this repo used to install under names it no longer has.
# ./install uninstalls them, so an old name fails instead of running a
# stale build.
RETIRED_RUST_TOOLS=(
    git-branch-assistant
    git-dirty-checker
)
```

and `install` runs `cargo uninstall "$tool"` for each one `cargo install
--list` still shows. The list is kept, not emptied after one run: it
costs nothing, and the next machine to be set up from an old checkout
needs it as much as this one.

### Documentation

`git-assistant/README.md` is the old `git-branch-assistant/README.md`
with the name changed, `--path` examples rewritten, and a "Dirty
repositories" section built from `git-dirty-checker/README.md` — the
features list and the `SUGGESTED_CD_FILE` section, minus Building and
Other implementations. `git-assistant/AGENTS.md` gets the rename and a
line on `dirty` in the project overview.

The tools table in the root `README.md` loses two rows and gains:

    | [git-assistant](git-assistant/) | Syncs branches, lists repos by latest commit, and finds uncommitted changes, across one or many repos |

The per-tool state example in the root `AGENTS.md` changes its path to
`~/.local/share/skagedal-tools/git-assistant/snoozed/`.

### Outside this repository

In `~/code/dotfiles`, committed together with the rename:

- `simons-assistant/tasks.yml`: the `git-dirty-checker` task becomes

  ```yaml
  - id: git-dirty
    shell: git-assistant dirty ~/aira ~/code ~/worktrees-aira ~/worktrees-code --interactive --exit-if-no-active-dirty
    when: always
  ```

  and the three `git-branch-assistant repos` tasks change the binary name
  and nothing else. Renaming the task id resets whatever `assistant`
  remembers about when it last ran, which for a `when: always` task is
  nothing that matters.
- `bin/simon-install-personal-software.sh` enters `$CODE/git-branch-assistant`
  and `bootstrap-repos/bootstrap.sh` clones
  `skagedal/git-branch-assistant`. Neither repository exists any more;
  both lines are deleted, since `skagedal-tools`'s own `./install` covers
  the tool.

## Open questions

- **Two pickers.** `repos --list --interactive` has a hand-rolled picker
  on `console`, with background refresh; `dirty` brings a ratatui one
  with search and snoozing. Moving `dirty` across as it is keeps this
  change a merge rather than a rewrite, but leaves the crate with two UI
  stacks. Folding them into one is worth doing once both live in the same
  crate — search and snoozing would be useful on the branch list too —
  and is left for a later change.
- **Snoozing branches as well.** Snoozing is keyed by repository path and
  only `dirty` reads it. Nothing here makes `repos` honour it, and it is
  not obvious that a repository snoozed for being dirty should also drop
  out of the branch list.
- **The flag name.** `--exit-if-no-active-dirty` is kept for continuity,
  but every caller changes in this pass anyway, so this is the cheapest
  time there will ever be to call it something shorter, such as
  `--quiet-if-snoozed`.
