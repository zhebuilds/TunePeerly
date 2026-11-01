// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Icons } from "../ui/icons";
import { useBackClose } from "../lib/back";

export function Revise() {
  const { t } = useTranslation();
  return <span className="setting-mark">{t("settings.revise")}</span>;
}

export function NotImplemented() {
  const { t } = useTranslation();
  return <span className="setting-mark">{t("settings.notImplemented")}</span>;
}

export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (value: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`switch${checked ? " on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string; icon?: ReactNode }[]; onChange: (value: T) => void; label: string }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          type="button"
          role="radio"
          aria-checked={value === option.value}
          key={option.value}
          className={value === option.value ? "on" : ""}
          onClick={() => onChange(option.value)}
        >
          {option.icon}
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Select({ value, onChange, options, label, disabled }: { value: string; onChange: (value: string) => void; options: { value: string; label: string; hint?: string }[]; label: string; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [place, setPlace] = useState<{ left: number; top?: number; bottom?: number; width: number; maxHeight: number } | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const listId = useId();
  const selected = Math.max(0, options.findIndex((option) => option.value === value));

  const show = () => {
    if (disabled) return;
    setActive(selected);
    setOpen(true);
  };
  const close = (refocus = false) => {
    setOpen(false);
    if (refocus) button.current?.focus();
  };
  useBackClose(open, () => setOpen(false));
  const choose = (index: number) => {
    const option = options[index];
    close(true);
    if (option && option.value !== value) onChange(option.value);
  };

  // Place the list under the button, or above it when there is no room.
  useLayoutEffect(() => {
    if (!open || !button.current) return;
    const rect = button.current.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - 12;
    const above = rect.top - 12;
    const flip = below < 200 && above > below;
    const width = Math.max(rect.width, options.some((option) => option.hint) ? 300 : 180);
    setPlace({
      left: Math.max(8, Math.min(rect.right - width, window.innerWidth - width - 8)),
      width,
      ...(flip ? { bottom: window.innerHeight - rect.top + 6 } : { top: rect.bottom + 6 }),
      maxHeight: Math.max(120, (flip ? above : below) - 6),
    });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      const target = event.target as Node;
      if (list.current?.contains(target) || button.current?.contains(target)) return;
      setOpen(false);
    };
    const leave = () => setOpen(false);
    document.addEventListener("pointerdown", away, true);
    window.addEventListener("resize", leave);
    window.addEventListener("blur", leave);
    document.addEventListener("scroll", away, true);
    return () => {
      document.removeEventListener("pointerdown", away, true);
      window.removeEventListener("resize", leave);
      window.removeEventListener("blur", leave);
      document.removeEventListener("scroll", away, true);
    };
  }, [open]);

  useEffect(() => {
    if (open) list.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [open, active, place]);

  const onKeyDown = (event: KeyboardEvent) => {
    if (disabled) return;
    const last = options.length - 1;
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        show();
      }
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((index) => Math.min(last, index + 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((index) => Math.max(0, index - 1));
        break;
      case "Home":
        event.preventDefault();
        setActive(0);
        break;
      case "End":
        event.preventDefault();
        setActive(last);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        choose(active);
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        close(true);
        break;
      case "Tab":
        setOpen(false);
        break;
    }
  };

  return (
    <span className={`select${open ? " open" : ""}`}>
      <button
        ref={button}
        type="button"
        className="select-button"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        disabled={disabled}
        onClick={() => (open ? close() : show())}
        onKeyDown={onKeyDown}
      >
        <span className="select-value">{options[selected]?.label ?? ""}</span>
        <Icons.chevronDown className="select-chevron" />
      </button>
      {open &&
        place &&
        createPortal(
          <div
            ref={list}
            className="select-list"
            style={{ left: place.left, top: place.top, bottom: place.bottom, minWidth: place.width, maxHeight: place.maxHeight }}
          >
            <div id={listId} role="listbox" aria-label={label} className="select-options">
              {options.map((option, index) => (
                <div
                  key={option.value}
                  role="option"
                  data-index={index}
                  aria-selected={index === selected}
                  className={`select-option${index === selected ? " selected" : ""}${index === active ? " active" : ""}`}
                  onPointerEnter={() => setActive(index)}
                  onClick={() => choose(index)}
                >
                  <span>{option.label}</span>
                  {index === selected && <Icons.check className="select-check" />}
                </div>
              ))}
            </div>
            {options.some((option) => option.hint) && <div className="select-hint">{options[active]?.hint}</div>}
          </div>,
          document.body,
        )}
    </span>
  );
}

/** A settings row: title and help on the left, the control on the right. */
export function Row({ title, help, children, tone }: { title: ReactNode; help?: ReactNode; children?: ReactNode; tone?: "danger" }) {
  return (
    <div className={`setting-row${tone ? ` ${tone}` : ""}`}>
      <div className="setting-text">
        <div className="setting-title">{title}</div>
        {help && <div className="setting-help">{help}</div>}
      </div>
      {children && <div className="setting-control">{children}</div>}
    </div>
  );
}

/** Text input that only reports on blur or Enter, so typing does not save every keystroke. */
export function CommitInput({ value, onCommit, placeholder, type = "text", label, width, secret }: { value: string; onCommit: (value: string) => void; placeholder?: string; type?: string; label: string; width?: number; secret?: boolean }) {
  const [draft, setDraft] = useState(value);
  const [shown, setShown] = useState(false);
  const reverting = useRef(false);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    if (reverting.current) {
      reverting.current = false;
      return;
    }
    if (draft !== value) onCommit(draft);
  };
  return (
    <span className="commit-input" style={width ? { width } : undefined}>
      <input
        className="input"
        aria-label={label}
        type={secret && !shown ? "password" : type}
        value={draft}
        placeholder={placeholder}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") (event.target as HTMLInputElement).blur();
          if (event.key === "Escape") {
            reverting.current = true;
            setDraft(value);
            (event.target as HTMLInputElement).blur();
          }
        }}
      />
      {secret && (
        <button type="button" className="input-addon" aria-pressed={shown} aria-label={label} onClick={() => setShown((item) => !item)}>
          {shown ? <Icons.eyeOff /> : <Icons.eye />}
        </button>
      )}
    </span>
  );
}
