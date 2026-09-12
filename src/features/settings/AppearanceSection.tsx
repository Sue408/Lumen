import { Palette } from "lucide-react";
import { SectionTitle } from "../../components/ConfigControls";
import type { Theme } from "../../app/useTheme";

export function AppearanceSection({
  theme,
  onToggleTheme,
}: {
  theme: Theme;
  onToggleTheme: () => void;
}) {
  return (
    <section className="settings-section">
      <SectionTitle icon={<Palette aria-hidden="true" />}>外观</SectionTitle>
      <div className="settings-list">
        <div className="settings-row">
          <span className="settings-row-label">主题</span>
          <div className="settings-row-control">
            <div className="settings-segment" role="group" aria-label="主题">
              <button
                className={theme === "light" ? "is-selected" : ""}
                type="button"
                aria-pressed={theme === "light"}
                onClick={() => theme !== "light" && onToggleTheme()}
              >
                浅色
              </button>
              <button
                className={theme === "dark" ? "is-selected" : ""}
                type="button"
                aria-pressed={theme === "dark"}
                onClick={() => theme !== "dark" && onToggleTheme()}
              >
                深色
              </button>
            </div>
          </div>
          <span className="settings-row-note">随本机保存，重启后沿用</span>
        </div>
      </div>
    </section>
  );
}
