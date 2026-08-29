# XSHELF Packaging

This packaging track builds and validates the native XSHELF CLI distribution.
The unsigned `v2026.08.25` macOS archives and checksum manifest are published;
no Homebrew formula or bottle is published.

## Published v2026.08.25 Assets

Release: https://github.com/fugamante/XSHELF/releases/tag/v2026.08.25

| Asset | SHA-256 |
|---|---|
| `xshelf-2026.08.25-aarch64-apple-darwin.tar.gz` | `9865e5440a5b6554cea952b630f8f3c26c6eabd64fdf39cb2e53c850306c873f` |
| `xshelf-2026.08.25-x86_64-apple-darwin.tar.gz` | `e908105a9767d60cb057e0a9469cd2c49b2b750c7082435a468980d29ca9fd2e` |
| `SHA256SUMS` | `3e56cb544b87d7e59a0d098941c1e4c4e3ab59f61c718aee34122b9c33b9beaf` |

The annotated tag object
`49318430f6163bd640882493d1e163666c53cb43` peels to immutable packaged
source `210b3b524c01f4dc673244077f02b53d39cedcda`. Protected-main controller
`8c2a16b937dc79d49ecc11dd2da94eb63ebd4eaf` supplied the canonical native
reproduction policy without changing that source provenance.

These assets are not Developer ID signed or notarized. The ARM64 executable has
linker-generated ad-hoc signing state; the Intel executable is unsigned. The
README embedded in both immutable archives is the tag-time snapshot, so its
pre-publication wording is historical rather than the current release status.

## Product Boundary

The package owns:

- one native executable installed as `xshelf`;
- `xs` and `cx` symlinks to that executable;
- the four canonical default schemas under `share/xshelf/schemas`;
- `xshelf(1)`, `xs(1)`, and `cx(1)`;
- license, README, manifest, provenance, and SHA-256 metadata.

The package does not own or install `cxops`, provider configuration, Desktop
launchers, shell functions, shell profiles, project `.cx` / `.codex` state, or
home-directory `.cx` / `.codex` state. `xshelf launch` continues to discover an
independently installed `cxops` runtime and reports its existing remediation
when none is available.

Schema discovery preserves an existing project-local or home
`.cx/schemas` registry before falling back to packaged defaults. `CX_DATA_DIR`
may point advanced packaging tests at another read-only XSHELF data root; the
installer does not set it or write to that location.

## Build Archives

The default build requires both supported Rust targets to be installed and
never installs a missing target automatically:

```bash
./scripts/build_packages.sh
```

Build one architecture explicitly when validating on a single-target host:

```bash
./scripts/build_packages.sh --target aarch64-apple-darwin
./scripts/build_packages.sh --target x86_64-apple-darwin
```

Outputs default to `.cx/packages`, and compilation uses the ignored
`.cx/package-target` directory. The build pins Rust `1.95.0`, uses the locked
dependency graph, resolves Cargo and rustc as absolute paths from that same
rustup toolchain, and rejects mismatched identities or inherited compiler
wrappers. Python 3.11 or newer is required for the packaging metadata tools.
The build also sets the macOS 11 deployment floor, disables incremental
compilation, and remaps checkout and build-home paths before verifying they are
absent from the executable string table.

Each thin archive is named:

```text
xshelf-<VERSION>-<target>.tar.gz
```

It contains:

```text
xshelf-<VERSION>-<target>/
  bin/xshelf
  bin/xs -> xshelf
  bin/cx -> xshelf
  share/xshelf/schemas/*.schema.json
  share/man/man1/{xshelf,xs,cx}.1
  LICENSE
  README.md
  manifest.json
  provenance.json
```

Archive headers have deterministic ordering, ownership, modes, and timestamps.
The `xshelf-package-manifest.v1` record inventories installed paths and hashes.
The `xshelf-package-provenance.v1` record identifies the version, source
revision and content fingerprint, dirty state, target, Rust compiler,
Cargo version, deployment floor, archive format, and explicit
unsigned/unnotarized state.
Per-archive sidecars and `SHA256SUMS` contain SHA-256 digests. These new formats
are fixture-checked by `test/package_release_test.py`; incompatible changes
require a versioned format and migration note.

## Verify And Exercise

Verify a sidecar without extracting its archive:

```bash
python3 scripts/package_release.py verify \
  .cx/packages/xshelf-2026.08.25-aarch64-apple-darwin.tar.gz
```

Run the focused packaging lifecycle suite:

```bash
python3 test/package_release_test.py
python3 test/reproduce_packages_test.py
cd rust/cxrs
cargo test --locked --test package_runtime -- --test-threads=1
```

The tests cover deterministic archives, checksum tampering, relocation, clean
home state, runtime operation without Rust/Cargo on `PATH`, aliases, schemas,
simulated package-layout upgrade/uninstall preservation, caller-repository version isolation,
embedded contract validation, and complete formula rendering. Native Intel
evidence requires an Intel host; Rosetta and static architecture inspection are
not substitutes.

Maintainers without Intel hardware can manually dispatch `cxrs-compat` on the
candidate branch. Its `intel-package` job runs only for `workflow_dispatch`,
uses GitHub's native `macos-15-intel` runner, and requires a non-translated
`x86_64` process. By default it packages the dispatched workflow commit. An
optional `package_revision` input accepts only a full commit ID, allowing a
reviewed newer harness to reproduce an older immutable source tag without
retagging it. The controller checkout and packaged source revision are recorded
separately. Both clean builds and archive provenance bind to the selected
package revision, and the job uploads bounded validation evidence. Ordinary
pull-request and push events do not start this lane.

The published `2026.08.25` Intel archive was reproduced by controller
`8c2a16b937dc79d49ecc11dd2da94eb63ebd4eaf` from immutable source
`210b3b524c01f4dc673244077f02b53d39cedcda` in workflow run `33234570375`.
Retained artifact `9709817894` records native `x86_64`, `translated=0`, two
clean canonical compilations, byte-identical archive and provenance evidence,
package relocation and clean-home operation, and the complete isolated
Homebrew lifecycle. The hosted evidence retains its repository-supported
90-day review horizon; it is build evidence rather than Developer ID signing,
notarization, or Homebrew publication.

Archive creation fails on a dirty source tree by default. `--allow-dirty` is an
explicit local-development lane that records `source_dirty: true` plus a
content fingerprint; such an artifact is never release-ready.

For release-candidate reproducibility, use the canonical native harness rather
than rebuilding against one shared Cargo target directory:

```bash
python3 scripts/reproduce_packages.py \
  --source-repo . \
  --revision "$(git rev-parse HEAD)" \
  --target aarch64-apple-darwin \
  --approved-prefix /tmp \
  --canonical-root /tmp/xshelf-canonical-native \
  --output-dir /tmp/xshelf-native-evidence
```

The canonical root must be one direct child of the approved temporary prefix.
An existing root is accepted only when its marker exactly names that resolved
path and the `xshelf-canonical-native.v1` policy. A sibling lock prevents
concurrent use. Both builds reuse the same absolute source, HOME, Cargo, temp,
XDG, output, and checkout-owned target paths, but the harness removes and
verifies all owned state before each clone. It refuses dirty or wrong revisions,
compares archive, executable, UUID, platform-observed linker signing state,
CDHash when present, manifest, and provenance bytes, writes
`xshelf-native-reproduction.v1` evidence outside the root, and removes the
canonical build root afterward. The accepted linker states are ad-hoc with a
CDHash or unsigned with a null CDHash; ambiguous, invalid, Developer ID, and
other unexpected states fail closed. This linker observation is build evidence,
not Developer ID release signing, and does not change the embedded
`signed=false` or `notarized=false` package provenance fields.

Host, SDK, linker, UUID-policy, and build-policy identities remain reproduction
evidence rather than fields in `xshelf-package-provenance.v1`. This preserves
the current archive contract and avoids making host-specific observations part
of otherwise identical release bytes.

## Draft Homebrew Formula

`packaging/homebrew/xshelf.rb.in` is intentionally a template. Render it only
from immutable versioned asset URLs and their actual SHA-256 values:

```bash
python3 scripts/render_formula.py \
  --output .cx/packages/Formula/xshelf.rb \
  --version 2026.08.25 \
  --arm-url https://example.invalid/releases/download/v2026.08.25/xshelf-2026.08.25-aarch64-apple-darwin.tar.gz \
  --arm-sha256 <64-hex-digest> \
  --intel-url https://example.invalid/releases/download/v2026.08.25/xshelf-2026.08.25-x86_64-apple-darwin.tar.gz \
  --intel-sha256 <64-hex-digest>
```

The renderer rejects incomplete hashes, non-HTTPS or credential-bearing URLs,
unexpected asset names, and URLs that are not anchored to `v<VERSION>`. Do not
publish the rendered formula until both archives have passed their native
runtime checks and publication is separately authorized.

For an isolated local Homebrew prefix, render a clearly marked fixture from the
available local archives without weakening the production renderer:

```bash
python3 scripts/render_formula_fixture.py \
  --output /tmp/xshelf-formula/Formula/xshelf.rb \
  --version 2026.08.25 \
  --arm-archive .cx/packages/xshelf-2026.08.25-aarch64-apple-darwin.tar.gz
```

An omitted architecture receives an unavailable `file://` URL and a zero hash,
so attempting to use the fixture on that architecture fails closed. Local
fixtures must remain outside taps intended for publication. A real isolated
test must use a temporary Homebrew clone and temporary HOME, cache, logs, temp,
and XDG paths; verify its prefix before installation and compare the installed
Homebrew configuration hashes afterward.

The hosted Intel lane applies this same isolation boundary, exercises install,
`brew test`, revision upgrade, relocation, and uninstall, and fails if either
the runner's Homebrew configuration or the isolated clone configuration drifts.

## Release Boundary

Local checksums and provenance alone do not establish signing, notarization,
publication, or immutable remote identity. The `v2026.08.25` publication pass
separately attached both verified thin archives and `SHA256SUMS` to the existing
immutable tag and verified their public download bytes. It did not authorize
Developer ID signing, notarization, or Homebrew publication.

Any future formula publication must be separately authorized, rendered from
the immutable attached assets, and validated with real install, `brew test`,
upgrade, uninstall, and state-preservation behavior. The focused local
lifecycle test is a package-layout simulation, not a substitute for that
published-formula validation.
