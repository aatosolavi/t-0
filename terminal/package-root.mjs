#!/usr/bin/env node
/**
 * Absolute path to the T-0 package root (works from clone or global npm install).
 */
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export function packageRoot(fromUrl = import.meta.url) {
  // terminal/package-root.mjs → repo root
  return join(dirname(fileURLToPath(fromUrl)), "..");
}
