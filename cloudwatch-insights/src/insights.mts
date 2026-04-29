import {
  CloudWatchLogsClient,
  GetQueryResultsCommand,
  QueryStatus,
  ResultField,
  StartQueryCommand,
} from "@aws-sdk/client-cloudwatch-logs";

export interface QueryStatistics {
  recordsMatched?: number;
  recordsScanned?: number;
  bytesScanned?: number;
}

export interface QueryProgress {
  status: QueryStatus | string;
  statistics: QueryStatistics;
}

export interface RunQueryOptions {
  client: CloudWatchLogsClient;
  logGroups: string[];
  queryString: string;
  startTime: Date;
  endTime: Date;
  limit?: number;
  /** Polling interval in ms. Defaults to 1000. */
  pollIntervalMs?: number;
  /** Called after each poll so callers can surface progress. */
  onProgress?: (progress: QueryProgress) => void;
}

export interface QueryRow {
  [field: string]: string;
}

export interface QueryResult {
  rows: QueryRow[];
  statistics?: QueryStatistics;
  status: QueryStatus | string;
}

const TERMINAL_STATUSES: ReadonlySet<string> = new Set([
  QueryStatus.Complete,
  QueryStatus.Failed,
  QueryStatus.Cancelled,
  QueryStatus.Timeout,
  "Unknown",
]);

export async function runInsightsQuery(options: RunQueryOptions): Promise<QueryResult> {
  const {
    client,
    logGroups,
    queryString,
    startTime,
    endTime,
    limit,
    pollIntervalMs = 1_000,
    onProgress,
  } = options;

  if (logGroups.length === 0) {
    throw new Error("at least one log group is required");
  }

  const start = await client.send(
    new StartQueryCommand({
      logGroupNames: logGroups,
      startTime: Math.floor(startTime.getTime() / 1000),
      endTime: Math.floor(endTime.getTime() / 1000),
      queryString,
      limit,
    }),
  );

  const queryId = start.queryId;
  if (!queryId) {
    throw new Error("StartQuery did not return a queryId");
  }

  while (true) {
    const response = await client.send(new GetQueryResultsCommand({ queryId }));
    const status = response.status ?? "Unknown";
    const statistics: QueryStatistics = response.statistics
      ? {
          recordsMatched: response.statistics.recordsMatched,
          recordsScanned: response.statistics.recordsScanned,
          bytesScanned: response.statistics.bytesScanned,
        }
      : {};
    onProgress?.({ status, statistics });

    if (TERMINAL_STATUSES.has(status)) {
      if (status !== QueryStatus.Complete) {
        throw new Error(`query ended with status: ${status}`);
      }
      return {
        status,
        rows: (response.results ?? []).map(resultFieldsToRow),
        statistics: response.statistics ? statistics : undefined,
      };
    }

    await sleep(pollIntervalMs);
  }
}

function resultFieldsToRow(fields: ResultField[]): QueryRow {
  const row: QueryRow = {};
  for (const field of fields) {
    if (field.field === undefined) continue;
    // "@ptr" is CloudWatch's internal pointer; keep it so the user can drill down if needed.
    row[field.field] = field.value ?? "";
  }
  return row;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
