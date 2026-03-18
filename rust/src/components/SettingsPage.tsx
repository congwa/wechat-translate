import { Button } from "@/components/ui/button";
import { RefreshCcw } from "lucide-react";
import { useSettingsController } from "@/features/settings/useSettingsController";
import {
  AdvancedConfigSection,
  DictSection,
  DisplaySection,
  GeneralSection,
  ImageCaptureSection,
  ListenSection,
  TranslateSection,
} from "@/components/settings/sections";

export function SettingsPage() {
  const controller = useSettingsController();

  if (!controller.settings) {
    return (
      <div className="glass-card rounded-2xl p-8 text-sm text-muted-foreground">
        正在加载设置...
      </div>
    );
  }

  return (
    <div className="relative">
      <nav className="fixed right-6 top-1/2 -translate-y-1/2 z-50 hidden xl:flex flex-col gap-1.5 py-2 px-1.5 rounded-xl bg-background/80 backdrop-blur-sm border border-border/50 shadow-sm">
        {controller.navItems.map((item) => (
          <button
            key={item.id}
            onClick={() => controller.scrollToSection(item.id)}
            className={`relative px-2.5 py-1.5 text-[11px] font-medium rounded-lg transition-all duration-150 ${
              controller.activeSection === item.id
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/50"
            }`}
          >
            {item.label}
            {item.isDirty ? (
              <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-red-500" />
            ) : null}
          </button>
        ))}
      </nav>

      <div className="max-w-2xl space-y-6">
        <section className="glass-card rounded-2xl p-4 shadow-sm border border-muted/50">
          <div className="flex items-center justify-between">
            <p className="text-[11px] text-muted-foreground">
              每个设置区域修改后会显示"应用"按钮，点击后即时生效。
            </p>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={() => controller.reloadFromBackend(false)}
              disabled={controller.busy === "reload"}
            >
              <RefreshCcw className="w-3 h-3 mr-1" />
              重新加载
            </Button>
          </div>
        </section>

        <GeneralSection
          ref={(el) => {
            controller.sectionRefs.current.general = el;
          }}
          closeToTray={controller.runtime.close_to_tray}
          onCloseToTrayChange={controller.handleTrayToggle}
        />

        <ListenSection
          ref={(el) => {
            controller.sectionRefs.current.listen = el;
          }}
          draft={controller.draft}
          sectionDirty={controller.sectionDirty.listen}
          isSaving={controller.busy === "section_listen"}
          monitoring={controller.runtime.tasks.monitoring}
          sidebarRunning={controller.runtime.tasks.sidebar}
          sidebarWindowMode={controller.sidebarWindowMode}
          onApply={() => controller.handleApplySection("listen")}
          onReset={() => controller.handleResetSection("listen")}
          onMonitoringToggle={controller.handleMonitoringToggle}
          onSidebarModeChange={(mode) =>
            controller.setUiPreferences({ sidebarWindowMode: mode })
          }
          updateDraft={controller.updateDraft}
          monitoringBusy={controller.busy === "monitoring"}
        />

        {controller.sidebarWindowMode === "independent" ? (
          <DisplaySection
            ref={(el) => {
              controller.sectionRefs.current.display = el;
            }}
            draft={controller.draft}
            sectionDirty={controller.sectionDirty.display}
            isSaving={controller.busy === "section_display"}
            onApply={() => controller.handleApplySection("display")}
            onReset={() => controller.handleResetSection("display")}
            updateDraft={controller.updateDraft}
          />
        ) : null}

        <TranslateSection
          ref={(el) => {
            controller.sectionRefs.current.translate = el;
          }}
          draft={controller.draft}
          sectionDirty={controller.sectionDirty.translate}
          isSaving={controller.busy === "section_translate"}
          updateDraft={controller.updateDraft}
          onApply={() => controller.handleApplySection("translate")}
          onReset={() => controller.handleResetSection("translate")}
          onTranslateTest={controller.handleTranslateTest}
          translateTestBusy={controller.busy === "translate_test"}
          translateTestError={controller.translateTestError}
          aiProviders={controller.aiProviders}
          aiProvidersLoading={controller.aiProvidersLoading}
          showApiKey={controller.showApiKey}
          onToggleShowApiKey={controller.toggleShowApiKey}
          status={controller.getTranslateConfigStatus()}
        />

        <DictSection
          ref={(el) => {
            controller.sectionRefs.current.dict = el;
          }}
          draft={controller.draft}
          updateDraft={controller.updateDraft}
        />

        <ImageCaptureSection
          checked={controller.draft.imageCapture}
          useRightPanelDetails={controller.draft.useRightPanelDetails}
          onCheckedChange={(checked) =>
            controller.updateDraft({ imageCapture: checked }, "display")
          }
        />

        <AdvancedConfigSection
          open={controller.advancedOpen}
          busy={controller.busy}
          configRaw={controller.configRaw}
          configDirty={controller.configDirty}
          configValid={controller.configValid}
          configErrors={controller.configErrors}
          onToggleOpen={controller.toggleAdvancedOpen}
          onConfigRawChange={controller.handleConfigRawChange}
          onRestoreDefault={controller.handleRestoreDefault}
          onReload={() => controller.reloadFromBackend(false)}
          onApply={controller.handleApplyConfig}
        />
      </div>
    </div>
  );
}
