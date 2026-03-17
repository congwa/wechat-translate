import { useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { AlertTriangle, XCircle, ExternalLink } from "lucide-react";
import { usePreflightStore } from "@/stores/preflightStore";

interface BannerAction {
  label: string;
  onClick: () => void;
  external?: boolean;
}

interface BannerItem {
  text: string;
  level: "warn" | "error";
  actions?: BannerAction[];
}

export function PreflightBar() {
  const result = usePreflightStore((s) => s.result);
  const check = usePreflightStore((s) => s.check);
  const promptingAccessibility = usePreflightStore((s) => s.promptingAccessibility);
  const awaitingUserAction = usePreflightStore((s) => s.awaitingUserAction);
  const settingsOpened = usePreflightStore((s) => s.settingsOpened);
  const recoveringListener = usePreflightStore((s) => s.recoveringListener);
  const recoveryFailed = usePreflightStore((s) => s.recoveryFailed);
  const openAccessibilitySettings = usePreflightStore((s) => s.openAccessibilitySettings);
  const retryAccessibilityCheck = usePreflightStore((s) => s.retryAccessibilityCheck);
  const recoverListener = usePreflightStore((s) => s.recoverListener);
  const promptRestartFallback = usePreflightStore((s) => s.promptRestartFallback);

  useEffect(() => {
    check();
    const timer = setInterval(check, 5000);
    return () => clearInterval(timer);
  }, [check]);

  if (!result) return null;

  const wechatAccessible = result.wechat_accessible ?? result.accessibility_ok;
  const allGood =
    result.wechat_running &&
    result.accessibility_ok &&
    wechatAccessible &&
    result.wechat_has_window;

  if (allGood && !recoveringListener && !recoveryFailed) {
    return null;
  }

  const items: BannerItem[] = [];

  if (recoveringListener) {
    items.push({
      text: "已获得辅助功能权限，正在重建监听...",
      level: "warn",
    });
  } else if (recoveryFailed) {
    items.push({
      text: "权限已获得，但监听恢复失败，可重新初始化；若仍失败再重启应用",
      level: "error",
      actions: [
        {
          label: "重新初始化监听",
          onClick: recoverListener,
        },
        {
          label: "重启应用",
          onClick: promptRestartFallback,
        },
      ],
    });
  } else if (!result.wechat_running) {
    items.push({ text: "请先启动微信并登录", level: "warn" });
  } else if (!result.accessibility_ok) {
    if (promptingAccessibility) {
      items.push({
        text: "正在请求辅助功能权限...",
        level: "warn",
      });
    } else if (awaitingUserAction) {
      items.push({
        text: "请在刚刚打开的系统设置页面中勾选本应用",
        level: "error",
        actions: [
          {
            label: "重新检测",
            onClick: retryAccessibilityCheck,
          },
          {
            label: settingsOpened ? "再次打开设置页" : "打开系统设置",
            onClick: openAccessibilitySettings,
            external: true,
          },
        ],
      });
    } else {
      items.push({
        text: "正在准备辅助功能授权引导...",
        level: "warn",
      });
    }
  } else if (!wechatAccessible) {
    items.push({
      text: "辅助功能已授权，但当前仍读不到微信窗口，正在等待监听恢复",
      level: "warn",
    });
  } else if (!result.wechat_has_window) {
    items.push({
      text: "微信已运行但未检测到窗口，请确认已登录并打开主窗口",
      level: "warn",
    });
  }

  return (
    <AnimatePresence>
      {items.map((item, index) => (
        <motion.div
          key={index}
          initial={{ opacity: 0, height: 0 }}
          animate={{ opacity: 1, height: "auto" }}
          exit={{ opacity: 0, height: 0 }}
          className={`mb-3 px-4 py-2.5 rounded-lg text-sm font-medium flex items-center gap-2 ${
            item.level === "error"
              ? "bg-red-50 text-red-800 border border-red-200"
              : "bg-amber-50 text-amber-800 border border-amber-200"
          }`}
        >
          {item.level === "error" ? (
            <XCircle className="w-4 h-4 shrink-0" />
          ) : (
            <AlertTriangle className="w-4 h-4 shrink-0" />
          )}
          <span className="flex-1">{item.text}</span>
          {item.actions?.map((action) => (
            <button
              key={action.label}
              onClick={action.onClick}
              className="flex items-center gap-1 text-xs underline underline-offset-2 hover:opacity-80 shrink-0"
            >
              {action.label}
              {action.external ? <ExternalLink className="w-3 h-3" /> : null}
            </button>
          ))}
        </motion.div>
      ))}
    </AnimatePresence>
  );
}
