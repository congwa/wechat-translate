import { forwardRef } from "react";
import {
  Check,
  ChevronsUpDown,
  Eye,
  EyeOff,
  Languages,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { ProviderInfo } from "@/lib/models-registry";
import { getModelsForProvider } from "@/lib/models-registry";
import { SOURCE_LANGS, TARGET_LANGS } from "@/components/settings/constants";
import type { SettingsDraft } from "@/stores/settingsStore";

interface TranslateSectionProps {
  draft: SettingsDraft;
  sectionDirty: boolean;
  isSaving: boolean;
  updateDraft: (patch: Partial<SettingsDraft>) => void;
  onApply: () => void;
  onReset: () => void;
  onTranslateTest: () => void;
  translateTestBusy: boolean;
  translateTestError: string | null;
  aiProviders: ProviderInfo[];
  aiProvidersLoading: boolean;
  showApiKey: boolean;
  onToggleShowApiKey: () => void;
  status: {
    tone: "ok" | "warn" | "error";
    title: string;
    detail: string;
  };
}

export const TranslateSection = forwardRef<HTMLElement, TranslateSectionProps>(
  function TranslateSection(
    {
      draft,
      sectionDirty,
      isSaving,
      updateDraft,
      onApply,
      onReset,
      onTranslateTest,
      translateTestBusy,
      translateTestError,
      aiProviders,
      aiProvidersLoading,
      showApiKey,
      onToggleShowApiKey,
      status,
    },
    ref,
  ) {
    return (
      <SettingsSection
        id="translate"
        ref={ref}
        icon={<Languages className="w-4 h-4 text-violet-600" />}
        iconBg="bg-violet-50"
        title="翻译设置"
        description="浮窗翻译参数"
        isDirty={sectionDirty}
        isSaving={isSaving}
        onApply={onApply}
        onReset={onReset}
      >
        <div className="flex items-center justify-between">
          <h4 className="text-sm font-medium">启用翻译</h4>
          <Switch
            checked={draft.translateEnabled}
            onCheckedChange={(v) => updateDraft({ translateEnabled: v })}
          />
        </div>

        <div className="space-y-2">
          <Label className="text-xs text-muted-foreground">翻译渠道</Label>
          <Select
            value={draft.translateProvider}
            onValueChange={(v) => updateDraft({ translateProvider: v })}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="deeplx">DeepLX（免费）</SelectItem>
              <SelectItem value="ai">AI 翻译（需 API Key）</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {draft.translateProvider === "deeplx" && (
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">DeepLX 地址</Label>
            <Input
              placeholder="https://api.deeplx.org"
              value={draft.deeplxUrl}
              onChange={(e) => updateDraft({ deeplxUrl: e.target.value })}
            />
            <p className="text-[11px] text-muted-foreground/70">
              填写完整翻译接口 URL。前往{" "}
              <a
                href="https://connect.linux.do/dash/deeplx"
                target="_blank"
                rel="noopener noreferrer"
                className="text-violet-500 hover:underline"
              >
                connect.linux.do/dash/deeplx
              </a>{" "}
              获取完整 URL
            </p>
          </div>
        )}

        {draft.translateProvider === "ai" && (
          <div className="space-y-4">
            <RadioGroup
              value={draft.aiInputMode}
              onValueChange={(v) =>
                updateDraft({ aiInputMode: v as "registry" | "custom" })
              }
              className="flex gap-4"
            >
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="registry" id="ai-registry" />
                <Label htmlFor="ai-registry" className="text-xs cursor-pointer">
                  从列表选择
                  {aiProvidersLoading ? (
                    <Loader2 className="w-3 h-3 animate-spin inline ml-1" />
                  ) : null}
                </Label>
              </div>
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="custom" id="ai-custom" />
                <Label htmlFor="ai-custom" className="text-xs cursor-pointer">
                  自定义
                </Label>
              </div>
            </RadioGroup>

            {draft.aiInputMode === "registry" && (
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
                        {draft.aiProviderId
                          ? aiProviders.find((p) => p.id === draft.aiProviderId)
                              ?.name || draft.aiProviderId
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
                                    aiProviderId: value,
                                    aiModelId: "",
                                  })
                                }
                              >
                                <Check
                                  className={`mr-2 h-4 w-4 ${
                                    draft.aiProviderId === provider.id
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
                        disabled={!draft.aiProviderId}
                      >
                        {draft.aiModelId
                          ? getModelsForProvider(
                              aiProviders,
                              draft.aiProviderId,
                            ).find((m) => m.id === draft.aiModelId)?.name ||
                            draft.aiModelId
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
                              draft.aiProviderId,
                            ).map((model) => (
                              <CommandItem
                                key={model.id}
                                value={model.id}
                                onSelect={(value) =>
                                  updateDraft({ aiModelId: value })
                                }
                              >
                                <Check
                                  className={`mr-2 h-4 w-4 ${
                                    draft.aiModelId === model.id
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

            {draft.aiInputMode === "custom" && (
              <div className="space-y-3">
                <div className="space-y-2">
                  <Label className="text-xs text-muted-foreground">API 地址</Label>
                  <Input
                    placeholder="https://api.openai.com/v1"
                    value={draft.aiBaseUrl}
                    onChange={(e) =>
                      updateDraft({ aiBaseUrl: e.target.value })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs text-muted-foreground">模型名称</Label>
                  <Input
                    placeholder="gpt-4o-mini"
                    value={draft.aiModelId}
                    onChange={(e) =>
                      updateDraft({ aiModelId: e.target.value })
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
                  value={draft.aiApiKey}
                  onChange={(e) => updateDraft({ aiApiKey: e.target.value })}
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

            {draft.aiInputMode === "registry" && draft.aiProviderId && (
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">
                  API 地址（自动填充）
                </Label>
                <Input
                  placeholder="自动填充"
                  value={draft.aiBaseUrl}
                  onChange={(e) => updateDraft({ aiBaseUrl: e.target.value })}
                />
                <p className="text-[11px] text-muted-foreground/70">
                  可手动修改用于代理
                </p>
              </div>
            )}
          </div>
        )}

        <div
          className={`rounded-xl border px-3 py-2 text-[11px] ${
            status.tone === "ok"
              ? "border-emerald-200 bg-emerald-50/70 text-emerald-700 dark:border-emerald-800/50 dark:bg-emerald-950/20 dark:text-emerald-300"
              : status.tone === "warn"
                ? "border-amber-200 bg-amber-50/70 text-amber-700 dark:border-amber-800/50 dark:bg-amber-950/20 dark:text-amber-300"
                : "border-red-200 bg-red-50/70 text-red-700 dark:border-red-800/50 dark:bg-red-950/20 dark:text-red-300"
          }`}
        >
          <div className="font-medium">{status.title}</div>
          <div className="mt-1 opacity-90">{status.detail}</div>
        </div>

        <div className="grid grid-cols-2 gap-4 lg:grid-cols-3">
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">源语言</Label>
            <Select
              value={draft.sourceLang}
              onValueChange={(v) => updateDraft({ sourceLang: v })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SOURCE_LANGS.map((lang) => (
                  <SelectItem key={lang.value} value={lang.value}>
                    {lang.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">目标语言</Label>
            <Select
              value={draft.targetLang}
              onValueChange={(v) => updateDraft({ targetLang: v })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {TARGET_LANGS.map((lang) => (
                  <SelectItem key={lang.value} value={lang.value}>
                    {lang.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">超时时间</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="1"
                step="1"
                value={draft.translateTimeout}
                onChange={(e) =>
                  updateDraft({ translateTimeout: e.target.value })
                }
              />
              <span className="text-xs text-muted-foreground shrink-0">秒</span>
            </div>
          </div>

          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">同时并发数</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="1"
                step="1"
                value={draft.translateMaxConcurrency}
                onChange={(e) =>
                  updateDraft({ translateMaxConcurrency: e.target.value })
                }
              />
              <span className="text-xs text-muted-foreground shrink-0">个</span>
            </div>
          </div>

          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">每秒请求数</Label>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min="1"
                step="1"
                value={draft.translateMaxRequestsPerSecond}
                onChange={(e) =>
                  updateDraft({
                    translateMaxRequestsPerSecond: e.target.value,
                  })
                }
              />
              <span className="text-xs text-muted-foreground shrink-0">次</span>
            </div>
          </div>
        </div>

        <Button
          variant="outline"
          className="w-full h-10 rounded-xl font-semibold text-sm"
          onClick={onTranslateTest}
          disabled={
            translateTestBusy ||
            (draft.translateProvider === "deeplx" && !draft.deeplxUrl.trim()) ||
            (draft.translateProvider === "ai" && !draft.aiApiKey.trim())
          }
        >
          {translateTestBusy ? (
            <span className="animate-pulse">测试中…</span>
          ) : (
            <>
              <Languages className="w-4 h-4 mr-2" />
              测试翻译
            </>
          )}
        </Button>

        {translateTestError ? (
          <div className="mt-3 p-3 rounded-xl bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50">
            <p className="text-xs font-medium text-red-600 dark:text-red-400 mb-1">
              连接失败
            </p>
            <p className="text-[11px] text-red-500 dark:text-red-400/80 break-all">
              {translateTestError}
            </p>
          </div>
        ) : null}
      </SettingsSection>
    );
  },
);
