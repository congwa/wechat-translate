import { forwardRef } from "react";
import { AlertCircle, ChevronDown, Code2 } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

interface AdvancedConfigSectionProps {
  open: boolean;
  busy: string | null;
  configRaw: string;
  configDirty: boolean;
  configValid: boolean;
  configErrors: string[];
  onToggleOpen: () => void;
  onConfigRawChange: (value: string) => void;
  onRestoreDefault: () => void;
  onReload: () => void;
  onApply: () => void;
}

export const AdvancedConfigSection = forwardRef<
  HTMLElement,
  AdvancedConfigSectionProps
>(function AdvancedConfigSection(
  {
    open,
    busy,
    configRaw,
    configDirty,
    configValid,
    configErrors,
    onToggleOpen,
    onConfigRawChange,
    onRestoreDefault,
    onReload,
    onApply,
  },
  ref,
) {
  return (
    <section ref={ref} className="glass-card rounded-2xl shadow-sm overflow-hidden">
      <button
        onClick={onToggleOpen}
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
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      <AnimatePresence>
        {open ? (
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
                onChange={(e) => onConfigRawChange(e.target.value)}
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
                {configErrors.length > 0 ? (
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
                      {configErrors.map((error, index) => (
                        <div
                          key={`${index}-${error}`}
                          className="text-xs text-red-700 dark:text-red-300"
                        >
                          {error}
                        </div>
                      ))}
                    </div>
                  </motion.div>
                ) : null}
              </AnimatePresence>

              <div className="flex flex-wrap items-center gap-2 justify-end">
                <Button
                  variant="outline"
                  onClick={onRestoreDefault}
                  disabled={busy === "restore"}
                >
                  {busy === "restore" ? "恢复中..." : "恢复默认"}
                </Button>
                <Button
                  variant="outline"
                  onClick={onReload}
                  disabled={busy === "reload"}
                >
                  {busy === "reload" ? "加载中..." : "重新加载"}
                </Button>
                <Button
                  onClick={onApply}
                  disabled={!configDirty || !configValid || busy === "apply"}
                >
                  {busy === "apply" ? "应用中..." : "应用配置"}
                </Button>
              </div>
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </section>
  );
});
