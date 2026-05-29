# Homebrew

Tap: **[github.com/sagwaco/homebrew-tap](https://github.com/sagwaco/homebrew-tap)**

```bash
brew install sagwaco/tap/plz
```

The formula installs a **prebuilt binary** from the matching GitHub release
(no Rust toolchain required). [`plz.rb.tmpl`](plz.rb.tmpl) is the source of
truth: the release workflow renders it (filling in the version and per-platform
`sha256`s from `SHA256SUMS`) and pushes the result to `Formula/plz.rb` in the
tap. Don't hand-edit the tap's formula — change the template here instead.

Upgrade with `brew upgrade plz`, `plz update`, or `plz configure update`
(auto-detects Homebrew installs).

See [docs/release.md](../docs/release.md) for the release flow and the one-time
`HOMEBREW_TAP_TOKEN` setup that lets CI push to the tap.
