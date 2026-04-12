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
import { useProviderBootstrap } from "../../app/dashboard-selectors";
import { ProviderInventoryPanel } from "./providers/ProviderInventoryPanel";
import { ProviderSetupPanel } from "./providers/ProviderSetupPanel";
import { parseOptionalJson, readProviderDraft } from "./providers/draft";

export function ProviderWorkbench() {
  const { providers, aliases } = useProviderBootstrap();
  const queryClient = useQueryClient();
  const [providerPreset, setProviderPreset] = useState("codex");
  const [providerModels, setProviderModels] = useState<string[]>([]);
  const [authStatus, setAuthStatus] = useState<string | null>(null);
  const [inventoryView, setInventoryView] = useState<"providers" | "aliases">("providers");

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
          model: String(data.get("alias_model") || "").trim() || defaultModel,
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
    if (!preset.browserAuthKind) {
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
      <ProviderSetupPanel
        providerPreset={providerPreset}
        providerModels={providerModels}
        authStatus={authStatus}
        onProviderPresetChange={setProviderPreset}
        onSubmit={saveProvider}
        onBeginBrowserAuth={(form) => {
          void beginBrowserAuth(form);
        }}
        onDiscoverModels={(form) => {
          void discoverModels(form);
        }}
      />

      <ProviderInventoryPanel
        providers={providers}
        aliases={aliases}
        inventoryView={inventoryView}
        onInventoryViewChange={setInventoryView}
        onFetchModels={(providerId) => {
          void getJson<string[]>(`/v1/providers/${encodeURIComponent(providerId)}/models`).then(
            setProviderModels
          );
        }}
        onClearCredentials={(providerId) => {
          void deleteJson(`/v1/providers/${encodeURIComponent(providerId)}/credentials`).then(
            refresh
          );
        }}
        onDeleteProvider={(providerId) => {
          void deleteJson(`/v1/providers/${encodeURIComponent(providerId)}`).then(refresh);
        }}
        onMakeMain={(alias) => {
          void putJson("/v1/main-alias", { alias }).then(refresh);
        }}
        onDeleteAlias={(alias) => {
          void deleteJson(`/v1/aliases/${encodeURIComponent(alias)}`).then(refresh);
        }}
      />
    </div>
  );
}
