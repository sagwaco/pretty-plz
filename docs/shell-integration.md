# Shell integration

With auto-prefill enabled, the command you pick is dropped onto your next shell
prompt — pre-filled and editable, not yet run — instead of just being printed
for you to copy:

```
$ plz print the most common filetype here and in subdirectories
⠹ claude-haiku-4-5:
    walk the tree and pick the extension with the highest count
$ find . -type f -name '*.*' | rev | cut -d. -f1 | rev | sort | uniq -c | sort -rn | head -1 | awk '{print $2}'▮
```

`plz configure` installs it for you. Under the hood: `plz` prints the chosen
command to stdout, and a small shell wrapper captures that stdout and pushes it
onto the next prompt — using zsh's `print -z` or a DSR / readline-macro polyfill
on bash. `cmd=$(plz …)` still works because the wrapper explicitly skips
subcommands and uses the same `$(…)` capture itself.

If you'd rather wire it up by hand, skip `plz configure` and add this to your
shell's rc file:

```
# ~/.zshrc
eval "$(plz init zsh)"

# ~/.bashrc  (or ~/.bash_profile on macOS)
eval "$(plz init bash)"
```

(Omit the shell name — `eval "$(plz init)"` — to detect it from `$SHELL`.)
