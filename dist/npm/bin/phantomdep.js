#!/usr/bin/env node
// Wrapper that exec's the platform binary downloaded by ./install.js.
// Forwards stdio + signals + exit code transparently.

const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");

const ext = process.platform === "win32" ? ".exe" : "";
const bin = path.join(__dirname, `phantomdep${ext}`);

if (!fs.existsSync(bin)) {
  console.error(
    `phantomdep: binary not found at ${bin}. Try reinstalling with \`npm install -g phantomdep --force\`.`
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 1);
  }
});
