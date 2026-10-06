/**
 * T-0 — HTML server for the browser terminal (Bun).
 *
 * Serves index.html on :4321. The real PTY lives in terminal/pty-server.mjs
 * under Node (node-pty is more reliable there than under Bun on macOS).
 *
 *   bun run terminal  →  http://127.0.0.1:4321
 */

import { randomBytes } from "crypto";
import { mkdirSync, readFileSync } from "fs";
import { dirname, join, resolve } from "path";
import { fileURLToPath } from "url";
import { dataDir } from "./data-dir.mjs";

const PORT = Number(process.env.MC_HTML_PORT || process.env.PORT || 4321);
const HOST = process.env.MC_BIND_HOST || "127.0.0.1";
const MAX_ATTACHMENTS = Number(process.env.MC_MAX_ATTACHMENTS || 20);
const MAX_ATTACHMENT_BYTES = Number(process.env.MC_MAX_ATTACHMENT_BYTES || 25 * 1024 * 1024);
const MAX_ATTACHMENT_TOTAL_BYTES = Number(
  process.env.MC_MAX_ATTACHMENT_TOTAL_BYTES || 100 * 1024 * 1024,
);
const currentDir = dirname(fileURLToPath(import.meta.url));
const distDir = resolve(currentDir, "dist");
const ALLOWED_ORIGINS = new Set(
  (process.env.MC_ALLOWED_ORIGINS || "")
    .split(",")
    .map((origin) => origin.trim())
    .filter(Boolean)
    .concat([
      `http://127.0.0.1:${PORT}`,
      `http://localhost:${PORT}`,
      "http://127.0.0.1:4321",
      "http://localhost:4321",
      // portless (https://portless.sh) fronts us at a stable https name.
      "https://t0.localhost",
    ]),
);
const PTY_PORT = Number(process.env.MC_PTY_PORT || 4322);
const PTY_URL = `ws://127.0.0.1:${PTY_PORT}`;
// A browser tab further behind than this is dropped; it reconnects and the
// broker replays history, which is bounded (unlike this buffer).
const MAX_CLIENT_BUFFERED = 8 * 1024 * 1024;

function resolveDataDir(): string {
  return dataDir();
}

// For fast iteration during testing we re-read the HTML on every request.
// (Cheap on localhost. We can cache later.)
function getHtml(nonce: string): string {
  try {
    return readFileSync(resolve(currentDir, "index.html"), "utf8")
      .replace("<style>", `<style nonce="${nonce}">`)
      .replace("<script type=\"module\">", `<script type="module" nonce="${nonce}">`);
  } catch {
    return "<h1>T-0</h1><p>index.html not found next to server.ts</p>";
  }
}

function securityHeaders(nonce: string): Record<string, string> {
  return {
    "Content-Security-Policy": [
      "default-src 'self'",
      `script-src 'self' 'nonce-${nonce}'`,
      // xterm positions terminal cells with inline styles; keep scripts nonce-locked.
      "style-src 'self' 'unsafe-inline'",
      // The PTY is proxied same-origin at /pty; ws:// forms spelled out for
      // browsers that don't extend 'self' to websockets.
      `connect-src 'self' ws://127.0.0.1:${PORT} ws://localhost:${PORT}`,
      "img-src 'self' data:",
      "font-src 'self'",
      "object-src 'none'",
      "base-uri 'none'",
      "frame-ancestors 'none'",
    ].join("; "),
    "X-Content-Type-Options": "nosniff",
    "X-Frame-Options": "DENY",
    "Referrer-Policy": "no-referrer",
  };
}

function originAllowed(req: Request): boolean {
  const origin = req.headers.get("origin");
  return Boolean(origin && ALLOWED_ORIGINS.has(origin));
}

function sanitizeFileName(name: string): string {
  const base = name.split(/[/\\]/).pop() || "attachment";
  const cleaned = base
    .normalize("NFKD")
    .replace(/[^a-zA-Z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (!cleaned || cleaned === "." || cleaned === "..") {
    return "attachment";
  }
  return cleaned.slice(0, 180);
}

function attachmentDir(): string {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const random = crypto.randomUUID().slice(0, 8);
  const dir = join(resolveDataDir(), "attachments", `${stamp}-${random}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

if (HOST !== "127.0.0.1" && HOST !== "localhost" && HOST !== "::1") {
  if (process.env.MC_ALLOW_REMOTE_BIND !== "1") {
    console.error(
      `[T-0] Refusing bind host ${HOST}. Use 127.0.0.1 or set MC_ALLOW_REMOTE_BIND=1 (dangerous).`,
    );
    process.exit(78);
  }
  console.warn(`[T-0] WARNING: binding HTML server to ${HOST}`);
}

const server = Bun.serve({
  hostname: HOST,
  port: PORT,
  maxRequestBodySize: MAX_ATTACHMENT_TOTAL_BYTES,

  async fetch(req: Request, srv) {
    const url = new URL(req.url);

    // Same-origin PTY endpoint → proxied to the Node broker on PTY_PORT.
    // Lets a single fronting URL (e.g. https://t0.localhost via portless)
    // carry both the page and the terminal websocket.
    if (url.pathname === "/pty") {
      if (!originAllowed(req)) {
        return new Response("Origin not allowed", { status: 403 });
      }
      const data = { broker: null as WebSocket | null, pending: [] as (string | Uint8Array<ArrayBuffer>)[] };
      if (srv.upgrade(req, { data: data as never })) {
        return undefined;
      }
      return new Response("WebSocket upgrade required", { status: 426 });
    }

    if (req.method === "GET" && (url.pathname === "/vendor.js" || url.pathname === "/vendor.css")) {
      const name = url.pathname.slice(1);
      const contentType = name.endsWith(".js")
        ? "text/javascript; charset=utf-8"
        : "text/css; charset=utf-8";
      try {
        return new Response(Bun.file(join(distDir, name)), {
          headers: {
            "Content-Type": contentType,
            "Cache-Control": "no-cache",
            "X-Content-Type-Options": "nosniff",
          },
        });
      } catch {
        return new Response("Asset not built", { status: 404 });
      }
    }

    if (url.pathname === "/attachments" && req.method === "POST") {
      if (!originAllowed(req)) {
        return Response.json({ error: "Origin not allowed" }, { status: 403 });
      }

      const contentLength = Number(req.headers.get("content-length") || 0);
      if (contentLength > MAX_ATTACHMENT_TOTAL_BYTES) {
        return Response.json({ error: "Attachment request too large" }, { status: 413 });
      }

      const form = await req.formData();
      const files = form
        .getAll("files")
        .filter((item): item is File => item instanceof File);

      if (files.length === 0) {
        return Response.json({ error: "No files uploaded" }, { status: 400 });
      }
      if (files.length > MAX_ATTACHMENTS) {
        return Response.json(
          { error: `Too many files (max ${MAX_ATTACHMENTS})` },
          { status: 400 },
        );
      }
      const totalBytes = files.reduce((total, file) => total + file.size, 0);
      if (totalBytes > MAX_ATTACHMENT_TOTAL_BYTES) {
        return Response.json({ error: "Attachments too large" }, { status: 413 });
      }

      const dir = attachmentDir();
      const paths: string[] = [];

      for (const file of files) {
        if (typeof file.size === "number" && file.size > MAX_ATTACHMENT_BYTES) {
          return Response.json(
            { error: `File too large (max ${MAX_ATTACHMENT_BYTES} bytes)` },
            { status: 400 },
          );
        }
        const safeName = sanitizeFileName(file.name);
        const path = resolve(join(dir, safeName));
        if (!path.startsWith(resolve(dir) + "/") && path !== resolve(dir)) {
          return Response.json({ error: "Invalid file name" }, { status: 400 });
        }
        await Bun.write(path, file);
        paths.push(path);
      }

      return Response.json({ paths });
    }

    // Everything else → the beautiful full-page terminal
    // Fresh read so you can edit index.html and just reload the tab during testing.
    const nonce = randomBytes(18).toString("base64");
    return new Response(getHtml(nonce), {
      headers: {
        "Content-Type": "text/html; charset=utf-8",
        "Cache-Control": "no-cache, no-store, must-revalidate",
        ...securityHeaders(nonce),
      },
    });
  },

  websocket: {
    open(ws: any) {
      // The browser's Origin was validated at upgrade; connect to the broker
      // with the server's own identity, which its allowlist already trusts.
      const broker = new WebSocket(PTY_URL, {
        headers: { origin: `http://127.0.0.1:${PORT}` },
      } as any);
      broker.binaryType = "arraybuffer";
      ws.data.broker = broker;
      broker.onopen = () => {
        for (const msg of ws.data.pending) broker.send(msg);
        ws.data.pending.length = 0;
      };
      broker.onmessage = (event) => {
        if (ws.getBufferedAmount() > MAX_CLIENT_BUFFERED) {
          ws.close(1013, "client too slow");
          return;
        }
        ws.send(event.data);
      };
      broker.onclose = () => ws.close();
      broker.onerror = () => ws.close();
    },
    message(ws: any, message: string | Uint8Array<ArrayBuffer>) {
      const broker: WebSocket | null = ws.data.broker;
      if (!broker) return;
      if (broker.readyState === WebSocket.OPEN) {
        broker.send(message);
      } else if (broker.readyState === WebSocket.CONNECTING) {
        ws.data.pending.push(message);
      }
    },
    close(ws: any) {
      try {
        ws.data.broker?.close();
      } catch {
        // already closed
      }
    },
  },

});

const openHost = HOST === "0.0.0.0" ? "127.0.0.1" : HOST;
console.log("");
console.log("T-0 HTML server ready (Bun)");
console.log(`    Open http://${openHost}:${PORT}`);
console.log("");
console.log("   The real PTY lives in a separate Node process (terminal/pty-server.mjs on :4322).");
console.log("   Run `bun run terminal` to start both pieces together.");
console.log("");

// Graceful shutdown
process.on("SIGINT", () => {
  console.log("\nShutting down terminal server...");
  server.stop(true);
  process.exit(0);
});
