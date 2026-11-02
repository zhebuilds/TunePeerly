# Android

TunePeerly runs on Android 7.0 (API 24) and later. The phone app works like the desktop app: it pairs with your other devices on the same Wi-Fi, plays your music and theirs, and can reach paired devices through your own relay when you are away from home.

The Android app is alpha, like the desktop app.

## Install

1. Copy the APK to the phone and open it.
2. Allow installs from that app when Android asks.
3. Open TunePeerly. On Android 13 and newer, Android asks the first time a song plays whether TunePeerly may show notifications. Allow it, so the playback controls show up in the notification shade and on the lock screen.

## Add music

Android does not let apps read your music folders directly. Use **Settings → Music and download folders**:

- **Add folder** opens the system folder picker. The songs in that folder and its subfolders are copied into the app.
- **Add files** opens the file picker for single songs.

The copies live in the app's own storage, so they are removed when you uninstall the app. Songs you download from other devices are kept there too.

## Pair with a computer

Pairing happens only on the local network, the same as between two computers.

1. Put the phone and the computer on the same Wi-Fi.
2. On both, turn on **Devices → Discover nearby devices**.
3. Tap **Pair** next to the computer. The computer shows a 6-digit code; type it on the phone. Pairing from the computer works the same way the other way round.

Some routers block devices from seeing each other (often called AP or client isolation, or a guest network). If the computer never shows up under Nearby, check that first.

## Away from home

After pairing, the phone can reach the computer through a relay you run yourself. Enter the same relay details on both devices under **Devices → Relay server**; see [Deploy a relay](relay.md). Both devices must set each other to **Full trust**. The relay only carries data between devices that are already paired; it cannot pair new devices. When the phone switches between Wi-Fi and mobile data, it reconnects to the relay and to nearby devices by itself.

## Playing in the background

- Music keeps playing when you leave the app with the back button or the home button. To stop it, swipe TunePeerly away in the recent apps list.
- The notification and the lock screen show the cover, play and pause, next and previous, the seek bar, and a favorite button. Headset and Bluetooth buttons work too.
- There is a home screen widget with the same controls.

Some phones stop background apps aggressively to save battery. If playback stops after a while with the screen off, allow TunePeerly to run in the background in the phone's battery settings (on realme, ColorOS and similar systems: **Settings → Battery → App battery management → TunePeerly → Allow background activity**).

## What is different from the computer app

- When the phone has no name of its own yet, or only the placeholder "localhost", it takes the device name from Android's settings. A name you set under Settings → **This computer** stays.
- Imported songs are copied into the app, and downloads from other devices go to the app's own folder. There is no downloads folder to choose and no **Show in folder**.
- There is no tray, mini player, or close-window setting. The back button closes whatever is open, then goes back a page, and at the first page it sends the app to the background instead of closing it.
- Songs play in a native player in a background service, so the queue, repeat, and shuffle keep working with the screen locked. The next song from another device is copied ahead into a cache of up to 200 MB. If a song from another device cannot be loaded, for example because that device is offline, the player keeps trying for up to 5 minutes, while the link is called again, before it moves on.
- On Android 13 and newer, TunePeerly asks for the notification permission when a song plays. If you declined, turn notifications on for TunePeerly in Android settings.

## Privacy

- The library database is encrypted. Its key is sealed with the Android Keystore, so it only opens on this phone. If TunePeerly cannot read the key from the Keystore, it stops starting instead of creating a new library, so a passing error does not wipe your library or your pairings. Open the app again. If it keeps failing, clearing the app's storage in Android settings starts a new library, and the other devices have to be paired again.
- Android backup is turned off for the app, so the database, the device identity, and your songs are not uploaded to a cloud backup or copied to a new phone.
- Plain HTTP is allowed only to the phone itself, for the app's own local server.

## Build from source

Android builds run on Linux. On Windows, build in WSL2 (Ubuntu). The library database uses SQLCipher, and its build compiles OpenSSL, which needs a Unix-style Perl and `make`. Strawberry Perl and the Perl that comes with Git for Windows cannot configure OpenSSL for Android, so a native Windows Android build fails. The Windows desktop build in the [README](../README.md#develop) is not affected.

You need Node.js, Rust with the Android targets, JDK 17, the Android SDK with platform 36 and build tools 36.0.0, and NDK 30.0.16248370. See also the [Tauri Android prerequisites](https://v2.tauri.app/start/prerequisites/#android).

### Linux and WSL2

On Windows, install WSL2 with Ubuntu first by running `wsl --install -d Ubuntu` in PowerShell as administrator, then run every command below in the Ubuntu terminal. On Linux, run them in a normal terminal. The commands are for Ubuntu and Debian.

1. Install the system packages:

   ```sh
   sudo apt update
   sudo apt install -y build-essential perl make pkg-config git curl unzip openjdk-17-jdk
   ```

2. Install Rust and the Android targets:

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   source ~/.cargo/env
   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
   ```

3. Install Node.js with [nvm](https://github.com/nvm-sh/nvm):

   ```sh
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
   source ~/.bashrc
   nvm install --lts
   ```

4. Install the Android SDK command-line tools, the SDK packages, and the NDK. Android Studio is not needed. The download link for the newest command-line tools is on the [Android Studio download page](https://developer.android.com/studio#command-line-tools-only).

   ```sh
   mkdir -p ~/android-sdk/cmdline-tools && cd ~/android-sdk/cmdline-tools
   curl -o tools.zip https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip
   unzip -q tools.zip && mv cmdline-tools latest && rm tools.zip
   cat >> ~/.bashrc <<'ENV'
   export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
   export ANDROID_HOME=$HOME/android-sdk
   export NDK_HOME=$ANDROID_HOME/ndk/30.0.16248370
   export NDK_BIN=$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
   export AR_aarch64_linux_android=$NDK_BIN/llvm-ar
   export RANLIB_aarch64_linux_android=$NDK_BIN/llvm-ranlib
   export RANLIB=$NDK_BIN/llvm-ranlib
   export PATH=$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH
   ENV
   source ~/.bashrc
   yes | sdkmanager --licenses
   sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0" "ndk;30.0.16248370"
   ```

   On an ARM64 machine, `JAVA_HOME` is `/usr/lib/jvm/java-17-openjdk-arm64`.

   NDK r23 and later no longer ship `aarch64-linux-android-ranlib`, which the OpenSSL build looks for. `AR_aarch64_linux_android`, `RANLIB_aarch64_linux_android`, and `RANLIB` point it to the NDK's `llvm-ar` and `llvm-ranlib` instead.

5. Clone the repository into your Linux home folder. In WSL2, do not build from a Windows folder under `/mnt/c`: it is many times slower, and `node_modules` installed by Windows does not work in Linux.

   ```sh
   cd ~
   git clone https://github.com/zhebuilds/TunePeerly.git
   cd TunePeerly
   npm install
   ```

6. Build a debug APK:

   ```sh
   npm run tauri android build -- --debug --target aarch64 --apk
   ```

   The APK is in `src-tauri/gen/android/app/build/outputs/apk/universal/debug/`.

The first build compiles OpenSSL and takes a few minutes.

### Verifying official APKs

Official APKs are published on [GitHub Releases](https://github.com/zhebuilds/TunePeerly/releases). The SHA-256 fingerprint of the signing certificate is:

```
f02eb081eabeb8b1a4b77ab1d4a7325722d6cda978226066e15ef423e759f2f0
```

Check it with `apksigner` from the Android SDK:

```sh
apksigner verify --print-certs TunePeerly-<version>-arm64-v8a.apk
```

The `certificate SHA-256 digest` line should match the value above. Check that a download is complete with `SHA256SUMS.txt` from the release page.

A release build you make yourself has to be signed with your own key, set through the `TUNEPEERLY_KEYSTORE`, `TUNEPEERLY_KEYSTORE_PASSWORD`, `TUNEPEERLY_KEY_ALIAS`, and `TUNEPEERLY_KEY_PASSWORD` environment variables. Its signature differs from the official one, so it cannot be installed over an official release, or the other way round.
