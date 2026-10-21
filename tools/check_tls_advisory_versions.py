#!/usr/bin/env python3
"""Fail closed on newly published 2026 emSign TLS path validation advisories.

This offline locked-dependency gate does NOT establish eIDAS QTSA status,
certificate-chain validity of a signed PDF or any trusted release.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import tomllib


def verify(packages: list[dict]) -> dict:
    def versions(name: str) -> list[str]:
        return [p["version"] for p in packages if p.get("name") == name]

    webpki = versions("rustls-webpki")
    if len(webpki) != 1 or webpki[0] != "0.103.16":
        raise ValueError("RUSTSEC-2026-0382: locked rustls-webpki requires explicit reviewed patched baseline 0.103.16")
    roots = versions("webpki-root-certs")
    if roots and (len(roots) != 1 or roots[0] != "1.0.10"):
        raise ValueError("RUSTSEC-2026-0383: root bundle requires explicit reviewed patched baseline 1.0.10")
    return {
        "schema": "craft.tls-crypto-lock-advisory-gate.v1",
        "rustls_webpki": webpki[0],
        "webpki_root_certs": roots[0] if roots else "NOT_USED",
        "rustsec_2026_0382": "PATCHED_BASELINE",
        "rustsec_2026_0383": "PATCHED_OR_NOT_USED",
        "eidas_qtsa": "NOT_CHECKED",
        "trusted_software_release": False,
    }


def self_test() -> None:
    good = [{"name": "rustls-webpki", "version": "0.103.16"}]
    verify(good)
    verify(good + [{"name": "webpki-root-certs", "version": "1.0.10"}])
    bad = [
        [],
        [{"name": "rustls-webpki", "version": "0.103.15"}],
        good + [{"name": "rustls-webpki", "version": "0.103.16"}],
        good + [{"name": "webpki-root-certs", "version": "1.0.9"}],
        good + [{"name": "webpki-root-certs", "version": "1.0.10"}, {"name": "webpki-root-certs", "version": "1.0.10"}],
    ]
    for case in bad:
        try:
            verify(case)
        except ValueError:
            continue
        raise AssertionError("Fail-open: invalid TLS package lock accepted")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--lock", default="Cargo.lock")
    p.add_argument("--self-test", action="store_true")
    args = p.parse_args()
    with Path(args.lock).open("rb") as fh:
        data = tomllib.load(fh)
    if args.self_test:
        self_test()
    print(json.dumps(verify(data["package"]), indent=2))


if __name__ == "__main__":
    main()
