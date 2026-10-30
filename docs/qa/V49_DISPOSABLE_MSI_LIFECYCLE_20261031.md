# V49 — disposable Windows MSI lifecycle gate

Status: **IMPLEMENTED / NOT YET VERIFIED ON HOSTED RUNNER** (2026-10-31).

## Goal

The original GridCraft 0.8.0 on Yanakiev-Lenovo must not be upgraded, uninstalled, or have its executable/registry settings overwritten by the independent **GridCraft (Craft Open Source)** MSI.

## New executable acceptance

- `.github/workflows/gridcraft-msi-lifecycle.yml` runs only on GitHub-hosted Windows 2022 machines (not the Lenovo).
- `packaging/windows/test-msi-side-by-side.ps1` has an independent, mandatory hosted-runner guard and refuses if *any* pre-existing original or fork GridCraft is present.
- The test builds an artificial **historical legacy MSI sentinel** with the historical UpgradeCode and independent minimal stub executable, and the actual fork WiX configuration with four stub executables.
- It installs the legacy MSI sentinel, fingerprints the legacy executable, registry marker and shortcut, then installs the independent MSI; it asserts both products remain registered and the legacy fingerprint and marker are unchanged.
- It removes the independent MSI; proves the legacy executable, shortcut and marker survive; then removes only its own synthetic legacy fixture. It never targets other ProductCodes.
- The previously present `test-desktop-shortcut.ps1` is included in the new workflow to exercise independent repair/upgrade/uninstall and desktop shortcut preference. Those are installer lifecycle tests, **not repeated Excel signature acceptance**.
- Offline script syntax and actual WiX compilation of the new sentinel on Lenovo: **PASS**.
- Explicit invocation on Lenovo: correctly **refused** before creating an MSI or modifying the computer: **PASS**.
- The V48 read-only MSI identity guard and V47 payload integrity still exist, including the inherited UpgradeCode negative control.

## Strict release boundaries

**No hosted CI result has been observed yet**, so do not label the lifecycle test PASS. No unsigned QA MSI was installed on Lenovo; no historical GridCraft data was edited; neither V44/V45 Excel acceptance nor V46 formula/signing tests were repeated.

The original publisher `Learning Machines LLC` is still used *only in the artificial historical MSI sentinel fixture* to detect accidental takeover. The independent edition's `Craft Open Source` publisher field is a project label and not a verified legal company name. Authenticode signing is still **NOT DONE**.

A successful CI run can confirm only installation behavior on that disposable runner, not third-party licensing/brand clearance, qualified B-Trust signing, long-term MSI upgrade behavior on all Windows versions, or the external V46 Excel acceptance.
