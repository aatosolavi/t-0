#!/usr/bin/env node

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

const root = process.cwd();
const home = process.env.HOME || root;
const resolvedDataDir = dataDir(home);
const releaseBinary = join(
  root,
  "terminal/launcher-ratatui/target/release/mc",
);
const installDir = join(resolvedDataDir, "bin");
const installedBinary = join(installDir, "mc");
const tempInstalledBinary = join(installDir, `.mc-${process.pid}.tmp`);
const pathShimDir = join(home, ".local", "bin");
const pathShim = join(pathShimDir, "mc");
const shellIntegrationDir = join(resolvedDataDir, "shell");
const zshIntegration = join(shellIntegrationDir, "mc.zsh");
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
    "terminal/launcher-ratatui/Cargo.toml",
  ],
  {
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

console.log(`[terminal] Installed Ratatui launcher to ${installedBinary}`);

mkdirSync(pathShimDir, { recursive: true });
try {
  const existing = lstatSync(pathShim);
  if (existing.isSymbolicLink()) {
    const target = readlinkSync(pathShim);
    if (target !== installedBinary) {
      unlinkSync(pathShim);
      symlinkSync(installedBinary, pathShim);
    }
  } else {
    console.warn(`[terminal] Skipped PATH shim because ${pathShim} already exists`);
  }
} catch (error) {
  if (error?.code !== "ENOENT") {
    throw error;
  }
  symlinkSync(installedBinary, pathShim);
}

console.log(`[terminal] PATH shim available at ${pathShim}`);

mkdirSync(shellIntegrationDir, { recursive: true });
writeFileSync(
  zshIntegration,
  `# Launchpad (mc) shell integration.
# This wrapper lets Launchpad selections cd the parent shell before
# launching a shell or agent. Herdr and other terminal managers can then observe
# the cwd change.
mc() {
  local _mc_bin="\${MC_LAUNCHER:-}"
  if [[ -z "\$_mc_bin" ]]; then
    if [[ -x "\$HOME/.mission-control/bin/mc" ]]; then
      _mc_bin="\$HOME/.mission-control/bin/mc"
    else
      _mc_bin="\$HOME/.grok-mission-control/bin/mc"
    fi
  fi
  local _mc_cd_file="\${TMPDIR:-/tmp}/mc-cd-$$"

  MC_SHELL_INTEGRATION=1 MC_CD_FILE="\$_mc_cd_file" "\$_mc_bin" "\$@"
  local _mc_status=\$?

  if [[ \$_mc_status -eq 0 && -s "\$_mc_cd_file" ]]; then
    local _mc_action="shell"
    local _mc_target
    while IFS='=' read -r _mc_key _mc_value; do
      case "\$_mc_key" in
        action) _mc_action="\$_mc_value" ;;
        cwd) _mc_target="\$_mc_value" ;;
      esac
    done < "\$_mc_cd_file"
    if [[ -z "\$_mc_target" ]]; then
      _mc_target="\$(cat "\$_mc_cd_file")"
    fi
    rm -f "\$_mc_cd_file"
    if [[ -n "\$_mc_target" && -d "\$_mc_target" ]]; then
      builtin cd "\$_mc_target"
    fi

    case "\$_mc_action" in
      shell)
        ;;
      codex)
        local _mc_codex_command="\${GROK_TERMINAL_CODEX_COMMAND:-codex}"
        eval "\$_mc_codex_command"
        return \$?
        ;;
      grok)
        local _mc_grok_command="\${GROK_TERMINAL_GROK_COMMAND:-grok}"
        eval "\$_mc_grok_command"
        return \$?
        ;;
      pi)
        local _mc_pi_command="\${GROK_TERMINAL_PI_COMMAND:-pi}"
        eval "\$_mc_pi_command"
        return \$?
        ;;
      cursor)
        local _mc_cursor_command="\${GROK_TERMINAL_CURSOR_COMMAND:-agent}"
        eval "\$_mc_cursor_command"
        return \$?
        ;;
      claude)
        local _mc_claude_command="\${GROK_TERMINAL_CLAUDE_COMMAND:-claude}"
        eval "\$_mc_claude_command"
        return \$?
        ;;
      amp)
        local _mc_amp_command="\${GROK_TERMINAL_AMP_COMMAND:-amp}"
        eval "\$_mc_amp_command"
        return \$?
        ;;
      devin)
        local _mc_devin_command="\${GROK_TERMINAL_DEVIN_COMMAND:-devin}"
        eval "\$_mc_devin_command"
        return \$?
        ;;
      droid)
        local _mc_droid_command="\${GROK_TERMINAL_DROID_COMMAND:-droid}"
        eval "\$_mc_droid_command"
        return \$?
        ;;
    esac
  else
    rm -f "\$_mc_cd_file"
  fi

  return \$_mc_status
}
`,
);

const sourceBlock = `
# >>> mission-control mc integration >>>
if [ -s "$HOME/.mission-control/shell/mc.zsh" ]; then
  source "$HOME/.mission-control/shell/mc.zsh"
elif [ -s "$HOME/.grok-mission-control/shell/mc.zsh" ]; then
  source "$HOME/.grok-mission-control/shell/mc.zsh"
fi
# <<< mission-control mc integration <<<
`;

let existingZshrc = "";
try {
  existingZshrc = readFileSync(zshrc, "utf8");
} catch (error) {
  if (error?.code !== "ENOENT") {
    throw error;
  }
}

if (!existingZshrc.includes("mission-control mc integration")) {
  writeFileSync(zshrc, `${existingZshrc.trimEnd()}\n${sourceBlock}`);
}

console.log(`[terminal] Shell integration available at ${zshIntegration}`);
