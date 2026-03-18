import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import type { ServiceEvent } from "@/lib/types";

const MAX_EVENTS = 500;
const TRIM_TO = 300;

interface EventStoreState {
  events: ServiceEvent[];
  addEvent: (event: ServiceEvent) => void;
  initEventListener: () => Promise<() => void>;
}

export const useEventStore = create<EventStoreState>((set) => ({
  events: [],

  addEvent: (event) =>
    set((state) => {
      const next = [...state.events, event];
      return {
        events: next.length > MAX_EVENTS ? next.slice(next.length - TRIM_TO) : next,
      };
    }),

  initEventListener: async () => {
    const unlisten = await listen<ServiceEvent>("wechat-event", (e) => {
      const event = e.payload;
      const { addEvent } = useEventStore.getState();
      addEvent(event);
    });

    return unlisten;
  },
}));
