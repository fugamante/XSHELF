# XSHELF Packaging

This packaging track prepares a Homebrew-first XSHELF CLI distribution. It is
a local release-candidate workflow, not evidence that a formula, bottle, or
binary release asset has been published.

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
uses GitHub's native `macos-15-intel` runner, checks out the exact candidate
revision dispatched by GitHub, requires a non-translated `x86_64` process,
reproduces the archive byte-for-byte, and uploads bounded validation evidence.
Ordinary pull-request and push events do not start this lane.

The `2026.08.25` implementation candidate passed this lane at source
`727afec7a2214704fb9cb6e686872325e765afd9` in workflow run `32986914305`.
Its evidence records native, non-translated Intel execution, byte-identical
archive reproduction, checksum and provenance verification, package relocation
and clean-home operation, and the complete isolated Homebrew lifecycle. This is
predecessor candidate evidence after later commits; it does not validate a
different branch head or claim a published, signed, or notarized release.

For final review, dispatch the lane after the candidate head is frozen and
require the retained artifact's `source_revision` to equal that exact head.
Intel evidence is retained for the repository-supported 90-day review horizon.
Keep the final checksum and provenance in that artifact rather than creating a
follow-up documentation commit that would invalidate the source binding.

Archive creation fails on a dirty source tree by default. `--allow-dirty` is an
explicit local-development lane that records `source_dirty: true` plus a
content fingerprint; such an artifact is never release-ready.

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

Local checksums and provenance do not establish signing, notarization,
publication, or immutable remote identity. A future release must separately
authorize and verify those steps, attach both thin archives to the intended
immutable tag, render the formula from the attached assets, run `brew test`,
and confirm real install, upgrade, uninstall, and state preservation behavior
against the published formula. The focused local lifecycle test is a package
layout simulation, not a substitute for that Homebrew validation.
