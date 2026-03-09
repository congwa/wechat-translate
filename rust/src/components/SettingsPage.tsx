import { useState, useEffect, useRef, useCallback } from "react";
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
  Upload,
  RotateCcw,
  Image,
  AlertCircle,
  Code2,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import * as api from "@/lib/tauri-api";
import { useToastStore } from "@/stores/toastStore";
import { useFormStore, type SidebarWindowMode } from "@/stores/formStore";
import { useEventStore } from "@/stores/eventStore";

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

// ---------------------------------------------------------------------------
// Frontend schema validation (mirrors backend rules)
// ---------------------------------------------------------------------------

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

  // listen
  const listen = cfg.listen as Record<string, unknown> | undefined;
  if (listen) {
    if (listen.mode !== undefined && listen.mode !== "session") {
      errors.push(`listen.mode 只允许 "session"，当前值: "${listen.mode}"`);
    }
    if (listen.interval_seconds !== undefined) {
      const v = Number(listen.interval_seconds);
      if (isNaN(v) || v < 0.3) errors.push(`listen.interval_seconds 不能小于 0.3，当前值: ${listen.interval_seconds}`);
    }
    if (listen.dedupe_window_seconds !== undefined) {
      const v = Number(listen.dedupe_window_seconds);
      if (isNaN(v) || v <= 0) errors.push(`listen.dedupe_window_seconds 必须大于 0，当前值: ${listen.dedupe_window_seconds}`);
    }
    if (listen.session_preview_dedupe_window_seconds !== undefined) {
      const v = Number(listen.session_preview_dedupe_window_seconds);
      if (isNaN(v) || v <= 0) errors.push(`listen.session_preview_dedupe_window_seconds 必须大于 0，当前值: ${listen.session_preview_dedupe_window_seconds}`);
    }
    if (listen.cross_source_merge_window_seconds !== undefined) {
      const v = Number(listen.cross_source_merge_window_seconds);
      if (isNaN(v) || v <= 0) errors.push(`listen.cross_source_merge_window_seconds 必须大于 0，当前值: ${listen.cross_source_merge_window_seconds}`);
    }
    if (listen.use_right_panel_details !== undefined && typeof listen.use_right_panel_details !== "boolean") {
      errors.push("listen.use_right_panel_details 必须是布尔值");
    }
  }

  // translate
  const translate = cfg.translate as Record<string, unknown> | undefined;
  if (translate) {
    if (translate.timeout_seconds !== undefined) {
      const v = Number(translate.timeout_seconds);
      if (isNaN(v) || v < 1.0) errors.push(`translate.timeout_seconds 不能小于 1.0，当前值: ${translate.timeout_seconds}`);
    }
    if (translate.max_concurrency !== undefined) {
      const v = Number(translate.max_concurrency);
      if (!Number.isInteger(v) || v < 1) errors.push(`translate.max_concurrency 必须是大于 0 的整数，当前值: ${translate.max_concurrency}`);
    }
    if (translate.max_requests_per_second !== undefined) {
      const v = Number(translate.max_requests_per_second);
      if (!Number.isInteger(v) || v < 1) errors.push(`translate.max_requests_per_second 必须是大于 0 的整数，当前值: ${translate.max_requests_per_second}`);
    }
  }

  // display
  const display = cfg.display as Record<string, unknown> | undefined;
  if (display) {
    if (display.width !== undefined) {
      const v = Number(display.width);
      if (isNaN(v) || v < 200 || v > 1200) errors.push(`display.width 须在 200–1200 之间，当前值: ${display.width}`);
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deepMerge(target: Record<string, unknown>, patch: Record<string, unknown>): Record<string, unknown> {
  const result = { ...target };
  for (const key of Object.keys(patch)) {
    const tVal = target[key];
    const pVal = patch[key];
    if (
      typeof tVal === "object" && tVal !== null && !Array.isArray(tVal) &&
      typeof pVal === "object" && pVal !== null && !Array.isArray(pVal)
    ) {
      result[key] = deepMerge(tVal as Record<string, unknown>, pVal as Record<string, unknown>);
    } else {
      result[key] = pVal;
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const showToast = useToastStore((s) => s.showToast);
  const set = useFormStore((s) => s.setSettings);
  const taskState = useEventStore((s) => s.taskState);

  const closeToTray = useFormStore((s) => s.closeToTray);
  const translateEnabled = useFormStore((s) => s.translateEnabled);
  const deeplxUrl = useFormStore((s) => s.deeplxUrl);
  const sourceLang = useFormStore((s) => s.sourceLang);
  const targetLang = useFormStore((s) => s.targetLang);
  const translateTimeout = useFormStore((s) => s.translateTimeout);
  const translateMaxConcurrency = useFormStore((s) => s.translateMaxConcurrency);
  const translateMaxRequestsPerSecond = useFormStore((s) => s.translateMaxRequestsPerSecond);
  const pollInterval = useFormStore((s) => s.pollInterval);
  const useRightPanelDetails = useFormStore((s) => s.useRightPanelDetails);
  const displayWidth = useFormStore((s) => s.displayWidth);
  const sidebarWindowMode = useFormStore((s) => s.sidebarWindowMode);
  const collapsedDisplayCount = useFormStore((s) => s.collapsedDisplayCount);
  const imageCapture = useFormStore((s) => s.imageCapture);

  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [configRaw, setConfigRaw] = useState("");
  const [configErrors, setConfigErrors] = useState<string[]>([]);
  const [configValid, setConfigValid] = useState(true);
  const [configDirty, setConfigDirty] = useState(false);
  const [configLoading, setConfigLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  const lastLoadedRef = useRef("");
  const validateTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  function getTranslateConfigStatus() {
    if (!translateEnabled) {
      return {
        tone: "warn" as const,
        title: "翻译已禁用",
        detail: "配置仍会保留，但当前不会调用翻译接口。",
      };
    }
    if (!deeplxUrl.trim()) {
      return {
        tone: "error" as const,
        title: "翻译接口未配置",
        detail: "缺少 DeepLX 地址；当前运行时无法构造翻译接口。",
      };
    }
    return {
      tone: "ok" as const,
      title: "翻译接口已配置并会写入配置文件",
      detail: "重启应用后会从 /rust 配置文件恢复当前完整翻译接口 URL。",
    };
  }

  // --- Config sync: load from backend on mount (one-time for form fields) ---
  const [configLoaded, setConfigLoaded] = useState(false);
  useEffect(() => {
    if (configLoaded) return;
    loadFromConfig(true);
    setConfigLoaded(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function loadFromConfig(silent = false) {
    try {
      const resp = (await api.configGet()) as unknown as Record<string, unknown>;
      const data = resp.data as Record<string, unknown> | undefined;
      if (!data) return;

      const translate = data.translate as Record<string, unknown> | undefined;
      const listen = data.listen as Record<string, unknown> | undefined;
      const display = data.display as Record<string, unknown> | undefined;

      const patch: Record<string, string | boolean | number> = {};
      const stored = useFormStore.getState();

      if (typeof translate?.enabled === "boolean") {
        patch.translateEnabled = translate.enabled;
      }
      if (typeof translate?.deeplx_url === "string") {
        patch.deeplxUrl = translate.deeplx_url;
      }
      if (typeof translate?.source_lang === "string") {
        patch.sourceLang = translate.source_lang;
      }
      if (typeof translate?.target_lang === "string") {
        patch.targetLang = translate.target_lang;
      }
      if (typeof translate?.timeout_seconds === "number") {
        patch.translateTimeout = String(translate.timeout_seconds);
      }
      if (typeof translate?.max_concurrency === "number") {
        patch.translateMaxConcurrency = String(translate.max_concurrency);
      }
      if (typeof translate?.max_requests_per_second === "number") {
        patch.translateMaxRequestsPerSecond = String(translate.max_requests_per_second);
      }
      if (listen?.interval_seconds && typeof listen.interval_seconds === "number" && stored.pollInterval === "1") {
        patch.pollInterval = String(listen.interval_seconds);
      }
      if (typeof listen?.use_right_panel_details === "boolean") {
        patch.useRightPanelDetails = listen.use_right_panel_details;
      }
      if (display?.width && typeof display.width === "number" && stored.displayWidth === "420") {
        patch.displayWidth = String(display.width);
      }

      if (Object.keys(patch).length > 0) {
        set(patch);
        if (!silent) showToast("已从配置加载参数", true);
      } else if (!silent) {
        showToast("配置无新增参数", true);
      }
    } catch {
      if (!silent) showToast("加载配置失败", false);
    }
  }

  // --- Auto-load config when advanced section opens ---
  const loadAdvancedConfig = useCallback(async () => {
    setConfigLoading(true);
    try {
      const resp = await api.configGet();
      const data = (resp as unknown as Record<string, unknown>).data;
      const raw = JSON.stringify(data, null, 2);
      setConfigRaw(raw);
      lastLoadedRef.current = raw;
      setConfigErrors([]);
      setConfigValid(true);
      setConfigDirty(false);
    } catch (e) {
      showToast(`加载配置失败: ${e}`, false);
    } finally {
      setConfigLoading(false);
    }
  }, [showToast]);

  useEffect(() => {
    if (advancedOpen) {
      loadAdvancedConfig();
    }
  }, [advancedOpen, loadAdvancedConfig]);

  // --- Real-time validation on raw change ---
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

  // --- Apply config (save + sync back to formStore) ---
  async function handleApplyConfig() {
    setBusy("apply");
    try {
      const parsed = JSON.parse(configRaw);
      const resp = await api.configPut(parsed);

      if (!resp.ok) {
        setConfigErrors(resp.errors ?? ["未知错误"]);
        setConfigValid(false);
        showToast("配置校验失败，请修正后重试", false);
        return;
      }

      // Sync back to formStore
      syncConfigToFormStore(parsed);

      lastLoadedRef.current = configRaw;
      setConfigDirty(false);
      showToast("配置已应用", true);
    } catch (e) {
      showToast(`应用失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  function syncConfigToFormStore(parsed: Record<string, unknown>) {
    const patch: Record<string, string | boolean> = {};
    const translate = parsed.translate as Record<string, unknown> | undefined;
    const listen = parsed.listen as Record<string, unknown> | undefined;
    const display = parsed.display as Record<string, unknown> | undefined;

    if (translate) {
      if (typeof translate.enabled === "boolean") patch.translateEnabled = translate.enabled;
      if (typeof translate.deeplx_url === "string") patch.deeplxUrl = translate.deeplx_url;
      if (typeof translate.source_lang === "string") patch.sourceLang = translate.source_lang;
      if (typeof translate.target_lang === "string") patch.targetLang = translate.target_lang;
      if (typeof translate.timeout_seconds === "number") patch.translateTimeout = String(translate.timeout_seconds);
      if (typeof translate.max_concurrency === "number") patch.translateMaxConcurrency = String(translate.max_concurrency);
      if (typeof translate.max_requests_per_second === "number") {
        patch.translateMaxRequestsPerSecond = String(translate.max_requests_per_second);
      }
    }
    if (listen) {
      if (typeof listen.interval_seconds === "number") patch.pollInterval = String(listen.interval_seconds);
      if (typeof listen.use_right_panel_details === "boolean") patch.useRightPanelDetails = listen.use_right_panel_details;
    }
    if (display) {
      if (typeof display.width === "number") patch.displayWidth = String(display.width);
    }

    if (Object.keys(patch).length > 0) set(patch);
  }

  // --- Restore default config ---
  async function handleRestoreDefault() {
    setBusy("restore");
    try {
      const resp = (await api.configDefault()) as unknown as Record<string, unknown>;
      const data = resp.data;
      const raw = JSON.stringify(data, null, 2);
      setConfigRaw(raw);
      setConfigDirty(raw !== lastLoadedRef.current);
      setConfigErrors([]);
      setConfigValid(true);
      showToast("已恢复为默认配置（点击应用生效）", true);
    } catch (e) {
      showToast(`恢复默认失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  // --- Form -> Config sync helper ---
  async function patchConfig(path: string, value: unknown) {
    try {
      const resp = (await api.configGet()) as unknown as Record<string, unknown>;
      const current = (resp.data ?? {}) as Record<string, unknown>;
      const parts = path.split(".");
      const patch: Record<string, unknown> = {};
      let ref = patch;
      for (let i = 0; i < parts.length - 1; i++) {
        ref[parts[i]] = {};
        ref = ref[parts[i]] as Record<string, unknown>;
      }
      ref[parts[parts.length - 1]] = value;
      const merged = deepMerge(current, patch);
      await api.configPut(merged);

      if (advancedOpen) {
        const raw = JSON.stringify(merged, null, 2);
        setConfigRaw(raw);
        lastLoadedRef.current = raw;
        setConfigDirty(false);
        setConfigErrors([]);
        setConfigValid(true);
      }
    } catch {
      // silently ignore config write failures for form sync
    }
  }

  // --- Wrapped set that also syncs to config file ---
  function setAndSync(
    formPatch: Record<string, string | boolean>,
    configPath: string,
    configValue: unknown,
  ) {
    set(formPatch);
    patchConfig(configPath, configValue);
  }

  function handleTrayToggle(checked: boolean) {
    set({ closeToTray: checked });
    api.setCloseToTray(checked).catch(() => {});
  }

  async function handleMonitoringToggle(checked: boolean) {
    setBusy("monitoring");
    try {
      if (checked) {
        await api.listenStart(parseFloat(pollInterval) || 1);
        showToast("消息监听已启动", true);
      } else {
        await api.listenStop();
        showToast("消息监听已暂停", true);
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
      const resp = (await api.translateTest({
        deeplxUrl: deeplxUrl.trim(),
        sourceLang,
        targetLang,
        timeoutSeconds: parseFloat(translateTimeout) || 8,
      })) as unknown as { ok: boolean; data?: string };
      showToast(`测试成功: ${resp.data}`, true);
    } catch (e) {
      showToast(`测试失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="max-w-2xl space-y-6">
      {/* Section: General */}
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
            checked={closeToTray}
            onCheckedChange={handleTrayToggle}
          />
        </div>
      </section>

      {/* Section: Monitoring & AutoReply */}
      <section className="glass-card rounded-2xl p-6 shadow-sm space-y-5">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-emerald-50 flex items-center justify-center">
            <Headphones className="w-4 h-4 text-emerald-600" />
          </div>
          <div>
            <h3 className="text-sm font-semibold">消息监听</h3>
            <p className="text-[11px] text-muted-foreground">轮询与浮窗显示</p>
          </div>
        </div>

        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-sm font-medium">消息监听</h4>
            <p className="text-[11px] text-muted-foreground mt-0.5">
              自动监听微信聊天消息并记录到数据库（应用启动时默认开启）
            </p>
          </div>
          <Switch
            checked={taskState.monitoring}
            onCheckedChange={handleMonitoringToggle}
            disabled={busy === "monitoring"}
          />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">轮询间隔</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="0.3"
                step="0.1"
                value={pollInterval}
                onChange={(e) => {
                  const val = e.target.value;
                  setAndSync({ pollInterval: val }, "listen.interval_seconds", parseFloat(val) || 1);
                }}
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
                value={displayWidth}
                onChange={(e) => {
                  const val = e.target.value;
                  setAndSync({ displayWidth: val }, "display.width", parseInt(val) || 420);
                }}
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
            checked={useRightPanelDetails}
            onCheckedChange={(v) => {
              setAndSync({ useRightPanelDetails: v }, "listen.use_right_panel_details", v);
            }}
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
                onClick={() => set({ sidebarWindowMode: opt.value })}
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
          {taskState.sidebar && (
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
                onClick={() => set({ collapsedDisplayCount: "0" })}
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
                  if (collapsedDisplayCount === "0") set({ collapsedDisplayCount: "1" });
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
                      onClick={() => set({ collapsedDisplayCount: n })}
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

      {/* Section: Translation */}
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
            onClick={() => loadFromConfig(false)}
            title="从配置文件同步"
          >
            <RotateCcw className="w-3.5 h-3.5" />
          </Button>
        </div>

        <div className="flex items-center justify-between">
          <h4 className="text-sm font-medium">启用翻译</h4>
          <Switch
            checked={translateEnabled}
            onCheckedChange={(v) => {
              setAndSync({ translateEnabled: v }, "translate.enabled", v);
            }}
          />
        </div>

        <div className="space-y-2">
          <Label className="text-xs text-muted-foreground">DeepLX 地址</Label>
          <Input
            placeholder="https://api.deeplx.org"
            value={deeplxUrl}
            onChange={(e) => {
              const val = e.target.value;
              setAndSync({ deeplxUrl: val }, "translate.deeplx_url", val);
            }}
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
              value={sourceLang}
              onValueChange={(v) => {
                setAndSync({ sourceLang: v }, "translate.source_lang", v);
              }}
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
              value={targetLang}
              onValueChange={(v) => {
                setAndSync({ targetLang: v }, "translate.target_lang", v);
              }}
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
                value={translateTimeout}
                onChange={(e) => {
                  const val = e.target.value;
                  setAndSync({ translateTimeout: val }, "translate.timeout_seconds", parseFloat(val) || 8);
                }}
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
                value={translateMaxConcurrency}
                onChange={(e) => {
                  const val = e.target.value;
                  setAndSync(
                    { translateMaxConcurrency: val },
                    "translate.max_concurrency",
                    Math.max(1, parseInt(val, 10) || 1),
                  );
                }}
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
                value={translateMaxRequestsPerSecond}
                onChange={(e) => {
                  const val = e.target.value;
                  setAndSync(
                    { translateMaxRequestsPerSecond: val },
                    "translate.max_requests_per_second",
                    Math.max(1, parseInt(val, 10) || 1),
                  );
                }}
              />
              <span className="text-xs text-muted-foreground shrink-0">次</span>
            </div>
          </div>
        </div>

        <Button
          variant="outline"
          className="w-full h-10 rounded-xl font-semibold text-sm"
          onClick={handleTranslateTest}
          disabled={busy === "translate_test" || !deeplxUrl.trim()}
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

      {/* Section: Image Features */}
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
              onCheckedChange={(v) => set({ imageCapture: v })}
            />
          </div>
          <p className="text-[11px] text-muted-foreground">
            监控微信本地缓存，自动提取聊天中发送的图片缩略图。
          </p>
          {!useRightPanelDetails && (
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
          <div className="text-[11px] text-muted-foreground/80 space-y-1">
            <p className="font-medium text-muted-foreground">注意事项</p>
            <ul className="list-disc pl-4 space-y-0.5">
              <li>仅支持 macOS 微信客户端</li>
              <li>需要微信已登录且聊天窗口已打开</li>
              <li>首次使用某个会话时需要等待几秒建立映射</li>
              <li>图片为缩略图质量，非原图</li>
            </ul>
          </div>
        </div>


      </section>

      {/* Section: Advanced (collapsible) */}
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
                {configLoading ? (
                  <div className="flex items-center justify-center py-8 text-muted-foreground">
                    <span className="animate-pulse text-sm">加载配置中…</span>
                  </div>
                ) : (
                  <>
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

                    {/* Validation errors */}
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
                            <ul className="text-[11px] text-red-600/90 dark:text-red-400/90 space-y-1 pl-5 list-disc">
                              {configErrors.map((err, i) => (
                                <li key={i}>{err}</li>
                              ))}
                            </ul>
                          </div>
                        </motion.div>
                      )}
                    </AnimatePresence>

                    {/* Status hint */}
                    {configValid && configDirty && (
                      <p className="text-[11px] text-amber-600 dark:text-amber-400">
                        配置已修改，点击「应用」使更改生效
                      </p>
                    )}
                    {configValid && !configDirty && configRaw && (
                      <p className="text-[11px] text-muted-foreground">
                        配置与磁盘一致，无待保存的更改
                      </p>
                    )}

                    <div className="flex gap-3">
                      <Button
                        variant="outline"
                        className="flex-1 h-10 rounded-xl font-semibold text-sm"
                        onClick={handleRestoreDefault}
                        disabled={busy === "restore"}
                      >
                        <RotateCcw className="w-4 h-4 mr-2" />
                        恢复默认
                      </Button>
                      <Button
                        className="flex-1 h-10 rounded-xl font-semibold text-sm"
                        onClick={handleApplyConfig}
                        disabled={!configValid || !configDirty || busy === "apply"}
                      >
                        {busy === "apply" ? (
                          <span className="animate-pulse">应用中…</span>
                        ) : (
                          <>
                            <Upload className="w-4 h-4 mr-2" />
                            应用
                          </>
                        )}
                      </Button>
                    </div>
                  </>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </section>
    </div>
  );
}
