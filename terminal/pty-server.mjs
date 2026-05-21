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
const INSTALLED_LAUNCHER_PATH = join(HOME, ".grok-mission-control", "bin/mc");
const LEGACY_INSTALLED_LAUNCHER_PATH = join(HOME, ".grok-mission-control", "bin/grok-terminal-launcher");
const BUILD_LAUNCHER_PATH = join(
  process.cwd(),
  "terminal/launcher-ratatui/target/release/mc",
);
const LEGACY_BUILD_LAUNCHER_PATH = join(
  process.cwd(),
  "terminal/launcher-ratatui/target/release/grok-terminal-launcher",
);
const LAUNCHER_PATH =
  process.env.GROK_TERMINAL_LAUNCHER ||
  (existsSync(INSTALLED_LAUNCHER_PATH)
    ? INSTALLED_LAUNCHER_PATH
    : existsSync(LEGACY_INSTALLED_LAUNCHER_PATH)
      ? LEGACY_INSTALLED_LAUNCHER_PATH
      : existsSync(BUILD_LAUNCHER_PATH)
        ? BUILD_LAUNCHER_PATH
        : LEGACY_BUILD_LAUNCHER_PATH);
const LAUNCHER_ENABLED = process.env.GROK_TERMINAL_USE_LAUNCHER !== "0" && existsSync(LAUNCHER_PATH);
const TERMINAL_NAME = "xterm-256color";
const SESSION_RETAIN_MS = Number(process.env.GROK_TERMINAL_SESSION_RETAIN_MS || 6 * 60 * 60 * 1000);
const SESSION_HISTORY_LIMIT = Number(process.env.GROK_TERMINAL_SESSION_HISTORY_LIMIT || 2_000_000);

const sessions = new Map();

const wss = new WebSocketServer({ port: PORT });

console.log(`[PTY Broker] Running under Node ${process.version} (this is required for stable PTY)`);
console.log(`[PTY Broker] Real shells are available on ws://localhost:${PORT}`);
console.log(`[PTY Broker] Open http://localhost:4321 in your browser to use the terminal.`);
if (LAUNCHER_ENABLED) {
  console.log(`[PTY Broker] Ratatui launcher enabled: ${LAUNCHER_PATH}`);
} else {
  console.log(`[PTY Broker] Ratatui launcher unavailable; falling back to ${SHELL}`);
}

function isDirectory(path) {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function normalizeCwd(path) {
  if (!path) return null;
  if (path === "~") return HOME;
  if (path.startsWith("~/")) return join(HOME, path.slice(2));
  return path;
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

function normalizeSessionId(value) {
  const raw = typeof value === "string" && value.trim() ? value.trim() : "default";
  return raw.replace(/[^a-zA-Z0-9._:-]+/g, "-").slice(0, 120) || "default";
}

function appendHistory(session, data) {
  session.history += data;
  if (session.history.length > SESSION_HISTORY_LIMIT) {
    session.history = session.history.slice(-SESSION_HISTORY_LIMIT);
  }
}

function sendToClient(ws, data) {
  if (ws.readyState === 1) {
    ws.send(data);
  }
}

function broadcast(session, data) {
  for (const client of session.clients) {
    sendToClient(client, data);
  }
}

function scheduleSessionCleanup(session) {
  if (session.killTimer || session.clients.size > 0 || !session.ptyProcess) return;
  session.killTimer = setTimeout(() => {
    if (session.clients.size === 0) {
      console.log(`[PTY Broker] Closing idle session ${session.id}`);
      destroySession(session);
    }
  }, SESSION_RETAIN_MS);
  session.killTimer.unref?.();
}

function cancelSessionCleanup(session) {
  if (!session.killTimer) return;
  clearTimeout(session.killTimer);
  session.killTimer = null;
}

function destroySession(session) {
  cancelSessionCleanup(session);
  session.stopCwdTracking?.();
  session.stopCwdTracking = null;
  if (session.ptyProcess) {
    try {
      session.ptyProcess.kill();
    } catch {}
    session.ptyProcess = null;
  }
  sessions.delete(session.id);
}

function getSession(id) {
  const sessionId = normalizeSessionId(id);
  let session = sessions.get(sessionId);
  if (!session) {
    session = {
      id: sessionId,
      clients: new Set(),
      history: "",
      ptyProcess: null,
      stopCwdTracking: null,
      killTimer: null,
      pendingCols: 80,
      pendingRows: 24,
      exited: false,
    };
    sessions.set(sessionId, session);
  }
  return session;
}

function spawnPtyForSession(session, cols, rows, requestedCwd) {
  if (session.ptyProcess) return;

  session.pendingCols = Math.max(1, cols);
  session.pendingRows = Math.max(1, rows);

  const normalizedCwd = normalizeCwd(requestedCwd);
  const useLauncher = LAUNCHER_ENABLED && !normalizedCwd;
  const command = useLauncher ? LAUNCHER_PATH : SHELL;
  const args = useLauncher ? [] : [];
  const cwd = normalizedCwd && isDirectory(normalizedCwd) ? normalizedCwd : readLastCwd();
  console.log(`[PTY Broker] Spawning ${command} for session ${session.id} at ${cols}x${rows} in ${cwd}`);

  try {
    const env = {
      ...process.env,
      TERM: TERMINAL_NAME,
      COLORTERM: "truecolor",
      FORCE_COLOR: "3",
      CLICOLOR: "1",
      CLICOLOR_FORCE: "1",
      TERM_PROGRAM: "xterm.js",
      TERM_PROGRAM_VERSION: process.env.TERM_PROGRAM_VERSION || "5",
    };
    delete env.NO_COLOR;
    delete env.TERMINFO;

    session.ptyProcess = pty.spawn(command, args, {
      name: TERMINAL_NAME,
      cols: session.pendingCols,
      rows: session.pendingRows,
      cwd,
      env,
      useConpty: false,
    });
    session.exited = false;
    session.stopCwdTracking = startCwdTracking(session.ptyProcess);

    session.ptyProcess.onData((data) => {
      appendHistory(session, data);
      broadcast(session, data);
    });

    session.ptyProcess.onExit(({ exitCode, signal }) => {
      const message = `\r\n[shell exited${signal ? ` (signal ${signal})` : ""} code ${exitCode}]\r\n`;
      appendHistory(session, message);
      broadcast(session, message);
      session.exited = true;
      destroySession(session);
    });
  } catch (err) {
    const message = `\r\n[failed to spawn shell: ${err.message}]\r\n`;
    console.error("[PTY Broker] Failed to spawn PTY:", err);
    appendHistory(session, message);
    broadcast(session, message);
    destroySession(session);
  }
}

wss.on("connection", (ws) => {
  console.log("[PTY Broker] New client connected");

  let session = null;
  let startRequested = false;

  ws.on("message", (raw) => {
    const text = raw.toString();

    // Resize control messages from the browser
    if (text.startsWith("{")) {
      try {
        const msg = JSON.parse(text);
        if (msg.type === "start") {
          session = getSession(msg.sessionId);
          cancelSessionCleanup(session);
          session.clients.add(ws);

          const cols = typeof msg.cols === "number" ? msg.cols : session.pendingCols;
          const rows = typeof msg.rows === "number" ? msg.rows : session.pendingRows;
          const cwd = typeof msg.cwd === "string" ? msg.cwd : undefined;
          startRequested = true;
          if (session.history) {
            sendToClient(ws, session.history);
          }
          spawnPtyForSession(session, cols, rows, cwd);
          return;
        }

        if (msg.type === "resize" && typeof msg.cols === "number" && typeof msg.rows === "number") {
          if (!session) return;
          session.pendingCols = Math.max(1, msg.cols);
          session.pendingRows = Math.max(1, msg.rows);
          if (session.ptyProcess) {
            try {
              session.ptyProcess.resize(session.pendingCols, session.pendingRows);
            } catch (e) {
              console.error("[PTY Broker] resize error:", e.message);
            }
          }
          return;
        }
      } catch {
        // not json, fall through
      }
    }

    // Normal user input → write to the real shell
    if (session?.ptyProcess) {
      try {
        session.ptyProcess.write(text);
      } catch (e) {
        console.error("[PTY Broker] write error:", e.message);
      }
    } else {
      // Client sent input before telling us the size → spawn with defaults
      startRequested = true;
      session = session || getSession("default");
      cancelSessionCleanup(session);
      session.clients.add(ws);
      spawnPtyForSession(session, session.pendingCols, session.pendingRows);
      // Give it a moment then write
      setTimeout(() => {
        if (session?.ptyProcess) session.ptyProcess.write(text);
      }, 80);
    }
  });

  ws.on("close", () => {
    console.log("[PTY Broker] Client disconnected");
    if (session) {
      session.clients.delete(ws);
      scheduleSessionCleanup(session);
    }
  });

  ws.on("error", () => {
    if (session) {
      session.clients.delete(ws);
      scheduleSessionCleanup(session);
    }
  });

  // If the client never sends a resize, still give them a shell after a short delay
  setTimeout(() => {
    if (session && !session.ptyProcess && startRequested && ws.readyState === 1) {
      spawnPtyForSession(session, session.pendingCols, session.pendingRows);
    }
  }, 400);
});

process.on("SIGINT", () => {
  console.log("\n[PTY Broker] Shutting down...");
  wss.close();
  process.exit(0);
});
