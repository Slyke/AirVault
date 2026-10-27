# Application identity and offline instructions

About is available before unlocking and from the sidebar. It obtains the package version, Git revision, build UTC timestamp, target, dirty-source marker and source SHA-256 from the native build. The source hash includes uncommitted and newly added files within the explicit build-input directories listed in `build-support/fingerprint.rs`; it is not just the last commit ID.

The current binary SHA-256 is streamed from `current_exe()` at runtime and recalculated on request. Compare it to a trusted checksum for the **EXE**, not the installer or ZIP. Windows PowerShell can check the file independently:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath 'C:\path\to\AirVault.exe'
```

The portable package includes BUILD-INFO.json, SHA256SUMS.txt alongside the ZIP, and the original Bitcoin whitepaper under docs/reference. The application embeds the paper and can save it from Instructions → Bitcoin whitepaper without network access.

Checksums are not signatures or attestation. A modified app could lie about its own result; authenticate the release reference and use an external hash tool. The EXE digest does not cover the loader DLL, WebView runtime, other software, in-memory modification or vault contents. Source hashes identify inputs, not reproducible binary output: timestamps and toolchains can change binary bytes.

Instructions now has 23 searchable articles grouped by task, with basic walkthroughs, advanced reference and related links. Contextual help opens the matching article. Mode help explains Offline, Online and Proxy, including why private-key profiles cannot become Proxy profiles.

Auto-lock accepts -1 to disable **inactivity** locking, or 30–3600 seconds. Manual lock, configured focus-loss lock, sleep/stall protections and process exit still end the unlocked session. New sessions always begin locked. Older positive timeout values remain compatible.
