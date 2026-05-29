# Homebrew tap setup — agent prompt

> **Status: complete.** Tap is live at [github.com/sagwaco/homebrew-tap](https://github.com/sagwaco/homebrew-tap).  
> `brew install sagwaco/tap/plz` works. Keep this file for reference if the tap needs to be recreated.

Copy everything inside the block below and paste it into a Claude Opus agent (or
similar) with permission to create GitHub repos and push code.

---

```
You are setting up a personal Homebrew tap for the `plz` CLI so users can run:

  brew install sagwaco/tap/plz

## Context

- Main repo: https://github.com/sagwaco/pretty-plz
- Binary/crate name: `plz`
- Current version: check `Cargo.toml` in the main repo for the latest version
- Formula source of truth in the main repo: `packaging/homebrew/plz.rb`
- The formula builds from source via `cargo install` (depends on Rust)
- Install command users should get: `brew install sagwaco/tap/plz`
- Upgrade command: `brew upgrade plz`

## Your task

1. Create a new **public** GitHub repository named `homebrew-tap` under the
   `sagwaco` GitHub account/org.

   Homebrew expects the repo to be named exactly `homebrew-tap` so the tap alias
   is `sagwaco/tap`.

2. Add a minimal README to the tap repo explaining:
   - What the tap is
   - How to install: `brew install sagwaco/tap/plz`
   - How to upgrade: `brew upgrade plz`
   - Link back to https://github.com/sagwaco/pretty-plz

3. Copy the formula from the main repo's `packaging/homebrew/plz.rb` into
   `Formula/plz.rb` in the tap repo.

   Ensure the formula has:
   - Correct `homepage`, `license`, `desc`
   - `url` pointing at the pretty-plz git repo with matching `tag` and `revision`
     for the latest release tag (e.g. `v0.1.0` and its full commit SHA from
     `git rev-parse v0.1.0` on the main repo)
   - `depends_on "rust" => :build`
   - `system "cargo", "install", *std_cargo_args` in `install`
   - A `test` block that runs `plz --version`

4. Validate the formula locally before pushing:
   - `brew install --build-from-source ./Formula/plz.rb` (or `brew install sagwaco/tap/plz` after the tap exists)
   - Confirm `plz --version` matches the expected release
   - Run `brew test plz` if applicable

5. Push the tap repo to GitHub.

6. Confirm end-to-end install works:
   ```bash
   brew install sagwaco/tap/plz
   plz --version
   plz --help
   ```

7. Report back with:
   - Tap repo URL
   - Formula version/revision used
   - Output of `plz --version` after brew install
   - Any issues hit during validation

## Constraints

- Do NOT submit to homebrew-core — this is a personal tap only.
- Do NOT change the main pretty-plz repo unless you find an error in
  `packaging/homebrew/plz.rb` that blocks installation (if so, open a PR or
  describe the fix needed).
- Keep the tap repo minimal: `Formula/plz.rb`, `README.md`, and nothing else
  unless Homebrew requires it.
- The main repo MUST be public — the formula clones source over HTTPS without credentials.

## Reference formula shape

```ruby
class Plz < Formula
  desc "Natural-language to shell-command CLI"
  homepage "https://github.com/sagwaco/pretty-plz"
  url "https://github.com/sagwaco/pretty-plz.git",
      tag:      "vX.Y.Z",
      revision: "<full-commit-sha>"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/plz --version")
  end
end
```

## Per-release maintenance (document in tap README)

When pretty-plz cuts a new release, update `Formula/plz.rb` in this tap:
- Bump `tag` to the new `vX.Y.Z`
- Bump `revision` to the tagged commit's full SHA
- Commit and push — users run `brew update && brew upgrade plz`
```

---

After the agent finishes, verify `brew install sagwaco/tap/plz` from a clean machine and update [release.md](release.md).
