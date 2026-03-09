import { useEffect, useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Monitor,
  Languages,
  Headphones,
  ChevronDown,
  RotateCcw,
  Image,
  AlertCircle,
  Code2,
  Save,
  RefreshCcw,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import * as api from "@/lib/tauri-api";
import type { AppSettings } from "@/lib/types";
import { useToastStore } from "@/stores/toastStore";
import { useFormStore, type SidebarWindowMode } from "@/stores/formStore";
import {
  draftFromSettings,
  settingsFromDraft,
  useSettingsStore,
} from "@/stores/settingsStore";
import { useRuntimeStore } from "@/stores/runtimeStore";

const SOURCE_LANGS = [
  { value: "auto", label: "auto (自动检测)" },
  { value: "ZH", label: "ZH (中文)" },
  { value: "EN", label: "EN (英语)" },
  { value: "JA", label: "JA (日语)" },
  { value: "KO", label: "KO (韩语)" },
  { value: "DE", label: "DE (德语)" },
  { value: "FR", label: "FR (法语)" },
  { value: "ES", label: "ES (西班牙语)" },
  { value: "RU", label: "RU (俄语)" },
];

const TARGET_LANGS = SOURCE_LANGS.filter((l) => l.value !== "auto");

interface ValidationResult {
  valid: boolean;
  errors: string[];
}

function validateConfigSchema(obj: unknown): ValidationResult {
  const errors: string[] = [];
  if (typeof obj !== "object" || obj === null || Array.isArray(obj)) {
    return { valid: false, errors: ["配置必须是一个 JSON 对象"] };
  }

  const cfg = obj as Record<string, unknown>;
  const listen = cfg.listen as Record<string, unknown> | undefined;
  if (listen) {
    if (listen.mode !== undefined && listen.mode !== "session") {
      errors.push(`listen.mode 只允许 "session"，当前值: "${listen.mode}"`);
    }
    if (listen.interval_seconds !== undefined) {
      const v = Number(listen.interval_seconds);
      if (Number.isNaN(v) || v < 0.3) {
        errors.push(`listen.interval_seconds 不能小于 0.3，当前值: ${listen.interval_seconds}`);
      }
    }
    if (listen.dedupe_window_seconds !== undefined) {
      const v = Number(listen.dedupe_window_seconds);
      if (Number.isNaN(v) || v <= 0) {
        errors.push(`listen.dedupe_window_seconds 必须大于 0，当前值: ${listen.dedupe_window_seconds}`);
      }
    }
    if (listen.session_preview_dedupe_window_seconds !== undefined) {
      const v = Number(listen.session_preview_dedupe_window_seconds);
      if (Number.isNaN(v) || v <= 0) {
        errors.push(`listen.session_preview_dedupe_window_seconds 必须大于 0，当前值: ${listen.session_preview_dedupe_window_seconds}`);
      }
    }
    if (listen.cross_source_merge_window_seconds !== undefined) {
      const v = Number(listen.cross_source_merge_window_seconds);
      if (Number.isNaN(v) || v <= 0) {
        errors.push(`listen.cross_source_merge_window_seconds 必须大于 0，当前值: ${listen.cross_source_merge_window_seconds}`);
      }
    }
    if (listen.use_right_panel_details !== undefined && typeof listen.use_right_panel_details !== "boolean") {
      errors.push("listen.use_right_panel_details 必须是布尔值");
    }
  }

  const translate = cfg.translate as Record<string, unknown> | undefined;
  if (translate) {
    if (translate.timeout_seconds !== undefined) {
      const v = Number(translate.timeout_seconds);
      if (Number.isNaN(v) || v < 1.0) {
        errors.push(`translate.timeout_seconds 不能小于 1.0，当前值: ${translate.timeout_seconds}`);
      }
    }
    if (translate.max_concurrency !== undefined) {
      const v = Number(translate.max_concurrency);
      if (!Number.isInteger(v) || v < 1) {
        errors.push(`translate.max_concurrency 必须是大于 0 的整数，当前值: ${translate.max_concurrency}`);
      }
    }
    if (translate.max_requests_per_second !== undefined) {
      const v = Number(translate.max_requests_per_second);
      if (!Number.isInteger(v) || v < 1) {
        errors.push(`translate.max_requests_per_second 必须是大于 0 的整数，当前值: ${translate.max_requests_per_second}`);
      }
    }
  }

  const display = cfg.display as Record<string, unknown> | undefined;
  if (display) {
    if (display.width !== undefined) {
      const v = Number(display.width);
      if (Number.isNaN(v) || v < 200 || v > 1200) {
        errors.push(`display.width 须在 200–1200 之间，当前值: ${display.width}`);
      }
    }
    if (display.side !== undefined && display.side !== "left" && display.side !== "right") {
      errors.push(`display.side 只允许 "left" 或 "right"，当前值: "${display.side}"`);
    }
    if (display.on_translate_fail !== undefined && display.on_translate_fail !== "show_cn_with_reason" && display.on_translate_fail !== "hide") {
      errors.push(`display.on_translate_fail 只允许 "show_cn_with_reason" 或 "hide"，当前值: "${display.on_translate_fail}"`);
    }
  }

  return { valid: errors.length === 0, errors };
}

function isDraftDirty(settings: AppSettings | null, draft: ReturnType<typeof draftFromSettings>) {
  if (!settings) return false;
  const baseline = draftFromSettings(settings);
  return JSON.stringify(baseline) !== JSON.stringify(draft);
}

export function SettingsPage() {
  const showToast = useToastStore((s) => s.showToast);
  const runtime = useRuntimeStore((s) => s.runtime);
  const settings = useSettingsStore((s) => s.settings);
  const draft = useSettingsStore((s) => s.draft);
  const updateDraft = useSettingsStore((s) => s.updateDraft);
  const resetDraft = useSettingsStore((s) => s.resetDraft);
  const setSettingsSnapshot = useSettingsStore((s) => s.setSettings);
  const setRuntime = useRuntimeStore((s) => s.setRuntime);
  const setTranslatorStatus = useRuntimeStore((s) => s.setTranslatorStatus);

  const sidebarWindowMode = useFormStore((s) => s.sidebarWindowMode);
  const collapsedDisplayCount = useFormStore((s) => s.collapsedDisplayCount);
  const imageCapture = useFormStore((s) => s.imageCapture);
  const setUiSettings = useFormStore((s) => s.setSettings);

  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [configRaw, setConfigRaw] = useState("");
  const [configErrors, setConfigErrors] = useState<string[]>([]);
  const [configValid, setConfigValid] = useState(true);
  const [configDirty, setConfigDirty] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  const lastLoadedRef = useRef("");
  const validateTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const basicDirty = isDraftDirty(settings, draft);

  const canSyncTranslateTestResult =
    !!settings &&
    draft.translateEnabled === settings.translate.enabled &&
    draft.deeplxUrl.trim() === settings.translate.deeplx_url.trim() &&
    draft.sourceLang === settings.translate.source_lang &&
    draft.targetLang === settings.translate.target_lang &&
    (parseFloat(draft.translateTimeout) || 8) === settings.translate.timeout_seconds;

  useEffect(() => {
    if (!advancedOpen || !settings || configDirty) return;
    const raw = JSON.stringify(settings, null, 2);
    setConfigRaw(raw);
    lastLoadedRef.current = raw;
    setConfigErrors([]);
    setConfigValid(true);
  }, [advancedOpen, settings, configDirty]);

  function getTranslateConfigStatus() {
    if (!draft.translateEnabled) {
      return {
        tone: "warn" as const,
        title: "翻译已禁用",
        detail: "配置仍会保留，但当前不会调用翻译接口。",
      };
    }
    if (!draft.deeplxUrl.trim()) {
      return {
        tone: "error" as const,
        title: "翻译接口未配置",
        detail: "缺少 DeepLX 地址；当前运行时无法构造翻译接口。",
      };
    }
    return {
      tone: "ok" as const,
      title: "翻译接口已配置",
      detail: "点击“应用更改”后，主窗口、侧边栏和菜单栏会同时使用这份配置。",
    };
  }

  async function reloadFromBackend(silent = false) {
    setBusy("reload");
    try {
      const resp = await api.appStateGet();
      if (resp.data) {
        setSettingsSnapshot(resp.data.settings);
        setRuntime(resp.data.runtime);
        if (advancedOpen) {
          const raw = JSON.stringify(resp.data.settings, null, 2);
          setConfigRaw(raw);
          lastLoadedRef.current = raw;
          setConfigDirty(false);
          setConfigErrors([]);
          setConfigValid(true);
        }
        if (!silent) showToast("已从后端重新加载配置", true);
      }
    } catch (e) {
      if (!silent) showToast(`加载配置失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  async function applySettings(nextSettings: AppSettings, successMessage: string) {
    const resp = await api.settingsUpdate(nextSettings);
    if (!resp.ok) {
      const errors = resp.errors ?? ["未知错误"];
      setConfigErrors(errors);
      setConfigValid(false);
      showToast(errors[0] ?? "配置校验失败", false);
      return false;
    }

    if (resp.data) {
      setSettingsSnapshot(resp.data.settings);
      setRuntime(resp.data.runtime);
      if (advancedOpen) {
        const raw = JSON.stringify(resp.data.settings, null, 2);
        setConfigRaw(raw);
        lastLoadedRef.current = raw;
        setConfigDirty(false);
        setConfigErrors([]);
        setConfigValid(true);
      }
    }

    showToast(successMessage, true);
    return true;
  }

  async function handleApplyDraft() {
    if (!settings) return;
    setBusy("save");
    try {
      const nextSettings = settingsFromDraft(settings, draft);
      await applySettings(nextSettings, "配置已应用");
    } catch (e) {
      showToast(`应用失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  async function handleRestoreDraft() {
    resetDraft();
    showToast("已撤销未应用更改", true);
  }

  async function handleApplyConfig() {
    setBusy("apply");
    try {
      const parsed = JSON.parse(configRaw) as AppSettings;
      await applySettings(parsed, "配置已应用");
    } catch (e) {
      showToast(`应用失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  async function handleRestoreDefault() {
    setBusy("restore");
    try {
      const resp = await api.configDefault();
      if (!resp.data) return;
      const raw = JSON.stringify(resp.data, null, 2);
      setConfigRaw(raw);
      setConfigDirty(true);
      setConfigErrors([]);
      setConfigValid(true);
      showToast("已恢复为默认配置（点击应用生效）", true);
    } catch (e) {
      showToast(`恢复默认失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  function handleConfigRawChange(value: string) {
    setConfigRaw(value);
    setConfigDirty(value !== lastLoadedRef.current);

    if (validateTimerRef.current) clearTimeout(validateTimerRef.current);
    validateTimerRef.current = setTimeout(() => {
      try {
        const parsed = JSON.parse(value);
        const result = validateConfigSchema(parsed);
        setConfigErrors(result.errors);
        setConfigValid(result.valid);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setConfigErrors([`JSON 语法错误: ${msg}`]);
        setConfigValid(false);
      }
    }, 300);
  }

  function handleTrayToggle(checked: boolean) {
    api.setCloseToTray(checked).catch(() => {});
  }

  async function handleMonitoringToggle(checked: boolean) {
    setBusy("monitoring");
    try {
      const interval = settings?.listen.interval_seconds ?? 1;
      if (checked) {
        await api.listenStart(interval);
        showToast(
          runtime.tasks.sidebar ? "消息监听已恢复，浮窗继续运行" : "消息监听已启动",
          true,
        );
      } else {
        await api.listenStop();
        showToast(
          runtime.tasks.sidebar ? "消息监听已暂停，浮窗保持运行" : "消息监听已暂停",
          true,
        );
      }
    } catch (e) {
      showToast(`${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  async function handleTranslateTest() {
    setBusy("translate_test");
    try {
      const resp = await api.translateTest({
        deeplxUrl: draft.deeplxUrl.trim(),
        sourceLang: draft.sourceLang,
        targetLang: draft.targetLang,
        timeoutSeconds: parseFloat(draft.translateTimeout) || 8,
      });
      if (canSyncTranslateTestResult && draft.translateEnabled) {
        setTranslatorStatus({
          enabled: true,
          configured: true,
          checking: false,
          healthy: true,
          last_error: null,
        });
      }
      showToast(`测试成功: ${resp.data}`, true);
    } catch (e) {
      if (canSyncTranslateTestResult && draft.translateEnabled) {
        setTranslatorStatus({
          enabled: true,
          configured: draft.deeplxUrl.trim() !== "",
          checking: false,
          healthy: false,
          last_error: `${e}`,
        });
      }
      showToast(`测试失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  if (!settings) {
    return (
      <div className="glass-card rounded-2xl p-8 text-sm text-muted-foreground">
        正在加载设置...
      </div>
    );
  }

  return (
    <div className="max-w-2xl space-y-6">
      <section className="glass-card rounded-2xl p-6 shadow-sm space-y-4 border border-primary/10">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h3 className="text-sm font-semibold">配置同步</h3>
            <p className="text-[11px] text-muted-foreground mt-1">
              表单编辑只修改本地草稿；点击应用后才会写入后端配置并同步到菜单栏和侧边栏。
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              className="rounded-xl"
              onClick={() => reloadFromBackend(false)}
              disabled={busy === "reload"}
            >
              <RefreshCcw className="w-4 h-4 mr-2" />
              重新加载
            </Button>
            <Button
              variant="outline"
              className="rounded-xl"
              onClick={handleRestoreDraft}
              disabled={!basicDirty}
            >
              <RotateCcw className="w-4 h-4 mr-2" />
              撤销草稿
            </Button>
            <Button
              className="rounded-xl"
              onClick={handleApplyDraft}
              disabled={!basicDirty || busy === "save"}
            >
              <Save className="w-4 h-4 mr-2" />
              {busy === "save" ? "应用中..." : "应用更改"}
            </Button>
          </div>
        </div>
        {basicDirty && (
          <div className="rounded-xl border border-amber-500/30 bg-amber-500/8 px-4 py-3 text-[11px] text-amber-700 dark:text-amber-300">
            当前有未应用更改。实时浮窗、托盘菜单和监听任务仍使用已提交的配置。
          </div>
        )}
      </section>

      <section className="glass-card rounded-2xl p-6 shadow-sm space-y-5">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-slate-100 flex items-center justify-center">
            <Monitor className="w-4 h-4 text-slate-600" />
          </div>
          <div>
            <h3 className="text-sm font-semibold">通用设置</h3>
            <p className="text-[11px] text-muted-foreground">应用基本行为</p>
          </div>
        </div>

        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-sm font-medium">关闭时最小化到托盘</h4>
            <p className="text-[11px] text-muted-foreground mt-0.5">
              关闭窗口后应用在系统托盘中继续运行
            </p>
          </div>
          <Switch
            checked={runtime.close_to_tray}
            onCheckedChange={handleTrayToggle}
          />
        </div>
      </section>

      <section className="glass-card rounded-2xl p-6 shadow-sm space-y-5">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-emerald-50 flex items-center justify-center">
            <Headphones className="w-4 h-4 text-emerald-600" />
          </div>
          <div>
            <h3 className="text-sm font-semibold">消息监听</h3>
            <p className="text-[11px] text-muted-foreground">轮询控制与浮窗联动</p>
          </div>
        </div>

        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-sm font-medium">消息监听</h4>
            <p className="text-[11px] text-muted-foreground mt-0.5">
              关闭后只暂停轮询；若浮窗已开启，会保留当前窗口和翻译状态，恢复监听后继续收流
            </p>
          </div>
          <Switch
            checked={runtime.tasks.monitoring}
            onCheckedChange={handleMonitoringToggle}
            disabled={busy === "monitoring"}
          />
        </div>

        {!runtime.tasks.monitoring && runtime.tasks.sidebar && (
          <div className="rounded-xl border border-amber-500/30 bg-amber-500/8 px-4 py-3">
            <div className="text-sm font-medium text-amber-700 dark:text-amber-300">
              监听已暂停，浮窗仍在运行
            </div>
            <p className="text-[11px] text-amber-700/80 dark:text-amber-300/80 mt-1">
              当前不会接收新消息；重新打开监听后，浮窗会自动继续展示和翻译新内容。
            </p>
          </div>
        )}

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">轮询间隔</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="0.3"
                step="0.1"
                value={draft.pollInterval}
                onChange={(e) => updateDraft({ pollInterval: e.target.value })}
              />
              <span className="text-xs text-muted-foreground shrink-0">秒</span>
            </div>
          </div>
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">浮窗宽度</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="200"
                step="10"
                value={draft.displayWidth}
                onChange={(e) => updateDraft({ displayWidth: e.target.value })}
              />
              <span className="text-xs text-muted-foreground shrink-0">px</span>
            </div>
          </div>
        </div>

        <div className="flex items-center justify-between gap-4 rounded-xl border border-border/60 bg-background/40 px-4 py-3">
          <div>
            <h4 className="text-sm font-medium">右侧详情补充</h4>
            <p className="text-[11px] text-muted-foreground mt-0.5">
              关闭时只监听左侧列表最新预览；开启后读取右侧消息区补充详情。无论开关状态，都会读取右侧标题区区分群聊和私聊。
            </p>
          </div>
          <Switch
            checked={draft.useRightPanelDetails}
            onCheckedChange={(v) => updateDraft({ useRightPanelDetails: v })}
          />
        </div>

        <div className="space-y-2">
          <Label className="text-xs text-muted-foreground">浮窗模式</Label>
          <div className="grid grid-cols-2 gap-3">
            {([
              { value: "follow" as SidebarWindowMode, label: "跟随微信", desc: "贴在微信窗口右侧，层级与微信一致" },
              { value: "independent" as SidebarWindowMode, label: "独立置顶", desc: "屏幕右上角，最高层级，可拖拽和折叠" },
            ]).map((opt) => (
              <button
                key={opt.value}
                onClick={() => setUiSettings({ sidebarWindowMode: opt.value })}
                className={`text-left rounded-xl border p-3 transition-all duration-150 ${
                  sidebarWindowMode === opt.value
                    ? "border-primary bg-primary/5 ring-1 ring-primary/20"
                    : "border-border hover:border-muted-foreground/30"
                }`}
              >
                <span className={`text-sm font-medium ${
                  sidebarWindowMode === opt.value ? "text-primary" : ""
                }`}>{opt.label}</span>
                <p className="text-[11px] text-muted-foreground mt-0.5">{opt.desc}</p>
              </button>
            ))}
          </div>
          {runtime.tasks.sidebar && (
            <p className="text-[11px] text-amber-600 dark:text-amber-400">
              浮窗运行中，切换模式需要重新开启浮窗后生效
            </p>
          )}
        </div>

        {sidebarWindowMode === "independent" && (
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">折叠显示</Label>
            <div className="grid grid-cols-2 gap-3">
              <button
                onClick={() => setUiSettings({ collapsedDisplayCount: "0" })}
                className={`text-left rounded-xl border p-3 transition-all duration-150 ${
                  collapsedDisplayCount === "0"
                    ? "border-primary bg-primary/5 ring-1 ring-primary/20"
                    : "border-border hover:border-muted-foreground/30"
                }`}
              >
                <span className={`text-sm font-medium ${
                  collapsedDisplayCount === "0" ? "text-primary" : ""
                }`}>完全折叠</span>
                <p className="text-[11px] text-muted-foreground mt-0.5">折叠后不显示任何消息</p>
              </button>
              <button
                onClick={() => {
                  if (collapsedDisplayCount === "0") {
                    setUiSettings({ collapsedDisplayCount: "1" });
                  }
                }}
                className={`text-left rounded-xl border p-3 transition-all duration-150 ${
                  collapsedDisplayCount !== "0"
                    ? "border-primary bg-primary/5 ring-1 ring-primary/20"
                    : "border-border hover:border-muted-foreground/30"
                }`}
              >
                <span className={`text-sm font-medium ${
                  collapsedDisplayCount !== "0" ? "text-primary" : ""
                }`}>保留最新消息</span>
                <p className="text-[11px] text-muted-foreground mt-0.5">折叠后仍显示最新消息</p>
              </button>
            </div>
            {collapsedDisplayCount !== "0" && (
              <div className="flex items-center gap-2 pt-1">
                <Label className="text-xs text-muted-foreground shrink-0">显示条数</Label>
                <div className="flex items-center gap-1">
                  {["1", "2", "3"].map((n) => (
                    <button
                      key={n}
                      onClick={() => setUiSettings({ collapsedDisplayCount: n })}
                      className={`w-8 h-8 rounded-lg text-sm font-medium transition-all duration-150 ${
                        collapsedDisplayCount === n
                          ? "bg-primary text-primary-foreground shadow-sm"
                          : "bg-muted text-muted-foreground hover:bg-muted/80"
                      }`}
                    >
                      {n}
                    </button>
                  ))}
                </div>
                <span className="text-xs text-muted-foreground">条</span>
              </div>
            )}
          </div>
        )}
      </section>

      <section className="glass-card rounded-2xl p-6 shadow-sm space-y-5">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-violet-50 flex items-center justify-center">
              <Languages className="w-4 h-4 text-violet-600" />
            </div>
            <div>
              <h3 className="text-sm font-semibold">翻译设置</h3>
              <p className="text-[11px] text-muted-foreground">浮窗翻译参数</p>
            </div>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => reloadFromBackend(false)}
            title="从后端重新加载"
          >
            <RotateCcw className="w-3.5 h-3.5" />
          </Button>
        </div>

        <div className="flex items-center justify-between">
          <h4 className="text-sm font-medium">启用翻译</h4>
          <Switch
            checked={draft.translateEnabled}
            onCheckedChange={(v) => updateDraft({ translateEnabled: v })}
          />
        </div>

        <div className="space-y-2">
          <Label className="text-xs text-muted-foreground">DeepLX 地址</Label>
          <Input
            placeholder="https://api.deeplx.org"
            value={draft.deeplxUrl}
            onChange={(e) => updateDraft({ deeplxUrl: e.target.value })}
          />
          <p className="text-[11px] text-muted-foreground/70">
            填写完整翻译接口 URL。前往{" "}
            <a
              href="https://connect.linux.do/dash/deeplx"
              target="_blank"
              rel="noopener noreferrer"
              className="text-violet-500 hover:underline"
            >
              connect.linux.do/dash/deeplx
            </a>
            {" "}获取完整 URL
          </p>
        </div>

        <div
          className={`rounded-xl border px-3 py-2 text-[11px] ${
            getTranslateConfigStatus().tone === "ok"
              ? "border-emerald-200 bg-emerald-50/70 text-emerald-700 dark:border-emerald-800/50 dark:bg-emerald-950/20 dark:text-emerald-300"
              : getTranslateConfigStatus().tone === "warn"
                ? "border-amber-200 bg-amber-50/70 text-amber-700 dark:border-amber-800/50 dark:bg-amber-950/20 dark:text-amber-300"
                : "border-red-200 bg-red-50/70 text-red-700 dark:border-red-800/50 dark:bg-red-950/20 dark:text-red-300"
          }`}
        >
          <div className="font-medium">{getTranslateConfigStatus().title}</div>
          <div className="mt-1 opacity-90">{getTranslateConfigStatus().detail}</div>
        </div>

        <div className="grid grid-cols-2 gap-4 lg:grid-cols-3">
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">源语言</Label>
            <Select
              value={draft.sourceLang}
              onValueChange={(v) => updateDraft({ sourceLang: v })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SOURCE_LANGS.map((l) => (
                  <SelectItem key={l.value} value={l.value}>
                    {l.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">目标语言</Label>
            <Select
              value={draft.targetLang}
              onValueChange={(v) => updateDraft({ targetLang: v })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {TARGET_LANGS.map((l) => (
                  <SelectItem key={l.value} value={l.value}>
                    {l.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">超时时间</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="1"
                step="1"
                value={draft.translateTimeout}
                onChange={(e) => updateDraft({ translateTimeout: e.target.value })}
              />
              <span className="text-xs text-muted-foreground shrink-0">秒</span>
            </div>
          </div>
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">同时并发数</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="1"
                step="1"
                value={draft.translateMaxConcurrency}
                onChange={(e) => updateDraft({ translateMaxConcurrency: e.target.value })}
              />
              <span className="text-xs text-muted-foreground shrink-0">个</span>
            </div>
          </div>
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">每秒请求数</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="1"
                step="1"
                value={draft.translateMaxRequestsPerSecond}
                onChange={(e) => updateDraft({ translateMaxRequestsPerSecond: e.target.value })}
              />
              <span className="text-xs text-muted-foreground shrink-0">次</span>
            </div>
          </div>
        </div>

        <Button
          variant="outline"
          className="w-full h-10 rounded-xl font-semibold text-sm"
          onClick={handleTranslateTest}
          disabled={busy === "translate_test" || !draft.deeplxUrl.trim()}
        >
          {busy === "translate_test" ? (
            <span className="animate-pulse">测试中…</span>
          ) : (
            <>
              <Languages className="w-4 h-4 mr-2" />
              测试翻译
            </>
          )}
        </Button>
      </section>

      <section className="glass-card rounded-2xl p-6 shadow-sm space-y-5 border border-amber-200/50 dark:border-amber-700/30">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-amber-50 dark:bg-amber-900/30 flex items-center justify-center">
            <Image className="w-4 h-4 text-amber-600 dark:text-amber-400" />
          </div>
          <div>
            <h3 className="text-sm font-semibold">图片缩略图</h3>
            <p className="text-[11px] text-muted-foreground">
              读取微信本地缓存中的聊天图片缩略图，仅支持 macOS。
            </p>
          </div>
        </div>

        <div className="bg-amber-50/50 dark:bg-amber-900/10 rounded-xl p-4 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Image className="w-4 h-4 text-amber-600 dark:text-amber-400" />
              <h4 className="text-sm font-medium">聊天图片缩略图</h4>
            </div>
            <Switch
              checked={imageCapture}
              onCheckedChange={(v) => setUiSettings({ imageCapture: v })}
            />
          </div>
          <p className="text-[11px] text-muted-foreground">
            该选项属于本地 UI 运行偏好；重新开启浮窗后会按当前偏好生效。
          </p>
          {!draft.useRightPanelDetails && (
            <p className="text-[11px] text-amber-600 dark:text-amber-400">
              当前已关闭“右侧详情补充”，图片缩略图不会生效。
            </p>
          )}
          <div className="text-[11px] text-muted-foreground/80 space-y-1">
            <p className="font-medium text-muted-foreground">工作原理</p>
            <p>
              当检测到 [图片] 消息时，从微信缓存目录读取对应的图片缩略图并展示在浮窗和历史记录中。
            </p>
          </div>
        </div>
      </section>

      <section className="glass-card rounded-2xl shadow-sm overflow-hidden">
        <button
          onClick={() => setAdvancedOpen(!advancedOpen)}
          className="w-full flex items-center justify-between p-6 hover:bg-muted/30 transition-colors"
        >
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-amber-50 flex items-center justify-center">
              <Code2 className="w-4 h-4 text-amber-600" />
            </div>
            <div className="text-left">
              <h3 className="text-sm font-semibold">高级配置</h3>
              <p className="text-[11px] text-muted-foreground">
                直接编辑 listener.json 配置文件
              </p>
            </div>
          </div>
          <ChevronDown
            className={`w-4 h-4 text-muted-foreground transition-transform duration-200 ${
              advancedOpen ? "rotate-180" : ""
            }`}
          />
        </button>

        <AnimatePresence>
          {advancedOpen && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.2 }}
              className="overflow-hidden"
            >
              <div className="px-6 pb-6 space-y-4">
                <Textarea
                  value={configRaw}
                  onChange={(e) => handleConfigRawChange(e.target.value)}
                  rows={16}
                  className={`font-mono text-xs resize-none transition-colors ${
                    !configValid
                      ? "border-red-400 focus-visible:ring-red-400"
                      : configDirty
                        ? "border-amber-400 focus-visible:ring-amber-400"
                        : ""
                  }`}
                />

                <AnimatePresence>
                  {configErrors.length > 0 && (
                    <motion.div
                      initial={{ height: 0, opacity: 0 }}
                      animate={{ height: "auto", opacity: 1 }}
                      exit={{ height: 0, opacity: 0 }}
                      className="overflow-hidden"
                    >
                      <div className="rounded-xl bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/40 p-3 space-y-1.5">
                        <div className="flex items-center gap-1.5 text-red-600 dark:text-red-400">
                          <AlertCircle className="w-3.5 h-3.5" />
                          <span className="text-xs font-medium">
                            校验错误 ({configErrors.length})
                          </span>
                        </div>
                        {configErrors.map((err, idx) => (
                          <div key={idx} className="text-xs text-red-700 dark:text-red-300">
                            {err}
                          </div>
                        ))}
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>

                <div className="flex flex-wrap items-center gap-2 justify-end">
                  <Button
                    variant="outline"
                    onClick={handleRestoreDefault}
                    disabled={busy === "restore"}
                  >
                    {busy === "restore" ? "恢复中..." : "恢复默认"}
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => reloadFromBackend(false)}
                    disabled={busy === "reload"}
                  >
                    {busy === "reload" ? "加载中..." : "重新加载"}
                  </Button>
                  <Button
                    onClick={handleApplyConfig}
                    disabled={!configDirty || !configValid || busy === "apply"}
                  >
                    {busy === "apply" ? "应用中..." : "应用配置"}
                  </Button>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </section>
    </div>
  );
}
