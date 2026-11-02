// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState } from "react";

/** Rounded hover tips that replace the square native `title` tooltips. */
export function Tooltip() {
  const [tip, setTip] = useState<{ text: string; x: number; y: number } | null>(null);
  const timer = useRef(0);
  const current = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const hide = () => {
      window.clearTimeout(timer.current);
      // Give back the `title` taken on hover, so React stays the owner of it and a title that
      // later changes or goes away is not shown from a stale copy.
      const shown = current.current;
      if (shown?.dataset.tipTitle) {
        shown.setAttribute("title", shown.getAttribute("data-tip") ?? "");
        shown.removeAttribute("data-tip");
        if (shown.dataset.tipTitle === "label") shown.removeAttribute("aria-label");
        delete shown.dataset.tipTitle;
      }
      current.current = null;
      setTip(null);
    };
    const over = (event: PointerEvent) => {
      const target = (event.target as HTMLElement | null)?.closest?.("[title],[data-tip]") as HTMLElement | null;
      if (!target) {
        if (current.current) hide();
        return;
      }
      if (target === current.current) return;
      hide();
      const title = target.getAttribute("title");
      if (title) {
        const label = !target.getAttribute("aria-label") && !target.textContent?.trim();
        if (label) target.setAttribute("aria-label", title);
        target.setAttribute("data-tip", title);
        target.removeAttribute("title");
        target.dataset.tipTitle = label ? "label" : "1";
      }
      const text = target.getAttribute("data-tip");
      if (!text) return;
      current.current = target;
      timer.current = window.setTimeout(() => {
        const rect = target.getBoundingClientRect();
        const below = rect.bottom + 48 < window.innerHeight;
        setTip({ text, x: rect.left + rect.width / 2, y: below ? rect.bottom + 8 : rect.top - 8 });
        target.dataset.tipBelow = below ? "1" : "0";
      }, 450);
    };
    window.addEventListener("pointerover", over);
    window.addEventListener("pointerdown", hide, true);
    window.addEventListener("blur", hide);
    window.addEventListener("scroll", hide, true);
    return () => {
      window.removeEventListener("pointerover", over);
      window.removeEventListener("pointerdown", hide, true);
      window.removeEventListener("blur", hide);
      window.removeEventListener("scroll", hide, true);
      window.clearTimeout(timer.current);
    };
  }, []);

  if (!tip) return null;
  const below = current.current?.dataset.tipBelow !== "0";
  const left = Math.max(12, Math.min(tip.x, window.innerWidth - 12));
  return (
    <div className={`tooltip ${below ? "below" : "above"}`} style={{ left, top: tip.y }} role="tooltip">
      {tip.text}
    </div>
  );
}
