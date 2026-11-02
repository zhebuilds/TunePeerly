# 部署中继

下面的步骤在 Ubuntu 服务器上搭一个私有的 strelaysrv 中继，已在 Azure 的 Ubuntu 虚拟机上验证。把 `<ip>`、`<relay-id>`、`RG`、`VM` 换成你自己的值。

中继只在已配对的设备之间转发数据，配对只在局域网里进行。中继能看到什么，见 [SECURITY.md](../SECURITY.md)。

## 1. 安装

```sh
sudo apt update
sudo apt install -y syncthing-relaysrv
sudo systemctl disable --now strelaysrv 2>/dev/null
sudo systemctl disable --now syncthing-relaysrv 2>/dev/null
```

这里关掉了软件包自带的服务。`strelaysrv.service` 是软件包里的别名，所以下面用另一个服务名。

## 2. 用户和 token

```sh
sudo useradd --system --home /var/lib/strelaysrv --create-home strelaysrv
openssl rand -hex 24 | sudo tee /etc/strelaysrv.token
sudo chown strelaysrv /etc/strelaysrv.token
sudo chmod 600 /etc/strelaysrv.token
```

## 3. 启动脚本

保存为 `/usr/local/bin/strelaysrv-start`，然后运行 `sudo chmod 755 /usr/local/bin/strelaysrv-start`：

```sh
#!/bin/sh
exec /usr/bin/strelaysrv -keys=/var/lib/strelaysrv -listen=:22067 -pools="" -status-srv="" -token="$(cat /etc/strelaysrv.token)"
```

## 4. 服务

保存为 `/etc/systemd/system/tunepeerly-relay.service`：

```ini
[Unit]
Description=TunePeerly private relay
After=network-online.target

[Service]
User=strelaysrv
ExecStart=/usr/local/bin/strelaysrv-start
Restart=always

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now tunepeerly-relay
```

## 5. 开放端口

在云防火墙里放行入站 TCP 22067。Azure 上，在虚拟机 > 网络里添加一条 TCP 22067 的入站端口规则，或者运行：

```sh
az vm open-port --resource-group RG --name VM --port 22067 --priority 1010
```

如果 `sudo ufw status` 显示 ufw 已启用，还要运行 `sudo ufw allow 22067/tcp`。

## 6. 中继标识和检查

```sh
sudo journalctl -u tunepeerly-relay --no-pager | grep -i "id="
```

`id=` 后面的值（到下一个 `&` 为止）就是中继标识。还需要服务器的公网 IP。在 Windows 上检查端口能不能连上：

```powershell
Test-NetConnection <ip> -Port 22067
```

应该显示 `TcpTestSucceeded : True`。

## 7. 在声渡里设置

1. 先在同一局域网里配对设备。
2. 在每台设备上，把对方设为**完全信任**。设为**每次确认**的设备不会经中继主动去连。
3. 在每台设备上打开**设备** > **中继服务器**，填写**中继地址**（`<ip>`）、**中继端口**（`22067`）、**中继设备标识**（`<relay-id>`）和**中继 token**（`/etc/strelaysrv.token` 的内容）。状态应变成**中继已连接**。

## 注意

- token 是密钥，不要分享或公开。
- strelaysrv 只能从命令行参数接收 token，服务器上的其他账户用 `ps` 就能看到。请用一台专门跑中继的服务器。
- `-pools=""` 让中继保持私有，不会公布到公共中继池。`-status-srv=""` 关掉状态页。
- 无痕模式会关掉中继连接。中继连接断开后，会在 5 秒后重试，之后等待的时间逐渐变长，最长 2 分钟。在 Android 上，网络变化时，例如从 Wi-Fi 切到移动数据，会立刻重新向中继注册。
