import { test } from "node:test";
import assert from "node:assert/strict";

import {
  buildConsoleLink,
  buildQueryDetail,
  decodeAwsString,
  decodeRison,
  encodeAwsString,
  encodeRison,
  logGroupArn,
  parseConsoleLink,
  parseLogGroupArn,
  type RisonValue,
} from "../src/console-link.mjs";

test("encodeAwsString leaves [A-Za-z0-9_.-] alone", () => {
  assert.equal(encodeAwsString("abcXYZ_0123.-"), "abcXYZ_0123.-");
});

test("encodeAwsString hex-escapes other ASCII bytes", () => {
  assert.equal(encodeAwsString(" "), "*20");
  assert.equal(encodeAwsString("@"), "*40");
  assert.equal(encodeAwsString(","), "*2c");
  assert.equal(encodeAwsString("\n"), "*0a");
  assert.equal(encodeAwsString("|"), "*7c");
  assert.equal(encodeAwsString("="), "*3d");
  assert.equal(encodeAwsString("'"), "*27");
  assert.equal(encodeAwsString(":"), "*3a");
  assert.equal(encodeAwsString("/"), "*2f");
  // Rison structural characters must also be escaped if they appear inside
  // a string value, otherwise they would terminate the string.
  assert.equal(encodeAwsString("~"), "*7e");
  assert.equal(encodeAwsString("("), "*28");
  assert.equal(encodeAwsString(")"), "*29");
  assert.equal(encodeAwsString("*"), "*2a");
});

test("encodeAwsString UTF-8 encodes non-ASCII per byte", () => {
  // "é" is U+00E9, two UTF-8 bytes 0xc3 0xa9.
  assert.equal(encodeAwsString("é"), "*c3*a9");
  // "✓" is U+2713, three UTF-8 bytes 0xe2 0x9c 0x93.
  assert.equal(encodeAwsString("✓"), "*e2*9c*93");
});

test("encodeRison emits AWS-flavored objects, arrays, strings, numbers", () => {
  assert.equal(encodeRison(0), "0");
  assert.equal(encodeRison(-3600), "-3600");
  assert.equal(encodeRison("UTC"), "'UTC");
  assert.equal(encodeRison(["a", "b"]), "(~'a~'b)");
  assert.equal(
    encodeRison({ end: 0, start: -3600, tz: "UTC" }),
    "(end~0~start~-3600~tz~'UTC)",
  );
});

test("encodeRison preserves object key insertion order", () => {
  const v: RisonValue = { z: 1, a: 2, m: 3 };
  assert.equal(encodeRison(v), "(z~1~a~2~m~3)");
});

test("logGroupArn builds the standard logs ARN", () => {
  assert.equal(
    logGroupArn({
      region: "eu-north-1",
      accountId: "361629632765",
      logGroupName: "/eks/uat/team-icc",
    }),
    "arn:aws:logs:eu-north-1:361629632765:log-group:/eks/uat/team-icc",
  );
});

test("buildConsoleLink reproduces a Console-generated URL byte-for-byte", () => {
  // Captured directly from the AWS Console "Share" button.
  const expected =
    "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1" +
    "#logsV2:logs-insights$3FqueryDetail$3D~(end~0~start~-3600~timeType~'RELATIVE" +
    "~tz~'UTC~unit~'seconds~editorString~'fields*20*40timestamp*2c*20*40message" +
    "*0a*7c*20sort*20*40timestamp*20desc*0a*7c*20filter*20app*20*3d*20" +
    "*27installer-notification*27*0a*7c*20limit*20200" +
    "~queryId~'dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb" +
    "~source~(~'arn*3aaws*3alogs*3aeu-north-1*3a361629632765*3alog-group" +
    "*3a*2feks*2fuat*2fteam-icc)" +
    "~lang~'CWLI~logClass~'STANDARD~queryBy~'logGroupName)";

  const queryDetail: Record<string, RisonValue> = {
    end: 0,
    start: -3600,
    timeType: "RELATIVE",
    tz: "UTC",
    unit: "seconds",
    editorString:
      "fields @timestamp, @message\n" +
      "| sort @timestamp desc\n" +
      "| filter app = 'installer-notification'\n" +
      "| limit 200",
    queryId: "dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
    source: ["arn:aws:logs:eu-north-1:361629632765:log-group:/eks/uat/team-icc"],
    lang: "CWLI",
    logClass: "STANDARD",
    queryBy: "logGroupName",
  };

  assert.equal(buildConsoleLink({ region: "eu-north-1", queryDetail }), expected);
});

test("buildQueryDetail emits RELATIVE form with seconds", () => {
  const detail = buildQueryDetail({
    query: "fields @timestamp",
    logGroupArns: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
    time: { kind: "relative", secondsBack: 3600 },
  });
  assert.deepEqual(detail, {
    end: 0,
    start: -3600,
    timeType: "RELATIVE",
    tz: "UTC",
    unit: "seconds",
    editorString: "fields @timestamp",
    source: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
    lang: "CWLI",
    logClass: "STANDARD",
    queryBy: "logGroupName",
  });
});

test("buildQueryDetail emits ABSOLUTE form with epoch-ms", () => {
  const detail = buildQueryDetail({
    query: "fields @timestamp",
    logGroupArns: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
    time: { kind: "absolute", startMs: 1700000000000, endMs: 1700003600000 },
  });
  assert.equal(detail.start, 1700000000000);
  assert.equal(detail.end, 1700003600000);
  assert.equal(detail.timeType, "ABSOLUTE");
  assert.equal(detail.unit, "milliseconds");
});

test("buildQueryDetail omits queryId when not supplied", () => {
  const detail = buildQueryDetail({
    query: "fields @timestamp",
    logGroupArns: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
    time: { kind: "relative", secondsBack: 60 },
  });
  assert.equal("queryId" in detail, false);
});

test("buildQueryDetail places source after editorString (matches Console field order)", () => {
  const detail = buildQueryDetail({
    query: "q",
    logGroupArns: ["arn:aws:logs:eu-north-1:1:log-group:/a"],
    time: { kind: "relative", secondsBack: 60 },
  });
  const keys = Object.keys(detail);
  assert.deepEqual(keys, [
    "end",
    "start",
    "timeType",
    "tz",
    "unit",
    "editorString",
    "source",
    "lang",
    "logClass",
    "queryBy",
  ]);
});

test("encodeRison round-trips arrays of strings with special chars", () => {
  assert.equal(
    encodeRison(["a:b", "c/d"]),
    "(~'a*3ab~'c*2fd)",
  );
});

test("decodeAwsString: inverse of encodeAwsString", () => {
  for (const s of [
    "abc",
    " ",
    "@",
    "@timestamp",
    "fields @timestamp, @message\n| sort @timestamp desc",
    "é",
    "✓",
    "*~()",
  ]) {
    assert.equal(decodeAwsString(encodeAwsString(s)), s, `round-trip: ${JSON.stringify(s)}`);
  }
});

test("decodeAwsString: rejects malformed *XX escapes", () => {
  assert.throws(() => decodeAwsString("*"), /invalid \*XX/);
  assert.throws(() => decodeAwsString("*g0"), /invalid \*XX/);
});

test("decodeRison: inverse of encodeRison for primitives, arrays, objects", () => {
  const cases: RisonValue[] = [
    0,
    -3600,
    "UTC",
    "fields @timestamp, @message\n| limit 200",
    [],
    ["a", "b"],
    { end: 0, start: -3600, tz: "UTC" },
    {
      end: 0,
      start: -3600,
      timeType: "RELATIVE",
      tz: "UTC",
      unit: "seconds",
      editorString: "fields @timestamp",
      source: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
      lang: "CWLI",
    },
  ];
  for (const v of cases) {
    assert.deepEqual(decodeRison(encodeRison(v)), v);
  }
});

test("decodeRison: round-trips the full Console-format example", () => {
  const original: Record<string, RisonValue> = {
    end: 0,
    start: -3600,
    timeType: "RELATIVE",
    tz: "UTC",
    unit: "seconds",
    editorString:
      "fields @timestamp, @message\n" +
      "| sort @timestamp desc\n" +
      "| filter app = 'installer-notification'\n" +
      "| limit 200",
    queryId: "dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
    source: ["arn:aws:logs:eu-north-1:361629632765:log-group:/eks/uat/team-icc"],
    lang: "CWLI",
    logClass: "STANDARD",
    queryBy: "logGroupName",
  };
  assert.deepEqual(decodeRison(encodeRison(original)), original);
});

test("decodeRison: parses ABSOLUTE timeType with epoch-ms numbers", () => {
  const encoded = encodeRison({
    end: 1700003600000,
    start: 1700000000000,
    timeType: "ABSOLUTE",
    unit: "milliseconds",
  });
  assert.deepEqual(decodeRison(encoded), {
    end: 1700003600000,
    start: 1700000000000,
    timeType: "ABSOLUTE",
    unit: "milliseconds",
  });
});

test("decodeRison: rejects trailing garbage", () => {
  assert.throws(() => decodeRison("(a~1)x"), /trailing/);
});

test("parseLogGroupArn: extracts region/account/name", () => {
  assert.deepEqual(
    parseLogGroupArn("arn:aws:logs:eu-north-1:361629632765:log-group:/eks/uat/team-icc"),
    {
      region: "eu-north-1",
      accountId: "361629632765",
      logGroupName: "/eks/uat/team-icc",
    },
  );
  assert.equal(parseLogGroupArn("not-an-arn"), null);
  assert.equal(
    parseLogGroupArn("arn:aws:s3:::my-bucket"),
    null,
    "non-logs ARN rejected",
  );
});

test("parseLogGroupArn: tolerates trailing :* selector", () => {
  assert.deepEqual(
    parseLogGroupArn("arn:aws:logs:eu-north-1:1:log-group:/x:*"),
    { region: "eu-north-1", accountId: "1", logGroupName: "/x" },
  );
});

test("parseConsoleLink: round-trips a Console-generated URL", () => {
  const queryDetail: Record<string, RisonValue> = {
    end: 0,
    start: -3600,
    timeType: "RELATIVE",
    tz: "UTC",
    unit: "seconds",
    editorString:
      "fields @timestamp, @message\n" +
      "| sort @timestamp desc\n" +
      "| filter app = 'installer-notification'\n" +
      "| limit 200",
    queryId: "dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
    source: ["arn:aws:logs:eu-north-1:361629632765:log-group:/eks/uat/team-icc"],
    lang: "CWLI",
    logClass: "STANDARD",
    queryBy: "logGroupName",
  };
  const url = buildConsoleLink({ region: "eu-north-1", queryDetail });
  const parsed = parseConsoleLink(url);
  assert.equal(parsed.region, "eu-north-1");
  assert.deepEqual(parsed.queryDetail, queryDetail);
});

test("parseConsoleLink: tolerates percent-encoded $3F/$3D", () => {
  const queryDetail: Record<string, RisonValue> = {
    end: 0,
    start: -60,
    timeType: "RELATIVE",
    tz: "UTC",
    unit: "seconds",
    editorString: "fields @timestamp",
    source: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
    lang: "CWLI",
    logClass: "STANDARD",
    queryBy: "logGroupName",
  };
  const original = buildConsoleLink({ region: "eu-north-1", queryDetail });
  const percentEncoded = original
    .replace("$3F", "%3F")
    .replace("$3D", "%3D");
  const parsed = parseConsoleLink(percentEncoded);
  assert.equal(parsed.region, "eu-north-1");
  assert.deepEqual(parsed.queryDetail, queryDetail);
});

test("parseConsoleLink: rejects URLs without queryDetail", () => {
  assert.throws(
    () => parseConsoleLink("https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1#logsV2:logs-insights"),
    /queryDetail/,
  );
});
