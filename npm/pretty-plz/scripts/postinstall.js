#!/usr/bin/env node

const { execFileSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const https = require("node:https");
const os = require("node:os");
const path = require("node:path");

const MAX_REDIRECTS = 5;

const REPO = "sagwaco/pretty-plz";
const VERSION = require("../package.json").version;

function platformTarget() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === "darwin") {
    return arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  }
  if (platform === "linux") {
    return arch === "arm64" ? "aarch64-unknown-linux-musl" : "x86_64-unknown-linux-musl";
  }
  throw new Error(`Unsupported platform: ${platform} ${arch}`);
}

function download(url, redirects = 0) {
  return new Promise((resolve, reject) => {
    https
      .get(
        url,
        {
          headers: {
            "User-Agent": `pretty-plz-npm/${VERSION}`,
          },
        },
        (res) => {
          if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
            res.resume(); // drain so the socket can be reused/closed
            if (redirects >= MAX_REDIRECTS) {
              reject(new Error(`too many redirects fetching ${url}`));
              return;
            }
            download(res.headers.location, redirects + 1).then(resolve, reject);
            return;
          }
          if (res.statusCode !== 200) {
            res.resume();
            reject(new Error(`HTTP ${res.statusCode} fetching ${url}`));
            return;
          }
          const chunks = [];
          res.on("data", (c) => chunks.push(c));
          res.on("end", () => resolve(Buffer.concat(chunks)));
          res.on("error", reject);
        }
      )
      .on("error", reject);
  });
}

// Look up the expected sha256 for `asset` in a SHA256SUMS manifest
// (`<hex>  <filename>` per line, matching `sha256sum` / install.sh).
function expectedDigest(manifest, asset) {
  for (const raw of manifest.split("\n")) {
    const parts = raw.trim().split(/\s+/);
    // sha256sum binary mode prefixes the filename with `*`.
    const name = parts.length >= 2 ? parts[1].replace(/^\*/, "") : "";
    if (name === asset && /^[0-9a-fA-F]{64}$/.test(parts[0])) {
      return parts[0].toLowerCase();
    }
  }
  return null;
}

function verifyDigest(buffer, manifest, asset) {
  const expected = expectedDigest(manifest, asset);
  if (!expected) {
    throw new Error(`SHA256SUMS has no entry for ${asset}`);
  }
  const actual = crypto.createHash("sha256").update(buffer).digest("hex");
  if (actual !== expected) {
    throw new Error(
      `checksum mismatch for ${asset}\n  expected: ${expected}\n  actual:   ${actual}`
    );
  }
}

async function main() {
  const target = platformTarget();
  const tag = `v${VERSION}`;
  const asset = `plz-${tag}-${target}.tar.gz`;
  const base = `https://github.com/${REPO}/releases/download/${tag}`;
  const url = `${base}/${asset}`;
  const sumsUrl = `${base}/SHA256SUMS`;
  const binDir = path.join(__dirname, "..", "vendor");
  const binPath = path.join(binDir, "plz");
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "plz-npm-"));
  const archivePath = path.join(tmpDir, asset);

  // Verify the download against the release manifest before trusting it,
  // mirroring install.sh — HTTPS-to-GitHub is the baseline, the checksum is
  // defense-in-depth against a corrupt or tampered artifact.
  const [archive, sums] = await Promise.all([download(url), download(sumsUrl)]);
  verifyDigest(archive, sums.toString("utf8"), asset);

  fs.mkdirSync(binDir, { recursive: true });
  fs.writeFileSync(archivePath, archive);
  execFileSync("tar", ["-xzf", archivePath, "-C", tmpDir]);

  const staged =
    [path.join(tmpDir, "plz"), path.join(tmpDir, `plz-${tag}-${target}`, "plz")].find((p) =>
      fs.existsSync(p)
    ) ??
    execFileSync("find", [tmpDir, "-type", "f", "-name", "plz"], { encoding: "utf8" })
      .trim()
      .split("\n")
      .find(Boolean);

  if (!staged || !fs.existsSync(staged)) {
    throw new Error(`plz binary not found in ${asset}`);
  }

  fs.copyFileSync(staged, binPath);
  fs.chmodSync(binPath, 0o755);
  fs.rmSync(tmpDir, { recursive: true, force: true });
  console.log(`Installed plz ${tag} for ${target}`);
}

main().catch((err) => {
  console.error(`pretty-plz postinstall failed: ${err.message}`);
  console.error(
    `Install manually: curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | sh`
  );
  process.exit(1);
});
