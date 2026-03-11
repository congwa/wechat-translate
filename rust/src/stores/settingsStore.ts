import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { AppSettings } from "@/lib/types";

export interface SettingsDraft {
  translateEnabled: boolean;
  translateProvider: string;
  deeplxUrl: string;
  // AI 翻译相关
  aiInputMode: "registry" | "custom"; // registry = models.dev, custom = 自定义
  aiProviderId: string;
  aiModelId: string;
  aiApiKey: string;
  aiBaseUrl: string;
  // 通用配置
  sourceLang: string;
  targetLang: string;
  translateTimeout: string;
  translateMaxConcurrency: string;
  translateMaxRequestsPerSecond: string;
  pollInterval: string;
  useRightPanelDetails: boolean;
  displayWidth: string;
  // 词典设置
  dictProvider: string;
}

function createDefaultSettings(): AppSettings {
  return {
    listen: {
      mode: "session",
      targets: [],
      interval_seconds: 1,
      dedupe_window_seconds: 2.5,
      session_preview_dedupe_window_seconds: 20,
      cross_source_merge_window_seconds: 3,
      focus_refresh: false,
      worker_debug: false,
      use_right_panel_details: false,
    },
    translate: {
      enabled: true,
      provider: "deeplx",
      deeplx_url: "",
      source_lang: "auto",
      target_lang: "EN",
      timeout_seconds: 15,
      max_concurrency: 3,
      max_requests_per_second: 3,
      ai_provider_id: "",
      ai_model_id: "",
      ai_api_key: "",
      ai_base_url: "",
    },
    display: {
      english_only: true,
      on_translate_fail: "show_cn_with_reason",
      width: 420,
      side: "right",
    },
    logging: {
      file: "logs/sidebar_listener.log",
    },
    dict: {
      provider: "cambridge",
    },
  };
}

export function draftFromSettings(settings: AppSettings): SettingsDraft {
  // 判断是否为自定义模式：如果有 base_url 但没有 provider_id，或者 provider_id 不在已知列表中
  const isCustomMode = settings.translate.ai_base_url && !settings.translate.ai_provider_id;
  return {
    translateEnabled: settings.translate.enabled,
    translateProvider: settings.translate.provider,
    deeplxUrl: settings.translate.deeplx_url,
    aiInputMode: isCustomMode ? "custom" : "registry",
    aiProviderId: settings.translate.ai_provider_id || "",
    aiModelId: settings.translate.ai_model_id || "",
    aiApiKey: settings.translate.ai_api_key || "",
    aiBaseUrl: settings.translate.ai_base_url || "",
    sourceLang: settings.translate.source_lang,
    targetLang: settings.translate.target_lang,
    translateTimeout: String(settings.translate.timeout_seconds),
    translateMaxConcurrency: String(settings.translate.max_concurrency),
    translateMaxRequestsPerSecond: String(settings.translate.max_requests_per_second),
    pollInterval: String(settings.listen.interval_seconds),
    useRightPanelDetails: settings.listen.use_right_panel_details,
    displayWidth: String(settings.display.width),
    dictProvider: settings.dict?.provider || "cambridge",
  };
}

export function settingsFromDraft(
  settings: AppSettings,
  draft: SettingsDraft,
): AppSettings {
  return {
    ...settings,
    listen: {
      ...settings.listen,
      interval_seconds: parseFloat(draft.pollInterval) || 1,
      use_right_panel_details: draft.useRightPanelDetails,
    },
    translate: {
      ...settings.translate,
      enabled: draft.translateEnabled,
      provider: draft.translateProvider,
      deeplx_url: draft.deeplxUrl,
      ai_provider_id: draft.aiProviderId,
      ai_model_id: draft.aiModelId,
      ai_api_key: draft.aiApiKey,
      ai_base_url: draft.aiBaseUrl,
      source_lang: draft.sourceLang,
      target_lang: draft.targetLang,
      timeout_seconds: parseFloat(draft.translateTimeout) || 15,
      max_concurrency: Math.max(1, parseInt(draft.translateMaxConcurrency, 10) || 1),
      max_requests_per_second: Math.max(1, parseInt(draft.translateMaxRequestsPerSecond, 10) || 1),
    },
    display: {
      ...settings.display,
      width: parseInt(draft.displayWidth, 10) || 420,
    },
    dict: {
      ...settings.dict,
      provider: draft.dictProvider,
    },
  };
}

export type SettingsSection = "listen" | "translate" | "display";

interface SectionDirtyState {
  listen: boolean;
  translate: boolean;
  display: boolean;
}

interface SettingsStoreState {
  settings: AppSettings | null;
  draft: SettingsDraft;
  sectionDirty: SectionDirtyState;
  setSettings: (settings: AppSettings) => void;
  updateDraft: (patch: Partial<SettingsDraft>, section?: SettingsSection) => void;
  resetDraft: () => void;
  resetSection: (section: SettingsSection) => void;
  markSectionClean: (section: SettingsSection) => void;
  initSettingsListener: () => Promise<() => void>;
}

const SECTION_FIELDS: Record<SettingsSection, (keyof SettingsDraft)[]> = {
  listen: ["pollInterval", "useRightPanelDetails"],
  translate: [
    "translateEnabled",
    "translateProvider",
    "deeplxUrl",
    "aiProviderId",
    "aiModelId",
    "aiApiKey",
    "aiBaseUrl",
    "sourceLang",
    "targetLang",
    "translateTimeout",
    "translateMaxConcurrency",
    "translateMaxRequestsPerSecond",
  ],
  display: ["displayWidth"],
};

export const useSettingsStore = create<SettingsStoreState>((set, get) => ({
  settings: null,
  draft: draftFromSettings(createDefaultSettings()),
  sectionDirty: { listen: false, translate: false, display: false },

  setSettings: (settings) =>
    set({
      settings,
      draft: draftFromSettings(settings),
      sectionDirty: { listen: false, translate: false, display: false },
    }),

  updateDraft: (patch, section) =>
    set((state) => {
      const nextDraft = { ...state.draft, ...patch };
      const nextDirty = { ...state.sectionDirty };

      if (section) {
        nextDirty[section] = true;
      } else {
        // Auto-detect section from patch keys
        for (const key of Object.keys(patch) as (keyof SettingsDraft)[]) {
          for (const sec of Object.keys(SECTION_FIELDS) as SettingsSection[]) {
            if (SECTION_FIELDS[sec].includes(key)) {
              nextDirty[sec] = true;
            }
          }
        }
      }

      return { draft: nextDraft, sectionDirty: nextDirty };
    }),

  resetDraft: () => {
    const settings = get().settings;
    if (!settings) return;
    set({
      draft: draftFromSettings(settings),
      sectionDirty: { listen: false, translate: false, display: false },
    });
  },

  resetSection: (section) => {
    const settings = get().settings;
    if (!settings) return;
    const baseline = draftFromSettings(settings);
    const patch: Partial<SettingsDraft> = {};
    for (const key of SECTION_FIELDS[section]) {
      (patch as Record<string, unknown>)[key] = baseline[key];
    }
    set((state) => ({
      draft: { ...state.draft, ...patch },
      sectionDirty: { ...state.sectionDirty, [section]: false },
    }));
  },

  markSectionClean: (section) =>
    set((state) => ({
      sectionDirty: { ...state.sectionDirty, [section]: false },
    })),

  initSettingsListener: async () => {
    const unlisten = await listen<AppSettings>("settings-updated", (event) => {
      useSettingsStore.getState().setSettings(event.payload);
    });

    return unlisten;
  },
}));
