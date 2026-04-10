import type { FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  deleteJson,
  getJson,
  postJson,
  putJson,
  startProviderBrowserAuth,
  fetchProviderBrowserAuthSession
} from "../../api/client";
import { useDashboardData } from "../../app/dashboard-data";
import { Panel } from "../../components/Panel";
import { PROVIDER_PRESETS } from "./catalog";

function slugify(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

function parseOptionalJson(value: string) {
  return value ? JSON.parse(value) : null;
}

function readProviderDraft(form: HTMLFormElement, providerPreset: string) {
  const preset = PROVIDER_PRESETS.find((entry) => entry.id === providerPreset)!;
  const data = new FormData(form);
  const displayName = String(data.get("display_name") || preset.displayName).trim();
  const providerId = String(data.get("id") || "").trim() || slugify(displayName);
  const defaultModel = String(data.get("default_model") || preset.defaultModel).trim();

  return {
    preset,
    data,
    displayName,
    providerId,
    defaultModel
  };
}

export function ProviderWorkbench() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const [providerPreset, setProviderPreset] = useState("codex");
  const [providerModels, setProviderModels] = useState<string[]>([]);
  const [authStatus, setAuthStatus] = useState<string | null>(null);

  async function refresh() {
    await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
  }

  async function saveProvider(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const { preset, data, displayName, providerId, defaultModel } = readProviderDraft(
      form,
      providerPreset
    );
    await postJson("/v1/providers", {
      provider: {
        id: providerId,
        display_name: displayName,
        kind: data.get("kind") || preset.kind,
        base_url: String(data.get("base_url") || preset.baseUrl).trim(),
        auth_mode: data.get("auth_mode") || preset.authMode,
        default_model: defaultModel || null,
        keychain_account: null,
        oauth: parseOptionalJson(String(data.get("oauth_json") || "").trim()),
        local: Boolean(data.get("local"))
      },
      api_key: String(data.get("api_key") || "").trim() || null,
      oauth_token: parseOptionalJson(String(data.get("oauth_token_json") || "").trim())
    });
    const alias = String(data.get("alias_name") || "").trim();
    if (alias) {
      await postJson("/v1/aliases", {
        alias: {
          alias,
          provider_id: providerId,
          model:
            String(data.get("alias_model") || "").trim() || defaultModel,
          description: String(data.get("alias_description") || "").trim() || null
        },
        set_as_main: Boolean(data.get("set_as_main"))
      });
    }
    form.reset();
    await refresh();
  }

  async function beginBrowserAuth(form: HTMLFormElement) {
    const { preset, data, displayName, providerId, defaultModel } = readProviderDraft(
      form,
      providerPreset
    );
    if (!preset?.browserAuthKind) {
      return;
    }
    const popup = window.open("", `provider-auth-${Date.now()}`, "popup=yes,width=720,height=840");
    const response = await startProviderBrowserAuth({
      kind: preset.browserAuthKind,
      provider_id: providerId,
      display_name: displayName,
      default_model: defaultModel || null,
      alias_name: String(data.get("alias_name") || "").trim() || null,
      alias_model: String(data.get("alias_model") || "").trim() || null,
      alias_description: String(data.get("alias_description") || "").trim() || null,
      set_as_main: Boolean(data.get("set_as_main"))
    });
    if (popup && response.authorization_url) {
      popup.location.href = response.authorization_url;
    }
    const startedAt = Date.now();
    while (Date.now() - startedAt < 300000) {
      const session = await fetchProviderBrowserAuthSession(response.session_id);
      setAuthStatus(`${session.display_name}: ${session.status}`);
      if (session.status !== "pending") {
        break;
      }
      await new Promise((resolve) => window.setTimeout(resolve, 1500));
    }
    await refresh();
  }

  async function discoverModels(form: HTMLFormElement) {
    const { providerId } = readProviderDraft(form, providerPreset);
    setProviderModels(
      await getJson<string[]>(`/v1/providers/${encodeURIComponent(providerId)}/models`)
    );
  }

  return (
    <div className="split-panels">
      <Panel eyebrow="Providers" title="Provider setup">
        <form className="stack-list" id="provider-form" onSubmit={saveProvider}>
          <label className="field">
            <span>Preset</span>
            <select
              id="provider-preset"
              value={providerPreset}
              onChange={(event) => setProviderPreset(event.target.value)}
            >
              {PROVIDER_PRESETS.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.label}
                </option>
              ))}
            </select>
          </label>
          {(() => {
            const preset = PROVIDER_PRESETS.find((entry) => entry.id === providerPreset)!;
            return (
              <div key={providerPreset}>
                <div className="grid-three">
                  <label className="field"><span>Provider ID</span><input id="provider-id" name="id" defaultValue={preset.id === "custom" ? "" : slugify(preset.displayName)} /></label>
                  <label className="field"><span>Display name</span><input id="provider-display-name" name="display_name" defaultValue={preset.displayName} /></label>
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
                <div className="grid-three">
                  <label className="field"><span>Base URL</span><input id="provider-base-url" name="base_url" defaultValue={preset.baseUrl} /></label>
                  <label className="field">
                    <span>Auth mode</span>
                    <select id="provider-auth-mode" name="auth_mode" defaultValue={preset.authMode}>
                      <option value="none">none</option>
                      <option value="api_key">api_key</option>
                      <option value="oauth">oauth</option>
                    </select>
                  </label>
                  <label className="field"><span>Default model</span><input id="provider-default-model" name="default_model" defaultValue={preset.defaultModel} /></label>
                </div>
                <label className="field"><span><input name="local" type="checkbox" defaultChecked={preset.local} /> Local provider</span></label>
                <label className="field"><span>API key</span><input name="api_key" type="password" autoComplete="off" /></label>
                <label className="field"><span>OAuth config JSON</span><textarea name="oauth_json" placeholder='{"client_id":"..."}' /></label>
                <label className="field"><span>OAuth token JSON</span><textarea name="oauth_token_json" placeholder='{"access_token":"..."}' /></label>
                <div className="grid-three">
                  <label className="field"><span>Alias name</span><input id="provider-alias-name" name="alias_name" placeholder="main" /></label>
                  <label className="field"><span>Alias model</span><input name="alias_model" placeholder={preset.defaultModel} /></label>
                  <label className="field"><span>Alias description</span><input name="alias_description" /></label>
                </div>
                <label className="field"><span><input name="set_as_main" type="checkbox" /> Set as main alias</span></label>
                <div className="button-row">
                  <button id="provider-save" type="submit">Save provider</button>
                  {preset.browserAuthKind ? (
                    <button
                      id="provider-browser-auth"
                      type="button"
                      onClick={(event) => {
                        const form = event.currentTarget.form;
                        if (form) {
                          void beginBrowserAuth(form);
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
                        void discoverModels(form);
                      }
                    }}
                  >
                    Discover models
                  </button>
                </div>
              </div>
            );
          })()}
        </form>
        {authStatus ? <p className="helper-copy" id="provider-auth-status">{authStatus}</p> : null}
        {providerModels.length ? (
          <div className="stack-list" id="provider-models">
            {providerModels.map((model) => (
              <article key={model} className="stack-card"><strong>{model}</strong></article>
            ))}
          </div>
        ) : null}
      </Panel>

      <Panel eyebrow="Aliases" title="Configured targets">
        <div className="stack-list" id="providers-list">
          {bootstrap.providers.map((provider) => (
            <article key={provider.id} className="stack-card">
              <div className="stack-card__title">
                <strong>{provider.display_name}</strong>
                <span>{provider.kind}</span>
              </div>
              <p className="stack-card__subtitle">{provider.id}</p>
              <div className="button-row">
                <button type="button" onClick={() => void getJson<string[]>(`/v1/providers/${encodeURIComponent(provider.id)}/models`).then(setProviderModels)}>
                  Models
                </button>
                <button type="button" onClick={() => void deleteJson(`/v1/providers/${encodeURIComponent(provider.id)}/credentials`).then(refresh)}>
                  Clear creds
                </button>
                <button type="button" onClick={() => void deleteJson(`/v1/providers/${encodeURIComponent(provider.id)}`).then(refresh)}>
                  Remove
                </button>
              </div>
            </article>
          ))}
        </div>
        <div className="stack-list" id="aliases-list">
          {bootstrap.aliases.map((alias) => (
            <article key={alias.alias} className="stack-card">
              <div className="stack-card__title">
                <strong>{alias.alias}</strong>
                <span>{alias.model}</span>
              </div>
              <p className="stack-card__subtitle">{alias.provider_id}</p>
              <div className="button-row">
                <button type="button" onClick={() => void putJson("/v1/main-alias", { alias: alias.alias }).then(refresh)}>
                  Make main
                </button>
                <button type="button" onClick={() => void deleteJson(`/v1/aliases/${encodeURIComponent(alias.alias)}`).then(refresh)}>
                  Remove
                </button>
              </div>
            </article>
          ))}
        </div>
      </Panel>
    </div>
  );
}
