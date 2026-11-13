#!/usr/bin/env bash
# Measures image size, startup time and idle memory of each Docker-distributed
# tool in the comparison, and prints a Markdown table.
#
#   ./measure.sh                 every tool
#   ./measure.sh floci moto      only these
#   ./measure.sh --list          tool names
#
# Startup is the time from `docker run` until the endpoint first answers HTTP.
# Idle memory is `docker stats` after the container has sat idle for
# SETTLE_SECONDS. Both are the median of RUNS fresh containers. Images are
# pulled (or built) before timing starts.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOST_PORT="${HOST_PORT:-45660}"
SETTLE_SECONDS="${SETTLE_SECONDS:-15}"
RUNS="${RUNS:-3}"
READY_TIMEOUT_SECONDS="${READY_TIMEOUT_SECONDS:-180}"
CONTAINER="aws-emulation-measure"

TOOLS="localstack aws-emulator floci ministack fakecloud localemu moto
s3mock silo rustfs seaweedfs garage versitygw
dynamodb-local alternator"

# Sets IMAGE, PORT, URL_PATH, SCHEME and RUN_ARGS / CMD_ARGS (arrays) for a tool.
tool-config() {
    SCHEME=http
    URL_PATH=/
    RUN_ARGS=()
    CMD_ARGS=()
    case "$1" in
        localstack)
            IMAGE=localstack/localstack:4.14.0 PORT=4566 URL_PATH=/_localstack/health ;;
        aws-emulator)
            IMAGE=aws-emulator:latest PORT=9297 SCHEME=https URL_PATH=/__admin/health
            CMD_ARGS=(-p 9297 --telemetry off) ;;
        floci)
            IMAGE=floci/floci:latest PORT=4566 ;;
        ministack)
            IMAGE=ministackorg/ministack:latest PORT=4566 ;;
        fakecloud)
            IMAGE=ghcr.io/faiscadev/fakecloud:latest PORT=4566 ;;
        localemu)
            IMAGE=localemu/localemu:latest PORT=4566 ;;
        moto)
            IMAGE=motoserver/moto:latest PORT=5000 ;;
        s3mock)
            IMAGE=adobe/s3mock:latest PORT=9090 ;;
        silo)
            IMAGE=pgsty/silo:latest PORT=9000
            RUN_ARGS=(-e MINIO_ROOT_USER=measure -e MINIO_ROOT_PASSWORD=measure-secret)
            CMD_ARGS=(server /data) ;;
        rustfs)
            IMAGE=rustfs/rustfs:latest PORT=9000 ;;
        seaweedfs)
            IMAGE=chrislusf/seaweedfs:latest PORT=8333
            CMD_ARGS=(mini -dir=/data) ;;
        garage)
            IMAGE=dxflrs/garage:v2.5.0 PORT=3900
            RUN_ARGS=(-v "$HERE/docker/garage.toml:/etc/garage.toml:ro") ;;
        versitygw)
            IMAGE=versity/versitygw:latest PORT=7070
            RUN_ARGS=(-e ROOT_ACCESS_KEY=measure -e ROOT_SECRET_KEY=measure-secret
                --entrypoint sh)
            CMD_ARGS=(-c 'mkdir -p /tmp/vgw && exec versitygw posix /tmp/vgw') ;;
        dynamodb-local)
            IMAGE=amazon/dynamodb-local:latest PORT=8000
            CMD_ARGS=(-jar DynamoDBLocal.jar -inMemory -disableTelemetry) ;;
        alternator)
            IMAGE=scylladb/scylla:latest PORT=8000
            CMD_ARGS=(--smp 1 --memory 512M --overprovisioned 1 --developer-mode 1
                --alternator-port 8000 --alternator-write-isolation always) ;;
        *)
            echo "unknown tool: $1" >&2
            return 1 ;;
    esac
}

now() {
    perl -MTime::HiRes=time -e 'printf "%.3f\n", time'
}

ensure-image() {
    if [ "$1" = aws-emulator ]; then
        local arch
        case "$(docker version --format '{{.Server.Arch}}')" in
            arm64) arch=aarch64 ;;
            *) arch=x86_64 ;;
        esac
        # Empty context: the Dockerfile ADDs everything it needs.
        local context
        context="$(mktemp -d)"
        docker build -q -t "$IMAGE" --build-arg "ARCH=$arch" \
            -f "$HERE/docker/aws-emulator/Dockerfile" "$context" >/dev/null
        rmdir "$context"
    elif ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
        docker pull -q "$IMAGE" >/dev/null
    fi
}

# Prints "<download> | <unpacked>" in MB. Unpacked is the sum of the layers.
# With the containerd image store, `docker image inspect` counts the compressed
# download as well, and the difference is that download; with the classic
# store there is no download left on disk to measure.
image-sizes() {
    local total unpacked
    total="$(docker image inspect -f '{{.Size}}' "$IMAGE")"
    unpacked="$(docker image history --human=false --format '{{.Size}}' "$IMAGE" \
        | awk '{ sum += $1 } END { print sum }')"
    awk -v t="$total" -v u="$unpacked" 'BEGIN {
        if (t > u) printf "%.0f | %.0f", (t - u) / 1e6, u / 1e6
        else printf "? | %.0f", u / 1e6
    }'
}

wait-until-ready() {
    local url="$SCHEME://127.0.0.1:$HOST_PORT$URL_PATH" deadline code
    deadline=$(( $(date +%s) + READY_TIMEOUT_SECONDS ))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        code="$(curl -sk -o /dev/null -m 2 -w '%{http_code}' "$url" || true)"
        if [ -n "$code" ] && [ "$code" != 000 ]; then
            return 0
        fi
        if ! docker container inspect "$CONTAINER" >/dev/null 2>&1; then
            echo "container exited" >&2
            return 1
        fi
        sleep 0.1
    done
    echo "not ready after ${READY_TIMEOUT_SECONDS}s" >&2
    return 1
}

stop-container() {
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}

# Converts a `docker stats` figure such as 8.73MiB or 1.2GiB to MiB.
to-mib() {
    awk -v v="$1" 'BEGIN {
        n = v + 0
        if (v ~ /GiB$/) n *= 1024
        else if (v ~ /KiB$/) n /= 1024
        else if (v ~ /[0-9]B$/) n /= 1048576
        printf "%.1f", n
    }'
}

median() {
    tr ' ' '\n' | grep . | sort -n | awk '{ v[NR] = $1 } END {
        if (NR % 2) print v[(NR + 1) / 2]
        else printf "%.1f\n", (v[NR / 2] + v[NR / 2 + 1]) / 2
    }'
}

# Starts the container once; prints "<startup seconds> <idle MiB>".
run-once() {
    local start ready
    stop-container
    start="$(now)"
    docker run -d --rm --name "$CONTAINER" -p "127.0.0.1:$HOST_PORT:$PORT" \
        ${RUN_ARGS[@]+"${RUN_ARGS[@]}"} "$IMAGE" ${CMD_ARGS[@]+"${CMD_ARGS[@]}"} >/dev/null
    if ! wait-until-ready; then
        docker logs "$CONTAINER" 2>&1 | tail -20 >&2 || true
        stop-container
        return 1
    fi
    ready="$(now)"
    sleep "$SETTLE_SECONDS"
    printf '%s %s\n' \
        "$(awk -v a="$start" -v b="$ready" 'BEGIN { printf "%.2f", b - a }')" \
        "$(to-mib "$(docker stats --no-stream --format '{{.MemUsage}}' "$CONTAINER" | awk '{ print $1 }')")"
    stop-container
}

measure-tool() {
    local name="$1" i result startups="" memories=""
    tool-config "$name"
    ensure-image "$name"
    for i in $(seq "$RUNS"); do
        result="$(run-once)" || return 1
        startups="$startups ${result% *}"
        memories="$memories ${result#* }"
    done
    printf '| %s | `%s` | %s | %s s | %s MiB |\n' "$name" "$IMAGE" "$(image-sizes)" \
        "$(echo "$startups" | median | awk '{ printf "%.1f", $1 }')" \
        "$(echo "$memories" | median | awk '{ printf "%.0f", $1 }')"
}

main() {
    if [ "${1:-}" = --list ]; then
        echo $TOOLS | tr ' ' '\n'
        return
    fi
    local selected="$*"
    if [ -z "$selected" ]; then
        selected="$TOOLS"
    fi

    trap stop-container EXIT
    echo "| Tool | Image | Download (MB) | Unpacked (MB) | Startup | Idle memory |"
    echo "|---|---|--:|--:|--:|--:|"
    local name
    for name in $selected; do
        if ! (measure-tool "$name"); then
            printf '| %s | failed, see stderr | — | — | — | — |\n' "$name"
            stop-container
        fi
    done
}

main "$@"
