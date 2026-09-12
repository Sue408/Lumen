import { FlaskConical } from "lucide-react";
import { SectionTitle } from "../../components/ConfigControls";
import { demoScenarios, type DemoScenario } from "../../services/demo";

export function DevToolsSection({
  demoBusy,
  onRunDemo,
}: {
  demoBusy: boolean;
  onRunDemo: (scenario: DemoScenario) => void;
}) {
  return (
    <section className="settings-section">
      <SectionTitle icon={<FlaskConical aria-hidden="true" />}>开发工具</SectionTitle>
      <div className="settings-list">
        <div className="settings-row">
          <span className="settings-row-label">演示数据</span>
          <div className="settings-row-control demo-scenarios">
            {demoScenarios.map((item) => (
              <button
                className="quiet-button"
                key={item.key}
                type="button"
                title={item.hint}
                disabled={demoBusy}
                onClick={() => onRunDemo(item.key)}
              >
                {item.label}
              </button>
            ))}
          </div>
          <span className="settings-row-note">
            完全替换全部业务与用量数据，仅开发构建可见；注入后切换页面即可查看
          </span>
        </div>
      </div>
    </section>
  );
}
