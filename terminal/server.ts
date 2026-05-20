/**
 * Grok Terminal — Minimal Level-1 MVP
 *
 * A real local shell (zsh/bash) running inside a browser tab via xterm.js + PTY over WebSocket.
 *
 * Why this exists (per the Helium + mission-control vision):
 * - Open a tab in Helium (or any browser) that *is* your terminal.
 * - Cmd+T in Helium = instant new real shell.
 * - Zero friction, native mental model, perfect for agentic work later.
 *
 * Run:
 *   bun run terminal
 *
 * Then visit http://localhost:4321
 *
 * For Helium new-tab override:
 *   - Load the extension/ folder (or point chrome_url_overrides.newtab at a redirector)
 *   - Or simply use this URL as your new-tab page.
 *
 * IMPORTANT: The actual PTY handling lives in terminal/pty-server.mjs, which
 * runs under Node because @lydell/node-pty has unstable fd behavior under Bun
 * on macOS.
 *
 * This Bun file only serves the HTML page on :4321.
 * The browser then connects to the real PTY broker on :4322.
 */

import { mkdirSync, readFileSync } from "fs";
import { homedir } from "os";
import { join, resolve } from "path";

const PORT = Number(process.env.PORT || 4321);

// For fast iteration during testing we re-read the HTML on every request.
// (Cheap on localhost. We can cache later.)
function getHtml(): string {
  try {
    return readFileSync(resolve(import.meta.dir, "index.html"), "utf8");
  } catch {
    return "<h1>Grok Terminal</h1><p>index.html not found next to server.ts</p>";
  }
}

function sanitizeFileName(name: string): string {
  const cleaned = name
    .normalize("NFKD")
    .replace(/[^a-zA-Z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return cleaned || "attachment";
}

function attachmentDir(): string {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const random = crypto.randomUUID().slice(0, 8);
  const dir = join(homedir(), ".grok-mission-control", "attachments", `${stamp}-${random}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

const server = Bun.serve({
  port: PORT,

  async fetch(req: Request) {
    const url = new URL(req.url);

    if (url.pathname === "/attachments" && req.method === "POST") {
      const form = await req.formData();
      const files = form
        .getAll("files")
        .filter((item): item is File => item instanceof File);

      if (files.length === 0) {
        return Response.json({ error: "No files uploaded" }, { status: 400 });
      }

      const dir = attachmentDir();
      const paths: string[] = [];

      for (const file of files) {
        const path = join(dir, sanitizeFileName(file.name));
        await Bun.write(path, file);
        paths.push(path);
      }

      return Response.json({ paths });
    }

    // Everything else → the beautiful full-page terminal
    // Fresh read so you can edit index.html and just reload the tab during testing.
    return new Response(getHtml(), {
      headers: {
        "Content-Type": "text/html; charset=utf-8",
        "Cache-Control": "no-cache, no-store, must-revalidate",
      },
    });
  },

});

console.log("");
console.log("🚀  Grok Terminal HTML server ready (Bun)");
console.log(`    Open http://localhost:${PORT} in Helium`);
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
