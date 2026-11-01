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

需要 Node.js、带 Android 目标的 Rust、Android SDK 和 NDK，以及 JDK 17。见 [Tauri 的 Android 环境要求](https://v2.tauri.app/start/prerequisites/#android)。

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
npm install
npm run tauri android build -- --debug --target aarch64 --apk
```

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

没有密钥时，发布版 APK 不会签名，Android 无法安装。每次更新都必须用同一个密钥签名，否则 Android 不允许覆盖安装旧版本。
