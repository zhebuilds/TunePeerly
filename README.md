<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="192" height="192" alt="TunePeerly">
</p>

<h1 align="center">TunePeerly（声渡）</h1>

<p align="center">
  English |
  <a href="README.zh-CN.md">简体中文</a>
</p>

TunePeerly plays the music already on this computer and shares it with paired devices. There is no account. Paired devices on the same local network find each other directly. A relay you run yourself can be used for connections outside that network. Pairing happens only on the local network; the relay only carries data between devices that are already paired, and never pairs.

> **Note:** The current release is an alpha test. The program is not guaranteed to match the documentation.

## Features

- Play local files, stream any song from a paired device, or download the original file.
- Find devices on the LAN. To pair, one device shows a 6-digit code and you type it on the other. Before pairing, **Discover nearby devices** on the Devices page (Nearby) must be turned on, on both devices, or they cannot be found.
- If devices cannot find each other on the LAN, turn on a hotspot and connect the devices to that same hotspot.
- Recently played records a song after 10 seconds, or after half of it when the song is shorter than 20 seconds. It stays on each device. You can view a connected device's list.
- Send or override liked songs and appearance by hand.
- Incognito cuts every device connection and the relay, stops discovery and sync, and stops recording history.
- Light and dark themes, English and Chinese, scrolling lyrics, a tray icon, a mini player, and playback buttons on the Windows taskbar thumbnail.
- An Android app plays with the screen locked, with controls in the notification, on the lock screen, and in a home screen widget. See [Android](docs/android.md).

## Security and privacy

- Devices talk only over TLS 1.3, and both sides prove who they are. Each device has its own key pair, and its device ID is the SHA-256 hash of its public key. After pairing, each device keeps the other's public key and accepts no other key for that device.
- The encryption runs end to end between your devices. A relay only passes the data along and cannot read it. It does see each device's relay ID, public IP, when it is online, and who connects to whom and how much data. See [SECURITY.md](SECURITY.md).
- A relay connection is accepted only from a paired device. When using a relay, set both devices to **Full trust** for each other.
- The library database is encrypted with SQLCipher. Its random key is kept in `db.key` next to the database. On Windows that file is sealed with DPAPI, so it opens only under your Windows account. On Android it is wrapped by a key that stays in the Android Keystore. On any other system (development builds only) the file can be read only by its owner (mode 0600).
- Pairing uses a 6-digit code shown on one computer and typed on the other. The code is checked with SPAKE2, so it never crosses the network and a device in the middle cannot pair in your place. A code works once, for 90 seconds. After 3 wrong codes, pairing pauses for 1 minute for every device.
- Removing a device unpairs both sides. This computer deletes the device's key and tells the device, which removes this computer from its own list. A device that is offline is told the next time the two connect. Your other devices are not changed. To use the two together again, pair them again with a new code from either side.

## Guide

[User guide](docs/guide.md) · [Deploy a relay](docs/relay.md)

## Install

### Windows

GitHub releases are packed with [Inno Setup](installer/TunePeerly.iss). The installer asks whether to install for all users or only for the current user. The default is the current user, at `C:\Users\<name>\AppData\Local\Programs\TunePeerly`, and it does not need administrator rights. Installing for all users uses `C:\Program Files\TunePeerly` and needs administrator rights.

The database and the cache do not follow that choice, and they cannot be changed on their own. The database is always `%APPDATA%\TunePeerly`. Covers and the window cache are always in `%LOCALAPPDATA%\TunePeerly`.

The downloaded installer is `TunePeerly-0.8.0-alpha.1-x64-Setup.exe`. In that file's folder, open PowerShell, compute its SHA256, and compare it with the SHA256 shown for this file on the release page. Install it when the two strings match.

```powershell
Get-FileHash -Algorithm SHA256 .\TunePeerly-0.8.0-alpha.1-x64-Setup.exe
```

### Android

Download `TunePeerly-0.8.0-alpha.1-arm64-v8a.apk` from the release page. It runs on Android 7.0 and later, on 64-bit ARM phones, which covers nearly all current phones. Open the APK on the phone and allow installs from that app when Android asks. See [Android: Install](docs/android.md#install).

To check that the APK is the official one, compare its signing certificate with the fingerprint in [Verifying official APKs](docs/android.md#verifying-official-apks).

## Develop

You need Node.js, npm, a Rust toolchain, Perl, and the Microsoft Edge WebView2 Runtime on Windows.

The library database is encrypted with SQLCipher. The build compiles its own copy of OpenSSL for it, which needs Perl and takes a few minutes the first time.
- Windows: install [Strawberry Perl](https://strawberryperl.com/) and make sure `perl` in your terminal is Strawberry Perl, not the Perl that comes with Git for Windows. `nmake` comes with the Visual Studio C++ build tools that Rust already needs. [NASM](https://www.nasm.us/) is optional: with `nasm` on PATH OpenSSL is built with its faster assembly code, without it OpenSSL is built without assembly.
- Linux and macOS: `perl` and `make` are usually already installed.

This is for the desktop app. To build the Android app, see [Android: Build from source](docs/android.md#build-from-source). To build Android on Windows, use WSL2.

```powershell
npm install
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run tauri build
```

If `npm run tauri dev` stops with `listen EACCES: permission denied ::1:1420`, Windows NAT has reserved port 1420. No program is listening on it. Open PowerShell as administrator, run the two commands below, then run `npm run tauri dev` again. Stopping Windows NAT drops existing Hyper-V, WSL, and Docker NAT connections until the service starts again.

```powershell
net stop winnat
net start winnat
```

## Installer

Run these from the repository root. Build the release program first, then compile [installer/TunePeerly.iss](installer/TunePeerly.iss). The script takes `src-tauri\target\release\TunePeerly.exe`. [Inno Setup 7](https://jrsoftware.org/isinfo.php) must be installed.

```powershell
npm run tauri build -- --no-bundle
& "${env:ProgramFiles}\Inno Setup 7\ISCC.exe" installer\TunePeerly.iss
```

`npm run tauri build -- --no-bundle` builds the executable only. It does not create an installer. The setup program is written to `installer\Output\TunePeerly-0.8.0-alpha.1-x64-Setup.exe`. `installer\Output` is gitignored. You can also open `installer\TunePeerly.iss` in Inno Setup and compile it.

A compile that stops at `EndUpdateResource failed (110)` means the new setup program was locked while its icons were being written. Windows Defender real-time scanning locks that unsigned exe, and Windows Security does not show a prompt. Wait a few seconds and run the `ISCC.exe` command above again. Do not compile again immediately after the failure. If it is still 110, add the repository's `installer\Output` folder to the Windows Security exclusions and compile again: Virus & threat protection → Manage settings → Exclusions → Add an exclusion → Folder.

## Contributing

The contribution process is in [CONTRIBUTING.md](CONTRIBUTING.md). Open an issue and wait until it is confirmed before writing code.

## Project

- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)

## License

[GNU Affero General Public License v3.0](https://www.gnu.org/licenses/agpl-3.0.html). Additional terms for attribution and modified versions are in [ADDITIONAL_TERMS.md](ADDITIONAL_TERMS.md).
