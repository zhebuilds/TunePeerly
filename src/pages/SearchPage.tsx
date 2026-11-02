// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Track } from "../api";
import { useApp } from "../app/model";
import { Card, Shelf } from "../components/Card";
import { Cover, LikedCover } from "../components/Cover";
import { TrackTable } from "../components/TrackTable";
import { tintFor } from "../lib/format";
import { covers, displayTitle, groupTracks, pickCovers } from "../lib/library";
import { Icons } from "../ui/icons";
import { EmptyHint } from "./TrackListPage";

export function SearchPage() {
  const { t } = useTranslation();
  const app = useApp();
  const [results, setResults] = useState<Track[] | null>(null);
  const [showAll, setShowAll] = useState(false);
  const ticket = useRef(0);
  const query = app.search.trim();

  useEffect(() => {
    setShowAll(false);
    const mine = ++ticket.current;
    if (!query) {
      setResults(null);
      return;
    }
    const timer = window.setTimeout(() => {
      void api
        .search(query)
        .then((items) => {
          if (mine === ticket.current) setResults(items);
        })
        .catch(() => {
          if (mine === ticket.current) setResults([]);
        });
    }, 160);
    return () => window.clearTimeout(timer);
  }, [query, app.tracks]);

  const lower = query.toLowerCase();
  const artists = useMemo(() => (results ? groupTracks(results, "artist").filter((group) => group.name && group.name.toLowerCase().includes(lower)).slice(0, 8) : []), [results, lower]);
  const albums = useMemo(() => (results ? groupTracks(results, "album").filter((group) => group.name && group.name.toLowerCase().includes(lower)).slice(0, 8) : []), [results, lower]);
  const playlists = useMemo(() => (app.boot?.playlists ?? []).filter((item) => lower && item.name.toLowerCase().includes(lower)), [app.boot, lower]);

  return (
    <div className="page search-page">
      {!query && <BrowseTiles />}

      {query && results && results.length === 0 && artists.length === 0 && playlists.length === 0 && (
        <EmptyHint icon={<Icons.search />} title={t("search.noResults", { text: query })} body={t("search.noResultsBody")} />
      )}

      {query && results && results.length > 0 && (
        <>
          <div className="search-top">
            <section className="top-result">
              <h2>{t("search.top")}</h2>
              <div className="top-card" role="button" tabIndex={0} onClick={() => app.playList(results, 0, "search")} onKeyDown={(event) => {
                if (event.key === "Enter" && !(event.target as HTMLElement).closest("button, input, a, select, textarea")) app.playList(results, 0, "search");
              }}>
                <Cover src={results[0].coverUrl} className="top-cover" />
                <div className="top-title">{displayTitle(results[0], t("library.untitled"))}</div>
                <div className="top-sub">
                  <span className="pill-label">{t("kind.song")}</span>
                  {results[0].artist || t("library.unknownArtist")}
                </div>
                <button type="button" className="play-fab small" aria-label={t("list.play")} onClick={(event) => {
                  event.stopPropagation();
                  app.playList(results, 0, "search");
                }}>
                  <Icons.play />
                </button>
              </div>
            </section>
            <section className="top-songs">
              <div className="shelf-head">
                <h2>{t("search.songs")}</h2>
                {results.length > 5 && (
                  <button type="button" className="link-btn" onClick={() => setShowAll((value) => !value)}>
                    {showAll ? t("common.showLess") : t("common.showAll")}
                  </button>
                )}
              </div>
              <TrackTable tracks={showAll ? results : results.slice(0, 5)} playTracks={results} contextId="search" columns={{ folder: false, added: false, source: true }} />
            </section>
          </div>
        </>
      )}

      {query && artists.length > 0 && (
        <Shelf title={t("search.artists")} rail>
          {artists.map((group) => (
            <Card key={group.name} cover={<Cover src={group.cover} round icon="artist" />} title={group.name} subtitle={t("kind.artist")} onOpen={() => app.navigate({ name: "artist", id: group.name })} onPlay={() => app.playAll(app.contextTracks(`artist:${group.name}`), `artist:${group.name}`)} />
          ))}
        </Shelf>
      )}
      {query && albums.length > 0 && (
        <Shelf title={t("search.albums")} rail>
          {albums.map((group) => (
            <Card key={group.id} cover={<Cover src={group.cover} />} title={group.name} subtitle={group.tracks[0]?.artist || t("kind.album")} onOpen={() => app.navigate({ name: "album", id: group.id })} onPlay={() => app.playAll(app.contextTracks(`album:${group.id}`), `album:${group.id}`)} />
          ))}
        </Shelf>
      )}
      {query && playlists.length > 0 && (
        <Shelf title={t("search.playlists")} rail>
          {playlists.map((playlist) => {
            const tracks = app.index.members(app.members[playlist.id] ?? []);
            return <Card key={playlist.id} cover={<Cover src={covers(tracks, 1, `playlist:${playlist.id}`)[0]} />} title={playlist.name} subtitle={t("list.songCount", { count: playlist.count })} onOpen={() => app.navigate({ name: "playlist", id: playlist.id })} onPlay={() => app.playAll(tracks, `playlist:${playlist.id}`)} />;
          })}
        </Shelf>
      )}
    </div>
  );
}

function BrowseTiles() {
  const { t } = useTranslation();
  const app = useApp();
  const albumArt = useMemo(() => pickCovers(groupTracks(app.library, "album").map((group) => group.cover), 2, "browse-albums"), [app.library]);
  const artistArt = useMemo(() => pickCovers(groupTracks(app.library, "artist").map((group) => group.cover), 2, "browse-artists"), [app.library]);
  const folderArt = useMemo(() => pickCovers(groupTracks(app.library, "folder").map((group) => group.cover), 2, "browse-folders"), [app.library]);
  const likedArt = useMemo(() => pickCovers(app.liked.map((track) => track.coverUrl), 2, "browse-liked"), [app.liked]);
  const recentArt = useMemo(() => pickCovers(app.recent.map((track) => track.coverUrl), 2, "browse-recent"), [app.recent]);
  const allArt = useMemo(() => pickCovers(app.library.map((track) => track.coverUrl), 2, "browse-all"), [app.library]);
  const deviceArt = useMemo(() => pickCovers(app.library.filter((track) => track.badge === "remote").map((track) => track.coverUrl), 2, "browse-devices"), [app.library]);
  const tiles = [
    { key: "all", label: t("nav.allMusic"), go: () => app.navigate({ name: "library" }), images: allArt, icon: <Icons.music /> },
    { key: "liked", label: t("nav.liked"), go: () => app.navigate({ name: "liked" }), images: likedArt, icon: <LikedCover /> },
    { key: "recent", label: t("nav.recent"), go: () => app.navigate({ name: "recent" }), images: recentArt, icon: <Icons.clock /> },
    { key: "artists", label: t("nav.artists"), go: () => app.navigate({ name: "artists" }), images: artistArt, icon: <Icons.artist /> },
    { key: "albums", label: t("nav.albums"), go: () => app.navigate({ name: "albums" }), images: albumArt, icon: <Icons.album /> },
    { key: "folders", label: t("nav.folders"), go: () => app.navigate({ name: "folders" }), images: folderArt, icon: <Icons.folder /> },
    { key: "devices", label: t("nav.devices"), go: () => app.navigate({ name: "devices" }), images: deviceArt, icon: <Icons.devices /> },
  ];
  return (
    <section className="shelf">
      <div className="shelf-head">
        <h2>{t("search.browse")}</h2>
      </div>
      <div className="browse-tiles">
        {tiles.map((tile) => (
          <button type="button" key={tile.key} className="browse-tile" style={{ background: tintFor(tile.key) }} onClick={tile.go}>
            <span>{tile.label}</span>
            <span className="browse-icon">
              {tile.images.length > 0 ? tile.images.map((src) => <Cover key={src} src={src} />) : tile.icon}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}
