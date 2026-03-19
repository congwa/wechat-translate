import { useEffect, useRef, useState } from "react";
import { Bot, Send, Square, Trash2, ChevronDown, ChevronRight, Database, Info } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import {
  PromptInput,
  PromptInputTextarea,
  PromptInputActions,
  PromptInputAction,
} from "@/components/ui/prompt-input";
import {
  Message,
  MessageContent,
  MessageAvatar,
} from "@/components/ui/message";
import { useChatAgentStore } from "@/stores/chatAgentStore";
import type { AgentToolCallEvent } from "@/lib/tauri-api";
import type { ChatMessage } from "@/stores/chatAgentStore";

function ToolCallCard({ call }: { call: AgentToolCallEvent }) {
  const [open, setOpen] = useState(false);

  const toolLabel: Record<string, string> = {
    execute_sql: "执行 SQL",
    get_schema: "查询表结构",
    get_context: "获取数据库概况",
  };

  let outputPreview = call.output;
  try {
    const parsed = JSON.parse(call.output);
    if (parsed && typeof parsed === "object") {
      if (parsed.rows && Array.isArray(parsed.rows)) {
        outputPreview = `返回 ${parsed.row_count ?? parsed.rows.length} 行${parsed.truncated ? "（已截断）" : ""}`;
      } else {
        outputPreview = JSON.stringify(parsed, null, 2);
      }
    }
  } catch {
    // keep original
  }

  return (
    <div className={`mt-1 rounded-lg border text-xs overflow-hidden ${call.is_error ? "border-red-300 bg-red-50 dark:bg-red-950/20" : "border-border bg-muted/30"}`}>
      <button
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-muted/50 transition-colors"
        onClick={() => setOpen((v) => !v)}
      >
        <Database className="w-3 h-3 shrink-0 text-muted-foreground" />
        <span className="font-medium text-muted-foreground">
          {toolLabel[call.tool_name] ?? call.tool_name}
        </span>
        {call.tool_name === "execute_sql" && (() => {
          try {
            const inp = call.input as { sql?: string };
            return inp?.sql ? (
              <span className="flex-1 truncate text-muted-foreground/70 font-mono">
                {inp.sql.substring(0, 60)}{inp.sql.length > 60 ? "…" : ""}
              </span>
            ) : null;
          } catch { return null; }
        })()}
        {call.is_error && <Badge variant="destructive" className="ml-auto text-[10px] py-0 h-4">失败</Badge>}
        <span className="ml-auto shrink-0">
          {open ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        </span>
      </button>
      {open && (
        <div className="px-3 pb-2 border-t border-border/50">
          {call.tool_name === "execute_sql" && (
            <div className="mt-2">
              <p className="text-[10px] uppercase text-muted-foreground mb-1">SQL</p>
              <pre className="text-[11px] font-mono bg-background rounded p-2 overflow-x-auto whitespace-pre-wrap break-all">
                {(call.input as { sql?: string })?.sql ?? JSON.stringify(call.input)}
              </pre>
            </div>
          )}
          <div className="mt-2">
            <p className="text-[10px] uppercase text-muted-foreground mb-1">
              {call.is_error ? "错误信息" : "结果"}
            </p>
            <pre className="text-[11px] font-mono bg-background rounded p-2 overflow-x-auto max-h-48 whitespace-pre-wrap break-all">
              {outputPreview}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

function MessageBubble({ msg }: { msg: ChatMessage }) {
  const isUser = msg.role === "user";
  const isError = msg.role === "error";

  if (isUser) {
    return (
      <Message className="flex-row-reverse">
        <MessageAvatar src="" alt="You" fallback="你" />
        <div className="flex flex-col items-end gap-1 max-w-[75%]">
          <MessageContent className="bg-primary text-primary-foreground rounded-br-sm">
            {msg.content}
          </MessageContent>
        </div>
      </Message>
    );
  }

  if (isError) {
    return (
      <Message>
        <div className="w-8 h-8 shrink-0 rounded-full bg-red-100 dark:bg-red-900/30 flex items-center justify-center">
          <Info className="w-4 h-4 text-red-500" />
        </div>
        <div className="flex flex-col gap-1 max-w-[85%]">
          <MessageContent className="bg-red-50 dark:bg-red-950/20 text-red-700 dark:text-red-400 rounded-bl-sm border border-red-200 dark:border-red-800">
            {msg.content}
          </MessageContent>
        </div>
      </Message>
    );
  }

  return (
    <Message>
      <div className="w-8 h-8 shrink-0 rounded-full bg-primary/10 flex items-center justify-center">
        <Bot className="w-4 h-4 text-primary" />
      </div>
      <div className="flex flex-col gap-1 max-w-[85%]">
        {msg.toolCalls && msg.toolCalls.length > 0 && (
          <div className="space-y-1">
            {msg.toolCalls.map((call, i) => (
              <ToolCallCard key={i} call={call} />
            ))}
          </div>
        )}
        <MessageContent markdown className="rounded-bl-sm">
          {msg.content}
        </MessageContent>
      </div>
    </Message>
  );
}

function ThinkingIndicator() {
  return (
    <Message>
      <div className="w-8 h-8 shrink-0 rounded-full bg-primary/10 flex items-center justify-center">
        <Bot className="w-4 h-4 text-primary" />
      </div>
      <div className="flex items-center gap-1.5 px-3 py-2.5 rounded-lg bg-secondary">
        {[0, 1, 2].map((i) => (
          <motion.span
            key={i}
            className="w-1.5 h-1.5 rounded-full bg-muted-foreground/60"
            animate={{ scale: [1, 1.4, 1], opacity: [0.4, 1, 0.4] }}
            transition={{ duration: 1.2, repeat: Infinity, delay: i * 0.2 }}
          />
        ))}
      </div>
    </Message>
  );
}

const WELCOME_SUGGESTIONS = [
  "数据库里一共有多少条消息？",
  "最近 7 天哪个群最活跃？",
  "帮我统计各群聊的消息数量",
  "最近一周每天的消息量是多少？",
];

export function ChatAgentPage() {
  const { sessionId, messages, isLoading, initSession, sendMessage, cancelChat, clearHistory } =
    useChatAgentStore();
  const [input, setInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    initSession();
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, isLoading]);

  const handleSubmit = async () => {
    const text = input.trim();
    if (!text || isLoading) return;
    setInput("");
    await sendMessage(text);
  };

  const isEmpty = messages.length === 0;

  return (
    <div className="flex flex-col gap-4" style={{ height: "calc(100vh - 260px)" }}>
      <ScrollArea className="flex-1 rounded-xl border bg-card/50">
        <div className="p-4 space-y-4 min-h-full">
          {isEmpty && (
            <div className="flex flex-col items-center justify-center h-full pt-12 pb-6 text-center space-y-4">
              <div className="w-14 h-14 rounded-2xl bg-primary/10 flex items-center justify-center">
                <Bot className="w-7 h-7 text-primary" />
              </div>
              <div>
                <h3 className="font-semibold text-base">消息数据分析助手</h3>
                <p className="text-sm text-muted-foreground mt-1 max-w-xs">
                  用自然语言查询微信消息记录，我会自动生成 SQL 并返回分析结果
                </p>
              </div>
              <div className="grid grid-cols-2 gap-2 w-full max-w-md pt-2">
                {WELCOME_SUGGESTIONS.map((s) => (
                  <button
                    key={s}
                    onClick={() => { setInput(s); }}
                    className="text-left text-xs px-3 py-2 rounded-lg border bg-background hover:bg-muted/60 transition-colors text-muted-foreground hover:text-foreground"
                  >
                    {s}
                  </button>
                ))}
              </div>
            </div>
          )}

          {messages.map((msg) => (
            <motion.div
              key={msg.id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.2 }}
            >
              <MessageBubble msg={msg} />
            </motion.div>
          ))}

          <AnimatePresence>
            {isLoading && (
              <motion.div
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
              >
                <ThinkingIndicator />
              </motion.div>
            )}
          </AnimatePresence>

          <div ref={bottomRef} />
        </div>
      </ScrollArea>

      <div className="shrink-0">
        <PromptInput
          value={input}
          onValueChange={setInput}
          onSubmit={handleSubmit}
          isLoading={isLoading}
          disabled={!sessionId}
          className="w-full"
        >
          <PromptInputTextarea
            placeholder={isLoading ? "Agent 正在思考中..." : "输入问题，例如：最近哪个群最活跃？"}
          />
          <PromptInputActions className="justify-between px-1 pb-1">
            <PromptInputAction tooltip="清空对话历史">
              <Button
                variant="ghost"
                size="sm"
                className="h-8 px-2 text-muted-foreground hover:text-foreground"
                onClick={clearHistory}
                disabled={isLoading || messages.length === 0}
              >
                <Trash2 className="w-4 h-4" />
              </Button>
            </PromptInputAction>
            {isLoading ? (
              <PromptInputAction tooltip="取消">
                <Button
                  size="sm"
                  variant="destructive"
                  className="h-8 px-3 gap-1.5"
                  onClick={cancelChat}
                >
                  <Square className="w-3 h-3" />
                  取消
                </Button>
              </PromptInputAction>
            ) : (
              <PromptInputAction tooltip="发送（Enter）">
                <Button
                  size="sm"
                  className="h-8 px-3 gap-1.5"
                  onClick={handleSubmit}
                  disabled={!input.trim() || !sessionId}
                >
                  <Send className="w-3.5 h-3.5" />
                  发送
                </Button>
              </PromptInputAction>
            )}
          </PromptInputActions>
        </PromptInput>
        <p className="text-[11px] text-muted-foreground/60 text-center mt-1.5">
          只读查询 · 数据不会被修改 · Enter 发送 · Shift+Enter 换行
        </p>
      </div>
    </div>
  );
}
