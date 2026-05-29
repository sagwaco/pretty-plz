# Configuration

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
and latency-sensitive. `plz configure` (or `plz configure model`) lets you
pick a different model interactively; per-call `--provider` and `--model`
overrides are not persisted.

## Configure subcommands

Run the full wizard with `plz configure`, or adjust one piece:

```
plz configure login   # sign in to a provider (alias for `plz login`)
plz configure model   # change the default model for the configured provider
plz configure update  # upgrade plz (alias for `plz update`)
```

## Updates

When a newer release is available, `plz` prints a one-line hint to stderr.
Upgrade with `plz update` or `plz configure update`. Set `PLZ_NO_UPDATE_CHECK=1`
to disable the nudge. The command auto-detects how you installed plz (curl,
Homebrew, npm, or Cargo) and runs the matching upgrade path.
