#!/usr/bin/env node
/**
 * Thin bootstrap for T-0.
 * - If already installed and the local stack responds → open the product URL.
 * - Else fetch https://t-0.dev/install (GitHub fallback) and run it with bash.
 */

import { spawn, spawnSync } from "node:child_process";
import { accessSync, constants as fsConstants } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import process from "node:process";

const INSTALL_URL = "https://t-0.dev/install";
const INSTALL_FALLBACK_URL =
  "https://raw.githubusercontent.com/aatosolavi/t-0/main/install.sh";
const PRODUCT_LOCAL = "http://127.0.0.1:4321";
const PRODUCT_PORTLESS = "https://t0.localhost";

function fail(message, code = 1) {
  console.error(`t-0: ${message}`);
  process.exit(code);
}

function launcherPath() {
  return join(homedir(), ".t-0", "bin", "t0");
}

function isExecutable(path) {
  try {
    accessSync(path, fsConstants.X_OK);
    return true;
  } catch {
    return false;
  }
}

async function urlOk(url) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 2500);
  try {
    const res = await fetch(url, {
      method: "GET",
      redirect: "follow",
      signal: controller.signal,
    });
    return res.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

function curlOk(url, insecure = false) {
  const args = ["-sf", "--max-time", "3", "-o", "/dev/null"];
  if (insecure) args.unshift("-k");
  args.push(url);
  const result = spawnSync("curl", args, { stdio: "ignore" });
  return result.status === 0;
}

async function productUrlIfUp() {
  if (await urlOk(PRODUCT_LOCAL) || curlOk(PRODUCT_LOCAL)) {
    return PRODUCT_LOCAL;
  }
  // portless uses a local CA; prefer curl -k for reachability
  if (curlOk(PRODUCT_PORTLESS, true)) {
    return PRODUCT_PORTLESS;
  }
  return null;
}

function openUrl(url) {
  spawnSync("open", [url], { stdio: "ignore" });
}

async function fetchInstallScript() {
  for (const url of [INSTALL_URL, INSTALL_FALLBACK_URL]) {
    try {
      const res = await fetch(url, { redirect: "follow" });
      if (!res.ok) continue;
      const body = await res.text();
      if (body.includes("T-0") || body.includes("terminal:install")) {
        return { url, body };
      }
    } catch {
      // try next
    }
  }
  fail(
    `could not download install script from ${INSTALL_URL} (or GitHub fallback)`,
  );
}

function runInstall(scriptBody) {
  return new Promise((resolve, reject) => {
    const child = spawn("bash", ["-s"], {
      env: process.env,
      stdio: ["pipe", "inherit", "inherit"],
    });
    child.stdin.write(scriptBody);
    child.stdin.end();
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`install interrupted (${signal})`));
        return;
      }
      resolve(code ?? 1);
    });
  });
}

async function main() {
  if (process.platform !== "darwin") {
    fail("macOS only for now (Linux/Windows later). See https://t-0.dev");
  }

  if (isExecutable(launcherPath())) {
    const up = await productUrlIfUp();
    if (up) {
      console.log(`t-0: already installed — opening ${up}`);
      openUrl(up);
      return;
    }
  }

  console.log("t-0: installing via https://t-0.dev/install …");
  const { url, body } = await fetchInstallScript();
  if (url !== INSTALL_URL) {
    console.log(`t-0: note — used fallback ${url}`);
  }

  const code = await runInstall(body);
  if (code !== 0) {
    process.exit(code);
  }
}

main().catch((err) => {
  fail(err?.message || String(err));
});
