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
import { dataDir } from "./data-dir.mjs";

const PORT = Number(process.env.MC_PTY_PORT || process.env.PORT || 4322);
const HOST = process.env.MC_BIND_HOST || "127.0.0.1";
const HTML_PORT = Number(process.env.MC_HTML_PORT || 4321);
// Browsers can open websockets to localhost from remote pages (CSWSH).
// Only allow Origins that match the local HTML UI unless MC_ALLOW_NO_ORIGIN=1.
const ALLOWED_ORIGINS = new Set(
  (process.env.MC_ALLOWED_ORIGINS || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .concat([
      `http://127.0.0.1:${HTML_PORT}`,
      `http://localhost:${HTML_PORT}`,
      "http://127.0.0.1:4321",
      "http://localhost:4321",
    ]),
);
const SHELL = process.env.SHELL || (process.platform === "win32" ? "powershell.exe" : "/bin/zsh");
const HOME = process.env.HOME || process.cwd();
const DATA_DIR = dataDir(HOME);

function defaultStartCwd() {
  if (process.env.MC_WORKSPACE_ROOT) return process.env.MC_WORKSPACE_ROOT;
  if (process.env.GROK_TERMINAL_START_CWD) return process.env.GROK_TERMINAL_START_CWD;
  const dev = join(HOME, "dev");
  if (existsSync(dev) && statSync(dev).isDirectory()) return dev;
  return HOME;
}

const DEFAULT_START_CWD = defaultStartCwd();
const STATE_PATH = join(DATA_DIR, "terminal-state.json");
// Prefer t0 (product CLI); keep mc / older names as fallbacks.
const LAUNCHER_CANDIDATES = [
  process.env.MC_LAUNCHER,
  process.env.GROK_TERMINAL_LAUNCHER,
  join(DATA_DIR, "bin/t0"),
  join(DATA_DIR, "bin/mc"),
  join(HOME, ".t-0", "bin/t0"),
  join(HOME, ".t-0", "bin/mc"),
  join(HOME, ".mission-control", "bin/t0"),
  join(HOME, ".mission-control", "bin/mc"),
  join(HOME, ".grok-mission-control", "bin/mc"),
  join(HOME, ".grok-mission-control", "bin/grok-terminal-launcher"),
  join(process.cwd(), "terminal/launcher-ratatui/target/release/t0"),
  join(process.cwd(), "terminal/launcher-ratatui/target/release/mc"),
  join(process.cwd(), "terminal/launcher-ratatui/target/release/grok-terminal-launcher"),
].filter(Boolean);
const LAUNCHER_PATH = LAUNCHER_CANDIDATES.find((p) => existsSync(p)) || LAUNCHER_CANDIDATES.at(-1);
const LAUNCHER_ENABLED =
  process.env.MC_USE_LAUNCHER !== "0" &&
  process.env.GROK_TERMINAL_USE_LAUNCHER !== "0" &&
  existsSync(LAUNCHER_PATH);
const TERMINAL_NAME = "xterm-256color";
const SESSION_RETAIN_MS = Number(
  process.env.MC_SESSION_RETAIN_MS ||
    process.env.GROK_TERMINAL_SESSION_RETAIN_MS ||
    6 * 60 * 60 * 1000,
);
const SESSION_HISTORY_LIMIT = Number(
  process.env.MC_SESSION_HISTORY_LIMIT ||
    process.env.GROK_TERMINAL_SESSION_HISTORY_LIMIT ||
    2_000_000,
);

const sessions = new Map();

if (HOST !== "127.0.0.1" && HOST !== "localhost" && HOST !== "::1") {
  if (process.env.MC_ALLOW_REMOTE_BIND !== "1") {
    console.error(
      `[PTY Broker] Refusing bind host ${HOST}. Use 127.0.0.1 or set MC_ALLOW_REMOTE_BIND=1 (dangerous).`,
    );
    process.exit(78);
  }
  console.warn(`[PTY Broker] WARNING: binding PTY to ${HOST} — full shell may be network-reachable.`);
}

const wss = new WebSocketServer({ host: HOST, port: PORT });

console.log(`[PTY Broker] Running under Node ${process.version} (this is required for stable PTY)`);
console.log(`[PTY Broker] Data dir: ${DATA_DIR}`);
console.log(`[PTY Broker] Real shells are available on ws://${HOST}:${PORT}`);
console.log(
  `[PTY Broker] Open http://${HOST === "0.0.0.0" ? "127.0.0.1" : HOST}:${HTML_PORT} in your browser to use the terminal.`,
);
if (LAUNCHER_ENABLED) {
  console.log(`[PTY Broker] Ratatui launcher enabled: ${LAUNCHER_PATH}`);
} else {
  console.log(`[PTY Broker] Ratatui launcher unavailable; falling back to ${SHELL}`);
}

function originAllowed(origin) {
  if (!origin) {
    // Non-browser clients omit Origin. Default deny; opt in for local tools.
    return process.env.MC_ALLOW_NO_ORIGIN === "1";
  }
  return ALLOWED_ORIGINS.has(origin);
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

function startCwdTracking(session, ptyProcess) {
  let lastSaved = null;
  let polling = false;
  let lastPolledDataAt = 0;

  const poll = () => {
    // P5: no clients, or no PTY output since last poll → skip lsof (battery/CPU).
    if (session.clients.size === 0) return;
    const lastData = session.lastDataAt || 0;
    if (lastData && lastData === lastPolledDataAt) return;
    if (polling) return;
    polling = true;
    lastPolledDataAt = lastData;

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

// P4: chunk list instead of string += (avoids quadratic copy near the 2MB cap).
// Chunks are whole PTY writes — safer cut points than mid-ANSI byte offsets
// (replaces the old trimHistoryForReplay heuristic).
function appendHistory(session, data) {
  if (typeof data !== "string" || data.length === 0) return;
  if (!session.chunks) {
    session.chunks = [];
    session.bytes = 0;
  }
  session.chunks.push(data);
  session.bytes += data.length;
  while (session.bytes > SESSION_HISTORY_LIMIT && session.chunks.length > 1) {
    session.bytes -= session.chunks.shift().length;
  }
}

function replayHistory(session, ws) {
  if (!session.chunks || session.chunks.length === 0) return;
  for (const chunk of session.chunks) {
    sendToClient(ws, chunk);
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
      chunks: [],
      bytes: 0,
      lastDataAt: 0,
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

function normalizeUiTheme(value) {
  if (typeof value !== "string") return null;
  const v = value.trim().toLowerCase();
  if (v === "light" || v === "dark") return v;
  return null;
}

function spawnPtyForSession(session, cols, rows, requestedCwd, uiTheme) {
  if (session.ptyProcess) return;

  session.pendingCols = Math.max(1, cols);
  session.pendingRows = Math.max(1, rows);

  const normalizedCwd = normalizeCwd(requestedCwd);
  const useLauncher = LAUNCHER_ENABLED && !normalizedCwd;
  const command = useLauncher ? LAUNCHER_PATH : SHELL;
  const args = useLauncher ? [] : [];
  const cwd = normalizedCwd && isDirectory(normalizedCwd) ? normalizedCwd : readLastCwd();
  const theme = normalizeUiTheme(uiTheme) || normalizeUiTheme(session.uiTheme);
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
    // Tell the Ratatui launcher which browser theme is active (auto mode).
    // COLORFGBG: light-fg;dark-bg (15;0) or dark-fg;light-bg (0;15).
    if (theme === "light") {
      env.MC_UI_THEME = "light";
      env.COLORFGBG = "0;15";
    } else if (theme === "dark") {
      env.MC_UI_THEME = "dark";
      env.COLORFGBG = "15;0";
    }
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
    session.stopCwdTracking = startCwdTracking(session, session.ptyProcess);

    session.ptyProcess.onData((data) => {
      session.lastDataAt = Date.now();
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

wss.on("connection", (ws, req) => {
  const origin = req.headers.origin;
  if (!originAllowed(origin)) {
    console.warn(`[PTY Broker] Rejected connection origin=${origin || "(none)"}`);
    try {
      ws.close(1008, "origin not allowed");
    } catch {
      // ignore
    }
    return;
  }

  console.log(`[PTY Broker] New client connected origin=${origin || "(none)"}`);

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
          const theme = normalizeUiTheme(msg.theme);
          if (theme) session.uiTheme = theme;
          startRequested = true;
          replayHistory(session, ws);
          spawnPtyForSession(session, cols, rows, cwd, theme);
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
