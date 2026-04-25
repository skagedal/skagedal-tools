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

  let restored = false;
  const restore = () => {
    if (restored) return;
    restored = true;
    process.stdout.write(SHOW_CURSOR + LEAVE_ALT_SCREEN);
  };
  process.stdout.write(ENTER_ALT_SCREEN + HIDE_CURSOR);
  // Best-effort cleanup if we exit through an unusual path.
  process.once("exit", restore);
  process.once("SIGINT", restore);
  process.once("SIGTERM", restore);

  try {
    const instance = render(React.createElement(App, opts), { exitOnCtrlC: true });
    await instance.waitUntilExit();
  } finally {
    restore();
  }
}
