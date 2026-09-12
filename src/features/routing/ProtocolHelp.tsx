import { Info } from "lucide-react";
import { HoverHelp } from "../../components/HoverHelp";
import { protocolEndpoints } from "./protocolEndpoints";

/**
 * 协议调用说明：一个统一的信息图标，悬停 / 聚焦时在旁侧弹出各协议的入站地址。
 */
export function ProtocolHelp() {
  return (
    <HoverHelp
      label="查看各协议的调用地址"
      triggerClassName="protocol-help-trigger"
      panelClassName="protocol-help"
      panelId="protocol-help-popover"
      icon={<Info aria-hidden="true" />}
      panel={
        <>
          <p className="protocol-help-title">各协议的调用地址</p>
          <dl className="protocol-help-list">
            {protocolEndpoints.map((endpoint) => (
              <div className="protocol-help-row" key={endpoint.protocol}>
                <dt>{endpoint.label}</dt>
                <dd>
                  <code>
                    {endpoint.method} {endpoint.path}
                  </code>
                  {endpoint.note ? <em>{endpoint.note}</em> : null}
                </dd>
              </div>
            ))}
          </dl>
          <p className="protocol-help-foot">
            基址 http://127.0.0.1:{"{端口}"}，调用需携带虚拟密钥。
          </p>
        </>
      }
    />
  );
}
