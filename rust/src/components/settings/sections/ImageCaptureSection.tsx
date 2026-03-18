import { forwardRef } from "react";
import { Image } from "lucide-react";
import { Switch } from "@/components/ui/switch";

interface ImageCaptureSectionProps {
  checked: boolean;
  useRightPanelDetails: boolean;
  onCheckedChange: (checked: boolean) => void;
}

export const ImageCaptureSection = forwardRef<
  HTMLElement,
  ImageCaptureSectionProps
>(function ImageCaptureSection(
  { checked, useRightPanelDetails, onCheckedChange },
  ref,
) {
  return (
    <section
      ref={ref}
      className="glass-card rounded-2xl p-6 shadow-sm space-y-5 border border-amber-200/50 dark:border-amber-700/30"
    >
      <div className="flex items-center gap-3">
        <div className="w-9 h-9 rounded-xl bg-amber-50 dark:bg-amber-900/30 flex items-center justify-center">
          <Image className="w-4 h-4 text-amber-600 dark:text-amber-400" />
        </div>
        <div>
          <h3 className="text-sm font-semibold">图片缩略图</h3>
          <p className="text-[11px] text-muted-foreground">
            读取微信本地缓存中的聊天图片缩略图，仅支持 macOS。
          </p>
        </div>
      </div>

      <div className="bg-amber-50/50 dark:bg-amber-900/10 rounded-xl p-4 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Image className="w-4 h-4 text-amber-600 dark:text-amber-400" />
            <h4 className="text-sm font-medium">聊天图片缩略图</h4>
          </div>
          <Switch checked={checked} onCheckedChange={onCheckedChange} />
        </div>
        <p className="text-[11px] text-muted-foreground">
          该选项属于应用配置；保存后会由后端统一决定是否读取图片缩略图。
        </p>
        {!useRightPanelDetails ? (
          <p className="text-[11px] text-amber-600 dark:text-amber-400">
            当前已关闭“右侧详情补充”，图片缩略图不会生效。
          </p>
        ) : null}
        <div className="text-[11px] text-muted-foreground/80 space-y-1">
          <p className="font-medium text-muted-foreground">工作原理</p>
          <p>
            当检测到 [图片] 消息时，从微信缓存目录读取对应的图片缩略图并展示在浮窗和历史记录中。
          </p>
        </div>
      </div>
    </section>
  );
});
