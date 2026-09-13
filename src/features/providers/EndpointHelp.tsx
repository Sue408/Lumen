import { Info } from "lucide-react";
import { HoverHelp } from "../../components/HoverHelp";

/** 协议端点说明：悬停图标解释多协议端点与模型共享。 */
export function EndpointHelp() {
  return (
    <HoverHelp
      label="查看协议端点说明"
      triggerClassName="section-help-trigger"
      panelClassName="section-help"
      panelId="provider-endpoints-help"
      icon={<Info aria-hidden="true" />}
      panel={
        <>
          <p className="section-help-title">协议端点</p>
          <p className="section-help-note">
            一个提供商可挂多个协议端点，模型在多协议间共享；同一模型可被不同协议的路由复用。
          </p>
        </>
      }
    />
  );
}
