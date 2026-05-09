# simons-misc-helpers

While this repo (skagedal-tools) is a collection of tools that do not (yet) deserve their own git repository, this tool is a collection of even smaller helpers that do not even deserve their own tool. 

Think of it as "this could have been a bash script". (But they never stay bash-script-sized, do they?)

##  Subcommands

### format-dependency-changes

This is a helper for Andrew Nesbitt's [git-pkgs](https://github.com/git-pkgs/git-pkgs) tool. it takes the output of `git pkgs diff --format=json` and formats it again, similar to the output of `git pkgs diff`, but with a difference that I want – major version bumps are highlighted.
