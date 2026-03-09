import { create } from "zustand";
import { persist } from "zustand/middleware";

export type DisplayMode = "translated" | "original" | "bilingual";
export type SidebarWindowMode = "follow" | "independent";

interface SettingsFields {
  closeToTray: boolean;

  translateEnabled: boolean;
  deeplxUrl: string;
  sourceLang: string;
  targetLang: string;
  translateTimeout: string;
  translateMaxConcurrency: string;
  translateMaxRequestsPerSecond: string;

  pollInterval: string;
  useRightPanelDetails: boolean;
  displayWidth: string;
  displayMode: DisplayMode;
  sidebarWindowMode: SidebarWindowMode;
  collapsedDisplayCount: string;

  targets: string;
  lastChatName: string;

  imageCapture: boolean;
}

interface FormStoreState extends SettingsFields {
  setSettings: (patch: Partial<SettingsFields>) => void;
  setLastChatName: (name: string) => void;
}

export const useFormStore = create<FormStoreState>()(
  persist(
    (set) => ({
      closeToTray: true,

      translateEnabled: true,
      deeplxUrl: "",
      sourceLang: "auto",
      targetLang: "EN",
      translateTimeout: "8",
      translateMaxConcurrency: "3",
      translateMaxRequestsPerSecond: "3",

      pollInterval: "1",
      useRightPanelDetails: false,
      displayWidth: "420",
      displayMode: "bilingual" as DisplayMode,
      sidebarWindowMode: "follow" as SidebarWindowMode,
      collapsedDisplayCount: "0",

      targets: "",
      lastChatName: "",

      imageCapture: false,

      setSettings: (patch) => set(patch),
      setLastChatName: (name) => set({ lastChatName: name }),
    }),
    {
      name: "wechat-form-settings",
      partialize: (state) => ({
        closeToTray: state.closeToTray,
        translateEnabled: state.translateEnabled,
        deeplxUrl: state.deeplxUrl,
        sourceLang: state.sourceLang,
        targetLang: state.targetLang,
        translateTimeout: state.translateTimeout,
        translateMaxConcurrency: state.translateMaxConcurrency,
        translateMaxRequestsPerSecond: state.translateMaxRequestsPerSecond,
        pollInterval: state.pollInterval,
        useRightPanelDetails: state.useRightPanelDetails,
        displayWidth: state.displayWidth,
        displayMode: state.displayMode,
        sidebarWindowMode: state.sidebarWindowMode,
        collapsedDisplayCount: state.collapsedDisplayCount,
        targets: state.targets,
        lastChatName: state.lastChatName,
        imageCapture: state.imageCapture,
      }),
    },
  ),
);
