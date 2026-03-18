import { create } from "zustand";
import type { SidebarSnapshot } from "@/lib/types";

const EMPTY_SIDEBAR_SNAPSHOT: SidebarSnapshot = {
  version: 0,
  current_chat: "",
  messages: [],
  translator: {
    enabled: false,
    configured: false,
    checking: false,
    healthy: null,
    last_error: null,
    provider: "",
  },
  refresh_version: 0,
};

interface SidebarStoreState {
  snapshot: SidebarSnapshot;
  loading: boolean;
  invalidatedVersion: number;
  setLoading: (loading: boolean) => void;
  applySnapshot: (snapshot: SidebarSnapshot) => void;
  invalidate: (version: number) => void;
  clearSnapshot: () => void;
}

export const useSidebarStore = create<SidebarStoreState>((set) => ({
  snapshot: EMPTY_SIDEBAR_SNAPSHOT,
  loading: false,
  invalidatedVersion: 0,

  setLoading: (loading) => set({ loading }),

  applySnapshot: (snapshot) =>
    set((state) => {
      if (snapshot.version < state.snapshot.version) {
        return state;
      }
      return {
        snapshot,
        invalidatedVersion: Math.max(state.invalidatedVersion, snapshot.version),
        loading: false,
      };
    }),

  invalidate: (version) =>
    set((state) => ({
      invalidatedVersion: Math.max(state.invalidatedVersion, version),
    })),

  clearSnapshot: () =>
    set({
      snapshot: EMPTY_SIDEBAR_SNAPSHOT,
      invalidatedVersion: 0,
      loading: false,
    }),
}));
