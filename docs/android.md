# Android

TunePeerly runs on Android 7.0 (API 24) and later. The phone app works like the desktop app: it pairs with your other devices on the same Wi-Fi, plays your music and theirs, and can reach paired devices through your own relay when you are away from home.

The Android app is alpha, like the desktop app.

## Install

1. Copy the APK to the phone and open it.
2. Allow installs from that app when Android asks.
3. Open TunePeerly. Allow notifications so the playback controls show up in the notification shade and on the lock screen.

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

## Privacy

- The library database is encrypted. Its key is sealed with the Android Keystore, so it only opens on this phone.
- Android backup is turned off for the app, so the database, the device identity, and your songs are not uploaded to a cloud backup or copied to a new phone.
- Plain HTTP is allowed only to the phone itself, for the app's own local server.

## Build from source

Android builds run on Linux or macOS. On Windows, build in WSL2 (Ubuntu). The library database uses SQLCipher, and its build compiles OpenSSL, which needs a Unix-style Perl and `make`. Strawberry Perl and the Perl that comes with Git for Windows cannot configure OpenSSL for Android, so a native Windows Android build fails. The Windows desktop build in the [README](../README.md#develop) is not affected.

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
   export PATH=$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH
   ENV
   source ~/.bashrc
   yes | sdkmanager --licenses
   sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0" "ndk;30.0.16248370"
   ```

   On an ARM64 machine, `JAVA_HOME` is `/usr/lib/jvm/java-17-openjdk-arm64`.

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

### macOS

Install the Xcode command line tools with `xcode-select --install`, which provide `perl` and `make`. Install JDK 17, and install the Android SDK and NDK with Android Studio or the macOS command-line tools, using the same `sdkmanager` packages as above. Set `JAVA_HOME`, `ANDROID_HOME`, and `NDK_HOME`, then run steps 2, 3, 5, and 6.

### Release APK

A release APK needs a signing key. Create one once and keep it outside the repository:

```sh
keytool -genkeypair -keystore ~/tunepeerly-release.jks -alias tunepeerly -keyalg RSA -keysize 3072 -validity 10000
```

Then either set these environment variables:

```sh
export TUNEPEERLY_KEYSTORE=$HOME/tunepeerly-release.jks
export TUNEPEERLY_KEYSTORE_PASSWORD=...
export TUNEPEERLY_KEY_ALIAS=tunepeerly
export TUNEPEERLY_KEY_PASSWORD=...
```

or write them to `src-tauri/gen/android/keystore.properties`, which git ignores:

```properties
storeFile=/home/you/tunepeerly-release.jks
storePassword=...
keyAlias=tunepeerly
keyPassword=...
```

and build:

```sh
npm run tauri android build -- --target aarch64 --apk
```

The signed APK is in `src-tauri/gen/android/app/build/outputs/apk/universal/release/`. In WSL2, copy it to Windows with `cp` into a folder under `/mnt/c`, for example `cp src-tauri/gen/android/app/build/outputs/apk/universal/release/*.apk /mnt/c/Users/<your Windows user>/Downloads/`.

Without a key the release APK is built unsigned, and Android will not install it. Every update has to be signed with the same key, or Android refuses to install it over the old version.
