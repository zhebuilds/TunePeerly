<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="192" height="192" alt="TunePeerly">
</p>

<h1 align="center">TunePeerly（声渡）</h1>

<p align="center">
  <a href="README.md">English</a> |
  简体中文
</p>

声渡播放这台电脑上已有的音乐，并分享给已配对的设备。没有账号。同一局域网里的已配对设备直接互相发现。你自己运行的中继可以用于局域网之外的连接。

> **注意：** 当前版本是 alpha 测试版，程序的实际效果不保证和[使用手册](docs/guide.zh-CN.md)的说明一致。

## 功能

- 播放本机文件，从已配对设备串流一首已公开的歌，或把原始文件下载到本机。
- 在局域网发现设备，或按 IPv4 地址和端口连接。双方都要确认。同步数据使用 libsignal 端到端加密。配对前必须打开设置 → 网络里的**发现附近设备**，否则无法发现设备。
- 最近播放按最新的 UTC 时间合并。一首歌大约播放 10 秒，或播放到一半，就会记下来。可以在这台电脑上线时再合并一次。
- 喜欢的歌曲和外观的同步和跟随一台在线设备的播放。
- 无痕模式会暂停发现、同步和播放记录。也可以拉黑一台设备，或按项目停止手动同步和跟随。
- 浅色和深色主题、中英文、滚动歌词、托盘图标、迷你播放器，以及 Windows 任务栏缩略图上的播放按钮。
- 有些设置标着**待修改**、**不要使用**或**待完善且未测试**。标签不说明具体问题。[使用手册](docs/guide.zh-CN.md)列出了每个标签对应哪些设置。跟随播放和中继标着不要使用。中继还标着待完善且未测试。

## 手册

[使用手册](docs/guide.zh-CN.md)

## 安装

GitHub 上发布的版本用 [Inno Setup](installer/TunePeerly.iss) 打包。安装程序会询问为所有用户安装，还是只为当前用户安装。默认是当前用户，目录为 `C:\Users\<用户名>\AppData\Local\Programs\TunePeerly`，不需要管理员权限。为所有用户安装时，目录为 `C:\Program Files\TunePeerly`，需要管理员权限。

数据库和缓存不跟着这个选择走，也不能单独修改。数据库始终在 `%APPDATA%\TunePeerly`。封面和界面缓存始终在 `%LOCALAPPDATA%\TunePeerly`。

下载的安装包是 `TunePeerly-0.6.0-alpha.1-x64-Setup.exe`。在这个文件所在的文件夹打开 PowerShell，算出它的 SHA256，再和发布页上这个文件的 SHA256 对照。两串一致再安装。

```powershell
Get-FileHash -Algorithm SHA256 .\TunePeerly-0.6.0-alpha.1-x64-Setup.exe
```

## 开发

需要 Node.js、npm、Rust 工具链，以及 Windows 上的 Microsoft Edge WebView2 运行库。

```powershell
npm install
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run tauri build
```

如果 `npm run tauri dev` 停在 `listen EACCES: permission denied ::1:1420`，是 Windows NAT 保留了 1420 端口，并没有程序在监听它。用管理员身份打开 PowerShell，执行下面两条命令，再重新运行 `npm run tauri dev`。停止 Windows NAT 期间，已有的 Hyper-V、WSL 和 Docker NAT 连接会断开，服务重新启动后恢复。

```powershell
net stop winnat
net start winnat
```

### Android

安卓版在 `src-tauri/gen/android`，还在开发中。除了上面的环境，还需要 Android SDK、Android NDK 和 JDK 17，可参考 [Tauri 的前置要求](https://v2.tauri.app/start/prerequisites/)。工程已经生成过，不要再执行 `tauri android init`。

```powershell
npm run tauri android dev
npm run tauri android build -- --apk
```

`android dev` 会把应用跑在连着的手机上，界面从 `build.devUrl` 也就是 `http://localhost:1420` 取，所以用这类包的时候，开发服务器要一直开着。

不要直接把用 Gradle 打出来的包交给别人。`gradlew assembleDebug` 打的是开发版，界面从 `devUrl` 取，而不是把 `frontendDist` 打进包里，单独装到手机上只会停在 `Failed to request http://localhost:1420/`，一直看不到界面。要脱离电脑运行，就得用 `tauri android build --apk`，它会把 `frontendDist` 打进包，并且需要 `src-tauri/gen/android/keystore.properties` 里的签名配置，见 [Android 签名](https://v2.tauri.app/distribute/sign/android/)。

最简单的区分办法：把包装到手机上，电脑不在旁边时能打开界面的是正式构建，停在报错页的是开发版。

## 安装包

在仓库根目录执行。先生成发布版程序，再编译 [installer/TunePeerly.iss](installer/TunePeerly.iss)。脚本从 `src-tauri\target\release\TunePeerly.exe` 取文件，需要已安装的 [Inno Setup 7](https://jrsoftware.org/isinfo.php)。

```powershell
npm run tauri build -- --no-bundle
& "${env:ProgramFiles}\Inno Setup 7\ISCC.exe" installer\TunePeerly.iss
```

`npm run tauri build -- --no-bundle` 只生成可执行文件，不生成安装包。安装包写到 `installer\Output\TunePeerly-0.6.0-alpha.1-x64-Setup.exe`。`installer\Output` 被 git 忽略。也可以用 Inno Setup 打开 `installer\TunePeerly.iss` 后编译。

编译停在 `EndUpdateResource failed (110)` 时，是往新安装包写入图标的时候文件被占用了。Windows Defender 实时扫描会锁住这个未签名的 exe，安全中心不会弹出提示。等几秒后重新执行上面的 `ISCC.exe` 命令，不要紧接着失败立刻再编译。如果还是 这样，把仓库里的 `installer\Output` 加进 Windows 安全中心的排除项后再编译。路径是：病毒和威胁防护 → 管理设置 → 排除项 → 添加排除项 → 文件夹。

## 贡献

贡献流程在 [CONTRIBUTING.md](CONTRIBUTING.md)。先开 issue，讨论确定后再开发。

## 项目

- [更新日志](CHANGELOG.md)
- [安全策略](SECURITY.md)

## 许可证

[GNU Affero General Public License v3.0](https://www.gnu.org/licenses/agpl-3.0.html)。关于署名和修改版的附加条款在 [ADDITIONAL_TERMS.md](ADDITIONAL_TERMS.md)。
