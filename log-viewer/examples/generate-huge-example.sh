#!/usr/bin/env bash
# Generate a large JSONL log file for stress-testing the viewer.
#
# Each line is intentionally fat: many fields, a long message, a stack
# trace blob, and a small nested object — so the resulting file gets
# big both row-wise and width-wise.
#
#   ./examples/generate-huge-example.sh           # 1000 rows (default)
#   ./examples/generate-huge-example.sh 50000     # 50k rows
#
# Output goes to examples/huge-sample.jsonl (gitignored).
set -euo pipefail

rows="${1:-1000}"
case "$rows" in
    ''|*[!0-9]*)
        echo "usage: $0 [row-count]" >&2
        exit 2
        ;;
esac

dir="$(cd "$(dirname "$0")" && pwd)"
out="$dir/huge-sample.jsonl"

levels=(info info info info debug warn warn error)
services=(api worker auth billing search notifier ingest scheduler)
methods=(GET POST DELETE PUT PATCH)
paths=(
    "/v1/users/42"
    "/v1/users/42/sessions"
    "/v1/orders"
    "/v1/orders/9817/items"
    "/v1/products/search?q=widget&limit=50"
    "/v1/sessions"
    "/v1/healthz"
    "/v1/internal/metrics"
    "/v1/orgs/acme-co/teams/platform/members"
)
hosts=(api-01.internal api-02.internal api-03.internal worker-01.internal worker-02.internal)
regions=(us-east-1 us-west-2 eu-west-1 eu-central-1 ap-southeast-2)

# A long-ish message so each line is fat. ~400 chars before any other fields.
long_message=$(printf 'Processed inbound request and dispatched to downstream worker queue; payload normalised, idempotency token verified, audit event enqueued, retry budget within bounds, downstream latency within SLO, cache hit ratio nominal, circuit breaker closed, no anomalies detected by guardrail %.0s' 1 2)

# Pre-built fake stack trace string — escaped for embedding inside JSON.
stack=$(cat <<'STACK'
java.lang.RuntimeException: simulated failure during request handling
    at com.example.api.RequestHandler.handle(RequestHandler.java:142)
    at com.example.api.Router.dispatch(Router.java:88)
    at com.example.api.Server$Connection.process(Server.java:331)
    at com.example.runtime.Worker.runOnce(Worker.java:217)
    at com.example.runtime.Worker.run(Worker.java:188)
    at java.base/java.lang.Thread.run(Thread.java:1583)
Caused by: java.io.IOException: upstream closed connection
    at com.example.net.UpstreamClient.read(UpstreamClient.java:74)
    ... 5 more
STACK
)
# Replace newlines with literal \n for embedding into a JSON string.
stack_json="${stack//$'\n'/\\n}"

epoch=$(date -u +%s)

write-rows() {
    local i=0
    while (( i < rows )); do
        local ts level service method path host region pod duration status seq trace_id user_id
        ts=$(printf '%(%Y-%m-%dT%H:%M:%S)T.%03dZ' $((epoch + i / 10)) $((i % 1000)))
        level="${levels[$((RANDOM % ${#levels[@]}))]}"
        service="${services[$((RANDOM % ${#services[@]}))]}"
        method="${methods[$((RANDOM % ${#methods[@]}))]}"
        path="${paths[$((RANDOM % ${#paths[@]}))]}"
        host="${hosts[$((RANDOM % ${#hosts[@]}))]}"
        region="${regions[$((RANDOM % ${#regions[@]}))]}"
        pod="api-server-$(printf '%04x' $((RANDOM % 4096)))"
        duration=$((RANDOM % 2000 + 1))
        status=200
        case "$level" in
            warn)  status=429 ;;
            error) status=500 ;;
        esac
        seq=$i
        trace_id="$(printf '%08x-%04x-%04x-%04x-%012x' \
            $((RANDOM * RANDOM)) $RANDOM $RANDOM $RANDOM $((RANDOM * RANDOM * RANDOM)))"
        user_id=$((RANDOM % 1000000 + 1000))

        if (( i % 250 == 17 )); then
            # Plain-text line every so often, exercising the wrapping path.
            printf '[stderr-bridge] dropped frame at offset %d on host %s region %s\n' \
                "$i" "$host" "$region"
        elif [[ "$level" == "error" ]]; then
            printf '{"@timestamp":"%s","level":"%s","service":"%s","podname":"%s","host":"%s","region":"%s","message":"%s","method":"%s","path":"%s","status":%d,"duration_ms":%d,"seq":%d,"trace_id":"%s","user_id":%d,"request":{"headers":{"x-request-id":"%s","x-forwarded-for":"203.0.113.%d"},"size_bytes":%d},"error":{"type":"RuntimeException","message":"simulated failure during request handling","stack":"%s"}}\n' \
                "$ts" "$level" "$service" "$pod" "$host" "$region" "$long_message" "$method" "$path" "$status" "$duration" "$seq" "$trace_id" "$user_id" "$trace_id" $((RANDOM % 256)) $((RANDOM % 8192 + 256)) "$stack_json"
        else
            printf '{"@timestamp":"%s","level":"%s","service":"%s","podname":"%s","host":"%s","region":"%s","message":"%s","method":"%s","path":"%s","status":%d,"duration_ms":%d,"seq":%d,"trace_id":"%s","user_id":%d,"request":{"headers":{"x-request-id":"%s","x-forwarded-for":"203.0.113.%d"},"size_bytes":%d}}\n' \
                "$ts" "$level" "$service" "$pod" "$host" "$region" "$long_message" "$method" "$path" "$status" "$duration" "$seq" "$trace_id" "$user_id" "$trace_id" $((RANDOM % 256)) $((RANDOM % 8192 + 256))
        fi
        i=$((i + 1))
    done
}

write-rows > "$out"
size=$(wc -c < "$out" | tr -d ' ')
printf 'wrote %s rows (%s bytes) to %s\n' "$rows" "$size" "$out"
