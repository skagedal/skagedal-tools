import { spawn } from "child_process";
import { createReadStream } from "fs";
import { Readable } from "stream";
import { createInterface } from "readline";
import { parseLine, type LogEntry } from "./entry.mjs";

export type SourceSpec =
  | { kind: "file"; path: string }
  | { kind: "command"; argv: string[] }
  | { kind: "stdin" };

export interface SourceHandle {
  /** Called once per parsed entry. */
  onEntry(listener: (entry: LogEntry) => void): void;
  /** Called when the source is exhausted (or fails). */
  onEnd(listener: (err?: Error) => void): void;
  /** Stop reading and free resources. Idempotent. */
  close(): void;
}

interface StartOptions {
  defaultField: string;
}

export function startSource(spec: SourceSpec, opts: StartOptions): SourceHandle {
  const entryListeners: Array<(entry: LogEntry) => void> = [];
  const endListeners: Array<(err?: Error) => void> = [];
  let nextId = 0;
  let closed = false;
  let cleanup: (() => void) | null = null;

  const emit = (entry: LogEntry) => {
    for (const fn of entryListeners) fn(entry);
  };
  const finish = (err?: Error) => {
    if (closed) return;
    closed = true;
    for (const fn of endListeners) fn(err);
    if (cleanup) cleanup();
  };

  let stream: Readable;
  if (spec.kind === "file") {
    stream = createReadStream(spec.path, { encoding: "utf8" });
  } else if (spec.kind === "stdin") {
    stream = process.stdin;
  } else {
    if (spec.argv.length === 0) {
      throw new Error("command source requires a non-empty argv");
    }
    const child = spawn(spec.argv[0]!, spec.argv.slice(1), {
      stdio: ["ignore", "pipe", "inherit"],
    });
    stream = child.stdout;
    cleanup = () => {
      if (!child.killed) child.kill();
    };
    child.on("error", (err) => finish(err));
    child.on("exit", () => finish());
  }

  const rl = createInterface({ input: stream, crlfDelay: Infinity });
  rl.on("line", (line) => {
    if (closed) return;
    if (line === "") return;
    emit(parseLine(line, nextId++, opts.defaultField));
  });
  rl.on("close", () => finish());
  stream.on("error", (err) => finish(err));

  return {
    onEntry(listener) {
      entryListeners.push(listener);
    },
    onEnd(listener) {
      endListeners.push(listener);
    },
    close() {
      rl.close();
      if (stream !== process.stdin) {
        stream.destroy();
      }
      finish();
    },
  };
}
