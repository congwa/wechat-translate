import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "@/lib/tauri-api";
import {
  BUILTIN_PROVIDERS,
  fetchProviders,
  getApiUrlForProvider,
} from "@/lib/models-registry";
import { useToastStore } from "@/stores/toastStore";
import { useRuntimeStore } from "@/stores/runtimeStore";
import {
  settingsFromDraft,
  useSettingsStore,
} from "@/stores/settingsStore";
import {
  useUiPreferencesStore,
} from "@/stores/uiPreferencesStore";
import {
  useSettingsDraftStore,
  type SettingsDraftSection,
} from "@/features/settings/draft-store";
import { validateConfigSchema } from "@/components/settings/validation";
import type { AppSettings } from "@/lib/types";

export function useSettingsController() {
  const showToast = useToastStore((s) => s.showToast);
  const runtime = useRuntimeStore((s) => s.runtime);
  const applySettingsSnapshot = useSettingsStore((s) => s.applySnapshot);
  const applyRuntimeSnapshot = useRuntimeStore((s) => s.applySnapshot);
  const settingsSnapshot = useSettingsStore((s) => s.snapshot);
  const settings = useSettingsStore((s) => s.settings);

  const sidebarWindowMode = useUiPreferencesStore((s) => s.sidebarWindowMode);
  const setUiPreferences = useUiPreferencesStore((s) => s.setPreferences);

  const draft = useSettingsDraftStore((s) => s.draft);
  const sectionDirty = useSettingsDraftStore((s) => s.sectionDirty);
  const advancedEditor = useSettingsDraftStore((s) => s.advancedEditor);
  const aiRegistry = useSettingsDraftStore((s) => s.aiRegistry);
  const updateDraft = useSettingsDraftStore((s) => s.updateDraft);
  const resetFromSettings = useSettingsDraftStore((s) => s.resetFromSettings);
  const resetSection = useSettingsDraftStore((s) => s.resetSection);
  const markSectionClean = useSettingsDraftStore((s) => s.markSectionClean);
  const setAdvancedEditor = useSettingsDraftStore((s) => s.setAdvancedEditor);
  const setAiRegistry = useSettingsDraftStore((s) => s.setAiRegistry);

  const [busy, setBusy] = useState<string | null>(null);
  const [translateTestError, setTranslateTestError] = useState<string | null>(
    null,
  );
  const [activeSection, setActiveSection] = useState("general");
  const sectionRefs = useRef<Record<string, HTMLElement | null>>({});
  const validateTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );

  useEffect(() => {
    if (!settings) return;
    resetFromSettings(settings);
  }, [resetFromSettings, settingsSnapshot?.version, settings]);

  useEffect(() => {
    let cancelled = false;
    setAiRegistry({ loading: true });
    fetchProviders()
      .then((providers) => {
        if (!cancelled) {
          setAiRegistry({
            providers: providers.length > 0 ? providers : BUILTIN_PROVIDERS,
          });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAiRegistry({ providers: BUILTIN_PROVIDERS });
        }
      })
      .finally(() => {
        if (!cancelled) {
          setAiRegistry({ loading: false });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [setAiRegistry]);

  useEffect(() => {
    if (draft.translateProvider === "ai" && draft.aiProviderId && !draft.aiBaseUrl) {
      const apiUrl = getApiUrlForProvider(
        aiRegistry.providers,
        draft.aiProviderId,
      );
      if (apiUrl) {
        updateDraft({ aiBaseUrl: apiUrl });
      }
    }
  }, [
    aiRegistry.providers,
    draft.aiBaseUrl,
    draft.aiProviderId,
    draft.translateProvider,
    updateDraft,
  ]);

  useEffect(() => {
    if (!advancedEditor.open || !settings || advancedEditor.dirty) return;
    const raw = JSON.stringify(settings, null, 2);
    setAdvancedEditor({
      raw,
      lastLoadedRaw: raw,
      errors: [],
      valid: true,
    });
  }, [
    advancedEditor.dirty,
    advancedEditor.open,
    settings,
    setAdvancedEditor,
  ]);

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting && entry.intersectionRatio >= 0.3) {
            setActiveSection(entry.target.id);
          }
        });
      },
      { threshold: [0.3, 0.5, 0.7], rootMargin: "-100px 0px -50% 0px" },
    );

    Object.values(sectionRefs.current).forEach((element) => {
      if (element) observer.observe(element);
    });

    return () => observer.disconnect();
  }, [sidebarWindowMode]);

  const scrollToSection = useCallback((id: string) => {
    sectionRefs.current[id]?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });
  }, []);

  const getTranslateConfigStatus = useCallback(() => {
    if (!draft.translateEnabled) {
      return {
        tone: "warn" as const,
        title: "翻译已禁用",
        detail: "配置仍会保留，但当前不会调用翻译接口。",
      };
    }

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
  }, [draft]);

  const reloadFromBackend = useCallback(
    async (silent = false) => {
      setBusy("reload");
      try {
        const resp = await api.appStateGet();
        if (resp.data) {
          applySettingsSnapshot(resp.data.settings);
          applyRuntimeSnapshot(resp.data.runtime);
          if (advancedEditor.open) {
            const raw = JSON.stringify(resp.data.settings.data, null, 2);
            setAdvancedEditor({
              raw,
              lastLoadedRaw: raw,
              dirty: false,
              errors: [],
              valid: true,
            });
          }
          if (!silent) showToast("已从后端重新加载配置", true);
        }
      } catch (error) {
        if (!silent) showToast(`加载配置失败: ${error}`, false);
      } finally {
        setBusy(null);
      }
    },
    [
      advancedEditor.open,
      applyRuntimeSnapshot,
      applySettingsSnapshot,
      setAdvancedEditor,
      showToast,
    ],
  );

  const applySettings = useCallback(
    async (nextSettings: AppSettings, successMessage: string) => {
      const resp = await api.settingsUpdate(nextSettings);
      if (!resp.ok) {
        const errors = resp.errors ?? ["未知错误"];
        setAdvancedEditor({ errors, valid: false });
        showToast(errors[0] ?? "配置校验失败", false);
        return false;
      }

      showToast(successMessage, true);
      return true;
    },
    [setAdvancedEditor, showToast],
  );

  const handleApplySection = useCallback(
    async (section: SettingsDraftSection) => {
      if (!settings) return;
      const sectionLabels = {
        listen: "监听设置",
        translate: "翻译设置",
        display: "显示设置",
      };
      setBusy(`section_${section}`);
      try {
        const nextSettings = settingsFromDraft(settings, draft);
        const success = await applySettings(
          nextSettings,
          `${sectionLabels[section]}已应用`,
        );
        if (success) {
          markSectionClean(section);
        }
      } catch (error) {
        showToast(`应用失败: ${error}`, false);
      } finally {
        setBusy(null);
      }
    },
    [applySettings, draft, markSectionClean, settings, showToast],
  );

  const handleResetSection = useCallback(
    (section: SettingsDraftSection) => {
      if (!settings) return;
      resetSection(section, settings);
      const sectionLabels = {
        listen: "监听设置",
        translate: "翻译设置",
        display: "显示设置",
      };
      showToast(`${sectionLabels[section]}已撤销`, true);
    },
    [resetSection, settings, showToast],
  );

  const handleApplyConfig = useCallback(async () => {
    setBusy("apply");
    try {
      const parsed = JSON.parse(advancedEditor.raw) as AppSettings;
      await applySettings(parsed, "配置已应用");
    } catch (error) {
      showToast(`应用失败: ${error}`, false);
    } finally {
      setBusy(null);
    }
  }, [advancedEditor.raw, applySettings, showToast]);

  const handleRestoreDefault = useCallback(async () => {
    setBusy("restore");
    try {
      const resp = await api.configDefault();
      if (!resp.data) return;
      const raw = JSON.stringify(resp.data, null, 2);
      setAdvancedEditor({
        raw,
        dirty: true,
        errors: [],
        valid: true,
      });
      showToast("已恢复为默认配置（点击应用生效）", true);
    } catch (error) {
      showToast(`恢复默认失败: ${error}`, false);
    } finally {
      setBusy(null);
    }
  }, [setAdvancedEditor, showToast]);

  const handleConfigRawChange = useCallback(
    (value: string) => {
      setAdvancedEditor({
        raw: value,
        dirty: value !== advancedEditor.lastLoadedRaw,
      });

      if (validateTimerRef.current) clearTimeout(validateTimerRef.current);
      validateTimerRef.current = setTimeout(() => {
        try {
          const parsed = JSON.parse(value);
          const result = validateConfigSchema(parsed);
          setAdvancedEditor({
            errors: result.errors,
            valid: result.valid,
          });
        } catch (error) {
          const message =
            error instanceof Error ? error.message : String(error);
          setAdvancedEditor({
            errors: [`JSON 语法错误: ${message}`],
            valid: false,
          });
        }
      }, 300);
    },
    [advancedEditor.lastLoadedRaw, setAdvancedEditor],
  );

  const handleTrayToggle = useCallback((checked: boolean) => {
    api.setCloseToTray(checked).catch(() => {});
  }, []);

  const handleMonitoringToggle = useCallback(
    async (checked: boolean) => {
      setBusy("monitoring");
      try {
        const interval = settings?.listen.interval_seconds ?? 1;
        if (checked) {
          await api.listenStart(interval);
          showToast(
            runtime.tasks.sidebar
              ? "消息监听已恢复，浮窗继续运行"
              : "消息监听已启动",
            true,
          );
        } else {
          await api.listenStop();
          showToast(
            runtime.tasks.sidebar
              ? "消息监听已暂停，浮窗保持运行"
              : "消息监听已暂停",
            true,
          );
        }
      } catch (error) {
        showToast(`${error}`, false);
      } finally {
        setBusy(null);
      }
    },
    [runtime.tasks.sidebar, settings?.listen.interval_seconds, showToast],
  );

  const handleTranslateTest = useCallback(async () => {
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
    } catch (error) {
      setTranslateTestError(`${error}`);
    } finally {
      setBusy(null);
    }
  }, [draft, showToast]);

  const navItems = [
    { id: "general", label: "通用", isDirty: false },
    { id: "listen", label: "监听", isDirty: sectionDirty.listen },
    ...(sidebarWindowMode === "independent"
      ? [{ id: "display", label: "浮窗", isDirty: sectionDirty.display }]
      : []),
    { id: "translate", label: "翻译", isDirty: sectionDirty.translate },
    { id: "dict", label: "词典", isDirty: false },
  ];

  return {
    runtime,
    settings,
    draft,
    sectionDirty,
    busy,
    translateTestError,
    activeSection,
    sectionRefs,
    sidebarWindowMode,
    aiProviders: aiRegistry.providers,
    aiProvidersLoading: aiRegistry.loading,
    showApiKey: aiRegistry.showApiKey,
    advancedOpen: advancedEditor.open,
    configRaw: advancedEditor.raw,
    configErrors: advancedEditor.errors,
    configValid: advancedEditor.valid,
    configDirty: advancedEditor.dirty,
    navItems,
    updateDraft,
    setUiPreferences,
    scrollToSection,
    getTranslateConfigStatus,
    reloadFromBackend,
    handleApplySection,
    handleResetSection,
    handleApplyConfig,
    handleRestoreDefault,
    handleConfigRawChange,
    handleTrayToggle,
    handleMonitoringToggle,
    handleTranslateTest,
    toggleAdvancedOpen: () =>
      setAdvancedEditor({ open: !advancedEditor.open }),
    toggleShowApiKey: () =>
      setAiRegistry({ showApiKey: !aiRegistry.showApiKey }),
  };
}
