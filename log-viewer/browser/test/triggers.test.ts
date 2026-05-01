import { test } from "node:test";
import { strict as assert } from "node:assert";
import { createTriggerRuntime } from "../src/triggers.ts";
import { parseLine } from "../src/entry.ts";

test("trigger fires once per new value, not on repeats", () => {
  let clock = 0;
  const fires: { value: string; command: string }[] = [];
  const rt = createTriggerRuntime(
    [
      {
        name: "pod",
        onNewValue: ["pod"],
        action: "echo {value}",
        startupDelayMs: 0,
      },
    ],
    {
      onFire: (info) => fires.push({ value: info.value, command: info.command }),
      now: () => clock,
    }
  );
  rt.start();
  clock = 100;

  rt.handle(parseLine(`{"pod":"a"}`, 0, "message"));
  rt.handle(parseLine(`{"pod":"a"}`, 1, "message"));
  rt.handle(parseLine(`{"pod":"b"}`, 2, "message"));

  assert.equal(fires.length, 2);
  assert.equal(fires[0].value, "a");
  assert.equal(fires[1].value, "b");
});

test("startup delay swallows values seen during the delay window", () => {
  let clock = 0;
  const fires: string[] = [];
  const rt = createTriggerRuntime(
    [
      {
        name: "pod",
        onNewValue: ["pod"],
        action: "echo {value}",
        startupDelayMs: 1000,
      },
    ],
    { onFire: (info) => fires.push(info.value), now: () => clock }
  );
  rt.start();

  clock = 500;
  rt.handle(parseLine(`{"pod":"early"}`, 0, "message"));

  clock = 1500;
  rt.handle(parseLine(`{"pod":"early"}`, 1, "message"));
  rt.handle(parseLine(`{"pod":"late"}`, 2, "message"));

  assert.deepEqual(fires, ["late"]);
});

test("trigger picks first non-empty candidate field", () => {
  const fires: string[] = [];
  const rt = createTriggerRuntime(
    [
      {
        name: "host",
        onNewValue: ["hostname", "host"],
        action: "echo {value}",
        startupDelayMs: 0,
      },
    ],
    { onFire: (info) => fires.push(info.value), now: () => 0 }
  );
  rt.start();

  rt.handle(parseLine(`{"host":"h1"}`, 0, "message"));
  rt.handle(parseLine(`{"hostname":"","host":"h1"}`, 1, "message"));
  rt.handle(parseLine(`{"hostname":"h2"}`, 2, "message"));

  assert.deepEqual(fires, ["h1", "h2"]);
});

test("substitutions are shell-quoted for safety", () => {
  let cmd = "";
  const rt = createTriggerRuntime(
    [
      {
        name: "x",
        onNewValue: ["v"],
        action: "echo {value}",
        startupDelayMs: 0,
      },
    ],
    { onFire: (info) => (cmd = info.command), now: () => 0 }
  );
  rt.start();
  rt.handle(parseLine(`{"v":"it's mine; rm -rf /"}`, 0, "message"));

  assert.equal(cmd, "echo 'it'\\''s mine; rm -rf /'");
});

test("trigger with no matching field is silently skipped", () => {
  const fires: string[] = [];
  const rt = createTriggerRuntime(
    [
      {
        name: "x",
        onNewValue: ["missing"],
        action: "echo {value}",
        startupDelayMs: 0,
      },
    ],
    { onFire: (info) => fires.push(info.value), now: () => 0 }
  );
  rt.start();
  rt.handle(parseLine(`{"other":"a"}`, 0, "message"));
  assert.deepEqual(fires, []);
});
