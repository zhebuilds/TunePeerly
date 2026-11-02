# Deploy a relay

These steps set up a private strelaysrv relay on an Ubuntu or Debian server, tested on an Azure Ubuntu VM. Connect to the server over SSH and run the blocks below in order, in the same SSH session. You do not edit any file. Every block can be run again safely.

The relay only carries data between devices that are already paired. Pairing happens only on the local network. [SECURITY.md](../SECURITY.md) describes what the relay can see.

## 1. Server address

This finds the server's public IPv4 address.

```sh
RELAY_HOST=$(curl -4 -fsS https://api.ipify.org)
echo "$RELAY_HOST"
```

The command prints the address. If it prints nothing, set `RELAY_HOST=` to the server's public IP yourself.

## 2. Install

```sh
sudo apt update
sudo apt install -y syncthing-relaysrv
sudo systemctl disable --now strelaysrv 2>/dev/null
sudo systemctl disable --now syncthing-relaysrv 2>/dev/null
```

The package's own unit is turned off, because it would join the public relay pool. `strelaysrv.service` is an alias name in the package, so the steps below use a unit with a different name.

## 3. User and token

```sh
id strelaysrv >/dev/null 2>&1 || sudo useradd --system --home /var/lib/strelaysrv --create-home strelaysrv
sudo test -s /etc/strelaysrv.token || openssl rand -hex 24 | sudo tee /etc/strelaysrv.token
sudo chown strelaysrv /etc/strelaysrv.token
sudo chmod 600 /etc/strelaysrv.token
```

The token is generated once and kept in `/etc/strelaysrv.token`. Running the block again keeps the same token. Step 7 prints it.

## 4. Start script and service

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

`systemctl status` should show `active (running)`. The log should have a line `URI: relay://...?id=...`. The value after `id=` (up to the next `&`) is the relay's certificate ID, which the app calls the relay device ID. The first start generates the relay's key in `/var/lib/strelaysrv`, which can take a few seconds; if the line is not there yet, run the `journalctl` line again. Step 7 prints the ID on its own.

## 5. Open the port on the server

```sh
if command -v ufw >/dev/null && sudo ufw status | grep -q "Status: active"; then sudo ufw allow 22067/tcp; fi
```

This opens TCP 22067 only if `sudo ufw status` shows `Status: active`. If ufw is inactive, as it usually is on Azure Ubuntu, nothing needs to be done here.

## 6. Open the port in the cloud firewall

The cloud provider also has to allow inbound TCP 22067. On Azure, replace the first two lines with your resource group and VM names, then run the block in Azure Cloud Shell, or on a computer where the Azure CLI is signed in:

```sh
RG=myResourceGroup
VM=myVM
az vm open-port --resource-group "$RG" --name "$VM" --port 22067 --priority 1010
```

If you do not know the two names, run `curl -fsS -H Metadata:true "http://169.254.169.254/metadata/instance/compute?api-version=2021-02-01" | grep -o '"\(resourceGroupName\|name\)":"[^"]*"'` on the VM to print them. If priority 1010 is already taken, change it to another free number such as 1020. Instead of the CLI, you can go to VM > Networking > Add inbound port rule in the Azure portal and allow TCP 22067. On another cloud, allow inbound TCP 22067 in its firewall or security group.

## 7. Values for TunePeerly

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

This prints the four values to enter in the app. The relay device ID looks like `ABCDEFG-HIJKLMN-...`, eight groups of seven characters. If it is empty, wait a few seconds and run the block again. If `Relay address` is empty because the SSH session was reconnected, run step 1 first.

## 8. Check from your computer

On a Windows computer outside the server's network, open PowerShell and run the `Test-NetConnection ...` line that step 7 printed. It should show `TcpTestSucceeded : True`. If it shows `False`, step 6 is not done yet.

## 9. Set up TunePeerly

1. Pair the devices on the same local network first.
2. On each device, set the other device to **Full trust**. A device set to **Ask each time** does not dial over the relay.
3. On each device, open the **Devices** page and, in **Relay server** below **Nearby**, fill in **Relay address**, **Relay port**, **Relay device ID**, and **Relay token** with the values from step 7. The status should change to **Relay connected**.

## Update

```sh
sudo apt update
sudo apt install --only-upgrade -y syncthing-relaysrv
sudo systemctl disable --now strelaysrv 2>/dev/null
sudo systemctl disable --now syncthing-relaysrv 2>/dev/null
sudo systemctl restart tunepeerly-relay
systemctl is-active tunepeerly-relay
```

The relay device ID and the token stay the same, so nothing changes in the app.

## Change the token

```sh
openssl rand -hex 24 | sudo tee /etc/strelaysrv.token >/dev/null
sudo systemctl restart tunepeerly-relay
echo "Relay token: $(sudo cat /etc/strelaysrv.token)"
```

Enter the new token in **Relay token** on every device.

## Remove

```sh
sudo systemctl disable --now tunepeerly-relay
sudo rm -f /etc/systemd/system/tunepeerly-relay.service /usr/local/bin/strelaysrv-start /etc/strelaysrv.token
sudo systemctl daemon-reload
sudo userdel -r strelaysrv
sudo apt remove -y syncthing-relaysrv
if command -v ufw >/dev/null && sudo ufw status | grep -q "Status: active"; then sudo ufw delete allow 22067/tcp; fi
```

`userdel -r` also deletes `/var/lib/strelaysrv`, which holds the relay's key, so a new relay gets a new relay device ID. Also remove the inbound rule for port 22067 in the cloud firewall.

## Notes

- The token is a secret. Do not share or publish it.
- strelaysrv accepts the token only as a command-line flag, so other accounts on the server can see it with `ps`. Use a server dedicated to the relay.
- `-pools=""` keeps the relay private: it is not announced to the public relay pool. `-status-srv=""` turns off the status page.
- Incognito turns the relay connection off. A lost relay connection is tried again after 5 seconds, then after longer waits up to 2 minutes. On Android, a network change, such as switching from Wi-Fi to mobile data, registers with the relay again at once.
