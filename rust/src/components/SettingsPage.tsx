import { useEffect, useRef, useState, useCallback } from "react";
import { Button } from "@/components/ui/button";
import {
  RefreshCcw,
} from "lucide-react";
import * as api from "@/lib/tauri-api";
import type { AppSettings } from "@/lib/types";
import { useToastStore } from "@/stores/toastStore";
import {
  createEmptyDraft,
  draftFromSettings,
  type SettingsDraft,
  settingsFromDraft,
  useSettingsStore,
} from "@/stores/settingsStore";
import {
  useUiPreferencesStore,
} from "@/stores/uiPreferencesStore";
import { useRuntimeStore } from "@/stores/runtimeStore";
import {
  fetchProviders,
  getApiUrlForProvider,
  BUILTIN_PROVIDERS,
  type ProviderInfo,
} from "@/lib/models-registry";
import {
  AdvancedConfigSection,
  DictSection,
  DisplaySection,
  GeneralSection,
  ImageCaptureSection,
  ListenSection,
  TranslateSection,
} from "@/components/settings/sections";

interface ValidationResult {
  valid: boolean;
  errors: string[];
}

type SettingsDraftSection = "listen" | "translate" | "display";

interface SectionDirtyState {
  listen: boolean;
  translate: boolean;
  display: boolean;
}

const SECTION_FIELDS: Record<SettingsDraftSection, (keyof SettingsDraft)[]> = {
  listen: ["pollInterval", "useRightPanelDetails"],
  translate: [
    "translateEnabled",
    "translateProvider",
    "deeplxUrl",
    "aiProviderId",
    "aiModelId",
    "aiApiKey",
    "aiBaseUrl",
    "sourceLang",
    "targetLang",
    "translateTimeout",
    "translateMaxConcurrency",
    "translateMaxRequestsPerSecond",
  ],
  display: [
    "displayWidth",
    "collapsedDisplayCount",
    "ghostMode",
    "imageCapture",
    "bgOpacity",
    "blur",
    "cardStyle",
    "textEnhance",
  ],
};

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

export function SettingsPage() {
  const showToast = useToastStore((s) => s.showToast);
  const runtime = useRuntimeStore((s) => s.runtime);
  const settingsSnapshot = useSettingsStore((s) => s.snapshot);
  const settings = useSettingsStore((s) => s.settings);
  const applySettingsSnapshot = useSettingsStore((s) => s.applySnapshot);
  const applyRuntimeSnapshot = useRuntimeStore((s) => s.applySnapshot);

  const sidebarWindowMode = useUiPreferencesStore((s) => s.sidebarWindowMode);
  const setUiPreferences = useUiPreferencesStore((s) => s.setPreferences);

  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [configRaw, setConfigRaw] = useState("");
  const [configErrors, setConfigErrors] = useState<string[]>([]);
  const [configValid, setConfigValid] = useState(true);
  const [configDirty, setConfigDirty] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [translateTestError, setTranslateTestError] = useState<string | null>(null);
  const [draft, setDraft] = useState<SettingsDraft>(createEmptyDraft());
  const [sectionDirty, setSectionDirty] = useState<SectionDirtyState>({
    listen: false,
    translate: false,
    display: false,
  });

  // AI 渠道动态加载
  const [aiProviders, setAiProviders] = useState<ProviderInfo[]>(BUILTIN_PROVIDERS);
  const [aiProvidersLoading, setAiProvidersLoading] = useState(false);
  const [showApiKey, setShowApiKey] = useState(false);

  const lastLoadedRef = useRef("");
  const validateTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const updateDraft = useCallback(
    (patch: Partial<SettingsDraft>, section?: SettingsDraftSection) => {
      setDraft((state) => ({ ...state, ...patch }));
      setSectionDirty((state) => {
        const next = { ...state };
        if (section) {
          next[section] = true;
        } else {
          for (const key of Object.keys(patch) as (keyof SettingsDraft)[]) {
            for (const candidate of Object.keys(
              SECTION_FIELDS,
            ) as SettingsDraftSection[]) {
              if (SECTION_FIELDS[candidate].includes(key)) {
                next[candidate] = true;
              }
            }
          }
        }
        return next;
      });
    },
    [],
  );

  const resetSection = useCallback(
    (section: SettingsDraftSection) => {
      if (!settings) return;
      const baseline = draftFromSettings(settings);
      const patch: Partial<SettingsDraft> = {};
      for (const key of SECTION_FIELDS[section]) {
        (patch as Record<string, unknown>)[key] = baseline[key];
      }
      setDraft((state) => ({ ...state, ...patch }));
      setSectionDirty((state) => ({ ...state, [section]: false }));
    },
    [settings],
  );

  const markSectionClean = useCallback((section: SettingsDraftSection) => {
    setSectionDirty((state) => ({ ...state, [section]: false }));
  }, []);

  useEffect(() => {
    if (!settings) return;
    setDraft(draftFromSettings(settings));
    setSectionDirty({ listen: false, translate: false, display: false });
  }, [settingsSnapshot?.version, settings]);

  // 加载 AI 渠道列表
  useEffect(() => {
    let cancelled = false;
    setAiProvidersLoading(true);
    fetchProviders()
      .then((providers) => {
        if (!cancelled) {
          setAiProviders(providers.length > 0 ? providers : BUILTIN_PROVIDERS);
        }
      })
      .catch(() => {
        // 使用内置列表作为降级
        if (!cancelled) {
          setAiProviders(BUILTIN_PROVIDERS);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setAiProvidersLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 当渠道变化时，自动填充 API 地址
  useEffect(() => {
    if (draft.translateProvider === "ai" && draft.aiProviderId && !draft.aiBaseUrl) {
      const apiUrl = getApiUrlForProvider(aiProviders, draft.aiProviderId);
      if (apiUrl) {
        updateDraft({ aiBaseUrl: apiUrl });
      }
    }
  }, [draft.aiProviderId, draft.translateProvider, aiProviders, updateDraft]);

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
    
    // 根据选择的渠道检查配置
    if (draft.translateProvider === "ai") {
      if (!draft.aiApiKey.trim()) {
        return {
          tone: "error" as const,
          title: "AI 翻译未配置",
          detail: "缺少 API Key；请填写 AI 渠道的 API Key。",
        };
      }
      if (!draft.aiProviderId || !draft.aiModelId) {
        return {
          tone: "error" as const,
          title: "AI 翻译未配置",
          detail: "请选择 AI 渠道和模型。",
        };
      }
      return {
        tone: "ok" as const,
        title: "AI 翻译已配置",
        detail: `使用 ${draft.aiProviderId} / ${draft.aiModelId}`,
      };
    }
    
    // DeepLX 渠道
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
      detail: "点击「应用更改」后，主窗口、侧边栏和菜单栏会同时使用这份配置。",
    };
  }

  async function reloadFromBackend(silent = false) {
    setBusy("reload");
    try {
      const resp = await api.appStateGet();
      if (resp.data) {
        applySettingsSnapshot(resp.data.settings);
        applyRuntimeSnapshot(resp.data.runtime);
        if (advancedOpen) {
          const raw = JSON.stringify(resp.data.settings.data, null, 2);
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

    showToast(successMessage, true);
    return true;
  }

  async function handleApplySection(section: "listen" | "translate" | "display") {
    if (!settings) return;
    const sectionLabels = { listen: "监听设置", translate: "翻译设置", display: "显示设置" };
    setBusy(`section_${section}`);
    try {
      const nextSettings = settingsFromDraft(settings, draft);
      const success = await applySettings(nextSettings, `${sectionLabels[section]}已应用`);
      if (success) {
        markSectionClean(section);
      }
    } catch (e) {
      showToast(`应用失败: ${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  function handleResetSection(section: "listen" | "translate" | "display") {
    resetSection(section);
    const sectionLabels = { listen: "监听设置", translate: "翻译设置", display: "显示设置" };
    showToast(`${sectionLabels[section]}已撤销`, true);
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
    setTranslateTestError(null);
    try {
      const resp = await api.translateTest({
        provider: draft.translateProvider,
        deeplxUrl: draft.deeplxUrl.trim(),
        aiProviderId: draft.aiProviderId,
        aiModelId: draft.aiModelId,
        aiApiKey: draft.aiApiKey,
        aiBaseUrl: draft.aiBaseUrl,
        sourceLang: draft.sourceLang,
        targetLang: draft.targetLang,
        timeoutSeconds: parseFloat(draft.translateTimeout) || 8,
      });
      showToast(`测试成功: ${resp.data}`, true);
    } catch (e) {
      const errorMsg = `${e}`;
      setTranslateTestError(errorMsg);
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

  // 导航项定义
  const NAV_ITEMS = [
    { id: "general", label: "通用", isDirty: false },
    { id: "listen", label: "监听", isDirty: sectionDirty.listen },
    ...(sidebarWindowMode === "independent" ? [{ id: "display", label: "浮窗", isDirty: sectionDirty.display }] : []),
    { id: "translate", label: "翻译", isDirty: sectionDirty.translate },
    { id: "dict", label: "词典", isDirty: false },
  ];

  // 当前可见的 section
  const [activeSection, setActiveSection] = useState("general");
  const sectionRefs = useRef<Record<string, HTMLElement | null>>({});

  // Intersection Observer 监听滚动
  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting && entry.intersectionRatio >= 0.3) {
            setActiveSection(entry.target.id);
          }
        });
      },
      { threshold: [0.3, 0.5, 0.7], rootMargin: "-100px 0px -50% 0px" }
    );

    Object.values(sectionRefs.current).forEach((el) => {
      if (el) observer.observe(el);
    });

    return () => observer.disconnect();
  }, [sidebarWindowMode]);

  // 点击导航项滚动到对应 section
  const scrollToSection = useCallback((id: string) => {
    sectionRefs.current[id]?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, []);

  return (
    <div className="relative">
      {/* 右侧固定导航 */}
      <nav className="fixed right-6 top-1/2 -translate-y-1/2 z-50 hidden xl:flex flex-col gap-1.5 py-2 px-1.5 rounded-xl bg-background/80 backdrop-blur-sm border border-border/50 shadow-sm">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            onClick={() => scrollToSection(item.id)}
            className={`relative px-2.5 py-1.5 text-[11px] font-medium rounded-lg transition-all duration-150 ${
              activeSection === item.id
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/50"
            }`}
          >
            {item.label}
            {item.isDirty && (
              <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-red-500" />
            )}
          </button>
        ))}
      </nav>

      <div className="max-w-2xl space-y-6">
      <section className="glass-card rounded-2xl p-4 shadow-sm border border-muted/50">
        <div className="flex items-center justify-between">
          <p className="text-[11px] text-muted-foreground">
            每个设置区域修改后会显示"应用"按钮，点击后即时生效。
          </p>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-xs"
            onClick={() => reloadFromBackend(false)}
            disabled={busy === "reload"}
          >
            <RefreshCcw className="w-3 h-3 mr-1" />
            重新加载
          </Button>
        </div>
      </section>

      <GeneralSection
        ref={(el) => {
          sectionRefs.current.general = el;
        }}
        closeToTray={runtime.close_to_tray}
        onCloseToTrayChange={handleTrayToggle}
      />

      <ListenSection
        ref={(el) => {
          sectionRefs.current.listen = el;
        }}
        draft={draft}
        sectionDirty={sectionDirty.listen}
        isSaving={busy === "section_listen"}
        monitoring={runtime.tasks.monitoring}
        sidebarRunning={runtime.tasks.sidebar}
        sidebarWindowMode={sidebarWindowMode}
        onApply={() => handleApplySection("listen")}
        onReset={() => handleResetSection("listen")}
        onMonitoringToggle={handleMonitoringToggle}
        onSidebarModeChange={(mode) =>
          setUiPreferences({ sidebarWindowMode: mode })
        }
        updateDraft={updateDraft}
        monitoringBusy={busy === "monitoring"}
      />

      {sidebarWindowMode === "independent" ? (
        <DisplaySection
          ref={(el) => {
            sectionRefs.current.display = el;
          }}
          draft={draft}
          sectionDirty={sectionDirty.display}
          isSaving={busy === "section_display"}
          onApply={() => handleApplySection("display")}
          onReset={() => handleResetSection("display")}
          updateDraft={updateDraft}
        />
      ) : null}

      <TranslateSection
        ref={(el) => {
          sectionRefs.current.translate = el;
        }}
        draft={draft}
        sectionDirty={sectionDirty.translate}
        isSaving={busy === "section_translate"}
        updateDraft={updateDraft}
        onApply={() => handleApplySection("translate")}
        onReset={() => handleResetSection("translate")}
        onTranslateTest={handleTranslateTest}
        translateTestBusy={busy === "translate_test"}
        translateTestError={translateTestError}
        aiProviders={aiProviders}
        aiProvidersLoading={aiProvidersLoading}
        showApiKey={showApiKey}
        onToggleShowApiKey={() => setShowApiKey((state) => !state)}
        status={getTranslateConfigStatus()}
      />

      <DictSection
        ref={(el) => {
          sectionRefs.current.dict = el;
        }}
        draft={draft}
        updateDraft={updateDraft}
      />

      <ImageCaptureSection
        checked={draft.imageCapture}
        useRightPanelDetails={draft.useRightPanelDetails}
        onCheckedChange={(checked) =>
          updateDraft({ imageCapture: checked }, "display")
        }
      />

      <AdvancedConfigSection
        open={advancedOpen}
        busy={busy}
        configRaw={configRaw}
        configDirty={configDirty}
        configValid={configValid}
        configErrors={configErrors}
        onToggleOpen={() => setAdvancedOpen((state) => !state)}
        onConfigRawChange={handleConfigRawChange}
        onRestoreDefault={handleRestoreDefault}
        onReload={() => reloadFromBackend(false)}
        onApply={handleApplyConfig}
      />
      </div>
    </div>
  );
}
