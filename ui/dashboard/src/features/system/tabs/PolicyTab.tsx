import type { FormEvent } from "react";
import type { PermissionPreset, TrustPolicy } from "../../../api/types";
import { Panel } from "../../../components/Panel";

interface PolicyTabProps {
  permissions: PermissionPreset;
  trust: TrustPolicy;
  onUpdatePermission: (event: FormEvent<HTMLFormElement>) => void;
  onUpdateTrust: (event: FormEvent<HTMLFormElement>) => void;
}

export function PolicyTab(props: PolicyTabProps) {
  const { permissions, trust, onUpdatePermission, onUpdateTrust } = props;

  return (
    <div className="split-panels">
      <Panel eyebrow="Permissions" title="Permission preset">
        <form className="stack-list" id="permissions-form" onSubmit={onUpdatePermission}>
          <label className="field">
            <span>Preset</span>
            <select id="permissions" name="permission_preset" defaultValue={permissions}>
              <option value="suggest">suggest</option>
              <option value="auto_edit">auto_edit</option>
              <option value="full_auto">full_auto</option>
            </select>
          </label>
          <button type="submit">Update preset</button>
        </form>
      </Panel>

      <Panel eyebrow="Trust" title="Workspace trust">
        <form className="stack-list" id="trust-form" onSubmit={onUpdateTrust}>
          <label className="field">
            <span>Trusted path</span>
            <input
              id="trusted-path"
              name="trusted_path"
              placeholder={trust.trusted_paths[0] || "Optional path to add"}
            />
          </label>
          <label className="field">
            <span>
              <input type="checkbox" name="allow_shell" defaultChecked={trust.allow_shell} /> Allow
              shell
            </span>
          </label>
          <label className="field">
            <span>
              <input type="checkbox" name="allow_network" defaultChecked={trust.allow_network} />{" "}
              Allow network
            </span>
          </label>
          <label className="field">
            <span>
              <input
                type="checkbox"
                name="allow_full_disk"
                defaultChecked={trust.allow_full_disk}
              />{" "}
              Allow full disk
            </span>
          </label>
          <label className="field">
            <span>
              <input
                type="checkbox"
                name="allow_self_edit"
                defaultChecked={trust.allow_self_edit}
              />{" "}
              Allow self edit
            </span>
          </label>
          <button type="submit">Update trust</button>
        </form>
      </Panel>
    </div>
  );
}
