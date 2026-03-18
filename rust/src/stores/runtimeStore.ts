import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { AppRuntime } from "@/lib/types";

const defaultRuntime: AppRuntime = {
  version: 0,
  tasks: {
    monitoring: false,
    sidebar: false,
  },
  translator: {
    enabled: false,
    configured: false,
    checking: false,
    healthy: null,
    last_error: null,
    provider: "",
  },
  close_to_tray: true,
};

interface RuntimeStoreState {
  runtime: AppRuntime;
  applySnapshot: (runtime: AppRuntime) => void;
  initRuntimeListener: () => Promise<() => void>;
}

export const useRuntimeStore = create<RuntimeStoreState>((set) => ({
  runtime: defaultRuntime,

  applySnapshot: (runtime) =>
    set((state) => {
      if (runtime.version < state.runtime.version) {
        return state;
      }
      return { runtime };
    }),

  initRuntimeListener: async () => {
    const unlisten = await listen<AppRuntime>("runtime-updated", (event) => {
      useRuntimeStore.getState().applySnapshot(event.payload);
    });

    return unlisten;
  },
}));
