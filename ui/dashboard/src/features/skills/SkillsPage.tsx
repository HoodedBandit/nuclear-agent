import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, postJson, putJson } from "../../api/client";
import type { MemoryRecord, SkillDraft } from "../../api/types";
import { SkillsTab } from "../operations/tabs/SkillsTab";

export function SkillsPage() {
  const queryClient = useQueryClient();
  const profileQuery = useQuery({
    queryKey: ["profile-memory"],
    queryFn: () => getJson<MemoryRecord[]>("/v1/memory/profile?limit=25")
  });
  const skillDraftsQuery = useQuery({
    queryKey: ["skill-drafts"],
    queryFn: () => getJson<SkillDraft[]>("/v1/skills/drafts")
  });
  const skillsQuery = useQuery({
    queryKey: ["enabled-skills"],
    queryFn: () => getJson<string[]>("/v1/skills")
  });

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["profile-memory"] }),
      queryClient.invalidateQueries({ queryKey: ["skill-drafts"] }),
      queryClient.invalidateQueries({ queryKey: ["enabled-skills"] }),
      queryClient.invalidateQueries({ queryKey: ["bootstrap"] })
    ]);
  }

  async function publishDraft(draftId: string, action: "publish" | "reject") {
    await postJson(`/v1/skills/drafts/${encodeURIComponent(draftId)}/${action}`, {});
    await refresh();
  }

  async function updateEnabledSkills(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const raw = new FormData(event.currentTarget)
      .get("skills")
      ?.toString()
      .split(",")
      .map((entry) => entry.trim())
      .filter(Boolean);
    await putJson("/v1/skills", { enabled_skills: raw || [] });
    await refresh();
  }

  return (
    <div className="page-stack">
      <SkillsTab
        enabledSkills={skillsQuery.data}
        profileMemories={profileQuery.data}
        skillDrafts={skillDraftsQuery.data}
        onUpdateSkills={updateEnabledSkills}
        onPublishDraft={(draftId, action) => {
          void publishDraft(draftId, action);
        }}
      />
    </div>
  );
}
