import { useRef, useState } from "react";
import html2canvas from "html2canvas";
import { save } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";
import { writeImage } from "@tauri-apps/plugin-clipboard-manager";
import { Image as TauriImage } from "@tauri-apps/api/image";
import { Copy, Download, Check, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";

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
        const arrayBuffer = await blob.arrayBuffer();
        const image = await TauriImage.fromBytes(new Uint8Array(arrayBuffer));
        await writeImage(image);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      } catch (err) {
        console.error("Clipboard write failed, falling back to save dialog", err);
        await saveWithTauriDialog(blob);
      }
    } catch (err) {
      console.error("Copy failed:", err);
    } finally {
      setCopying(false);
    }
  }

  async function saveWithTauriDialog(blob: Blob) {
    const arrayBuffer = await blob.arrayBuffer();
    const uint8Array = new Uint8Array(arrayBuffer);

    const filePath = await save({
      defaultPath: `summary-${Date.now()}.png`,
      filters: [{ name: "PNG Image", extensions: ["png"] }],
    });

    if (filePath) {
      await writeFile(filePath, uint8Array);
      return true;
    }
    return false;
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
        await saveWithTauriDialog(blob);
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

      {/* Summary card for export - using inline hex colors for html2canvas compatibility */}
      <div
        ref={cardRef}
        style={{
          borderRadius: "16px",
          background: "linear-gradient(to bottom right, #f8fafc, #ffffff, #eff6ff)",
          padding: "24px",
          boxShadow: "0 10px 15px -3px rgba(0,0,0,0.1)",
          border: "1px solid rgba(226,232,240,0.6)",
        }}
      >
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: "20px" }}>
          <div>
            <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
              <div style={{
                width: "32px",
                height: "32px",
                borderRadius: "12px",
                background: "linear-gradient(to bottom right, #3b82f6, #4f46e5)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}>
                <Sparkles style={{ width: "16px", height: "16px", color: "#ffffff" }} />
              </div>
              <h2 style={{ fontSize: "18px", fontWeight: "bold", color: "#1e293b", margin: 0 }}>{title}</h2>
            </div>
            {subtitle && (
              <p style={{ fontSize: "14px", color: "#64748b", paddingLeft: "40px", margin: "6px 0 0 0" }}>{subtitle}</p>
            )}
          </div>
          <div>
            <span style={{
              display: "inline-block",
              padding: "4px 12px",
              borderRadius: "9999px",
              backgroundColor: "#dbeafe",
              color: "#1d4ed8",
              fontSize: "12px",
              fontWeight: "500",
            }}>
              {dateRange}
            </span>
          </div>
        </div>

        {/* Stats */}
        <div style={{ display: "flex", flexWrap: "wrap", gap: "12px", marginBottom: "20px" }}>
          <div style={{
            padding: "8px 12px",
            borderRadius: "12px",
            backgroundColor: "rgba(255,255,255,0.8)",
            border: "1px solid rgba(226,232,240,0.6)",
            boxShadow: "0 1px 2px rgba(0,0,0,0.05)",
          }}>
            <p style={{ fontSize: "12px", color: "#64748b", margin: 0 }}>消息数</p>
            <p style={{ fontSize: "18px", fontWeight: "bold", color: "#1e293b", margin: 0 }}>{messageCount}</p>
          </div>
          {extraStats?.map((stat) => (
            <div
              key={stat.label}
              style={{
                padding: "8px 12px",
                borderRadius: "12px",
                backgroundColor: "rgba(255,255,255,0.8)",
                border: "1px solid rgba(226,232,240,0.6)",
                boxShadow: "0 1px 2px rgba(0,0,0,0.05)",
              }}
            >
              <p style={{ fontSize: "12px", color: "#64748b", margin: 0 }}>{stat.label}</p>
              <p style={{ fontSize: "18px", fontWeight: "bold", color: "#1e293b", margin: 0 }}>{stat.value}</p>
            </div>
          ))}
        </div>

        {/* Overall summary */}
        <div style={{
          borderRadius: "12px",
          background: "linear-gradient(to right, rgba(59,130,246,0.1), rgba(99,102,241,0.1))",
          padding: "16px",
          marginBottom: "16px",
          border: "1px solid rgba(191,219,254,0.5)",
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "8px" }}>
            <Sparkles style={{ width: "16px", height: "16px", color: "#2563eb" }} />
            <h3 style={{ fontSize: "14px", fontWeight: "600", color: "#1e40af", margin: 0 }}>整体总览</h3>
          </div>
          <div style={{ whiteSpace: "pre-wrap", fontSize: "14px", lineHeight: "1.6", color: "#334155" }}>
            {overallSummary}
          </div>
        </div>

        {/* Daily items */}
        {dailyItems.length > 0 && (
          <div>
            <h3 style={{ fontSize: "14px", fontWeight: "600", color: "#475569", display: "flex", alignItems: "center", gap: "8px", margin: "0 0 12px 0" }}>
              <span style={{ width: "4px", height: "16px", borderRadius: "9999px", background: "linear-gradient(to bottom, #3b82f6, #4f46e5)" }} />
              每日详情
            </h3>
            {dailyItems.map((item) => (
              <div
                key={item.date}
                style={{
                  borderRadius: "12px",
                  backgroundColor: "rgba(255,255,255,0.6)",
                  padding: "16px",
                  border: "1px solid rgba(226,232,240,0.5)",
                  boxShadow: "0 1px 2px rgba(0,0,0,0.05)",
                  marginBottom: "12px",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "8px" }}>
                  <span style={{ fontSize: "14px", fontWeight: "500", color: "#334155" }}>
                    {item.date}
                  </span>
                  <span style={{
                    fontSize: "10px",
                    padding: "2px 8px",
                    borderRadius: "9999px",
                    border: "1px solid #cbd5e1",
                    color: "#64748b",
                  }}>
                    {item.message_count} 条
                  </span>
                </div>
                <div style={{ whiteSpace: "pre-wrap", fontSize: "14px", lineHeight: "1.6", color: "#475569" }}>
                  {item.summary}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Footer */}
        <div style={{
          marginTop: "20px",
          paddingTop: "16px",
          borderTop: "1px solid rgba(226,232,240,0.6)",
          display: "flex",
          justifyContent: "space-between",
          fontSize: "10px",
          color: "#94a3b8",
        }}>
          <span>由 AI 自动生成</span>
          <span>WeChat Translate</span>
        </div>
      </div>
    </div>
  );
}
