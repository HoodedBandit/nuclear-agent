import { FormEvent, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useDashboardData } from "../../app/useDashboardData";
import { listProviders, saveProvider } from "../../api/client";
import type { ProviderConfig, ProviderKind } from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import styles from "./IntegrationsPage.module.css";

const PROVIDER_PRESETS: Array<{
  kind: ProviderKind;
  label: string;
  baseUrl: string;
}> = [
  { kind: "chat_gpt_codex", label: "ChatGPT Codex", baseUrl: "https://chatgpt.com/backend-api" },
  { kind: "anthropic", label: "Anthropic", baseUrl: "https://api.anthropic.com" },
  { kind: "open_ai_compatible", label: "OpenAI Compatible", baseUrl: "https://api.openai.com/v1" },
  { kind: "ollama", label: "Ollama", baseUrl: "http://127.0.0.1:11434" }
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

export function IntegrationsPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const providersQuery = useQuery({
    queryKey: ["providers"],
    queryFn: listProviders,
    initialData: bootstrap.providers
  });
  const [formState, setFormState] = useState({
    id: "",
    display_name: "",
    kind: "ollama" as ProviderKind,
    base_url: "http://127.0.0.1:11434",
    default_model: ""
  });

  const saveProviderMutation = useMutation({
    mutationFn: async () => {
      const provider: ProviderConfig = {
        id: formState.id.trim(),
        display_name: formState.display_name.trim(),
        kind: formState.kind,
        base_url: formState.base_url.trim(),
        auth_mode: formState.kind === "ollama" ? "none" : "api_key",
        default_model: formState.default_model.trim() || null,
        keychain_account: null,
        local: formState.kind === "ollama"
      };
      await saveProvider(provider);
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["providers"] })
      ]);
      setFormState({
        id: "",
        display_name: "",
        kind: "ollama",
        base_url: "http://127.0.0.1:11434",
        default_model: ""
      });
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
                  </div>
                  <div className={styles.capabilityBlock}>
                    <Pill tone={provider.local ? "good" : "accent"}>
                      {provider.local ? "Local" : "Hosted"}
                    </Pill>
                    <span className={styles.meta}>{provider.default_model ?? "No default model"}</span>
                  </div>
                </article>
              ))}
            </div>
          ) : (
            <EmptyState
              title="No providers configured"
              body="The staged cockpit can create providers directly, while the classic dashboard remains available for the full legacy workbench."
            />
          )}
        </Surface>

        <Surface eyebrow="Workbench" title="Create provider">
          <form className={styles.form} onSubmit={handleSubmit}>
            <label>
              Provider preset
              <select
                value={formState.kind}
                onChange={(event) => {
                  const selected = PROVIDER_PRESETS.find((preset) => preset.kind === event.target.value);
                  setFormState((current) => ({
                    ...current,
                    kind: event.target.value as ProviderKind,
                    base_url: selected?.baseUrl ?? current.base_url
                  }));
                }}
              >
                {PROVIDER_PRESETS.map((preset) => (
                  <option key={preset.kind} value={preset.kind}>
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
                placeholder="ollama-local"
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
                placeholder="Ollama local"
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
              Default model
              <input
                value={formState.default_model}
                onChange={(event) =>
                  setFormState((current) => ({ ...current, default_model: event.target.value }))
                }
                placeholder="qwen2.5-coder:7b"
              />
            </label>
            <div className={styles.formActions}>
              <button type="submit" className={styles.primaryButton} data-testid="modern-provider-save">
                {saveProviderMutation.isPending ? "Saving…" : "Save provider"}
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
  );
}
