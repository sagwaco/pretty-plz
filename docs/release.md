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

Both publish jobs run in the **`release-env`** GitHub Actions environment on the
pretty-plz repo. This is already configured — the section documents it.

### npm — OIDC trusted publishing (no token)

npm publishes via **OpenID Connect**: GitHub Actions mints a short-lived OIDC
token, npm verifies it against a trusted-publisher config, and no long-lived
token is stored anywhere. Provenance attestations are generated automatically.

Configured on npmjs.com (`@sagwaco/plz` → Settings → **Trusted Publisher**):

- Repository: `sagwaco/pretty-plz`
- Workflow: `release.yml`
- Environment: `release-env`

The job requires `permissions: id-token: write`, `environment: release-env`, and
npm ≥ 11.5.1 — it runs `npm install -g npm@latest` first, since Node 20 ships
npm 10. The repo must stay **public**.

> If you rename the workflow file, move the job out of `release-env`, or change
> the package name, update the trusted-publisher config to match — otherwise
> publishes are rejected.

### `HOMEBREW_TAP_TOKEN` — push to the tap (environment secret)

The default `GITHUB_TOKEN` can't push to another repo, so the `homebrew` job
authenticates to `sagwaco/homebrew-tap` with a fine-grained PAT stored as an
**environment secret** on `release-env`:

- Fine-grained PAT, resource owner `sagwaco`, scoped to **only**
  `sagwaco/homebrew-tap`, permission **Contents: Read and write**.
- Stored as the `release-env` environment secret **`HOMEBREW_TAP_TOKEN`** — which
  is why the `homebrew` job declares `environment: release-env`.

Fine-grained PATs expire (max 1 year) — rotate before then, or switch to an SSH
deploy key with write access for no expiry.

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
| `npm` job: OIDC / `401` / `Unable to authenticate` | Trusted-publisher mismatch — confirm npmjs.com lists repo `sagwaco/pretty-plz`, workflow `release.yml`, environment `release-env`, and the job has `id-token: write`. |
| `npm` job: OIDC unsupported / `EUSAGE` | npm too old — the `npm install -g npm@latest` step must run before publish (needs npm ≥ 11.5.1). |
| `npm` job: `E402` / version exists | npm can't republish a version. Cut a new patch (`scripts/release.sh patch`); never reuse a version. |
| `npm` / `homebrew` job stuck on "Waiting" | `release-env` has protection rules (required reviewers) — approve the run, or relax the environment rule. |
| `homebrew` job: `Permission … denied` / 403 on push | `HOMEBREW_TAP_TOKEN` missing/expired, not scoped to `homebrew-tap` with Contents:write, or the job is missing `environment: release-env`. |
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
| CI auth | npm OIDC trusted publishing + `HOMEBREW_TAP_TOKEN`, both via the `release-env` environment |
