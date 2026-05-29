# plz

Natural-language to shell-command CLI. Ask for what you want, choose from up to
3 candidate commands, then run the selected command yourself.

```
$ plz find the biggest files in this folder
⠹ claude-haiku-4-5:
? Pick a command:
> du -ah . | sort -rh | head -n 10
    list every file's size and show the 10 largest
  find . -type f -exec du -h {} + | sort -rh | head
    walk the tree with find and surface the biggest files
  du -sh */ | sort -rh
    summarize each top-level subdirectory and sort by size
```

`plz` biases toward a single best command — multiple choices only appear when
the alternatives meaningfully differ (different tools, tradeoffs, or behavior).

The chosen command is printed to stdout, so it works with command substitution:

```
cmd=$(plz find duplicate jpegs under cwd)
eval "$cmd"
```

## Shell integration

With auto-prefill enabled, the command you pick is dropped onto your next shell
prompt — pre-filled and editable, not yet run — instead of just being printed
for you to copy:

```
$ plz print the most common filetype here and in subdirectories
⠹ claude-haiku-4-5:
    walk the tree and pick the extension with the highest count
$ find . -type f -name '*.*' | rev | cut -d. -f1 | rev | sort | uniq -c | sort -rn | head -1 | awk '{print $2}'▮
```

`plz configure` (see Setup below) installs it for you. Under the hood: `plz`
prints the chosen command to stdout, and a small shell wrapper captures that
stdout and pushes it onto the next prompt — using zsh's `print -z` or a DSR /
readline-macro polyfill on bash. `cmd=$(plz …)` still works because the wrapper
explicitly skips subcommands and uses the same `$(…)` capture itself.

If you'd rather wire it up by hand, skip `plz configure` and add this to your
shell's rc file:

```
# ~/.zshrc
eval "$(plz init zsh)"

# ~/.bashrc  (or ~/.bash_profile on macOS)
eval "$(plz init bash)"
```

(Omit the shell name — `eval "$(plz init)"` — to detect it from `$SHELL`.)

## Install

Requires Rust 1.85+.

```
git clone git@github.com:sagwaco/pretty-plz.git
cd pretty-plz
cargo install --path .
```

Make sure `~/.cargo/bin` is on your `PATH`.

## Setup

Run the guided setup once after installing — it walks you through picking a
provider and installing the shell wrapper that auto-prefills the next prompt:

```
plz configure
```

Or manage the pieces separately:

```
plz login                  # interactive provider picker
plz login anthropic        # Claude OAuth
plz login openai           # paste an OpenAI API key
plz login chatgpt          # ChatGPT OAuth
plz status
plz logout <provider>
```

You can also use environment variables:

```
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
```

Credential priority is env var, pasted API key, then stored OAuth tokens.
ChatGPT OAuth is owned by `plz`, like Claude account auth: `plz` opens the
browser, receives the ChatGPT callback locally, stores tokens under
`<config_dir>/oauth/chatgpt.json`, and refreshes them on use. Override a call
with `--provider`, `--model`, or `--auth auto|api|oauth`.

## Usage

```
plz <natural-language query>...
plz --provider openai show disk usage of subdirs sorted descending
plz --model claude-haiku-4-5 convert input.svg to a transparent 512x512 png
```

Quotes are optional because everything after the flags is treated as the query.
Run `plz --help` for all flags.

## Configuration

On first run, `plz` writes a config file under your platform config directory:

- macOS: `~/Library/Application Support/dev.sanglee.plz/config.toml`
- Linux: `~/.config/plz/config.toml`

Example:

```toml
provider = "anthropic"
anthropic_model = "claude-haiku-4-5"
openai_model = "gpt-5-mini"
codex_model = "gpt-5-mini"
```

Defaults pick the fastest tier per provider, since `plz` queries are short
and latency-sensitive. `plz configure` lets you pick a different model
interactively; per-call `--provider` and `--model` overrides are not
persisted.

## Privacy

`plz` sends your query, OS, shell, current directory, and a truncated listing of
the current directory to the selected model. It does not send shell history, and
it has no telemetry or usage tracking.

## Development

```
cargo build
cargo build --release
cargo run -- "list files in cwd"
```

## License

Apache 2.0. See [LICENSE](LICENSE).
