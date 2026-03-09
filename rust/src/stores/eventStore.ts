import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import type { ServiceEvent, TaskState } from "@/lib/types";
import { useSidebarStore } from "./sidebarStore";

const MAX_EVENTS = 500;
const TRIM_TO = 300;

interface EventStoreState {
  events: ServiceEvent[];
  taskState: TaskState;
  setTaskState: (state: TaskState) => void;
  addEvent: (event: ServiceEvent) => void;
  initEventListener: () => Promise<() => void>;
}

export const useEventStore = create<EventStoreState>((set) => ({
  events: [],
  taskState: { monitoring: false, autoreply: false, sidebar: false },

  setTaskState: (taskState) => set({ taskState }),

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

      if (event.type === "task_state") {
        const state = (event.payload as Record<string, unknown>).state as
          | TaskState
          | undefined;
        if (state) {
          useEventStore.getState().setTaskState(state);
        }
      }

      if (event.type === "message" && event.source === "sidebar") {
        useSidebarStore.getState().addMessage(event);
      }

      if (event.type === "status" && (event.source === "monitor" || event.source === "sidebar")) {
        const p = event.payload as Record<string, unknown>;
        if (p.type === "chat_switched" && typeof p.chat_name === "string") {
          useSidebarStore.getState().setChatSwitched(p.chat_name, event.timestamp);
        }
      }
    });

    return unlisten;
  },
}));
