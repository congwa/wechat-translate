import { useEffect, useState } from "react";
import type { ThemeMode } from "./types";

export const DEFAULT_THEME_MODE: ThemeMode = "system";

const SYSTEM_THEME_QUERY = "(prefers-color-scheme: dark)";

export function getSystemDarkMode(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia(SYSTEM_THEME_QUERY).matches;
}

export function isThemeMode(value: string | null | undefined): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

export function resolveDarkMode(
  themeMode: ThemeMode,
  systemDarkMode: boolean,
): boolean {
  if (themeMode === "dark") return true;
  if (themeMode === "light") return false;
  return systemDarkMode;
}

export function useResolvedDarkMode(themeMode: ThemeMode): boolean {
  const [systemDarkMode, setSystemDarkMode] = useState<boolean>(getSystemDarkMode);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }

    const mediaQuery = window.matchMedia(SYSTEM_THEME_QUERY);

    function handleChange(event: MediaQueryListEvent) {
      setSystemDarkMode(event.matches);
    }

    setSystemDarkMode(mediaQuery.matches);
    mediaQuery.addEventListener("change", handleChange);

    return () => {
      mediaQuery.removeEventListener("change", handleChange);
    };
  }, []);

  return resolveDarkMode(themeMode, systemDarkMode);
}

export function useApplyThemeMode(themeMode: ThemeMode): boolean {
  const isDarkMode = useResolvedDarkMode(themeMode);

  useEffect(() => {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.classList.toggle("dark", isDarkMode);
    root.style.colorScheme = isDarkMode ? "dark" : "light";
  }, [isDarkMode]);

  return isDarkMode;
}
