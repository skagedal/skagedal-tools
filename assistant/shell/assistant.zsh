# The `n` shell function: run the next assistant task, and cd where it asks.
#
# Tools that can suggest a directory — git-dirty-checker, git-branch-assistant,
# sfslint — write it to whatever $SUGGESTED_CD_FILE names. They run as
# subprocesses of `assistant`, so the variable has to reach that whole tree,
# but nothing outside this function needs it: run those tools by hand without
# it and they print the path to stdout instead, which is what you want there.
#
# A fresh temp file per run, rather than a fixed path. The old value was
# ~/.assistant/data/requested-directory, a directory nothing ever created — so
# on a new machine every write failed and `n` simply never cd'd anywhere.
# Nothing said so: git-dirty-checker discarded the result of that write and
# exited 10 either way. A per-run file also cannot go stale, which a fixed one
# can if a previous run was interrupted between writing and reading.
function n() {
  local cd_file target=""

  cd_file="$(mktemp -t assistant-cd)" || return 1

  if [ $# -gt 0 ]; then
    SUGGESTED_CD_FILE="$cd_file" assistant run "$1"
  else
    SUGGESTED_CD_FILE="$cd_file" assistant next
  fi

  # mktemp creates the file, so emptiness is the signal here, not existence.
  [[ -s "$cd_file" ]] && target="$(<$cd_file)"
  rm -f "$cd_file"

  [[ -n "$target" ]] && cd "$target"
}
