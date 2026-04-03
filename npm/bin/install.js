#!/usr/bin/env node
"use strict";

const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");
const { createWriteStream } = require("fs");

const REPO = "hong4rc/git-vanity";
const BIN_NAME = "git-vanity";

const PLATFORM_MAP = {
  darwin: "apple-darwin",
  linux: "unknown-linux-gnu",
  win32: "pc-windows-msvc",
};

const ARCH_MAP = {
  x64: "x86_64",
  arm64: "aarch64",
};

function getPlatformTarget() {
  const platform = PLATFORM_MAP[process.platform];
  const arch = ARCH_MAP[process.arch];
  if (!platform || !arch) {
    throw new Error(
      `Unsupported platform: ${process.platform}-${process.arch}`
    );
  }
  return `${arch}-${platform}`;
}

function getBinPath() {
  return path.join(__dirname, process.platform === "win32" ? `${BIN_NAME}.exe` : BIN_NAME);
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (url) => {
      https.get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return follow(res.headers.location);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`Download failed: ${res.statusCode}`));
        }
        const file = createWriteStream(dest);
        res.pipe(file);
        file.on("finish", () => { file.close(); resolve(); });
      }).on("error", reject);
    };
    follow(url);
  });
}

async function install() {
  const target = getPlatformTarget();
  const binPath = getBinPath();

  if (fs.existsSync(binPath)) {
    console.log(`${BIN_NAME} already installed.`);
    return;
  }

  const ext = process.platform === "win32" ? ".exe" : "";
  const assetName = `${BIN_NAME}-${target}${ext}`;
  const url = `https://github.com/${REPO}/releases/latest/download/${assetName}`;

  console.log(`Downloading ${BIN_NAME} for ${target}...`);
  try {
    await download(url, binPath);
    if (process.platform !== "win32") {
      fs.chmodSync(binPath, 0o755);
    }
    console.log(`Installed ${BIN_NAME} to ${binPath}`);
  } catch (err) {
    console.error(`Failed to download: ${err.message}`);
    console.error(`You can build from source: cargo install git-vanity`);
    process.exit(0); // Don't fail npm install
  }
}

install();
