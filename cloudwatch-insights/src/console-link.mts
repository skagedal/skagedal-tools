/**
 * Build shareable AWS CloudWatch Logs Insights console URLs.
 *
 * The AWS Console encodes complex values into the URL fragment using a
 * variant of Rison. There is no first-party documentation for this format
 * (the closest official reference is AWS's general "Creating a URL to the
 * AWS Management Console" page); the rules below were reverse-engineered
 * from links produced by the Console.
 *
 * Encoding rules:
 *   - Objects:  (key1~value1~key2~value2~...)
 *   - Arrays:   (~value1~value2~...)
 *   - Strings:  '<encoded>           — opening quote, no closing quote;
 *               the next ~ ) terminates the value.
 *   - Numbers:  bare decimal, no quoting.
 *   - In strings, every byte outside [A-Za-z0-9_.-] is encoded as *XX,
 *     where XX is the lower-case hex of the UTF-8 byte.
 *   - The fragment itself uses $XX (not %XX) to encode `?` and `=`, so the
 *     hash router can find its inner query string.
 *
 * The unit test reproduces a real Console-generated URL byte-for-byte.
 */
export type RisonValue =
  | string
  | number
  | RisonValue[]
  | { [key: string]: RisonValue };

const SAFE_BYTE_RE = /[A-Za-z0-9_.\-]/;

/**
 * Encode `s` for use as a string value inside the AWS Console's Rison-like
 * fragment format. Every UTF-8 byte that is not [A-Za-z0-9_.-] is replaced
 * with `*` followed by two lower-case hex digits.
 */
export function encodeAwsString(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let out = "";
  for (const b of bytes) {
    const ch = String.fromCharCode(b);
    if (b < 0x80 && SAFE_BYTE_RE.test(ch)) {
      out += ch;
    } else {
      out += "*" + b.toString(16).padStart(2, "0");
    }
  }
  return out;
}

/**
 * Encode a JSON-like value into AWS's Rison variant (see file header).
 * Object key order is preserved (insertion order, per JS semantics).
 */
export function encodeRison(value: RisonValue): string {
  if (typeof value === "string") {
    return "'" + encodeAwsString(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error(`cannot encode non-finite number: ${value}`);
    }
    return String(value);
  }
  if (Array.isArray(value)) {
    return "(~" + value.map(encodeRison).join("~") + ")";
  }
  if (typeof value === "object" && value !== null) {
    const parts = Object.entries(value).map(
      ([k, v]) => `${encodeAwsString(k)}~${encodeRison(v)}`,
    );
    return "(" + parts.join("~") + ")";
  }
  throw new Error(`cannot encode value of type ${typeof value}`);
}

export interface ConsoleLinkInput {
  region: string;
  /** The full queryDetail object as the Console expects it. */
  queryDetail: Record<string, RisonValue>;
}

/**
 * Build the final shareable URL. The AWS Console hash router parses
 * `logsV2:logs-insights?queryDetail=<value>` from the fragment, with `?`
 * and `=` $-encoded.
 */
export function buildConsoleLink(input: ConsoleLinkInput): string {
  const inner = encodeRison(input.queryDetail);
  // The leading `~` after `$3D` is part of the AWS encoding — top-level
  // values in this format are introduced by `~`, mirroring how key/value
  // pairs are separated inside objects.
  return (
    `https://${input.region}.console.aws.amazon.com/cloudwatch/home` +
    `?region=${input.region}` +
    `#logsV2:logs-insights$3FqueryDetail$3D~${inner}`
  );
}

/** Time-range portion of queryDetail. Either ABSOLUTE epoch-ms or
 * RELATIVE seconds before "now". */
export type TimeSpec =
  | { kind: "absolute"; startMs: number; endMs: number }
  | { kind: "relative"; secondsBack: number };

export interface QueryDetailInput {
  query: string;
  /** Full ARNs, e.g. arn:aws:logs:eu-north-1:123:log-group:/foo/bar. */
  logGroupArns: string[];
  time: TimeSpec;
  /** Optional saved-query UUID; AWS Console will assign one if omitted. */
  queryId?: string;
  logClass?: string;
}

/**
 * Build the queryDetail object the Console expects, in the field order
 * used by Console-generated URLs.
 */
export function buildQueryDetail(input: QueryDetailInput): Record<string, RisonValue> {
  const time: Record<string, RisonValue> =
    input.time.kind === "relative"
      ? {
          end: 0,
          start: -input.time.secondsBack,
          timeType: "RELATIVE",
          tz: "UTC",
          unit: "seconds",
        }
      : {
          end: input.time.endMs,
          start: input.time.startMs,
          timeType: "ABSOLUTE",
          tz: "UTC",
          unit: "milliseconds",
        };

  const detail: Record<string, RisonValue> = {
    ...time,
    editorString: input.query,
  };
  if (input.queryId) {
    detail.queryId = input.queryId;
  }
  detail.source = input.logGroupArns;
  detail.lang = "CWLI";
  detail.logClass = input.logClass ?? "STANDARD";
  detail.queryBy = "logGroupName";
  return detail;
}

/** Build the standard CloudWatch Logs ARN for a log group. */
export function logGroupArn(opts: {
  region: string;
  accountId: string;
  logGroupName: string;
}): string {
  return `arn:aws:logs:${opts.region}:${opts.accountId}:log-group:${opts.logGroupName}`;
}
