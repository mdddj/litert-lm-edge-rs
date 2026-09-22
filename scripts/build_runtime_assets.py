#!/usr/bin/env python3
"""Package the vendored LiteRT-LM runtimes as crates.io-friendly release assets.

The `litert-lm-edge-sys` crate is far too large for the crates.io 10 MB limit,
so the native runtimes are shipped as GitHub Release assets and fetched by
`litert-lm-edge-sys/build.rs`. This script produces those assets from the
locally built `litert-lm-edge-sys/vendor/<platform>` directories and prints the
`runtime-checksums.txt` entries that pin them.

Usage:
    python3 scripts/build_runtime_assets.py [--out-dir dist/runtime]

Then publish the assets on the release matching the crate version:

    gh release upload v0.2.1 dist/runtime/*.tar.gz --clobber
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import pathlib
import tarfile

PLATFORMS = ("darwin-arm64", "linux-x86_64", "windows-x86_64")
REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
VENDOR_ROOT = REPO_ROOT / "litert-lm-edge-sys" / "vendor"

EXECUTABLE_SUFFIXES = (".dylib", ".dll", ".so")


def asset_name(platform: str) -> str:
    return f"litert-lm-runtime-{platform}.tar.gz"


def build_asset(platform: str, out_dir: pathlib.Path) -> pathlib.Path:
    """Write a deterministic .tar.gz containing `<platform>/<runtime files>`."""
    vendor_dir = VENDOR_ROOT / platform
    if not vendor_dir.is_dir():
        raise SystemExit(
            f"missing vendor directory {vendor_dir}; run the matching "
            f"scripts/prepare_litert_lm_*.sh (or .ps1) first"
        )

    entries = sorted(path for path in vendor_dir.rglob("*") if path.is_file())
    if not entries:
        raise SystemExit(f"no runtime files found in {vendor_dir}")

    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for path in entries:
            relative = pathlib.PurePosixPath(platform) / path.relative_to(vendor_dir).as_posix()
            info = tarfile.TarInfo(str(relative))
            payload = path.read_bytes()
            info.size = len(payload)
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mode = 0o755 if path.name.endswith(EXECUTABLE_SUFFIXES) else 0o644
            archive.addfile(info, io.BytesIO(payload))

    out_dir.mkdir(parents=True, exist_ok=True)
    destination = out_dir / asset_name(platform)
    with destination.open("wb") as handle:
        with gzip.GzipFile(fileobj=handle, mode="wb", compresslevel=9, mtime=0) as gz:
            gz.write(raw.getvalue())
    return destination


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out-dir",
        type=pathlib.Path,
        default=REPO_ROOT / "dist" / "runtime",
        help="directory to write the .tar.gz assets into",
    )
    parser.add_argument(
        "--checksums-only",
        action="store_true",
        help="recompute hashes for existing assets without rebuilding them",
    )
    arguments = parser.parse_args()

    lines: list[str] = []
    for platform in PLATFORMS:
        destination = arguments.out_dir / asset_name(platform)
        if not arguments.checksums_only:
            destination = build_asset(platform, arguments.out_dir)
        if not destination.is_file():
            raise SystemExit(f"missing asset {destination}")
        lines.append(f"{platform} {sha256(destination)}")
        print(f"{destination}  ({destination.stat().st_size / 1_048_576:.1f} MiB)")

    print("\nlitert-lm-edge-sys/runtime-checksums.txt:")
    for line in lines:
        print(line)


if __name__ == "__main__":
    main()
