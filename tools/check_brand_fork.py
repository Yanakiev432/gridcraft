#!/usr/bin/env python3
"""Fail-closed brand gate for independently redistributed ArtCraft-derived Craft forks.

Allows source attribution of upstream trademarks in plain text, never original
restricted marks in source, embedded resources, logo references or release output.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

RESTRICTED_DIR = "docs/brand/"
EXECUTABLE_ASSET_REFS = re.compile(
    r'(?:src\s*=|srcset\s*=|href\s*=|include_bytes!\s*\(|'
    r'path\s*=)[^\n]*docs/brand/', re.IGNORECASE
)
APPROVED_LICENSES = ("assets/app-icon/LICENSE.txt", "assets/app-icon/README.md")


def check(root: Path) -> dict:
    cmd = subprocess.run(
        ["git", "-c", "safe.directory=*", "-C", str(root), "ls-files", "-z"],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
    )
    tracked = [p.decode("utf-8") for p in cmd.stdout.split(b"\x00") if p]
    forbidden = [p for p in tracked if p.startswith(RESTRICTED_DIR)]
    violations = [{"path": p, "reason": "restricted upstream brand file remains tracked"} for p in forbidden]
    for name in tracked:
        if not name.endswith((".md", ".toml", ".rs", ".html", ".htm", ".xml", ".json", ".tsx", ".yaml", ".yml")):
            continue
        path = root / name
        if not path.is_file() or path.stat().st_size > 2_000_000:
            continue
        contents = path.read_text(encoding="utf-8", errors="replace")
        if name in ("README.md", "ATTRIBUTION.md", "ATTRIBUTION.toml") and EXECUTABLE_ASSET_REFS.search(contents):
            violations.append({"path": name, "reason": "live branded asset link in metadata"})
        if name.startswith(("crates/", "apps/", "packaging/")) and EXECUTABLE_ASSET_REFS.search(contents):
            violations.append({"path": name, "reason": "compiled/runtime reference to restricted mark"})
        if name in ("README.md", "ATTRIBUTION.md") and re.search(
            r"<img[^>]+(?:artcraft-mark|artcraft-logo)", contents, re.I
        ):
            violations.append({"path": name, "reason": "embedded upstream ArtCraft mark"})
    return {
        "schema": "craft.brand-source-gate.v1",
        "tracked_files": len(tracked),
        "restricted_assets": len(forbidden),
        "violations": violations,
        "status": "PASS" if not violations else "NO_GO",
        "binary_package_branding": "NOT_CHECKED",
        "screenshots_and_store_listings": "NOT_CHECKED",
        "software_publisher_signing": "NOT_CHECKED",
    }


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = p.parse_args()
    result = check(args.root)
    print(json.dumps(result, indent=2, ensure_ascii=False))
    if result["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
