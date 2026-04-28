/**
 * Tiny terminal-escape helpers.
 *
 * `hyperlink` wraps text in an OSC 8 hyperlink escape sequence:
 *   \x1b]8;;<URL>\x1b\\<TEXT>\x1b]8;;\x1b\\
 * Terminals that understand OSC 8 (iTerm2, recent VS Code, GNOME Terminal,
 * WezTerm, …) render <TEXT> as a clickable link to <URL>; ones that don't
 * simply display <TEXT>.
 *
 * Reference: https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda
 */
const ESC = "\x1b";
const ST = `${ESC}\\`;

export function hyperlink(url: string, text: string): string {
  return `${ESC}]8;;${url}${ST}${text}${ESC}]8;;${ST}`;
}

/**
 * Per-platform command for opening a URL in the user's default browser.
 * Exported for unit tests; the runtime caller spawns the result.
 */
export function openUrlCommand(
  url: string,
  platform: NodeJS.Platform = process.platform,
): { cmd: string; args: string[] } {
  switch (platform) {
    case "darwin":
      return { cmd: "open", args: [url] };
    case "win32":
      // `start` is a cmd.exe builtin; the empty "" is the window title.
      return { cmd: "cmd", args: ["/c", "start", "", url] };
    default:
      return { cmd: "xdg-open", args: [url] };
  }
}
