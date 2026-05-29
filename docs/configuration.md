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
and latency-sensitive. `plz configure` lets you pick a different model
interactively; per-call `--provider` and `--model` overrides are not
persisted.
