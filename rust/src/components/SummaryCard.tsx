import { useRef, useState } from "react";
import html2canvas from "html2canvas";
import { Copy, Download, Check, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

interface DailyItem {
  date: string;
  message_count: number;
  summary: string;
}

interface SummaryCardProps {
  title: string;
  subtitle?: string;
  dateRange: string;
  messageCount: number;
  extraStats?: { label: string; value: string | number }[];
  overallSummary: string;
  dailyItems: DailyItem[];
}

export function SummaryCard({
  title,
  subtitle,
  dateRange,
  messageCount,
  extraStats,
  overallSummary,
  dailyItems,
}: SummaryCardProps) {
  const cardRef = useRef<HTMLDivElement>(null);
  const [copying, setCopying] = useState(false);
  const [copied, setCopied] = useState(false);
  const [exporting, setExporting] = useState(false);

  async function captureCard(): Promise<HTMLCanvasElement | null> {
    if (!cardRef.current) return null;

    const canvas = await html2canvas(cardRef.current, {
      backgroundColor: "#ffffff",
      scale: 2,
      useCORS: true,
      logging: false,
    });

    return canvas;
  }

  async function handleCopy() {
    if (copying) return;
    setCopying(true);
    try {
      const canvas = await captureCard();
      if (!canvas) {
        console.error("Failed to capture card");
        setCopying(false);
        return;
      }

      const blob = await new Promise<Blob | null>((resolve) => {
        canvas.toBlob((b) => resolve(b), "image/png");
      });

      if (!blob) {
        console.error("Failed to create blob");
        setCopying(false);
        return;
      }

      try {
        await navigator.clipboard.write([
          new ClipboardItem({ "image/png": blob }),
        ]);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      } catch (err) {
        console.error("Clipboard write failed, falling back to download", err);
        downloadBlob(blob);
      }
    } catch (err) {
      console.error("Copy failed:", err);
    } finally {
      setCopying(false);
    }
  }

  function downloadBlob(blob: Blob) {
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.download = `summary-${Date.now()}.png`;
    link.href = url;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }

  async function handleExport() {
    if (exporting) return;
    setExporting(true);
    try {
      const canvas = await captureCard();
      if (!canvas) {
        console.error("Failed to capture card for export");
        setExporting(false);
        return;
      }

      const blob = await new Promise<Blob | null>((resolve) => {
        canvas.toBlob((b) => resolve(b), "image/png");
      });

      if (blob) {
        downloadBlob(blob);
      }
    } catch (err) {
      console.error("Export failed:", err);
    } finally {
      setExporting(false);
    }
  }

  if (messageCount === 0) {
    return (
      <div className="rounded-2xl border border-dashed border-border px-6 py-8 text-center text-sm text-muted-foreground">
        当前时间范围内暂无可总结的消息。
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Action buttons */}
      <div className="flex items-center justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={handleCopy}
          disabled={copying}
          className="gap-1.5"
        >
          {copied ? (
            <>
              <Check className="w-3.5 h-3.5 text-green-500" />
              已复制
            </>
          ) : (
            <>
              <Copy className="w-3.5 h-3.5" />
              复制图片
            </>
          )}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={handleExport}
          disabled={exporting}
          className="gap-1.5"
        >
          <Download className="w-3.5 h-3.5" />
          导出图片
        </Button>
      </div>

      {/* Summary card for export */}
      <div
        ref={cardRef}
        className="rounded-2xl bg-gradient-to-br from-slate-50 via-white to-blue-50 p-6 shadow-lg border border-slate-200/60"
      >
        {/* Header */}
        <div className="flex items-start justify-between mb-5">
          <div className="space-y-1.5">
            <div className="flex items-center gap-2">
              <div className="w-8 h-8 rounded-xl bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center">
                <Sparkles className="w-4 h-4 text-white" />
              </div>
              <h2 className="text-lg font-bold text-slate-800">{title}</h2>
            </div>
            {subtitle && (
              <p className="text-sm text-slate-500 pl-10">{subtitle}</p>
            )}
          </div>
          <div className="text-right">
            <Badge
              variant="secondary"
              className="bg-blue-100 text-blue-700 border-0 font-medium"
            >
              {dateRange}
            </Badge>
          </div>
        </div>

        {/* Stats */}
        <div className="flex flex-wrap gap-3 mb-5">
          <div className="px-3 py-2 rounded-xl bg-white/80 border border-slate-200/60 shadow-sm">
            <p className="text-xs text-slate-500">消息数</p>
            <p className="text-lg font-bold text-slate-800">{messageCount}</p>
          </div>
          {extraStats?.map((stat) => (
            <div
              key={stat.label}
              className="px-3 py-2 rounded-xl bg-white/80 border border-slate-200/60 shadow-sm"
            >
              <p className="text-xs text-slate-500">{stat.label}</p>
              <p className="text-lg font-bold text-slate-800">{stat.value}</p>
            </div>
          ))}
        </div>

        {/* Overall summary */}
        <div className="rounded-xl bg-gradient-to-r from-blue-500/10 to-indigo-500/10 p-4 mb-4 border border-blue-200/50">
          <div className="flex items-center gap-2 mb-2">
            <Sparkles className="w-4 h-4 text-blue-600" />
            <h3 className="text-sm font-semibold text-blue-800">整体总览</h3>
          </div>
          <div className="whitespace-pre-wrap text-sm leading-relaxed text-slate-700">
            {overallSummary}
          </div>
        </div>

        {/* Daily items */}
        {dailyItems.length > 0 && (
          <div className="space-y-3">
            <h3 className="text-sm font-semibold text-slate-600 flex items-center gap-2">
              <span className="w-1 h-4 rounded-full bg-gradient-to-b from-blue-500 to-indigo-500" />
              每日详情
            </h3>
            {dailyItems.map((item) => (
              <div
                key={item.date}
                className="rounded-xl bg-white/60 p-4 border border-slate-200/50 shadow-sm"
              >
                <div className="flex items-center gap-2 mb-2">
                  <span className="text-sm font-medium text-slate-700">
                    {item.date}
                  </span>
                  <Badge
                    variant="outline"
                    className="text-[10px] h-5 border-slate-300 text-slate-500"
                  >
                    {item.message_count} 条
                  </Badge>
                </div>
                <div className="whitespace-pre-wrap text-sm leading-relaxed text-slate-600">
                  {item.summary}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Footer */}
        <div className="mt-5 pt-4 border-t border-slate-200/60 flex items-center justify-between text-[10px] text-slate-400">
          <span>由 AI 自动生成</span>
          <span>WeChat Translate</span>
        </div>
      </div>
    </div>
  );
}
