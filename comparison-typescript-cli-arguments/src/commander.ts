// CLI argument parsing with commander
// https://github.com/tj/commander.js
//
// Run: pnpm commander -- greet Alice --times 3 --shout
//      pnpm commander -- farewell Bob --formal

import { Command } from "commander";

// pnpm run inserts '--' before extra arguments; strip it so parsers see clean argv
if (process.argv[2] === "--") process.argv.splice(2, 1);

const program = new Command();

program
  .name("greet-demo")
  .description("A demo CLI for comparing argument parsing libraries")
  .version("1.0.0");

program
  .command("greet <name>")
  .description("Greet someone")
  .option("-t, --times <number>", "how many times to repeat the greeting", "1")
  .option("-s, --shout", "shout the greeting in uppercase")
  .action((name: string, options: { times: string; shout?: boolean }) => {
    const greeting = `Hello, ${name}!`;
    const output = options.shout ? greeting.toUpperCase() : greeting;
    const times = parseInt(options.times, 10);
    for (let i = 0; i < times; i++) {
      console.log(output);
    }
  });

program
  .command("farewell <name>")
  .description("Say farewell to someone")
  .option("-f, --formal", "use formal language")
  .action((name: string, options: { formal?: boolean }) => {
    const message = options.formal
      ? `Farewell, ${name}. It was a pleasure.`
      : `Bye, ${name}!`;
    console.log(message);
  });

program.parse();
