import { useRef, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { X, MessageCircle, Languages, AlertCircle, BookOpen, Users, User, Loader2, Volume2 } from "lucide-react";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { Skeleton } from "@/components/ui/skeleton";
import {
  DEFAULT_THEME_MODE,
  isThemeMode,
  useApplyThemeMode,
} from "@/lib/theme";
import type {
  SidebarMessage,
  SidebarAppearance,
  SettingsSnapshot,
  ThemeMode,
} from "@/lib/types";
import { useUiPreferencesStore, type DisplayMode } from "@/stores/uiPreferencesStore";
import { useRuntimeStore } from "@/stores/runtimeStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { SegmentedText } from "@/components/SegmentedText";
import { WordPopover } from "@/components/WordPopover";
import { WordBook } from "@/components/WordBook";
import { useSidebarSnapshot } from "@/features/sidebar/useSidebarSnapshot";
import * as api from "@/lib/tauri-api";

type WindowMode = "follow" | "independent";

function getWindowMode(): WindowMode {
  const params = new URLSearchParams(window.location.search);
  return params.get("mode") === "independent" ? "independent" : "follow";
}

function getDisplayCount(): number {
  const params = new URLSearchParams(window.location.search);
  const count = parseInt(params.get("count") || "3", 10);
  return Math.max(count, 1);
}

const DEFAULT_APPEARANCE: SidebarAppearance = {
  bg_opacity: 0.8,
  blur: "strong",
  card_style: "standard",
  text_enhance: "none",
};

function getAppearanceFromUrl(): SidebarAppearance {
  const params = new URLSearchParams(window.location.search);
  const b64 = params.get("appearance");
  if (b64) {
    try {
      return JSON.parse(atob(b64));
    } catch {
      return DEFAULT_APPEARANCE;
    }
  }
  return DEFAULT_APPEARANCE;
}

function getGhostModeFromUrl(): boolean {
  const params = new URLSearchParams(window.location.search);
  return params.get("ghost") === "true";
}

function getThemeModeFromUrl(): ThemeMode {
  const params = new URLSearchParams(window.location.search);
  const themeMode = params.get("theme");
  return isThemeMode(themeMode) ? themeMode : DEFAULT_THEME_MODE;
}

const BLUR_MAP: Record<string, string> = {
  none: "none",
  weak: "blur(8px) saturate(120%)",
  medium: "blur(14px) saturate(150%)",
  strong: "blur(20px) saturate(180%)",
};

const CARD_STYLE_MAP: Record<string, { bg: string; darkBg: string }> = {
  transparent: { bg: "rgba(255,255,255,0.1)", darkBg: "rgba(255,255,255,0.03)" },
  light: { bg: "rgba(245,246,248,0.5)", darkBg: "rgba(255,255,255,0.04)" },
  standard: { bg: "rgba(245,246,248,0.7)", darkBg: "rgba(255,255,255,0.05)" },
  dark: { bg: "rgba(230,232,236,0.9)", darkBg: "rgba(255,255,255,0.08)" },
};

function getTextEnhanceStyle(enhance: string): React.CSSProperties {
  switch (enhance) {
    case "shadow":
      return { textShadow: "0 0.5px 1px rgba(0,0,0,0.22)" };
    case "bold":
      return { fontWeight: 600 };
    default:
      return {};
  }
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

function hasCjkInText(text: string): boolean {
  return /[\u4E00-\u9FFF\u3400-\u4DBF\uF900-\uFAFF]/.test(text);
}

const BRACKET_WRAPPED_MESSAGE_RE = /^\[[^\[\]]*\]$/;
const PURE_NUMBER_MESSAGE_RE = /^\d+$/;
const LINK_TOKEN_RE = /^(?:(?:https?:\/\/|ftp:\/\/|file:\/\/|mailto:|www\.)\S+)$/i;

function normalizeMessageText(text: string): string {
  return text.trim();
}

function isBracketWrappedMessage(text: string): boolean {
  return BRACKET_WRAPPED_MESSAGE_RE.test(normalizeMessageText(text));
}

function isPureNumberMessage(text: string): boolean {
  return PURE_NUMBER_MESSAGE_RE.test(normalizeMessageText(text));
}

function isPureLinkMessage(text: string): boolean {
  const normalized = normalizeMessageText(text);
  if (!normalized) return false;
  return normalized.split(/\s+/).every((token) => LINK_TOKEN_RE.test(token));
}

function shouldSkipWordParsing(text: string): boolean {
  return isBracketWrappedMessage(text);
}

function shouldSkipAutoTts(text: string): boolean {
  return (
    isBracketWrappedMessage(text) ||
    isPureNumberMessage(text) ||
    isPureLinkMessage(text)
  );
}

/**
 * 根据显示模式选择朗读文本。
 * 返回 string → 立即朗读；返回 "pending" → 等翻译完成；返回 null → 跳过。
 */
function selectTtsText(
  textCn: string,
  textEn: string,
  displayMode: DisplayMode
): string | "pending" | null {
  if (!textCn.trim()) return null;
  if (!hasCjkInText(textCn)) return textCn; // 纯英文，直接朗读
  if (displayMode === "original") return textCn; // 原文模式，读中文
  // 译文/双语模式：有译文则读译文，否则等待
  if (textEn && textEn.trim() && textEn !== textCn) return textEn;
  return "pending";
}

function selectManualTtsText(
  msg: SidebarMessage,
  displayMode: DisplayMode
): string | null {
  if (msg.imagePath) return null;

  const textCn = normalizeMessageText(msg.textCn);
  const textEn = normalizeMessageText(msg.textEn);
  if ((!textCn && !textEn) || isBracketWrappedMessage(textCn)) return null;

  if (!hasCjkInText(textCn)) return textCn || textEn || null;
  if (displayMode === "original") return textCn || null;
  if (textEn && textEn !== textCn) return textEn;
  return textCn || null;
}

function useSidebarWindowAppearance(themeMode: ThemeMode) {
  const isDarkMode = useApplyThemeMode(themeMode);
  const [isWindowFocused, setIsWindowFocused] = useState<boolean>(() => {
    if (typeof document === "undefined") return true;
    return document.hasFocus();
  });

  useEffect(() => {
    function onFocus() {
      setIsWindowFocused(true);
    }

    function onBlur() {
      setIsWindowFocused(false);
    }

    setIsWindowFocused(document.hasFocus());
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);

    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  return { isDarkMode, isWindowFocused };
}

function renderMessageCard(
  msg: SidebarMessage,
  displayMode: DisplayMode,
  translatingIds: Set<number>,
  setTranslatingIds: React.Dispatch<React.SetStateAction<Set<number>>>,
  speakingMessageId: number | null
) {
  const hasSender = msg.sender !== "";
  const sender = msg.sender || msg.chatName;
  const color = getSenderColor(sender);
  const manualTtsText = selectManualTtsText(msg, displayMode);
  const isSpeaking = speakingMessageId === msg.id;
  const messageTextClass =
    "text-[12px] text-gray-800 dark:text-gray-200 leading-relaxed whitespace-pre-wrap break-words";

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
              ) : msg.chatType === "group" ? (
                <Users className="w-2.5 h-2.5 text-blue-400 dark:text-blue-500 shrink-0" />
              ) : msg.chatType === "private" ? (
                <User className="w-2.5 h-2.5 text-gray-400 dark:text-gray-500 shrink-0" />
              ) : (
                <MessageCircle className="w-2.5 h-2.5 text-gray-400 dark:text-gray-500 shrink-0" />
              )}
              <span className="text-[10px] font-medium text-gray-500 dark:text-gray-400 truncate">
                {msg.isSelf ? "我" : hasSender ? sender : ""}
              </span>
              {manualTtsText && (
                <TooltipProvider delayDuration={200}>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          api.ttsSpeak(msg.id, manualTtsText).catch(() => {});
                        }}
                        className="shrink-0 p-0.5 rounded hover:bg-gray-200/50 dark:hover:bg-white/10 transition-colors"
                        aria-label="朗读消息"
                      >
                        <Volume2
                          className={`w-3 h-3 ${
                            isSpeaking
                              ? "text-violet-500 dark:text-violet-400 animate-pulse"
                              : "text-gray-400 dark:text-gray-500"
                          }`}
                        />
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="top" className="text-xs">
                      {isSpeaking ? "朗读中..." : "朗读消息"}
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              )}
            </div>
            <div className="flex items-center gap-1 shrink-0 ml-2">
              <span className="text-[9px] text-gray-300 dark:text-gray-600 tabular-nums">
                {msg.timestamp.slice(11, 19)}
              </span>
            </div>
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
            // 译文模式：区分三种状态
            // 1. 已翻译：显示英文（可点词查词）
            // 2. 正在翻译中（5秒内的消息）：显示骨架屏，hover 可看中文
            // 3. 历史未翻译（超过5秒）：显示中文 + 翻译按钮
            (() => {
              if (isBracketWrappedMessage(msg.textCn)) {
                return (
                  <p className={messageTextClass}>
                    {msg.textCn}
                  </p>
                );
              }

              // 已翻译：直接显示英文
              if (msg.textEn && msg.textEn !== msg.textCn) {
                const skipWordParsing =
                  shouldSkipWordParsing(msg.textEn) || isBracketWrappedMessage(msg.textCn);
                return (
                  skipWordParsing ? (
                    <p className={messageTextClass}>
                      {msg.textEn}
                    </p>
                  ) : (
                    <SegmentedText
                      text={msg.textEn}
                      className={messageTextClass}
                    />
                  )
                );
              }
              
              // 判断是否是最近 5 秒内的消息（可能正在翻译中）
              const msgTime = new Date(msg.timestamp).getTime();
              const now = Date.now();
              const isRecent = (now - msgTime) < 5000;
              
              if (isRecent) {
                // 正在翻译中：显示骨架屏，hover 可看中文
                return (
                  <TooltipProvider delayDuration={200}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <div className="cursor-pointer hover:opacity-70 transition-opacity inline-block">
                          <Skeleton className="h-3 w-32" />
                          {msg.textCn.length > 20 && <Skeleton className="h-3 w-24 mt-1.5" />}
                        </div>
                      </TooltipTrigger>
                      <TooltipContent 
                        side="top" 
                        className="max-w-xs px-2 py-1.5 text-xs bg-zinc-800 text-zinc-100 border-zinc-700"
                      >
                        <p className="whitespace-pre-wrap">{msg.textCn}</p>
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                );
              }
              
              // 历史未翻译：显示中文 + 翻译按钮
              const isTranslating = translatingIds.has(msg.id);
              return (
                <div className="flex items-start gap-1.5">
                  <p className={`${messageTextClass} flex-1`}>
                    {msg.textCn}
                  </p>
                  <TooltipProvider delayDuration={200}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button 
                          className="shrink-0 mt-0.5 p-1 rounded hover:bg-gray-200/50 dark:hover:bg-white/10 transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
                          disabled={isTranslating}
                          onClick={async (e) => {
                            e.stopPropagation();
                            if (isTranslating) return;
                            console.log("[Sidebar] 点击翻译按钮", {
                              messageId: msg.id,
                              chatName: msg.chatName,
                              sender: msg.sender,
                              content: msg.textCn.substring(0, 50),
                            });
                            setTranslatingIds(prev => new Set(prev).add(msg.id));
                            try {
                              await api.translateSidebarMessage({
                                messageId: msg.id,
                                chatName: msg.chatName,
                                sender: msg.sender,
                                content: msg.textCn,
                                detectedAt: msg.timestamp,
                              });
                              console.log("[Sidebar] 翻译请求发送成功");
                            } catch (err) {
                              console.error("[Sidebar] 翻译请求失败", err);
                              setTranslatingIds(prev => {
                                const next = new Set(prev);
                                next.delete(msg.id);
                                return next;
                              });
                            }
                          }}
                        >
                          {isTranslating ? (
                            <Loader2 className="w-3 h-3 text-gray-400 dark:text-gray-500 animate-spin" />
                          ) : (
                            <Languages className="w-3 h-3 text-gray-400 dark:text-gray-500" />
                          )}
                        </button>
                      </TooltipTrigger>
                      <TooltipContent side="left" className="text-xs">
                        {isTranslating ? "翻译中..." : "点击翻译"}
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                </div>
              );
            })()
          ) : displayMode === "original" ? (
            <p className={messageTextClass}>
              {msg.textCn}
            </p>
          ) : (
            <>
              <p className={messageTextClass}>
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
  const [windowMode] = useState<WindowMode>(getWindowMode);
  const isIndependent = windowMode === "independent";
  const displayCount = isIndependent ? getDisplayCount() : 0;
  const [themeMode, setThemeMode] = useState<ThemeMode>(getThemeModeFromUrl);
  const { isDarkMode, isWindowFocused } = useSidebarWindowAppearance(themeMode);
  const [appearance, setAppearance] = useState<SidebarAppearance>(getAppearanceFromUrl);
  const [ghostMode, setGhostMode] = useState<boolean>(getGhostModeFromUrl);

  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
  }, []);

  // 监听配置变更事件，实时更新外观和隐身模式
  useEffect(() => {
    const unlisten = listen<SettingsSnapshot>("settings-updated", (e) => {
      if (e.payload.data.display?.sidebar_appearance) {
        setAppearance(e.payload.data.display.sidebar_appearance);
      }
      setThemeMode(e.payload.data.display?.theme_mode ?? DEFAULT_THEME_MODE);
      if (e.payload.data.display?.ghost_mode !== undefined) {
        setGhostMode(e.payload.data.display.ghost_mode);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const displayMode = useUiPreferencesStore((s) => s.displayMode);
  const setPreferences = useUiPreferencesStore((s) => s.setPreferences);
  const settings = useSettingsStore((s) => s.settings);
  const translatorStatus = useRuntimeStore((s) => s.runtime.translator);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(true);
  const [showWordBook, setShowWordBook] = useState(false);
  const [translatingIds, setTranslatingIds] = useState<Set<number>>(new Set());
  const [speakingMessageId, setSpeakingMessageId] = useState<number | null>(null);
  const { snapshot, snapshotLoading } = useSidebarSnapshot();
  const items = [...snapshot.messages].reverse().map((message) => ({
    id: message.id,
    chatName: message.chat_name,
    chatType: message.chat_type || undefined,
    sender: message.sender,
    textCn: message.content,
    textEn: message.content_en || "",
    translateError: "",
    timestamp: message.detected_at,
    isSelf: message.is_self,
    imagePath: message.image_path || undefined,
  }));
  const ttsEnabled = settings?.tts?.enabled ?? false;
  const currentChat = snapshot.current_chat ?? "";
  const translateEnabled = settings?.translate.enabled ?? false;
  const deeplxUrl = settings?.translate.deeplx_url ?? "";
  const canSwitchDisplayMode = !settings || (translateEnabled && deeplxUrl.trim() !== "");
  const effectiveDisplayMode: DisplayMode = canSwitchDisplayMode ? displayMode : "original";

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
    const unlistenBegin = listen<{ message_id: number }>("tts-utterance-begin", (e) => {
      setSpeakingMessageId(e.payload.message_id || null);
    });
    const unlistenEnd = listen<{ message_id: number }>("tts-utterance-end", () => {
      setSpeakingMessageId(null);
    });
    return () => {
      unlistenBegin.then((fn) => fn());
      unlistenEnd.then((fn) => fn());
    };
  }, []);

  const prevItemIdsRef = useRef<Set<number>>(new Set());
  const pendingTtsRef = useRef<Map<number, true>>(new Map());
  useEffect(() => {
    const currentIds = new Set(items.map((i) => i.id));
    if (!ttsEnabled) {
      prevItemIdsRef.current = currentIds;
      pendingTtsRef.current.clear();
      return;
    }
    const prev = prevItemIdsRef.current;
    prevItemIdsRef.current = currentIds;

    if (prev.size > 0) {
      const newItems = items.filter((i) => !prev.has(i.id) && !i.isSelf && !i.imagePath);
      for (const msg of newItems) {
        if (shouldSkipAutoTts(msg.textCn)) {
          continue;
        }
        const result = selectTtsText(msg.textCn, msg.textEn, effectiveDisplayMode);
        if (result === "pending") {
          pendingTtsRef.current.set(msg.id, true);
        } else if (result !== null) {
          api.ttsSpeak(msg.id, result).catch(() => {});
        }
      }
    }

    // 检查 pending 队列中已完成翻译的消息
    for (const [pendingId] of pendingTtsRef.current.entries()) {
      const msg = items.find((i) => i.id === pendingId);
      if (!msg) {
        pendingTtsRef.current.delete(pendingId);
        continue;
      }
      if (shouldSkipAutoTts(msg.textCn)) {
        pendingTtsRef.current.delete(pendingId);
        continue;
      }
      if (msg.textEn && msg.textEn.trim() && msg.textEn !== msg.textCn) {
        pendingTtsRef.current.delete(pendingId);
        api.ttsSpeak(msg.id, msg.textEn).catch(() => {});
        break; // 每次只朗读一条，避免连续多条积压时全部触发
      }
    }
  }, [items, ttsEnabled, effectiveDisplayMode]);

  // 滚动到底部：当消息数量变化或最后一条消息内容更新时触发（仅跟随模式）
  // 这样翻译完成后内容增多也会自动滚动到底部
  const lastItemContent = items.length > 0 ? items[items.length - 1]?.textEn : "";
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [items.length, lastItemContent]);

  function handleClose() {
    api.sidebarStop().catch(() => {});
  }

  const textStyle = getTextEnhanceStyle(appearance.text_enhance);
  // cardBg 和 CARD_STYLE_MAP 可用于未来消息卡片样式自定义
  void CARD_STYLE_MAP;

  return (
    <motion.div
      variants={containerVariants}
      initial="visible"
      animate={visible ? "visible" : "hidden"}
      transition={containerTransition}
      data-window-focus={isWindowFocused ? "true" : "false"}
      className="relative flex flex-col h-screen overflow-hidden select-none"
    >
      {/* 背景层 - 独立透明度控制 */}
      <div
        className="absolute inset-0 border border-[var(--sidebar-window-border)] rounded-lg"
        style={{
          opacity: appearance.bg_opacity,
          background: isDarkMode ? "rgba(18, 20, 26, 1)" : "rgba(255, 255, 255, 1)",
          backdropFilter: BLUR_MAP[appearance.blur],
          WebkitBackdropFilter: BLUR_MAP[appearance.blur],
        }}
      />

      {/* 内容层 - 始终不透明 */}
      <div className="relative z-10 flex flex-col h-full" style={textStyle}>
        {/* Title bar - 背景跟随窗口透明度 */}
        <div
          data-tauri-drag-region
          data-window-focus={isWindowFocused ? "true" : "false"}
          className="flex items-center justify-between px-3 py-2 shrink-0 border-b border-[var(--sidebar-window-border)]"
          style={ghostMode ? {} : {
            background: isDarkMode 
              ? `rgba(28, 30, 38, ${appearance.bg_opacity * 0.85})`
              : `rgba(255, 255, 255, ${appearance.bg_opacity * 0.85})`,
            backdropFilter: BLUR_MAP[appearance.blur],
            WebkitBackdropFilter: BLUR_MAP[appearance.blur],
          }}
        >
        <div data-tauri-drag-region className="flex items-center gap-1.5 flex-1 min-w-0">
          {(() => {
            const chatType = items[0]?.chatType;
            if (chatType === "group") {
              return <Users className="w-3.5 h-3.5 text-blue-500 dark:text-blue-400 shrink-0 pointer-events-none" />;
            } else if (chatType === "private") {
              return <User className="w-3.5 h-3.5 text-gray-500 dark:text-gray-400 shrink-0 pointer-events-none" />;
            }
            return <MessageCircle className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400 shrink-0 pointer-events-none" />;
          })()}
          <span
            data-tauri-drag-region
            className="text-xs font-medium text-gray-700 dark:text-gray-300 truncate"
          >
            {currentChat || "等待聊天..."}
          </span>
        </div>
        {/* 隐身模式下隐藏所有按钮 */}
        {!ghostMode && (
          <div className="flex items-center gap-1 shrink-0">
            {canSwitchDisplayMode && (
              <div className="sidebar-mode-switch flex items-center h-5 rounded-md p-0.5">
                {DISPLAY_MODES.map((mode) => (
                  <button
                    key={mode.value}
                    onClick={() => setPreferences({ displayMode: mode.value })}
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
            {/* 单词本按钮 - 仅跟随模式显示 */}
            {!isIndependent && (
              <button
                onClick={() => setShowWordBook(!showWordBook)}
                className={`w-6 h-6 flex items-center justify-center rounded-md transition-colors ${
                  showWordBook
                    ? "bg-primary/10 text-primary"
                    : "hover:bg-black/[0.05] dark:hover:bg-white/[0.08]"
                }`}
                title="单词本"
              >
                <BookOpen className={`w-3 h-3 ${showWordBook ? "text-primary" : "text-gray-400 dark:text-gray-500"}`} />
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
        )}
      </div>

      {/* 独立模式：显示最新 N 条消息 */}
      {isIndependent && items.length > 0 && (
        <div className="px-2.5 pt-1 pb-2 overflow-hidden">
          <div className="flex flex-col gap-1.5">
            {items
              .slice(-displayCount)
              .map((msg) => renderMessageCard(msg, effectiveDisplayMode, translatingIds, setTranslatingIds, speakingMessageId))}
          </div>
        </div>
      )}

      {/* 单词本视图 - 仅跟随模式 */}
      {!isIndependent && showWordBook && (
        <div className="flex-1 overflow-hidden bg-background">
          <WordBook />
        </div>
      )}

      {/* 跟随模式：完整消息列表 */}
      {!isIndependent && !showWordBook && (
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
              {items.map((msg) => renderMessageCard(msg, effectiveDisplayMode, translatingIds, setTranslatingIds, speakingMessageId))}
            </AnimatePresence>
          </div>
          <div ref={bottomRef} />
        </div>
      )}
      
      {/* 单词查词弹窗 */}
      <WordPopover />
      </div>
    </motion.div>
  );
}
