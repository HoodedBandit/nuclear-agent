import type { FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { deleteJson, postJson } from "../../api/client";
import { useDashboardData } from "../../app/dashboard-data";
import { Panel } from "../../components/Panel";
import {
  CONNECTOR_DEFINITIONS,
  type ConnectorDefinition
} from "./catalog";

function parseList(value: FormDataEntryValue | null) {
  return String(value || "")
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function parseIntList(value: FormDataEntryValue | null) {
  return parseList(value)
    .map((entry) => Number(entry))
    .filter((entry) => Number.isFinite(entry));
}

function slugify(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

function renderConnectorDetails(entry: Record<string, unknown>) {
  const details: string[] = [];

  if (typeof entry.path === "string" && entry.path.trim()) {
    details.push(entry.path);
  }
  if (typeof entry.base_url === "string" && entry.base_url.trim()) {
    details.push(entry.base_url);
  }
  if (typeof entry.account === "string" && entry.account.trim()) {
    details.push(`Account ${entry.account}`);
  }
  if (typeof entry.alias === "string" && entry.alias.trim()) {
    details.push(`Alias ${entry.alias}`);
  }
  if (typeof entry.requested_model === "string" && entry.requested_model.trim()) {
    details.push(`Model ${entry.requested_model}`);
  }
  if (typeof entry.cwd === "string" && entry.cwd.trim()) {
    details.push(`CWD ${entry.cwd}`);
  }
  if (entry.delete_after_read === true) {
    details.push("Deletes after read");
  }
  if (entry.require_pairing_approval === true) {
    details.push("Pairing approval required");
  }

  return details;
}

export function ConnectorWorkbench() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const [connectorKind, setConnectorKind] = useState<ConnectorDefinition["id"]>("telegram");
  const connectorDefinition = useMemo(
    () => CONNECTOR_DEFINITIONS.find((entry) => entry.id === connectorKind)!,
    [connectorKind]
  );

  async function refresh() {
    await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
  }

  async function saveConnector(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    const connector: Record<string, unknown> = {};
    for (const field of connectorDefinition.fields) {
      const raw = form.get(field.id);
      if (field.type === "checkbox") {
        connector[field.id] = Boolean(raw);
      } else if (field.type === "string_list") {
        connector[field.id] = parseList(raw);
      } else if (field.type === "int_list") {
        connector[field.id] = parseIntList(raw);
      } else if (field.type === "textarea" || field.type === "text") {
        connector[field.id] = String(raw || "").trim() || null;
      }
    }
    connector.id =
      (connector.id as string | null) || slugify(String(connector.name || connectorDefinition.label));
    connector.name = connector.name || connectorDefinition.label;
    connector.description = connector.description || "";
    const payload: Record<string, unknown> = { connector };
    if (connectorDefinition.secretField) {
      payload[connectorDefinition.secretField] =
        String(form.get(connectorDefinition.secretField) || "").trim() || null;
    }
    await postJson(connectorDefinition.endpoint, payload);
    formElement.reset();
    await refresh();
  }

  async function connectorAction(endpoint: string, id: string, action: "delete" | "poll") {
    const path = `${endpoint}/${encodeURIComponent(id)}`;
    if (action === "delete") {
      await deleteJson(path);
    } else {
      await postJson(`${path}/poll`, {});
    }
    await refresh();
  }

  const connectorCards = [
    ["/v1/webhooks", bootstrap.webhook_connectors],
    ["/v1/inboxes", bootstrap.inbox_connectors],
    ["/v1/telegram", bootstrap.telegram_connectors],
    ["/v1/discord", bootstrap.discord_connectors],
    ["/v1/slack", bootstrap.slack_connectors],
    ["/v1/signal", bootstrap.signal_connectors],
    ["/v1/home-assistant", bootstrap.home_assistant_connectors],
    ["/v1/gmail", bootstrap.gmail_connectors],
    ["/v1/brave", bootstrap.brave_connectors]
  ] as const;

  return (
    <div className="split-panels">
      <Panel eyebrow="Connectors" title="Connector setup">
        <form className="stack-list" id="connector-form" onSubmit={saveConnector}>
          <label className="field">
            <span>Connector kind</span>
            <select
              id="connector-kind"
              value={connectorKind}
              onChange={(event) => setConnectorKind(event.target.value as ConnectorDefinition["id"])}
            >
              {CONNECTOR_DEFINITIONS.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>
          <div key={connectorDefinition.id}>
            {connectorDefinition.fields.map((field) => (
              <label key={field.id} className="field">
                <span>
                  {field.type === "checkbox" ? (
                    <>
                      <input type="checkbox" name={field.id} defaultChecked={Boolean(field.defaultValue)} /> {field.label}
                    </>
                  ) : (
                    field.label
                  )}
                </span>
                {field.type === "textarea" ? (
                  <textarea
                    id={`connector-${field.id}`}
                    name={field.id}
                    placeholder={field.placeholder}
                  />
                ) : field.type === "checkbox" ? null : (
                  <input
                    id={`connector-${field.id}`}
                    name={field.id}
                    type={field.type === "secret" ? "password" : "text"}
                    placeholder={field.placeholder}
                    autoComplete="off"
                  />
                )}
              </label>
            ))}
          </div>
          <button id="connector-save" type="submit">Save connector</button>
        </form>
      </Panel>

      <Panel eyebrow="Roster" title="Installed connectors">
        <div className="stack-list" id="connector-roster">
          {connectorCards.flatMap(([endpoint, entries]) =>
            entries.map((entry) => {
              const details = renderConnectorDetails(entry as unknown as Record<string, unknown>);
              return (
                <article key={`${endpoint}-${entry.id}`} className="stack-card">
                  <div className="stack-card__title">
                    <strong>{entry.name}</strong>
                    <span>{entry.enabled ? "enabled" : "disabled"}</span>
                  </div>
                  <p className="stack-card__subtitle">{entry.id}</p>
                  {entry.description ? (
                    <p className="stack-card__copy">{entry.description}</p>
                  ) : null}
                  {details.length ? (
                    <p className="stack-card__copy mono">{details.join(" | ")}</p>
                  ) : null}
                  {!entry.description && details.length === 0 ? (
                    <p className="stack-card__copy">No description</p>
                  ) : null}
                  <div className="button-row">
                    {endpoint !== "/v1/webhooks" && endpoint !== "/v1/brave" ? (
                      <button type="button" onClick={() => void connectorAction(endpoint, entry.id, "poll")}>
                        Poll
                      </button>
                    ) : null}
                    <button type="button" onClick={() => void connectorAction(endpoint, entry.id, "delete")}>
                      Remove
                    </button>
                  </div>
                </article>
              );
            })
          )}
        </div>
      </Panel>
    </div>
  );
}
