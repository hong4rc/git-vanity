#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");
const path = require("path");

const BIN_NAME = process.platform === "win32" ? "git-vanity.exe" : "git-vanity";
const binPath = path.join(__dirname, BIN_NAME);

try {
  execFileSync(binPath, process.argv.slice(2), { stdio: "inherit" });
} catch (err) {
  if (err.status !== undefined) {
    process.exit(err.status);
  }
  console.error(`Failed to run git-vanity: ${err.message}`);
  console.error("Try: cargo install git-vanity");
  process.exit(1);
}
