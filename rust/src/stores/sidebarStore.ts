import { create } from "zustand";
import type { SidebarMessage, StoredMessage } from "@/lib/types";

const MAX_MESSAGES = 200;
const TRIM_TO = 150;

interface SidebarStoreState {
  items: SidebarMessage[];
  currentChat: string;
  refreshVersion: number;
  setCurrentChat: (chatName: string) => void;
  requestRefresh: (chatName?: string) => void;
  hydrateSnapshot: (chatName: string, dbMessages: StoredMessage[]) => void;
  clearMessages: () => void;
}

let snapshotIdCounter = -100000;

function storedToSidebar(message: StoredMessage): SidebarMessage {
  return {
    id: snapshotIdCounter--,
    chatName: message.chat_name,
    sender: message.sender,
    textCn: message.content,
    textEn: message.content_en || "",
    translateError: "",
    timestamp: message.detected_at,
    isSelf: message.is_self,
    imagePath: message.image_path || undefined,
  };
}

export const useSidebarStore = create<SidebarStoreState>((set) => ({
  items: [],
  currentChat: "",
  refreshVersion: 0,

  setCurrentChat: (chatName) =>
    set((state) => {
      if (!chatName || state.currentChat === chatName) {
        return state;
      }
      return {
        currentChat: chatName,
        refreshVersion: state.refreshVersion + 1,
      };
    }),

  requestRefresh: (chatName) =>
    set((state) => ({
      currentChat: state.currentChat || chatName || "",
      refreshVersion: state.refreshVersion + 1,
    })),

  hydrateSnapshot: (chatName, dbMessages) =>
    set((state) => {
      const nextItems = dbMessages
        .slice()
        .reverse()
        .map(storedToSidebar);

      return {
        items:
          nextItems.length > MAX_MESSAGES
            ? nextItems.slice(nextItems.length - TRIM_TO)
            : nextItems,
        currentChat: chatName || state.currentChat,
      };
    }),

  clearMessages: () =>
    set((state) => ({
      items: [],
      currentChat: "",
      refreshVersion: state.refreshVersion + 1,
    })),
}));
