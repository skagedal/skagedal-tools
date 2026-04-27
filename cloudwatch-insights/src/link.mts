/**
 * `cloudwatch-insights link` — generate a shareable AWS Console URL for the
 * query that `cloudwatch-insights query` would otherwise run.
 *
 * Resolution mirrors the query subcommand: --query / --query-file / the
 * repo's current.insights, with front-matter and config defaults filling
 * in time, environment, log groups. We never open the editor here — link
 * is a read-only view over what was last edited.
 */

import { existsSync, readFileSync } from "fs";

import { STSClient, GetCallerIdentityCommand } from "@aws-sdk/client-sts";

import {
  ConfigError,
  EnvConfig,
  RepoConfig,
  Settings,
  expandTemplate,
  isValidEnvironmentName,
  loadSettings,
  resolveEnvConfig,
  resolveRepoDefaults,
} from "./config.mjs";
import {
  buildConsoleLink,
  buildQueryDetail,
  logGroupArn,
  TimeSpec,
} from "./console-link.mjs";
import {
  parseTimeRange,
  TimeRangeParseError,
  tryParseRelativeDurationSeconds,
} from "./time-range.mjs";
import {
  FrontMatter,
  currentInsightsPath,
  loadQueryFile,
  parseQueryFile,
} from "./query-file.mjs";

export interface LinkValues {
  "log-group"?: string[];
  time?: string;
  query?: string;
  "query-file"?: string;
  environment?: string;
  region?: string;
  profile?: string;
  "preserve-time-window": boolean;
  "account-id"?: string;
  quiet: boolean;
}

export async function runLink(values: LinkValues): Promise<void> {
  if (values.query && values["query-file"]) {
    fail("--query and --query-file are mutually exclusive", 2);
  }

  const settings = loadSettings();
  const { defaults, sectionName } = resolveRepoDefaults(settings);

  const cliEnvironment = validateEnvironmentArg(values.environment, "--environment");

  const { queryBody, frontMatter } = resolveQuerySourceReadOnly(values);

  const environment =
    cliEnvironment ?? validateEnvironmentArg(frontMatter.env, "front-matter `env`");
  const timeExpr = values.time ?? frontMatter.time ?? "1h";

  let range;
  try {
    range = parseTimeRange(timeExpr);
  } catch (err) {
    if (err instanceof TimeRangeParseError) fail(err.message, 2);
    throw err;
  }

  const templateVars: Record<string, string | undefined> = {
    env: environment,
    app: frontMatter.app ?? defaults.app,
  };

  const logGroups = resolveLogGroups({
    cliLogGroups: flatten((values["log-group"] ?? []).map((s) => s.split(","))),
    frontMatterLogGroup: frontMatter["log-group"],
    configGroupTemplate: defaults.group,
    templateVars,
    sectionName,
  });

  let expandedQuery: string;
  try {
    expandedQuery = expandTemplate(queryBody, templateVars);
  } catch (err) {
    if (err instanceof ConfigError) fail(err.message, 2);
    throw err;
  }

  const envConfig: EnvConfig =
    environment ? resolveEnvConfig(settings, sectionName, environment) : {};
  const profile = values.profile ?? envConfig.awsProfile;
  const region =
    values.region ?? envConfig.region ?? process.env.AWS_REGION;

  if (!region) {
    fail(
      "no AWS region — set [env." + (environment ?? "<env>") + "].region in settings.toml," +
        " pass --region, or set AWS_REGION",
      2,
    );
  }
  if (profile) {
    process.env.AWS_PROFILE = profile;
  }

  const accountId = await resolveAccountId({
    cliAccountId: values["account-id"],
    envConfig,
    region,
    quiet: values.quiet,
    environmentLabel: environment,
  });

  const arns = logGroups.map((name) =>
    logGroupArn({ region, accountId, logGroupName: name }),
  );

  const time = chooseTimeSpec(timeExpr, range, values["preserve-time-window"]);

  if (!values.quiet) {
    if (profile && !values.profile) {
      process.stderr.write(`  AWS_PROFILE: ${profile} (from [env.${environment}])\n`);
    }
    if (envConfig.region && !values.region) {
      process.stderr.write(`  AWS region:  ${envConfig.region} (from [env.${environment}])\n`);
    }
    process.stderr.write(`  account ID:  ${accountId}\n`);
    process.stderr.write(`  log groups:  ${logGroups.join(", ")}\n`);
    process.stderr.write(
      `  time:        ${time.kind === "relative" ? `RELATIVE -${time.secondsBack}s` : `ABSOLUTE ${range.startTime.toISOString()}–${range.endTime.toISOString()}`}\n`,
    );
  }

  const queryDetail = buildQueryDetail({
    query: expandedQuery,
    logGroupArns: arns,
    time,
  });
  const url = buildConsoleLink({ region, queryDetail });
  process.stdout.write(url + "\n");
}

function chooseTimeSpec(
  timeExpr: string,
  range: { startTime: Date; endTime: Date },
  preserveAbsolute: boolean,
): TimeSpec {
  if (!preserveAbsolute) {
    const seconds = tryParseRelativeDurationSeconds(timeExpr);
    if (seconds !== null) {
      return { kind: "relative", secondsBack: seconds };
    }
  }
  return {
    kind: "absolute",
    startMs: range.startTime.getTime(),
    endMs: range.endTime.getTime(),
  };
}

interface AccountIdArgs {
  cliAccountId: string | undefined;
  envConfig: EnvConfig;
  region: string;
  quiet: boolean;
  environmentLabel: string | undefined;
}

async function resolveAccountId(args: AccountIdArgs): Promise<string> {
  if (args.cliAccountId) return args.cliAccountId;
  if (args.envConfig.accountId) return args.envConfig.accountId;

  if (!args.quiet) {
    process.stderr.write(
      `  looking up account ID via STS (set [env.${args.environmentLabel ?? "<env>"}].account-id to skip)\n`,
    );
  }
  const client = new STSClient({ region: args.region });
  try {
    const result = await client.send(new GetCallerIdentityCommand({}));
    if (!result.Account) {
      fail("STS GetCallerIdentity returned no Account", 1);
    }
    return result.Account;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(
      `could not look up AWS account ID via STS: ${message}\n` +
        `  hint: set [env.${args.environmentLabel ?? "<env>"}].account-id in settings.toml, or pass --account-id`,
      1,
    );
  }
}

interface QuerySource {
  queryBody: string;
  frontMatter: FrontMatter;
}

function resolveQuerySourceReadOnly(values: LinkValues): QuerySource {
  if (values.query) {
    return { queryBody: values.query, frontMatter: {} };
  }
  if (values["query-file"]) {
    const source = values["query-file"] === "-" ? 0 : values["query-file"];
    const raw = readFileSync(source, "utf8");
    const parsed = parseQueryFile(raw);
    if (!parsed.body) fail(`${values["query-file"]} has no query body`, 2);
    return { queryBody: parsed.body, frontMatter: parsed.frontMatter };
  }

  const path = currentInsightsPath();
  if (!existsSync(path)) {
    fail(
      `${path} does not exist — run \`cloudwatch-insights query\` first, or pass --query / --query-file`,
      2,
    );
  }
  const parsed = loadQueryFile(path);
  if (!parsed.body) fail(`${path} has no query body`, 2);
  return { queryBody: parsed.body, frontMatter: parsed.frontMatter };
}

interface ResolveLogGroupsArgs {
  cliLogGroups: string[];
  frontMatterLogGroup: string | string[] | undefined;
  configGroupTemplate: string | undefined;
  templateVars: Record<string, string | undefined>;
  sectionName: string | null;
}

function resolveLogGroups(args: ResolveLogGroupsArgs): string[] {
  const expand = (templates: string[]): string[] => {
    try {
      return templates.map((t) => expandTemplate(t, args.templateVars));
    } catch (err) {
      if (err instanceof ConfigError) fail(err.message, 2);
      throw err;
    }
  };

  const fromCli = args.cliLogGroups.map((s) => s.trim()).filter(Boolean);
  if (fromCli.length > 0) return expand(fromCli);

  const fromFm = toArray(args.frontMatterLogGroup);
  if (fromFm.length > 0) return expand(fromFm);

  if (args.configGroupTemplate) return expand([args.configGroupTemplate]);

  fail(
    args.sectionName
      ? `no log group given, and config section [${args.sectionName}] has no "group". Set it with \`cloudwatch-insights edit-config\` or pass --log-group.`
      : "no log group given, and no matching config section was found. Pass --log-group or run `cloudwatch-insights edit-config`.",
    2,
  );
}

function validateEnvironmentArg(
  raw: string | undefined,
  source: string,
): string | undefined {
  if (raw === undefined) return undefined;
  if (!isValidEnvironmentName(raw)) {
    fail(
      `${source} must be lower kebab-case (e.g. "prod", "us-east-1") — got ${JSON.stringify(raw)}`,
      2,
    );
  }
  return raw;
}

function toArray<T>(value: T | T[] | undefined): T[] {
  if (value === undefined) return [];
  return Array.isArray(value) ? value : [value];
}

function flatten<T>(arrays: T[][]): T[] {
  const out: T[] = [];
  for (const a of arrays) out.push(...a);
  return out;
}

function fail(message: string, code = 1): never {
  process.stderr.write(`error: ${message}\n`);
  process.exit(code);
}

// Keep types referenced from index.ts in scope without forcing extra imports
// at the call site.
export type { Settings, RepoConfig };
