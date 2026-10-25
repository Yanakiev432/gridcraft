# Runtime navigation and trademark separation — review-only

Date: 2026-10-25. Branch: feature/oss-brand-assets-review-20261025.

This is an independent Craft Open Source modification of the original GridCraft project. It is NOT an official ArtCraft edition. The source now points the active About/help dialog, native Help menu, Windows MSI help/update metadata and Linux package descriptions to the independent GitHub fork and separately credits the original upstream source.

The original copyright text, LICENSE-MIT, LICENSE-APACHE, NOTICE and historical trademark policy are retained unchanged. Brand-only source cleanup remains review-only; existing application icons require separate rights and visual inspection.

Check: `cargo check --offline --locked -p gridcraft-ui-egui -p gridcraft`, independent runtime navigation audit and source trademark gate. No installed product changes.

No mandatory paid code signing. Unsigned local previews can be tested with SHA-256 manifests and WiX ICE, but this is NOT an approved public release; do not bypass Windows trust controls.
