// CLI argument parsing with gunshi
// https://github.com/kazupon/gunshi
//
// Run: pnpm run gunshi -- greet Alice --times 3 --shout
//      pnpm run gunshi -- farewell Bob --formal

import { cli, define } from "gunshi";

// pnpm run inserts '--' before extra arguments; strip it so parsers see clean argv
if (process.argv[2] === "--") process.argv.splice(2, 1);

const greet = define({
  name: "greet",
  description: "Greet someone",
  args: {
    name: {
      type: "positional",
      description: "the name to greet",
    },
    times: {
      type: "number",
      short: "t",
      description: "how many times to repeat the greeting",
      default: 1,
    },
    shout: {
      type: "boolean",
      short: "s",
      description: "shout the greeting in uppercase",
    },
  },
  run: (ctx) => {
    const greeting = `Hello, ${ctx.values.name}!`;
    const output = ctx.values.shout ? greeting.toUpperCase() : greeting;
    for (let i = 0; i < (ctx.values.times ?? 1); i++) {
      console.log(output);
    }
  },
});

const farewell = define({
  name: "farewell",
  description: "Say farewell to someone",
  args: {
    name: {
      type: "positional",
      description: "the name to bid farewell",
    },
    formal: {
      type: "boolean",
      short: "f",
      description: "use formal language",
    },
  },
  run: (ctx) => {
    const message = ctx.values.formal
      ? `Farewell, ${ctx.values.name}. It was a pleasure.`
      : `Bye, ${ctx.values.name}!`;
    console.log(message);
  },
});

const main = define({
  name: "greet-demo",
  description: "A demo CLI for comparing argument parsing libraries",
  run: () => {
    console.log("Please specify a subcommand: greet, farewell");
  },
});

await cli(process.argv.slice(2), main, {
  name: "greet-demo",
  version: "1.0.0",
  description: "A demo CLI for comparing argument parsing libraries",
  subCommands: {
    greet,
    farewell,
  },
});
