import { useRef, useEffect, useState, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { X, MessageCircle, Languages, AlertCircle, ChevronUp, ChevronDown } from "lucide-react";
import { useSidebarStore } from "@/stores/sidebarStore";
import type { SidebarMessage } from "@/lib/types";
import { useFormStore, type DisplayMode } from "@/stores/formStore";
import { useRuntimeStore } from "@/stores/runtimeStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { SegmentedText } from "@/components/SegmentedText";
import * as api from "@/lib/tauri-api";

type WindowMode = "follow" | "independent";

const TITLE_BAR_H = 40;
const MSG_CARD_H = 62;
const MSG_GAP = 6;
const CONTAINER_PAD = 12;

function getCollapsedHeight(count: number): number {
  if (count <= 0) return TITLE_BAR_H;
  return TITLE_BAR_H + CONTAINER_PAD + count * MSG_CARD_H + (count - 1) * MSG_GAP;
}

function getWindowMode(): WindowMode {
  const params = new URLSearchParams(window.location.search);
  return params.get("mode") === "independent" ? "independent" : "follow";
}

const DISPLAY_MODES: { value: DisplayMode; label: string; title: string }[] = [
  { value: "translated", label: "译", title: "纯翻译" },
  { value: "original", label: "中", title: "纯中文" },
  { value: "bilingual", label: "双", title: "双语混排" },
];

const containerVariants = {
  visible: { opacity: 1, x: 0, scale: 1 },
  hidden: { opacity: 0, x: -24, scale: 0.96 },
};

const containerTransition = {
  type: "spring" as const,
  stiffness: 380,
  damping: 28,
  mass: 0.8,
};

const SENDER_COLORS = [
  "bg-blue-400",
  "bg-amber-400",
  "bg-rose-400",
  "bg-violet-400",
  "bg-teal-400",
  "bg-orange-400",
  "bg-pink-400",
  "bg-cyan-400",
];

function getSenderColor(name: string): string {
  let hash = 0;
  for (const ch of name) hash = ((hash << 5) - hash + ch.charCodeAt(0)) | 0;
  return SENDER_COLORS[Math.abs(hash) % SENDER_COLORS.length];
}

function getInitialSystemDarkMode(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function useSidebarWindowAppearance() {
  const [isDarkMode, setIsDarkMode] = useState<boolean>(getInitialSystemDarkMode);
  const [isWindowFocused, setIsWindowFocused] = useState<boolean>(() => {
    if (typeof document === "undefined") return true;
    return document.hasFocus();
  });

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");

    function onThemeChange(e: MediaQueryListEvent) {
      setIsDarkMode(e.matches);
    }

    function onFocus() {
      setIsWindowFocused(true);
    }

    function onBlur() {
      setIsWindowFocused(false);
    }

    setIsDarkMode(mq.matches);
    setIsWindowFocused(document.hasFocus());
    mq.addEventListener("change", onThemeChange);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);

    return () => {
      mq.removeEventListener("change", onThemeChange);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  return { isDarkMode, isWindowFocused };
}

function renderMessageCard(msg: SidebarMessage, displayMode: DisplayMode) {
  const hasSender = msg.sender !== "";
  const sender = msg.sender || msg.chatName;
  const color = getSenderColor(sender);

  const cardClass = msg.isSelf
    ? "sidebar-msg-card-self rounded-lg overflow-hidden transition-colors duration-100 max-w-[88%]"
    : "sidebar-msg-card rounded-lg overflow-hidden transition-colors duration-100";

  const wrapperClass = msg.isSelf ? "flex justify-end" : "";

  return (
    <motion.div
      key={msg.id}
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.15, ease: "easeOut" }}
      className={wrapperClass}
    >
      <div className={cardClass}>
        <div className="px-3 py-2">
          <div className="flex items-center justify-between mb-1">
            <div className="flex items-center gap-1 min-w-0">
              {msg.isSelf ? (
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0" />
              ) : hasSender ? (
                <>
                  <span className={`w-1.5 h-1.5 rounded-full ${color} shrink-0`} />
                </>
              ) : (
                <MessageCircle className="w-2.5 h-2.5 text-gray-400 dark:text-gray-500 shrink-0" />
              )}
              <span className="text-[10px] font-medium text-gray-500 dark:text-gray-400 truncate">
                {msg.isSelf ? "我" : hasSender ? sender : ""}
              </span>
            </div>
            <span className="text-[9px] text-gray-300 dark:text-gray-600 tabular-nums shrink-0 ml-2">
              {msg.timestamp.slice(11, 19)}
            </span>
          </div>
          {msg.imagePath ? (
            <div className="mt-1">
              <img
                src={convertFileSrc(msg.imagePath)}
                alt="chat image"
                className="max-w-full max-h-48 rounded-md object-contain bg-black/5 dark:bg-white/5"
                loading="lazy"
              />
            </div>
          ) : displayMode === "translated" ? (
            <SegmentedText
              text={(msg.textEn && msg.textEn !== msg.textCn) ? msg.textEn : msg.textCn}
              className="text-[12px] text-gray-800 dark:text-gray-200 leading-relaxed whitespace-pre-wrap break-words"
            />
          ) : displayMode === "original" ? (
            <p className="text-[12px] text-gray-800 dark:text-gray-200 leading-relaxed whitespace-pre-wrap break-words">
              {msg.textCn}
            </p>
          ) : (
            <>
              <p className="text-[12px] text-gray-800 dark:text-gray-200 leading-relaxed whitespace-pre-wrap break-words">
                {msg.textCn}
              </p>
              {msg.textEn && msg.textEn !== msg.textCn && (
                <div className="mt-1 pt-1 border-t border-dashed border-gray-200/60 dark:border-white/[0.06] flex gap-1.5 items-start">
                  <Languages className="w-3 h-3 text-sky-400/40 dark:text-sky-400/25 shrink-0 mt-0.5" />
                  <p className="text-[11px] text-sky-700/50 dark:text-sky-300/40 leading-relaxed whitespace-pre-wrap break-words">
                    {msg.textEn}
                  </p>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </motion.div>
  );
}

export function SidebarView() {
  const { isDarkMode, isWindowFocused } = useSidebarWindowAppearance();

  const [windowMode] = useState<WindowMode>(getWindowMode);
  const isIndependent = windowMode === "independent";

  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
  }, []);

  const items = useSidebarStore((s) => s.items);
  const currentChat = useSidebarStore((s) => s.currentChat);
  const refreshVersion = useSidebarStore((s) => s.refreshVersion);
  const hydrateSnapshot = useSidebarStore((s) => s.hydrateSnapshot);
  const displayMode = useFormStore((s) => s.displayMode);
  const setSettings = useFormStore((s) => s.setSettings);
  const settings = useSettingsStore((s) => s.settings);
  const translatorStatus = useRuntimeStore((s) => s.runtime.translator);
  const setTranslatorStatus = useRuntimeStore((s) => s.setTranslatorStatus);
  const collapsedCount = parseInt(useFormStore((s) => s.collapsedDisplayCount) || "0", 10);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(true);
  const [collapsed, setCollapsed] = useState(false);
  const [snapshotLoading, setSnapshotLoading] = useState(false);
  const expandedSizeRef = useRef({ w: 380, h: 600 });
  const translateEnabled = settings?.translate.enabled ?? false;
  const deeplxUrl = settings?.translate.deeplx_url ?? "";
  const canSwitchDisplayMode = !settings || (translateEnabled && deeplxUrl.trim() !== "");
  const effectiveDisplayMode: DisplayMode = canSwitchDisplayMode ? displayMode : "original";

  const fetchSnapshot = useCallback(async () => {
    setSnapshotLoading(true);
    try {
      const resp = await api.sidebarSnapshotGet({
        chatName: currentChat || undefined,
        limit: 50,
      });
      if (resp.data) {
        hydrateSnapshot(
          resp.data.current_chat ?? "",
          resp.data.messages ?? [],
          resp.data.refresh_version
        );
        setTranslatorStatus(resp.data.translator);
      }
    } catch {
      /* ignore */
    } finally {
      setSnapshotLoading(false);
    }
  }, [currentChat, hydrateSnapshot, setTranslatorStatus]);

  useEffect(() => {
    fetchSnapshot();
  }, [fetchSnapshot, refreshVersion]);

  useEffect(() => {
    if (isIndependent) return;
    const unlisten = listen<boolean>("sidebar-visibility", (e) => {
      setVisible(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [isIndependent]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [items.length]);

  function handleClose() {
    api.sidebarStop().catch(() => {});
  }

  async function handleToggleCollapse() {
    const win = getCurrentWindow();
    const scale = await win.scaleFactor();
    if (collapsed) {
      await win.setSize(new LogicalSize(expandedSizeRef.current.w, expandedSizeRef.current.h));
      setCollapsed(false);
    } else {
      const size = await win.innerSize();
      expandedSizeRef.current = { w: size.width / scale, h: size.height / scale };
      const targetHeight = getCollapsedHeight(collapsedCount);
      await win.setSize(new LogicalSize(size.width / scale, targetHeight));
      setCollapsed(true);
    }
  }

  return (
    <motion.div
      variants={containerVariants}
      initial="visible"
      animate={visible ? "visible" : "hidden"}
      transition={containerTransition}
      data-window-focus={isWindowFocused ? "true" : "false"}
      className={`${isDarkMode ? "dark " : ""}sidebar-shell flex flex-col h-screen overflow-hidden select-none transition-colors duration-200`}
    >
      {/* Title bar */}
      <div
        data-tauri-drag-region
        data-window-focus={isWindowFocused ? "true" : "false"}
        className="sidebar-titlebar flex items-center justify-between px-3 py-2 shrink-0"
      >
        <div data-tauri-drag-region className="flex items-center gap-1.5 flex-1 min-w-0">
          <MessageCircle className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400 shrink-0 pointer-events-none" />
          <span
            data-tauri-drag-region
            className="text-xs font-medium text-gray-700 dark:text-gray-300 truncate"
          >
            {currentChat || "等待聊天..."}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {canSwitchDisplayMode && (
            <div className="sidebar-mode-switch flex items-center h-5 rounded-md p-0.5">
              {DISPLAY_MODES.map((mode) => (
                <button
                  key={mode.value}
                  onClick={() => setSettings({ displayMode: mode.value })}
                  title={mode.title}
                  className={`px-1.5 h-4 rounded text-[9px] font-medium transition-all duration-150 ${
                    effectiveDisplayMode === mode.value
                      ? "sidebar-mode-button-active shadow-sm"
                      : "sidebar-mode-button"
                  }`}
                >
                  {mode.label}
                </button>
              ))}
            </div>
          )}
          {isIndependent && (
            <button
              onClick={handleToggleCollapse}
              className="w-6 h-6 flex items-center justify-center rounded-md hover:bg-black/[0.05] dark:hover:bg-white/[0.08] transition-colors"
              title={collapsed ? "展开" : "折叠"}
            >
              {collapsed ? (
                <ChevronDown className="w-3 h-3 text-gray-400 dark:text-gray-500" />
              ) : (
                <ChevronUp className="w-3 h-3 text-gray-400 dark:text-gray-500" />
              )}
            </button>
          )}
          <button
            onClick={handleClose}
            className="w-6 h-6 flex items-center justify-center rounded-md hover:bg-red-500/10 transition-colors group"
            title="关闭"
          >
            <X className="w-3 h-3 text-gray-400 dark:text-gray-500 group-hover:text-red-500" />
          </button>
        </div>
      </div>

      {/* Collapsed preview: show latest N messages */}
      {collapsed && collapsedCount > 0 && items.length > 0 && (
        <div className="px-2.5 pt-1 pb-2 overflow-hidden">
          <div className="flex flex-col gap-1.5">
            {items
              .slice(-collapsedCount)
              .map((msg) => renderMessageCard(msg, effectiveDisplayMode))}
          </div>
        </div>
      )}

      {/* Full message list (expanded) */}
      {!collapsed && (
        <div className="sidebar-scroll flex-1 overflow-y-auto px-2.5 pt-1 pb-4">
          {items.length === 0 && (
            <div className="flex flex-col items-center justify-center h-full gap-3">
              <div className="w-12 h-12 rounded-2xl bg-gradient-to-br from-emerald-100 to-sky-100 dark:from-emerald-900/40 dark:to-sky-900/40 flex items-center justify-center">
                <MessageCircle className="w-6 h-6 text-emerald-500/60 dark:text-emerald-400/50" />
              </div>
              <div className="text-center">
                <p className="text-xs text-gray-500 dark:text-gray-400 font-medium">
                  {snapshotLoading ? "加载消息中..." : "等待新消息..."}
                </p>
                <p className="text-[10px] text-gray-400/70 dark:text-gray-500/60 mt-1">
                  {snapshotLoading ? "正在从数据库同步当前聊天" : "切换微信聊天窗口即可开始"}
                </p>
              </div>
            </div>
          )}

          {settings && translateEnabled && !deeplxUrl.trim() && (
            <div className="flex items-center gap-1.5 px-3 py-1.5 mb-2 rounded-lg bg-amber-500/5 dark:bg-amber-500/5">
              <AlertCircle className="w-3 h-3 text-amber-500 shrink-0" />
              <span className="text-[10px] text-amber-700 dark:text-amber-300 italic truncate">
                翻译未配置，当前只显示原文
              </span>
            </div>
          )}

          {translatorStatus.last_error && (
            <div className="flex items-center gap-1.5 px-3 py-1.5 mb-2 rounded-lg bg-red-500/5 dark:bg-red-500/5">
              <AlertCircle className="w-3 h-3 text-red-500 shrink-0" />
              <span className="text-[10px] text-red-600 dark:text-red-300 italic truncate">
                {translatorStatus.last_error}
              </span>
            </div>
          )}

          <div className="flex flex-col gap-1.5">
            <AnimatePresence initial={false}>
              {items.map((msg) => renderMessageCard(msg, effectiveDisplayMode))}
            </AnimatePresence>
          </div>
          <div ref={bottomRef} />
        </div>
      )}
    </motion.div>
  );
}
