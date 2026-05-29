# pretty-plz

Natural-language to shell-command CLI. Ask for what you want, pick a command, run it.

```
$ plz find the biggest files in this folder
⠹ claude-haiku-4-5:
? Pick a command:
> du -ah . | sort -rh | head -n 10
    list every file's size and show the 10 largest
```

The chosen command is printed to stdout — use it directly or with command substitution:

```
cmd=$(plz find duplicate jpegs under cwd)
eval "$cmd"
```

## Quickstart

```bash
curl -fsSL https://raw.githubusercontent.com/sagwaco/pretty-plz/main/install.sh | sh
plz configure   # pick a provider and enable shell auto-prefill
plz find the biggest files here
```

Or build from source (requires Rust 1.85+):

```bash
git clone git@github.com:sagwaco/pretty-plz.git
cd pretty-plz
cargo install --path .
```

Make sure `~/.cargo/bin` is on your `PATH` if you built from source.

Or set credentials manually and skip configure:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
plz list files in cwd
```

## Usage

```
plz <natural-language query>...
plz --provider openai show disk usage of subdirs sorted descending
plz --model claude-haiku-4-5 convert input.svg to a transparent 512x512 png
```

Quotes are optional — everything after the flags is the query. Run `plz --help` for all flags.

See [docs/](docs/) for shell integration, authentication, and configuration.

## Privacy

`plz` sends your query, OS, shell, current directory, and a truncated listing of
the current directory to the selected model. It does not send shell history, and
it has no telemetry or usage tracking.

## Development

```bash
cargo build
cargo build --release
cargo run -- "list files in cwd"
```

## License

Apache 2.0. See [LICENSE](LICENSE).
