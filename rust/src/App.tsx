import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Settings,
  Database,
  Radio,
  ChevronRight,
  MonitorPlay,
  Square,
} from "lucide-react";
import { PreflightBar } from "@/components/PreflightBar";
import { SettingsPage } from "@/components/SettingsPage";
import { EventStream } from "@/components/EventStream";
import { ServiceLogs } from "@/components/ServiceLogs";
import { MessageHistory } from "@/components/MessageHistory";
import { SidebarView } from "@/components/SidebarView";
import { useEventStore } from "@/stores/eventStore";
import { useToastStore } from "@/stores/toastStore";
import { useFormStore } from "@/stores/formStore";
import * as api from "@/lib/tauri-api";
import type { TaskState } from "@/lib/types";

const isSidebarView =
  new URLSearchParams(window.location.search).get("view") === "sidebar";

export default function App() {
  if (isSidebarView) {
    return <SidebarView />;
  }
  return <MainApp />;
}

type PageKey = "settings" | "history" | "logs";

interface NavItem {
  key: PageKey;
  label: string;
  icon: React.ReactNode;
  taskKey?: keyof TaskState;
  beta?: boolean;
}

const NAV_ITEMS: NavItem[] = [
  { key: "settings", label: "设置", icon: <Settings className="w-[18px] h-[18px]" /> },
  { key: "history", label: "历史", icon: <Database className="w-[18px] h-[18px]" /> },
  { key: "logs", label: "日志", icon: <Radio className="w-[18px] h-[18px]" /> },
];

function MainApp() {
  const [page, setPage] = useState<PageKey>("settings");
  const [liveBusy, setLiveBusy] = useState(false);
  const toast = useToastStore((s) => s.toast);
  const showToast = useToastStore((s) => s.showToast);
  const taskState = useEventStore((s) => s.taskState);
  const closeToTray = useFormStore((s) => s.closeToTray);

  useEffect(() => {
    const cleanup = useEventStore.getState().initEventListener();

    api
      .getTaskStatus()
      .then((resp) => {
        const r = resp as unknown as Record<string, unknown>;
        const data = r.data as { tasks: TaskState } | undefined;
        if (data?.tasks) {
          useEventStore.getState().setTaskState(data.tasks);
        }
      })
      .catch(() => {});

    api.setCloseToTray(closeToTray).catch(() => {});

    return () => {
      cleanup.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handleLiveToggle() {
    if (liveBusy) return;
    setLiveBusy(true);

    try {
      if (taskState.sidebar) {
        await api.sidebarStop();
        showToast("浮窗已关闭", true);
      } else {
        const store = useFormStore.getState();
        const resp = (await api.configGet()) as unknown as Record<string, unknown>;
        const data = (resp.data ?? {}) as Record<string, unknown>;
        const translate = (data.translate ?? {}) as Record<string, unknown>;
        const savedUrl = typeof translate.deeplx_url === "string" ? translate.deeplx_url : store.deeplxUrl;
        const savedEnabled = typeof translate.enabled === "boolean" ? translate.enabled : store.translateEnabled;
        const savedSource = typeof translate.source_lang === "string" ? translate.source_lang : store.sourceLang;
        const savedTarget = typeof translate.target_lang === "string" ? translate.target_lang : store.targetLang;
        const fullUrl = savedUrl.trim();
        await api.liveStart({
          translateEnabled: savedEnabled,
          deeplxUrl: fullUrl,
          sourceLang: savedSource,
          targetLang: savedTarget,
          intervalSeconds: parseFloat(store.pollInterval) || 1,
          imageCapture: store.imageCapture,
          windowMode: store.sidebarWindowMode,
        });
        showToast("实时浮窗已开启", true);
      }
    } catch (e) {
      showToast(`${e}`, false);
    } finally {
      setLiveBusy(false);
    }
  }

  const sidebarRunning = taskState.sidebar;

  return (
    <div className="h-screen flex overflow-hidden bg-background">
      {/* Sidebar */}
      <aside
        className="w-[200px] shrink-0 flex flex-col"
        style={{
          background: "var(--color-sidebar-bg)",
          color: "var(--color-sidebar-foreground)",
        }}
      >
        {/* Brand */}
        <div className="px-5 pt-6 pb-3">
          <h1 className="text-[15px] font-semibold text-white tracking-tight leading-tight">
            WeChat Auto
          </h1>
          <p className="text-[11px] mt-1 opacity-50">macOS · Rust + Tauri</p>
        </div>

        {/* Live toggle button */}
        {taskState.monitoring && (
          <div className="px-3 pb-4">
            <button
              onClick={handleLiveToggle}
              disabled={liveBusy}
              className={`w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl text-[13px] font-semibold transition-all duration-200 ${
                sidebarRunning
                  ? "bg-red-500/90 hover:bg-red-500 text-white"
                  : "bg-emerald-500/90 hover:bg-emerald-500 text-white"
              } ${liveBusy ? "opacity-60" : ""}`}
            >
              {sidebarRunning ? (
                <>
                  <Square className="w-4 h-4" />
                  {liveBusy ? "关闭中..." : "关闭浮窗"}
                </>
              ) : (
                <>
                  <MonitorPlay className="w-4 h-4" />
                  {liveBusy ? "启动中..." : "开启实时浮窗"}
                </>
              )}
            </button>
            {sidebarRunning && (
              <div className="flex items-center justify-center gap-1.5 mt-2">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                <span className="text-[10px] text-emerald-400">运行中</span>
              </div>
            )}
          </div>
        )}

        {/* Navigation */}
        <nav className="flex-1 px-3 space-y-0.5">
          {NAV_ITEMS.map((item) => {
            const active = page === item.key;
            const running = item.taskKey && taskState[item.taskKey];
            return (
              <button
                key={item.key}
                onClick={() => setPage(item.key)}
                className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-[13px] font-medium transition-all duration-150 group relative ${
                  active ? "text-white" : "hover:text-white/90"
                }`}
                style={active ? { background: "var(--color-sidebar-hover)" } : undefined}
              >
                {active && (
                  <motion.div
                    layoutId="nav-active"
                    className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 rounded-r-full"
                    style={{ background: "var(--color-sidebar-active)" }}
                    transition={{ type: "spring", stiffness: 400, damping: 30 }}
                  />
                )}
                <span className={active ? "text-white" : "opacity-60 group-hover:opacity-90"}>
                  {item.icon}
                </span>
                <span className="flex-1 text-left">{item.label}</span>
                {item.beta && (
                  <span className="px-1.5 py-0.5 rounded bg-amber-400/20 text-amber-300 text-[9px] font-bold uppercase leading-none">
                    Beta
                  </span>
                )}
                {running && (
                  <span
                    className="w-2 h-2 rounded-full animate-pulse"
                    style={{ background: "var(--color-sidebar-active)" }}
                  />
                )}
                {active && <ChevronRight className="w-3.5 h-3.5 opacity-40" />}
              </button>
            );
          })}
        </nav>

      </aside>

      {/* Main content */}
      <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Top bar */}
        <div className="shrink-0 px-6 pt-5 pb-3 flex items-center justify-between">
          <h2 className="text-lg font-semibold tracking-tight">
            {NAV_ITEMS.find((n) => n.key === page)?.label}
          </h2>
          <div className="flex items-center gap-2">
            {(["monitoring", "sidebar"] as const).map((key) => (
              <span
                key={key}
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium transition-colors ${
                  taskState[key]
                    ? "bg-primary/10 text-primary"
                    : "bg-muted text-muted-foreground"
                }`}
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full ${
                    taskState[key] ? "bg-primary animate-pulse" : "bg-muted-foreground/30"
                  }`}
                />
                {key === "sidebar" ? "浮窗" : "监听"}
              </span>
            ))}
          </div>
        </div>

        {/* Preflight + Toast */}
        <div className="shrink-0 px-6">
          <PreflightBar />
          <AnimatePresence>
            {toast && (
              <motion.div
                initial={{ opacity: 0, y: -8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -8 }}
                className={`mb-3 px-4 py-2.5 rounded-xl text-sm font-medium shadow-sm ${
                  toast.ok
                    ? "bg-primary/10 text-primary border border-primary/20"
                    : "bg-destructive/10 text-destructive border border-destructive/20"
                }`}
              >
                {toast.text}
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* Page content */}
        <div className="flex-1 overflow-y-auto px-6 pb-6">
          <AnimatePresence mode="wait">
            <motion.div
              key={page}
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -12 }}
              transition={{ duration: 0.18 }}
            >
              {page === "settings" && <SettingsPage />}
              {page === "history" && <MessageHistory />}
              {page === "logs" && <LogsPage />}
            </motion.div>
          </AnimatePresence>
        </div>
      </main>
    </div>
  );
}

function LogsPage() {
  return (
    <div className="space-y-4">
      <EventStream />
      <ServiceLogs />
    </div>
  );
}
