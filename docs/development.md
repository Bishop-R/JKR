# Development

Read [AGENTS.md](../AGENTS.md) and [architecture.md](architecture.md) first.
Linux is the verified development platform; Windows runtime validation remains
open. The manifests declare Rust 1.88 or newer and edition 2024.

## Build and checks

Install Rust/Cargo, a C/C++ toolchain, CMake and pkg-config. Linux also needs
ALSA, Wayland and XKB development libraries and a working graphics driver.
See [workspace dependencies](../Cargo.toml) and the
[viewer manifest](../crates/jkr-viewer/Cargo.toml) when diagnosing missing libraries.

From the repository root:

```sh
cargo fmt --all --check
cargo build --locked --workspace
cargo test --locked --workspace
cargo build --locked --release -p jkr-viewer -p jkr-dedicated
```

The first formatting command requires the rustfmt component. Use `-j2` to limit
build parallelism on constrained machines. `CARGO_TARGET_DIR` can put build
artifacts outside the checkout; do not commit binaries or generated output.

The repository currently has no bundled regression suite. `cargo test` checks
its test/doc-test build targets but is not evidence of gameplay or wire parity.
Some source comments refer to external reference harnesses; those paths are not
available in this checkout. Do not silently skip required compatibility evidence
or claim those checks ran. Record a verification gap if the reference is unavailable.

## Verification appropriate to the change

| Change | Evidence required in addition to workspace checks |
| --- | --- |
| Movement, combat or animation | OpenJK `codemp` reference comparison; command steps 8/7/4/3 ms |
| Wire encode/decode | Captured or reference-emitted bytes, including affected edge cases |
| Rendering or shaders | Actual GPU startup and affected map/pass; compare images when appearance changes |
| Frame-loop performance | Release frame-time measurements with hardware, map, settings and workload |
| Server behavior | Isolated local server and relevant native/legacy client scenarios |
| Documentation | Check commands against source, relative links and the accuracy of status claims |

Use a server you control with no human players. Keep evidence under
`target/parity-reports/`, delete large scratch reports afterwards, and record a
compact account of the scenario, revision, expected result and observed result.
Never bundle retail assets in evidence or source commits.

## Change workflow

Check `git status` and existing work before editing. Follow the requested scope,
inspect the affected module and reference behavior, then implement and verify.
Update the relevant wiki pages in the same change. Record durable decisions and
limitations; use commit descriptions for the change narrative.

A handoff should state what changed, the exact checks and their results, and
what remains unverified. Do not label a feature complete merely because it builds.
Use [status.md](status.md) for current agreed priorities, without treating that
list as authorization to start unrelated work.
