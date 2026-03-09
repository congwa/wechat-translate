import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { motion, AnimatePresence } from "framer-motion";
import {
  Send,
  FileUp,
  RefreshCw,
  MessageSquareText,
  Loader2,
  User,
} from "lucide-react";
import * as api from "@/lib/tauri-api";
import { useToastStore } from "@/stores/toastStore";
import { useFormStore } from "@/stores/formStore";

export function SendCenter() {
  const showToast = useToastStore((s) => s.showToast);
  const who = useFormStore((s) => s.lastChatName);
  const setLastChatName = useFormStore((s) => s.setLastChatName);
  const [text, setText] = useState("");
  const [files, setFiles] = useState("");
  const [sessions, setSessions] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState("text");

  useEffect(() => {
    loadSessions();
  }, []);

  async function loadSessions() {
    try {
      const resp = await api.getSessions();
      const data = (resp as unknown as Record<string, unknown>).data as
        | string[]
        | undefined;
      setSessions(data ?? []);
    } catch {
      // silently fail
    }
  }

  async function handleSendText() {
    setBusy("send-text");
    try {
      await api.sendText(who || "文件传输助手", text);
      showToast("文本发送成功", true);
      setText("");
    } catch (e) {
      showToast(`${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  async function handleSendFiles() {
    setBusy("send-files");
    try {
      const paths = files
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean);
      await api.sendFiles(who || "文件传输助手", paths);
      showToast("文件发送成功", true);
      setFiles("");
    } catch (e) {
      showToast(`${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  async function handleRefreshSessions() {
    setBusy("refresh");
    try {
      const resp = await api.getSessions();
      const data = (resp as unknown as Record<string, unknown>).data as
        | string[]
        | undefined;
      setSessions(data ?? []);
      showToast(`获取到 ${data?.length ?? 0} 个会话`, true);
    } catch (e) {
      showToast(`${e}`, false);
    } finally {
      setBusy(null);
    }
  }

  const isSending =
    activeTab === "text" ? busy === "send-text" : busy === "send-files";
  const canSend =
    activeTab === "text" ? text.trim().length > 0 : files.trim().length > 0;

  function handleSend() {
    if (activeTab === "text") handleSendText();
    else handleSendFiles();
  }

  return (
    <div className="max-w-2xl">
      <div className="glass-card rounded-2xl shadow-sm overflow-hidden">
        {/* --- Session selector header --- */}
        <div className="px-6 pt-5 pb-4 space-y-3">
          <div className="flex items-center gap-3">
            <div className="flex items-center justify-center size-8 rounded-lg bg-primary/10 text-primary shrink-0">
              <User className="size-4" />
            </div>
            <div className="flex-1 min-w-0">
              <Input
                placeholder="输入会话名称，如「文件传输助手」"
                value={who}
                onChange={(e) => setLastChatName(e.target.value)}
                className="h-9 bg-background border-border/60 focus-visible:border-primary/50 focus-visible:ring-primary/20"
              />
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="shrink-0 size-9 rounded-lg text-muted-foreground hover:text-primary"
              onClick={handleRefreshSessions}
              disabled={busy === "refresh"}
              title="刷新会话列表"
            >
              <RefreshCw
                className={`size-4 ${busy === "refresh" ? "animate-spin" : ""}`}
              />
            </Button>
          </div>

          <AnimatePresence>
            {sessions.length > 0 && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
                className="overflow-hidden"
              >
                <div className="flex flex-wrap gap-1.5 max-h-24 overflow-y-auto py-1 sidebar-scroll">
                  {sessions.map((s) => (
                    <button
                      key={s}
                      className={`px-2.5 py-1 rounded-md text-xs transition-all cursor-pointer ${
                        who === s
                          ? "bg-primary/12 text-primary font-medium ring-1 ring-primary/25"
                          : "bg-secondary/50 text-muted-foreground hover:bg-secondary hover:text-foreground"
                      }`}
                      onClick={() => setLastChatName(s)}
                    >
                      {s}
                    </button>
                  ))}
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* --- Divider --- */}
        <div className="border-t border-border/40" />

        {/* --- Tabs composer --- */}
        <div className="px-6 pt-4 pb-5">
          <Tabs
            value={activeTab}
            onValueChange={setActiveTab}
            className="w-full"
          >
            <TabsList className="w-full mb-4">
              <TabsTrigger value="text" className="flex-1 gap-1.5">
                <MessageSquareText className="size-3.5" />
                文本
              </TabsTrigger>
              <TabsTrigger value="file" className="flex-1 gap-1.5">
                <FileUp className="size-3.5" />
                文件
              </TabsTrigger>
            </TabsList>

            <TabsContent value="text">
              <Textarea
                placeholder="在这里输入要发送的文本内容…"
                rows={5}
                value={text}
                onChange={(e) => setText(e.target.value)}
                className="resize-none bg-background border-border/60 focus-visible:border-primary/50 focus-visible:ring-primary/20"
              />
            </TabsContent>

            <TabsContent value="file">
              <Textarea
                placeholder={"每行输入一个文件的完整路径，例如：\n/Users/me/Documents/report.pdf"}
                rows={5}
                value={files}
                onChange={(e) => setFiles(e.target.value)}
                className="resize-none font-mono text-[13px] bg-background border-border/60 focus-visible:border-primary/50 focus-visible:ring-primary/20"
              />
            </TabsContent>
          </Tabs>

          {/* --- Send button --- */}
          <Button
            className="w-full h-10 mt-4 rounded-xl font-medium text-sm gap-2 transition-all"
            onClick={handleSend}
            disabled={!canSend || isSending}
          >
            {isSending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : activeTab === "text" ? (
              <Send className="size-4" />
            ) : (
              <FileUp className="size-4" />
            )}
            {isSending
              ? "发送中…"
              : activeTab === "text"
                ? `发送文本${who ? ` 给 ${who}` : ""}`
                : `发送文件${who ? ` 给 ${who}` : ""}`}
          </Button>
        </div>
      </div>
    </div>
  );
}
