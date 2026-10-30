// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { useApp } from "../app/model";
import { albumId } from "../lib/library";
import { Icons } from "../ui/icons";

export function ContextMenu() {
  const { t } = useTranslation();
  const app = useApp();
  const menu = app.menu;
  const box = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: 0, top: 0, flip: false });
  const [sub, setSub] = useState(false);

  useLayoutEffect(() => {
    if (!menu || !box.current) return;
    const rect = box.current.getBoundingClientRect();
    const left = Math.min(menu.x, window.innerWidth - rect.width - 8);
    const top = Math.min(menu.y, window.innerHeight - rect.height - 8);
    setPosition({ left: Math.max(8, left), top: Math.max(8, top), flip: left + rect.width + 220 > window.innerWidth });
    setSub(false);
  }, [menu]);

  if (!menu) return null;
  const tracks = menu.tracks;
  const one = tracks.length === 1 ? tracks[0] : null;
  const close = () => app.setMenu(null);
  const run = (action: () => void) => () => {
    close();
    action();
  };

  if (menu.playlistMenu) {
    const id = menu.playlistMenu;
    return (
      <Layer onClose={close}>
        <div ref={box} className="menu" role="menu" style={{ left: position.left, top: position.top }}>
          <Item icon={<Icons.play />} label={t("list.play")} disabled={tracks.length === 0} onClick={run(() => app.playAll(tracks, `playlist:${id}`))} />
          <Item icon={<Icons.shuffle />} label={t("list.shuffle")} disabled={tracks.length === 0} onClick={run(() => app.playAll(tracks, `playlist:${id}`, true))} />
          <Item icon={<Icons.queue />} label={t("menu.addToQueue")} disabled={tracks.length === 0} onClick={run(() => app.queueActions.add(tracks))} />
          <hr />
          <Item icon={<Icons.pencil />} label={t("playlist.rename")} onClick={run(() => app.renamePlaylist(id))} />
          <Item icon={<Icons.trash />} label={t("playlist.delete")} danger onClick={run(() => app.deletePlaylist(id))} />
        </div>
      </Layer>
    );
  }

  const allLocal = tracks.every((track) => track.localFileId && track.accessStatus === "available");
  const allRemote = tracks.every((track) => !(track.localFileId && track.accessStatus === "available") && track.contentId);
  const playlists = app.boot?.playlists ?? [];
  const allLiked = tracks.every((track) => track.favorite);

  return (
    <Layer onClose={close}>
      <div ref={box} className="menu" role="menu" style={{ left: position.left, top: position.top }}>
        {one && (
          <Item
            icon={<Icons.play />}
            label={t("menu.play")}
            disabled={!one.playable}
            onClick={run(() => {
              if (menu.queueIndex !== undefined) app.queueActions.jump(menu.queueIndex, one);
              else if (menu.play) menu.play();
              else app.playList([one], 0, app.queue.contextId);
            })}
          />
        )}
        <Item icon={<Icons.next />} label={t("menu.playNext")} onClick={run(() => app.queueActions.playNext(tracks))} />
        <Item icon={<Icons.queue />} label={t("menu.addToQueue")} onClick={run(() => app.queueActions.add(tracks))} />
        {menu.queueIndex !== undefined && <Item icon={<Icons.close />} label={t("queue.remove")} onClick={run(() => app.queueActions.remove(menu.queueIndex!, one))} />}
        <hr />
        <div className="menu-sub" onMouseEnter={() => setSub(true)} onMouseLeave={() => setSub(false)}>
          <button type="button" role="menuitem" className="menu-item" aria-haspopup="menu" aria-expanded={sub} onClick={() => setSub((value) => !value)}>
            <Icons.plus />
            <span>{t("menu.addToPlaylist")}</span>
            <Icons.chevronRight className="menu-arrow" />
          </button>
          {sub && (
            <div className={`menu submenu${position.flip ? " flip" : ""}`} role="menu">
              <Item icon={<Icons.plus />} label={t("menu.newPlaylist")} onClick={run(() => app.createPlaylist(tracks))} />
              {playlists.length > 0 && <hr />}
              <div className="menu-scroll">
                {playlists.map((playlist) => (
                  <Item key={playlist.id} icon={<Icons.list />} label={playlist.name} disabled={playlist.id === menu.playlistId} onClick={run(() => void app.addToPlaylist(playlist.id, tracks))} />
                ))}
              </div>
            </div>
          )}
        </div>
        {menu.playlistId && <Item icon={<Icons.trash />} label={t("menu.removeFromPlaylist")} onClick={run(() => void app.removeFromPlaylist(menu.playlistId!, tracks))} />}
        <Item
          icon={allLiked ? <Icons.heartFill /> : <Icons.heart />}
          label={allLiked ? t("menu.unlike") : t("menu.like")}
          onClick={run(() => {
            for (const track of tracks) if (track.favorite === allLiked) void app.like(track);
          })}
        />
        {one && (
          <>
            <hr />
            {one.artist && <Item icon={<Icons.artist />} label={t("menu.goArtist")} onClick={run(() => app.navigate({ name: "artist", id: one.artist }))} />}
            {one.album && <Item icon={<Icons.album />} label={t("menu.goAlbum")} onClick={run(() => app.navigate({ name: "album", id: albumId(one.artist, one.album) }))} />}
            {one.localFileId && one.accessStatus === "available" && (
              <Item icon={<Icons.folder />} label={t("menu.showInFolder")} onClick={run(() => void api.showInFolder(one.localFileId!).catch(app.say))} />
            )}
          </>
        )}
        {allRemote && (
          <Item
            icon={<Icons.download />}
            label={t("menu.download")}
            disabled={tracks.some((track) => track.holderIds.length === 0)}
            onClick={run(() => {
              for (const track of tracks) if (track.contentId) void api.network("download", track.contentId).catch(app.say);
            })}
          />
        )}
        {allLocal && (
          <>
            <hr />
            {tracks.every((track) => track.exposed) ? (
              <Item icon={<Icons.eyeOff />} label={t("menu.stopExposing")} onClick={run(() => void exposeTracks(tracks.map((track) => track.localFileId!), false).then(() => { app.toast(t("toast.hidden")); app.scheduleRefresh(); }).catch(app.say))} />
            ) : (
              <Item icon={<Icons.eye />} label={t("menu.expose")} onClick={run(() => void exposeTracks(tracks.map((track) => track.localFileId!), true).then(() => { app.toast(t("toast.exposed")); app.scheduleRefresh(); }).catch(app.say))} />
            )}
          </>
        )}
      </div>
    </Layer>
  );
}

async function exposeTracks(ids: string[], expose: boolean) {
  for (const id of ids) await api.setTrackExposure(id, expose);
}

function Layer({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  return (
    <div
      className="menu-layer"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        if (event.target === event.currentTarget) onClose();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") onClose();
      }}
    >
      {children}
    </div>
  );
}

function Item({ icon, label, onClick, disabled, danger }: { icon: ReactNode; label: string; onClick: () => void; disabled?: boolean; danger?: boolean }) {
  return (
    <button type="button" role="menuitem" className={`menu-item${danger ? " danger" : ""}`} disabled={disabled} onClick={onClick}>
      {icon}
      <span>{label}</span>
    </button>
  );
}
