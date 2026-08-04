#!/usr/bin/env node
/**
 * npm/CLI entry for T-0 (package `@aatosolavi/t0-terminal`; bin `t-0`).
 *
 * The Ratatui workspace pad is still the native binary `t0` under ~/.t-0/bin.
 * This command installs and runs the browser-terminal stack.
 *
 *   npm install -g @aatosolavi/t0-terminal
 *   t-0 install
 *   t-0 start
 *   t-0 doctor
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { packageRoot } from "../terminal/package-root.mjs";

const root = packageRoot();
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const args = process.argv.slice(2);
const cmd = args[0] && !args[0].startsWith("-") ? args[0] : "start";
const rest = cmd === args[0] ? args.slice(1) : args;

function usage() {
  console.log(`t-0 ${pkg.version} — browser terminal + agent launcher

Usage:
  t-0 install     Build t0 launcher, install LaunchAgent, set up portless
  t-0 start       Run HTML server + PTY broker in the foreground
  t-0 doctor      Check ports, LaunchAgent, and https://t0.localhost
  t-0 version     Print package version
  t-0 help        This message

After install, open https://t0.localhost (or http://127.0.0.1:4321).
Native pad binary: t0 (legacy: mc) under ~/.t-0/bin and ~/.local/bin.
`);
}

function findBin(name) {
  const fromPath = spawnSync("which", [name], { encoding: "utf8" });
  if (fromPath.status === 0 && fromPath.stdout.trim()) {
    return fromPath.stdout.trim();
  }
  return null;
}

function requireTools(names) {
  const missing = [];
  for (const name of names) {
    if (!findBin(name)) missing.push(name);
  }
  if (missing.length) {
    console.error(`error: missing required tools: ${missing.join(", ")}`);
    if (missing.includes("bun")) {
      console.error("  install Bun: https://bun.sh");
    }
    if (missing.includes("rustup")) {
      console.error("  install rustup: https://rustup.rs (needed to build the t0 launcher)");
    }
    process.exit(1);
  }
}

function run(command, commandArgs, opts = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
    ...opts,
  });
  if (result.status !== 0) {
    process.exit(result.status || 1);
  }
}

function runSoft(command, commandArgs) {
  return spawnSync(command, commandArgs, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });
}

function doctor() {
  const checks = [];

  const curl = (url, insecure = false) => {
    const a = ["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--connect-timeout", "2"];
    if (insecure) a.push("-k");
    a.push(url);
    const r = spawnSync("curl", a, { encoding: "utf8" });
    return r.status === 0 ? r.stdout.trim() : "000";
  };

  const code4321 = curl("http://127.0.0.1:4321/");
  checks.push([`http://127.0.0.1:4321`, code4321 === "200" ? "ok" : `fail (${code4321})`]);

  const codeT0 = curl("https://t0.localhost/", true);
  checks.push([`https://t0.localhost`, codeT0 === "200" ? "ok" : `fail (${codeT0})`]);

  const agent = spawnSync(
    "launchctl",
    ["print", `gui/${process.getuid?.() ?? "501"}/com.mission-control.terminal`],
    { encoding: "utf8" },
  );
  const agentOk =
    agent.status === 0 && /state = running/.test(agent.stdout || "");
  checks.push([
    "LaunchAgent com.mission-control.terminal",
    agentOk ? "running" : "not running",
  ]);

  const t0Bin =
    process.env.MC_LAUNCHER ||
    join(process.env.HOME || "", ".t-0/bin/t0");
  checks.push([`launcher ${t0Bin}`, existsSync(t0Bin) ? "present" : "missing"]);

  checks.push([`package root`, root]);
  checks.push([`version`, pkg.version]);
  checks.push([`bun`, findBin("bun") || "missing"]);
  checks.push([`node`, process.version]);
  checks.push([`rustup`, findBin("rustup") || "missing"]);

  for (const [k, v] of checks) {
    console.log(`${k.padEnd(44)} ${v}`);
  }

  if (code4321 === "200" && codeT0 !== "200") {
    console.log(`
hint: T-0 is up; only the portless HTTPS front is down.
  cd ${root} && bun run portless:repair
  # persist across reboot: bunx portless service install
`);
  }
}

function install() {
  requireTools(["node", "bun", "rustup"]);
  console.log(`→ Installing T-0 ${pkg.version} from ${root}`);

  console.log("→ Vendor xterm bundle");
  run("bun", ["run", "terminal:vendor:build"]);

  console.log("→ Build + install t0 launcher");
  run(process.execPath, [join(root, "terminal/build-launcher.mjs")]);

  console.log("→ LaunchAgent");
  run("bash", [join(root, "terminal/install-launch-agent.sh")]);

  const nodeMajor = Number(process.versions.node.split(".")[0]);
  if (nodeMajor >= 24) {
    console.log("→ portless (https://t0.localhost)");
    runSoft("bunx", ["portless", "alias", "t0", "4321"]);
    runSoft("bunx", ["portless", "proxy", "start"]);
    runSoft("bunx", ["portless", "trust"]);
    runSoft("bunx", ["portless", "service", "install"]);
  } else {
    console.log(
      `→ Skipping portless (needs Node 24+, found ${process.versions.node})`,
    );
  }

  console.log(`
✓ T-0 installed
  Open:  https://t0.localhost  or  http://127.0.0.1:4321
  CLI:   t0  (pad) ·  t-0 doctor  (stack health)
  Logs:  ~/.t-0/logs/
`);
}

function start() {
  requireTools(["node", "bun"]);
  const bun = findBin("bun");
  const env = {
    ...process.env,
    BUN_BIN: process.env.BUN_BIN || bun || "bun",
  };
  console.log(`[t-0] starting from ${root}`);
  const child = spawn(process.execPath, [join(root, "terminal/start.mjs")], {
    cwd: root,
    env,
    stdio: "inherit",
  });
  child.on("exit", (code, signal) => {
    process.exit(code ?? (signal ? 1 : 0));
  });
}

switch (cmd) {
  case "help":
  case "-h":
  case "--help":
    usage();
    break;
  case "version":
  case "-v":
  case "--version":
    console.log(pkg.version);
    break;
  case "doctor":
    doctor();
    break;
  case "install":
    install();
    break;
  case "start":
    start();
    break;
  default:
    console.error(`unknown command: ${cmd}\n`);
    usage();
    process.exit(1);
}
