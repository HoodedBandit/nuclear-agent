import { useState } from "react";
import { DelegationWorkbench } from "../integrations/DelegationWorkbench";
import { PluginWorkbench } from "../integrations/PluginWorkbench";
import { ProviderWorkbench } from "../integrations/ProviderWorkbench";
import { ToolingWorkbench } from "../integrations/ToolingWorkbench";

type InfrastructureTab = "providers" | "plugins" | "tooling" | "delegation";

export function InfrastructurePage() {
  const [activeTab, setActiveTab] = useState<InfrastructureTab>("providers");

  return (
    <div className="page-stack">
      <section className="route-tabs" aria-label="Infrastructure sections">
        {(["providers", "plugins", "tooling", "delegation"] as InfrastructureTab[]).map(
          (tab) => (
            <button
              key={tab}
              type="button"
              className={activeTab === tab ? "is-active" : undefined}
              onClick={() => setActiveTab(tab)}
            >
              {tab}
            </button>
          )
        )}
      </section>
      {activeTab === "providers" ? <ProviderWorkbench /> : null}
      {activeTab === "plugins" ? <PluginWorkbench /> : null}
      {activeTab === "tooling" ? <ToolingWorkbench /> : null}
      {activeTab === "delegation" ? <DelegationWorkbench /> : null}
    </div>
  );
}
