import { create } from "zustand";
import { persist } from "zustand/middleware";

export type DisplayMode = "translated" | "original" | "bilingual";
export type SidebarWindowMode = "follow" | "independent";

interface UiPreferencesStoreState {
  displayMode: DisplayMode;
  sidebarWindowMode: SidebarWindowMode;
  setPreferences: (
    patch: Partial<Omit<UiPreferencesStoreState, "setPreferences">>,
  ) => void;
}

export const useUiPreferencesStore = create<UiPreferencesStoreState>()(
  persist(
    (set) => ({
      displayMode: "bilingual",
      sidebarWindowMode: "follow",
      setPreferences: (patch) => set(patch),
    }),
    {
      name: "wechat-ui-preferences",
      partialize: (state) => ({
        displayMode: state.displayMode,
        sidebarWindowMode: state.sidebarWindowMode,
      }),
    },
  ),
);
