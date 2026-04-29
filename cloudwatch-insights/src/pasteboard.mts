/**
 * Cross-platform clipboard read/write via the host's standard utilities.
 *
 *   macOS    → pbcopy / pbpaste
 *   Windows  → clip / powershell Get-Clipboard
 *   Linux    → wl-copy/wl-paste (Wayland) or xclip/xsel (X11)
 */

import { spawn, spawnSync } from "child_process";

export class PasteboardError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PasteboardError";
  }
}

interface CopyCommand {
  cmd: string;
  args: string[];
}

interface PasteCommand {
  cmd: string;
  args: string[];
}

export function copyCommand(platform: NodeJS.Platform = process.platform): CopyCommand {
  switch (platform) {
    case "darwin":
      return { cmd: "pbcopy", args: [] };
    case "win32":
      return { cmd: "clip", args: [] };
    default:
      if (process.env.WAYLAND_DISPLAY && hasCommand("wl-copy")) {
        return { cmd: "wl-copy", args: [] };
      }
      if (hasCommand("xclip")) {
        return { cmd: "xclip", args: ["-selection", "clipboard"] };
      }
      if (hasCommand("xsel")) {
        return { cmd: "xsel", args: ["--clipboard", "--input"] };
      }
      throw new PasteboardError(
        "no clipboard tool found (install xclip, xsel, or wl-clipboard)",
      );
  }
}

export function pasteCommand(platform: NodeJS.Platform = process.platform): PasteCommand {
  switch (platform) {
    case "darwin":
      return { cmd: "pbpaste", args: [] };
    case "win32":
      return { cmd: "powershell", args: ["-NoProfile", "-Command", "Get-Clipboard"] };
    default:
      if (process.env.WAYLAND_DISPLAY && hasCommand("wl-paste")) {
        return { cmd: "wl-paste", args: ["--no-newline"] };
      }
      if (hasCommand("xclip")) {
        return { cmd: "xclip", args: ["-selection", "clipboard", "-o"] };
      }
      if (hasCommand("xsel")) {
        return { cmd: "xsel", args: ["--clipboard", "--output"] };
      }
      throw new PasteboardError(
        "no clipboard tool found (install xclip, xsel, or wl-clipboard)",
      );
  }
}

export async function copyToPasteboard(text: string): Promise<void> {
  const { cmd, args } = copyCommand();
  await new Promise<void>((resolve, reject) => {
    const child = spawn(cmd, args, { stdio: ["pipe", "ignore", "pipe"] });
    let stderr = "";
    child.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (err) =>
      reject(new PasteboardError(`failed to run ${cmd}: ${err.message}`)),
    );
    child.on("close", (code) => {
      if (code === 0) resolve();
      else reject(new PasteboardError(`${cmd} exited with ${code}: ${stderr.trim()}`));
    });
    child.stdin?.end(text);
  });
}

export async function readFromPasteboard(): Promise<string> {
  const { cmd, args } = pasteCommand();
  return await new Promise<string>((resolve, reject) => {
    const child = spawn(cmd, args, { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (err) =>
      reject(new PasteboardError(`failed to run ${cmd}: ${err.message}`)),
    );
    child.on("close", (code) => {
      if (code === 0) resolve(stdout);
      else reject(new PasteboardError(`${cmd} exited with ${code}: ${stderr.trim()}`));
    });
  });
}

function hasCommand(cmd: string): boolean {
  const result = spawnSync("which", [cmd], { stdio: "ignore" });
  return result.status === 0;
}
