# Office B-Trust V23 — fail-closed GUI routing and stage diagnostics

**Status: QA source only; physical B-Trust signing not accepted.** Based on V22 native OPC signing core with opt-in PIN-owner HWND and stage trace.

Evidence:
- V22 WordCraft showed a nonterminating "Digital signature in progress"; the V22 screen label was confirmed.
- No `opc-stage-*.log` was found in the V22 QA root. This is not proof the Windows provider was reached.
- The GUI has separate signing implementations: Windows native OPC and a PowerShell child process for the Craft-only restrictive policy. The latter was hidden and had no native traces.

Changes in V23:
1. Refuse the unsupported *no further signatures* (Craft-only) policy before spawning a worker. Never silently run its hidden PowerShell helper from this QA UI.
2. The default option is a **visible** fixed-location appearance: Word at the end of the DOCX document; GridCraft anchored at the chosen XLSX cell. The previous mouse rectangle is merely a screen guide; it **does not** place a signature in DOCX/XLSX. The interface states this limitation.
3. Record only `utc_ms` and fixed event names in `office-ui-stage-<pid>.log` when `CRAFT_OPC_TRACE_DIR` is set to an existing folder. The native layer separately writes `opc-stage-<pid>.log`. No certificate IDs, filenames, PINs, keys or document bytes are logged.
4. Show `QA V23` on the progress window and an elapsed counter. A warning after 30 seconds is **not** a timeout and does not abort the Windows provider. Never start a second hardware request while one is pending.
5. Guarded verification and no-overwrite signed-copy generation remain unchanged. Do not install or release without supervised token trial and independently verified signed DOCX/XLSX.

Minimal acceptance: focused UI unit tests without token; both GUI apps build offline; process-local QA launchers and code hashes match; only then one synthetic document at a time, with PIN entered solely into the official B-Trust dialog; inspect both stage logs; independently reopen and verify result and eIDAS qualification separately.
