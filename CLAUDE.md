# layon

See `README.md` for layout and usage.

- **Never exercise a power request against a machine you need.** Start the
  host with `layon run --dry-run` when testing; without it `layon remote sleep`
  suspends the host for real.
- The serving `layon run` shares no socket with `pair`, `devices` and `unpair`:
  they reach it only through the files in its data dir, which is why it
  re-reads them on every request.
- iOS forbids raw multicast and UDP broadcast without the
  `com.apple.developer.networking.multicast` entitlement, which a free Apple ID
  cannot get. Consequences: the client's `mdns` feature must be off on iOS (the
  app browses with Bonjour and calls `Client::set_addrs`), and of the packets
  `Client::wake` sends only the unicast one leaves an iPhone.
- The Windows code path (`crates/layon/src/power.rs`) cannot be compiled on the
  Linux dev box: the crypto dependencies need MSVC. CI is the check.
- `Cargo.lock` pins `libc` to 0.2.189: from 0.2.190 `libc::in6_ifreq` is
  macOS-only, and `netdev` (via iroh's `netwatch`) uses it on iOS too. A plain
  `cargo update` breaks the iOS build; unpin once
  https://github.com/shellrow/netdev/pull/192 is released and reaches iroh.
- The apps are shells over `layon remote` by decision: a new action goes into
  it first, and its `--json` output is the interface of the Android and Windows
  apps (each has a `LayonCli` that parses it). Only iOS links `crates/ffi`.
- No app builds on the Linux dev box (no Swift, Android SDK or .NET); CI is the
  check. A Mac with Xcode can type-check the Swift quickly:
  `xcrun -sdk iphoneos swiftc -typecheck -I <include dir> Sources/*.swift
  layon_ffi.swift`, against bindings generated on Linux.
