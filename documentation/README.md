# OpenPDF maintainer guide

Internal notes for building, testing and releasing this repository. User-facing
documentation lives in [../readme.md](../readme.md) and the
[package README](../projects/packages/open-pdf/README.md).

## Project layout

| Path | Crate / package | Role |
|---|---|---|
| `projects/crates/pdf` | `open-pdf-core` | Product contract: re-exports the shared engine and reports the capability matrix. |
| `projects/crates/napi` | `open-pdf-napi` | Node-API mapping. Each export performs one operation and returns a JSON string. |
| `projects/packages/open-pdf` | `@wae/open-pdf` | TypeScript session, CLI, locale catalog and tests. |
| `projects/packages/open-pdf-<platform>-<arch>` | `@wae/open-pdf-…` | Prebuilt addon sidecars; `workspace:*` optional dependencies of `@wae/open-pdf`. |
| `projects/sub_projects` | `sub_projects` | Placeholder Rust workspace member, marked `publish = false`. |

Dependency direction:

```text
@wae/open-pdf (TypeScript) -> open-pdf-napi (Node-API) -> open-pdf-core -> document-pdf
```

PDF parsing, filters, writing and page operations live in the shared
`document-pdf` crate. `open-pdf-core` owns only the product surface and the
capability matrix. The TypeScript layer owns argument parsing, human output,
exit codes and the session API.

The Cargo workspace includes `projects/crates/*` and `projects/sub_projects`,
and excludes `projects/packages`. The pnpm workspace includes
`projects/packages/*`.

## Prerequisites

- Rust `1.90` or newer (workspace `rust-version`; edition 2024).
- Node.js `18` or newer.
- pnpm `10.28.0` (declared as `packageManager`).

The addon is built with cargo through the repository scripts. `@wae/wae` is a root dev dependency; `wae.config.ts` declares the native targets, the five platform packages, the TypeScript build and CI metadata.

## Build

```bash
pnpm install
pnpm run build
```

`pnpm run build` runs, in order:

1. `cargo build -p open-pdf-napi` (`pnpm run build:napi`)
2. `pnpm --filter @wae/open-pdf run build` (`pnpm run build:ts`, which runs `tsc`)

To build one side only:

```bash
cargo build -p open-pdf-napi
pnpm --filter @wae/open-pdf run build
```

To build and stage a per-platform addon, use `pnpm exec wae native build` (all targets declared in `wae.config.ts`) or `pnpm exec wae native build --target <target>`. The staged file is `projects/packages/open-pdf-<target>/open-pdf.<target>.node`.

The TypeScript package resolves the addon from
`@wae/open-pdf-${process.platform}-${process.arch}`, or from the path in
`OPEN_PDF_NATIVE` when that variable is set. Set `OPEN_PDF_NATIVE` to a locally
built `.node` file to test the native build without installing a platform
package. If no addon is found, SDK calls fail with
`capability/not-implemented` instead of returning invented results.

## Test and checks

```bash
cargo test -p open-pdf-core
cargo check -p open-pdf-napi
pnpm run typecheck
pnpm --filter @wae/open-pdf test
```

Root aliases:

| Script | Runs |
|---|---|
| `pnpm run test:rust` | `cargo test` |
| `pnpm run check:rust` | `cargo fmt -p open-pdf-core -p open-pdf-napi -- --check`, clippy with `-D warnings`, then tests for both crates |
| `pnpm run typecheck` | `pnpm -r --filter "./projects/packages/*" run typecheck` |
| `pnpm run test` | `pnpm -r --filter "./projects/packages/*" run test` |
| `pnpm run fmt` | `cargo fmt --all` |
| `pnpm run doc` | `cargo doc --no-deps --all-features` |

`pnpm --filter @wae/open-pdf test` compiles `src` and `tests` with `tsc -p tsconfig.test.json` and then runs `node --test dist-test/tests/smoke.test.js`. The smoke tests cover the format registry,
page-list parsing, the stable error code set and, when an addon is present, a
few native round trips. Native PDF behaviour is covered by the Rust integration
tests.

CI is `.github/workflows/rust.yml`: on pushes and pull requests to `master` and
`dev`, it runs `cargo build --release` and `cargo test --release` on Ubuntu,
macOS and Windows with the nightly toolchain. `.github/workflows/publish-npm.yml`
handles tagged npm releases.

## Release and publish

Current facts, verified in the repository:

- `projects/packages/open-pdf` is publishable (no `private` flag) at version
  `0.0.0`. Published files are `dist` and `locales`; the CLI binary is
  `dist/cli.js`.
- The five `projects/packages/open-pdf-<platform>-<arch>` packages ship the
  prebuilt addon resolved by `src/native.ts`. They are `workspace:*` optional
  dependencies of `@wae/open-pdf`.
- `wae.config.ts` declares the `open-pdf-napi` crate, the five native targets
  and the TypeScript build.
- `.github/workflows/publish-npm.yml` builds and stages the five addons, builds
  the TypeScript surface, and publishes through `nifty publish` on `v*` tags
  with `id-token: write` and the `NPM_PUBLISH` environment.
- `nifty.config.ts` declares the Trusted Publisher target (`nyar-os/OpenPDF`,
  `publish-npm.yml`, `NPM_PUBLISH`).
- `.secrets/nifty.json` is a local-only npm TOTP file (`npmTotpSecret`).
  `.secrets/` is git-ignored. Never commit it, print it or copy it into CI
  logs.
- `projects/sub_projects` sets `publish = false`; its local `p` script exists
  but the crate is marked non-publishable.

Local placeholder publishing uses `nifty publish --placeholder`; real versions
come from the tagged CI workflow.

## Shared dependency pins

`Cargo.toml` pins every shared git dependency to a full 40-character revision:

- `document-pdf` and `data-compression` both come from
  `https://github.com/oovm/rust-boost` at the same revision.
- `wae` comes from `https://github.com/doki-land/wae-framework` at its own
  revision.

Keep `document-pdf` and `data-compression` on the same `rust-boost` revision and
move both together when upgrading. Never replace a full revision with a branch
or tag. After changing a pin, run `cargo test -p open-pdf-core` and
`cargo check -p open-pdf-napi` before touching the TypeScript package.

`open-pdf-core` depends on `document-pdf`; `data-compression` is a
dev-dependency used by the core tests. `open-pdf-napi` depends on
`open-pdf-core` and `wae`.

## stdin and byte APIs

The CLI accepts `--stdin` only on the three readers:

- `open-pdf inspect --stdin`
- `open-pdf extract-text --stdin`
- `open-pdf extract-images --stdin --out-dir <dir> [--name <stem>]`

A path and `--stdin` are mutually exclusive. The CLI reads standard input to
the end as raw bytes; empty input fails with `open-pdf.cli.stdin-empty`.
`extract-images --stdin` uses `--name` as the file stem and defaults it to
`stdin`. Writers have no stdin mode: they always take paths.

The SDK exposes the same readers over bytes the caller already owns:

- `inspectBytes(bytes)`
- `extractTextBytes(bytes)`
- `extractImagesBytes(bytes, outDir, stem)`

These accept a `Uint8Array` or `Buffer` and buffer the whole document in memory.
There is no streaming interface.

## Documentation map

| File | Audience |
|---|---|
| [../readme.md](../readme.md) | Product landing page. |
| [../projects/packages/open-pdf/README.md](../projects/packages/open-pdf/README.md) | npm registry page. |
| [agents.md](./agents.md) | Agent-facing command, flag, report and error reference. |
