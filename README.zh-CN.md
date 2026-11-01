<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="192" height="192" alt="TunePeerly">
</p>

<h1 align="center">TunePeerly（声渡）</h1>

<p align="center">
  <a href="README.md">English</a> |
  简体中文
</p>

声渡播放这台电脑上已有的音乐，并分享给已配对的设备。没有账号。同一局域网里的已配对设备直接互相发现。你自己运行的中继可以用于局域网之外的连接。配对只在局域网里进行；中继只在已配对的设备之间转发数据，从不配对。

> **注意：** 当前版本是 alpha 测试版，程序的实际效果不保证和[使用手册](docs/guide.zh-CN.md)的说明一致。

## 功能

- 播放本机文件，从已配对设备串流任意一首歌，或把原始文件下载到本机。
- 在局域网发现设备，或按 IPv4 地址和端口连接。配对时，一台电脑显示 6 位配对码，在另一台上输入。配对前必须打开设备页“附近”里的**发现附近设备**，否则无法发现设备。
- 一首歌大约播放 10 秒，或播放到一半，就会记进最近播放。最近播放只留在各自的电脑上，可以查看一台已连接设备的列表，不会保存也不会合并。
- 手动发送或覆盖喜欢的歌曲和外观，不会自动同步。
- 无痕模式会暂停发现、同步和播放记录。也可以拉黑一台设备。
- 浅色和深色主题、中英文、滚动歌词、托盘图标、迷你播放器，以及 Windows 任务栏缩略图上的播放按钮。

## 安全和隐私

- 设备之间只用 TLS 1.3 通信，双方都要证明自己的身份。每台设备有自己的密钥对，设备标识就是它公钥的 SHA-256 值。配对之后，每台设备都记住对方的公钥，对这台设备不接受别的密钥。
- 加密在你的设备之间端到端进行。中继只转发数据，读不到内容。但它能看到每台设备的中继标识、公网 IP、在线时间，以及谁连谁、传了多少数据。详见 [SECURITY.md](SECURITY.md)。
- 经中继的连接只接受已配对的设备。使用中继时，两台设备要互相设为**完全信任**。
- 音乐库数据库用 SQLCipher 加密。它的随机密钥保存在数据库旁边的 `db.key` 里。在 Windows 上这个文件用 DPAPI 保护，只有你的 Windows 账户能打开。在其他系统上，这个文件只有所有者能读（权限 0600）。
- 配对用 6 位配对码：一台电脑显示，在另一台上输入。配对码用 SPAKE2 核对，不会在网络上传送，中间人也无法冒充你完成配对。一个配对码只能用一次，90 秒内有效。输错 3 次后，配对暂停 1 分钟。
- 移除设备会让两边都解除配对：这台电脑删除它的密钥并通知它，它也会从自己的列表里移除这台电脑。对方不在线的话，会在两边下次连上时收到通知。你的其他设备不受影响。要再一起使用，在任一边重新配对，输入新的配对码。

## 手册

[使用手册](docs/guide.zh-CN.md) · [部署中继](docs/relay.zh-CN.md)

## 安装

GitHub 上发布的版本用 [Inno Setup](installer/TunePeerly.iss) 打包。安装程序会询问为所有用户安装，还是只为当前用户安装。默认是当前用户，目录为 `C:\Users\<用户名>\AppData\Local\Programs\TunePeerly`，不需要管理员权限。为所有用户安装时，目录为 `C:\Program Files\TunePeerly`，需要管理员权限。

数据库和缓存不跟着这个选择走，也不能单独修改。数据库始终在 `%APPDATA%\TunePeerly`。封面和界面缓存始终在 `%LOCALAPPDATA%\TunePeerly`。

下载的安装包是 `TunePeerly-0.6.0-alpha.1-x64-Setup.exe`。在这个文件所在的文件夹打开 PowerShell，算出它的 SHA256，再和发布页上这个文件的 SHA256 对照。两串一致再安装。

```powershell
Get-FileHash -Algorithm SHA256 .\TunePeerly-0.6.0-alpha.1-x64-Setup.exe
```

## 开发

需要 Node.js、npm、Rust 工具链、Perl，以及 Windows 上的 Microsoft Edge WebView2 运行库。

音乐库数据库用 SQLCipher 加密。构建时会为它编译一份自带的 OpenSSL，这一步需要 Perl，第一次要几分钟。
- Windows：安装 [Strawberry Perl](https://strawberryperl.com/)，并确认终端里的 `perl` 是 Strawberry Perl，而不是 Git for Windows 自带的 Perl。`nmake` 随 Rust 本来就需要的 Visual Studio C++ 生成工具一起安装。[NASM](https://www.nasm.us/) 可选：PATH 里有 `nasm` 时，OpenSSL 会带上更快的汇编代码；没有时，OpenSSL 不用汇编编译。
- Linux 和 macOS：一般已经装好了 `perl` 和 `make`。

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
