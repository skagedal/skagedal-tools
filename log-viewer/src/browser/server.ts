import { createServer as createViteServer } from "vite";
import { fileURLToPath } from "url";
import { dirname, join } from "path";
import { readFile } from "fs/promises";
import type { IncomingMessage, ServerResponse } from "http";
import type { Config } from "../config.js";
import type { LogEntry } from "../entry.js";
import type { SourceHandle } from "../source.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
// In dev (tsx) this resolves to <pkg>/src/browser, in build to <pkg>/dist/browser.
// `web/` ships next to dist/ in the published package, and next to src/ in dev.
const webRoot = join(__dirname, "..", "..", "web");

export interface RunBrowserOptions {
  config: Config;
  source: SourceHandle;
  sourceLabel: string;
  port: number;
  host: string;
  open?: boolean;
}

export async function runBrowser(opts: RunBrowserOptions): Promise<void> {
  const buffer: LogEntry[] = [];
  const subscribers = new Set<ServerResponse>();
  let ended = false;

  opts.source.onEntry((entry) => {
    buffer.push(entry);
    const payload = `event: entry\ndata: ${JSON.stringify(entry)}\n\n`;
    for (const res of subscribers) res.write(payload);
  });
  opts.source.onEnd(() => {
    ended = true;
    for (const res of subscribers) res.write("event: end\ndata: {}\n\n");
  });

  const vite = await createViteServer({
    root: webRoot,
    server: { middlewareMode: true, host: opts.host, port: opts.port },
    appType: "custom",
    clearScreen: false,
    logLevel: "warn",
  });

  const handler = async (req: IncomingMessage, res: ServerResponse) => {
    const url = req.url ?? "/";
    if (url === "/" || url.startsWith("/?") || url === "/index.html") {
      try {
        const raw = await readFile(join(webRoot, "index.html"), "utf8");
        const html = await vite.transformIndexHtml(url, raw);
        res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
        res.end(html);
      } catch (err) {
        res.writeHead(500);
        res.end(err instanceof Error ? err.message : String(err));
      }
      return;
    }
    if (url === "/api/meta") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ config: opts.config, sourceLabel: opts.sourceLabel }));
      return;
    }
    if (url === "/api/stream") {
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      });
      // Replay everything we've already buffered so late-joining clients catch up.
      for (const entry of buffer) {
        res.write(`event: entry\ndata: ${JSON.stringify(entry)}\n\n`);
      }
      if (ended) {
        res.write("event: end\ndata: {}\n\n");
      }
      subscribers.add(res);
      req.on("close", () => subscribers.delete(res));
      return;
    }
    vite.middlewares(req, res);
  };

  const http = await import("http");
  const server = http.createServer(handler);
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(opts.port, opts.host, () => resolve());
  });

  const addr = server.address();
  const port = typeof addr === "object" && addr ? addr.port : opts.port;
  const displayHost = opts.host === "0.0.0.0" || opts.host === "::" ? "localhost" : opts.host;
  const url = `http://${displayHost}:${port}/`;
  process.stdout.write(`log-viewer listening at ${url}\n`);
  process.stdout.write(`  source: ${opts.sourceLabel}\n`);
  process.stdout.write(`  press Ctrl-C to stop\n`);

  if (opts.open) {
    openBrowser(url).catch(() => {
      /* opening is best-effort */
    });
  }

  await new Promise<void>((resolve) => {
    const stop = () => {
      server.close();
      vite.close();
      opts.source.close();
      resolve();
    };
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
  });
}

async function openBrowser(url: string): Promise<void> {
  const { spawn } = await import("child_process");
  const cmd =
    process.platform === "darwin"
      ? ["open", url]
      : process.platform === "win32"
        ? ["cmd", "/c", "start", "", url]
        : ["xdg-open", url];
  const child = spawn(cmd[0]!, cmd.slice(1), { stdio: "ignore", detached: true });
  child.on("error", () => {
    /* ignore */
  });
  child.unref();
}
