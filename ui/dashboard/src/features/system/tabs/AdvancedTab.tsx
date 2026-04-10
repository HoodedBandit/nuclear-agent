import type { FormEvent } from "react";
import { Panel } from "../../../components/Panel";

interface AdvancedTabProps {
  config: Record<string, unknown> | undefined;
  onSaveConfig: (event: FormEvent<HTMLFormElement>) => void;
}

export function AdvancedTab(props: AdvancedTabProps) {
  const { config, onSaveConfig } = props;

  return (
    <Panel eyebrow="Advanced" title="Full config editor">
      <form className="stack-list" id="advanced-config-form" onSubmit={onSaveConfig}>
        <label className="field">
          <span>Raw config JSON</span>
          <textarea
            id="advanced-config-editor"
            name="config"
            defaultValue={JSON.stringify(config || {}, null, 2)}
            style={{ minHeight: 420 }}
          />
        </label>
        <div className="button-row">
          <button id="advanced-config-save" type="submit">
            Save full config
          </button>
        </div>
      </form>
    </Panel>
  );
}
