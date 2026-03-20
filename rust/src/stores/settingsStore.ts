import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { AppSettings, SettingsSnapshot } from "@/lib/types";

export interface SettingsDraft {
  translateEnabled: boolean;
  translateProvider: string;
  deeplxUrl: string;
  aiInputMode: "registry" | "custom";
  aiProviderId: string;
  aiModelId: string;
  aiApiKey: string;
  aiBaseUrl: string;
  sourceLang: string;
  targetLang: string;
  translateTimeout: string;
  translateMaxConcurrency: string;
  translateMaxRequestsPerSecond: string;
  pollInterval: string;
  useRightPanelDetails: boolean;
  displayWidth: string;
  collapsedDisplayCount: string;
  ghostMode: boolean;
  imageCapture: boolean;
  bgOpacity: string;
  blur: string;
  cardStyle: string;
  textEnhance: string;
  dictProvider: string;
  agentAiInputMode: "registry" | "custom";
  agentAiProviderId: string;
  agentAiModelId: string;
  agentAiApiKey: string;
  agentAiBaseUrl: string;
  ttsEnabled: boolean;
}

export function draftFromSettings(settings: AppSettings): SettingsDraft {
  const isCustomMode =
    settings.translate.ai_base_url && !settings.translate.ai_provider_id;
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
    translateMaxRequestsPerSecond: String(
      settings.translate.max_requests_per_second,
    ),
    pollInterval: String(settings.listen.interval_seconds),
    useRightPanelDetails: settings.listen.use_right_panel_details,
    displayWidth: String(settings.display.width),
    collapsedDisplayCount: String(settings.display.collapsed_display_count || 3),
    ghostMode: settings.display.ghost_mode ?? false,
    imageCapture: settings.display.image_capture ?? false,
    bgOpacity: String(settings.display.sidebar_appearance?.bg_opacity ?? 0.8),
    blur: settings.display.sidebar_appearance?.blur ?? "strong",
    cardStyle: settings.display.sidebar_appearance?.card_style ?? "standard",
    textEnhance: settings.display.sidebar_appearance?.text_enhance ?? "none",
    dictProvider: settings.dict?.provider || "cambridge",
    agentAiInputMode:
      settings.agent?.ai_base_url && !settings.agent?.ai_provider_id
        ? "custom"
        : "registry",
    agentAiProviderId: settings.agent?.ai_provider_id || "",
    agentAiModelId: settings.agent?.ai_model_id || "",
    agentAiApiKey: settings.agent?.ai_api_key || "",
    agentAiBaseUrl: settings.agent?.ai_base_url || "",
    ttsEnabled: settings.tts?.enabled ?? false,
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
      max_concurrency:
        Math.max(1, parseInt(draft.translateMaxConcurrency, 10)) || 1,
      max_requests_per_second:
        Math.max(1, parseInt(draft.translateMaxRequestsPerSecond, 10)) || 1,
    },
    display: {
      ...settings.display,
      width: parseInt(draft.displayWidth, 10) || 420,
      collapsed_display_count:
        Math.max(1, parseInt(draft.collapsedDisplayCount, 10)) || 3,
      ghost_mode: draft.ghostMode,
      image_capture: draft.imageCapture,
      sidebar_appearance: {
        bg_opacity: Math.max(
          0.2,
          Math.min(1.0, parseFloat(draft.bgOpacity) || 0.8),
        ),
        blur: draft.blur as "none" | "weak" | "medium" | "strong",
        card_style: draft.cardStyle as
          | "transparent"
          | "light"
          | "standard"
          | "dark",
        text_enhance: draft.textEnhance as "none" | "shadow" | "bold",
      },
    },
    dict: {
      ...settings.dict,
      provider: draft.dictProvider,
    },
    agent: {
      ...settings.agent,
      ai_provider_id: draft.agentAiProviderId,
      ai_model_id: draft.agentAiModelId,
      ai_api_key: draft.agentAiApiKey,
      ai_base_url: draft.agentAiBaseUrl,
    },
    tts: {
      ...settings.tts,
      enabled: draft.ttsEnabled,
    },
  };
}

export function createEmptyDraft(): SettingsDraft {
  return {
    translateEnabled: false,
    translateProvider: "deeplx",
    deeplxUrl: "",
    aiInputMode: "registry",
    aiProviderId: "",
    aiModelId: "",
    aiApiKey: "",
    aiBaseUrl: "",
    sourceLang: "auto",
    targetLang: "EN",
    translateTimeout: "15",
    translateMaxConcurrency: "3",
    translateMaxRequestsPerSecond: "3",
    pollInterval: "1",
    useRightPanelDetails: false,
    displayWidth: "420",
    collapsedDisplayCount: "3",
    ghostMode: false,
    imageCapture: false,
    bgOpacity: "0.8",
    blur: "strong",
    cardStyle: "standard",
    textEnhance: "none",
    dictProvider: "cambridge",
    agentAiInputMode: "registry",
    agentAiProviderId: "",
    agentAiModelId: "",
    agentAiApiKey: "",
    agentAiBaseUrl: "",
    ttsEnabled: false,
  };
}

interface SettingsStoreState {
  snapshot: SettingsSnapshot | null;
  settings: AppSettings | null;
  applySnapshot: (snapshot: SettingsSnapshot) => void;
  initSettingsListener: () => Promise<() => void>;
}

export const useSettingsStore = create<SettingsStoreState>((set) => ({
  snapshot: null,
  settings: null,

  applySnapshot: (snapshot) =>
    set((state) => {
      if (state.snapshot && snapshot.version < state.snapshot.version) {
        return state;
      }
      return {
        snapshot,
        settings: snapshot.data,
      };
    }),

  initSettingsListener: async () => {
    const unlisten = await listen<SettingsSnapshot>("settings-updated", (event) => {
      useSettingsStore.getState().applySnapshot(event.payload);
    });

    return unlisten;
  },
}));
