#!/usr/bin/env python3
"""Fail closed on compromised emSign TLS certificate validation dependencies."""
from __future__ import annotations
import argparse, json, tomllib
from pathlib import Path

def verify(data: bytes):
    doc=tomllib.loads(data.decode("utf-8"))
    packages=doc.get("package",[])
    if not isinstance(packages,list):raise ValueError("Missing Cargo.lock dependencies")
    fixes={"rustls-webpki":(0,103,16)}
    # Some Craft apps transitively include a static root bundle, others do not.
    optional={"webpki-root-certs":(1,0,10)}
    found={}
    for name,needed in {**fixes,**optional}.items():
        versions=[p.get("version") for p in packages if isinstance(p,dict) and p.get("name")==name]
        if name in fixes and len(versions)!=1:raise ValueError("Missing expected rustls-webpki")
        if name in optional and len(versions)==0:continue
        if len(versions)!=1:raise ValueError("Duplicate unsafe TLS dependency")
        val=versions[0]
        if not isinstance(val,str) or len(val.split("."))!=3 or not all(s.isascii() and s.isdecimal() for s in val.split(".")):
            raise ValueError("Unreviewed prerelease or malformed TLS dependency version")
        v=tuple(map(int,val.split(".")))
        if v[:2]!=needed[:2] or v[2]<needed[2]:raise ValueError(f"Unpatched {name}: {val}")
        found[name]=val
    return {"status":"PASS","locked":found,"scope":"RUSTSEC-2026-0382/0383 known stable branches",
            "btrust_qes_verified":False,"qualified_timestamp_verified":False,"trusted_release_ready":False}

def test():
    valid=b'[[package]]\nname = "rustls-webpki"\nversion = "0.103.16"\n'
    assert verify(valid)["status"]=="PASS"
    for unsafe in (b'0.103.15',b'0.103.16-rc.1',b'0.104.0-alpha.8'):
        try:verify(valid.replace(b'0.103.16',unsafe))
        except ValueError:continue
        raise AssertionError("Accepted compromised/unreviewed version "+str(unsafe))
    for root_version in (b'1.0.9',b'1.0.10'):
        sample=valid+b'\n[[package]]\nname = "webpki-root-certs"\nversion = "'+root_version+b'"\n'
        if root_version==b'1.0.10':assert verify(sample)["status"]=="PASS"
        else:
            try:verify(sample)
            except ValueError:pass
            else:raise AssertionError("Accepted stale trust root bundle")

def main():
    a=argparse.ArgumentParser(description=__doc__)
    a.add_argument("--lock",type=Path,default=Path(__file__).resolve().parents[1]/"Cargo.lock")
    a.add_argument("--self-test",action="store_true")
    x=a.parse_args()
    result=verify(x.lock.read_bytes())
    if x.self_test:test()
    print(json.dumps(result,sort_keys=True))
if __name__=="__main__":
    main()
