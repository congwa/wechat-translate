import { useEffect, useState, useCallback, useRef } from "react";
import { X, Volume2, Loader2 } from "lucide-react";
import { useDictionaryStore } from "@/stores/dictionaryStore";
import type { Definition, Meaning } from "@/lib/tauri-api";

interface DefinitionItemProps {
  definition: Definition;
  onTranslate: (text: string) => Promise<string | null>;
}

function DefinitionItem({ definition, onTranslate }: DefinitionItemProps) {
  const [chinese, setChinese] = useState(definition.chinese);
  const [exampleChinese, setExampleChinese] = useState(definition.example_chinese);
  const [translating, setTranslating] = useState(false);

  const handleTranslate = useCallback(async () => {
    if (chinese || translating) return;
    setTranslating(true);
    const result = await onTranslate(definition.english);
    if (result) setChinese(result);
    setTranslating(false);
  }, [chinese, translating, definition.english, onTranslate]);

  const handleTranslateExample = useCallback(async () => {
    if (!definition.example || exampleChinese || translating) return;
    setTranslating(true);
    const result = await onTranslate(definition.example);
    if (result) setExampleChinese(result);
    setTranslating(false);
  }, [definition.example, exampleChinese, translating, onTranslate]);

  useEffect(() => {
    handleTranslate();
  }, [handleTranslate]);

  return (
    <div className="py-1.5 border-b border-border last:border-b-0">
      <p className="text-xs text-foreground leading-relaxed">
        {definition.english}
      </p>
      {translating ? (
        <p className="text-[11px] text-primary/70 mt-0.5 flex items-center gap-1">
          <Loader2 className="w-3 h-3 animate-spin" />
          翻译中...
        </p>
      ) : chinese ? (
        <p className="text-[11px] text-primary mt-0.5">
          {chinese}
        </p>
      ) : null}
      {definition.example && (
        <p
          className="text-[11px] text-muted-foreground mt-1 italic cursor-pointer hover:text-foreground transition-colors"
          onClick={handleTranslateExample}
          title="点击翻译例句"
        >
          例: {definition.example}
        </p>
      )}
      {exampleChinese && (
        <p className="text-[11px] text-primary/70 mt-0.5">
          {exampleChinese}
        </p>
      )}
    </div>
  );
}

interface MeaningItemProps {
  meaning: Meaning;
  onTranslate: (text: string) => Promise<string | null>;
}

function MeaningItem({ meaning, onTranslate }: MeaningItemProps) {
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
            onTranslate={onTranslate}
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
    popoverPosition,
    clearCurrent,
    translateDefinition,
  } = useDictionaryStore();

  const popoverRef = useRef<HTMLDivElement>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playingRegion, setPlayingRegion] = useState<string | null>(null);

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

  const playAudio = useCallback((url: string, region: string) => {
    if (audioRef.current) {
      audioRef.current.pause();
    }
    audioRef.current = new Audio(url);
    setPlayingRegion(region);
    audioRef.current.onended = () => setPlayingRegion(null);
    audioRef.current.onerror = () => setPlayingRegion(null);
    audioRef.current.play().catch(() => setPlayingRegion(null));
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
      <div className="flex items-center justify-between px-3 py-2 border-b border-border" style={{ backgroundColor: "var(--color-muted)" }}>
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
          {phoneticsWithAudio.map((p) => (
            <button
              key={p.region || "default"}
              onClick={() => p.audio_url && playAudio(p.audio_url, p.region || "default")}
              className="p-1 rounded hover:bg-accent transition-colors"
              title={`播放 ${p.region?.toUpperCase() || ""} 发音`}
            >
              <Volume2
                className={`w-3.5 h-3.5 ${
                  playingRegion === (p.region || "default")
                    ? "text-primary animate-pulse"
                    : "text-muted-foreground"
                }`}
              />
            </button>
          ))}
          <button
            onClick={clearCurrent}
            className="p-1 rounded hover:bg-accent transition-colors ml-0.5"
          >
            <X className="w-3.5 h-3.5 text-muted-foreground" />
          </button>
        </div>
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
                  onTranslate={translateDefinition}
                />
              ))}
            </div>
          ) : null}
      </div>
    </div>
  );
}
