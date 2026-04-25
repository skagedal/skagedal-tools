# AWS Local Emulation Tools

A comparison of tools that emulate AWS services locally for development and
testing.

## Tools

| | [LocalStack](https://www.localstack.cloud/) | [Moto](https://github.com/getmoto/moto) | [DynamoDB Local](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DynamoDBLocal.html) | [MinIO](https://min.io/) | [Adobe S3Mock](https://github.com/adobe/S3Mock) |
|---|---|---|---|---|---|
| **Scope** | Multi-service | Multi-service | DynamoDB | S3 | S3 |

## Subsystem coverage

✓ = solid · ◐ = partial / API-only · ✗ = not supported · — = out of scope

| Subsystem | LocalStack | Moto | DynamoDB Local | MinIO | Adobe S3Mock |
|---|:-:|:-:|:-:|:-:|:-:|
| **S3** | ✓ | ✓ | — | ✓ | ◐ |
| **DynamoDB** | ✓ | ✓ | ✓ | — | — |
| **SSM Parameter Store** | ✓ | ✓ | — | — | — |
| **CloudWatch Logs Insights** | ◐ (✓ in Pro) | ◐ | — | — | — |

Notes on the partials:

- **S3 / Adobe S3Mock** — objects, multipart, presigned URLs work; no IAM,
  no notifications, no replication.
- **CloudWatch Logs Insights / LocalStack Community** — API surface present,
  but query execution against ingested logs is Pro-only.
- **CloudWatch Logs Insights / Moto** — `StartQuery` / `GetQueryResults`
  return canned responses; the query language is not actually executed.

## Distribution

| | LocalStack | Moto | DynamoDB Local | MinIO | Adobe S3Mock |
|---|:-:|:-:|:-:|:-:|:-:|
| **Official Docker image** | ✓ `localstack/localstack` | ✓ `motoserver/moto` | ✓ `amazon/dynamodb-local` | ✓ `minio/minio` | ✓ `adobe/s3mock` |
| **Standalone binary / JAR** | ✗ | ✗ | ✓ JAR | ✓ binary | ✓ JAR |
| **Library / in-process mode** | ✗ | ✓ Python | ✗ | ✗ | ✓ JVM |
| **Helm chart** | ✓ | ✗ | ✗ | ✓ | ✗ |
| **Testcontainers module** | ✓ | ✓ | ✓ | ✓ | ✓ |

## Licensing

| | LocalStack | Moto | DynamoDB Local | MinIO | Adobe S3Mock |
|---|:-:|:-:|:-:|:-:|:-:|
| **Open source** | ◐ | ✓ | ✗ | ✓ | ✓ |
| **License** | Apache 2.0 core; Pro & some extensions source-available / commercial | Apache 2.0 | Amazon Software License (proprietary) | AGPLv3 | Apache 2.0 |
| **Free for commercial use** | ✓ (Community) | ✓ | ✓ | ◐ (AGPL obligations) | ✓ |
