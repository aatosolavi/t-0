import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, mock, test } from "node:test";
import {
  appendHistory,
  broadcast,
  classifyMessage,
  createSession,
  flushPendingInput,
  forgetSession,
  getSession,
  MAX_CLIENT_BUFFERED,
  OUT_MAX_WAIT_MS,
  rebindSession,
} from "./pty-session.mjs";

function fakeWs() {
  return { readyState: 1, bufferedAmount: 0, sent: [], send(data) { this.sent.push(data); } };
}

function fakePty() {
  return { written: [], write(text) { this.written.push(text); } };
}

describe("output coalescing", () => {
  beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "Date"] }));
  afterEach(() => mock.timers.reset());

  test("a steady stream still flushes within the max wait", () => {
    const session = createSession("s");
    const ws = fakeWs();
    session.clients.add(ws);
    // A chunk every 1 ms resets a plain debounce forever.
    for (let i = 0; i < OUT_MAX_WAIT_MS; i++) {
      broadcast(session, "y\n");
      mock.timers.tick(1);
    }
    assert.ok(ws.sent.length >= 1, "expected at least one flush during the stream");
  });

  test("a short burst is sent as one frame", () => {
    const session = createSession("s");
    const ws = fakeWs();
    session.clients.add(ws);
    broadcast(session, "a");
    broadcast(session, "b");
    mock.timers.tick(OUT_MAX_WAIT_MS);
    assert.deepEqual(ws.sent, ["ab"]);
  });

  test("a lagging client gets reset + replay instead of unbounded sends", () => {
    const session = createSession("s");
    const ws = fakeWs();
    session.clients.add(ws);
    ws.bufferedAmount = MAX_CLIENT_BUFFERED + 1;
    for (const chunk of ["one", "two"]) {
      appendHistory(session, chunk);
      broadcast(session, chunk);
      mock.timers.tick(OUT_MAX_WAIT_MS);
    }
    assert.deepEqual(ws.sent, []);
    assert.ok(session.lagging.has(ws));

    ws.bufferedAmount = 0;
    appendHistory(session, "three");
    broadcast(session, "three");
    mock.timers.tick(OUT_MAX_WAIT_MS);
    assert.deepEqual(ws.sent, ["\x1bc", "onetwothree"]);
    assert.ok(!session.lagging.has(ws));
  });
});

describe("session registry", () => {
  test("forgetting a stale session keeps the newer live one", () => {
    const sessions = new Map();
    const stale = getSession(sessions, "tab");
    forgetSession(sessions, stale);
    const live = getSession(sessions, "tab");
    forgetSession(sessions, stale);
    assert.equal(sessions.get("tab"), live);
  });

  test("rebinding an exited session re-registers it", () => {
    const sessions = new Map();
    const session = getSession(sessions, "tab");
    forgetSession(sessions, session);
    assert.equal(rebindSession(sessions, session, fakeWs()), session);
    assert.equal(sessions.get("tab"), session);
  });

  test("rebinding moves the client to a newer session with the same id", () => {
    const sessions = new Map();
    const ws = fakeWs();
    const stale = getSession(sessions, "tab");
    stale.clients.add(ws);
    forgetSession(sessions, stale);
    const live = getSession(sessions, "tab");
    assert.equal(rebindSession(sessions, stale, ws), live);
    assert.ok(live.clients.has(ws));
    assert.ok(!stale.clients.has(ws));
  });
});

describe("message framing", () => {
  const resize = JSON.stringify({ type: "resize", cols: 1, rows: 1 });

  test("binary frames are always input, even if they look like control JSON", () => {
    assert.deepEqual(classifyMessage(Buffer.from(resize), true), { kind: "input", text: resize });
  });

  test("text control JSON is a control message", () => {
    const frame = classifyMessage(Buffer.from(resize), false);
    assert.equal(frame.kind, "control");
    assert.equal(frame.msg.type, "resize");
  });

  test("text that is not a control message falls back to input", () => {
    const text = JSON.stringify({ type: "other" });
    assert.deepEqual(classifyMessage(Buffer.from(text), false), { kind: "input", text });
    assert.deepEqual(classifyMessage(Buffer.from("ls\r"), false), { kind: "input", text: "ls\r" });
  });
});

describe("pending input", () => {
  test("is written in order once the PTY exists", () => {
    const session = createSession("s");
    session.pendingInput.push("a", "b", "c");
    session.ptyProcess = fakePty();
    flushPendingInput(session);
    assert.deepEqual(session.ptyProcess.written, ["a", "b", "c"]);
    assert.deepEqual(session.pendingInput, []);
  });
});
