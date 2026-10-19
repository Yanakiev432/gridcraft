# Code signing policy — gridcraft Craft edition (application pending)

**Status on 2026-10-19:** the public modified fork is a candidate for SignPath Foundation's *free open-source code signing*. **NO APPLICATION APPROVAL OR SIGNED CRAFT RELEASE IS CLAIMED.** The required public attribution, *“Free code signing provided by SignPath.io, certificate by SignPath Foundation”*, will apply only if the project is accepted and its actual releases are signed with that service.

## Project origin and legal gate

- Source repository: https://github.com/Yanakiev432/gridcraft; public GitHub fork (GitHub API `fork=true`) of https://github.com/storytold/gridcraft.
- GitHub-reported license: Apache-2.0. Original upstream, third-party copyright/NOTICE files and source obligations must be retained and rechecked before redistribution.
- Official upstream release: `v0.8.0`, Windows x64 MSI, **14131200 bytes**, SHA-256 `2986211704e8ef791f2efd0278e46b13ed7f23f3da6b247f982c1c2acf81d852`.
- On 2026-10-19 Windows Get-AuthenticodeSignature returned **Valid**, signer **Learning Machines Inc** for that exact upstream MSI. No executable was launched or installed.
- This demonstrates an officially signed upstream release and visible fork relationship, two of SignPath Foundation's modified-fork eligibility conditions. **It is not a guarantee of eligibility or approval**; the remaining terms require verification by SignPath.

## Roles and release approval

- Source-code custodian and maintainer: https://github.com/Yanakiev432.
- Committers: repository maintainers with explicitly assigned GitHub write rights. External contributions require human review before acceptance.
- Release approver: repository maintainer; approve every signing request explicitly within SignPath, using MFA.
- Qualified separate reviewer for release-signing policies: **TO BE VERIFIED / ASSIGNED BEFORE APPLICATION**. Never fabricate a review or signing approval.
- Source and CI build scripts are reviewed together. Protect the release branch, pin build inputs and use trusted, origin-verified GitHub Actions builds from the exact release tag. No self-hosted unsigned binaries may enter the production signing lane.

## Signable outputs and verification

- Sign **only this project's own built EXE, DLL and MSI** (as applicable), never re-sign upstream packages as if they were our own.
- Product metadata, name, version, and original source commit must match the approved release.
- The signing identity stays with the approved hosted code-signing provider. Do not use a B-Trust *document-QES* certificate as an EXE/MSI Code Signing EKU credential, export USB PINs, or disable SAC/WDAC.
- Verify every published artifact with full SHA-256, Authenticode Code Signing EKU, timestamp and a real clean Windows SAC-enforced install/uninstall/rollback smoke test.
- Until all gates pass, release status is **NO-GO**, including any unsigned MSI candidates.

## Privacy and security

- The signing implementation processes users' local documents; hardware PIN entry remains with the legitimate certificate provider UI.
- A user-approved RFC 3161 TSA integration sends a cryptographic imprint and nonce, **not the original document**. The user chooses the provider; ACCV free use is limited to permitted personal usage. Open TSA is not eIDAS qualified.
- Additional optional network-dependent application features and upstream dependencies require separate privacy disclosure. Do not assert that the entire application never uses the network without code review.
- Avoid storing user documents, private keys, TSA credentials, or hardware PINs in source, CI artifacts, or public logs.

## Required release provenance before requesting free signing

1. Source/code/license/dependency audit and documented upstream signed-release evidence.
2. Public README link to this policy, review/approval roles, accessible release/download information and existing releases of the modified project.
3. MFA, protected release branches, reviewable source CI and build provenance with artifact naming/version enforcement.
4. Manual SignPath Foundation application and **written approval**, including any project-specific fork conditions.
5. Hosted trusted build/signing pipeline, manual release approval, independent Authenticode and SAC tests. Never mark this step PASS on the strength of an upstream signature.

Official references: https://signpath.org/terms.html · https://signpath.org/apply.html · https://docs.signpath.io/origin-verification/ .
