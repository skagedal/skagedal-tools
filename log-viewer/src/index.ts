#!/usr/bin/env node
import { existsSync } from "fs";
import { spawnSync } from "child_process";
import { cli, define } from "gunshi";
import { applyProfile, configPath, ensureConfigFile, loadConfig } from "./config.js";
import { startSource, type SourceSpec } from "./source.js";
import { createTriggerRuntime } from "./triggers.js";
import { runTui } from "./tui/index.js";

const DESCRIPTION = "View JSONL logs in a TUI or browser.";

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
    config: {
      type: "string",
      short: "c",
      description: "path to config file (overrides ~/.skagedal-tools/log-viewer/config.json5 and $LOG_VIEWER_CONFIG)",
    },
    profile: {
      type: "string",
      description: "name of a profile defined in the config file; overrides default fields/default_field",
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
  },
  run: async (ctx) => {
    const values = ctx.values as {
      file?: string;
      config?: string;
      profile?: string;
      browser: boolean;
      port: number;
      host: string;
      open: boolean;
    };

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

    if (values.config && !existsSync(values.config)) {
      fail(`config file not found: ${values.config}`, 2);
    }
    let config = loadConfig(values.config ?? configPath());
    if (values.profile) {
      try {
        config = applyProfile(config, values.profile);
      } catch (err) {
        fail(err instanceof Error ? err.message : String(err), 2);
      }
    }
    const source = startSource(spec, { defaultField: config.defaultField });

    if (config.triggers.length > 0) {
      const triggers = createTriggerRuntime(config.triggers);
      triggers.start();
      source.onEntry((entry) => triggers.handle(entry));
    }

    if (values.browser) {
      const { runBrowser } = await import("./browser/server.js");
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

const editConfigCmd = define({
  name: "edit-config",
  description: "create the config file if missing and open it in $EDITOR",
  args: {
    config: {
      type: "string",
      short: "c",
      description: "path to config file (overrides default and $LOG_VIEWER_CONFIG)",
    },
  },
  run: (ctx) => {
    const values = ctx.values as { config?: string };
    const path = values.config ?? configPath();
    const created = ensureConfigFile(path);
    if (created) process.stderr.write(`Created ${path}\n`);
    openEditor(path);
  },
});

function openEditor(path: string): void {
  const editor = process.env.VISUAL || process.env.EDITOR;
  if (!editor) {
    fail("no editor set — define $EDITOR (or $VISUAL) and retry", 2);
  }
  const parts = editor.split(/\s+/).filter(Boolean);
  const cmd = parts[0]!;
  const args = [...parts.slice(1), path];
  const result = spawnSync(cmd, args, { stdio: "inherit" });
  if (result.error) {
    fail(`failed to launch editor (${editor}): ${result.error.message}`, 1);
  }
  if (typeof result.status === "number" && result.status !== 0) {
    process.exit(result.status);
  }
}

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
    fallbackToEntry: true,
    subCommands: {
      "edit-config": editConfigCmd,
    },
  });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}
