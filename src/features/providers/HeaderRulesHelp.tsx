import { Info } from "lucide-react";
import { HoverHelp } from "../../components/HoverHelp";
import { DEFAULT_FORWARD_HEADERS } from "./providerHeaderModel.ts";

/** 请求头映射说明：悬停图标弹出求值顺序与默认放行清单。 */
export function HeaderRulesHelp() {
  return (
    <HoverHelp
      label="查看请求头映射规则"
      triggerClassName="header-rules-help-trigger"
      panelClassName="header-rules-help"
      panelId="provider-header-rules-help"
      icon={<Info aria-hidden="true" />}
      panel={
        <>
          <p className="header-rules-help-title">请求头如何发给上游</p>
          <p className="header-rules-help-note">
            上游只会收到下列规则产出的请求头；鉴权由 Lumen 最后自动注入，传输层头（
            <code>host</code> 等）永不外发。
          </p>
          <ol className="header-rules-help-order">
            <li>默认放行</li>
            <li>透传</li>
            <li>替换</li>
            <li>添加</li>
            <li>移除（最高优先）</li>
          </ol>
          <p className="header-rules-help-note">默认放行：</p>
          <p className="header-rules-help-codes">
            {DEFAULT_FORWARD_HEADERS.map((name) => (
              <code key={name}>{name}</code>
            ))}
          </p>
        </>
      }
    />
  );
}
