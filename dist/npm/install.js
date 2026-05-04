#!/usr/bin/env node
// Postinstall hook: download the right phantomdep binary for the host OS/arch
// and place it in ./bin/. Run by npm during `npm install -g phantomdep`.

const fs = require("fs");
const os = require("os");
const path = require("path");
const https = require("https");
const zlib = require("zlib");
const { execSync } = require("child_process");

const VERSION = require("./package.json").version;
const REPO = "openintelligence-labs/phantomdep";

function targetForHost() {
  const platform = os.platform(); // 'darwin' | 'linux' | 'win32'
  const arch = os.arch(); // 'x64' | 'arm64'
  if (platform === "darwin" && arch === "arm64")
    return "aarch64-apple-darwin";
  if (platform === "darwin" && arch === "x64") return "x86_64-apple-darwin";
  if (platform === "linux" && arch === "arm64")
    return "aarch64-unknown-linux-gnu";
  if (platform === "linux" && arch === "x64")
    return "x86_64-unknown-linux-gnu";
  if (platform === "win32" && arch === "x64")
    return "x86_64-pc-windows-msvc";
  throw new Error(`unsupported os/arch: ${platform}/${arch}`);
}

function archiveExt(target) {
  return target.endsWith("windows-msvc") ? "zip" : "tar.gz";
}

function download(url, destPath) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destPath);
    const get = (u) =>
      https.get(u, (res) => {
        if (res.statusCode === 301 || res.statusCode === 302) {
          return get(res.headers.location);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} for ${u}`));
        }
        res.pipe(file);
        file.on("finish", () => file.close(resolve));
      });
    get(url).on("error", reject);
  });
}

async function main() {
  const target = targetForHost();
  const ext = archiveExt(target);
  const asset = `phantomdep-${target}.${ext}`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${asset}`;
  const tmpdir = fs.mkdtempSync(path.join(os.tmpdir(), "phantomdep-"));
  const archivePath = path.join(tmpdir, asset);

  console.log(`phantomdep: downloading ${url}`);
  await download(url, archivePath);

  const binDir = path.join(__dirname, "bin");
  fs.mkdirSync(binDir, { recursive: true });

  if (ext === "tar.gz") {
    execSync(`tar -C "${tmpdir}" -xzf "${archivePath}"`, { stdio: "inherit" });
    fs.copyFileSync(
      path.join(tmpdir, "phantomdep"),
      path.join(binDir, "phantomdep")
    );
    fs.chmodSync(path.join(binDir, "phantomdep"), 0o755);
  } else {
    // Windows zip — best to defer to native extraction.
    execSync(
      `powershell -Command "Expand-Archive -Force '${archivePath}' '${tmpdir}'"`,
      { stdio: "inherit" }
    );
    fs.copyFileSync(
      path.join(tmpdir, "phantomdep.exe"),
      path.join(binDir, "phantomdep.exe")
    );
  }

  console.log("phantomdep: installed binary to bin/");
}

main().catch((err) => {
  console.error(`phantomdep install failed: ${err.message}`);
  process.exit(1);
});
