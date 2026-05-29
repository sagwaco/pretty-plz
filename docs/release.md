# Release checklist

Use this when cutting a new `plz` release. GitHub releases must exist before npm
publish — the npm `postinstall` script downloads platform binaries from GitHub.

## One-time setup (done)

| Channel | Status | Notes |
|---------|--------|-------|
| GitHub releases | ✅ v0.1.0 live | Tag push triggers [`.github/workflows/release.yml`](../.github/workflows/release.yml) |
| Homebrew tap | ✅ live | [`sagwaco/homebrew-tap`](https://github.com/sagwaco/homebrew-tap) — `brew install sagwaco/tap/plz` |
| npm | ⬜ pending first publish | Package name is **`@sagwaco/plz`** — `pretty-plz` is taken on npm by another project |

The main repo must stay **public** — the Homebrew formula clones source from GitHub.

---

## Version bumps (pretty-plz repo)

- [ ] Bump `version` in [`Cargo.toml`](../Cargo.toml)
- [ ] Bump `version` in [`npm/pretty-plz/package.json`](../npm/pretty-plz/package.json) (same semver, no `v` prefix)
- [ ] Update [`packaging/homebrew/plz.rb`](../packaging/homebrew/plz.rb):
  - `tag: "vX.Y.Z"`
  - `revision: "<full commit sha>"` — run `git rev-parse vX.Y.Z` after tagging
- [ ] Commit: `Release vX.Y.Z` (or similar)

## GitHub release

- [ ] Create and push the tag (triggers the release workflow):

  ```bash
  git tag vX.Y.Z
  git push origin vX.Y.Z
  ```

- [ ] Wait for the **Release** workflow to finish
- [ ] On the GitHub release page, confirm these assets exist:
  - `plz-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `plz-vX.Y.Z-x86_64-apple-darwin.tar.gz`
  - `plz-vX.Y.Z-aarch64-unknown-linux-musl.tar.gz`
  - `plz-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz`
  - `SHA256SUMS`
  - `install.sh`
- [ ] Smoke-test the curl installer:

  ```bash
  curl -fsSL https://raw.githubusercontent.com/sagwaco/pretty-plz/main/install.sh | sh
  plz --version
  ```

## npm

Prerequisite: GitHub release assets for `vX.Y.Z` must be live.

Package: **`@sagwaco/plz`** (scoped — do not use `pretty-plz`, that name is taken).

**Prerequisites:**

- npm account with **2FA enabled** (required to publish — npm returns `E403` without it)
- The `@sagwaco` org on npmjs.com, and your account added as owner/publisher
- Logged in via `npm login` or a granular access token (see below)

- [ ] Enable 2FA on [npmjs.com](https://www.npmjs.com/) → Account → **Enable 2FA** → choose **Authorization and publishing**
- [ ] Create the `@sagwaco` org (if needed) and add yourself as owner
- [ ] Log in: `npm login` (you'll be prompted for username, password, email, and OTP)
- [ ] Confirm `npm/pretty-plz/package.json` version matches the tag
- [ ] Dry run:

  ```bash
  cd npm/pretty-plz
  npm publish --dry-run --access public
  ```

- [ ] Publish:

  ```bash
  npm publish --access public
  ```

  npm will prompt for your 2FA one-time code.

- [ ] Smoke-test:

  ```bash
  npm install -g @sagwaco/plz
  plz --version
  ```

**Alternative: granular access token (CI or if you prefer tokens over interactive login)**

1. npmjs.com → Access Tokens → **Generate New Token** → **Granular Access Token**
2. Permissions: read/write for `@sagwaco/plz` (or the whole `@sagwaco` scope)
3. Enable **Bypass 2FA for publish** (if offered — only for automation tokens)
4. Publish with:

   ```bash
   npm publish --access public --otp=123456   # if using login + TOTP each time
   # or
   npm config set //registry.npmjs.org/:_authToken=npm_...
   npm publish --access public
   ```

**Common errors:**

| Error | Fix |
|-------|-----|
| `E403` … Two-factor authentication … required | Enable 2FA on your npm account, then re-run `npm login` and publish again |
| `E403` … you do not have permission to publish | Create `@sagwaco` org or publish under your user scope (`@youruser/plz`) |
| `E402` / version already exists | Bump patch version in `package.json` — can't republish `0.1.0` |

Note: npm does not allow republishing the same version. If publish fails after partial upload, bump the patch version or unpublish within 72 hours (discouraged).

## Homebrew tap

Tap repo: **[github.com/sagwaco/homebrew-tap](https://github.com/sagwaco/homebrew-tap)**  
Install: `brew install sagwaco/tap/plz`

Per release — sync the formula from this repo into the tap:

- [ ] Copy updated [`packaging/homebrew/plz.rb`](../packaging/homebrew/plz.rb) → `Formula/plz.rb` in [`homebrew-tap`](https://github.com/sagwaco/homebrew-tap)
- [ ] Commit and push to `homebrew-tap`
- [ ] Smoke-test:

  ```bash
  brew update
  brew upgrade plz
  plz --version
  ```

  Or for a fresh install:

  ```bash
  brew install sagwaco/tap/plz
  ```

The formula builds from source (requires Rust). Users can also install directly from this repo without the tap:

```bash
brew install ./packaging/homebrew/plz.rb
```

## Post-release

- [ ] Verify `plz update` detects the new version (may take up to 24h due to update-check cache, or delete `<config_dir>/update_check.json`)
- [ ] Update release notes on GitHub if needed

## Quick reference

| Artifact | Location |
|----------|----------|
| Crate / binary version | `Cargo.toml` |
| npm package | `npm/pretty-plz/` → registry name `@sagwaco/plz` |
| Homebrew formula (source of truth) | `packaging/homebrew/plz.rb` |
| Homebrew tap (published copy) | [github.com/sagwaco/homebrew-tap](https://github.com/sagwaco/homebrew-tap) → `Formula/plz.rb` |
| Release binaries | GitHub Releases (`v*` tags) |

## v0.1.0 status

| Step | Done |
|------|------|
| GitHub release `v0.1.0` + all platform assets | ✅ |
| Homebrew tap created and validated | ✅ |
| npm `@sagwaco/plz@0.1.0` published | ⬜ |
| Main-repo docs committed | ⬜ |
