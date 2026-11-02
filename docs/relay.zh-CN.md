# 部署中继

下面的步骤在 Ubuntu 或 Debian 服务器上搭一个私有的 strelaysrv 中继，已在 Azure 的 Ubuntu 虚拟机上验证。用 SSH 连上服务器，在同一个 SSH 会话里按顺序运行下面的命令块。不用编辑任何文件。每个命令块重复运行也没有问题。

中继只在已配对的设备之间转发数据，配对只在局域网里进行。中继能看到什么，见 [SECURITY.md](../SECURITY.md)。

## 1. 服务器地址

这条命令会查出服务器的公网 IPv4 地址。

```sh
RELAY_HOST=$(curl -4 -fsS https://api.ipify.org)
echo "$RELAY_HOST"
```

命令会打印出地址。如果什么都没打印，就自己写 `RELAY_HOST=服务器公网 IP`。

## 2. 安装

```sh
sudo apt update
sudo apt install -y syncthing-relaysrv
sudo systemctl disable --now strelaysrv 2>/dev/null
sudo systemctl disable --now syncthing-relaysrv 2>/dev/null
```

这里关掉了软件包自带的服务，因为它会加入公共中继池。`strelaysrv.service` 是软件包里的别名，所以下面用另一个服务名。

## 3. 用户和 token

```sh
id strelaysrv >/dev/null 2>&1 || sudo useradd --system --home /var/lib/strelaysrv --create-home strelaysrv
sudo test -s /etc/strelaysrv.token || openssl rand -hex 24 | sudo tee /etc/strelaysrv.token
sudo chown strelaysrv /etc/strelaysrv.token
sudo chmod 600 /etc/strelaysrv.token
```

token 只生成一次，保存在 `/etc/strelaysrv.token`。重复运行这一块不会换 token。第 7 步会把它打印出来。

## 4. 启动脚本和服务

```sh
sudo tee /usr/local/bin/strelaysrv-start >/dev/null <<'EOF'
#!/bin/sh
exec /usr/bin/strelaysrv -keys=/var/lib/strelaysrv -listen=:22067 -pools="" -status-srv="" -token="$(cat /etc/strelaysrv.token)"
EOF
sudo chmod 755 /usr/local/bin/strelaysrv-start

sudo tee /etc/systemd/system/tunepeerly-relay.service >/dev/null <<'EOF'
[Unit]
Description=TunePeerly private relay
After=network-online.target

[Service]
User=strelaysrv
ExecStart=/usr/local/bin/strelaysrv-start
Restart=always

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now tunepeerly-relay
sleep 3
sudo systemctl status tunepeerly-relay --no-pager
sudo journalctl -u tunepeerly-relay -n 20 --no-pager
```

`systemctl status` 应该显示 `active (running)`。日志里应该有一行 `URI: relay://...?id=...`，`id=` 后面的值（到下一个 `&` 为止）就是中继的证书标识，也就是应用里的中继设备标识。第一次启动时会在 `/var/lib/strelaysrv` 里生成中继的密钥，可能要几秒钟；没看到这一行就再运行一次 `journalctl` 那一行。第 7 步会把它单独打印出来。

## 5. 在服务器上开放端口

```sh
if command -v ufw >/dev/null && sudo ufw status | grep -q "Status: active"; then sudo ufw allow 22067/tcp; fi
```

只有 `sudo ufw status` 显示 `Status: active` 时，这一步才会放行 TCP 22067。ufw 没启用（Azure 的 Ubuntu 通常没启用）就什么都不用做。

## 6. 在云防火墙里开放端口

云服务商那边也要放行入站 TCP 22067。这一步在云服务商的网页控制台里手动添加规则。

在 Azure 上：

1. 打开 [Azure 门户](https://portal.azure.com)，进入你的虚拟机。
2. 在左侧菜单打开**网络**（有的版本叫**网络设置**）。
3. 点**创建端口规则** > **入站端口规则**（有的版本叫**添加入站端口规则**）。
4. 这样填：源 **Any**，源端口范围 **\***，目标 **Any**，服务 **Custom**，目标端口范围 **22067**，协议 **TCP**，操作**允许**，名称 **tunepeerly-relay**。优先级保持默认。
5. 点**添加**，等一分钟左右规则生效。

其他云服务商，在它的安全组或防火墙里添加一条入站规则，允许任意来源访问 TCP 22067 端口。

## 7. 声渡里要填的值

```sh
RELAY_ID=""
for i in $(seq 30); do
  RELAY_ID=$(sudo journalctl -u tunepeerly-relay --no-pager | grep -o 'id=[A-Z0-9-]*' | tail -n 1 | cut -d= -f2)
  [ -n "$RELAY_ID" ] && break
  sleep 1
done
echo "Relay address:   $RELAY_HOST"
echo "Relay port:      22067"
echo "Relay device ID: $RELAY_ID"
echo "Relay token:     $(sudo cat /etc/strelaysrv.token)"
echo "Check from Windows: Test-NetConnection $RELAY_HOST -Port 22067"
```

这一步会打印出要在应用里填的四个值：Relay address 是中继地址，Relay port 是中继端口，Relay device ID 是中继设备标识，Relay token 是中继 token。中继设备标识的样子是 `ABCDEFG-HIJKLMN-...`，共 8 组，每组 7 个字符。如果它是空的，等几秒再运行一次这一块。如果 SSH 重新连接过，Relay address 会是空的，先重新运行第 1 步。

## 8. 在自己的电脑上检查

在服务器所在网络以外的一台 Windows 电脑上，打开 PowerShell，运行第 7 步打印出来的 `Test-NetConnection ...` 那一行。应该显示 `TcpTestSucceeded : True`。如果显示 `False`，说明第 6 步还没做好。

## 9. 在声渡里设置

1. 先在同一局域网里配对设备。
2. 在每台设备上，把对方设为**完全信任**。设为**每次确认**的设备不会经中继主动去连。
3. 在每台设备上打开**设备**页面，在**附近**下面的**中继服务器**里，用第 7 步打印的值填写**中继地址**、**中继端口**、**中继设备标识**和**中继 token**。状态应变成**中继已连接**。

## 更新

```sh
sudo apt update
sudo apt install --only-upgrade -y syncthing-relaysrv
sudo systemctl disable --now strelaysrv 2>/dev/null
sudo systemctl disable --now syncthing-relaysrv 2>/dev/null
sudo systemctl restart tunepeerly-relay
systemctl is-active tunepeerly-relay
```

中继设备标识和 token 都不变，应用里不用改。

## 更换 token

```sh
openssl rand -hex 24 | sudo tee /etc/strelaysrv.token >/dev/null
sudo systemctl restart tunepeerly-relay
echo "Relay token: $(sudo cat /etc/strelaysrv.token)"
```

在每台设备的**中继 token** 里填上新的 token。

## 删除

```sh
sudo systemctl disable --now tunepeerly-relay
sudo rm -f /etc/systemd/system/tunepeerly-relay.service /usr/local/bin/strelaysrv-start /etc/strelaysrv.token
sudo systemctl daemon-reload
sudo userdel -r strelaysrv
sudo apt remove -y syncthing-relaysrv
if command -v ufw >/dev/null && sudo ufw status | grep -q "Status: active"; then sudo ufw delete allow 22067/tcp; fi
```

`userdel -r` 会一起删掉存放中继密钥的 `/var/lib/strelaysrv`，所以以后重新部署的中继会有新的中继设备标识。云防火墙里 22067 端口的入站规则也要删掉。

## 注意

- token 是密钥，不要分享或公开。
- strelaysrv 只能从命令行参数接收 token，服务器上的其他账户用 `ps` 就能看到。请用一台专门跑中继的服务器。
- `-pools=""` 让中继保持私有，不会公布到公共中继池。`-status-srv=""` 关掉状态页。
- 无痕模式会关掉中继连接。中继连接断开后，会在 5 秒后重试，之后等待的时间逐渐变长，最长 2 分钟。在 Android 上，网络变化时，例如从 Wi-Fi 切到移动数据，会立刻重新向中继注册。
