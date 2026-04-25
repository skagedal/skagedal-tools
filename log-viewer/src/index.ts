#!/usr/bin/env node
import { existsSync } from "fs";
import { cli, define } from "gunshi";
import { configPath, ensureConfigFile, loadConfig } from "./config.js";
import { startSource, type SourceSpec } from "./source.js";
import { createTriggerRuntime } from "./triggers.js";
import { runTui } from "./tui/index.js";
import { runBrowser } from "./browser/server.js";

const DESCRIPTION = "View JSONL logs in a TUI or browser, with vi-like navigation.";

// Pull --exec out of argv before gunshi parses, so any flags meant for the
// executed command (e.g. `--exec kubectl logs -f my-pod`) aren't interpreted
// as log-viewer flags.
const { rest: rawArgv, exec: execArgv } = extractExec(process.argv.slice(2));

const main = define({
  name: "log-viewer",
  description: DESCRIPTION,
  args: {
    file: {
      type: "string",
      short: "f",
      description: "JSONL file to read (use '-' for stdin)",
    },
    exec: {
      type: "string",
      short: "e",
      description:
        "run an executable; its stdout is parsed as JSONL. All argv after the command is passed to it as args, e.g. `--exec kubectl logs -f my-pod`",
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
      negatable: true,
      description: "open the browser automatically in browser mode (use --no-open to suppress)",
      default: true,
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

    if (values.file && execArgv) {
      fail("--file and --exec are mutually exclusive", 2);
    }

    const positional = ctx.positionals ?? [];
    const fileArg = values.file ?? (positional.length > 0 ? positional[0] : undefined);

    let spec: SourceSpec;
    let sourceLabel: string;
    if (execArgv) {
      if (execArgv.length === 0) fail("--exec requires a command", 2);
      spec = { kind: "command", argv: execArgv };
      sourceLabel = `$ ${formatCommand(execArgv)}`;
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
        "no input given — pass a JSONL file (or '-' for stdin), use --exec, or pipe data in",
        2,
      );
    }

    const config = loadConfig();
    const source = startSource(spec, { defaultField: config.defaultField });

    if (config.triggers.length > 0) {
      const triggers = createTriggerRuntime(config.triggers);
      triggers.start();
      source.onEntry((entry) => triggers.handle(entry));
    }

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

/**
 * Extract `--exec`/`-e` and the rest of argv that follows it. Everything from
 * the flag onward (after its value) is consumed as positional args to the
 * executed command, so `log-viewer --exec kubectl logs -f my-pod` doesn't
 * have its `-f` reinterpreted as log-viewer's `--file`.
 *
 * Supports `--exec foo bar baz`, `-e foo bar`, and `--exec=foo bar`.
 */
function extractExec(argv: string[]): { rest: string[]; exec: string[] | null } {
  const rest: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--exec" || a === "-e") {
      return { rest, exec: argv.slice(i + 1) };
    }
    if (a.startsWith("--exec=")) {
      const head = a.slice("--exec=".length);
      return { rest, exec: [head, ...argv.slice(i + 1)] };
    }
    rest.push(a);
  }
  return { rest, exec: null };
}

function formatCommand(argv: string[]): string {
  return argv.map((a) => (/[\s'"\\$]/.test(a) ? JSON.stringify(a) : a)).join(" ");
}

function fail(message: string, code = 1): never {
  process.stderr.write(`error: ${message}\n`);
  process.exit(code);
}

try {
  await cli(rawArgv, main, {
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
