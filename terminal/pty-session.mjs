/**
 * PTY session state for the broker (terminal/pty-server.mjs).
 *
 * Pure-ish helpers — no node-pty, no WebSocketServer — so the output
 * coalescing, history, backpressure and session registry can be unit tested
 * (terminal/pty-session.test.mjs).
 */

export const SESSION_HISTORY_LIMIT = Number(
  process.env.MC_SESSION_HISTORY_LIMIT ||
    process.env.GROK_TERMINAL_SESSION_HISTORY_LIMIT ||
    2_000_000,
);

// P4: chunk list instead of string += (avoids quadratic copy near the 2MB cap).
// Chunks are whole PTY writes — safer cut points than mid-ANSI byte offsets.
// Coalesce small writes (interactive echo is often 1 byte) so replay is not
// tens of thousands of websocket frames; only front-drops matter for ANSI safety.
const HISTORY_COALESCE_MAX = 4096;
const REPLAY_FRAME_MAX = 64 * 1024;

// node-pty often emits large draws as many ~1KB onData callbacks with gaps
// between groups. Debounce so one ratatui frame ≈ one websocket write.
// Large in-flight buffers keep a slightly longer window; tiny echo stays tight.
// OUT_MAX_WAIT_MS / OUT_MAX_BUF cap the debounce so a steady stream
// (`yes`, big `cat`, build logs) still flushes instead of starving.
export const OUT_COALESCE_MS_SMALL = 4;
export const OUT_COALESCE_MS_LARGE = 16;
const OUT_COALESCE_LARGE_AT = 4096;
export const OUT_MAX_WAIT_MS = 32;
export const OUT_MAX_BUF = 64 * 1024;

// A client further behind than this stops getting live output; once it
// drains it gets a terminal reset + history replay instead.
export const MAX_CLIENT_BUFFERED = 8 * 1024 * 1024;

const WS_OPEN = 1;
const RESET_TERMINAL = "\x1bc";

export function normalizeSessionId(value) {
  const raw = typeof value === "string" && value.trim() ? value.trim() : "default";
  return raw.replace(/[^a-zA-Z0-9._:-]+/g, "-").slice(0, 120) || "default";
}

export function createSession(id) {
  return {
    id,
    clients: new Set(),
    lagging: new Set(),
    chunks: [],
    bytes: 0,
    outBuf: "",
    outFirstAt: 0,
    outFlushTimer: null,
    pendingInput: [],
    lastDataAt: 0,
    ptyProcess: null,
    stopCwdTracking: null,
    killTimer: null,
    pendingCols: 80,
    pendingRows: 24,
    exited: false,
  };
}

export function getSession(sessions, id) {
  const sessionId = normalizeSessionId(id);
  let session = sessions.get(sessionId);
  if (!session) {
    session = createSession(sessionId);
    sessions.set(sessionId, session);
  }
  return session;
}

/** Remove from the registry only if this object still owns its id. */
export function forgetSession(sessions, session) {
  if (sessions.get(session.id) === session) {
    sessions.delete(session.id);
  }
}

/**
 * A client may still hold a session that exited (and was forgotten). Before
 * respawning, re-register it — or, if a newer session took the id meanwhile,
 * move the client there instead of spawning a second, unreachable PTY.
 */
export function rebindSession(sessions, session, ws) {
  const live = sessions.get(session.id);
  if (live === session) return session;
  if (live) {
    session.clients.delete(ws);
    session.lagging.delete(ws);
    live.clients.add(ws);
    return live;
  }
  sessions.set(session.id, session);
  return session;
}

export function appendHistory(session, data, limit = SESSION_HISTORY_LIMIT) {
  if (typeof data !== "string" || data.length === 0) return;
  const last = session.chunks[session.chunks.length - 1];
  if (last != null && last.length < HISTORY_COALESCE_MAX) {
    session.chunks[session.chunks.length - 1] = last + data;
  } else {
    session.chunks.push(data);
  }
  session.bytes += data.length;
  while (session.bytes > limit && session.chunks.length > 1) {
    session.bytes -= session.chunks.shift().length;
  }
}

export function replayHistory(session, ws) {
  if (session.chunks.length === 0 || ws.readyState !== WS_OPEN) return;
  // Join into fewer websocket frames so xterm paints one big write, not N×1KB.
  let buf = "";
  for (const chunk of session.chunks) {
    if (buf.length > 0 && buf.length + chunk.length > REPLAY_FRAME_MAX) {
      ws.send(buf);
      buf = "";
    }
    buf += chunk;
  }
  if (buf) ws.send(buf);
}

/**
 * Live output to one client, with backpressure. History is appended before
 * output is broadcast, so a resync replay already contains `data`.
 */
export function sendToClient(session, ws, data) {
  if (ws.readyState !== WS_OPEN) return;
  if (ws.bufferedAmount > MAX_CLIENT_BUFFERED) {
    session.lagging.add(ws);
    return;
  }
  if (session.lagging.delete(ws)) {
    ws.send(RESET_TERMINAL);
    replayHistory(session, ws);
    return;
  }
  ws.send(data);
}

export function flushOutput(session) {
  if (session.outFlushTimer != null) {
    clearTimeout(session.outFlushTimer);
    session.outFlushTimer = null;
  }
  const buf = session.outBuf;
  session.outBuf = "";
  session.outFirstAt = 0;
  if (!buf) return;
  for (const client of session.clients) {
    sendToClient(session, client, buf);
  }
}

export function broadcast(session, data) {
  if (typeof data !== "string" || data.length === 0) return;
  const now = Date.now();
  if (!session.outBuf) session.outFirstAt = now;
  session.outBuf += data;
  if (session.outFlushTimer != null) {
    clearTimeout(session.outFlushTimer);
    session.outFlushTimer = null;
  }
  const waited = now - session.outFirstAt;
  if (waited >= OUT_MAX_WAIT_MS || session.outBuf.length >= OUT_MAX_BUF) {
    flushOutput(session);
    return;
  }
  const delay =
    session.outBuf.length >= OUT_COALESCE_LARGE_AT
      ? OUT_COALESCE_MS_LARGE
      : OUT_COALESCE_MS_SMALL;
  session.outFlushTimer = setTimeout(() => {
    session.outFlushTimer = null;
    flushOutput(session);
  }, Math.min(delay, OUT_MAX_WAIT_MS - waited));
  session.outFlushTimer.unref?.();
}

/** Input that arrived before the PTY existed, written in order after spawn. */
export function flushPendingInput(session) {
  const pending = session.pendingInput;
  session.pendingInput = [];
  if (!session.ptyProcess) return;
  for (const text of pending) session.ptyProcess.write(text);
}

function frameText(raw) {
  if (typeof raw === "string") return raw;
  if (Array.isArray(raw)) return Buffer.concat(raw).toString("utf8");
  return Buffer.from(raw).toString("utf8");
}

function parseControl(text) {
  if (!text.startsWith("{")) return null;
  let msg;
  try {
    msg = JSON.parse(text);
  } catch {
    return null;
  }
  if (msg?.type === "start") return msg;
  if (msg?.type === "resize" && typeof msg.cols === "number" && typeof msg.rows === "number") {
    return msg;
  }
  return null;
}

/**
 * Browser sends keystrokes as binary frames (never parsed) and control
 * messages as text JSON. Text that isn't a control message is still treated
 * as input so a stale cached page keeps working until a hard reload.
 */
export function classifyMessage(raw, isBinary) {
  const text = frameText(raw);
  if (isBinary) return { kind: "input", text };
  const msg = parseControl(text);
  return msg ? { kind: "control", msg } : { kind: "input", text };
}
