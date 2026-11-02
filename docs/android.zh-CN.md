# Android

TunePeerly 支持 Android 7.0（API 24）及以上。手机版和电脑版用法一样：在同一个 Wi-Fi 下和你的其他设备配对，播放自己和对方的音乐；不在家时，可以通过你自己的中继连上已配对的设备。

和电脑版一样，Android 版目前是 alpha 版。

## 安装

1. 把 APK 拷到手机上并打开。
2. 系统询问时，允许从该来源安装。
3. 打开 TunePeerly。允许通知，这样通知栏和锁屏上才会有播放控制。

## 添加音乐

Android 不允许应用直接读取你的音乐文件夹。请在 **设置 → 音乐和下载文件夹** 里添加：

- **添加文件夹** 打开系统的文件夹选择器，该文件夹及其子文件夹里的歌曲会被复制到应用里。
- **添加文件** 打开文件选择器，添加单首歌曲。

复制的歌曲保存在应用自己的存储里，卸载应用时会一并删除。从其他设备下载的歌曲也保存在这里。

## 和电脑配对

配对只在局域网里进行，和两台电脑之间配对一样。

1. 让手机和电脑连同一个 Wi-Fi。
2. 两边都打开 **设备 → 发现附近设备**。
3. 点电脑旁边的 **配对**。电脑上会显示一个 6 位配对码，在手机上输入。从电脑发起配对也一样，只是方向相反。

有些路由器会禁止设备互相发现（常叫 AP 隔离、客户端隔离或访客网络）。如果附近一直看不到电脑，先检查这一项。

## 不在家时

配对之后，手机可以通过你自己搭的中继连上电脑。在两台设备的 **设备 → 中继服务器** 里填相同的中继信息，见 [部署中继](relay.zh-CN.md)。两台设备都要把对方设为 **完全信任**。中继只在已配对的设备之间转发数据，不能用来配对新设备。手机在 Wi-Fi 和移动数据之间切换时，会自动重新连上中继和附近的设备。

## 后台播放

- 用返回键或 Home 键离开应用时，音乐会继续播放。要停止，在最近任务里把 TunePeerly 划掉。
- 通知和锁屏上有封面、播放/暂停、上一首/下一首、进度条和收藏按钮。耳机和蓝牙按键也能用。
- 桌面小组件提供同样的控制。

有些手机为了省电会强制停止后台应用。如果熄屏一段时间后播放停了，请在手机的电池设置里允许 TunePeerly 后台运行（realme、ColorOS 等系统：**设置 → 电池 → 应用耗电管理 → TunePeerly → 允许后台行为**）。

## 隐私

- 曲库数据库是加密的，密钥由 Android Keystore 封装，只能在这台手机上打开。
- 应用关闭了 Android 备份，数据库、设备身份和歌曲不会上传到云备份，也不会被迁移到新手机。
- 明文 HTTP 只允许访问手机自己，供应用自己的本地服务使用。

## 从源码构建

Android 版在 Linux 或 macOS 上构建。Windows 上请在 WSL2（Ubuntu）里构建：音乐库数据库用 SQLCipher，构建时要编译 OpenSSL，而 OpenSSL 的构建需要类 Unix 的 Perl 和 `make`。Strawberry Perl 和 Git for Windows 自带的 Perl 都无法为 Android 配置 OpenSSL，所以在原生 Windows 上构建 Android 版会失败。[README](../README.zh-CN.md#开发) 里的 Windows 桌面版构建不受影响。

需要 Node.js、带 Android 目标的 Rust、JDK 17、带 platform 36 和 build tools 36.0.0 的 Android SDK，以及 NDK 30.0.16248370。另见 [Tauri 的 Android 环境要求](https://v2.tauri.app/start/prerequisites/#android)。

### Linux 和 WSL2

Windows 上先装好 WSL2 和 Ubuntu：用管理员身份打开 PowerShell，执行 `wsl --install -d Ubuntu`，之后下面的命令都在 Ubuntu 终端里执行。Linux 上在普通终端里执行即可。这些命令适用于 Ubuntu 和 Debian。

1. 安装系统依赖：

   ```sh
   sudo apt update
   sudo apt install -y build-essential perl make pkg-config git curl unzip openjdk-17-jdk
   ```

2. 安装 Rust 和 Android 目标：

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   source ~/.cargo/env
   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
   ```

3. 用 [nvm](https://github.com/nvm-sh/nvm) 安装 Node.js：

   ```sh
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
   source ~/.bashrc
   nvm install --lts
   ```

4. 安装 Android SDK 命令行工具、SDK 组件和 NDK，不需要 Android Studio。最新命令行工具的下载链接见 [Android Studio 下载页](https://developer.android.com/studio#command-line-tools-only)。

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

   ARM64 机器上，`JAVA_HOME` 是 `/usr/lib/jvm/java-17-openjdk-arm64`。

5. 把仓库克隆到 Linux 的主目录。在 WSL2 里不要在 `/mnt/c` 下的 Windows 文件夹里构建：那样会慢很多倍，而且 Windows 装的 `node_modules` 在 Linux 里不能用。

   ```sh
   cd ~
   git clone https://github.com/zhebuilds/TunePeerly.git
   cd TunePeerly
   npm install
   ```

6. 构建调试版 APK：

   ```sh
   npm run tauri android build -- --debug --target aarch64 --apk
   ```

   APK 在 `src-tauri/gen/android/app/build/outputs/apk/universal/debug/`。

第一次构建要编译 OpenSSL，需要几分钟。

### macOS

用 `xcode-select --install` 安装 Xcode 命令行工具，它带有 `perl` 和 `make`。安装 JDK 17，再用 Android Studio 或 macOS 版命令行工具安装 Android SDK 和 NDK，组件和上面 `sdkmanager` 装的一样。设置好 `JAVA_HOME`、`ANDROID_HOME` 和 `NDK_HOME`，然后执行第 2、3、5、6 步。

### 发布版 APK

发布版 APK 需要签名密钥。只需创建一次，放在仓库之外：

```sh
keytool -genkeypair -keystore ~/tunepeerly-release.jks -alias tunepeerly -keyalg RSA -keysize 3072 -validity 10000
```

然后设置这些环境变量：

```sh
export TUNEPEERLY_KEYSTORE=$HOME/tunepeerly-release.jks
export TUNEPEERLY_KEYSTORE_PASSWORD=...
export TUNEPEERLY_KEY_ALIAS=tunepeerly
export TUNEPEERLY_KEY_PASSWORD=...
```

或者写进 `src-tauri/gen/android/keystore.properties`（git 会忽略这个文件）：

```properties
storeFile=/home/you/tunepeerly-release.jks
storePassword=...
keyAlias=tunepeerly
keyPassword=...
```

再构建：

```sh
npm run tauri android build -- --target aarch64 --apk
```

签好名的 APK 在 `src-tauri/gen/android/app/build/outputs/apk/universal/release/`。在 WSL2 里，用 `cp` 把它复制到 `/mnt/c` 下的文件夹就能在 Windows 上拿到，例如 `cp src-tauri/gen/android/app/build/outputs/apk/universal/release/*.apk /mnt/c/Users/<你的 Windows 用户名>/Downloads/`。

没有密钥时，发布版 APK 不会签名，Android 无法安装。每次更新都必须用同一个密钥签名，否则 Android 不允许覆盖安装旧版本。
