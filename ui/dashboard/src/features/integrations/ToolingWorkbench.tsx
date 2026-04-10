import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { AppConnectorConfig, McpServerConfig } from "../../api/types";
import { deleteJson, getJson, postJson } from "../../api/client";
import { Panel } from "../../components/Panel";

function parseList(value: FormDataEntryValue | null) {
  return String(value || "")
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export function ToolingWorkbench() {
  const queryClient = useQueryClient();
  const mcpQuery = useQuery({
    queryKey: ["mcp"],
    queryFn: () => getJson<McpServerConfig[]>("/v1/mcp")
  });
  const appsQuery = useQuery({
    queryKey: ["apps"],
    queryFn: () => getJson<AppConnectorConfig[]>("/v1/apps")
  });

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["mcp"] }),
      queryClient.invalidateQueries({ queryKey: ["apps"] })
    ]);
  }

  async function saveMcp(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    await postJson("/v1/mcp", {
      server: {
        id: String(form.get("id") || "").trim(),
        name: String(form.get("name") || "").trim(),
        description: String(form.get("description") || "").trim(),
        command: String(form.get("command") || "").trim(),
        args: parseList(form.get("args")),
        tool_name: String(form.get("tool_name") || "").trim(),
        input_schema_json: String(form.get("input_schema_json") || "{}").trim(),
        enabled: Boolean(form.get("enabled")),
        cwd: String(form.get("cwd") || "").trim() || null
      }
    });
    formElement.reset();
    await refresh();
  }

  async function saveApp(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    await postJson("/v1/apps", {
      connector: {
        id: String(form.get("id") || "").trim(),
        name: String(form.get("name") || "").trim(),
        description: String(form.get("description") || "").trim(),
        command: String(form.get("command") || "").trim(),
        args: parseList(form.get("args")),
        tool_name: String(form.get("tool_name") || "").trim(),
        input_schema_json: String(form.get("input_schema_json") || "{}").trim(),
        enabled: Boolean(form.get("enabled")),
        cwd: String(form.get("cwd") || "").trim() || null
      }
    });
    formElement.reset();
    await refresh();
  }

  return (
    <div className="split-panels">
      <Panel eyebrow="MCP" title="MCP servers">
        <form className="stack-list" onSubmit={saveMcp}>
          <label className="field"><span>ID</span><input name="id" required /></label>
          <label className="field"><span>Name</span><input name="name" required /></label>
          <label className="field"><span>Description</span><input name="description" /></label>
          <label className="field"><span>Command</span><input name="command" required /></label>
          <label className="field"><span>Args</span><input name="args" /></label>
          <label className="field"><span>Tool name</span><input name="tool_name" required /></label>
          <label className="field"><span>Input schema JSON</span><textarea name="input_schema_json" defaultValue="{}" /></label>
          <label className="field"><span><input type="checkbox" name="enabled" defaultChecked /> Enabled</span></label>
          <label className="field"><span>CWD</span><input name="cwd" /></label>
          <button type="submit">Save MCP server</button>
        </form>
        <div className="stack-list">
          {mcpQuery.data?.map((server) => (
            <article key={server.id} className="stack-card">
              <div className="stack-card__title">
                <strong>{server.name}</strong>
                <span>{server.enabled ? "enabled" : "disabled"}</span>
              </div>
              <p className="stack-card__subtitle">{server.command}</p>
              <button type="button" onClick={() => void deleteJson(`/v1/mcp/${encodeURIComponent(server.id)}`).then(refresh)}>
                Remove
              </button>
            </article>
          ))}
        </div>
      </Panel>
      <Panel eyebrow="Apps" title="Command apps">
        <form className="stack-list" onSubmit={saveApp}>
          <label className="field"><span>ID</span><input name="id" required /></label>
          <label className="field"><span>Name</span><input name="name" required /></label>
          <label className="field"><span>Description</span><input name="description" /></label>
          <label className="field"><span>Command</span><input name="command" required /></label>
          <label className="field"><span>Args</span><input name="args" /></label>
          <label className="field"><span>Tool name</span><input name="tool_name" required /></label>
          <label className="field"><span>Input schema JSON</span><textarea name="input_schema_json" defaultValue="{}" /></label>
          <label className="field"><span><input type="checkbox" name="enabled" defaultChecked /> Enabled</span></label>
          <label className="field"><span>CWD</span><input name="cwd" /></label>
          <button type="submit">Save app connector</button>
        </form>
        <div className="stack-list">
          {appsQuery.data?.map((connector) => (
            <article key={connector.id} className="stack-card">
              <div className="stack-card__title">
                <strong>{connector.name}</strong>
                <span>{connector.enabled ? "enabled" : "disabled"}</span>
              </div>
              <p className="stack-card__subtitle">{connector.command}</p>
              <button type="button" onClick={() => void deleteJson(`/v1/apps/${encodeURIComponent(connector.id)}`).then(refresh)}>
                Remove
              </button>
            </article>
          ))}
        </div>
      </Panel>
    </div>
  );
}
