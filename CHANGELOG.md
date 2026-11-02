# Changelog

This file describes each released version. 0.6.0 is the first release under the name TunePeerly (声渡).

## [0.8.0-alpha.1] - 2026-11-02

This is an alpha test release. It adds an Android app and changes how devices pair, connect, and sync.

TunePeerly now runs on Android phones (arm64). The phone layout has a tab bar, a mini player, and a top bar menu. Songs play in a native Media3 player, so the queue keeps playing with the screen locked, with repeat, shuffle, and controls on the lock screen and in the notification. Imported music goes into the app's own storage. The library database key is kept in the Android Keystore. The phone uses its own device name, caches the next songs from linked devices, and redials when the network changes. Android is built in WSL2 on Windows, and the Android build guide lists the setup.

Pairing uses a one-time 6-digit code (SPAKE2). Devices talk over TLS 1.3 and accept only the keys saved when pairing. libsignal was removed. The library database is encrypted with SQLCipher. On Windows its key is protected by DPAPI. Unpairing removes the pairing on both sides, also when the other device is offline. The relay never pairs, and only devices that are already paired can connect through it.

The LAN port is picked automatically. Discovery broadcasts every 15 seconds, and Nearby has a refresh button. Connecting by a typed address was removed. Paired devices stay in the Paired list when they disconnect. Their addresses are remembered and redialed with backoff and keepalive, and the relay is used as a fallback.

Follow playback and Connect mode were removed. Each device plays on its own. Every song is visible to paired devices, and per-song exposure was removed. Songs from linked devices use stable stream URLs that pick an online device for each request. Sync is manual: Send or Override, per device, and the receiving device chooses to accept silently, ask, or deny. Automatic sync, history sync, and conflict handling were removed. A paired device's Recently played is read-only. Search always covers all music, and the scope buttons were removed.

Settings has a This computer section with the device name, network information, and device ID, followed by the music and download folders. Relay settings moved to the Devices page. The unfinished-setting marks were removed. The About page is shorter. Database migrations and backward-compatibility code were removed.

## [0.6.0] - 2026-10-19

TunePeerly plays local audio from the original file, streams an exposed song from a paired device, and can download that file into the library. Pairing is by LAN discovery or by IPv4 address and port. Both computers must accept. Sync traffic is end-to-end encrypted.

Liked songs and appearance can sync as a full copy. Recently played is recorded after about 10 seconds, or half the song, and can merge by UTC time. Incognito cuts device connections and stops discovery, sync, and history recording. Device block and per-item sync blocks are included. The app has light and dark themes, English and Chinese, lyrics, a tray icon, a mini player, and playback buttons on the Windows taskbar thumbnail.

The program is `TunePeerly.exe`. The database is in `%APPDATA%\TunePeerly`. Covers and the window cache are in `%LOCALAPPDATA%\TunePeerly`.

Some settings are marked because they are not finished. **To be revised** means this version keeps the current behavior. **Do not use** means do not use it, because the feature has problems. **Not completed yet, not tested** and **Do not use** are on Relay. Its client is included. It is not completed yet, and it has not been tested. The guide lists which setting has which mark. The mark does not describe the problem. Follow playback is also marked do not use.

## 更新日志

这里说明每个发布版本的情况。0.6.0 是以 TunePeerly（声渡）这个名字发布的第一个版本。

## [0.8.0-alpha.1] - 2026-11-02

这是 alpha 测试版。这个版本加入了安卓应用，并改变了设备配对、连接和同步的方式。

TunePeerly 现在可以在安卓手机（arm64）上运行。手机界面有底部标签栏、迷你播放器和顶栏菜单。歌曲由原生的 Media3 播放器播放，锁屏后队列会继续播放，支持循环、随机，锁屏和通知栏上有播放控制。导入的音乐保存在应用自己的存储里。音乐库数据库的密钥保存在安卓 Keystore 中。手机使用自己的设备名，会提前缓存来自已连接设备的后几首歌，网络变化时会重新连接。安卓版在 Windows 上通过 WSL2 编译，安卓编译手册列出了需要的环境。

配对使用一次性的 6 位配对码（SPAKE2）。设备之间使用 TLS 1.3 通信，只接受配对时保存的密钥。移除了 libsignal。音乐库数据库用 SQLCipher 加密，在 Windows 上它的密钥由 DPAPI 保护。取消配对会在两边同时生效，对方离线时也一样。中继从不配对，只有已经配对的设备才能通过它连接。

局域网端口自动选择。设备发现每 15 秒广播一次，附近设备有刷新按钮。移除了手动输入地址连接。已配对设备断开后仍留在已配对列表里。程序会记住它们的地址，并按退避间隔重新连接、保持心跳，连不上时改用中继。

移除了跟随播放和连接模式，每台设备各自播放。所有歌曲对已配对设备都可见，移除了按歌曲公开的设置。来自已连接设备的歌曲使用固定的串流地址，每次请求时挑选一台在线设备。同步改为手动：按设备发送或覆盖，接收方可以选择静默接受、询问或拒绝。移除了自动同步、播放记录同步和冲突处理。已配对设备的最近播放是只读的。搜索总是覆盖全部音乐，移除了搜索范围按钮。

设置里新增“这台电脑”，包括设备名、网络信息和设备 ID，后面是音乐文件夹和下载文件夹。中继设置移到了设备页面。移除了未完成设置的标签。关于页面更简洁了。移除了数据库迁移和向后兼容代码。

## [0.6.0] - 2026-10-19

TunePeerly 从原始文件播放本机音频，可以从已配对设备串流已公开的歌曲，也可以把那个文件下载进曲库。配对通过局域网发现，或通过 IPv4 地址和端口。两台电脑都要接受。同步数据端到端加密。

喜欢的歌曲和外观可以按整份副本同步。最近播放在大约 10 秒或歌曲一半之后记下，并可以按 UTC 时间合并。无痕模式会断开设备连接，并停止发现、同步和播放记录。包含拉黑设备，以及按项目阻止同步。程序有浅色和深色主题、中英文、歌词、托盘图标、迷你播放器，以及 Windows 任务栏缩略图上的播放按钮。

程序名是 `TunePeerly.exe`。数据库位于 `%APPDATA%\TunePeerly\library.db`。封面在 `%LOCALAPPDATA%\TunePeerly\covers`。

有些设置因为还没完成而标了标签。**待修改**表示这个版本先保持现在的行为。**不要使用**就是不要使用，这个功能有问题。**待完善且未测试**和**不要使用**标在中继上：客户端在这个版本里，但还没完善，也还没测试。手册列出了每个标签对应哪些设置。标签不说明具体问题。跟随播放也标着不要使用。
