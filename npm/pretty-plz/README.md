# npm package

Global install:

```bash
npm install -g @sagwaco/plz
```

The package is scoped as **`@sagwaco/plz`** — the unscoped name `pretty-plz` is
already taken on npm by another project.

The `postinstall` script downloads the platform-matching release binary from
GitHub. Upgrade with `npm update -g @sagwaco/plz`, `plz update`, or
`plz configure update`.

Publish (maintainers):

```bash
cd npm/pretty-plz
npm publish --access public
```

Ensure `package.json` version matches the GitHub release tag before publishing.

See [docs/release.md](../../docs/release.md) for the full release checklist.
