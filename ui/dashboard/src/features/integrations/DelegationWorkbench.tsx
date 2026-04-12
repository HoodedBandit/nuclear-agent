import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { DelegationTarget } from "../../api/types";
import { getJson, putJson } from "../../api/client";
import { useDelegationBootstrap } from "../../app/dashboard-selectors";
import { EmptyState } from "../../components/EmptyState";
import { Panel } from "../../components/Panel";

function parseList(value: FormDataEntryValue | null) {
  return String(value || "")
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export function DelegationWorkbench() {
  const { delegationConfig } = useDelegationBootstrap();
  const queryClient = useQueryClient();
  const delegationTargetsQuery = useQuery({
    queryKey: ["delegation-targets"],
    queryFn: () => getJson<DelegationTarget[]>("/v1/delegation/targets")
  });

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
      queryClient.invalidateQueries({ queryKey: ["delegation-targets"] })
    ]);
  }

  async function updateDelegation(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const depth = String(form.get("max_depth") || "1");
    const parallel = String(form.get("max_parallel_subagents") || "8");
    await putJson("/v1/delegation/config", {
      max_depth: depth === "unlimited" ? { mode: "unlimited" } : { mode: "limited", value: Number(depth) },
      max_parallel_subagents:
        parallel === "unlimited"
          ? { mode: "unlimited" }
          : { mode: "limited", value: Number(parallel) },
      disabled_provider_ids: parseList(form.get("disabled_provider_ids"))
    });
    await refresh();
  }

  return (
    <div className="split-panels">
      <Panel eyebrow="Delegation" title="Delegation policy">
        <form className="stack-list" onSubmit={updateDelegation}>
          <label className="field">
            <span>Max depth</span>
            <input
              name="max_depth"
              defaultValue={
                delegationConfig.max_depth.mode === "limited"
                  ? delegationConfig.max_depth.value
                  : "unlimited"
              }
            />
          </label>
          <label className="field">
            <span>Max parallel subagents</span>
            <input
              name="max_parallel_subagents"
              defaultValue={
                delegationConfig.max_parallel_subagents.mode === "limited"
                  ? delegationConfig.max_parallel_subagents.value
                  : "unlimited"
              }
            />
          </label>
          <label className="field">
            <span>Disabled provider IDs</span>
            <input
              name="disabled_provider_ids"
              defaultValue={delegationConfig.disabled_provider_ids.join(", ")}
            />
          </label>
          <button type="submit">Update delegation config</button>
        </form>
      </Panel>
      <Panel eyebrow="Targets" title="Available targets">
        <div className="stack-list">
          {delegationTargetsQuery.data?.length ? (
            delegationTargetsQuery.data.map((target) => (
              <article key={`${target.alias}-${target.provider_id}`} className="stack-card">
                <div className="stack-card__title">
                  <strong>{target.alias}</strong>
                  <span>{target.primary ? "primary" : "available"}</span>
                </div>
                <p className="stack-card__subtitle">
                  {target.provider_display_name} / {target.model}
                </p>
              </article>
            ))
          ) : (
            <EmptyState title="No delegation targets" copy="Configured aliases surface here." />
          )}
        </div>
      </Panel>
    </div>
  );
}
