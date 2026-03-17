import { create } from "zustand";
import * as api from "@/lib/tauri-api";
import type { PreflightResult } from "@/lib/tauri-api";

const PROMPT_RECHECK_DELAY_MS = 900;
const PREFLIGHT_LOG_PREFIX = "[preflight]";

function logPreflight(message: string, extra?: unknown) {
  if (extra) {
    console.info(PREFLIGHT_LOG_PREFIX, message, extra);
    return;
  }
  console.info(PREFLIGHT_LOG_PREFIX, message);
}

function shouldAutoRecoverListener(
  previous: PreflightResult | null,
  next: PreflightResult,
  awaitingUserAction: boolean,
) {
  return (
    !!previous &&
    previous.accessibility_ok === false &&
    next.accessibility_ok === true &&
    awaitingUserAction
  );
}

interface PreflightStoreState {
  result: PreflightResult | null;
  loading: boolean;
  promptingAccessibility: boolean;
  accessibilityFlowStarted: boolean;
  lastPromptAt: number | null;
  settingsOpened: boolean;
  awaitingUserAction: boolean;
  recoveringListener: boolean;
  recoveryAttempted: boolean;
  recoveryFailed: boolean;
  check: () => Promise<void>;
  runAccessibilityFlow: () => Promise<void>;
  openAccessibilitySettings: () => Promise<void>;
  retryAccessibilityCheck: () => Promise<void>;
  recoverListener: () => Promise<void>;
  promptRestartFallback: () => Promise<void>;
}

export const usePreflightStore = create<PreflightStoreState>((set, get) => {
  const fetchPreflight = async (): Promise<PreflightResult> => {
    set({ loading: true });
    try {
      const result = await api.preflightCheck();
      set({
        result,
        loading: false,
      });
      logPreflight("check result", {
        wechat_running: result.wechat_running,
        accessibility_ok: result.accessibility_ok,
        wechat_accessible: result.wechat_accessible,
        wechat_has_window: result.wechat_has_window,
      });
      return result;
    } catch {
      const fallback = {
        wechat_running: false,
        accessibility_ok: false,
        wechat_has_window: false,
      };
      set({ result: fallback, loading: false });
      logPreflight("check failed, use fallback result");
      return fallback;
    }
  };

  const recoverListener = async () => {
    const state = get();
    if (state.recoveringListener) {
      logPreflight("listener recovery already in progress");
      return;
    }

    set({
      recoveringListener: true,
      recoveryFailed: false,
      recoveryAttempted: true,
      awaitingUserAction: false,
      promptingAccessibility: false,
    });
    logPreflight("start listener recovery");

    try {
      const response = await api.accessibilityRecoverListener();
      logPreflight("listener recovery succeeded", response);
      set({
        recoveringListener: false,
        recoveryFailed: false,
        accessibilityFlowStarted: false,
        settingsOpened: false,
        lastPromptAt: null,
      });
      await fetchPreflight();
    } catch (error) {
      set({
        recoveringListener: false,
        recoveryFailed: true,
      });
      logPreflight("listener recovery failed", error);
    }
  };

  const runAccessibilityFlow = async () => {
    const state = get();
    const current = state.result;
    if (!current || !current.wechat_running || current.accessibility_ok) {
      return;
    }
    if (state.accessibilityFlowStarted || state.promptingAccessibility) {
      logPreflight("flow already started, skip auto trigger");
      return;
    }

    set({
      accessibilityFlowStarted: true,
      promptingAccessibility: true,
      lastPromptAt: Date.now(),
      recoveryFailed: false,
      recoveryAttempted: false,
    });
    logPreflight("start accessibility flow");

    try {
      const promptResult = await api.accessibilityRequestAccess();
      logPreflight("prompt attempted", promptResult);
      await new Promise((resolve) => setTimeout(resolve, PROMPT_RECHECK_DELAY_MS));
      const afterPrompt = await fetchPreflight();
      if (!afterPrompt.accessibility_ok && !get().settingsOpened) {
        await get().openAccessibilitySettings();
      }
      if (!afterPrompt.accessibility_ok) {
        set({ awaitingUserAction: true });
        logPreflight("awaiting user action in system settings");
      }
    } finally {
      set({ promptingAccessibility: false });
      logPreflight("flow step finished");
    }
  };

  return {
    result: null,
    loading: false,
    promptingAccessibility: false,
    accessibilityFlowStarted: false,
    lastPromptAt: null,
    settingsOpened: false,
    awaitingUserAction: false,
    recoveringListener: false,
    recoveryAttempted: false,
    recoveryFailed: false,

    check: async () => {
      const previous = get().result;
      const awaitingUserAction = get().awaitingUserAction;
      const result = await fetchPreflight();

      if (shouldAutoRecoverListener(previous, result, awaitingUserAction)) {
        if (!get().recoveryAttempted) {
          await get().recoverListener();
        }
        return;
      }

      if (result.wechat_running && !result.accessibility_ok) {
        set({
          recoveryAttempted: false,
          recoveryFailed: false,
        });
        await get().runAccessibilityFlow();
      } else if (result.accessibility_ok && !get().recoveringListener) {
        set({ awaitingUserAction: false, promptingAccessibility: false });
      }
    },

    runAccessibilityFlow,

    openAccessibilitySettings: async () => {
      try {
        const response = await api.accessibilityOpenSettings();
        logPreflight("open settings result", response);
        if (response.ok) {
          set({
            settingsOpened: true,
            awaitingUserAction: true,
          });
        }
      } catch {
        logPreflight("open settings failed");
      }
    },

    retryAccessibilityCheck: async () => {
      logPreflight("manual retry check");
      await get().check();
    },

    recoverListener,

    promptRestartFallback: async () => {
      try {
        const response = await api.preflightPromptRestart();
        logPreflight("prompt restart fallback", response);
      } catch (error) {
        logPreflight("prompt restart fallback failed", error);
      }
    },
  };
});
