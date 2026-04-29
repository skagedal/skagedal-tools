import { spawn } from "node:child_process";

export async function copyToSystemClipboard(text: string): Promise<void> {
  const [cmd, ...args] = pickCommand();
  await new Promise<void>((resolve, reject) => {
    const child = spawn(cmd!, args, { stdio: ["pipe", "ignore", "pipe"] });
    let stderr = "";
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${cmd} exited with code ${code}${stderr ? `: ${stderr.trim()}` : ""}`));
    });
    child.stdin.end(text);
  });
}

function pickCommand(): string[] {
  if (process.platform === "darwin") return ["pbcopy"];
  if (process.platform === "linux") {
    if (process.env.WAYLAND_DISPLAY) return ["wl-copy"];
    return ["xclip", "-selection", "clipboard"];
  }
  throw new Error(`clipboard not supported on platform ${process.platform}`);
}
