#!/usr/bin/env node

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const bin = path.join(__dirname, "..", "vendor", "plz");

if (!fs.existsSync(bin)) {
  console.error(
    "pretty-plz: the plz binary isn't installed — the postinstall step was skipped " +
      "(e.g. `npm install --ignore-scripts`) or failed.\n" +
      "Reinstall the package, or install manually:\n" +
      "  curl -fsSL https://raw.githubusercontent.com/sagwaco/pretty-plz/main/install.sh | sh"
  );
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
