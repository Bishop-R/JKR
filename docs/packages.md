# Distributable packages

The [GameData packages workflow](../.github/workflows/packages.yml) creates
Windows x64 and Linux x64 ZIPs with optimized client and dedicated-server
executables at the archive root. Extract the entire platform ZIP directly into
the installed game's `GameData` directory, beside `base/`, and launch the client.
Keep the dedicated server beside it for Create game and local devmap. Updating
the executables does not replace `jkr/` user files.

Each ZIP contains installation instructions, the project license, dependency
license files and a build manifest with the exact revision, target, compiler and
binary hashes. It contains no retail assets, personal configuration, generated
test files, source tree or debug symbols. Each artifact also provides SHA-256
checksums and a matching source snapshot ZIP; the source snapshot is for
contributors and is not needed in GameData.

## Build and verify

Once the workflow is on the default branch, run it manually with `source_ref`
set to the exact commit or branch to package. Pull requests changing the workflow
or packaging script also exercise it against the PR's base revision, avoiding
unmerged game changes. Download the resulting `JKR-windows-x64` and
`JKR-linux-x64` Actions artifacts. These are build artifacts with a 30-day
retention period, not automatically published GitHub Releases.

The Linux build uses Ubuntu 22.04, requiring glibc 2.35 or newer, ALSA and the
window-system/graphics-driver libraries. The Windows build targets MSVC with a
static C runtime, for Windows 10/11 x64. Drivers remain a system requirement.
Source checkout and source archives preserve the repository's line endings on
both platforms, including embedded WGSL used by exact-text shader patches.

After building, [package_client.py](../scripts/package_client.py) extracts each
ZIP into an isolated directory with spaces, starts the dedicated server on an
ephemeral loopback port and closes it through stdin EOF,
and launches the client from a different working directory with synthetic asset
markers. The client must find those assets and save `jkr/config.cfg`, then exit
on the intentionally invalid content before creating a window. No real servers,
retail files or personal profiles are used. This is an installation/startup check,
not a Windows GPU or gameplay certification.

The script can also package an existing clean build locally:

```sh
cargo build --locked --release --target x86_64-unknown-linux-gnu -p jkr-viewer -p jkr-dedicated
python3 scripts/package_client.py --source . --target x86_64-unknown-linux-gnu --platform linux-x64 --output target/packages --smoke-check
```

Build on the intended distribution baseline: packaging a binary built on a newer
Linux distribution does not lower its glibc requirement. The workflow is the
reference environment for the distributed Linux ZIP.
