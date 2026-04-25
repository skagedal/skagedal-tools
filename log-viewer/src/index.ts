#!/usr/bin/env node
import { existsSync } from "fs";
import { cli, define } from "gunshi";
import { configPath, ensureConfigFile, loadConfig } from "./config.js";
import { startSource, type SourceSpec } from "./source.js";
import { runTui } from "./tui/index.js";
import { runBrowser } from "./browser/server.js";

const DESCRIPTION = "View JSONL logs in a TUI or browser, with vi-like navigation.";

const main = define({
  name: "log-viewer",
  description: DESCRIPTION,
  args: {
    file: {
      type: "string",
      short: "f",
      description: "JSONL file to read (use '-' for stdin)",
    },
    command: {
      type: "string",
      short: "c",
      description: "executable command to run; its stdout is parsed as JSONL",
    },
    browser: {
      type: "boolean",
      short: "b",
      description: "start a browser app (Vite server) instead of the TUI",
      default: false,
    },
    port: {
      type: "number",
      short: "p",
      description: "port for the browser server",
      default: 5173,
    },
    host: {
      type: "string",
      description: "host for the browser server",
      default: "127.0.0.1",
    },
    open: {
      type: "boolean",
      description: "open the browser automatically (browser mode only)",
      default: false,
    },
    "init-config": {
      type: "boolean",
      description: "create the config file if missing and exit",
      default: false,
    },
  },
  run: async (ctx) => {
    const values = ctx.values as {
      file?: string;
      command?: string;
      browser: boolean;
      port: number;
      host: string;
      open: boolean;
      "init-config": boolean;
    };

    if (values["init-config"]) {
      const path = configPath();
      const created = ensureConfigFile(path);
      process.stdout.write(`${created ? "created" : "already exists"}: ${path}\n`);
      return;
    }

    if (values.file && values.command) {
      fail("--file and --command are mutually exclusive", 2);
    }

    const positional = ctx.positionals ?? [];
    const fileArg = values.file ?? (positional.length > 0 ? positional[0] : undefined);

    let spec: SourceSpec;
    let sourceLabel: string;
    if (values.command) {
      const argv = parseCommand(values.command);
      spec = { kind: "command", argv };
      sourceLabel = `$ ${values.command}`;
    } else if (fileArg === "-") {
      spec = { kind: "stdin" };
      sourceLabel = "stdin";
    } else if (fileArg) {
      if (!existsSync(fileArg)) fail(`file not found: ${fileArg}`, 2);
      spec = { kind: "file", path: fileArg };
      sourceLabel = fileArg;
    } else if (!process.stdin.isTTY) {
      spec = { kind: "stdin" };
      sourceLabel = "stdin";
    } else {
      fail(
        "no input given — pass a JSONL file (or '-' for stdin), use --command, or pipe data in",
        2,
      );
    }

    const config = loadConfig();
    const source = startSource(spec, { defaultField: config.defaultField });

    if (values.browser) {
      await runBrowser({
        config,
        source,
        sourceLabel,
        port: values.port,
        host: values.host,
        open: values.open,
      });
    } else {
      await runTui({ config, source, sourceLabel });
    }
  },
});

function parseCommand(input: string): string[] {
  const out: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let escape = false;
  for (const ch of input) {
    if (escape) {
      current += ch;
      escape = false;
      continue;
    }
    if (ch === "\\") {
      escape = true;
      continue;
    }
    if (quote) {
      if (ch === quote) quote = null;
      else current += ch;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }
    if (/\s/.test(ch)) {
      if (current.length > 0) {
        out.push(current);
        current = "";
      }
      continue;
    }
    current += ch;
  }
  if (current.length > 0) out.push(current);
  if (quote) {
    throw new Error(`unterminated ${quote} quote in --command`);
  }
  return out;
}

function fail(message: string, code = 1): never {
  process.stderr.write(`error: ${message}\n`);
  process.exit(code);
}

try {
  await cli(process.argv.slice(2), main, {
    name: "log-viewer",
    version: "1.0.0",
    description: DESCRIPTION,
    renderHeader: null,
  });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}
