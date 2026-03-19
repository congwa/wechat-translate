import { forwardRef } from "react";
import {
  Check,
  ChevronsUpDown,
  Database,
  Eye,
  EyeOff,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SettingsSection } from "@/components/SettingsSection";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import type { ProviderInfo } from "@/lib/models-registry";
import { getModelsForProvider } from "@/lib/models-registry";
import type { SettingsDraft } from "@/stores/settingsStore";

interface AgentSectionProps {
  draft: SettingsDraft;
  sectionDirty: boolean;
  isSaving: boolean;
  updateDraft: (patch: Partial<SettingsDraft>) => void;
  onApply: () => void;
  onReset: () => void;
  aiProviders: ProviderInfo[];
  aiProvidersLoading: boolean;
  showApiKey: boolean;
  onToggleShowApiKey: () => void;
  hasFallback: boolean;
}

export const AgentSection = forwardRef<HTMLElement, AgentSectionProps>(
  function AgentSection(
    {
      draft,
      sectionDirty,
      isSaving,
      updateDraft,
      onApply,
      onReset,
      aiProviders,
      aiProvidersLoading,
      showApiKey,
      onToggleShowApiKey,
      hasFallback,
    },
    ref,
  ) {
    const isConfigured =
      !!draft.agentAiApiKey.trim() && !!draft.agentAiModelId.trim();

    return (
      <SettingsSection
        id="agent"
        ref={ref}
        icon={<Database className="w-4 h-4 text-cyan-600" />}
        iconBg="bg-cyan-50"
        title="数据问答设置"
        description="配置数据问答 Agent 专用的 AI 模型"
        isDirty={sectionDirty}
        isSaving={isSaving}
        onApply={onApply}
        onReset={onReset}
      >
        <div
          className={`rounded-xl border px-3 py-2 text-[11px] ${
            isConfigured
              ? "border-emerald-200 bg-emerald-50/70 text-emerald-700 dark:border-emerald-800/50 dark:bg-emerald-950/20 dark:text-emerald-300"
              : hasFallback
                ? "border-blue-200 bg-blue-50/70 text-blue-700 dark:border-blue-800/50 dark:bg-blue-950/20 dark:text-blue-300"
                : "border-amber-200 bg-amber-50/70 text-amber-700 dark:border-amber-800/50 dark:bg-amber-950/20 dark:text-amber-300"
          }`}
        >
          <div className="font-medium">
            {isConfigured
              ? "已配置专用模型"
              : hasFallback
                ? "使用翻译设置中的 AI 模型"
                : "未配置"}
          </div>
          <div className="mt-1 opacity-90">
            {isConfigured
              ? `使用 ${draft.agentAiProviderId || "自定义"} / ${draft.agentAiModelId}`
              : hasFallback
                ? "当前未设置专用模型，将自动使用翻译设置中的 AI 配置。如需使用不同模型，可在下方单独配置。"
                : "请在下方配置专用模型，或在翻译设置中配置 AI 模型作为回退。"}
          </div>
        </div>

        <div className="space-y-4">
          <RadioGroup
            value={draft.agentAiInputMode}
            onValueChange={(v) =>
              updateDraft({ agentAiInputMode: v as "registry" | "custom" })
            }
            className="flex gap-4"
          >
            <div className="flex items-center space-x-2">
              <RadioGroupItem value="registry" id="agent-ai-registry" />
              <Label htmlFor="agent-ai-registry" className="text-xs cursor-pointer">
                从列表选择
                {aiProvidersLoading ? (
                  <Loader2 className="w-3 h-3 animate-spin inline ml-1" />
                ) : null}
              </Label>
            </div>
            <div className="flex items-center space-x-2">
              <RadioGroupItem value="custom" id="agent-ai-custom" />
              <Label htmlFor="agent-ai-custom" className="text-xs cursor-pointer">
                自定义
              </Label>
            </div>
          </RadioGroup>

          {draft.agentAiInputMode === "registry" && (
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">AI 渠道</Label>
                <Popover>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      role="combobox"
                      className="w-full justify-between font-normal"
                    >
                      {draft.agentAiProviderId
                        ? aiProviders.find((p) => p.id === draft.agentAiProviderId)
                            ?.name || draft.agentAiProviderId
                        : "选择渠道..."}
                      <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-[200px] p-0">
                    <Command>
                      <CommandInput placeholder="搜索渠道..." />
                      <CommandList>
                        <CommandEmpty>未找到渠道</CommandEmpty>
                        <CommandGroup>
                          {aiProviders.map((provider) => (
                            <CommandItem
                              key={provider.id}
                              value={provider.id}
                              onSelect={(value) =>
                                updateDraft({
                                  agentAiProviderId: value,
                                  agentAiModelId: "",
                                })
                              }
                            >
                              <Check
                                className={`mr-2 h-4 w-4 ${
                                  draft.agentAiProviderId === provider.id
                                    ? "opacity-100"
                                    : "opacity-0"
                                }`}
                              />
                              {provider.name}
                            </CommandItem>
                          ))}
                        </CommandGroup>
                      </CommandList>
                    </Command>
                  </PopoverContent>
                </Popover>
              </div>

              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">模型</Label>
                <Popover>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      role="combobox"
                      className="w-full justify-between font-normal"
                      disabled={!draft.agentAiProviderId}
                    >
                      {draft.agentAiModelId
                        ? getModelsForProvider(
                            aiProviders,
                            draft.agentAiProviderId,
                          ).find((m) => m.id === draft.agentAiModelId)?.name ||
                          draft.agentAiModelId
                        : "选择模型..."}
                      <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-[280px] p-0">
                    <Command>
                      <CommandInput placeholder="搜索模型..." />
                      <CommandList>
                        <CommandEmpty>未找到模型</CommandEmpty>
                        <CommandGroup>
                          {getModelsForProvider(
                            aiProviders,
                            draft.agentAiProviderId,
                          ).map((model) => (
                            <CommandItem
                              key={model.id}
                              value={model.id}
                              onSelect={(value) =>
                                updateDraft({ agentAiModelId: value })
                              }
                            >
                              <Check
                                className={`mr-2 h-4 w-4 ${
                                  draft.agentAiModelId === model.id
                                    ? "opacity-100"
                                    : "opacity-0"
                                }`}
                              />
                              {model.name}
                            </CommandItem>
                          ))}
                        </CommandGroup>
                      </CommandList>
                    </Command>
                  </PopoverContent>
                </Popover>
              </div>
            </div>
          )}

          {draft.agentAiInputMode === "custom" && (
            <div className="space-y-3">
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">API 地址</Label>
                <Input
                  placeholder="https://api.openai.com/v1"
                  value={draft.agentAiBaseUrl}
                  onChange={(e) =>
                    updateDraft({ agentAiBaseUrl: e.target.value })
                  }
                />
              </div>
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">模型名称</Label>
                <Input
                  placeholder="gpt-4o"
                  value={draft.agentAiModelId}
                  onChange={(e) =>
                    updateDraft({ agentAiModelId: e.target.value })
                  }
                />
              </div>
            </div>
          )}

          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">API Key</Label>
            <div className="relative">
              <Input
                type={showApiKey ? "text" : "password"}
                placeholder="sk-..."
                value={draft.agentAiApiKey}
                onChange={(e) => updateDraft({ agentAiApiKey: e.target.value })}
                className="pr-10"
              />
              <button
                type="button"
                onClick={onToggleShowApiKey}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
              >
                {showApiKey ? (
                  <EyeOff className="w-4 h-4" />
                ) : (
                  <Eye className="w-4 h-4" />
                )}
              </button>
            </div>
          </div>

          {draft.agentAiInputMode === "registry" && draft.agentAiProviderId && (
            <div className="space-y-2">
              <Label className="text-xs text-muted-foreground">
                API 地址（自动填充）
              </Label>
              <Input
                placeholder="自动填充"
                value={draft.agentAiBaseUrl}
                onChange={(e) => updateDraft({ agentAiBaseUrl: e.target.value })}
              />
              <p className="text-[11px] text-muted-foreground/70">
                可手动修改用于代理
              </p>
            </div>
          )}
        </div>

        <p className="text-[11px] text-muted-foreground/70">
          留空则自动使用翻译设置中的 AI 模型配置。数据问答需要支持 Function Calling 的模型（如 gpt-4o、gpt-4-turbo）。
        </p>
      </SettingsSection>
    );
  },
);
