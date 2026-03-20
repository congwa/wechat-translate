import { forwardRef } from "react";
import { Volume2 } from "lucide-react";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { SettingsSection } from "@/components/SettingsSection";
import type { SettingsDraft } from "@/stores/settingsStore";

interface TtsSectionProps {
  draft: SettingsDraft;
  sectionDirty: boolean;
  isSaving: boolean;
  onApply: () => void;
  onReset: () => void;
  updateDraft: (updates: Partial<SettingsDraft>) => void;
}

export const TtsSection = forwardRef<HTMLElement, TtsSectionProps>(
  function TtsSection(
    { draft, sectionDirty, isSaving, onApply, onReset, updateDraft },
    ref
  ) {
    return (
      <SettingsSection
        id="tts"
        ref={ref}
        icon={<Volume2 className="w-4 h-4 text-violet-600" />}
        iconBg="bg-violet-50"
        title="自动朗读 (TTS)"
        description="新消息到达时自动语音播报"
        isDirty={sectionDirty}
        isSaving={isSaving}
        onApply={onApply}
        onReset={onReset}
      >
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-sm font-medium">启用自动朗读</Label>
            <p className="text-[11px] text-muted-foreground">
              浮窗收到新消息时自动朗读，正在朗读的消息会有动画提示
            </p>
          </div>
          <Switch
            checked={draft.ttsEnabled}
            onCheckedChange={(checked) => updateDraft({ ttsEnabled: checked })}
          />
        </div>

        <div className="pt-2 border-t border-border/40 space-y-1.5">
          <Label className="text-xs text-muted-foreground">语言策略</Label>
          <div className="space-y-1 text-[11px] text-muted-foreground leading-relaxed">
            <p>• <span className="font-medium text-foreground/70">纯英文消息</span>：直接朗读原文</p>
            <p>• <span className="font-medium text-foreground/70">中文 / 中英混排（目标语言 EN）</span>：优先朗读翻译后的英文</p>
            <p>• <span className="font-medium text-foreground/70">中文 / 中英混排（目标语言 ZH）</span>：朗读中文原文</p>
          </div>
        </div>

        <div className="pt-2 border-t border-border/40">
          <p className="text-[11px] text-muted-foreground">
            使用系统内置语音引擎（macOS AVFoundation），无需额外权限
          </p>
        </div>
      </SettingsSection>
    );
  }
);
