import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { useTranslation } from "../i18n";
import { isMac } from "../lib/platform";
import { BrandMark } from "./BrandMark";

export function WindowTitlebar({ appName }: { appName: string }) {
  const { t } = useTranslation();
  const native = isTauri();
  const toggleMaximize = () => { if (native) void getCurrentWindow().toggleMaximize(); };

  // macOS draws its own close/minimize/zoom buttons over the left edge of this bar.
  if (isMac) return <header className="window-titlebar window-titlebar--mac">
    <div className="window-titlebar__identity" data-tauri-drag-region>
      <BrandMark small /><strong>{appName}</strong>
    </div>
    <div className="window-titlebar__drag" data-tauri-drag-region />
  </header>;

  return <header className="window-titlebar">
    <div className="window-titlebar__identity" data-tauri-drag-region>
      <BrandMark small /><strong>{appName}</strong>
    </div>
    <div className="window-titlebar__drag" data-tauri-drag-region />
    <div className="window-titlebar__controls">
      <button disabled={!native} aria-label={t("Minimize window")} title={t("Minimize window")} onClick={() => void getCurrentWindow().minimize()}><Minus size={16} /></button>
      <button disabled={!native} aria-label={t("Maximize window")} title={t("Maximize window")} onClick={toggleMaximize}><Square size={13} /></button>
      <button disabled={!native} className="window-titlebar__close" aria-label={t("Close window")} title={t("Close window")} onClick={() => void getCurrentWindow().close()}><X size={16} /></button>
    </div>
  </header>;
}
