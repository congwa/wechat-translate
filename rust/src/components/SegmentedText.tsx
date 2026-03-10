import { useState, useMemo, useCallback } from "react";
import { motion } from "framer-motion";
import { useWordSegmenter, isSegmenterSupported } from "@/hooks/useWordSegmenter";

interface WordSpanProps {
  word: string;
  isActive: boolean;
  onClick: () => void;
}

function WordSpan({ word, isActive, onClick }: WordSpanProps) {
  return (
    <motion.span
      className={`
        cursor-pointer select-none
        transition-colors duration-150
        hover:text-sky-600 dark:hover:text-sky-400
        hover:underline hover:underline-offset-2 hover:decoration-sky-400/60
        ${isActive ? "text-sky-600 dark:text-sky-400 underline underline-offset-2 decoration-sky-500" : ""}
      `}
      whileTap={{ scale: 0.96 }}
      transition={{ type: "spring", stiffness: 400, damping: 25 }}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      {word}
    </motion.span>
  );
}

export interface SegmentedTextProps {
  text: string;
  className?: string;
  onWordClick?: (word: string, index: number) => void;
}

export function SegmentedText({ text, className, onWordClick }: SegmentedTextProps) {
  const { segment, supported } = useWordSegmenter("en");
  const segments = useMemo(() => segment(text), [text, segment]);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

  const handleWordClick = useCallback(
    (word: string, segIndex: number, arrayIndex: number) => {
      setActiveIndex(arrayIndex);
      onWordClick?.(word, segIndex);
    },
    [onWordClick]
  );

  const handleContainerClick = useCallback(() => {
    setActiveIndex(null);
  }, []);

  // 不支持分词时，直接渲染纯文本
  if (!supported) {
    return (
      <span className={className}>
        {text}
      </span>
    );
  }

  return (
    <span className={className} onClick={handleContainerClick}>
      {segments.map((seg, i) =>
        seg.isWord ? (
          <WordSpan
            key={`${i}-${seg.index}`}
            word={seg.text}
            isActive={activeIndex === i}
            onClick={() => handleWordClick(seg.text, seg.index, i)}
          />
        ) : (
          <span key={`${i}-${seg.index}`}>{seg.text}</span>
        )
      )}
    </span>
  );
}

export { isSegmenterSupported };
