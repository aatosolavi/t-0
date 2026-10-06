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
import {
  appendHistory,
  broadcast,
  classifyMessage,
  flushPendingInput,
  forgetSession,
  getSession,
  rebindSession,
  replayHistory,
} from "./pty-session.mjs";

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
  // -1 ⇒ never polled; allows one initial lsof even when lastDataAt is still 0.
  let lastPolledDataAt = -1;

  const poll = () => {
    // P5: no clients, or no new PTY output since last poll → skip lsof (battery/CPU).
    if (session.clients.size === 0) return;
    const lastData = session.lastDataAt || 0;
    if (lastData === lastPolledDataAt) return;
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
  forgetSession(sessions, session);
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
  // After the user quits t0, later spawns in this session are a plain shell.
  const useLauncher = LAUNCHER_ENABLED && !normalizedCwd && !session.launcherClosed;
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
    flushPendingInput(session);

    session.ptyProcess.onData((data) => {
      session.lastDataAt = Date.now();
      appendHistory(session, data);
      broadcast(session, data);
    });

    const exitedProcess = session.ptyProcess;
    session.ptyProcess.onExit(({ exitCode, signal }) => {
      // Quitting the launcher drops straight into a shell in the last folder
      // instead of a dead "[shell exited]" screen.
      if (useLauncher && session.clients.size > 0 && session.ptyProcess === exitedProcess) {
        const hint = "\r\n\x1b[2m[t0 closed — this is a plain shell. Run t0 to reopen the launcher.]\x1b[0m\r\n";
        appendHistory(session, hint);
        broadcast(session, hint);
        session.launcherClosed = true;
        session.stopCwdTracking?.();
        session.stopCwdTracking = null;
        session.ptyProcess = null;
        spawnPtyForSession(session, session.pendingCols, session.pendingRows);
        return;
      }
      const message = `\r\n[shell exited${signal ? ` (signal ${signal})` : ""} code ${exitCode}]\r\n`;
      appendHistory(session, message);
      broadcast(session, message);
      session.exited = true;
      destroySession(session);
    });
  } catch (err) {
    const message = `\r\n[failed to spawn shell: ${err.message}]\r\n`;
    console.error("[PTY Broker] Failed to spawn PTY:", err);
    session.pendingInput = [];
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

  function detach() {
    if (!session) return;
    session.clients.delete(ws);
    session.lagging.delete(ws);
    scheduleSessionCleanup(session);
  }

  function handleControl(msg) {
    if (msg.type === "start") {
      const next = getSession(sessions, msg.sessionId);
      if (session && session !== next) detach();
      session = next;
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

    // resize
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
  }

  ws.on("message", (raw, isBinary) => {
    const frame = classifyMessage(raw, isBinary);
    if (frame.kind === "control") {
      handleControl(frame.msg);
      return;
    }

    // No live PTY: input before `start` (spawn with defaults), or the shell
    // exited and a keypress respawns it.
    if (!session?.ptyProcess) {
      startRequested = true;
      session = session ? rebindSession(sessions, session, ws) : getSession(sessions, "default");
      cancelSessionCleanup(session);
      session.clients.add(ws);
    }

    // Normal user input → write to the real shell
    if (session.ptyProcess) {
      try {
        session.ptyProcess.write(frame.text);
      } catch (e) {
        console.error("[PTY Broker] write error:", e.message);
      }
      return;
    }

    // Queue so nothing races the spawn; flushed right after pty.spawn.
    session.pendingInput.push(frame.text);
    spawnPtyForSession(session, session.pendingCols, session.pendingRows);
  });

  ws.on("close", () => {
    console.log("[PTY Broker] Client disconnected");
    detach();
  });

  ws.on("error", detach);

  // If the client never sends a resize, still give them a shell after a short delay
  setTimeout(() => {
    if (session && !session.ptyProcess && startRequested && ws.readyState === 1) {
      session = rebindSession(sessions, session, ws);
      spawnPtyForSession(session, session.pendingCols, session.pendingRows);
    }
  }, 400);
});

process.on("SIGINT", () => {
  console.log("\n[PTY Broker] Shutting down...");
  wss.close();
  process.exit(0);
});
