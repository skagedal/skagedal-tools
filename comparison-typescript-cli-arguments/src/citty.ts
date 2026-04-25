// CLI argument parsing with citty
// https://github.com/unjs/citty
//
// Run: pnpm citty -- greet Alice --times 3 --shout
//      pnpm citty -- farewell Bob --formal

import { defineCommand, runMain } from "citty";

// pnpm run inserts '--' before extra arguments; strip it so parsers see clean argv
if (process.argv[2] === "--") process.argv.splice(2, 1);

const greet = defineCommand({
  meta: {
    name: "greet",
    description: "Greet someone",
  },
  args: {
    name: {
      type: "positional",
      description: "the name to greet",
      required: true,
    },
    times: {
      type: "string",
      alias: "t",
      description: "how many times to repeat the greeting",
      default: "1",
    },
    shout: {
      type: "boolean",
      alias: "s",
      description: "shout the greeting in uppercase",
    },
  },
  run({ args }) {
    const greeting = `Hello, ${args.name}!`;
    const output = args.shout ? greeting.toUpperCase() : greeting;
    const times = parseInt(args.times, 10);
    for (let i = 0; i < times; i++) {
      console.log(output);
    }
  },
});

const farewell = defineCommand({
  meta: {
    name: "farewell",
    description: "Say farewell to someone",
  },
  args: {
    name: {
      type: "positional",
      description: "the name to bid farewell",
      required: true,
    },
    formal: {
      type: "boolean",
      alias: "f",
      description: "use formal language",
    },
  },
  run({ args }) {
    const message = args.formal
      ? `Farewell, ${args.name}. It was a pleasure.`
      : `Bye, ${args.name}!`;
    console.log(message);
  },
});

const main = defineCommand({
  meta: {
    name: "greet-demo",
    version: "1.0.0",
    description: "A demo CLI for comparing argument parsing libraries",
  },
  subCommands: {
    greet,
    farewell,
  },
});

runMain(main);
