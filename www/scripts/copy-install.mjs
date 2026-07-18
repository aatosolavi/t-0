#!/usr/bin/env node
/** Copy repo-root install.sh → www/public/install (no extension drift). */

import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const wwwRoot = join(here, "..");
const repoRoot = join(wwwRoot, "..");
const src = join(repoRoot, "install.sh");
const destDir = join(wwwRoot, "public");
const dest = join(destDir, "install");

mkdirSync(destDir, { recursive: true });
copyFileSync(src, dest);
console.log(`www: copied install.sh → public/install`);
