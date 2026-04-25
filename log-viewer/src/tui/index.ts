import React from "react";
import { render } from "ink";
import type { Config } from "../config.js";
import type { SourceHandle } from "../source.js";
import { App } from "./app.js";

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
  const instance = render(React.createElement(App, opts));
  await instance.waitUntilExit();
}
