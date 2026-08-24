#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const result = spawnSync(
  "bun",
  [
    "build",
    join(root, "www/agentation-entry.js"),
    "--outfile",
    join(root, "www/public/agentation.bundle.js"),
    "--minify",
    "--target",
    "browser",
  ],
  { cwd: root, stdio: "inherit" },
);
process.exit(result.status ?? 1);
