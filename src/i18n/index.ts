// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import zh from "./zh.json";

/** The language from the last run, else the system's, so Chinese users do not see English before settings load. */
function startLanguage(): "en" | "zh" {
  try {
    const saved = window.localStorage.getItem("language");
    if (saved === "en" || saved === "zh") return saved;
  } catch {
    // Storage can be unavailable; fall through to the system language.
  }
  return typeof navigator !== "undefined" && navigator.language?.toLowerCase().startsWith("zh") ? "zh" : "en";
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, zh: { translation: zh } },
  lng: startLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
