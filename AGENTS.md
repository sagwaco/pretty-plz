# AGENTS.md

This file provides guidance to agents like Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`plz` is a Rust CLI that turns a natural-language request into candidate shell commands. The user picks one; the chosen command is printed and (with shell integration) prefilled onto their next prompt. The repo is `pretty-plz`; the crate and binary are both named `plz` (`Cargo.toml` defines `[[bin]] name = "plz"`).

Rust **edition 2024** — requires Rust 1.85+.

## Commands

```bash
cargo build                          # debug build
cargo build --release                # release build (fat LTO, panic=abort, stripped)
cargo run -- "list files in cwd"     # run with a query (everything after -- is the query)
cargo test                           # run all tests
cargo test schema::                  # run tests in one module (e.g. provider/schema.rs)
cargo test commands_happy_path       # run a single test by name
cargo clippy --all-targets           # lint
cargo fmt                            # format
```

Tests are inline `#[cfg(test)] mod tests` blocks (no `tests/` dir). Modules with meaningful test coverage: `provider/schema.rs`, `context.rs`, `oauth/server.rs`, `oauth/openai.rs`, `oauth/pkce.rs`, `tui.rs`.

Manual run against a real provider needs a credential: either `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` in the environment, or a prior `plz login`. The local `.env` is gitignored and **not** auto-loaded by the binary — env vars must be exported in the shell.

## The stdout/stderr contract (most important invariant)

**Only the chosen command is ever written to stdout.** Everything else — the spinner, interactive pickers, prompts, error messages, status output — goes to **stderr**. This is load-bearing: the shell wrapper installed by `plz configure` captures `$(plz …)` to prefill the next prompt, and `cmd=$(plz …)` must capture *only* a runnable command.

Consequences enforced in code:
- `provider/schema.rs` rejects any command containing `\n`/`\r` (would be silently truncated by `$(…)`).
- `spinner.rs` and `tui.rs` write exclusively to stderr.
- `main::run_query` ends with a single `println!("{chosen}")` — the only stdout write in the query path.

Exit codes (`main.rs`): `0` ok, `1` user cancelled, `3` config/auth errors (`Config`, `NoApiKey`, `MissingProviderKey`, `NotSignedIn`, plus OAuth errors during `login`/`logout`), `2` everything else.

## Architecture

Request flow (`main::run_query`): build `Provider` from config/flags → `context::build()` assembles the environment block → first `provider.complete(turns)` → if the model returns `Clarify`, ask via `tui`, append the answer, call once more (a second clarify is an error, `ClarifyLoop`) → `tui::pick_command` → print to stdout.

### Provider abstraction (`provider/`)
- `Provider` trait: `model()` + `complete(&[Turn]) -> Result<Response>`. `Turn` is `User | Assistant(Response) | ClarifyAnswer` — the conversation re-serialized per provider wire format.
- `Kind` enum: `Anthropic | OpenAi | Codex`. `provider::build()` is the factory; it resolves the credential and constructs the concrete provider.
- Three implementations:
  - `anthropic.rs` — Messages API, structured output via a single **forced `tool_use`** (`tool_choice` = the `plz_response` tool).
  - `openai.rs` — public Responses API, **API-key only**, structured output via `json_schema` strict mode.
  - `codex.rs` — ChatGPT *subscription* access via plz-managed OAuth, hitting `chatgpt.com/backend-api/codex/responses`. Reuses `OpenAi::build_input` / `extract_output_text`.
- Default models live as `DEFAULT_*_MODEL` consts at the top of `provider/mod.rs` (fastest tier per provider — queries are short and latency-sensitive). **Bump those consts to rotate the recommended model.** `provider/models.rs` does live `/v1/models` enumeration for the `configure` picker, falling back to the small curated lists in `provider/mod.rs`.

### Shared response schema (`provider/schema.rs`)
Single source of truth for structured output, fed to **both** providers. `Response` is `Commands { 1–3 }` or `Clarify { question, choices ≤4 }`. Because OpenAI strict mode forbids `oneOf` and demands every field `required` with `additionalProperties: false`, the schema is one flat object where the unused branch is filled with empty array/empty string. Deserialization is deliberately lenient (`lenient_vec`/`lenient_string` accept `""`/`null` for the unused branch) but still rejects genuine type errors. The system prompt lives in `prompt.rs` and biases toward returning a **single** command.

### Auth & credentials (`provider/auth.rs`, `oauth/`, `api_key.rs`)
- Precedence (default `--auth auto`): env API key → `plz login`-pasted key (`<config_dir>/keys/`) → OAuth tokens (`<config_dir>/oauth/`). `--auth api|oauth` (or `PLZ_AUTH`) forces one mode.
- `auth::call_with_auth` is the shared HTTP helper: attaches the right header(s) and, on a 401 with OAuth, force-refreshes and retries **exactly once**.
- `oauth/anthropic.rs` — PKCE **manual code-paste** flow. Constants (CLIENT_ID, URLs, `OAUTH_BETA_HEADER`) are borrowed from the official `claude` CLI and are undocumented/may rotate; quirks: `state` must equal the PKCE verifier, the exchange echoes `state` back, and `anthropic-beta: oauth-2025-04-20` is required on OAuth Messages calls. **First place to check on a persistent 401.**
- `oauth/openai.rs` — ChatGPT PKCE **loopback** flow; the callback redirect (`localhost:1455/auth/callback`) is strictly validated by the IdP — do not change host/port/path.
- `oauth/tokens.rs` — token storage + refresh. Critical distinction: a token-endpoint **4xx** → `OAuthInvalidGrant` → wipe local tokens and force re-login; **5xx/network** → propagate and **keep** tokens (a transient blip must not delete a valid refresh_token).
- `secret_file.rs` — atomic writes for all secrets: temp file at mode `0600` + `rename`, parent dir forced to `0700` (Unix).

### Context & prompt-injection defense (`context.rs`)
Builds the environment block (OS, shell, pwd, directory listing capped at 50 entries). Filenames are untrusted, so the listing is wrapped in a **per-call randomized fence** (`===PLZ-UNTRUSTED-<rand>===`) that a malicious filename can't forge, and the system prompt instructs the model to treat fenced content as data.

### Shell integration (`shell.rs`)
`plz init <zsh|bash>` prints a wrapper function; `plz configure` appends `eval "$(plz init …)"` to the rc file (`.zshrc`, or `.bash_profile` on macOS / `.bashrc` on Linux). The wrapper captures plz's stdout and pushes it onto the next prompt — zsh `print -z`, bash via a DSR / readline-macro polyfill. It deliberately **skips subcommands** (`login`, `status`, `init`, `uninstall`, …) so those pass straight through.

### Config (`config.rs`)
TOML at the platform config dir (`ProjectDirs::from("dev", "sanglee", "plz")`). First run auto-detects a provider from available credentials and writes defaults. Saves are atomic (temp + rename).

## Subcommands

`plz <query>` (default), `plz configure` (guided setup), `plz configure login` (alias for `plz login`), `plz configure model` (change default model), `plz configure update` (alias for `plz update`), `plz login [provider]`, `plz logout <provider>`, `plz status`, `plz update`, `plz uninstall [-y]`, `plz init [shell]`. Provider aliases: `anthropic`/`claude`, `openai`/`gpt`, `chatgpt`/`codex`/`openai-codex`.

## Release

Tagging `v*` triggers `.github/workflows/release.yml`: cross-builds macOS (arm64/x86_64) and Linux musl (arm64/x86_64) binaries, tars them with `SHA256SUMS`, and publishes a GitHub release including `install.sh`. `install.sh` (curl-pipe-sh installer) downloads the matching archive.
