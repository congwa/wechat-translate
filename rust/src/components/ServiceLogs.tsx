import { useRef, useEffect } from "react";
import { Badge } from "@/components/ui/badge";
import { useEventStore } from "@/stores/eventStore";

export function ServiceLogs() {
  const events = useEventStore((s) => s.events);
  const scrollRef = useRef<HTMLDivElement>(null);

  const logEvents = events.filter(
    (e) => e.type === "log" || e.type === "error" || e.type === "status",
  );
  const recent = logEvents.slice(-200);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [recent.length]);

  return (
    <div className="glass-card rounded-2xl shadow-sm overflow-hidden">
      <div className="px-5 py-4 border-b flex items-center justify-between">
        <h3 className="text-sm font-semibold">服务日志</h3>
        <Badge variant="secondary" className="text-[10px] font-normal">
          {logEvents.length}
        </Badge>
      </div>
      <div
        ref={scrollRef}
        className="max-h-72 overflow-y-auto p-4 font-mono text-[11px] space-y-0.5"
      >
        {recent.length === 0 && (
          <p className="text-muted-foreground text-xs py-10 text-center font-sans">
            暂无日志
          </p>
        )}
        {recent.map((evt) => (
          <div key={evt.id} className="flex items-start gap-2 py-0.5">
            <span className="text-muted-foreground shrink-0 w-[120px]">
              {evt.timestamp}
            </span>
            <span
              className={`${
                evt.type === "error" ? "text-destructive" : "text-foreground/80"
              } break-all`}
            >
              [{evt.source}] {JSON.stringify(evt.payload)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
