import type { ModelAlias, ProviderConfig } from "../../../api/types";
import { Panel } from "../../../components/Panel";

interface ProviderInventoryPanelProps {
  providers: ProviderConfig[];
  aliases: ModelAlias[];
  inventoryView: "providers" | "aliases";
  onInventoryViewChange: (value: "providers" | "aliases") => void;
  onFetchModels: (providerId: string) => void;
  onClearCredentials: (providerId: string) => void;
  onDeleteProvider: (providerId: string) => void;
  onMakeMain: (alias: string) => void;
  onDeleteAlias: (alias: string) => void;
}

export function ProviderInventoryPanel({
  providers,
  aliases,
  inventoryView,
  onInventoryViewChange,
  onFetchModels,
  onClearCredentials,
  onDeleteProvider,
  onMakeMain,
  onDeleteAlias
}: ProviderInventoryPanelProps) {
  return (
    <Panel
      eyebrow="Targets"
      title="Configured providers and aliases"
      meta={`${providers.length} providers / ${aliases.length} aliases`}
    >
      <div className="stack-list">
        <div className="subtabs subtabs--panel" role="tablist" aria-label="Provider inventory">
          {(["providers", "aliases"] as const).map((view) => (
            <button
              key={view}
              type="button"
              className={inventoryView === view ? "is-active" : undefined}
              onClick={() => onInventoryViewChange(view)}
            >
              {view}
            </button>
          ))}
        </div>

        {inventoryView === "providers" ? (
          <div className="stack-list" id="providers-list">
            {providers.map((provider) => (
              <article key={provider.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{provider.display_name}</strong>
                  <span>{provider.kind}</span>
                </div>
                <p className="stack-card__subtitle mono">{provider.id}</p>
                <div className="button-row">
                  <button type="button" onClick={() => onFetchModels(provider.id)}>
                    Models
                  </button>
                  <button type="button" onClick={() => onClearCredentials(provider.id)}>
                    Clear creds
                  </button>
                  <button type="button" onClick={() => onDeleteProvider(provider.id)}>
                    Remove
                  </button>
                </div>
              </article>
            ))}
          </div>
        ) : null}

        {inventoryView === "aliases" ? (
          <div className="stack-list" id="aliases-list">
            {aliases.map((alias) => (
              <article key={alias.alias} className="stack-card">
                <div className="stack-card__title">
                  <strong>{alias.alias}</strong>
                  <span>{alias.model}</span>
                </div>
                <p className="stack-card__subtitle mono">{alias.provider_id}</p>
                <div className="button-row">
                  <button type="button" onClick={() => onMakeMain(alias.alias)}>
                    Make main
                  </button>
                  <button type="button" onClick={() => onDeleteAlias(alias.alias)}>
                    Remove
                  </button>
                </div>
              </article>
            ))}
          </div>
        ) : null}
      </div>
    </Panel>
  );
}
