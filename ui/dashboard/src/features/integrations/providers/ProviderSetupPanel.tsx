import type { FormEvent } from "react";
import { DisclosureSection } from "../../../components/DisclosureSection";
import { Panel } from "../../../components/Panel";
import { PROVIDER_PRESETS } from "../catalog";
import { slugify } from "./draft";

interface ProviderSetupPanelProps {
  providerPreset: string;
  providerModels: string[];
  authStatus: string | null;
  onProviderPresetChange: (value: string) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onBeginBrowserAuth: (form: HTMLFormElement) => void;
  onDiscoverModels: (form: HTMLFormElement) => void;
}

export function ProviderSetupPanel({
  providerPreset,
  providerModels,
  authStatus,
  onProviderPresetChange,
  onSubmit,
  onBeginBrowserAuth,
  onDiscoverModels
}: ProviderSetupPanelProps) {
  const preset = PROVIDER_PRESETS.find((entry) => entry.id === providerPreset)!;

  return (
    <Panel eyebrow="Providers" title="Provider setup">
      <form className="stack-list" id="provider-form" onSubmit={onSubmit}>
        <article className="stack-card stack-card--summary">
          <div className="stack-card__title">
            <strong>{preset.label}</strong>
            <span>{preset.kind}</span>
          </div>
          <div className="fact-grid">
            <article className="fact-card">
              <span>Base URL</span>
              <strong>{preset.baseUrl || "service default"}</strong>
            </article>
            <article className="fact-card">
              <span>Auth</span>
              <strong>{preset.authMode}</strong>
            </article>
            <article className="fact-card">
              <span>Model</span>
              <strong>{preset.defaultModel || "manual"}</strong>
            </article>
            <article className="fact-card">
              <span>Browser auth</span>
              <strong>{preset.browserAuthKind || "not supported"}</strong>
            </article>
          </div>
        </article>
        <label className="field">
          <span>Preset</span>
          <select
            id="provider-preset"
            value={providerPreset}
            onChange={(event) => onProviderPresetChange(event.target.value)}
          >
            {PROVIDER_PRESETS.map((presetOption) => (
              <option key={presetOption.id} value={presetOption.id}>
                {presetOption.label}
              </option>
            ))}
          </select>
        </label>
        <div key={providerPreset} className="stack-list">
          <DisclosureSection
            title="Identity"
            subtitle="Provider id, display name, and implementation family"
            meta="required"
            defaultOpen
          >
            <div className="grid-three">
              <label className="field">
                <span>Provider ID</span>
                <input
                  id="provider-id"
                  name="id"
                  defaultValue={preset.id === "custom" ? "" : slugify(preset.displayName)}
                />
              </label>
              <label className="field">
                <span>Display name</span>
                <input
                  id="provider-display-name"
                  name="display_name"
                  defaultValue={preset.displayName}
                />
              </label>
              <label className="field">
                <span>Kind</span>
                <select id="provider-kind" name="kind" defaultValue={preset.kind}>
                  <option value="chat_gpt_codex">chat_gpt_codex</option>
                  <option value="open_ai_compatible">open_ai_compatible</option>
                  <option value="anthropic">anthropic</option>
                  <option value="ollama">ollama</option>
                </select>
              </label>
            </div>
          </DisclosureSection>

          <DisclosureSection
            title="Connection"
            subtitle="Base URL, authentication mode, default model, and local/runtime credentials"
            meta="primary"
            defaultOpen
          >
            <div className="stack-list">
              <div className="grid-three">
                <label className="field">
                  <span>Base URL</span>
                  <input
                    id="provider-base-url"
                    name="base_url"
                    defaultValue={preset.baseUrl}
                  />
                </label>
                <label className="field">
                  <span>Auth mode</span>
                  <select
                    id="provider-auth-mode"
                    name="auth_mode"
                    defaultValue={preset.authMode}
                  >
                    <option value="none">none</option>
                    <option value="api_key">api_key</option>
                    <option value="oauth">oauth</option>
                  </select>
                </label>
                <label className="field">
                  <span>Default model</span>
                  <input
                    id="provider-default-model"
                    name="default_model"
                    defaultValue={preset.defaultModel}
                  />
                </label>
              </div>
              <div className="grid-two">
                <label className="field">
                  <span>API key</span>
                  <input name="api_key" type="password" autoComplete="off" />
                </label>
                <label className="field">
                  <span>
                    <input name="local" type="checkbox" defaultChecked={preset.local} /> Local
                    provider
                  </span>
                </label>
              </div>
            </div>
          </DisclosureSection>

          <DisclosureSection
            title="Alias target"
            subtitle="Optionally create an alias alongside the provider"
            meta="optional"
            defaultOpen
          >
            <div className="stack-list">
              <div className="grid-three">
                <label className="field">
                  <span>Alias name</span>
                  <input id="provider-alias-name" name="alias_name" placeholder="main" />
                </label>
                <label className="field">
                  <span>Alias model</span>
                  <input name="alias_model" placeholder={preset.defaultModel} />
                </label>
                <label className="field">
                  <span>Alias description</span>
                  <input name="alias_description" />
                </label>
              </div>
              <label className="field">
                <span>
                  <input name="set_as_main" type="checkbox" /> Set as main alias
                </span>
              </label>
            </div>
          </DisclosureSection>

          <DisclosureSection
            title="OAuth payloads"
            subtitle="Manual JSON overrides for providers that need explicit OAuth material"
            meta="advanced"
          >
            <div className="stack-list">
              <label className="field">
                <span>OAuth config JSON</span>
                <textarea name="oauth_json" placeholder='{"client_id":"..."}' />
              </label>
              <label className="field">
                <span>OAuth token JSON</span>
                <textarea name="oauth_token_json" placeholder='{"access_token":"..."}' />
              </label>
            </div>
          </DisclosureSection>

          <div className="button-row">
            <button id="provider-save" type="submit">
              Save provider
            </button>
            {preset.browserAuthKind ? (
              <button
                id="provider-browser-auth"
                type="button"
                onClick={(event) => {
                  const form = event.currentTarget.form;
                  if (form) {
                    onBeginBrowserAuth(form);
                  }
                }}
              >
                Browser auth
              </button>
            ) : null}
            <button
              id="provider-discover-models"
              type="button"
              onClick={(event) => {
                const form = event.currentTarget.form;
                if (form) {
                  onDiscoverModels(form);
                }
              }}
            >
              Discover models
            </button>
          </div>
        </div>
      </form>
      {authStatus ? (
        <p className="helper-copy" id="provider-auth-status">
          {authStatus}
        </p>
      ) : null}
      {providerModels.length ? (
        <DisclosureSection
          title="Discovered models"
          subtitle="Models reported by the currently targeted provider"
          meta={`${providerModels.length} found`}
          className="disclosure--surface"
          defaultOpen
        >
          <div className="stack-list" id="provider-models">
            {providerModels.map((model) => (
              <article key={model} className="stack-card">
                <strong>{model}</strong>
              </article>
            ))}
          </div>
        </DisclosureSection>
      ) : null}
    </Panel>
  );
}
