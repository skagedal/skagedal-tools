// CLI argument parsing with yargs
// https://yargs.js.org/
//
// Run: pnpm yargs -- greet Alice --times 3 --shout
//      pnpm yargs -- farewell Bob --formal

import yargs from "yargs";
import { hideBin } from "yargs/helpers";

// pnpm run inserts '--' before extra arguments; strip it so parsers see clean argv
if (process.argv[2] === "--") process.argv.splice(2, 1);

yargs(hideBin(process.argv))
  .scriptName("greet-demo")
  .version("1.0.0")
  .command(
    "greet <name>",
    "Greet someone",
    (yargs) =>
      yargs
        .positional("name", {
          describe: "the name to greet",
          type: "string",
          demandOption: true,
        })
        .option("times", {
          alias: "t",
          describe: "how many times to repeat the greeting",
          type: "number",
          default: 1,
        })
        .option("shout", {
          alias: "s",
          describe: "shout the greeting in uppercase",
          type: "boolean",
          default: false,
        }),
    (argv) => {
      const greeting = `Hello, ${argv.name}!`;
      const output = argv.shout ? greeting.toUpperCase() : greeting;
      for (let i = 0; i < argv.times; i++) {
        console.log(output);
      }
    }
  )
  .command(
    "farewell <name>",
    "Say farewell to someone",
    (yargs) =>
      yargs
        .positional("name", {
          describe: "the name to bid farewell",
          type: "string",
          demandOption: true,
        })
        .option("formal", {
          alias: "f",
          describe: "use formal language",
          type: "boolean",
          default: false,
        }),
    (argv) => {
      const message = argv.formal
        ? `Farewell, ${argv.name}. It was a pleasure.`
        : `Bye, ${argv.name}!`;
      console.log(message);
    }
  )
  .demandCommand(1, "Please specify a command")
  .strict()
  .parse();
