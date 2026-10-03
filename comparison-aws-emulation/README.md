# AWS Local Emulation Tools

A comparison of tools that emulate AWS services locally for development and
testing, focused on four services: S3, DynamoDB, SSM Parameter Store and
CloudWatch Logs Insights. Researched October 2026.

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

✓ = solid · ◐ = partial, see notes · ✗ = not supported

## Multi-service emulators

| | LocalStack Community 4.14 | [Floci](https://floci.io/) | [MiniStack](https://ministack.org/) | [fakecloud](https://fakecloud.dev/) | [LocalEmu](https://github.com/localemu/localemu) | [Moto](https://github.com/getmoto/moto) |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| **S3** | ✓ | ✓ | ◐ | ✓ | ✓ | ✓ |
| **DynamoDB** | ✓ | ◐ | ✓ | ✓ | ✓ | ✓ |
| **SSM Parameter Store** | ✓ | ◐ | ✓ | ✓ | ✓ | ✓ |
| **Logs Insights queries** | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ |
| **Persistence** | ✗ | ✓ | ◐ | ✓ | ✓ | ✗ |
| **LocalStack init hooks** | ✓ | ✓ | ✓ | ✗ | ◐ | ✗ |
| **Port 4566 by default** | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ (5000) |

Notes:

- **LocalStack Community** — frozen. Its DynamoDB runs DynamoDB Local under
  the hood, and Logs Insights falls through to Moto's engine. Persistence
  (`PERSISTENCE=1`) has been paid-only since 1.0 and does nothing in
  Community; the third-party
  [localstack-persist](https://github.com/GREsau/localstack-persist) image
  adds it, but is itself frozen at 4.14.
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
against ingested events, but each understands only a small subset of the
query language. How a tool treats a query outside its subset matters as much
as the subset itself.

| | Floci | MiniStack | fakecloud | Moto, LocalStack Community, LocalEmu |
|---|:-:|:-:|:-:|:-:|
| `fields`, `sort`, `limit` | ✓ | ◐ (sort on `@timestamp` only) | ✓ | ✓ |
| `filter` with `=` / `!=` | ✓ | ◐ (`@` fields only) | ✓ | ✗ |
| `filter` with `like` / regex | ✗ | ✓ | ✓ | ✗ |
| JSON fields in `@message` | ✓ | ✗ | ✓ | ✗ |
| `stats … by` | ✗ | ✗ | ✓ | ✗ |
| `parse` | ✗ | ✗ | ✓ | ✗ |
| Unsupported input | query fails | silently ignored | query fails | silently ignored |

fakecloud also supports `dedup` and `display`, and Floci `dedup`. None
supports `and` / `or` in a filter, or `bin()`. In Floci a compound filter
quietly matches nothing or everything; in MiniStack and Moto an unsupported
`filter` is dropped, so the query returns rows it should not.

### Distribution

| | LocalStack Community 4.14 | Floci | MiniStack | fakecloud | LocalEmu | Moto |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| **Docker image** | `localstack/localstack:4.14.0` | `floci/floci` | `ministackorg/ministack` | `ghcr.io/faiscadev/fakecloud` | `localemu/localemu` | `motoserver/moto` |
| **Without Docker** | ✗ | ✗ | ✓ `pip` | ✓ static binary, Homebrew | ✓ `pip` | ✓ `pip` |
| **In-process mode** | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ Python |
| **Testcontainers** | Java, Node, Python, Go, .NET, Rust | Java, Node, Python, .NET | Java | ✗ | ✗ | ✗ |
| **Helm chart** | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |

Floci prints the same readiness line as LocalStack, so according to its docs
Testcontainers' `LocalStackContainer` also works with the Floci image. The
"native binary" Floci advertises is what runs inside the image; `floci-cli`
is a wrapper that starts the container.

### Licensing and project health

| | LocalStack Community 4.14 | Floci | MiniStack | fakecloud | LocalEmu | Moto |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| **License** | Apache 2.0 | MIT | MIT | AGPLv3 | Apache 2.0 | Apache 2.0 |
| **Language** | Python | Java (GraalVM native) | Python | Rust | Python | Python |
| **First release** | 2017 | March 2026 | March 2026 | April 2026 | May 2026 | 2013 |
| **Releases, July–Sept 2026** | 0 (archived) | 10 | 46 | 30 | 1 | 1 |
| **Maintainers** | — | 1 lead + 1, sponsor-funded | 1 | 1 | 1 | community |

Floci has by far the most adoption of the new ones (about 26k GitHub stars to
MiniStack's 5k) and releases on a fixed twice-monthly schedule. fakecloud is
pre-1.0. LocalEmu has had no commits since August 2026. The current, paid
LocalStack remains the reference point for fidelity; commercial use starts
at $39 a month.

## S3 servers

Measured on Docker Desktop (arm64), with path-style requests from the AWS CLI.
"Presigned checked" is whether a URL with a tampered signature is rejected.

| | [Adobe S3Mock](https://github.com/adobe/S3Mock) | [Silo](https://github.com/pgsty/silo) (MinIO fork) | [RustFS](https://github.com/rustfs/rustfs) | [SeaweedFS](https://github.com/seaweedfs/seaweedfs) | [Garage](https://garagehq.deuxfleurs.fr/) | [Versity Gateway](https://github.com/versity/versitygw) |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| **License** | Apache 2.0 | AGPLv3 | Apache 2.0 | Apache 2.0 | AGPLv3 | Apache 2.0 |
| **Docker image** | `adobe/s3mock` | `pgsty/silo` | `rustfs/rustfs` | `chrislusf/seaweedfs` | `dxflrs/garage` | `versity/versitygw` |
| **Startup / idle memory** | 3.0 s / 232 MiB | 0.5 s / 82 MiB | 0.5 s / 112 MiB | 1.4 s / 273 MiB | 0.9 s / 49 MiB | 0.6 s / 51 MiB |
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
  the memory.
- **Garage** signs requests against a region named `garage` unless
  configured otherwise, and requires keys of the form `GK…`.
- For Go tests, [gofakes3](https://github.com/johannesboyne/gofakes3) runs
  in-process.

## DynamoDB servers

| | [DynamoDB Local](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DynamoDBLocal.html) | [ScyllaDB Alternator](https://docs.scylladb.com/manual/stable/alternator/) | [dynalite](https://github.com/architect/dynalite) |
|---|:-:|:-:|:-:|
| **License** | Proprietary, needs an AWS account | Source-available | Apache 2.0 |
| **Docker image** | `amazon/dynamodb-local` | `scylladb/scylla` | ✗ (npm) |
| **Startup / idle memory** | 2.9 s / ~220 MiB | 3.1 s / 81 MiB | ~1 s |
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
