// CLI argument parsing with meow
// https://github.com/sindresorhus/meow
//
// Run: pnpm meow -- greet Alice --times 3 --shout
//      pnpm meow -- farewell Bob --formal
//
// Note: meow is designed for single-command CLIs. Subcommands are handled
// manually by reading the first positional argument and dispatching.

import meow from "meow";

// pnpm run inserts '--' before extra arguments; strip it so parsers see clean argv
if (process.argv[2] === "--") process.argv.splice(2, 1);

const cli = meow(
  `
  Usage
    $ greet-demo <command> [options]

  Commands
    greet <name>     Greet someone
    farewell <name>  Say farewell to someone

  Options (greet)
    --times, -t  How many times to repeat the greeting  [default: 1]
    --shout, -s  Shout the greeting in uppercase

  Options (farewell)
    --formal, -f  Use formal language

  Examples
    $ greet-demo greet Alice --times 3 --shout
    $ greet-demo farewell Bob --formal
`,
  {
    importMeta: import.meta,
    flags: {
      times: { type: "number", shortFlag: "t", default: 1 },
      shout: { type: "boolean", shortFlag: "s", default: false },
      formal: { type: "boolean", shortFlag: "f", default: false },
    },
  }
);

const [command, name] = cli.input;

if (command === "greet") {
  if (!name) {
    console.error("Error: <name> is required for the greet command");
    process.exit(1);
  }
  const greeting = `Hello, ${name}!`;
  const output = cli.flags.shout ? greeting.toUpperCase() : greeting;
  for (let i = 0; i < cli.flags.times; i++) {
    console.log(output);
  }
} else if (command === "farewell") {
  if (!name) {
    console.error("Error: <name> is required for the farewell command");
    process.exit(1);
  }
  const message = cli.flags.formal
    ? `Farewell, ${name}. It was a pleasure.`
    : `Bye, ${name}!`;
  console.log(message);
} else {
  cli.showHelp();
  process.exit(command ? 1 : 0);
}
