# layon

See `README.md` for layout and usage.

- **Never exercise a power request against a machine you need.** Start the
  agent with `run --dry-run` when testing; without it `layon sleep` suspends
  the host for real.
- The agent's subcommands share no socket. `pair`, `devices` and `unpair` talk
  to the running agent only through the files in its data dir, which is why the
  agent re-reads them on every request.
- iOS forbids raw multicast and UDP broadcast without the
  `com.apple.developer.networking.multicast` entitlement, which a free Apple ID
  cannot get. Consequences: the client's `mdns` feature must be off on iOS (the
  app browses with Bonjour and calls `Client::set_addrs`), and of the packets
  `Client::wake` sends only the unicast one leaves an iPhone.
- The Windows code path (`crates/agent/src/power.rs`) cannot be compiled on the
  Linux dev box: the crypto dependencies need MSVC. CI is the check.
- `Cargo.lock` pins `libc` to 0.2.189: from 0.2.190 `libc::in6_ifreq` is
  macOS-only, and `netdev` (via iroh's `netwatch`) uses it on iOS too. A plain
  `cargo update` breaks the iOS build; unpin once
  https://github.com/shellrow/netdev/pull/192 is released and reaches iroh.
- The apps are shells over the `layon` CLI by decision: a new action goes into
  the CLI first, and its `--json` output is the Android app's interface
  (`apps/android/.../LayonCli.kt` parses it). Only iOS links `crates/ffi`.
- Neither app builds on the Linux dev box (no Swift, no Android SDK); CI is the
  check. A Mac with Xcode can type-check the Swift quickly:
  `xcrun -sdk iphoneos swiftc -typecheck -I <include dir> Sources/*.swift
  layon_ffi.swift`, against bindings generated on Linux.
