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
const RESET = `${ESC}[0m`;

export function hyperlink(url: string, text: string): string {
  return `${ESC}]8;;${url}${ST}${text}${ESC}]8;;${ST}`;
}

export type Color =
  | "blue"
  | "cyan"
  | "green"
  | "yellow"
  | "red"
  | "magenta"
  | "gray";

const COLOR_CODES: Record<Color, string> = {
  blue: "34",
  cyan: "36",
  green: "32",
  yellow: "33",
  red: "31",
  magenta: "35",
  gray: "90",
};

export interface ColorOptions {
  color?: Color;
  bold?: boolean;
  underline?: boolean;
}

export function colorize(text: string, opts: ColorOptions): string {
  const codes: string[] = [];
  if (opts.bold) codes.push("1");
  if (opts.underline) codes.push("4");
  if (opts.color) codes.push(COLOR_CODES[opts.color]);
  if (codes.length === 0) return text;
  return `${ESC}[${codes.join(";")}m${text}${RESET}`;
}

/**
 * Format a clickable, styled link: an OSC 8 hyperlink whose visible text is
 * wrapped in a "link-like" SGR style (bright blue + underline by default).
 * Terminals without OSC 8 support still see the styled text.
 */
export function styledLink(url: string, text: string, opts: ColorOptions = { color: "cyan", underline: true }): string {
  return hyperlink(url, colorize(text, opts));
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
