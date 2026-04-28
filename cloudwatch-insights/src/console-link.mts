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

/**
 * Inverse of {@link encodeAwsString}. Replaces `*XX` (hex byte) escapes with
 * the corresponding bytes, then UTF-8 decodes the result. Plain ASCII bytes
 * are kept as-is.
 */
export function decodeAwsString(encoded: string): string {
  const bytes: number[] = [];
  for (let i = 0; i < encoded.length; i++) {
    const c = encoded[i];
    if (c === "*") {
      const hex = encoded.slice(i + 1, i + 3);
      if (!/^[0-9a-fA-F]{2}$/.test(hex)) {
        throw new Error(`invalid *XX escape at position ${i}: *${hex}`);
      }
      bytes.push(parseInt(hex, 16));
      i += 2;
    } else {
      bytes.push(c.charCodeAt(0));
    }
  }
  return new TextDecoder("utf-8").decode(new Uint8Array(bytes));
}

/**
 * Inverse of {@link encodeRison}. Parses AWS's Rison variant into a
 * RisonValue tree. Throws on malformed input.
 */
export function decodeRison(input: string): RisonValue {
  const parser = new RisonParser(input);
  const value = parser.parseValue();
  if (parser.pos !== input.length) {
    throw new Error(
      `unexpected trailing characters at position ${parser.pos}: ${input.slice(parser.pos)}`,
    );
  }
  return value;
}

class RisonParser {
  pos = 0;
  constructor(public src: string) {}

  parseValue(): RisonValue {
    const c = this.src[this.pos];
    if (c === "(") {
      this.pos++;
      if (this.src[this.pos] === "~") {
        return this.parseArrayBody();
      }
      return this.parseObjectBody();
    }
    if (c === "'") {
      this.pos++;
      return decodeAwsString(this.readUntilDelimiter());
    }
    return this.parseNumber();
  }

  parseArrayBody(): RisonValue[] {
    const out: RisonValue[] = [];
    while (this.src[this.pos] === "~") {
      this.pos++;
      if (this.src[this.pos] === ")") break;
      out.push(this.parseValue());
    }
    if (this.src[this.pos] !== ")") {
      throw new Error(`expected ')' at position ${this.pos}`);
    }
    this.pos++;
    return out;
  }

  parseObjectBody(): { [k: string]: RisonValue } {
    const out: { [k: string]: RisonValue } = {};
    if (this.src[this.pos] === ")") {
      this.pos++;
      return out;
    }
    while (true) {
      const key = decodeAwsString(this.readUntilDelimiter());
      if (this.src[this.pos] !== "~") {
        throw new Error(`expected '~' after key '${key}' at position ${this.pos}`);
      }
      this.pos++;
      out[key] = this.parseValue();
      const c = this.src[this.pos];
      if (c === "~") {
        this.pos++;
        continue;
      }
      if (c === ")") {
        this.pos++;
        return out;
      }
      throw new Error(`expected '~' or ')' at position ${this.pos}`);
    }
  }

  readUntilDelimiter(): string {
    const start = this.pos;
    while (this.pos < this.src.length) {
      const c = this.src[this.pos];
      if (c === "~" || c === ")") break;
      this.pos++;
    }
    return this.src.slice(start, this.pos);
  }

  parseNumber(): number {
    const raw = this.readUntilDelimiter();
    const n = Number(raw);
    if (raw.length === 0 || !Number.isFinite(n)) {
      throw new Error(`invalid number '${raw}' at position ${this.pos - raw.length}`);
    }
    return n;
  }
}

/**
 * Parse a log-group ARN of the form
 * `arn:aws:logs:<region>:<account>:log-group:<name>` (with an optional
 * trailing `:*` selector). Returns null if the input doesn't match.
 */
export function parseLogGroupArn(arn: string): {
  region: string;
  accountId: string;
  logGroupName: string;
} | null {
  const m = /^arn:aws:logs:([^:]+):([^:]+):log-group:([^:]+)(?::\*)?$/.exec(arn);
  if (!m) return null;
  return { region: m[1], accountId: m[2], logGroupName: m[3] };
}

export interface ParsedConsoleLink {
  region: string;
  queryDetail: Record<string, RisonValue>;
}

/**
 * Inverse of {@link buildConsoleLink}. Extracts the region from the URL and
 * decodes the queryDetail object from the fragment. Accepts URLs whose
 * fragment uses either AWS's `$3F`/`$3D` encoding or standard `%3F`/`%3D`.
 */
export function parseConsoleLink(url: string): ParsedConsoleLink {
  const trimmed = url.trim();
  const hashIdx = trimmed.indexOf("#");
  if (hashIdx < 0) throw new Error("URL has no fragment (no '#')");
  const queryStr = trimmed.slice(0, hashIdx);
  const fragment = trimmed.slice(hashIdx + 1);

  const regionMatch = /[?&]region=([^&]+)/.exec(queryStr);
  if (!regionMatch) throw new Error("URL is missing the ?region=... query parameter");
  const region = decodeURIComponent(regionMatch[1]);

  // Browsers often percent-encode parts of the fragment when copying the
  // URL from the address bar (e.g. `'` → `%27`). Decode standard
  // percent-escapes first; the AWS-specific `*XX` escapes use `*`, not `%`,
  // so they're untouched. Then turn `$3F` / `$3D` into `?` / `=` so we can
  // locate `queryDetail=...` reliably.
  let normalized: string;
  try {
    normalized = decodeURIComponent(fragment);
  } catch {
    normalized = fragment;
  }
  normalized = normalized.replace(/\$3F/g, "?").replace(/\$3D/g, "=");
  const m = /(?:^|[?&])queryDetail=~?(.+)$/.exec(normalized);
  if (!m) throw new Error("fragment is missing queryDetail=...");
  const detail = decodeRison(m[1]);
  if (!isPlainObject(detail)) {
    throw new Error("queryDetail is not a Rison object");
  }
  return { region, queryDetail: detail };
}

function isPlainObject(v: RisonValue): v is { [k: string]: RisonValue } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}
