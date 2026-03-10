import { create } from "zustand";
import type { WordEntry } from "@/lib/tauri-api";
import * as api from "@/lib/tauri-api";

function simpleHash(text: string): string {
  let hash = 0;
  for (let i = 0; i < text.length; i++) {
    hash = ((hash << 5) - hash) + text.charCodeAt(i);
    hash = hash & hash;
  }
  return hash.toString(36);
}

interface DictionaryStoreState {
  // UI 状态
  currentWord: string | null;
  currentEntry: WordEntry | null;
  loading: boolean;
  error: string | null;
  
  // 弹窗位置
  popoverPosition: { x: number; y: number } | null;
  
  // 正在进行的请求（防重复）
  pendingLookups: Set<string>;
  pendingTranslations: Set<string>;
  
  // Actions
  lookupWord: (word: string, position?: { x: number; y: number }) => Promise<WordEntry | null>;
  translateDefinition: (text: string) => Promise<string | null>;
  clearCurrent: () => void;
  setPopoverPosition: (position: { x: number; y: number } | null) => void;
}

export const useDictionaryStore = create<DictionaryStoreState>((set, get) => ({
  currentWord: null,
  currentEntry: null,
  loading: false,
  error: null,
  popoverPosition: null,
  pendingLookups: new Set(),
  pendingTranslations: new Set(),

  lookupWord: async (word, position) => {
    const normalized = word.toLowerCase().trim();
    if (!normalized) return null;
    
    // 防重复请求
    if (get().pendingLookups.has(normalized)) {
      return get().currentEntry;
    }
    
    set((s) => ({
      currentWord: normalized,
      loading: true,
      error: null,
      popoverPosition: position ?? s.popoverPosition,
      pendingLookups: new Set(s.pendingLookups).add(normalized),
    }));
    
    try {
      const entry = await api.wordLookup(normalized);
      set({ currentEntry: entry, loading: false });
      return entry;
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      set({ error: errorMsg, loading: false });
      return null;
    } finally {
      set((s) => {
        const next = new Set(s.pendingLookups);
        next.delete(normalized);
        return { pendingLookups: next };
      });
    }
  },

  translateDefinition: async (text) => {
    const hash = simpleHash(text);
    
    // 防重复请求
    if (get().pendingTranslations.has(hash)) {
      return null;
    }
    
    set((s) => ({
      pendingTranslations: new Set(s.pendingTranslations).add(hash),
    }));
    
    try {
      // 明确指定源语言为英文，目标语言为中文
      const translated = await api.translateCached({
        text,
        sourceLang: "en",
        targetLang: "zh",
      });
      return translated;
    } catch {
      return null;
    } finally {
      set((s) => {
        const next = new Set(s.pendingTranslations);
        next.delete(hash);
        return { pendingTranslations: next };
      });
    }
  },
  
  clearCurrent: () => set({
    currentWord: null,
    currentEntry: null,
    loading: false,
    error: null,
    popoverPosition: null,
  }),

  setPopoverPosition: (position) => set({ popoverPosition: position }),
}));
