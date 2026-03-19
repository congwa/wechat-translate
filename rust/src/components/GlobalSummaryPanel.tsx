import { useEffect, useMemo, useState } from "react";
import {
  CalendarRange,
  Globe,
  Loader2,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SummaryCard } from "@/components/SummaryCard";
import * as api from "@/lib/tauri-api";
import type { GlobalSummaryResult, SummaryLanguage } from "@/lib/tauri-api";
import { useSettingsStore } from "@/stores/settingsStore";

type SummaryPreset = "today" | "3d" | "7d" | "custom";

const LANGUAGE_OPTIONS: { value: SummaryLanguage; label: string }[] = [
  { value: "en", label: "English" },
  { value: "zh", label: "中文" },
  { value: "bilingual", label: "双语" },
];

function formatDateInput(date: Date) {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function resolvePresetRange(preset: Exclude<SummaryPreset, "custom">) {
  const end = new Date();
  const start = new Date();
  const diff = preset === "today" ? 0 : preset === "3d" ? 2 : 6;
  start.setDate(end.getDate() - diff);
  return {
    startDate: formatDateInput(start),
    endDate: formatDateInput(end),
  };
}

function inclusiveDayDiff(startDate: string, endDate: string) {
  const start = new Date(`${startDate}T00:00:00`);
  const end = new Date(`${endDate}T00:00:00`);
  if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) {
    return null;
  }
  return Math.floor((end.getTime() - start.getTime()) / 86_400_000) + 1;
}

function isAiSummaryAvailable(
  settings: ReturnType<typeof useSettingsStore.getState>["settings"],
) {
  if (!settings || settings.translate.provider !== "ai") {
    return false;
  }

  const hasModel = settings.translate.ai_model_id.trim().length > 0;
  const hasKey = settings.translate.ai_api_key.trim().length > 0;
  const hasProvider = settings.translate.ai_provider_id.trim().length > 0;
  const hasBaseUrl = settings.translate.ai_base_url.trim().length > 0;

  return hasModel && hasKey && (hasProvider || hasBaseUrl);
}

export function GlobalSummaryPanel() {
  const settings = useSettingsStore((s) => s.settings);
  const aiReady = useMemo(() => isAiSummaryAvailable(settings), [settings]);

  const initialRange = useMemo(() => resolvePresetRange("today"), []);
  const [summaryPreset, setSummaryPreset] = useState<SummaryPreset>("today");
  const [summaryStartDate, setSummaryStartDate] = useState(initialRange.startDate);
  const [summaryEndDate, setSummaryEndDate] = useState(initialRange.endDate);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState("");
  const [summaryResult, setSummaryResult] = useState<GlobalSummaryResult | null>(null);
  const [summaryLanguage, setSummaryLanguage] = useState<SummaryLanguage>("en");

  const daySpan = useMemo(
    () => inclusiveDayDiff(summaryStartDate, summaryEndDate),
    [summaryStartDate, summaryEndDate],
  );

  useEffect(() => {
    if (summaryPreset === "custom") return;
    const range = resolvePresetRange(summaryPreset);
    setSummaryStartDate(range.startDate);
    setSummaryEndDate(range.endDate);
  }, [summaryPreset]);

  useEffect(() => {
    setSummaryError("");
    setSummaryResult(null);
  }, [summaryStartDate, summaryEndDate]);

  function handleCustomDateChange(
    setter: (value: string) => void,
    value: string,
  ) {
    setSummaryPreset("custom");
    setter(value);
  }

  async function handleGenerateSummary() {
    if (!aiReady) {
      setSummaryError("总结功能需要在设置页启用 AI 翻译配置");
      return;
    }

    if (!summaryStartDate || !summaryEndDate || daySpan === null || daySpan <= 0) {
      setSummaryError("请选择有效的日期范围");
      return;
    }

    if (daySpan > 14) {
      setSummaryError("自定义日期范围最多支持 14 天");
      return;
    }

    setSummaryLoading(true);
    setSummaryError("");

    try {
      const resp = await api.historySummaryGlobalGenerate({
        startDate: summaryStartDate,
        endDate: summaryEndDate,
        language: summaryLanguage,
      });
      setSummaryResult(resp.data ?? null);
    } catch (error) {
      setSummaryResult(null);
      setSummaryError(`${error}`);
    } finally {
      setSummaryLoading(false);
    }
  }

  return (
    <div className="shrink-0 border-b border-border/50 bg-muted/10 px-5 py-4 space-y-4 max-h-[70vh] overflow-y-auto">
      <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <Globe className="w-4 h-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">整体动态总结</h3>
          </div>
          <p className="text-xs text-muted-foreground">
            跨所有群聊生成整体动态总结，了解近期消息概况。
          </p>
        </div>
        <Button
          size="sm"
          onClick={handleGenerateSummary}
          disabled={summaryLoading || !aiReady}
          className="min-w-[112px]"
        >
          {summaryLoading ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              生成中...
            </>
          ) : (
            <>
              <Sparkles className="w-4 h-4" />
              生成总结
            </>
          )}
        </Button>
      </div>

      <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant={summaryPreset === "today" ? "default" : "outline"}
            onClick={() => setSummaryPreset("today")}
          >
            今天
          </Button>
          <Button
            type="button"
            size="sm"
            variant={summaryPreset === "3d" ? "default" : "outline"}
            onClick={() => setSummaryPreset("3d")}
          >
            近 3 天
          </Button>
          <Button
            type="button"
            size="sm"
            variant={summaryPreset === "7d" ? "default" : "outline"}
            onClick={() => setSummaryPreset("7d")}
          >
            近 7 天
          </Button>

          <Select
            value={summaryLanguage}
            onValueChange={(v) => setSummaryLanguage(v as SummaryLanguage)}
          >
            <SelectTrigger size="sm" className="w-[120px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LANGUAGE_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="relative">
            <CalendarRange className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
            <Input
              type="date"
              className="h-8 w-[150px] pl-8 text-xs"
              value={summaryStartDate}
              onChange={(event) =>
                handleCustomDateChange(setSummaryStartDate, event.target.value)
              }
            />
          </div>
          <span className="text-xs text-muted-foreground">至</span>
          <Input
            type="date"
            className="h-8 w-[150px] text-xs"
            value={summaryEndDate}
            onChange={(event) =>
              handleCustomDateChange(setSummaryEndDate, event.target.value)
            }
          />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
        <Badge variant="secondary">全部群聊</Badge>
        {daySpan && daySpan > 0 ? <Badge variant="outline">{daySpan} 天</Badge> : null}
        {!aiReady ? (
          <span>总结功能需要在设置页启用 AI 翻译配置。</span>
        ) : (
          <span>将按所选时间范围生成跨群聊整体动态总结。</span>
        )}
      </div>

      {summaryError ? (
        <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-xs text-amber-800">
          {summaryError}
        </div>
      ) : null}

      {summaryResult ? (
        <SummaryCard
          title="整体动态总结"
          subtitle="跨所有群聊"
          dateRange={`${summaryResult.start_date} ~ ${summaryResult.end_date}`}
          messageCount={summaryResult.message_count}
          extraStats={[{ label: "群聊数", value: summaryResult.chat_count }]}
          overallSummary={summaryResult.overall_summary}
          dailyItems={summaryResult.daily_items}
        />
      ) : null}
    </div>
  );
}
