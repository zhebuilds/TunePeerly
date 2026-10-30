<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="192" height="192" alt="TunePeerly">
</p>

<h1 align="center">TunePeerly（声渡）</h1>

<p align="center">
  English |
  <a href="README.zh-CN.md">简体中文</a>
</p>

TunePeerly plays the music already on this computer and shares it with paired devices. There is no account. Paired devices on the same local network find each other directly. A relay you run yourself can be used for connections outside that network.

> **Note:** The current release is an alpha test. The program is not guaranteed to match the [user guide](docs/guide.md).

## Features

- Play local files, stream an exposed song from a paired device, or download the original file.
- Find devices on the LAN, or connect by IPv4 address and port. Both sides confirm. Sync data is end-to-end encrypted with libsignal. Before pairing, **Discover nearby devices** in Settings → Network must be on, or devices cannot be found.
- Recently played merges by the latest UTC time. A song is recorded after about 10 seconds, or half its length. It can be merged again when this computer comes online.
- Sync liked songs and appearance, and follow playback from one online device.
- Incognito pauses discovery, sync, and history recording. You can also block a device, or stop manual sync and following per item.
- Light and dark themes, English and Chinese, scrolling lyrics, a tray icon, a mini player, and playback buttons on the Windows taskbar thumbnail.
- Some settings are marked **To be revised**, **Do not use**, or **Not completed yet, not tested**. The mark does not describe the problem. The [user guide](docs/guide.md) lists which setting has which mark. Follow playback and the relay are marked do not use. The relay is also marked not completed yet and not tested.

## Guide

[User guide](docs/guide.md)

## Install

GitHub releases are packed with [Inno Setup](installer/TunePeerly.iss). The installer asks whether to install for all users or only for the current user. The default is the current user, at `C:\Users\<name>\AppData\Local\Programs\TunePeerly`, and it does not need administrator rights. Installing for all users uses `C:\Program Files\TunePeerly` and needs administrator rights.

The database and the cache do not follow that choice, and they cannot be changed on their own. The database is always `%APPDATA%\TunePeerly`. Covers and the window cache are always in `%LOCALAPPDATA%\TunePeerly`.

The downloaded installer is `TunePeerly-0.6.0-alpha.1-x64-Setup.exe`. In that file's folder, open PowerShell, compute its SHA256, and compare it with the SHA256 shown for this file on the release page. Install it when the two strings match.

```powershell
Get-FileHash -Algorithm SHA256 .\TunePeerly-0.6.0-alpha.1-x64-Setup.exe
```

## Develop

You need Node.js, npm, a Rust toolchain, and the Microsoft Edge WebView2 Runtime on Windows.

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

### Android

The Android app is in `src-tauri/gen/android` and is still in development. It needs the Android SDK, the Android NDK, and JDK 17 on top of the requirements above; see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). The project has already been generated, so do not run `tauri android init` again.

```powershell
npm run tauri android dev
npm run tauri android build -- --apk
```

`android dev` runs the app on a connected phone and loads the frontend from `build.devUrl`, which is `http://localhost:1420`, so the dev server has to stay running for as long as you use that build.

Do not hand out an APK built with Gradle directly. `gradlew assembleDebug` produces the development variant, which reads the frontend from `devUrl` instead of embedding `frontendDist`, so on its own it opens to `Failed to request http://localhost:1420/` and never shows the interface. An APK that should run without a computer has to come from `tauri android build --apk`, which embeds `frontendDist` and needs a signing configuration in `src-tauri/gen/android/keystore.properties`, as described in [Android signing](https://v2.tauri.app/distribute/sign/android/).

The quickest way to tell the two apart is to install the build on a phone with the computer out of reach: a build that embeds the frontend opens, and a development variant stays on that error screen.

## Installer

Run these from the repository root. Build the release program first, then compile [installer/TunePeerly.iss](installer/TunePeerly.iss). The script takes `src-tauri\target\release\TunePeerly.exe`. [Inno Setup 7](https://jrsoftware.org/isinfo.php) must be installed.

```powershell
npm run tauri build -- --no-bundle
& "${env:ProgramFiles}\Inno Setup 7\ISCC.exe" installer\TunePeerly.iss
```

`npm run tauri build -- --no-bundle` builds the executable only. It does not create an installer. The setup program is written to `installer\Output\TunePeerly-0.6.0-alpha.1-x64-Setup.exe`. `installer\Output` is gitignored. You can also open `installer\TunePeerly.iss` in Inno Setup and compile it.

A compile that stops at `EndUpdateResource failed (110)` means the new setup program was locked while its icons were being written. Windows Defender real-time scanning locks that unsigned exe, and Windows Security does not show a prompt. Wait a few seconds and run the `ISCC.exe` command above again. Do not compile again immediately after the failure. If it is still 110, add the repository's `installer\Output` folder to the Windows Security exclusions and compile again: Virus & threat protection → Manage settings → Exclusions → Add an exclusion → Folder.

## Contributing

The contribution process is in [CONTRIBUTING.md](CONTRIBUTING.md). Open an issue and wait until it is confirmed before writing code.

## Project

- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)

## License

[GNU Affero General Public License v3.0](https://www.gnu.org/licenses/agpl-3.0.html). Additional terms for attribution and modified versions are in [ADDITIONAL_TERMS.md](ADDITIONAL_TERMS.md).
