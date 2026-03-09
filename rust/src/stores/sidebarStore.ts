import { create } from "zustand";
import type { ServiceEvent, SidebarMessage, StoredMessage } from "@/lib/types";

const MAX_MESSAGES = 200;
const TRIM_TO = 150;

interface SidebarStoreState {
  items: SidebarMessage[];
  currentChat: string;
  addMessage: (event: ServiceEvent) => void;
  setChatSwitched: (chatName: string, timestamp: string) => void;
  loadHistory: (dbMessages: StoredMessage[], chatName: string) => void;
  clearMessages: () => void;
}

let historyIdCounter = -100000;

function storedToSidebar(m: StoredMessage): SidebarMessage {
  return {
    id: historyIdCounter--,
    chatName: m.chat_name,
    sender: m.sender,
    textCn: m.content,
    textEn: m.content_en || "",
    translateError: "",
    timestamp: m.detected_at,
    isSelf: m.is_self,
    imagePath: m.image_path || undefined,
  };
}

function msgFingerprint(m: SidebarMessage): string {
  return `${m.chatName}\x00${m.sender}\x00${m.textCn}\x00${m.timestamp}`;
}

function parseSidebarMessage(event: ServiceEvent): SidebarMessage {
  const p = event.payload as Record<string, unknown>;
  return {
    id: event.id,
    chatName: (p.chat_name as string) || "",
    sender: (p.sender as string) || "",
    textCn: (p.text_cn as string) || "",
    textEn: (p.text_en as string) || "",
    translateError: (p.translate_error as string) || "",
    timestamp: event.timestamp,
    isSelf: (p.is_self as boolean) || false,
    imagePath: (p.image_path as string) || undefined,
  };
}

function shouldAcceptAppend(currentChat: string, msgChat: string): boolean {
  if (!msgChat) return false;
  return !currentChat || currentChat === msgChat;
}

export const useSidebarStore = create<SidebarStoreState>((set) => ({
  items: [],
  currentChat: "",

  addMessage: (event) => {
    const p = event.payload as Record<string, unknown>;
    const kind = p.kind as string | undefined;
    const messageId = p.message_id as number | undefined;

    if (kind === "update" && typeof messageId === "number") {
      set((state) => {
        const index = state.items.findIndex((item) => item.id === messageId);
        if (index < 0) {
          return state;
        }

        const current = state.items[index];
        const nextItems = [...state.items];
        nextItems[index] = {
          ...current,
          textEn: typeof p.text_en === "string" ? p.text_en : current.textEn,
          translateError:
            typeof p.translate_error === "string"
              ? p.translate_error
              : current.translateError,
          imagePath:
            typeof p.image_path === "string" ? p.image_path : current.imagePath,
        };

        return { items: nextItems };
      });
      return;
    }

    const msg = parseSidebarMessage(event);

    set((state) => {
      if (!shouldAcceptAppend(state.currentChat, msg.chatName)) {
        return state;
      }
      const next = [...state.items, msg];
      return {
        items: next.length > MAX_MESSAGES ? next.slice(next.length - TRIM_TO) : next,
        currentChat: state.currentChat || msg.chatName,
      };
    });
  },

  setChatSwitched: (chatName) =>
    set((state) => {
      if (!chatName || state.currentChat === chatName) {
        return state;
      }
      return { items: [], currentChat: chatName };
    }),

  loadHistory: (dbMessages, chatName) => {
    if (dbMessages.length === 0 || !chatName) return;

    const historyItems: SidebarMessage[] = dbMessages
      .slice()
      .reverse()
      .filter((m) => m.chat_name === chatName)
      .map(storedToSidebar);

    set((state) => {
      if (!shouldAcceptAppend(state.currentChat, chatName)) {
        return state;
      }

      const existingFingerprints = new Set(
        state.items.map(msgFingerprint),
      );

      const newHistory = historyItems.filter(
        (m) => !existingFingerprints.has(msgFingerprint(m)),
      );

      if (newHistory.length === 0) return state;

      const merged = [...newHistory, ...state.items];
      return {
        items: merged.length > MAX_MESSAGES ? merged.slice(merged.length - TRIM_TO) : merged,
        currentChat: state.currentChat || chatName,
      };
    });
  },

  clearMessages: () => set({ items: [], currentChat: "" }),
}));
