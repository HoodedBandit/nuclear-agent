import { FormEvent, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useDashboardData } from "../../app/useDashboardData";
import {
  discoverProvider,
  listProviders,
  saveProvider,
  validateProvider
} from "../../api/client";
import type {
  AuthMode,
  ProviderConfig,
  ProviderKind,
  ProviderProfile,
  ProviderReadinessResult,
  ProviderUpsertRequest
} from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import styles from "./IntegrationsPage.module.css";

type ProviderPresetId =
  | "openai"
  | "moonshot"
  | "openrouter"
  | "venice"
  | "anthropic"
  | "ollama"
  | "local_openai";

interface ProviderPreset {
  id: ProviderPresetId;
  label: string;
  providerKind: ProviderKind;
  providerProfile: ProviderProfile;
  displayName: string;
  providerId: string;
  baseUrl: string;
  authMode: AuthMode;
  local: boolean;
  apiKeyLabel?: string;
  apiKeyPlaceholder?: string;
  defaultModelPlaceholder: string;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "openai",
    label: "OpenAI",
    providerKind: "open_ai_compatible",
    providerProfile: "open_ai",
    displayName: "OpenAI",
    providerId: "openai",
    baseUrl: "https://api.openai.com/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "OpenAI API key",
    apiKeyPlaceholder: "sk-...",
    defaultModelPlaceholder: "gpt-5.4"
  },
  {
    id: "moonshot",
    label: "Moonshot (Kimi)",
    providerKind: "open_ai_compatible",
    providerProfile: "moonshot",
    displayName: "Moonshot",
    providerId: "moonshot",
    baseUrl: "https://api.moonshot.ai/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "Moonshot API key",
    apiKeyPlaceholder: "sk-...",
    defaultModelPlaceholder: "kimi-k2.5"
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    providerKind: "open_ai_compatible",
    providerProfile: "open_router",
    displayName: "OpenRouter",
    providerId: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "OpenRouter API key",
    apiKeyPlaceholder: "sk-or-...",
    defaultModelPlaceholder: "openai/gpt-5.4"
  },
  {
    id: "venice",
    label: "Venice",
    providerKind: "open_ai_compatible",
    providerProfile: "venice",
    displayName: "Venice AI",
    providerId: "venice",
    baseUrl: "https://api.venice.ai/api/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "Venice API key",
    apiKeyPlaceholder: "venice-...",
    defaultModelPlaceholder: "venice-uncensored"
  },
  {
    id: "anthropic",
    label: "Anthropic",
    providerKind: "anthropic",
    providerProfile: "anthropic",
    displayName: "Anthropic",
    providerId: "anthropic",
    baseUrl: "https://api.anthropic.com",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "Anthropic API key",
    apiKeyPlaceholder: "sk-ant-...",
    defaultModelPlaceholder: "claude-sonnet-4.5"
  },
  {
    id: "ollama",
    label: "Ollama",
    providerKind: "ollama",
    providerProfile: "ollama",
    displayName: "Ollama local",
    providerId: "ollama-local",
    baseUrl: "http://127.0.0.1:11434",
    authMode: "none",
    local: true,
    defaultModelPlaceholder: "qwen2.5-coder:7b"
  },
  {
    id: "local_openai",
    label: "Local OpenAI-compatible",
    providerKind: "open_ai_compatible",
    providerProfile: "local_open_ai_compatible",
    displayName: "Local OpenAI-compatible",
    providerId: "local-openai",
    baseUrl: "http://127.0.0.1:5001/v1",
    authMode: "none",
    local: true,
    defaultModelPlaceholder: "custom-model"
  }
];

const CONNECTOR_SECTIONS = [
  { label: "Inbox", itemsKey: "inbox_connectors" },
  { label: "Telegram", itemsKey: "telegram_connectors" },
  { label: "Discord", itemsKey: "discord_connectors" },
  { label: "Slack", itemsKey: "slack_connectors" },
  { label: "Home Assistant", itemsKey: "home_assistant_connectors" },
  { label: "Signal", itemsKey: "signal_connectors" },
  { label: "Gmail", itemsKey: "gmail_connectors" },
  { label: "Brave", itemsKey: "brave_connectors" },
  { label: "Webhook", itemsKey: "webhook_connectors" }
] as const;

interface ProviderFormState {
  presetId: ProviderPresetId;
  id: string;
  display_name: string;
  kind: ProviderKind;
  base_url: string;
  provider_profile: ProviderProfile;
  auth_mode: AuthMode;
  default_model: string;
  api_key: string;
  local: boolean;
}

function presetById(id: ProviderPresetId): ProviderPreset {
  return PROVIDER_PRESETS.find((preset) => preset.id === id) ?? PROVIDER_PRESETS[0];
}

function buildInitialFormState(presetId: ProviderPresetId = "ollama"): ProviderFormState {
  const preset = presetById(presetId);
  return {
    presetId: preset.id,
    id: preset.providerId,
    display_name: preset.displayName,
    kind: preset.providerKind,
    base_url: preset.baseUrl,
    provider_profile: preset.providerProfile,
    auth_mode: preset.authMode,
    default_model: "",
    api_key: "",
    local: preset.local
  };
}

function buildProviderPayload(formState: ProviderFormState): ProviderUpsertRequest {
  const provider: ProviderConfig = {
    id: formState.id.trim(),
    display_name: formState.display_name.trim(),
    kind: formState.kind,
    base_url: formState.base_url.trim(),
    provider_profile: formState.provider_profile,
    auth_mode: formState.auth_mode,
    default_model: formState.default_model.trim() || null,
    keychain_account: null,
    local: formState.local
  };

  return {
    provider,
    api_key:
      formState.auth_mode === "api_key"
        ? formState.api_key.trim() || null
        : null,
    oauth_token: null
  };
}

function discoveryKeyForSecret(secret: string): string {
  let hash = 0;
  for (let index = 0; index < secret.length; index += 1) {
    hash = (hash * 31 + secret.charCodeAt(index)) >>> 0;
  }
  return `${secret.length}:${hash.toString(16)}`;
}

export function IntegrationsPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const providersQuery = useQuery({
    queryKey: ["providers"],
    queryFn: listProviders,
    initialData: bootstrap.providers
  });
  const [formState, setFormState] = useState<ProviderFormState>(() => buildInitialFormState());
  const [lastValidated, setLastValidated] = useState<ProviderReadinessResult | null>(null);

  const selectedPreset = useMemo(
    () => presetById(formState.presetId),
    [formState.presetId]
  );
  const secretDiscoveryKey = useMemo(
    () => discoveryKeyForSecret(formState.api_key),
    [formState.api_key]
  );

  const discoveryPayload = useMemo(() => buildProviderPayload(formState), [formState]);
  const canDiscoverModels =
    discoveryPayload.provider.id.length > 0 &&
    discoveryPayload.provider.base_url.length > 0 &&
    (discoveryPayload.provider.auth_mode !== "api_key" ||
      (discoveryPayload.api_key ?? "").length > 0);

  const modelDiscoveryQuery = useQuery({
    queryKey: [
      "provider-model-discovery",
      discoveryPayload.provider.provider_profile ?? "",
      discoveryPayload.provider.kind,
      discoveryPayload.provider.id,
      discoveryPayload.provider.base_url,
      discoveryPayload.provider.auth_mode,
      secretDiscoveryKey
    ],
    queryFn: async () => discoverProvider(discoveryPayload),
    enabled: canDiscoverModels,
    retry: false,
    staleTime: 30_000
  });

  useEffect(() => {
    const recommendedModel = modelDiscoveryQuery.data?.recommended_model?.trim();
    if (!recommendedModel) {
      return;
    }
    if (formState.default_model.trim().length > 0) {
      return;
    }
    setFormState((current) => ({
      ...current,
      default_model: recommendedModel
    }));
  }, [formState.default_model, modelDiscoveryQuery.data?.recommended_model]);

  useEffect(() => {
    setLastValidated(null);
  }, [formState]);

  const saveProviderMutation = useMutation({
    mutationFn: async () => {
      const payload = buildProviderPayload(formState);
      const readiness = await validateProvider(payload);
      if (!readiness.ok) {
        throw new Error(readiness.detail);
      }
      setLastValidated(readiness);
      await saveProvider(payload);
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["providers"] })
      ]);
      setFormState(buildInitialFormState(formState.presetId));
    },
    onError: () => {
      setLastValidated(null);
    }
  });

  const connectorCounts = useMemo(
    () =>
      CONNECTOR_SECTIONS.map((section) => ({
        label: section.label,
        count: bootstrap[section.itemsKey].length
      })),
    [bootstrap]
  );

  const discoveredModels = modelDiscoveryQuery.data?.models?.map((model) => model.id) ?? [];
  const saveDisabled =
    saveProviderMutation.isPending ||
    (canDiscoverModels && modelDiscoveryQuery.isPending && formState.default_model.trim().length === 0);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void saveProviderMutation.mutateAsync();
  }

  return (
    <div className={styles.page} data-testid="modern-integrations-page">
      <div className={styles.grid}>
        <Surface eyebrow="Providers" title="Configured provider stack" emphasis="accent">
          {providersQuery.data && providersQuery.data.length > 0 ? (
            <div className={styles.list}>
              {providersQuery.data.map((provider) => (
                <article key={provider.id} className={styles.listCard}>
                  <div>
                    <strong>{provider.display_name}</strong>
                    <div className={styles.meta}>{provider.id}</div>
                    <div className={styles.meta}>{provider.base_url}</div>
                  </div>
                  <div className={styles.capabilityBlock}>
                    <Pill tone={provider.local ? "good" : "accent"}>
                      {provider.local ? "Local" : "Hosted"}
                    </Pill>
                    <span className={styles.meta}>{provider.default_model ?? "No default model"}</span>
                    <span className={styles.meta}>
                      {provider.auth_mode === "api_key" ? "API key" : provider.auth_mode}
                    </span>
                  </div>
                </article>
              ))}
            </div>
          ) : (
            <EmptyState
              title="No providers configured"
              body="The modern cockpit covers direct API-key onboarding and local runtimes. Classic tools still handle ChatGPT/Codex browser-session auth and long-tail recovery flows."
            />
          )}
        </Surface>

        <Surface eyebrow="Workbench" title="Create provider">
          <form className={styles.form} onSubmit={handleSubmit}>
            <label>
              Provider preset
              <select
                value={formState.presetId}
                onChange={(event) => {
                  setFormState(buildInitialFormState(event.target.value as ProviderPresetId));
                }}
              >
                {PROVIDER_PRESETS.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.label}
                  </option>
                ))}
              </select>
            </label>

            <label>
              Provider ID
              <input
                value={formState.id}
                onChange={(event) => setFormState((current) => ({ ...current, id: event.target.value }))}
                placeholder={selectedPreset.providerId}
                required
              />
            </label>

            <label>
              Display name
              <input
                value={formState.display_name}
                onChange={(event) =>
                  setFormState((current) => ({ ...current, display_name: event.target.value }))
                }
                placeholder={selectedPreset.displayName}
                required
              />
            </label>

            <label>
              Base URL
              <input
                value={formState.base_url}
                onChange={(event) =>
                  setFormState((current) => ({ ...current, base_url: event.target.value }))
                }
                required
              />
            </label>

            <label>
              Provider profile
              <input value={formState.provider_profile} readOnly />
            </label>

            {formState.auth_mode === "api_key" ? (
              <label>
                {selectedPreset.apiKeyLabel ?? "API key"}
                <input
                  type="password"
                  value={formState.api_key}
                  onChange={(event) =>
                    setFormState((current) => ({ ...current, api_key: event.target.value }))
                  }
                  placeholder={selectedPreset.apiKeyPlaceholder ?? "Paste API key"}
                  autoComplete="off"
                  required
                />
              </label>
            ) : null}

            <label>
              Default model
              <input
                list="provider-discovered-models"
                value={formState.default_model}
                onChange={(event) =>
                  setFormState((current) => ({ ...current, default_model: event.target.value }))
                }
                placeholder={selectedPreset.defaultModelPlaceholder}
              />
              {discoveredModels.length > 0 ? (
                <datalist id="provider-discovered-models">
                  {discoveredModels.map((model) => (
                    <option key={model} value={model} />
                  ))}
                </datalist>
              ) : null}
            </label>

            <div className={styles.discoveryStatus}>
              {modelDiscoveryQuery.isPending ? (
                <span className={styles.meta}>Loading models...</span>
              ) : null}
              {discoveredModels.length > 0 ? (
                <span className={styles.successCopy}>
                  Loaded {discoveredModels.length} model{discoveredModels.length === 1 ? "" : "s"}.
                </span>
              ) : null}
              {modelDiscoveryQuery.error ? (
                <span className={styles.errorCopy}>
                  {modelDiscoveryQuery.error instanceof Error
                    ? `Could not load models automatically: ${modelDiscoveryQuery.error.message}`
                    : "Could not load models automatically."}
                </span>
              ) : null}
              {modelDiscoveryQuery.data?.warnings?.map((warning) => (
                <span key={warning} className={styles.meta}>
                  {warning}
                </span>
              ))}
              {lastValidated ? (
                <span className={styles.successCopy}>
                  Validated {lastValidated.model}: {lastValidated.detail}
                </span>
              ) : null}
            </div>

            <div className={styles.formActions}>
              <button
                type="submit"
                className={styles.primaryButton}
                data-testid="modern-provider-save"
                disabled={saveDisabled}
              >
                {saveProviderMutation.isPending ? "Saving..." : "Save provider"}
              </button>
              <button
                type="button"
                className={styles.linkButtonGhost}
                onClick={() =>
                  queryClient.invalidateQueries({ queryKey: ["provider-model-discovery"] })
                }
                disabled={!canDiscoverModels || modelDiscoveryQuery.isPending}
              >
                Refresh models
              </button>
              <a className={styles.linkButtonGhost} href="/dashboard-classic">
                Open classic tools
              </a>
            </div>

            {saveProviderMutation.error ? (
              <p className={styles.errorCopy}>
                {saveProviderMutation.error instanceof Error
                  ? saveProviderMutation.error.message
                  : "Provider save failed."}
              </p>
            ) : null}
          </form>
        </Surface>
      </div>

      <div className={styles.secondaryGrid}>
        <Surface eyebrow="Browser session" title="Use the classic workbench for session-based auth">
          <p className={styles.copy}>
            ChatGPT/Codex browser sign-in and long-tail browser-session recovery still use the
            classic dashboard. Anthropic third-party access is API-key-only. The modern cockpit
            covers direct API-key onboarding for hosted providers and local runtime presets.
          </p>
          <div className={styles.formActions}>
            <a className={styles.linkButtonGhost} href="/dashboard-classic">
              Open classic auth tools
            </a>
          </div>
        </Surface>

        <Surface eyebrow="Connectors" title="Migration status">
          <div className={styles.connectorGrid}>
            {connectorCounts.map((item) => (
              <article key={item.label} className={styles.connectorCard}>
                <div>
                  <strong>{item.label}</strong>
                  <div className={styles.meta}>{item.count} configured</div>
                </div>
                <a href="/dashboard-classic" className={styles.inlineLink}>
                  Classic workbench
                </a>
              </article>
            ))}
          </div>
        </Surface>
      </div>
    </div>
  );
}
