# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.8.x (alpha) | Yes |
| 0.6.x | No |

## Reporting a vulnerability

Report privately. Do not open a public GitHub issue, and do not post proof-of-concept details in a pull request.

Use [a private security advisory](https://github.com/zhebuilds/TunePeerly/security/advisories/new).

Include the TunePeerly version, the operating system, the steps to reproduce, and what an attacker gains. If you can, name the pairing, sync, or streaming path involved.

Please give the maintainer time to confirm and ship a fix before any public disclosure.

## In scope

- Pairing and device identity, including accepting a device the user did not confirm
- End-to-end encryption of sync and signaling traffic, including anything the configured relay can read
- Reading the encrypted library database or its key file `db.key` without the user's account (on Windows) or without the app's own storage and Keystore (on Android)
- A removed device syncing with, or streaming from, the computer that removed it without a new pairing code, or any device other than the paired one making a computer forget a pairing
- Applying a sync copy, or showing a Recently Played list, from a device that is not paired and confirmed
- Streaming or downloading a song from this computer without being a paired, connected device, or while it is in incognito
- Reading or writing files outside the library, the download folder, or the app data folder

## Pairing

- A device is paired with a 6-digit code that the asked device shows and the asking device types. Both run SPAKE2 with the code over the TLS connection they already have. The code never crosses the network, and both sides bind the result to the two TLS public keys they saw, so someone in the middle fails even if they know the code.
- A code works once and expires after 90 seconds. A wrong code is replaced by a new one. After 3 wrong codes, pairing is paused for 1 minute. The pause covers every device, so someone on the same network can pause pairing for a minute; they cannot pair.
- After pairing, each device keeps the other's public key and accepts no other key for that device. A device ID is the SHA-256 hash of its public key.

## Relay

- Pairing happens only on the local network. The relay only carries data between devices that are already paired, and never pairs. Pairing messages are dropped on a relay connection, and a relay connection from a device that is not paired is closed at once.
- The relay cannot read the content: devices talk over TLS 1.3 end to end, and each side checks the other's pinned key.
- The relay does see metadata: each device's relay ID (its certificate ID), its public IP, when it is online, and who connects to whom and how much data.
- strelaysrv checks the token only when a device registers, not when someone asks it for a session. So anyone who knows the relay address and a device's relay ID can ask for a session and learn whether that device is online. The app refuses every device that is not paired, so they get no data. A device's relay ID is exchanged only during pairing on the local network.

## Out of scope

- What discovery announces on the local network: the device ID, the device name, the TCP port, and the LAN IPv4 addresses. It never announces song names. Discovery is off until the user turns it on.
- A person who already confirmed the pairing on both computers.
- The Windows Firewall prompt. The app does not create that rule.
- Vulnerabilities in dependencies that do not affect TunePeerly.

## 安全策略

### 支持的版本

| 版本 | 是否支持 |
| --- | --- |
| 0.8.x（alpha） | 是 |
| 0.6.x | 否 |

### 如何私下报告

请私下报告。不要开公开的 GitHub issue，也不要在 pull request 里贴出利用细节。

使用[私人安全公告](https://github.com/zhebuilds/TunePeerly/security/advisories/new)。

请写上 TunePeerly 版本、操作系统、复现步骤，以及攻击者能得到什么。如果可以，请指出涉及的是配对、同步还是串流。

在维护者确认并发布修复之前，请留出时间，不要公开披露。

### 范围内

- 配对和设备身份，包括未经用户确认就接受一台设备。
- 同步和信令的端到端加密，包括你配置的中继能够读到的内容。
- 在 Windows 上不用用户的账户，或在 Android 上不用应用自己的存储和 Keystore，就能读取加密的音乐库数据库或密钥文件 `db.key`。
- 被移除的设备不用新的配对码就能再和移除它的电脑同步或串流，或者不是那台已配对的设备却能让一台电脑删掉这条配对。
- 采用未配对或未确认设备发来的同步副本，或显示它发来的最近播放。
- 不是已配对并已连接的设备，或这台电脑开着无痕模式时，仍能串流或下载它的歌曲。
- 读写曲库、下载文件夹和应用数据文件夹以外的文件。

### 配对

- 配对用 6 位配对码：被请求配对的设备显示，发起配对的设备输入。双方在已经建立的 TLS 连接上用这个配对码运行 SPAKE2。配对码不会在网络上传送，双方还会把结果和各自看到的两把 TLS 公钥绑在一起，所以中间人即使知道配对码也会失败。
- 一个配对码只能用一次，90 秒后过期。输错后会换成新的配对码。输错 3 次后，配对暂停 1 分钟。这个暂停对所有设备生效，所以同一网络里的人可以让配对暂停一分钟，但配对不上。
- 配对之后，每台设备都记住对方的公钥，对这台设备不接受别的密钥。设备标识就是它公钥的 SHA-256 值。

### 中继

- 配对只在局域网里进行。中继只在已配对的设备之间转发数据，从不配对。经中继收到的配对消息会被丢弃，未配对设备经中继连进来会被立刻断开。
- 中继读不到内容：设备之间用端到端 TLS 1.3 通信，双方都核对配对时记住的公钥。
- 中继能看到元数据：每台设备的中继标识（证书标识）、公网 IP、在线时间，以及谁连谁、传了多少数据。
- strelaysrv 只在设备注册时检查 token，有人请求会话时不检查。所以知道中继地址和某台设备中继标识的人，可以请求会话，看出这台设备在不在线。应用会拒绝所有未配对的设备，他拿不到任何数据。设备的中继标识只在局域网配对时交换。

### 范围外

- 发现在局域网上宣布的内容：设备标识、设备名、TCP 端口和局域网 IPv4 地址。它不会宣布歌名。用户打开之前，发现是关闭的。
- 已经在两台电脑上确认过配对的人。
- Windows 防火墙的询问框。程序不创建那条规则。
- 不影响 TunePeerly 的依赖漏洞。
