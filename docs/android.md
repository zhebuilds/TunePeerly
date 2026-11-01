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

You need Node.js, Rust with the Android targets, the Android SDK and NDK, and JDK 17. See the [Tauri Android prerequisites](https://v2.tauri.app/start/prerequisites/#android).

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
npm install
npm run tauri android build -- --debug --target aarch64 --apk
```

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

Without a key the release APK is built unsigned, and Android will not install it. Every update has to be signed with the same key, or Android refuses to install it over the old version.
