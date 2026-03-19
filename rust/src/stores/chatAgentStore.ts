import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import * as api from "@/lib/tauri-api";
import type { AgentChatResponse, AgentToolCallEvent } from "@/lib/tauri-api";

export type MessageRole = "user" | "assistant" | "error";

export interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  toolCalls?: AgentToolCallEvent[];
  timestamp: number;
}

interface ChatAgentState {
  sessionId: string | null;
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;

  initSession: () => Promise<void>;
  sendMessage: (text: string) => Promise<void>;
  cancelChat: () => void;
  clearHistory: () => Promise<void>;
  _handleResponse: (resp: AgentChatResponse) => void;
}

let _unlisten: (() => void) | null = null;
let _initLock = false;
let _msgIdCounter = 0;
let _requestNonce = 0;

function nextId(suffix: string): string {
  return `${Date.now()}-${++_msgIdCounter}-${suffix}`;
}

export const useChatAgentStore = create<ChatAgentState>((set, get) => ({
  sessionId: null,
  messages: [],
  isLoading: false,
  error: null,

  initSession: async () => {
    if (_initLock) return;
    _initLock = true;

    try {
      if (_unlisten) {
        _unlisten();
        _unlisten = null;
      }

      const resp = await api.agentSessionNew();
      set({ sessionId: resp.session_id, messages: [], error: null });

      const unlisten = await listen<AgentChatResponse>("agent-chat-response", (event) => {
        const payload = event.payload;
        if (payload.session_id === get().sessionId) {
          get()._handleResponse(payload);
        }
      });
      _unlisten = unlisten;
    } finally {
      _initLock = false;
    }
  },

  sendMessage: async (text: string) => {
    const { sessionId } = get();
    if (!sessionId || !text.trim()) return;

    const userMsg: ChatMessage = {
      id: nextId("user"),
      role: "user",
      content: text.trim(),
      timestamp: Date.now(),
    };

    const nonce = ++_requestNonce;
    set((s) => ({ messages: [...s.messages, userMsg], isLoading: true, error: null }));

    try {
      await api.agentChat(sessionId, text.trim());
    } catch (e) {
      if (_requestNonce !== nonce) return;
      const errMsg: ChatMessage = {
        id: nextId("err"),
        role: "error",
        content: String(e),
        timestamp: Date.now(),
      };
      set((s) => ({ messages: [...s.messages, errMsg], isLoading: false, error: String(e) }));
    }
  },

  cancelChat: () => {
    if (!get().isLoading) return;
    ++_requestNonce;
    set({ isLoading: false });
  },

  clearHistory: async () => {
    const { sessionId } = get();
    if (sessionId) {
      await api.agentSessionClear(sessionId);
    }
    set({ messages: [], error: null });
  },

  _handleResponse: (resp: AgentChatResponse) => {
    const { messages, isLoading } = get();
    if (!isLoading) return;
    set({ isLoading: false });

    if (resp.is_error) {
      const lastMsg = messages[messages.length - 1];
      if (lastMsg?.role === "error" && lastMsg.content === (resp.error_message ?? "未知错误")) {
        return;
      }
      const errMsg: ChatMessage = {
        id: nextId("err"),
        role: "error",
        content: resp.error_message ?? "未知错误",
        timestamp: Date.now(),
      };
      set((s) => ({ messages: [...s.messages, errMsg], error: resp.error_message ?? null }));
    } else {
      const lastMsg = messages[messages.length - 1];
      if (lastMsg?.role === "assistant" && lastMsg.content === resp.response) {
        return;
      }
      const assistantMsg: ChatMessage = {
        id: nextId("assistant"),
        role: "assistant",
        content: resp.response,
        toolCalls: resp.tool_calls.length > 0 ? resp.tool_calls : undefined,
        timestamp: Date.now(),
      };
      set((s) => ({ messages: [...s.messages, assistantMsg] }));
    }
  },
}));
