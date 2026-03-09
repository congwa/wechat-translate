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

export const useSidebarStore = create<SidebarStoreState>((set) => ({
  items: [],
  currentChat: "",

  addMessage: (event) => {
    const p = event.payload as Record<string, unknown>;
    const msg: SidebarMessage = {
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

    set((state) => {
      const next = [...state.items, msg];
      return {
        items: next.length > MAX_MESSAGES ? next.slice(next.length - TRIM_TO) : next,
        currentChat: msg.chatName || state.currentChat,
      };
    });
  },

  setChatSwitched: (chatName) => {
    set({ items: [], currentChat: chatName });
  },

  loadHistory: (dbMessages, chatName) => {
    if (dbMessages.length === 0) return;

    const historyItems: SidebarMessage[] = dbMessages
      .slice()
      .reverse()
      .map(storedToSidebar);

    set((state) => {
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
        currentChat: chatName || state.currentChat,
      };
    });
  },

  clearMessages: () => set({ items: [], currentChat: "" }),
}));
