import React from "react";
import { render } from "ink";
import type { Config } from "../config.js";
import type { SourceHandle } from "../source.js";
import { App } from "./app.js";

const ENTER_ALT_SCREEN = "\x1b[?1049h";
const LEAVE_ALT_SCREEN = "\x1b[?1049l";
const HIDE_CURSOR = "\x1b[?25l";
const SHOW_CURSOR = "\x1b[?25h";

export interface RunTuiOptions {
  config: Config;
  source: SourceHandle;
  sourceLabel: string;
}

export async function runTui(opts: RunTuiOptions): Promise<void> {
  if (!process.stdout.isTTY) {
    throw new Error(
      "stdout is not a TTY — the TUI needs an interactive terminal. " +
        "If you're piping data via stdin, run `log-viewer --browser` instead, " +
        "or use a file/command source.",
    );
  }

  // Single cleanup path: close the input source and restore the terminal.
  // Runs at most once, regardless of how we exit (normal q/Esc, Ink's
  // built-in Ctrl-C handling, SIGINT/SIGTERM, or a thrown error).
  let cleanedUp = false;
  const cleanup = () => {
    if (cleanedUp) return;
    cleanedUp = true;
    opts.source.close();
    process.stdout.write(SHOW_CURSOR + LEAVE_ALT_SCREEN);
  };

  process.stdout.write(ENTER_ALT_SCREEN + HIDE_CURSOR);
  process.once("exit", cleanup);
  // For signals, do our cleanup and then re-raise the default behavior so
  // the process exits with the conventional 128+signal status.
  const onSignal = (signal: NodeJS.Signals, code: number) => {
    cleanup();
    process.exit(code);
  };
  process.once("SIGINT", () => onSignal("SIGINT", 130));
  process.once("SIGTERM", () => onSignal("SIGTERM", 143));
  process.once("SIGHUP", () => onSignal("SIGHUP", 129));

  try {
    const instance = render(React.createElement(App, opts), { exitOnCtrlC: true });
    await instance.waitUntilExit();
  } finally {
    cleanup();
  }
}
