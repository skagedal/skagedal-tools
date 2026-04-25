# AWS Local Emulation Tools

A comparison of tools that emulate AWS services locally for development and
testing. LocalStack is the most well-known option, but several others exist —
some multi-service, some focused on a single API. This document covers four
subsystems that come up often in everyday work: **S3**, **DynamoDB**, **SSM
Parameter Store**, and **CloudWatch Logs Insights**.

## Tools covered

| Tool | Scope | Project |
|---|---|---|
| [LocalStack](https://www.localstack.cloud/) | Multi-service | `localstack/localstack` |
| [Moto](https://github.com/getmoto/moto) (`moto_server`) | Multi-service | `getmoto/moto` |
| [DynamoDB Local](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DynamoDBLocal.html) | DynamoDB only | AWS-published |
| [MinIO](https://min.io/) | S3-compatible object storage | `minio/minio` |
| [Adobe S3Mock](https://github.com/adobe/S3Mock) | S3 only | `adobe/S3Mock` |

## Subsystem coverage

| Subsystem | LocalStack | Moto (`moto_server`) | DynamoDB Local | MinIO | Adobe S3Mock |
|---|---|---|---|---|---|
| **S3** | Broad coverage of object, multipart, ACL, lifecycle, presigned URL, notification APIs. Edge cases (event filters, replication) sometimes diverge from AWS. | Broad object/multipart coverage; presigned URLs, versioning, notifications work. Lifecycle and replication implemented partially. | — | Full S3-compatible API for objects, multipart, presigned URLs, versioning, notifications; uses its own bucket/IAM model rather than AWS IAM. | Object, multipart, copy, presigned URLs, basic lifecycle. Intentionally minimal — no IAM, no notifications, no replication. |
| **DynamoDB** | Wraps **DynamoDB Local** under the hood, so behavior matches AWS-published Local. Streams and Global Tables are emulated separately. | Pure-Python reimplementation. Most of the API including Streams, TTL, transactions, indexes; some edge-case error parity gaps. | The reference local emulator from AWS — covers tables, indexes, streams, transactions, TTL, on-demand vs provisioned. Closest to AWS behavior. | — | — |
| **SSM Parameter Store** | Parameters (`PutParameter`, `GetParameter(s)`, `GetParametersByPath`), parameter history, hierarchical paths, `SecureString` (encryption is a no-op stub). Documents and Run Command are Pro-only. | Parameters with history, paths, `SecureString` (no real KMS), tags, labels. Documents/Run Command unimplemented or stubbed. | — | — | — |
| **CloudWatch Logs Insights** | CloudWatch Logs APIs in Community; Logs Insights query execution (`StartQuery`/`GetQueryResults` against log data) is part of the Pro tier. Community returns the API surface but does not execute queries against ingested logs. | `StartQuery`/`GetQueryResults`/`StopQuery`/`DescribeQueries` are mocked — they accept calls and return canned/empty results, but the Logs Insights query language is not actually executed against stored events. Useful for wiring tests, not for query-correctness tests. | — | — | — |

Legend: "—" means the tool does not target this service at all.

## Distribution and licensing

| Tool | Docker image | Other distribution | License |
|---|---|---|---|
| **LocalStack** | `localstack/localstack` (official, multi-arch) | `pip install localstack` CLI wrapper; Helm chart; Testcontainers module | Community: Apache 2.0 source for most components, with an increasing number of Pro/proprietary modules behind a paid subscription. Some newer extensions ship under a non-OSI source-available license. |
| **Moto** | `motoserver/moto` (official, multi-arch) | `pip install moto[server]` — primarily consumed as a Python library/decorator, with `moto_server` as the standalone HTTP mode | Apache 2.0 |
| **DynamoDB Local** | `amazon/dynamodb-local` (official, AWS) | Downloadable JAR from AWS; Maven artifact `com.amazonaws:DynamoDBLocal` | Proprietary "Amazon Software License" / DynamoDB Local terms — closed-source, free to use, redistribution restricted. |
| **MinIO** | `minio/minio` (official, multi-arch) | Static binaries for Linux/macOS/Windows; Helm chart; Operator for Kubernetes | AGPLv3 (re-licensed from Apache 2.0 in 2021). Commercial license available from MinIO Inc. |
| **Adobe S3Mock** | `adobe/s3mock` (official, multi-arch) | JAR on Maven Central (`com.adobe.testing:s3mock`); Testcontainers module; JUnit 4/5 rules | Apache 2.0 |

## Notes

**LocalStack** — The default choice when you want one process that speaks
many AWS APIs. The Community Edition covers the breadth of services you
typically need for application-level integration tests; Pro adds higher-fidelity
implementations and several services (RDS, EKS, full CloudWatch metrics, IAM
enforcement, Insights query execution, etc.). Be aware that the licensing has
drifted away from pure Apache 2.0 over time — newer features and extensions
may sit under source-available or commercial terms even if the original
`localstack` repo is still mostly Apache 2.0. The Docker image is the canonical
way to run it; the `localstack` CLI is just a wrapper that pulls and runs the
image.

**Moto** — Originally a Python-only library where you decorate a test
(`@mock_aws`) and the AWS SDK calls are intercepted in-process. `moto_server`
exposes the same in-memory implementations over HTTP so non-Python clients can
use it via the AWS SDK's `endpoint_url`. Coverage is wide and the
implementations are pragmatic — good for unit/integration tests but not a
fidelity-first emulator. Logs Insights in particular is mocked at the API
surface only.

**DynamoDB Local** — Published by AWS itself, so it is the gold standard for
DynamoDB behavior parity, including secondary indexes, transactions, TTL, and
Streams. The licensing is the main tradeoff: it is not OSS, redistribution is
restricted, and there is no source. LocalStack uses it internally for the
DynamoDB service. Ships as a JAR and as the `amazon/dynamodb-local` Docker
image.

**MinIO** — Not technically an AWS emulator; it is a production-grade
S3-compatible object store. For S3 work it is fast, has a real backing
filesystem, and supports erasure coding and distributed deployments. The
relicense to **AGPLv3** in 2021 means embedding it in proprietary products
typically requires the commercial license; for ephemeral local dev/test usage
the AGPL implications are usually acceptable, but worth checking with legal if
you ship it.

**Adobe S3Mock** — Smallest-footprint option for "I just need an S3 endpoint
in a test." Ships as a Docker image and as a Testcontainers/JUnit module for
JVM projects. No IAM, no notifications, no replication — those are explicit
non-goals. Apache 2.0, easy to drop into CI.

## Choosing between them

- **One AWS service, fastest path:** Use the dedicated emulator — DynamoDB
  Local for DynamoDB, Adobe S3Mock or MinIO for S3.
- **Several AWS services in one process, OSS-only:** Moto via `moto_server`.
- **Broadest service coverage and active commercial support:** LocalStack
  (Community for most cases; Pro if you specifically need Insights query
  execution, RDS, full CloudWatch, etc.).
- **Production-like S3 with real persistence, beyond just tests:** MinIO
  (mind the AGPLv3).
- **AGPL/proprietary licenses are a problem:** Stay with Moto, Adobe S3Mock,
  or LocalStack Community (and avoid Pro/extensions).
