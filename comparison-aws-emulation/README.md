# AWS Local Emulation Tools

A comparison of tools that emulate AWS services locally for development and
testing, focused on four services: S3, DynamoDB, SSM Parameter Store and
CloudWatch Logs Insights. Researched October 2026; AWS Emulator added
November 2026.

## What changed in 2026

Two tools that used to be the default answers stopped being free and open:

- **LocalStack** archived its Apache 2.0 Community edition on 23 March 2026
  (last release 4.14.0). The current image requires an auth token, and the
  free Hobby plan is for non-commercial use only. Pinned 4.x Community images
  are still on Docker Hub and run without a token, but get no fixes.
- **MinIO** stopped publishing binaries in October 2025, archived the repo in
  2026, and in September 2026 removed `minio/minio` from Docker Hub and gated
  `quay.io/minio/minio`. Testcontainers' MinIO modules still default to the
  deleted image. [Silo](https://github.com/pgsty/silo) (`pgsty/silo`) is a
  maintained AGPL fork.

Several LocalStack replacements appeared within a few months of each other.
They are young, fast-moving, and each is largely the work of one person.

In November 2026 AWS published its own:
[AWS Emulator](https://github.com/aws/aws-emulator), a CC0-licensed Rust
emulator with one crate per AWS service (over 400 of them), free of charge.

✓ = solid · ◐ = partial, see notes · ✗ = not supported

Image sizes, startup times and idle memory were measured on 12 November 2026
with [`measure.sh`](measure.sh), on OrbStack (arm64, Apple silicon). Download
is the compressed image and unpacked its size on disk; startup is the time
from `docker run` until the endpoint first answers HTTP, which can be before
every service is ready; idle memory is `docker stats` after 15 seconds
without requests. Startup and memory are the median of three fresh
containers. The script's settings for each tool are the minimum to start
it; memory especially depends on them.

## Multi-service emulators

| | LocalStack Community 4.14 | [AWS Emulator](https://github.com/aws/aws-emulator) | [Floci](https://floci.io/) | [MiniStack](https://ministack.org/) | [fakecloud](https://fakecloud.dev/) | [LocalEmu](https://github.com/localemu/localemu) | [Moto](https://github.com/getmoto/moto) |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| **S3** | ✓ | ✓ | ✓ | ◐ | ✓ | ✓ | ✓ |
| **DynamoDB** | ✓ | ✓ | ◐ | ✓ | ✓ | ✓ | ✓ |
| **SSM Parameter Store** | ✓ | ✓ | ◐ | ✓ | ✓ | ✓ | ✓ |
| **Logs Insights queries** | ◐ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ |
| **Persistence** | ✗ | ✗ | ✓ | ◐ | ✓ | ✓ | ✗ |
| **LocalStack init hooks** | ✓ | ✗ | ✓ | ✓ | ✗ | ◐ | ✗ |
| **Port 4566 by default** | ✓ | ✗ (9297, HTTPS) | ✓ | ✓ | ✓ | ✓ | ✗ (5000) |

Notes:

- **LocalStack Community** — frozen. Its DynamoDB runs DynamoDB Local under
  the hood, and Logs Insights falls through to Moto's engine. Persistence
  (`PERSISTENCE=1`) has been paid-only since 1.0 and does nothing in
  Community; the third-party
  [localstack-persist](https://github.com/GREsau/localstack-persist) image
  adds it, but is itself frozen at 4.14.
- **AWS Emulator** — implemented from AWS's own Smithy models, so every
  operation exists on the wire, but some answer `501 NotImplemented`; gaps are
  tracked as files under `issues/things-that-do-not-work/` (about 6,500, roughly
  400 of them naming S3, DynamoDB, SSM or Logs). DynamoDB has transactions,
  PartiQL and Streams; SSM SecureString values are really encrypted through
  its KMS emulation. State is in memory only and is lost when the process
  stops. It serves HTTPS with a self-signed certificate and writes credentials
  and the certificate under `~/.aws/emulator/<port>/` for the SDK to pick up.
  S3 routes are path-style. Telemetry is on by default (`--telemetry off` or
  `EMULATOR_TELEMETRY=off`). These notes come from reading the source and
  docs, not from running it.
- **Floci** — DynamoDB is a native engine by default: GSIs are not stored but
  answered by scanning the base table, and are always consistent. It can
  instead forward to a DynamoDB Local you run yourself. SSM keeps only 5
  versions of history by default (AWS keeps 100). SecureString is not
  encrypted. Persistence is off by default (`FLOCI_STORAGE_MODE`), with
  write-ahead-log and per-service modes. It runs scripts from
  `/etc/localstack/init/*.d` unchanged, and translates LocalStack environment
  variables such as `PERSISTENCE=1`.
- **MiniStack** — S3 state is shared across regions and any
  `LocationConstraint` is accepted. Persistence (`PERSIST_STATE=1`, plus
  `S3_PERSIST=1` for object bodies) is saved only on graceful shutdown, so a
  crash or `docker kill` loses everything. It bundles `awslocal` and the AWS
  CLI and supports `boot.d` and `ready.d` hooks.
- **fakecloud** — SecureString goes through its KMS emulation. Persistence
  snapshots on every change. SigV4 signatures are only checked with
  `--verify-sigv4`. No init-script mechanism.
- **LocalEmu** — a re-release of the LocalStack Community code under new
  names: hooks live in `/etc/localemu/init/`, and the CLI is `awsemu`. Unlike
  LocalStack, its DynamoDB is backed by Moto rather than DynamoDB Local.
- **Moto** — SecureString is a `kms:<key>:` prefix, not encryption. Moto has
  declined to add persistence. It is the only one that also runs in-process,
  as Python decorators.

### Logs Insights

Every tool here accepts `StartQuery` / `GetQueryResults` and runs the query
against ingested events, but all except AWS Emulator understand only a small
subset of the query language. How a tool treats a query outside its subset
matters as much as the subset itself.

| | AWS Emulator | Floci | MiniStack | fakecloud | Moto, LocalStack Community, LocalEmu |
|---|:-:|:-:|:-:|:-:|:-:|
| `fields`, `sort`, `limit` | ✓ | ✓ | ◐ (sort on `@timestamp` only) | ✓ | ✓ |
| `filter` with `=` / `!=` | ✓ | ✓ | ◐ (`@` fields only) | ✓ | ✗ |
| `filter` with `like` / regex | ✓ | ✗ | ✓ | ✓ | ✗ |
| JSON fields in `@message` | ✓ | ✓ | ✗ | ✓ | ✗ |
| `stats … by` | ✓ | ✗ | ✗ | ✓ | ✗ |
| `parse` | ✓ | ✗ | ✗ | ✓ | ✗ |
| Unsupported input | query fails | query fails | silently ignored | query fails | silently ignored |

AWS Emulator is the exception to the small subset: its engine (read from the
source, not run) also has `and` / `or` / `not`, `bin()`, `dedup`, `display`,
`pattern` and `lookup`, and accepts OpenSearch PPL and SQL as query
languages. A malformed query is refused with `MalformedQueryException`, as
in AWS. fakecloud also supports `dedup` and `display`, and Floci `dedup`.
Among the others, none supports `and` / `or` in a filter, or `bin()`. In Floci a compound filter
quietly matches nothing or everything; in MiniStack and Moto an unsupported
`filter` is dropped, so the query returns rows it should not.

### Distribution

| | LocalStack Community 4.14 | AWS Emulator | Floci | MiniStack | fakecloud | LocalEmu | Moto |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| **Docker image** | `localstack/localstack:4.14.0` | ◐ [build it yourself](docker/aws-emulator/Dockerfile) | `floci/floci` | `ministackorg/ministack` | `ghcr.io/faiscadev/fakecloud` | `localemu/localemu` | `motoserver/moto` |
| **Without Docker** | ✗ | ◐ Linux RPM / deb | ✗ | ✓ `pip` | ✓ static binary, Homebrew | ✓ `pip` | ✓ `pip` |
| **In-process mode** | ✗ | ✓ Rust | ✗ | ✗ | ✗ | ✗ | ✓ Python |
| **Testcontainers** | Java, Node, Python, Go, .NET, Rust | ✗ | Java, Node, Python, .NET | Java | ✗ | ✗ | ✗ |
| **Helm chart** | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Download / unpacked** | 456 / 1303 MB | 453 / 1170 MB | 93 / 274 MB | 68 / 189 MB | 112 / 336 MB | 302 / 1023 MB | 102 / 446 MB |
| **Startup / idle memory** | 1.8 s / 93 MiB | 0.4 s / 175 MiB | 0.2 s / 26 MiB | 0.8 s / 41 MiB | 0.2 s / 10 MiB | 1.3 s / 132 MiB | 0.7 s / 58 MiB |

Floci prints the same readiness line as LocalStack, so according to its docs
Testcontainers' `LocalStackContainer` also works with the Floci image. The
"native binary" Floci advertises is what runs inside the image; `floci-cli`
is a wrapper that starts the container.

AWS Emulator publishes no prebuilt image; its docs give a Dockerfile that
installs the RPM on Amazon Linux, kept here as
[`docker/aws-emulator/Dockerfile`](docker/aws-emulator/Dockerfile) with the
architecture as a build argument. All 436 services are compiled into one
binary, and nothing selects a subset: the router crate depends on every
service crate, with no Cargo features. Most of its unpacked size is that
binary. Idle, it uses 175 MiB, far below the 2 GiB limit its docs suggest;
services hold no state until a request first touches them. Building from
source is supported on Linux only and needs about 100 GB for `target/`,
which embedded mode also pays on the first `cargo test`.

### Licensing and project health

| | LocalStack Community 4.14 | AWS Emulator | Floci | MiniStack | fakecloud | LocalEmu | Moto |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| **License** | Apache 2.0 | CC0 (vendored crates Apache 2.0) | MIT | MIT | AGPLv3 | Apache 2.0 | Apache 2.0 |
| **Language** | Python | Rust | Java (GraalVM native) | Python | Rust | Python | Python |
| **First release** | 2017 | November 2026 | March 2026 | March 2026 | April 2026 | May 2026 | 2013 |
| **Releases, July–Sept 2026** | — (12 in its first week) | 10 | 46 | 30 | 1 | 1 |
| **Maintainers** | — | AWS; no outside PRs | 1 lead + 1, sponsor-funded | 1 | 1 | 1 | community |

Floci has by far the most adoption of the new ones (about 26k GitHub stars to
MiniStack's 5k) and releases on a fixed twice-monthly schedule. fakecloud is
pre-1.0. LocalEmu has had no commits since August 2026. The current, paid
LocalStack remains the reference point for fidelity among the third-party
tools; commercial use starts at $39 a month. AWS Emulator, at about 3.5k
stars within days of release, publishes per-service fidelity scores with
each release and takes bug reports but not pull requests.

## S3 servers

Features were checked on Docker Desktop (arm64), with path-style requests
from the AWS CLI; sizes and timings come from `measure.sh`, as above.
"Presigned checked" is whether a URL with a tampered signature is rejected.

| | [Adobe S3Mock](https://github.com/adobe/S3Mock) | [Silo](https://github.com/pgsty/silo) (MinIO fork) | [RustFS](https://github.com/rustfs/rustfs) | [SeaweedFS](https://github.com/seaweedfs/seaweedfs) | [Garage](https://garagehq.deuxfleurs.fr/) | [Versity Gateway](https://github.com/versity/versitygw) |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| **License** | Apache 2.0 | AGPLv3 | Apache 2.0 | Apache 2.0 | AGPLv3 | Apache 2.0 |
| **Docker image** | `adobe/s3mock` | `pgsty/silo` | `rustfs/rustfs` | `chrislusf/seaweedfs` | `dxflrs/garage` | `versity/versitygw` |
| **Download / unpacked** | 84 / 139 MB | 55 / 166 MB | 129 / 302 MB | 271 / 760 MB | 27 / 66 MB | 35 / 78 MB |
| **Startup / idle memory** | 1.6 s / 279 MiB | 0.3 s / 74 MiB | 0.1 s / 22 MiB | 3.9 s / 77 MiB | 0.1 s / 9 MiB | 0.1 s / 13 MiB |
| **Multipart** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Versioning** | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ (needs `--versioning-dir`) |
| **Presigned checked** | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Virtual-host style** | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Non-us-east-1 buckets** | ✓ | ◐ (location reads back `null`) | ◐ (reads back `us-east-1`) | ◐ (reads back `null`) | ◐ (only its configured region) | ✓ (only its configured region, as AWS) |
| **Setup on first start** | none | root user env | none | none, or keys as env | config file + env | root keys env |
| **In-process mode** | ✓ JUnit 5, TestNG | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Testcontainers** | Java, Node, Go | ◐ MinIO modules with the image overridden (untested) | Rust | ✗ | ✗ | ✗ |

Notes:

- **S3Mock** is built for tests: no auth, buckets pre-created from an
  environment variable, data deleted on exit unless `RETAIN_FILES_ON_EXIT`.
  5.x needs Spring Boot 4, which can clash with the app under test in
  embedded mode.
- **RustFS** reached 1.0 in September 2026. A bind-mounted data directory must
  be owned by uid 10001.
- **SeaweedFS** `weed mini` also starts a filer, WebDAV and an admin UI, hence
  the slower start.
- **Garage** signs requests against a region named `garage` unless
  configured otherwise, and requires keys of the form `GK…`.
- For Go tests, [gofakes3](https://github.com/johannesboyne/gofakes3) runs
  in-process.

## DynamoDB servers

| | [DynamoDB Local](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DynamoDBLocal.html) | [ScyllaDB Alternator](https://docs.scylladb.com/manual/stable/alternator/) | [dynalite](https://github.com/architect/dynalite) |
|---|:-:|:-:|:-:|
| **License** | Proprietary, needs an AWS account | Source-available | Apache 2.0 |
| **Docker image** | `amazon/dynamodb-local` | `scylladb/scylla` | ✗ (npm) |
| **Download / unpacked** | 239 / 586 MB | 260 / 628 MB | — |
| **Startup / idle memory** | 0.7 s / 123 MiB | 2.9 s / 78 MiB | ~1 s |
| **GSIs** | ✓ | ✓ | ✓ |
| **Transactions** | ✓ | ✗ | ✗ |
| **PartiQL** | ✓ | ✗ | ✗ |
| **Streams** | ✓ | ✓ | ✗ |
| **In-process mode** | ✓ Java | ✗ | ✓ Node |
| **Testcontainers** | Go, .NET, Rust | Java, Node, Python, Go, Rust | ✗ |

DynamoDB Local is still the reference implementation, and LocalStack
Community uses it internally. Without `-sharedDb` it keeps a separate database
per access key and region, so clients with different credentials see
different tables. It sends telemetry unless started with `-disableTelemetry`.
Alternator is a full database, and its license since 2026 allows commercial
users only CI and testing use. dynalite has had one release since 2020 and
lacks transactions.

## Sources

- AWS Emulator: [README](https://github.com/aws/aws-emulator),
  [user guide](https://docs.aws.amazon.com/emulator/latest/userguide/what-is-service.html),
  [Docker setup](https://docs.aws.amazon.com/emulator/latest/userguide/getting-started-docker.html),
  `services/service-com-amazonaws-cloudwatchlogs-logs-20140328/src/state/insights/`,
  `issues/things-that-do-not-work/`

- [LocalStack: The Road Ahead](https://blog.localstack.cloud/the-road-ahead-for-localstack/),
  [2026.03.0 release](https://blog.localstack.cloud/localstack-for-aws-release-2026-03-0/),
  [pricing](https://localstack.cloud/pricing)
- Floci: [README](https://github.com/floci-io/floci),
  [docs/services](https://github.com/floci-io/floci/tree/main/docs/services),
  [storage](https://github.com/floci-io/floci/blob/main/docs/configuration/storage.md)
- MiniStack: [README](https://github.com/ministackorg/ministack),
  `ministack/services/*.py`
- fakecloud: [docs](https://github.com/faiscadev/fakecloud/tree/main/website/content/docs),
  `crates/fakecloud-logs/src/query.rs`
- LocalEmu: [README and NOTICE](https://github.com/localemu/localemu)
- Moto: [service docs](https://github.com/getmoto/moto/tree/master/docs/docs/services),
  `moto/logs/logs_query/`, [persistence issue #9755](https://github.com/getmoto/moto/issues/9755)
- MinIO: [README](https://github.com/minio/minio),
  [Docker Hub removal](https://byteiota.com/minio-docker-hub-quay-anonymous-pull-fix/)
- [DynamoDB Local usage notes](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DynamoDBLocal.UsageNotes.html),
  [Alternator compatibility](https://docs.scylladb.com/manual/stable/alternator/compatibility.html)
