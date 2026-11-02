// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useTranslation } from "react-i18next";
import { api, type Track } from "../api";
import { useApp } from "../app/model";
import { Cover } from "../components/Cover";
import { displayTitle, problemGroups, trackKey } from "../lib/library";
import { Icons } from "../ui/icons";
import { EmptyHint } from "./TrackListPage";
import { isAndroid } from "../lib/platform";

export function ProblemsPage() {
  const { t } = useTranslation();
  const app = useApp();
  const groups = problemGroups(app.tracks);
  const total = groups.access.length + groups.playback.length;
  return (
    <div className="page settings-page problems-page">
      <button type="button" className="link-btn back-link" onClick={() => app.navigate({ name: "settings" })}>
        <Icons.back /> {t("nav.settings")}
      </button>
      <h1 className="page-title">{t("problems.title")}</h1>
      <p className="setting-help">{t("problems.help")}</p>
      <div className="button-row">
        <button type="button" className="btn soft" onClick={() => void app.rescan()}>
          <Icons.sync /> {t("library.rescan")}
        </button>
      </div>
      {total === 0 ? (
        <EmptyHint icon={<Icons.check />} title={t("problems.noneTitle")} body={t("problems.noneBody")} />
      ) : (
        <>
          <ProblemGroup title={t("problems.access")} help={t("problems.accessHelp")} tracks={groups.access} />
          <ProblemGroup title={t("problems.playback")} help={t("problems.playbackHelp")} tracks={groups.playback} />
        </>
      )}
    </div>
  );
}

function ProblemGroup({ title, help, tracks }: { title: string; help: string; tracks: Track[] }) {
  const { t } = useTranslation();
  const app = useApp();
  if (tracks.length === 0) return null;
  return (
    <section className="panel">
      <h2>
        {title} <span className="count-badge">{tracks.length}</span>
      </h2>
      <p className="setting-help">{help}</p>
      <ul className="problem-list">
        {tracks.map((track, at) => (
          <li key={track.localFileId ?? `${trackKey(track)}:${at}`} className="problem-item">
            <Cover src={track.coverUrl} className="row-cover" />
            <div className="problem-text">
              <div className="problem-title">{displayTitle(track, t("library.untitled"))}</div>
              <div className="problem-path mono" title={track.absolutePath ?? ""}>
                {track.absolutePath}
              </div>
            </div>
            <span className="problem-reason">{t(track.reasonKey ?? "problems.unknown", { defaultValue: track.reasonKey ?? "" })}</span>
            {track.accessStatus === "available" && !track.video && (
              <button type="button" className="btn ghost small" onClick={() => app.playList(tracks, at, "problems")}>
                <Icons.play /> {t("problems.retry")}
              </button>
            )}
            {track.localFileId && !isAndroid && (
              <button type="button" className="icon-btn" title={t("menu.showInFolder")} aria-label={t("menu.showInFolder")} onClick={() => void api.showInFolder(track.localFileId!).catch(app.say)}>
                <Icons.external />
              </button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
