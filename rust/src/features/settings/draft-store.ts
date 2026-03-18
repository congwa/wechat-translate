import { create } from "zustand";
import type { ProviderInfo } from "@/lib/models-registry";
import {
  createEmptyDraft,
  draftFromSettings,
  type SettingsDraft,
} from "@/stores/settingsStore";
import type { AppSettings } from "@/lib/types";
import { BUILTIN_PROVIDERS } from "@/lib/models-registry";

export type SettingsDraftSection = "listen" | "translate" | "display";

export interface SectionDirtyState {
  listen: boolean;
  translate: boolean;
  display: boolean;
}

export const SECTION_FIELDS: Record<
  SettingsDraftSection,
  (keyof SettingsDraft)[]
> = {
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
  display: [
    "displayWidth",
    "collapsedDisplayCount",
    "ghostMode",
    "imageCapture",
    "bgOpacity",
    "blur",
    "cardStyle",
    "textEnhance",
  ],
};

interface AdvancedConfigEditorState {
  open: boolean;
  raw: string;
  errors: string[];
  valid: boolean;
  dirty: boolean;
  lastLoadedRaw: string;
}

interface AiProviderRegistryState {
  providers: ProviderInfo[];
  loading: boolean;
  showApiKey: boolean;
}

interface SettingsDraftStoreState {
  draft: SettingsDraft;
  sectionDirty: SectionDirtyState;
  advancedEditor: AdvancedConfigEditorState;
  aiRegistry: AiProviderRegistryState;
  resetFromSettings: (settings: AppSettings) => void;
  updateDraft: (
    patch: Partial<SettingsDraft>,
    section?: SettingsDraftSection,
  ) => void;
  resetSection: (section: SettingsDraftSection, settings: AppSettings) => void;
  markSectionClean: (section: SettingsDraftSection) => void;
  setAdvancedEditor: (patch: Partial<AdvancedConfigEditorState>) => void;
  setAiRegistry: (patch: Partial<AiProviderRegistryState>) => void;
}

const defaultSectionDirty: SectionDirtyState = {
  listen: false,
  translate: false,
  display: false,
};

export const useSettingsDraftStore = create<SettingsDraftStoreState>((set) => ({
  draft: createEmptyDraft(),
  sectionDirty: defaultSectionDirty,
  advancedEditor: {
    open: false,
    raw: "",
    errors: [],
    valid: true,
    dirty: false,
    lastLoadedRaw: "",
  },
  aiRegistry: {
    providers: BUILTIN_PROVIDERS,
    loading: false,
    showApiKey: false,
  },

  resetFromSettings: (settings) =>
    set({
      draft: draftFromSettings(settings),
      sectionDirty: defaultSectionDirty,
    }),

  updateDraft: (patch, section) =>
    set((state) => {
      const nextDirty = { ...state.sectionDirty };
      if (section) {
        nextDirty[section] = true;
      } else {
        for (const key of Object.keys(patch) as (keyof SettingsDraft)[]) {
          for (const candidate of Object.keys(
            SECTION_FIELDS,
          ) as SettingsDraftSection[]) {
            if (SECTION_FIELDS[candidate].includes(key)) {
              nextDirty[candidate] = true;
            }
          }
        }
      }

      return {
        draft: { ...state.draft, ...patch },
        sectionDirty: nextDirty,
      };
    }),

  resetSection: (section, settings) =>
    set((state) => {
      const baseline = draftFromSettings(settings);
      const patch: Partial<SettingsDraft> = {};
      for (const key of SECTION_FIELDS[section]) {
        (patch as Record<string, unknown>)[key] = baseline[key];
      }
      return {
        draft: { ...state.draft, ...patch },
        sectionDirty: { ...state.sectionDirty, [section]: false },
      };
    }),

  markSectionClean: (section) =>
    set((state) => ({
      sectionDirty: { ...state.sectionDirty, [section]: false },
    })),

  setAdvancedEditor: (patch) =>
    set((state) => ({
      advancedEditor: { ...state.advancedEditor, ...patch },
    })),

  setAiRegistry: (patch) =>
    set((state) => ({
      aiRegistry: { ...state.aiRegistry, ...patch },
    })),
}));
