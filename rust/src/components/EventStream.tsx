import { useRef, useEffect } from "react";
import { Badge } from "@/components/ui/badge";
import { motion, AnimatePresence } from "framer-motion";
import { useEventStore } from "@/stores/eventStore";

const typeStyles: Record<string, string> = {
  status: "bg-blue-100 text-blue-700",
  message: "bg-emerald-100 text-emerald-700",
  log: "bg-slate-100 text-slate-600",
  error: "bg-red-100 text-red-700",
  task_state: "bg-violet-100 text-violet-700",
};

export function EventStream() {
  const events = useEventStore((s) => s.events);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [events.length]);

  const recent = events.slice(-120);

  return (
    <div className="glass-card rounded-2xl shadow-sm overflow-hidden">
      <div className="px-5 py-4 border-b flex items-center justify-between">
        <h3 className="text-sm font-semibold">事件流</h3>
        <Badge variant="secondary" className="text-[10px] font-normal">
          {events.length}
        </Badge>
      </div>
      <div
        ref={scrollRef}
        className="max-h-72 overflow-y-auto p-4 space-y-1 font-mono text-[11px]"
      >
        {recent.length === 0 && (
          <div className="text-center text-muted-foreground py-10 text-xs font-sans">
            启动任务后将显示实时事件
          </div>
        )}
        <AnimatePresence initial={false}>
          {recent.map((evt) => (
            <motion.div
              key={evt.id}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              className="flex items-start gap-2 py-0.5"
            >
              <span className="text-muted-foreground shrink-0 w-[120px]">
                {evt.timestamp}
              </span>
              <span
                className={`shrink-0 px-1.5 py-0.5 rounded text-[10px] font-medium ${typeStyles[evt.type] ?? "bg-gray-100 text-gray-600"}`}
              >
                {evt.type}/{evt.source}
              </span>
              <span className="text-foreground/80 break-all">
                {JSON.stringify(evt.payload)}
              </span>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}
