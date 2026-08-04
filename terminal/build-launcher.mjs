#!/usr/bin/env node

/**
 * Build + install the T-0 launcher (`t0`).
 * Legacy `mc` shims are kept so old muscle memory / scripts still work.
 */

import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readlinkSync,
  renameSync,
  symlinkSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";
import { dataDir } from "./data-dir.mjs";
import { packageRoot } from "./package-root.mjs";

const BIN_NAME = "t0";
const LEGACY_BIN_NAME = "mc";

// Package root (clone or global npm install) — not process.cwd().
const root = packageRoot();
const home = process.env.HOME || root;
const resolvedDataDir = dataDir(home);
const releaseBinary = join(
  root,
  `terminal/launcher-ratatui/target/release/${BIN_NAME}`,
);
const installDir = join(resolvedDataDir, "bin");
const installedBinary = join(installDir, BIN_NAME);
const legacyInstalledBinary = join(installDir, LEGACY_BIN_NAME);
const tempInstalledBinary = join(installDir, `.${BIN_NAME}-${process.pid}.tmp`);
const pathShimDir = join(home, ".local", "bin");
const pathShim = join(pathShimDir, BIN_NAME);
const legacyPathShim = join(pathShimDir, LEGACY_BIN_NAME);
const shellIntegrationDir = join(resolvedDataDir, "shell");
const zshIntegration = join(shellIntegrationDir, "t0.zsh");
const zshrc = join(home, ".zshrc");

const rustc = spawnSync("rustup", ["which", "rustc"], {
  encoding: "utf8",
  stdio: ["ignore", "pipe", "inherit"],
});

if (rustc.status !== 0) {
  process.exit(rustc.status || 1);
}

const toolchainBin = dirname(rustc.stdout.trim());
const env = {
  ...process.env,
  PATH: `${toolchainBin}:${process.env.PATH || ""}`,
};

const build = spawnSync(
  "rustup",
  [
    "run",
    "stable",
    "cargo",
    "build",
    "--release",
    "--locked",
    "--manifest-path",
    join(root, "terminal/launcher-ratatui/Cargo.toml"),
  ],
  {
    cwd: root,
    env,
    stdio: "inherit",
  },
);

if (build.status !== 0) {
  process.exit(build.status || 1);
}

mkdirSync(installDir, { recursive: true });
copyFileSync(releaseBinary, tempInstalledBinary);
chmodSync(tempInstalledBinary, 0o755);
renameSync(tempInstalledBinary, installedBinary);

// Legacy name → same binary (PATH + muscle memory).
try {
  unlinkSync(legacyInstalledBinary);
} catch {
  // ignore
}
symlinkSync(BIN_NAME, legacyInstalledBinary);

console.log(`[terminal] Installed T-0 launcher to ${installedBinary}`);
console.log(`[terminal] Legacy alias: ${legacyInstalledBinary} → ${BIN_NAME}`);

function ensureShim(shimPath, target) {
  mkdirSync(pathShimDir, { recursive: true });
  try {
    const existing = lstatSync(shimPath);
    if (existing.isSymbolicLink()) {
      const current = readlinkSync(shimPath);
      if (current !== target) {
        unlinkSync(shimPath);
        symlinkSync(target, shimPath);
      }
    } else {
      console.warn(
        `[terminal] Skipped PATH shim because ${shimPath} already exists and is not a symlink`,
      );
      return;
    }
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw error;
    }
    symlinkSync(target, shimPath);
  }
  console.log(`[terminal] PATH shim available at ${shimPath}`);
}

ensureShim(pathShim, installedBinary);
ensureShim(legacyPathShim, installedBinary);

mkdirSync(shellIntegrationDir, { recursive: true });
writeFileSync(
  zshIntegration,
  `# T-0 shell integration (\`t0\`; \`mc\` is a legacy alias).
# Lets T-0 selections cd the parent shell before launching a shell or agent.
t0() {
  local _t0_bin="\${MC_LAUNCHER:-}"
  if [[ -z "\$_t0_bin" ]]; then
    if [[ -x "\$HOME/.t-0/bin/t0" ]]; then
      _t0_bin="\$HOME/.t-0/bin/t0"
    elif [[ -x "\$HOME/.t-0/bin/mc" ]]; then
      _t0_bin="\$HOME/.t-0/bin/mc"
    elif [[ -x "\$HOME/.mission-control/bin/t0" ]]; then
      _t0_bin="\$HOME/.mission-control/bin/t0"
    elif [[ -x "\$HOME/.mission-control/bin/mc" ]]; then
      _t0_bin="\$HOME/.mission-control/bin/mc"
    elif [[ -x "\$HOME/.grok-mission-control/bin/mc" ]]; then
      _t0_bin="\$HOME/.grok-mission-control/bin/mc"
    else
      _t0_bin="t0"
    fi
  fi
  local _t0_cd_file="\${TMPDIR:-/tmp}/t0-cd-$$"

  MC_SHELL_INTEGRATION=1 MC_CD_FILE="\$_t0_cd_file" "\$_t0_bin" "\$@"
  local _t0_status=\$?

  if [[ \$_t0_status -eq 0 && -s "\$_t0_cd_file" ]]; then
    local _t0_action="shell"
    local _t0_target
    while IFS='=' read -r _t0_key _t0_value; do
      case "\$_t0_key" in
        action) _t0_action="\$_t0_value" ;;
        cwd) _t0_target="\$_t0_value" ;;
      esac
    done < "\$_t0_cd_file"
    if [[ -z "\$_t0_target" ]]; then
      _t0_target="\$(cat "\$_t0_cd_file")"
    fi
    rm -f "\$_t0_cd_file"
    if [[ -n "\$_t0_target" && -d "\$_t0_target" ]]; then
      builtin cd "\$_t0_target"
    fi

    case "\$_t0_action" in
      shell)
        ;;
      codex)
        local _t0_cmd="\${GROK_TERMINAL_CODEX_COMMAND:-codex}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      grok)
        local _t0_cmd="\${GROK_TERMINAL_GROK_COMMAND:-grok}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      pi)
        local _t0_cmd="\${GROK_TERMINAL_PI_COMMAND:-pi}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      cursor)
        local _t0_cmd="\${GROK_TERMINAL_CURSOR_COMMAND:-agent}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      claude)
        local _t0_cmd="\${GROK_TERMINAL_CLAUDE_COMMAND:-claude}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      amp)
        local _t0_cmd="\${GROK_TERMINAL_AMP_COMMAND:-amp}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      devin)
        local _t0_cmd="\${GROK_TERMINAL_DEVIN_COMMAND:-devin}"
        eval "\$_t0_cmd"
        return \$?
        ;;
      droid)
        local _t0_cmd="\${GROK_TERMINAL_DROID_COMMAND:-droid}"
        eval "\$_t0_cmd"
        return \$?
        ;;
    esac
  else
    rm -f "\$_t0_cd_file"
  fi

  return \$_t0_status
}

# Legacy alias — same function.
mc() { t0 "\$@"; }
`,
);

// Prefer modern t0.zsh block; keep legacy mc.zsh marker compatible.
const sourceBlock = `
# >>> t-0 launcher integration >>>
if [ -s "$HOME/.t-0/shell/t0.zsh" ]; then
  source "$HOME/.t-0/shell/t0.zsh"
elif [ -s "$HOME/.mission-control/shell/t0.zsh" ]; then
  source "$HOME/.mission-control/shell/t0.zsh"
elif [ -s "$HOME/.mission-control/shell/mc.zsh" ]; then
  source "$HOME/.mission-control/shell/mc.zsh"
elif [ -s "$HOME/.grok-mission-control/shell/mc.zsh" ]; then
  source "$HOME/.grok-mission-control/shell/mc.zsh"
fi
# <<< t-0 launcher integration <<<
`;

let existingZshrc = "";
try {
  existingZshrc = readFileSync(zshrc, "utf8");
} catch (error) {
  if (error?.code !== "ENOENT") {
    throw error;
  }
}

if (
  !existingZshrc.includes("t-0 launcher integration") &&
  !existingZshrc.includes("mission-control mc integration")
) {
  writeFileSync(zshrc, `${existingZshrc.trimEnd()}\n${sourceBlock}`);
} else if (
  existingZshrc.includes("mission-control mc integration") &&
  !existingZshrc.includes("t-0 launcher integration")
) {
  // Upgrade old zshrc block to prefer t0.zsh.
  const upgraded = existingZshrc.replace(
    /# >>> mission-control mc integration >>>[\s\S]*?# <<< mission-control mc integration <<</,
    sourceBlock.trim(),
  );
  writeFileSync(zshrc, upgraded);
}

console.log(`[terminal] Shell integration available at ${zshIntegration}`);
