#!/usr/bin/env node
/**
 * PTY Broker — Runs under real Node.js (not Bun)
 *
 * This is the piece that actually spawns and talks to real shells via @lydell/node-pty.
 * It must run under Node because the native PTY addon has fragile fd/ioctl behavior under Bun on macOS.
 *
 * The Bun server (terminal/server.ts) only serves the HTML page.
 * The browser connects to this process for the actual terminal I/O.
 */

import pty from "@lydell/node-pty";
import { execFile } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readlinkSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";
import { WebSocketServer } from "ws";

const PORT = Number(process.env.PORT || 4322);
const SHELL = process.env.SHELL || (process.platform === "win32" ? "powershell.exe" : "/bin/zsh");
const HOME = process.env.HOME || process.cwd();
const DEFAULT_START_CWD =
  process.env.GROK_TERMINAL_START_CWD ||
  join(HOME, "Documents/10-19 Work/Personal Projects");
const STATE_PATH = join(HOME, ".grok-mission-control", "terminal-state.json");
const GHOSTTY_TERMINFO = "/Applications/Ghostty.app/Contents/Resources/terminfo";
const USE_GHOSTTY_TERM =
  process.platform === "darwin" &&
  (process.env.TERM_PROGRAM === "ghostty" || existsSync(GHOSTTY_TERMINFO));
const TERMINAL_NAME = USE_GHOSTTY_TERM ? "xterm-ghostty" : "xterm-256color";

const wss = new WebSocketServer({ port: PORT });

console.log(`[PTY Broker] Running under Node ${process.version} (this is required for stable PTY)`);
console.log(`[PTY Broker] Real shells are available on ws://localhost:${PORT}`);
console.log(`[PTY Broker] Open http://localhost:4321 in your browser to use the terminal.`);

function isDirectory(path) {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function readLastCwd() {
  try {
    const state = JSON.parse(readFileSync(STATE_PATH, "utf8"));
    if (typeof state.lastCwd === "string" && isDirectory(state.lastCwd)) {
      return state.lastCwd;
    }
  } catch {
    // Missing or invalid state is fine.
  }

  if (isDirectory(DEFAULT_START_CWD)) return DEFAULT_START_CWD;
  return HOME;
}

function writeLastCwd(cwd) {
  if (!cwd || !isDirectory(cwd)) return;

  try {
    mkdirSync(dirname(STATE_PATH), { recursive: true });
    writeFileSync(
      STATE_PATH,
      JSON.stringify({ lastCwd: cwd, updatedAt: new Date().toISOString() }, null, 2),
    );
  } catch (error) {
    console.error("[PTY Broker] failed to save last cwd:", error.message);
  }
}

function readProcessCwd(pid, callback) {
  if (process.platform === "linux") {
    try {
      callback(readlinkSync(`/proc/${pid}/cwd`));
    } catch {
      callback(null);
    }
    return;
  }

  if (process.platform !== "darwin") {
    callback(null);
    return;
  }

  execFile("lsof", ["-a", "-p", String(pid), "-d", "cwd", "-Fn"], { encoding: "utf8" }, (error, stdout) => {
    if (error) {
      callback(null);
      return;
    }

    const cwdLine = stdout
      .split("\n")
      .find((line) => line.startsWith("n/"));
    callback(cwdLine ? cwdLine.slice(1) : null);
  });
}

function startCwdTracking(ptyProcess) {
  let lastSaved = null;
  let polling = false;

  const poll = () => {
    if (polling) return;
    polling = true;

    readProcessCwd(ptyProcess.pid, (cwd) => {
      polling = false;
      if (cwd && cwd !== lastSaved) {
        lastSaved = cwd;
        writeLastCwd(cwd);
      }
    });
  };

  poll();
  const interval = setInterval(poll, 2000);
  interval.unref?.();
  return () => clearInterval(interval);
}

wss.on("connection", (ws) => {
  console.log("[PTY Broker] New client connected");

  // We will spawn the PTY once we know a reasonable initial size.
  // The client should send an initial resize (or we default to 80x24).
  let ptyProcess = null;
  let stopCwdTracking = null;
  const sendToClient = (data) => {
    if (ws.readyState === 1) {
      ws.send(data);
    }
  };

  const cleanup = () => {
    stopCwdTracking?.();
    stopCwdTracking = null;
    if (ptyProcess) {
      try {
        ptyProcess.kill();
      } catch {}
      ptyProcess = null;
    }
  };

  ws.on("message", (raw) => {
    const text = raw.toString();

    // Resize control messages from the browser
    if (text.startsWith("{")) {
      try {
        const msg = JSON.parse(text);
        if (msg.type === "resize" && typeof msg.cols === "number" && typeof msg.rows === "number") {
          if (ptyProcess) {
            try {
              ptyProcess.resize(Math.max(1, msg.cols), Math.max(1, msg.rows));
            } catch (e) {
              console.error("[PTY Broker] resize error:", e.message);
            }
          } else {
            // Spawn now that we have a size from the client
            spawnPty(msg.cols, msg.rows);
          }
          return;
        }
      } catch {
        // not json, fall through
      }
    }

    // Normal user input → write to the real shell
    if (ptyProcess) {
      try {
        ptyProcess.write(text);
      } catch (e) {
        console.error("[PTY Broker] write error:", e.message);
      }
    } else {
      // Client sent input before telling us the size → spawn with defaults
      spawnPty(80, 24);
      // Give it a moment then write
      setTimeout(() => {
        if (ptyProcess) ptyProcess.write(text);
      }, 80);
    }
  });

  ws.on("close", () => {
    console.log("[PTY Broker] Client disconnected");
    cleanup();
  });

  ws.on("error", () => {
    cleanup();
  });

  function spawnPty(cols, rows) {
    if (ws.readyState !== 1) return;
    if (ptyProcess) return; // already spawned

    const cwd = readLastCwd();
    console.log(`[PTY Broker] Spawning ${SHELL} at ${cols}x${rows} in ${cwd}`);

    try {
      const env = {
        ...process.env,
        TERM: TERMINAL_NAME,
        COLORTERM: "truecolor",
        FORCE_COLOR: "3",
        CLICOLOR: "1",
        CLICOLOR_FORCE: "1",
        TERM_PROGRAM: USE_GHOSTTY_TERM ? "ghostty" : (process.env.TERM_PROGRAM || "xterm.js"),
        TERM_PROGRAM_VERSION: USE_GHOSTTY_TERM
          ? (process.env.TERM_PROGRAM_VERSION || "1.3.1")
          : (process.env.TERM_PROGRAM_VERSION || "5"),
      };
      if (USE_GHOSTTY_TERM) {
        env.TERMINFO = process.env.TERMINFO || GHOSTTY_TERMINFO;
      }
      delete env.NO_COLOR;

      ptyProcess = pty.spawn(SHELL, [], {
        name: TERMINAL_NAME,
        cols: Math.max(1, cols),
        rows: Math.max(1, rows),
        cwd,
        env,
        useConpty: false,
      });

      stopCwdTracking = startCwdTracking(ptyProcess);

      ptyProcess.onData((data) => {
        sendToClient(data);
      });

      ptyProcess.onExit(({ exitCode, signal }) => {
        sendToClient(`\r\n[shell exited${signal ? ` (signal ${signal})` : ""} code ${exitCode}]\r\n`);
        ws.close();
      });

      // Send nothing extra — keep it feeling like a plain native terminal
      // (user can run `clear` if they want a clean slate)

    } catch (err) {
      console.error("[PTY Broker] Failed to spawn PTY:", err);
      sendToClient(`\r\n[failed to spawn shell: ${err.message}]\r\n`);
      ws.close();
    }
  }

  // If the client never sends a resize, still give them a shell after a short delay
  setTimeout(() => {
    if (!ptyProcess && ws.readyState === 1) {
      spawnPty(80, 24);
    }
  }, 400);
});

process.on("SIGINT", () => {
  console.log("\n[PTY Broker] Shutting down...");
  wss.close();
  process.exit(0);
});
