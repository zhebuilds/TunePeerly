English | [简体中文](guide.zh-CN.md)

# TunePeerly user guide

TunePeerly (声渡) plays audio files on this computer and shares them with computers you pair. There is no account. This guide matches version 0.8.0-alpha.1. Version 0.8.0-alpha.1 is an alpha test release. The program is not guaranteed to match this guide.

## Install

The GitHub release is an Inno Setup installer. It asks whether to install for all users or only for the current user. The current user is selected by default and uses `C:\Users\<name>\AppData\Local\Programs\TunePeerly`. That choice does not need an administrator account. All users uses `C:\Program Files\TunePeerly` and does. The database and the cache stay in their own fixed folders, below, and are not changed by that choice.

The installer does not add a firewall rule. The first time the app listens for other devices, Windows Defender Firewall may ask whether to allow `TunePeerly.exe`. Allow it on the networks you use. A later copy of the program at a different path is treated as a different program and is asked about again. Uninstalling asks whether to remove the rules Windows created for that copy. Agreeing asks for an administrator account when the uninstall itself is not running as one.

## Add music

Open **Music and download folders** in Settings, or drop folders and audio files onto the window. The app reads the files where they are. It does not copy them and it does not transcode them.

**Scan** on the All music page, and **Rescan** in Settings, read every added folder again. New songs are added. A changed song, or a `.lrc` file you added or edited next to it, is read again. Nothing else changes.

Removing a folder only removes it from TunePeerly. The files stay on disk. Its songs stay in the library as unavailable, with their tags, covers, favorites, playlists and recently played. Add the folder again and they play again.

## Play

The bar at the bottom controls this computer's own queue: play, pause, skip, seek, shuffle, and repeat. Sound uses the system's default output device.

Right-click a song in a playlist for **Play next**, **Add to queue**, **Add to playlist**, **Save to Liked Songs**, and others.

The queue is only on this computer. It is not synced.

When a song has LRC lyrics, the lyrics button scrolls with the song.

## Window, tray, and mini player

The window has its own title bar. **When closing** in Settings → Appearance chooses what the close button does: **Ask me** shows a prompt every time, **Minimize to tray** keeps the player running in the background, and **Quit** closes it.

**Mini player when minimized** and **Mini player in the tray** leave a small bar on screen with the cover, playback controls, and the heart, when the window is minimized or hidden in the tray. Turn both off for no bar. Drag the cover, the title, or the empty part of the bar to move it. Click the cover or the title to open the main window. The cross in its corner closes only the bar; it comes back the next time the window is minimized or hidden.

The Windows taskbar thumbnail shows the whole cover, scaled down to fit, with previous, play, next, and a heart.

## Playlists

A playlist is a list of songs. Deleting a playlist does not delete the songs.

## Pair a device

Open **Devices**. Before pairing, **Discover nearby devices**, the switch next to **Nearby**, must be on, or devices cannot be found.

- **Discover nearby devices** announces this computer on the local network about every 3 seconds by UDP multicast, and about every 15 seconds by subnet broadcast. The announcement carries the device ID, the name, the TCP port, and the LAN IPv4 addresses. It never carries song names. The same switch also listens for other devices, so it has to be switched on at both ends.
- To pair, press **Pair** on one computer. The other computer shows a 6-digit code; type it on the first one. A code works once, for 90 seconds, and a wrong code is replaced by a new one. After 3 wrong codes, pairing pauses for 1 minute for every device. Until pairing finishes, the connection is pending. While no other pairing is going on, any device on the network that asks to pair makes this computer show a code, and nothing is paired until that code is typed on the device that asked.
- After you decline a request, that device cannot ask again until **Silence after a decline** has passed. That setting is under History and privacy. The default is 60 seconds, and the allowed range is 1 to 86400. You can still connect to that device yourself.
- Paired devices reconnect on their own. A device that drops is called again at once, at the address it was last reached at and where Nearby finds it, and through the relay if one is set up. If that fails, the waits grow from 1 second up to 1 minute. A device set to **Ask each time** is not called; it connects when the other device calls. **Disconnect** stops the calls, from both sides, until you press **Connect** on the device where you pressed **Disconnect**, or that app is restarted. The other device cannot reconnect before then.
- **Block** refuses every connection from a device and hides it from the paired list and from Nearby. It is listed under **Blocked devices**, where **Unblock** brings it back.
- **Remove** unpairs both sides. This computer deletes the device's saved key and tells the device, which removes this computer and shows a notice. If the device is offline, it is told the next time the two connect, and until then it only gets that notice from this computer. Your other devices are not changed. To use the two together again, press **Pair** on either one and type the new code.

The four relay fields are on the **Devices** page, in **Relay server** below **Nearby**: address, port, the relay's certificate ID, and a token, with the relay status shown under the heading. Both devices must set each other to **Full trust**. To set up your own relay, see [Deploy a relay](relay.md).

- Pairing happens only on the local network. The relay only carries data between devices that are already paired, and never pairs. Pair the devices on the same network first.
- When using a relay, set both devices to **Full trust** for each other. A device set to **Ask each time** does not dial over the relay; it only waits for the other device to connect and then asks you. If both sides use **Ask each time**, they never connect over the relay.
- The relay cannot read your data (TLS 1.3 end to end), but it sees each device's relay ID, public IP, when it is online, and who connects to whom and how much data. Anyone who knows the relay address and a device's relay ID can learn whether that device is online; the app refuses every device that is not paired. See [SECURITY.md](../SECURITY.md).

Settings → **This computer** is where you rename this computer; other devices see that name. Below it are read-only details: the **TCP listening port**, the discovery UDP port and multicast group, the LAN IPv4 addresses this computer announces, and last the **Device ID**. The discovery rows show Off while discovery is off. The TCP port cannot be set: each time the app starts or the network is turned back on, the system picks a free port, and discovery announces it. Changing other settings keeps the current port.

## Play a song that is on another computer

Every readable local song is offered to paired devices. Once a device is paired and connected, it can see, play, and download all of them, including songs added later. Only incognito stops this. Video files, songs that cannot be read now (the file is gone or its folder was removed), and new songs the app has not finished identifying in the background are not offered.

Any song on an online paired device can be played from that device. **Download** saves the original file into the downloads folder (set in Settings → Music and download folders) and adds it to this library. The downloaded file is checked against the song's content ID, and a file that does not match is discarded. One song downloads at a time; the others wait in the list.

## Sync

The **Sync** section in Settings, right after Music and download folders. Nothing syncs on its own: a copy moves only when you press Send or Override.

Incognito cuts every device connection and the relay connection, stops discovery, turns sync off, refuses downloads, stops recording Recently played, and stops offering this computer's songs to other devices.

| Control | What it does |
| --- | --- |
| Send | Push this computer's copy to one connected device, or to all connected devices. |
| Override | Ask one online device for its copy and replace this computer's copy with it. You pick the device each time; none is preselected. |

A Send always overwrites; there is no merging and no conflict handling. Each computer chooses, per item, what a Send from another device may do: **Overwrite directly** replaces the copy at once, **Ask first** waits up to a minute for you to accept or decline, and **Don't allow** refuses it and changes nothing. The default is Overwrite directly. The sending computer shows each device's result, such as updated, not allowed, declined, timed out, or in incognito, and keeps it in the notifications. A device that does not answer within 15 seconds shows as no result. Override does not ask and is not affected by this choice, because this computer requested that copy.

Appearance sync includes the theme, the language, the two mini player switches, and the close-window choice.

## Recently played

Recently played is kept on each computer and is not synced. When a paired device is connected, the list at the top of **Recently played** can switch to that device. Its list is asked for once and shown read-only: it is not saved or merged here, and nothing is sent back. Songs that are also in this library, or that a connected device has, can be played from it. If that device disconnects, the page goes back to this computer's list. A device in incognito does not share its list.

## Notifications

Notifications record what happens between your devices, such as pairing, connecting and disconnecting, removing or blocking a device, and the results of Send and Override. The bell opens the notification list. You can mark items read when they scroll into view, when the panel opens, or only when you open one. Clearing can ask every time, delete everything except pinned items, or delete everything.

## Where your data stays

The database is in `%APPDATA%\TunePeerly`. It is encrypted, and its key file `db.key` beside it is sealed for your Windows account. Covers and the window cache are in `%LOCALAPPDATA%\TunePeerly`. These two folders cannot be changed. Uninstalling asks whether to delete them. If the key file cannot be opened, for example after it was copied from another Windows account, the old database and key are renamed with `.unreadable-` and the date, and a new, empty library starts. The device identity lives in the database, so the other devices have to be paired again.

On Android everything is in the app's private storage; see [Android](android.md). There is no account. Paired devices on the same local network sync and stream directly. Outside that network they can meet through the relay configured on the **Devices** page under **Relay server**. Both devices must set each other to **Full trust**.
