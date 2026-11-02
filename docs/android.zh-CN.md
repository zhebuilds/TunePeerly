# Android

TunePeerly 支持 Android 7.0（API 24）及以上。手机版和电脑版用法一样：在同一个 Wi-Fi 下和你的其他设备配对，播放自己和对方的音乐；不在家时，可以通过你自己的中继连上已配对的设备。

和电脑版一样，Android 版目前是 alpha 版。

## 安装

1. 把 APK 拷到手机上并打开。
2. 系统询问时，允许从该来源安装。
3. 打开 TunePeerly。在 Android 13 及以上，第一次播放歌曲时系统会询问是否允许 TunePeerly 显示通知。请允许，这样通知栏和锁屏上才会有播放控制。

## 添加音乐

Android 不允许应用直接读取你的音乐文件夹。请在 **设置 → 音乐和下载文件夹** 里添加：

- **添加文件夹** 打开系统的文件夹选择器，该文件夹及其子文件夹里的歌曲会被复制到应用里。
- **添加文件** 打开文件选择器，添加单首歌曲。

复制的歌曲保存在应用自己的存储里，卸载应用时会一并删除。从其他设备下载的歌曲也保存在这里。

## 和电脑配对

配对只在局域网里进行，和两台电脑之间配对一样。

1. 让手机和电脑连同一个 Wi-Fi。也可以打开手机热点，让电脑连到这个热点上。
2. 两边都打开 **设备 → 发现附近设备**。
3. 点电脑旁边的 **配对**。电脑上会显示一个 6 位配对码，在手机上输入。从电脑发起配对也一样，只是方向相反。

有些路由器会禁止设备互相发现（常叫 AP 隔离、客户端隔离或访客网络）。如果附近一直看不到电脑，先检查这一项，或者改用手机热点配对。

## 不在家时

配对之后，手机可以通过你自己搭的中继连上电脑。在两台设备的 **设备 → 中继服务器** 里填相同的中继信息，见 [部署中继](relay.zh-CN.md)。两台设备都要把对方设为 **完全信任**。中继只在已配对的设备之间转发数据，不能用来配对新设备。手机在 Wi-Fi 和移动数据之间切换时，会自动重新连上中继和附近的设备。

## 后台播放

- 用返回键或 Home 键离开应用时，音乐会继续播放。要停止，在最近任务里把 TunePeerly 划掉。
- 通知和锁屏上有封面、播放/暂停、上一首/下一首、进度条和收藏按钮。耳机和蓝牙按键也能用。
- 桌面小组件提供同样的控制。

有些手机为了省电会强制停止后台应用。如果熄屏一段时间后播放停了，请在手机的电池设置里允许 TunePeerly 后台运行（realme、ColorOS 等系统：**设置 → 电池 → 应用耗电管理 → TunePeerly → 允许后台行为**）。

## 和电脑版的不同

- 手机还没有自己的名称，或者名称只是占位的 “localhost” 时，会采用 Android 设置里的设备名。你在设置 → **这台电脑**里设置的名称会保留。
- 导入的歌曲会复制进应用里，从其他设备下载的歌曲保存在应用自己的文件夹里。没有可选的下载文件夹，也没有**在文件夹中显示**。
- 没有托盘、迷你播放器和关闭窗口的设置。返回键先关掉打开的东西，再返回上一页，在第一页时会把应用送到后台，而不是关闭它。
- 歌曲由后台服务里的原生播放器播放，所以锁屏时队列、循环和随机仍然有效。来自其他设备的下一首歌会提前复制到最多 200 MB 的缓存里。如果来自其他设备的歌曲加载不了，例如那台设备离线，播放器会在重新连接的同时最多尝试 5 分钟，然后才跳到下一首。
- 在 Android 13 及以上，TunePeerly 会在播放歌曲时请求通知权限。如果你拒绝了，请在 Android 设置里为 TunePeerly 打开通知。

## 隐私

- 曲库数据库是加密的，密钥由 Android Keystore 封装，只能在这台手机上打开。如果 TunePeerly 读不到 Keystore 里的密钥，它会停止启动，而不是新建一个曲库，这样一次偶发的错误不会清掉你的曲库和配对。请再打开一次应用。如果一直失败，在 Android 设置里清除应用的存储，会重新开始一个新曲库，其他设备要重新配对。
- 应用关闭了 Android 备份，数据库、设备身份和歌曲不会上传到云备份，也不会被迁移到新手机。
- 明文 HTTP 只允许访问手机自己，供应用自己的本地服务使用。

## 从源码构建

Android 版在 Linux 上构建。Windows 上请在 WSL2（Ubuntu）里构建：音乐库数据库用 SQLCipher，构建时要编译 OpenSSL，而 OpenSSL 的构建需要类 Unix 的 Perl 和 `make`。Strawberry Perl 和 Git for Windows 自带的 Perl 都无法为 Android 配置 OpenSSL，所以在原生 Windows 上构建 Android 版会失败。[README](../README.zh-CN.md#开发) 里的 Windows 桌面版构建不受影响。

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

   ARM64 机器上，`JAVA_HOME` 是 `/usr/lib/jvm/java-17-openjdk-arm64`。

   NDK r23 起已经没有 `aarch64-linux-android-ranlib`，编译 OpenSSL 时会找不到它，所以要用 `AR_aarch64_linux_android`、`RANLIB_aarch64_linux_android` 和 `RANLIB` 指定 NDK 自带的 `llvm-ar` 和 `llvm-ranlib`。

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

### 验证官方 APK

官方 APK 发布在 [GitHub Releases](https://github.com/zhebuilds/TunePeerly/releases)，签名证书的 SHA-256 指纹是：

```
f02eb081eabeb8b1a4b77ab1d4a7325722d6cda978226066e15ef423e759f2f0
```

可以用 Android SDK 的 `apksigner` 核对：

```sh
apksigner verify --print-certs TunePeerly-<版本>-arm64-v8a.apk
```

输出里 `certificate SHA-256 digest` 那一行应该和上面一致。下载的文件可以用发布页上的 `SHA256SUMS.txt` 核对是否完整。

自己构建的发布版需要用自己的密钥签名（通过 `TUNEPEERLY_KEYSTORE`、`TUNEPEERLY_KEYSTORE_PASSWORD`、`TUNEPEERLY_KEY_ALIAS`、`TUNEPEERLY_KEY_PASSWORD` 环境变量设置）。它的签名和官方版本不同，不能覆盖安装官方版本，反过来也一样。
