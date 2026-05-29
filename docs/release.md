# Release guide

Cutting a `plz` release is one command. The git tag is the single trigger:
pushing `vX.Y.Z` runs [`.github/workflows/release.yml`](../.github/workflows/release.yml),
which builds the binaries and publishes to **GitHub Releases**, **npm**, and the
**Homebrew tap** automatically.

## Cut a release

```bash
scripts/release.sh patch     # 0.1.1 -> 0.1.2  (also: minor | major | <X.Y.Z>)
```

The script bumps `Cargo.toml` + `Cargo.lock` + `npm/pretty-plz/package.json`,
commits `Release vX.Y.Z`, tags it, and (after a `y/N` prompt) pushes `main` and
the tag. That's it — the pipeline does the rest.

Watch it:

```bash
gh run watch --workflow=release.yml
```

### What the pipeline does on a `v*` tag

| Job | Output |
|-----|--------|
| `build` | Cross-builds 4 targets (macOS arm64/x86_64, Linux musl arm64/x86_64) |
| `release` | GitHub Release with the 4 tarballs, `SHA256SUMS`, `install.sh` |
| `npm` | Publishes `@sagwaco/plz@X.Y.Z` (with provenance) — version taken from the tag |
| `homebrew` | Renders [`packaging/homebrew/plz.rb.tmpl`](../packaging/homebrew/plz.rb.tmpl) and pushes `Formula/plz.rb` to [`sagwaco/homebrew-tap`](https://github.com/sagwaco/homebrew-tap) |

`npm` and `homebrew` both wait on `release`, so they only run once the GitHub
assets exist (npm's `postinstall` and the Homebrew formula download from them).

### Versioning model

The git tag is authoritative. `Cargo.toml` is the canonical in-repo version
(the compiled binary's `--version` comes from it, which is why the bump must be
in the tagged commit); `package.json` and the Homebrew formula version are
**derived from the tag** in CI. `scripts/release.sh` keeps all three in sync so
the repo stays honest, but if you ever tag by hand, CI still publishes the
tag's version to npm/Homebrew.

---

## One-time setup

These secrets must exist on the **pretty-plz** repo (Settings → Secrets and
variables → Actions). Without them the `npm` / `homebrew` jobs fail.

### `NPM_TOKEN` — npm publish

npm requires 2FA for publishing, so interactive OTP won't work in CI. Use a
token that bypasses it:

1. [npmjs.com](https://www.npmjs.com/) → Access Tokens → **Generate New Token**
   → **Granular Access Token** (or classic **Automation** token).
2. Scope: read/write on `@sagwaco/plz` (or the whole `@sagwaco` scope).
3. Enable **Bypass 2FA** (granular) — automation tokens bypass it by default.
4. Add it as repo secret **`NPM_TOKEN`**.

Provenance is enabled (`--provenance`, `id-token: write`). It needs the repo to
stay **public** and `package.json`'s `repository.url` to match the repo — both
already true.

### `HOMEBREW_TAP_TOKEN` — push to the tap

The default `GITHUB_TOKEN` can't push to another repo, so the `homebrew` job
authenticates to `sagwaco/homebrew-tap` with its own token:

1. GitHub → Settings → Developer settings → **Fine-grained personal access
   tokens** → Generate new token.
2. Resource owner: `sagwaco`. Repository access: **Only select repositories** →
   `sagwaco/homebrew-tap`.
3. Permissions: **Contents → Read and write**.
4. Add it as repo secret **`HOMEBREW_TAP_TOKEN`**.

Fine-grained PATs expire (max 1 year). For a no-expiry alternative, add an SSH
**deploy key** with write access to the tap and switch the job's checkout to it.

The main repo must stay **public** (npm provenance and the Homebrew/install.sh
download paths assume public release assets).

---

## Testing without a real release

Cut a prerelease tag — the script appends `-rc1`:

```bash
scripts/release.sh patch --rc      # e.g. v0.1.2-rc1
```

For a prerelease tag (any tag containing `-`):

- `npm` publishes under the **`next`** dist-tag, not `latest`
  (`npm install -g @sagwaco/plz@next` to try it; `@latest` is untouched).
- `homebrew` is **skipped** (the stable formula isn't moved).

Once happy, cut the real version with `scripts/release.sh patch`.

---

## Verify a release

```bash
# curl installer
curl -fsSL https://raw.githubusercontent.com/sagwaco/pretty-plz/main/install.sh | sh
plz --version

# npm
npm install -g @sagwaco/plz && plz --version

# Homebrew
brew update && brew upgrade plz   # or: brew install sagwaco/tap/plz
plz --version
```

`plz update` may take up to 24h to notice the new version (update-check cache —
delete `<config_dir>/update_check.json` to force a recheck).

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `npm` job: `E403 … two-factor authentication … required` | The token doesn't bypass 2FA — regenerate as an Automation token or a Granular token with **Bypass 2FA**, update `NPM_TOKEN`. |
| `npm` job: `E403 … you do not have permission to publish` | Token isn't scoped to `@sagwaco/plz`, or the `@sagwaco` org/your membership is missing. |
| `npm` job: `E402` / version exists | npm can't republish a version. Cut a new patch (`scripts/release.sh patch`); never reuse a version. |
| `homebrew` job: `Permission … denied` / 403 on push | `HOMEBREW_TAP_TOKEN` missing, expired, or not scoped to `homebrew-tap` with Contents:write. |
| `homebrew` job: `missing sha256 for …` | A build target didn't produce its tarball — check the `build` matrix. |
| `release.sh`: `working tree is dirty` / `not on main` / `not in sync` | Commit/stash, switch to `main`, or `git pull` first — the script refuses to release from an unclean state. |

---

## Quick reference

| Artifact | Location |
|----------|----------|
| Canonical version | `Cargo.toml` (synced to `package.json` by `scripts/release.sh`) |
| Release entrypoint | `scripts/release.sh` → pushes the `v*` tag |
| CI pipeline | [`.github/workflows/release.yml`](../.github/workflows/release.yml) |
| Homebrew formula (source of truth) | [`packaging/homebrew/plz.rb.tmpl`](../packaging/homebrew/plz.rb.tmpl) → rendered to the tap |
| Homebrew tap (published) | [github.com/sagwaco/homebrew-tap](https://github.com/sagwaco/homebrew-tap) → `Formula/plz.rb` |
| npm package | `npm/pretty-plz/` → registry `@sagwaco/plz` |
| Required secrets | `NPM_TOKEN`, `HOMEBREW_TAP_TOKEN` |
