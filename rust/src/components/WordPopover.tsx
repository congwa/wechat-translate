import { useEffect, useState, useCallback, useRef } from "react";
import { X, Volume2, Loader2, Star } from "lucide-react";
import { useDictionaryStore } from "@/stores/dictionaryStore";
import { useFavoriteStore } from "@/stores/favoriteStore";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import * as api from "@/lib/tauri-api";
import type { Definition, Meaning } from "@/lib/tauri-api";

// 发音地区显示名称映射
const REGION_LABELS: Record<string, string> = {
  uk: "🇬🇧 英式发音",
  us: "🇺🇸 美式发音",
  au: "🇦🇺 澳式发音",
  default: "🔊 播放发音",
};

interface DefinitionItemProps {
  definition: Definition;
  translating: boolean;
}

function DefinitionItem({ definition, translating }: DefinitionItemProps) {
  return (
    <div className="py-1.5 border-b border-border last:border-b-0">
      {/* 中文释义为主 */}
      {definition.chinese ? (
        <p className="text-xs text-foreground leading-relaxed">
          {definition.chinese}
        </p>
      ) : translating ? (
        <Skeleton className="h-4 w-3/4" />
      ) : (
        <p className="text-xs text-foreground leading-relaxed">
          {definition.english}
        </p>
      )}
      {/* 英文例句 */}
      {definition.example && (
        <p className="text-[11px] text-muted-foreground mt-1 italic">
          例: {definition.example}
        </p>
      )}
      {/* 例句中文翻译 */}
      {definition.example && (
        definition.example_chinese ? (
          <p className="text-[11px] text-primary/70 mt-0.5">
            {definition.example_chinese}
          </p>
        ) : translating ? (
          <Skeleton className="h-3 w-2/3 mt-0.5" />
        ) : null
      )}
    </div>
  );
}

interface MeaningItemProps {
  meaning: Meaning;
  translating: boolean;
}

function MeaningItem({ meaning, translating }: MeaningItemProps) {
  return (
    <div className="mb-3 last:mb-0">
      <div className="flex items-center gap-1.5 mb-1">
        <span className="text-[10px] font-medium text-primary-foreground bg-primary px-1.5 py-0.5 rounded">
          {meaning.part_of_speech}
        </span>
        <span className="text-[10px] text-muted-foreground">
          {meaning.part_of_speech_zh}
        </span>
      </div>
      <div className="pl-2 border-l-2 border-primary/30">
        {meaning.definitions.slice(0, 3).map((def, i) => (
          <DefinitionItem
            key={i}
            definition={def}
            translating={translating}
          />
        ))}
        {meaning.definitions.length > 3 && (
          <p className="text-[10px] text-muted-foreground py-1">
            +{meaning.definitions.length - 3} 更多释义
          </p>
        )}
      </div>
      {meaning.synonyms.length > 0 && (
        <p className="text-[10px] text-muted-foreground mt-1">
          <span className="opacity-70">同义词:</span> {meaning.synonyms.slice(0, 5).join(", ")}
        </p>
      )}
    </div>
  );
}

export function WordPopover() {
  const {
    currentWord,
    currentEntry,
    loading,
    error,
    translating,
    popoverPosition,
    clearCurrent,
  } = useDictionaryStore();

  const { checkFavorite, toggleFavorite, getCachedStatus } = useFavoriteStore();

  const popoverRef = useRef<HTMLDivElement>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playingRegion, setPlayingRegion] = useState<string | null>(null);
  const [isFavorited, setIsFavorited] = useState(false);
  const [favoriteLoading, setFavoriteLoading] = useState(false);

  // 检查收藏状态
  useEffect(() => {
    if (currentWord) {
      const cached = getCachedStatus(currentWord);
      if (cached !== undefined) {
        setIsFavorited(cached);
      } else {
        checkFavorite(currentWord).then(setIsFavorited);
      }
    }
  }, [currentWord, checkFavorite, getCachedStatus]);

  const handleToggleFavorite = useCallback(async () => {
    if (!currentWord || favoriteLoading) return;
    setFavoriteLoading(true);
    try {
      const newStatus = await toggleFavorite(currentWord, currentEntry ?? undefined);
      setIsFavorited(newStatus);
    } finally {
      setFavoriteLoading(false);
    }
  }, [currentWord, currentEntry, favoriteLoading, toggleFavorite]);

  // 点击外部关闭
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        clearCurrent();
      }
    }
    if (currentWord) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [currentWord, clearCurrent]);

  // ESC 关闭
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        clearCurrent();
      }
    }
    if (currentWord) {
      document.addEventListener("keydown", handleKeyDown);
      return () => document.removeEventListener("keydown", handleKeyDown);
    }
  }, [currentWord, clearCurrent]);

  // 播放音频（使用缓存）
  const playAudio = useCallback(async (url: string, region: string) => {
    if (audioRef.current) {
      audioRef.current.pause();
    }
    setPlayingRegion(region);
    
    try {
      // 通过 Tauri 获取缓存后的本地 URL
      const localUrl = await api.audioGetUrl(url);
      
      audioRef.current = new Audio(localUrl);
      audioRef.current.onended = () => setPlayingRegion(null);
      audioRef.current.onerror = () => setPlayingRegion(null);
      audioRef.current.play().catch(() => setPlayingRegion(null));
    } catch (error) {
      console.error("播放音频失败:", error);
      setPlayingRegion(null);
    }
  }, []);

  if (!currentWord || !popoverPosition) return null;

  const phoneticsWithAudio = currentEntry?.phonetics.filter(p => p.audio_url) ?? [];
  const phoneticText = currentEntry?.phonetics.find(p => p.text)?.text;

  // 计算弹窗位置，避免超出屏幕边界
  const popoverWidth = 288;
  const popoverMaxHeight = 320;
  const padding = 8;
  
  let left = popoverPosition.x;
  let top = popoverPosition.y + 8;
  
  // 右边界检测
  if (left + popoverWidth > window.innerWidth - padding) {
    left = window.innerWidth - popoverWidth - padding;
  }
  // 左边界检测
  if (left < padding) {
    left = padding;
  }
  // 下边界检测（如果下方空间不足，显示在上方）
  if (top + popoverMaxHeight > window.innerHeight - padding) {
    top = popoverPosition.y - popoverMaxHeight - 8;
  }
  // 上边界检测
  if (top < padding) {
    top = padding;
  }

  return (
    <div
      ref={popoverRef}
      className="fixed z-[9999] w-72 rounded-lg border border-border shadow-xl overflow-hidden"
      style={{
        left,
        top,
        backgroundColor: "var(--color-popover)",
        color: "var(--color-popover-foreground)",
        animation: "popover-in 150ms ease-out",
      }}
    >
      <style>{`
        @keyframes popover-in {
          from {
            opacity: 0;
            transform: scale(0.96) translateY(-4px);
          }
          to {
            opacity: 1;
            transform: scale(1) translateY(0);
          }
        }
      `}</style>
      {/* Header */}
      <div className="px-3 py-2 border-b border-border" style={{ backgroundColor: "var(--color-muted)" }}>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-sm font-semibold truncate">
              {currentWord}
            </span>
            {phoneticText && (
              <span className="text-xs text-muted-foreground shrink-0">
                {phoneticText}
              </span>
            )}
          </div>
          <div className="flex items-center gap-0.5 shrink-0">
            {/* 收藏按钮 */}
            <button
              onClick={handleToggleFavorite}
              disabled={favoriteLoading}
              className="p-1 rounded hover:bg-accent transition-colors"
              title={isFavorited ? "取消收藏" : "收藏单词"}
            >
              <Star
                className={`w-3.5 h-3.5 transition-colors ${
                  favoriteLoading
                    ? "text-muted-foreground animate-pulse"
                    : isFavorited
                    ? "fill-yellow-400 text-yellow-400"
                    : "text-muted-foreground hover:text-yellow-400"
                }`}
              />
            </button>
            <TooltipProvider delayDuration={200}>
              {phoneticsWithAudio.map((p) => (
                <Tooltip key={p.region || "default"}>
                  <TooltipTrigger asChild>
                    <button
                      onClick={() => p.audio_url && playAudio(p.audio_url, p.region || "default")}
                      className="p-1 rounded hover:bg-accent transition-colors"
                    >
                      <Volume2
                        className={`w-3.5 h-3.5 ${
                          playingRegion === (p.region || "default")
                            ? "text-primary animate-pulse"
                            : "text-muted-foreground"
                        }`}
                      />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="text-xs">
                    {REGION_LABELS[p.region || "default"] || `${p.region?.toUpperCase()} 发音`}
                  </TooltipContent>
                </Tooltip>
              ))}
            </TooltipProvider>
            <button
              onClick={clearCurrent}
              className="p-1 rounded hover:bg-accent transition-colors ml-0.5"
            >
              <X className="w-3.5 h-3.5 text-muted-foreground" />
            </button>
          </div>
        </div>
        {/* 中文总释义 */}
        {currentEntry?.summary_zh ? (
          <p className="text-xs text-primary mt-1.5 font-medium">
            {currentEntry.summary_zh}
          </p>
        ) : translating ? (
          <Skeleton className="h-4 w-1/2 mt-1.5" />
        ) : null}
      </div>

      {/* Content */}
      <div className="overflow-y-auto max-h-60 p-3">
          {loading ? (
            <div className="flex items-center justify-center py-6">
              <Loader2 className="w-5 h-5 text-primary animate-spin" />
              <span className="ml-2 text-sm text-muted-foreground">查询中...</span>
            </div>
          ) : error ? (
            <div className="text-center py-4">
              <p className="text-sm text-destructive">{error}</p>
            </div>
          ) : currentEntry ? (
            <div>
              {currentEntry.meanings.map((meaning, i) => (
                <MeaningItem
                  key={i}
                  meaning={meaning}
                  translating={translating}
                />
              ))}
            </div>
          ) : null}
      </div>
    </div>
  );
}
