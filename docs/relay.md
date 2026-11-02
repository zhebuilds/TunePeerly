# Deploy a relay

These steps set up a private strelaysrv relay on an Ubuntu server, tested on an Azure Ubuntu VM. Replace `<ip>`, `<relay-id>`, `RG`, and `VM` with your own values.

The relay only carries data between devices that are already paired. Pairing happens only on the local network. [SECURITY.md](../SECURITY.md) describes what the relay can see.

## 1. Install

```sh
sudo apt update
sudo apt install -y syncthing-relaysrv
sudo systemctl disable --now strelaysrv 2>/dev/null
sudo systemctl disable --now syncthing-relaysrv 2>/dev/null
```

The package's own unit is turned off. `strelaysrv.service` is an alias name in the package, so the steps below use a unit with a different name.

## 2. User and token

```sh
sudo useradd --system --home /var/lib/strelaysrv --create-home strelaysrv
openssl rand -hex 24 | sudo tee /etc/strelaysrv.token
sudo chown strelaysrv /etc/strelaysrv.token
sudo chmod 600 /etc/strelaysrv.token
```

## 3. Start script

Save as `/usr/local/bin/strelaysrv-start`, then run `sudo chmod 755 /usr/local/bin/strelaysrv-start`:

```sh
#!/bin/sh
exec /usr/bin/strelaysrv -keys=/var/lib/strelaysrv -listen=:22067 -pools="" -status-srv="" -token="$(cat /etc/strelaysrv.token)"
```

## 4. Service

Save as `/etc/systemd/system/tunepeerly-relay.service`:

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

## 5. Open the port

Allow inbound TCP 22067 in the cloud firewall. On Azure, go to VM > Networking and add an inbound port rule for TCP 22067, or run:

```sh
az vm open-port --resource-group RG --name VM --port 22067 --priority 1010
```

If `sudo ufw status` shows ufw is active, also run `sudo ufw allow 22067/tcp`.

## 6. Relay ID and check

```sh
sudo journalctl -u tunepeerly-relay --no-pager | grep -i "id="
```

The relay ID is the value after `id=` (up to the next `&`). You also need the server's public IP. From Windows, check that the port is reachable:

```powershell
Test-NetConnection <ip> -Port 22067
```

It should show `TcpTestSucceeded : True`.

## 7. Set up TunePeerly

1. Pair the devices on the same local network first.
2. On each device, set the other device to **Full trust**. A device set to **Ask each time** does not dial over the relay.
3. On each device, open **Devices** > **Relay server** and fill in **Relay address** (`<ip>`), **Relay port** (`22067`), **Relay device ID** (`<relay-id>`), and **Relay token** (the contents of `/etc/strelaysrv.token`). The status should change to **Relay connected**.

## Notes

- The token is a secret. Do not share or publish it.
- strelaysrv accepts the token only as a command-line flag, so other accounts on the server can see it with `ps`. Use a server dedicated to the relay.
- `-pools=""` keeps the relay private: it is not announced to the public relay pool. `-status-srv=""` turns off the status page.
- Incognito turns the relay connection off. A lost relay connection is tried again after 5 seconds, then after longer waits up to 2 minutes. On Android, a network change, such as switching from Wi-Fi to mobile data, registers with the relay again at once.
