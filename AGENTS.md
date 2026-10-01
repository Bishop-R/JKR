# Working on JKR

JKR is a native Rust engine, client and dedicated server for Jedi Academy
multiplayer. Read [docs/status.md](docs/status.md),
[docs/architecture.md](docs/architecture.md) and the relevant page in the
[project wiki](docs/README.md) before changing code.

## Compatibility and architecture

- Preserve OpenJK multiplayer `codemp` movement, weapon/saber timing and
  animation behavior. Gameplay changes need reference evidence, not visual guesses.
  Cover integer command steps of 8/7/4/3 ms (125/142/250/333 FPS).
- Protocol 26 is fixed. Encode/decode changes in `jkr-protocol` or `jkr-network`
  need OpenJK-emitted or captured byte fixtures. Never infer wire correctness
  from a successful connection alone. Use `codemp`, not the single-player `code` tree.
- Keep legacy formats, identifiers and limits in compatibility adapters.
  Engine world ownership and presentation must remain independent of the wire.
- Community PK3 maps and models are compatibility inputs, just like retail assets.
  Modern rendering and UI must preserve gameplay and content semantics.
- Write small, named modules with rustdoc for public APIs. Keep new viewer work
  out of `crates/jkr-viewer/src/main.rs`; use focused modules instead.
- Avoid new allocation, locking and quadratic work in per-frame paths. Measure
  performance-sensitive changes in release builds; 500+ FPS is a target, not a
  blanket claim about current performance.

## Verification and scope

- Follow [development.md](docs/development.md). Before handing off a change, run
  `cargo fmt --all --check`, `cargo build --locked --workspace` and
  `cargo test --locked --workspace`. Explain any check that could not run.
- Passing Cargo tests currently does not establish gameplay parity: the repository
  has no bundled regression suite. Use focused external evidence for affected behavior.
  Do not add fixtures, test infrastructure or large reports without task authorization.
- Only exercise servers you control without human players. Do not automate chat
  messages. Use an unobtrusive name and stop only processes started for your check.
- Never commit retail game data, downloaded PK3s, credentials or personal paths.
  Put scratch evidence under `target/parity-reports/`, not `/tmp`, and remove it
  when finished. Keep a concise verification summary in the change description.
- Respect the requested scope and existing work. Do not discard unrelated changes,
  rewrite history or publish without authorization. A status page is context, not
  permission to start a new task.
- Follow the [branch, commit and pull request
  rules](docs/development.md#branches-commits-and-pull-requests): one topic per
  branch and pull request, never commit to `main`.

## Documentation is part of the change

- When behavior, architecture, configuration or build instructions change, update
  the relevant wiki page in the same change and commit. Add new pages to the index.
- Update `docs/status.md` when verification, a known limitation or agreed priorities
  change. Record the revision, environment, evidence and limits of new verification.
- Distinguish implemented, verified and planned. Code presence is not proof of
  correctness; a smoke check is not full compatibility or performance certification.
- Keep durable decisions and current facts in the wiki. Do not append chat transcripts,
  session diaries, speculative completion claims or duplicate task histories.
- Documentation-only changes belong with their subject. Unchanged behavior does not
  require a ceremonial wiki update. Review affected links when moving code or docs.
- `AGENTS.md` is the shared instruction source; `CLAUDE.md` points here. Keep
  operational facts in the wiki so assistant-specific files cannot drift apart.

Finish with a short report of what changed, how it was verified and what remains
unverified. Commit and push only within the user's authorized scope.
