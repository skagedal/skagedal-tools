# appstore: App Store Connect from the command line

Implements [#86](https://github.com/skagedal/skagedal-tools/issues/86).

iggybilly's release talks to App Store Connect in three places, all in
shell. `local/make-ios-signing` has a distribution certificate issued,
registers the bundle id and replaces the provisioning profile.
`ci/upload-to-testflight` hands the `.ipa` to `xcrun altool`.
`ci/await-testflight-build` polls until the build shows up, because an
upload Apple takes can still be refused a minute later with nothing but
an email to say so. Underneath two of them is `ci/appstore-api.sh`, which
mints an ES256 JWT with `openssl`, unpacks a DER signature into raw
`r||s` by hand with `asn1parse`, `sed` and `xxd`, and wraps `curl` so that
Apple's error sentences reach the log.

It works, and the comments explain every trap it fell into. But it is the
kind of code that is only correct because each bug has already happened
once, and the next app with a release — jikido is the obvious one — would
copy all of it. This moves the App Store Connect half into a Rust crate
and CLI in this repository, and leaves iggybilly's scripts holding only
what is genuinely iggybilly's.

## What exists already

The issue asks whether there is a crate for this. There are three, and a
CLI, and none of them is the right thing to depend on:

- **[`app-store-connect`](https://docs.rs/app-store-connect/)**, from
  Gregory Szorc's `apple-platform-rs` — the same project as `rcodesign`.
  It is good, has a CLI, and covers the JWT, bundle ids, certificates,
  devices, profiles and the notary API. It does not cover apps or builds,
  which is the half the CI scripts need, and its last release was
  November 2024.
- **[`appstoreconnect`](https://crates.io/crates/appstoreconnect)** covers
  profiles, certificates and bundle ids. Same gap, fewer users.
- **[`cnctd_appstore`](https://crates.io/crates/cnctd_appstore)** covers
  TestFlight, builds and submissions. It is one release old with a
  couple of hundred downloads.
- **[`asc`](https://github.com/rorkai/App-Store-Connect-CLI)** is a Go
  CLI, in Homebrew as `asc`, which covers essentially the whole API,
  including uploading a build without Xcode. It is the most complete
  option by a distance. It also has telemetry on by default and a very
  large surface, and it would put the release pipeline on a third party's
  release cadence for what is, for these apps, six endpoints.

So the plan is a small crate of our own that does exactly what these
releases need, with the JWT and the error handling done once and tested,
and a CLI shaped around the release steps rather than around the API's
resource list. If that turns out to be the wrong trade, `asc` is the
fallback, and the open questions say so.

## Functionality

### Credentials

An App Store Connect API key is three things: a key id, an issuer id, and
a `.p8` private key. `appstore` reads them from the environment under the
names iggybilly already uses, so nothing in its secrets changes:

| Variable | |
|----------|--|
| `APP_STORE_CONNECT_KEY_ID` | the key id |
| `APP_STORE_CONNECT_ISSUER_ID` | the issuer id |
| `APP_STORE_CONNECT_PRIVATE_KEY` | the `.p8` itself, as CI has it |
| `APP_STORE_CONNECT_KEY_FILE` | a path to the `.p8`, as a laptop has it |

The private key comes from `APP_STORE_CONNECT_PRIVATE_KEY` if that is
set, else from the file. Missing any of it is an error naming the
variable that is missing.

On a Mac, `appstore auth` stores all three in the keychain instead, the
way `trafikverket auth` stores its key, and the environment then only
needs to override them. The `.p8` is read from a path or stdin and never
echoed. `appstore auth status` says where each value is coming from, and
`appstore auth forget` removes them.

A token is minted for every request. They last ten minutes and cost a
signature to make, so there is no expiry to track and a twenty-minute
wait cannot outlive its credentials.

### Commands

    appstore builds upload <IPA>
    appstore builds wait --bundle-id <ID> --build <NUMBER> [--timeout 20m]
    appstore bundle-ids ensure <IDENTIFIER> --name <NAME> [--platform IOS]
    appstore certificates list [--type DISTRIBUTION]
    appstore certificates create --csr <FILE> [--type DISTRIBUTION] --out <FILE>
    appstore profiles replace --name <NAME> --bundle-id <IDENTIFIER>
                              --certificate <ID> [--type IOS_APP_STORE] --out <FILE>
    appstore api <METHOD> <PATH> [--data <JSON>]
    appstore token

**`builds upload`** sends an `.ipa` to App Store Connect, which is what
puts it in TestFlight. The app, the version and the build number are read
out of the `.ipa`'s `Info.plist`, so they cannot disagree with what was
built. It prints progress on stderr and, once Apple has the whole file,
the id of the upload. It does not need Xcode, which also means it does
not need a macOS runner for this step.

**`builds wait`** finds the app by bundle id and polls every 30 seconds
for the build with that number. When the build appears it prints its
processing state and exits 0. `INVALID` or `FAILED` exits 1 with the same
explanation `await-testflight-build` gives — Apple sends the reason by
email, and a filter may be keeping it out of the inbox. Reaching the
timeout exits 1 saying that a build that never appears is what a refused
binary looks like from outside. No app record for the bundle id is an
error saying one has to be made by hand, since the API cannot.

**`bundle-ids ensure`** looks the identifier up with no platform filter —
one registered as `UNIVERSAL` is still the one wanted — registers it if
absent, and prints its id either way.

**`certificates create`** submits a CSR, writes the issued certificate as
PEM, and prints its id. Before it asks, it lists the account's existing
certificates of that type on stderr, since the account can hold very few,
and if Apple refuses it is usually because one of those has to be revoked.
Generating the private key and CSR, and bundling a `.p12`, stay with
`openssl` in the caller: they are not App Store Connect, and the key
should be made where it is going to be kept.

**`profiles replace`** deletes any profile with that name and makes a new
one naming the bundle id and the certificate, written to `--out`. A
profile can be remade from its certificate at any time, so replacing it
outright is safe in a way that replacing a certificate is not.

**`api`** signs a request, sends it, and prints the JSON reply — the
escape hatch for everything else, and the first thing to reach for when
a field is not called what the docs say. **`token`** prints a fresh JWT,
for use with `curl`.

Every command that returns data prints a short human line by default and
the JSON object with `--json`.

### Errors

When App Store Connect answers with `errors`, every entry is printed as
`title: detail`, and the command exits 1. A reply that is not JSON is
printed, truncated to 2000 bytes. These are the two cases `api()` in
`appstore-api.sh` was written to surface, and they stay surfaced.

## Implementation

### The crate

A new workspace member, `appstore/`, library and binary in one crate:

```
appstore/
  Cargo.toml
  README.md
  src/
    lib.rs           the client, for another tool to use
    main.rs          clap, dispatch, exit codes
    cli.rs
    credentials.rs   environment, key file, keychain
    keychain.rs      /usr/bin/security, as in trafikverket
    token.rs         ES256 JWT
    client.rs        reqwest, errors, the endpoint override
    model.rs         the JSON:API shapes these commands touch
    builds.rs        upload and wait
    signing.rs       bundle ids, certificates, profiles
    ipa.rs           Info.plist out of an .ipa
  tests/
```

It goes into `INSTALLED_RUST_TOOLS` in `shared.sh` and the tools table in
the root `README.md`:

    | [appstore](appstore/) | Upload builds, wait for TestFlight, and manage signing through the App Store Connect API |

Dependencies, all added to `[workspace.dependencies]`: `jsonwebtoken`
for the token, since it takes a PKCS#8 EC key as PEM directly and emits
the raw `r||s` signature JOSE wants, which is the whole of what the shell
version does by hand; `zip` to reach into the `.ipa`; whichever digest
crate the upload checksum calls for; and the existing `reqwest`, `tokio`,
`serde`, `serde_json`, `plist`, `clap`, `anyhow` and `chrono`.

`keychain.rs` is copied from `trafikverket` with the service name
`appstore` and three accounts, `key-id`, `issuer-id` and `private-key`.
If a third tool wants the keychain it becomes a shared crate like
`skagedal-dirs`; two copies is not yet worth it.

`APPSTORE_API_ENDPOINT` overrides `https://api.appstoreconnect.apple.com`,
as `TRAFIKVERKET_API_ENDPOINT` does, so tests can run against a local
stub. The token claims are `iss`, `iat`, `exp = iat + 600` and `aud =
"appstoreconnect-v1"`, with `kid` in the header. A test signs a token
with a key generated in the test and verifies it with the matching public
key, which is the check the shell version never had.

### The endpoints

- `GET /v1/apps?filter[bundleId]=…`
- `GET /v1/builds?filter[app]=…&limit=20&sort=-uploadedDate`, matching
  `attributes.version` in code rather than trusting a filter to mean what
  it looks like, as the shell does.
- `GET`/`POST /v1/bundleIds`, `GET`/`POST /v1/certificates`,
  `GET`/`POST`/`DELETE /v1/profiles`, with the bodies
  `make-ios-signing` sends today.
- The build upload API Apple added in 2025: `POST /v1/buildUploads` with
  the version, build number and platform against the app;
  `POST /v1/buildUploadFiles` describing the `.ipa`, whose reply carries
  upload operations — URL, method, offset, length and headers per part;
  one request per operation with that slice of the file; then marking the
  file uploaded with its checksum, and polling the build upload until
  Apple has taken it. The attribute names are to be taken from Apple's
  reference when this is written rather than from this paragraph, which
  is from the WWDC25 session, and `appstore api` is how to check them
  against a real account first.

### In iggybilly

Its own change, in its own repository, once `appstore` is on `main` here:

- `ci/appstore-api.sh` is deleted.
- `ci/upload-to-testflight` becomes the `.ipa` lookup followed by
  `appstore builds upload "$ipa"`. The key is no longer written into
  `~/.appstoreconnect/private_keys`, since nothing looks for it there.
- `ci/await-testflight-build` becomes one line,
  `appstore builds wait --bundle-id tech.skagedal.iggybilly --build "$BUILD"`,
  and its long explanation moves into `appstore`'s messages.
- `local/make-ios-signing` keeps its `openssl` and `gh` steps and its
  prose, and its three `api` sections become `appstore certificates
  list`, `certificates create`, `bundle-ids ensure` and `profiles
  replace`. `openssl`, `gh` and `appstore` replace `curl`, `jq` and `xxd`
  in its tool check.
- `.github/workflows/release.yml` installs the tool before those steps:

  ```yaml
  - name: Install appstore
    run: cargo install --locked --git https://github.com/skagedal/skagedal-tools --rev <sha> appstore
  ```

  pinned to a commit, so a change here cannot break a release there
  without a deliberate bump.
- `mobile/README.md`'s "Releasing" section points at `appstore auth` for
  the local setup.

## Open questions

- **`asc` instead.** Everything above is a real amount of work to own a
  handful of endpoints. `brew install asc` and three command lines would
  replace the same shell today. The case for building is that the release
  pipeline is small, rarely changes, and should not depend on a large,
  fast-moving third-party tool with telemetry on by default; the case
  against is everything `asc` already handles that we will meet later
  (screenshots, metadata, App Store submission). Worth one evening of
  trying `asc` against iggybilly's account before starting.
- **Installing in CI.** `cargo install --git` compiles the tool on every
  release, which is a minute or two on a macOS runner. Publishing release
  binaries from this repository would be faster but is machinery this
  repository does not have yet.
- **Webhooks.** Apple added webhooks for build processing alongside the
  upload API. `builds wait` polls because a workflow has nowhere to
  receive a webhook, but a longer-lived setup could use one.
- **The name.** `appstore` is short, but undersells TestFlight and
  signing. `app-store-connect` says what it is, and is also the name of
  Szorc's crate and its binary, which would be confusing on a machine
  that has both.
