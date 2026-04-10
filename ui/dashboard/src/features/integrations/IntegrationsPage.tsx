import { useState } from "react";
import { Panel } from "../../components/Panel";
import { ConnectorWorkbench } from "./ConnectorWorkbench";
import { DelegationWorkbench } from "./DelegationWorkbench";
import { PluginWorkbench } from "./PluginWorkbench";
import { ProviderWorkbench } from "./ProviderWorkbench";
import { ToolingWorkbench } from "./ToolingWorkbench";

type IntegrationsTab =
  | "providers"
  | "connectors"
  | "plugins"
  | "tooling"
  | "delegation";

export function IntegrationsPage() {
  const [activeTab, setActiveTab] = useState<IntegrationsTab>("providers");

  return (
    <Panel eyebrow="Integrations" title="Providers, connectors, plugins, and tooling">
      <div className="toolbar">
        <div className="toolbar__title">
          <strong>Integration workbench</strong>
          <span>Bridge the daemon to models, external sources, plugin packages, and tool servers.</span>
        </div>
        <div className="subtabs">
          {(["providers", "connectors", "plugins", "tooling", "delegation"] as IntegrationsTab[]).map((tab) => (
            <button
              key={tab}
              type="button"
              data-integrations-tab-trigger={tab}
              className={activeTab === tab ? "is-active" : undefined}
              onClick={() => setActiveTab(tab)}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {activeTab === "providers" ? <ProviderWorkbench /> : null}
      {activeTab === "connectors" ? <ConnectorWorkbench /> : null}
      {activeTab === "plugins" ? <PluginWorkbench /> : null}
      {activeTab === "tooling" ? <ToolingWorkbench /> : null}
      {activeTab === "delegation" ? <DelegationWorkbench /> : null}
    </Panel>
  );
}
