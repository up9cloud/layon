# layon

Work from bed: drive a work machine from a phone or tablet.

Two halves. An **agent** runs on the work machine (Windows first; Linux and
macOS compile too). A **client** runs on the phone or tablet (iOS and iPadOS
first, then Android). The apps are thin: every button runs the `layon` CLI,
except on iOS, which cannot run a bundled executable and links the library
instead.

What works: put the machine to sleep or hibernate, and wake it with
Wake-on-LAN. Planned: type into the work machine's terminal from the mobile
device.

## How it fits together

- `crates/proto` — the request/response types and the pairing code format.
- `crates/agent` — `layon-agent`, the work-machine daemon.
- `crates/client` — the mobile-side library, and `layon`, a CLI that stands in
  for the mobile device.
- `crates/ffi` — the client library exposed to Swift through UniFFI.
- `apps/ios` — SwiftUI app for iPhone and iPad, over `crates/ffi`.
- `apps/android` — Android app; it bundles the `layon` CLI and runs it with
  `--json`.

The two sides talk over [iroh](https://iroh.computer) (QUIC). Each has a
permanent key; a device is identified by its key, never by an address. Pairing
is scanning a QR code once. After that the mobile device finds the machine
through mDNS, so either side changing IP needs no re-pairing.

## Download

Each CI run on the Actions tab keeps one file per device:

| file | install on |
| --- | --- |
| `layon-agent-<platform>` | the work machine |
| `layon-app-ios.ipa` | iPhone or iPad, signed with your own Apple ID (e.g. Sideloadly) |
| `layon-app-android.apk` | Android phone or tablet |
| `layon-cli-<platform>` | another computer, to test an agent without the app |

`<platform>` is `windows-x64`, `windows-arm64`, `linux-x64` or `macos-arm64`.

## Build

```sh
cargo build --release
```

On Windows this needs the MSVC build tools (the "Desktop development with C++"
workload).

## Use

On the work machine:

```sh
layon-agent            # serve; leave it running
layon-agent pair       # in a second terminal: shows the QR code
layon-agent devices
layon-agent unpair <id prefix>
```

`layon-agent run --dry-run` logs power requests without acting on them.

From another computer on the same network, in place of the mobile device:

```sh
layon pair 'layon://pair/...'   # the text printed under the QR code
layon sleep
layon wake
```

`--json` is what the apps use: `pair`, `hosts` and `info` print JSON, and a
failure exits 1 with `{"error": ...}` on stderr.

### Apps

- iOS: on a Mac, `apps/ios/build-rust.sh`, then `xcodegen generate` in
  `apps/ios` and open `Layon.xcodeproj`.
- Android: `apps/android/build-cli.sh` (needs `cargo-ndk` and the NDK), then
  `gradle assembleDebug` in `apps/android`.

Without the repository secrets `ANDROID_KEYSTORE_BASE64` and
`ANDROID_KEYSTORE_PASSWORD` (key alias `layon`), each CI run signs the apk with
a fresh key, and Android only installs it after the old app is uninstalled.

### Windows setup

Run the agent as the logged-in user, not as a service: its data lives in that
user's `%LOCALAPPDATA%\layon`, and `pair` must read the same files.

Let the mobile device in (elevated prompt):

```bat
netsh advfirewall firewall add rule name="layon" dir=in action=allow protocol=UDP localport=47126
netsh advfirewall firewall add rule name="layon mDNS" dir=in action=allow protocol=UDP localport=5353
```

Start at logon:

```bat
schtasks /create /tn layon-agent /sc onlogon /tr "C:\path\to\layon-agent.exe"
```

### Waking

Wake-on-LAN is a property of the hardware, not of layon:

- Enable it in the BIOS/UEFI, and in Device Manager → the network adapter →
  Power Management ("Allow this device to wake the computer", "Only allow a
  magic packet") and Advanced ("Wake on Magic Packet").
- Wired adapters do this reliably. Most Wi-Fi adapters cannot be woken.
- Waking from hibernate, as opposed to sleep, depends on the board.
