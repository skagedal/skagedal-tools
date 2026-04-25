import { test } from "node:test";
import { strict as assert } from "node:assert";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// Smoke-test every parser implementation by spawning it with the same args
// and asserting identical stdout. The point of this comparison repo is that
// the five entry points behave the same way for the canonical examples — if
// one drifts (e.g. an upstream library changes default coercion), this test
// catches it.

const __dirname = dirname(fileURLToPath(import.meta.url));
const SRC = join(__dirname, "..", "src");

// meow and gunshi are .mts because they require ESM (import.meta and
// top-level await respectively) and the repo doesn't commit a package.json
// declaring "type": "module".
const IMPLEMENTATIONS = [
  "commander.ts",
  "yargs.ts",
  "meow.mts",
  "citty.ts",
  "gunshi.mts",
] as const;

function run(impl: string, args: string[]): { stdout: string; status: number | null } {
  const tsx = join(__dirname, "..", "node_modules", ".bin", "tsx");
  const result = spawnSync(tsx, [join(SRC, impl), ...args], {
    encoding: "utf8",
    env: process.env,
  });
  return { stdout: result.stdout, status: result.status };
}

const CASES: { name: string; args: string[]; expected: string }[] = [
  {
    name: "greet with --times 3 and --shout uppercases and repeats",
    args: ["greet", "Alice", "--times", "3", "--shout"],
    expected: "HELLO, ALICE!\nHELLO, ALICE!\nHELLO, ALICE!\n",
  },
  {
    name: "greet with default options prints once, no shouting",
    args: ["greet", "Alice"],
    expected: "Hello, Alice!\n",
  },
  {
    name: "farewell --formal uses the formal phrasing",
    args: ["farewell", "Bob", "--formal"],
    expected: "Farewell, Bob. It was a pleasure.\n",
  },
  {
    name: "farewell without --formal uses the casual phrasing",
    args: ["farewell", "Bob"],
    expected: "Bye, Bob!\n",
  },
];

for (const impl of IMPLEMENTATIONS) {
  for (const c of CASES) {
    test(`${impl}: ${c.name}`, () => {
      const { stdout, status } = run(impl, c.args);
      assert.equal(status, 0, `non-zero exit; stdout=${stdout}`);
      assert.equal(stdout, c.expected);
    });
  }
}
