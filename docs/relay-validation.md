# Private relay deployment and validation

Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

This document records a Windows two-device relay exercise against upstream
`dev` commit `6e062b06d8400e45f8c026b88ed7960a4ddd96db`.
It does not change application code or certify the relay implementation as complete.

## Deployment

Use an Ubuntu 24.04 x64 host with a reachable public IPv4 address. The exercise
used Ubuntu package `syncthing-relaysrv` version `1.27.2~ds4-1ubuntu0.24.04.3`.

```sh
sudo apt-get update
sudo apt-get install syncthing-relaysrv
sudo systemctl stop syncthing-relaysrv
```

Run the relay as the package's unprivileged `syncthing` user. Persist the
certificate and key under `/var/lib/syncthing/relaysrv`; replacing them changes
its certificate ID. Configure these strelaysrv options using a systemd override:

```text
-nat=false
-protocol=tcp4
-listen=:22067
-ext-address=<PUBLIC_IPV4>:22067
-keys=/var/lib/syncthing/relaysrv
-pools=
-status-srv=127.0.0.1:22070
-token=<PRIVATE_RANDOM_TOKEN>
```

Generate a random token on the server and keep it in a root-readable environment
file, not in source control. Do not publish the actual service command line or
startup logs without redacting the token. Enable and start the service after
configuration. Allow inbound TCP 22067 in the cloud network security group;
restrict SSH to the administrator's public IP. Do not expose the status port.
No public relay-pool registration is needed for this private test.

A reusable version of this configuration is in
[`scripts/setup-private-relay.sh`](../scripts/setup-private-relay.sh). On a
dedicated Ubuntu host, review it and run `sudo bash setup-private-relay.sh <PUBLIC_IPV4>`.
It installs the package, replaces the named systemd override and restarts the
service, preserving any existing nonempty token file and certificate directory.
It does not configure cloud network rules or change the OS firewall. The script
was derived from the manually executed deployment commands; the new wrapper
itself has not been run on another fresh server.

## Client setup

Use the same upstream commit on both Windows computers. A build with
`npm run tauri build -- --debug --no-bundle` embeds the frontend and does not
need Vite, Node.js or Rust on the receiving computer. WebView2 is required.
Keep LICENSE, NOTICE and ADDITIONAL_TERMS.md with the executable and identify
the exact source commit. Each computer must retain its own device identity;
do not transfer the application database or private keys between computers.

In Settings > Network, enter the relay host, port, certificate ID and token on
both clients. The certificate ID is the SHA-256 hash of the DER certificate,
encoded as unpadded Base32 (accepted by this commit), or the checked Syncthing
ID format. Verify the ID from the server's certificate, not solely from an
untrusted network connection. The token controls relay admission; it is not an
end-to-end decryption key.

Pair the two clients on the same LAN and compare safety numbers. Then place the
clients on different networks and reconnect. Relay registration alone does not
prove that the paired-device session or music stream uses the relay.

## Evidence available for this exercise

| Check | Evidence / result |
| --- | --- |
| Frontend automated tests | 47 passed, 0 failed on the first Windows computer. |
| Embedded frontend build | TypeScript, Vite and Tauri debug no-bundle build completed. |
| Source changes | No application source changes relative to the commit above. |
| Second Windows environment | Operator reports SHA-256 matched the first build; preinstalled WebView2 156.0.4314.50; no Rust/Build Tools installation or compilation. |
| Independent executable | Process started with a TunePeerly window title and no Vite listener on port 1420; no visual verification was recorded. |
| Cloud service | `active`, `enabled`; TCP 22067 listening and status bound to loopback. |
| Public TCP reachability | Connection to port 22067 succeeded; this is not a TLS or relay-session test. |
| Initial failure | First client displayed `Relay unavailable: tls handshake eof`; cause was not established. |
| Pairing | A paired peer with full trust was present in the first client's local database. |
| Same-LAN direct session | User clarified that the same-LAN connection was direct, not relayed. |
| Different-network relay session | User reports Relay with computer 1 on a phone hotspot and computer 2 on the campus network. |
| Audible remote playback and controls | User reports audible playback and linked music controls between the two computers, including play/pause and track changes. The user observed starting and stopping together. |
| Remote catalogue | Second-device operator reported exposing three generated test tones; exact first-device browse contents were not separately confirmed. |
| Seeking | Not confirmed separately. |
| Disconnect and reconnect | Not confirmed separately. |
| Rust tests for this commit | Not rerun for this submission. |

The same-LAN direct and different-network relay outcomes above are user-reported manual
results. No packet capture, timing measurements or independent visual recording
was retained. Starting/stopping together is an observed functional result,
not a claim of measured sample-accurate synchronization. The initial TLS EOF was followed by successful reported sessions after disabling
the VPN on the first computer. In this environment, connections failed with the
VPN enabled and worked with it disabled; the specific routing/proxy cause was
not isolated, so this is not proof of a unique root cause or a code fix. The
second-device operator reported relay TCP connections held for over 18 minutes
and an independent device identity, with no database or private-key copying.
Different local subnets alone do not prove relay use; the application transport
label and connection observations are the supporting evidence.

## Additional evidence for follow-up

Record each device's OS and source commit, whether the networks differed,
relay registration on both clients, peer transport, browsing, audible playback,
pause/resume, seeking and disconnect/reconnect. Include actual results and any
error text. Avoid tokens, private keys, public endpoint addresses and personal
music paths in public evidence.

When the exercise is over, save the redacted results before removing temporary
build artifacts. Stop/deallocate or remove the dedicated cloud resources when
no longer needed; deleting a local build does not stop cloud charges.

## Optional seeking fixture

For the still-unperformed seeking case, generate a synthetic 60-second WAV:

```sh
python scripts/generate-relay-test-tone.py /path/to/new-test-tone.wav
```

The destination parent must exist. The script refuses to overwrite an existing
file. Import the resulting file on one device, expose it to the paired peer,
and seek from the other device. Each 15-second segment rises one semitone from
440 Hz, making position changes easier to hear. Use a low playback volume.
The generator was locally checked for WAV format, duration and overwrite
refusal; it was added after the reported two-device exercise. This fixture is
not evidence that seeking has already passed.
