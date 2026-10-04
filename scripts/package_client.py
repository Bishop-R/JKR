#!/usr/bin/env python3
"""Package a clean, already-built JKR revision into a drop-in GameData ZIP."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile


def command(source, *args):
    return subprocess.check_output(args, cwd=source, text=True).strip()


def add_file(archive, source, name, executable=False):
    info = zipfile.ZipInfo(name)
    info.create_system = 3
    info.external_attr = (0o100755 if executable else 0o100644) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    archive.writestr(info, source.read_bytes())


def dependency_notices(source, target, archive):
    metadata = json.loads(command(source, "cargo", "metadata", "--locked",
                                  "--format-version", "1", "--filter-platform", target))
    resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
    notices = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["source"] is None or package["id"] not in resolved:
            continue
        root = Path(package["manifest_path"]).parent
        files = [p for p in root.rglob("*") if p.is_file()
                 and p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))]
        if package.get("license_file"):
            files.append(root / package["license_file"])
        prefix = f'JKR-licenses/{package["name"]}-{package["version"]}'
        for path in sorted(set(files)):
            add_file(archive, path, f"{prefix}/{path.relative_to(root).as_posix()}")
        selected_license = None
        if not files:
            # Some crates declare Apache-2.0 but omit its text from their crate.
            # Select that offered license and include the canonical text plus
            # the original package manifest (authors and license declaration).
            if package["license"] not in ("Apache-2.0", "MIT OR Apache-2.0"):
                raise RuntimeError(f"No supplied license text for {prefix}: {package['license']}")
            add_file(archive, Path(__file__).parent / "licenses/Apache-2.0.txt",
                     f"{prefix}/LICENSE-APACHE-2.0.txt")
            add_file(archive, Path(package["manifest_path"]), f"{prefix}/Cargo.toml")
            selected_license = "Apache-2.0"
        notices.append({"name": package["name"], "version": package["version"],
                        "license": package["license"], "authors": package["authors"],
                        "repository": package["repository"],
                        "selected_license": selected_license,
                        "license_files": len(set(files)) or 1})
    archive.writestr("JKR-licenses/dependencies.json", json.dumps(notices, indent=2) + "\n")


def instructions(platform, revision):
    suffix = ".exe" if platform == "windows-x64" else ""
    requirements = ("Windows 10/11 x64 and a working graphics driver. The MSVC runtime is statically linked."
                    if suffix else
                    "Linux x64 with glibc 2.35+, ALSA, Wayland/X11 libraries and a Vulkan or OpenGL driver.\n"
                    "If your archive extractor drops permissions: chmod +x jkr-viewer jkr-dedicated")
    return f"""JKR playtest build - {platform}
Source revision: {revision}

INSTALL
Extract ALL files from this ZIP directly into Jedi Academy's GameData folder,
beside its existing base folder. Do not put them inside base or an extra JKR folder.
The installed game must include base/assets0.pk3 through base/assets3.pk3.
Launch jkr-viewer{suffix}. No game-data path or environment variable is needed.
Keep jkr-dedicated{suffix} beside it for Create game and local devmap.
An existing shortcut must point to this client, not an older named playtest binary.

YOUR FILES
Settings, marks, screenshots, demos, favorites and friends live in GameData/jkr/.
Existing JKR user files are imported once; originals and existing destination
files are preserved. Unwritable installations use the per-user profile instead.
The console command path shows the selected folder. Downloaded PK3s retain their
separate per-user cache. This archive contains no game assets or personal settings.
Close JKR before replacing its executables; retain your jkr folder when updating.

REQUIREMENTS
{requirements}
This is a playtest build. Startup checks do not establish full GPU/gameplay
compatibility on every system. Windows graphical runtime testing is still pending.

SOURCE AND LICENSES
https://github.com/Bishop-R/JKR/tree/{revision}
GPL-2.0-only: see JKR-LICENSE.txt. Bundled notices are in JKR-licenses/.
The matching source snapshot is distributed separately as JKR-{revision[:7]}-source.zip.
Repository access may be required while the project is private.
"""


def smoke_check(package, platform, source):
    # All scratch stays under the repository target directory, never system /tmp.
    scratch = source / "target/parity-reports"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="package-", dir=scratch) as directory:
        root = Path(directory)
        game = root / "Jedi Academy space/GameData"
        game.mkdir(parents=True)
        with zipfile.ZipFile(package) as archive:
            assert archive.testzip() is None
            archive.extractall(game)
        suffix = ".exe" if platform == "windows-x64" else ""
        for name in ("jkr-viewer", "jkr-dedicated"):
            (game / (name + suffix)).chmod(0o755)
        server = subprocess.run([str(game / ("jkr-dedicated" + suffix)),
                                 "--bind", "127.0.0.1:0", "--quit-on-eof"],
                                cwd=root, input="", text=True,
                                check=True, capture_output=True, timeout=30)
        assert "listening on 127.0.0.1:" in server.stdout, server.stderr
        (game / "base").mkdir()
        for number in (0, 3):
            (game / f"base/assets{number}.pk3").touch()
        env = os.environ.copy()
        for name in ("JKR_GAME_DATA", "DISPLAY", "WAYLAND_DISPLAY"):
            env.pop(name, None)
        env["XDG_CONFIG_HOME"] = str(root / "profile")
        env["APPDATA"] = str(root / "profile")
        run = subprocess.run([str(game / ("jkr-viewer" + suffix))], cwd=root,
                             env=env, capture_output=True, text=True, timeout=60)
        assert run.returncode == 1, run.stderr
        assert "was not found in the mounted game data" in run.stderr, run.stderr
        assert (game / "jkr/config.cfg").is_file(), run.stderr
        print("Extracted client discovered adjacent synthetic assets and saved portable settings; loopback server startup/shutdown passed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--platform", choices=("linux-x64", "windows-x64"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--smoke-check", action="store_true")
    args = parser.parse_args()
    source, output = args.source.resolve(), args.output.resolve()
    if command(source, "git", "status", "--porcelain", "--untracked-files=no"):
        raise SystemExit("Refusing to package a modified source checkout")
    revision = command(source, "git", "rev-parse", "HEAD")
    output.mkdir(parents=True, exist_ok=True)
    package = output / f"JKR-{revision[:7]}-{args.platform}.zip"
    suffix = ".exe" if args.platform == "windows-x64" else ""
    binaries = [source / "target" / args.target / "release" / (name + suffix)
                for name in ("jkr-viewer", "jkr-dedicated")]
    with zipfile.ZipFile(package, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for binary in binaries:
            add_file(archive, binary, binary.name, executable=True)
        archive.writestr("README-JKR.txt", instructions(args.platform, revision))
        add_file(archive, source / "LICENSE", "JKR-LICENSE.txt")
        add_file(archive, source / "crates/jkr-viewer/assets/fonts/LICENSE.txt", "JKR-licenses/Inter-LICENSE.txt")
        dependency_notices(source, args.target, archive)
        archive.writestr("JKR-build.json", json.dumps({
            "revision": revision, "target": args.target,
            "rustc": command(source, "rustc", "--version"),
            "profile": "release", "rustflags": os.environ.get("RUSTFLAGS", ""),
            "binaries": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in binaries},
        }, indent=2) + "\n")
    if args.smoke_check:
        smoke_check(package, args.platform, source)
    source_zip = output / f"JKR-{revision[:7]}-source.zip"
    subprocess.run(["git", "-c", "core.autocrlf=false", "archive", "--format=zip", f"--prefix=JKR-{revision[:7]}/",
                    f"--output={source_zip}", revision], cwd=source, check=True)
    checksums = "".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n"
                        for p in (package, source_zip))
    (output / f"SHA256SUMS-{args.platform}.txt").write_text(checksums, encoding="utf-8")
    print(package)


if __name__ == "__main__":
    main()
