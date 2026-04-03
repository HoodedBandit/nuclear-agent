async function handleMessagingApprovalCommand(command, rawArgs) {
  const [action, id, ...noteParts] = rawArgs.split(/\s+/).filter(Boolean);
  const kind = command.startsWith("telegram")
    ? "telegram"
    : command.startsWith("discord")
      ? "discord"
      : "slack";
  if (!action) {
    const collection =
      kind === "telegram"
        ? state.lastData?.telegrams || []
        : kind === "discord"
          ? state.lastData?.discords || []
          : state.lastData?.slacks || [];
    focusSection("connectors");
    window.dashboardConnectors?.selectKind?.(kind);
    pushConsoleEntry(`${kind} connectors`, formatConnectorSummary(collection), {
      tone: "info",
      label: "connectors",
    });
    return true;
  }
  if (action === "approvals" || action === "approval") {
    focusSection("approvals");
    const approvals = await apiGet(`/v1/connector-approvals?kind=${kind}&status=pending&limit=25`);
    pushConsoleEntry(
      `${kind} approvals`,
      formatRecordLines(
        approvals,
        (approval) =>
          `${approval.id} connector=${approval.connector_id} chat=${approval.external_chat_display || approval.external_chat_id || "-"} user=${approval.external_user_display || approval.external_user_id || "-"}`
      ),
      { tone: "info", label: "approvals" }
    );
    return true;
  }
  if (action === "approve" || action === "reject") {
    if (!id) {
      throw new Error(`usage: /${kind} ${action} <approval-id> [note]`);
    }
    await apiPost(
      `/v1/connector-approvals/${encodeURIComponent(id)}/${action}`,
      noteParts.length ? { note: noteParts.join(" ") } : {}
    );
    await refreshDashboard();
    pushConsoleEntry(`${kind} approvals`, `${action}d approval ${id}.`, {
      tone: "good",
      label: "approvals",
      preformatted: false,
    });
    return true;
  }
  return false;
}

async function handleMemoryCommand(rawArgs) {
  const lowered = rawArgs.toLowerCase();
  if (lowered === "review") {
    focusSection("memory");
    const queue = await apiGet("/v1/memory/review?limit=25");
    pushConsoleEntry(
      "Memory review",
      formatRecordLines(queue, (memory) => `${memory.id} [${memory.kind}/${memory.scope}] ${memory.subject}\n  ${memory.content}`),
      { tone: "info", label: "memory" }
    );
    return true;
  }
  if (lowered === "rebuild" || lowered.startsWith("rebuild ")) {
    const sessionId = rawArgs.slice("rebuild".length).trim();
    const response = await apiPost("/v1/memory/rebuild", {
      session_id: sessionId || undefined,
      recompute_embeddings: false,
    });
    await refreshDashboard({ includeLoadedPanels: true, forcePanels: ["memory", "profile", "sessions"] });
    pushConsoleEntry(
      "Memory rebuild",
      `sessions=${response.sessions_scanned}\nobservations=${response.observations_scanned}\nmemories=${response.memories_upserted}\nembeddings=${response.embeddings_refreshed}`,
      { tone: "good", label: "memory" }
    );
    return true;
  }
  if (lowered.startsWith("approve ")) {
    const [, id, ...noteParts] = rawArgs.split(/\s+/);
    await apiPost(`/v1/memory/${encodeURIComponent(id)}/approve`, noteParts.length ? { note: noteParts.join(" ") } : {});
    await refreshDashboard();
    pushConsoleEntry("Memory review", `Approved memory ${id}.`, {
      tone: "good",
      label: "memory",
      preformatted: false,
    });
    return true;
  }
  if (lowered.startsWith("reject ")) {
    const [, id, ...noteParts] = rawArgs.split(/\s+/);
    await apiPost(`/v1/memory/${encodeURIComponent(id)}/reject`, noteParts.length ? { note: noteParts.join(" ") } : {});
    await refreshDashboard();
    pushConsoleEntry("Memory review", `Rejected memory ${id}.`, {
      tone: "good",
      label: "memory",
      preformatted: false,
    });
    return true;
  }
  if (rawArgs) {
    focusSection("memory-tools");
    const result = await apiPost("/v1/memory/search", { query: rawArgs });
    const items = [...(result.memories || []), ...(result.transcript_hits || [])];
    pushConsoleEntry(
      "Memory search",
      items.length
        ? items
            .map((item) =>
              item.subject
                ? `${item.id} [${item.kind}/${item.scope}] ${item.subject}\n  ${item.content}`
                : `session=${item.session_id} [${item.role}] ${item.preview}`
            )
            .join("\n")
        : "No matching memory.",
      { tone: "info", label: "memory" }
    );
    return true;
  }
  focusSection("memory-tools");
  const memories = await apiGet("/v1/memory?limit=10");
  pushConsoleEntry(
    "Memory",
    formatRecordLines(memories, (memory) => `${memory.id} [${memory.kind}/${memory.scope}] ${memory.subject}\n  ${memory.content}`),
    { tone: "info", label: "memory" }
  );
  return true;
}

async function handleSkillsCommand(rawArgs) {
  const loweredArgs = rawArgs.toLowerCase();
  if (!rawArgs || ["drafts", "published", "rejected"].includes(loweredArgs)) {
    focusSection("skills");
    const suffix = loweredArgs ? `?status=${encodeURIComponent(loweredArgs === "drafts" ? "draft" : loweredArgs)}` : "";
    const skills = await apiGet(`/v1/skills/drafts${suffix}`);
    pushConsoleEntry(
      "Skills",
      formatRecordLines(skills, (skill) => `${skill.id} [${skill.status}] ${skill.title}`),
      { tone: "info", label: "skills" }
    );
    return true;
  }
  if (loweredArgs.startsWith("publish ")) {
    const id = rawArgs.split(/\s+/)[1];
    await apiPost(`/v1/skills/drafts/${encodeURIComponent(id)}/publish`, {});
    await refreshDashboard();
    pushConsoleEntry("Skills", `Published skill draft ${id}.`, {
      tone: "good",
      label: "skills",
      preformatted: false,
    });
    return true;
  }
  if (loweredArgs.startsWith("reject ")) {
    const id = rawArgs.split(/\s+/)[1];
    await apiPost(`/v1/skills/drafts/${encodeURIComponent(id)}/reject`, {});
    await refreshDashboard();
    pushConsoleEntry("Skills", `Rejected skill draft ${id}.`, {
      tone: "good",
      label: "skills",
      preformatted: false,
    });
    return true;
  }
  throw new Error("usage: /skills [drafts|published|rejected|publish <id>|reject <id>]");
}

async function runSlashCommand(input) {
  const line = String(input || "").trim();
  if (!line.startsWith("/")) {
    return false;
  }
  const body = line.slice(1);
  const spaceIndex = body.search(/\s/);
  const command = (spaceIndex >= 0 ? body.slice(0, spaceIndex) : body).trim().toLowerCase();
  const rawArgs = (spaceIndex >= 0 ? body.slice(spaceIndex + 1) : "").trim();

  switch (command) {
    case "help":
      pushConsoleEntry(
        "Slash commands",
        "/help\n/status\n/config\n/dashboard\n/telegrams\n/telegram approvals\n/telegram approve <approval-id> [note]\n/telegram reject <approval-id> [note]\n/discords\n/discord approvals\n/discord approve <approval-id> [note]\n/discord reject <approval-id> [note]\n/slacks\n/slack approvals\n/slack approve <approval-id> [note]\n/slack reject <approval-id> [note]\n/signals\n/home-assistant\n/webhooks\n/inboxes\n/autopilot [on|pause|resume|status]\n/missions\n/events [limit]\n/schedule <seconds> <title>\n/repeat <seconds> <title>\n/watch <path> <title>\n/profile\n/memory [query]\n/memory review\n/memory rebuild [session-id]\n/memory approve <id> [note]\n/memory reject <id> [note]\n/remember <text>\n/forget <memory-id>\n/skills [drafts|published|rejected]\n/skills publish <draft-id>\n/skills reject <draft-id>\n/alias [name]\n/model [name]\n/providers [name]\n/provider [name]\n/mode [build|daily|default]\n/permissions [preset]\n/approvals [preset]\n/attach <path>\n/attachments\n/detach\n/attachments-clear\n/thinking [level]\n/fast\n/review [instructions]\n/diff\n/copy\n/compact\n/resume [last|session]\n/fork [last|session]\n/rename <title>\n/new\n/clear\n/init\n/onboard\n/exit\n!<command>",
        { tone: "info", label: "help" }
      );
      return true;
    case "status":
      pushConsoleEntry("Status", formatStatusSummary(), { tone: "info", label: "status" });
      return true;
    case "config":
    case "settings":
      focusSection("advanced");
      pushConsoleEntry("Settings", "Opened the system config surface and advanced editor.", {
        tone: "info",
        label: "config",
        preformatted: false,
      });
      return true;
    case "dashboard":
    case "ui":
      activateDashboardTab("chat", { scrollToTop: true });
      pushConsoleEntry("Dashboard", "Already in the dashboard. Switched to the chat workspace.", {
        tone: "good",
        label: "dashboard",
        preformatted: false,
      });
      return true;
    case "telegram":
    case "telegrams":
    case "discord":
    case "discords":
    case "slack":
    case "slacks":
      return handleMessagingApprovalCommand(command, rawArgs);
    case "signal":
    case "signals":
      focusSection("connectors");
      window.dashboardConnectors?.selectKind?.("signal");
      pushConsoleEntry("Signal connectors", formatConnectorSummary(state.lastData?.signals || []), {
        tone: "info",
        label: "connectors",
      });
      return true;
    case "home-assistant":
    case "homeassistant":
    case "homeassistants":
    case "home-assistants":
    case "ha":
      focusSection("connectors");
      window.dashboardConnectors?.selectKind?.("home-assistant");
      pushConsoleEntry(
        "Home Assistant connectors",
        formatConnectorSummary(state.lastData?.homeAssistants || []),
        { tone: "info", label: "connectors" }
      );
      return true;
    case "webhooks":
      focusSection("connectors");
      window.dashboardConnectors?.selectKind?.("webhook");
      pushConsoleEntry("Webhook connectors", formatConnectorSummary(state.lastData?.webhooks || []), {
        tone: "info",
        label: "connectors",
      });
      return true;
    case "inboxes":
      focusSection("connectors");
      window.dashboardConnectors?.selectKind?.("inbox");
      pushConsoleEntry("Inbox connectors", formatConnectorSummary(state.lastData?.inboxes || []), {
        tone: "info",
        label: "connectors",
      });
      return true;
    case "autopilot": {
      const mode = rawArgs.toLowerCase();
      if (!mode || mode === "status") {
        focusSection("controls");
        pushConsoleEntry(
          "Autopilot",
          `state=${state.lastData?.status?.autopilot?.state || "-"}\nmax_concurrent=${fmt(
            state.lastData?.status?.autopilot?.max_concurrent_missions
          )}\nwake_interval=${fmt(state.lastData?.status?.autopilot?.wake_interval_seconds)}`,
          { tone: "info", label: "autopilot" }
        );
        return true;
      }
      if (["on", "enable"].includes(mode)) {
        await setAutopilot("enabled");
      } else if (mode === "pause") {
        await setAutopilot("paused");
      } else if (mode === "resume") {
        await setAutopilot("enabled");
      } else {
        throw new Error("usage: /autopilot [on|pause|resume|status]");
      }
      pushConsoleEntry("Autopilot", `Autopilot set to ${mode}.`, {
        tone: "good",
        label: "autopilot",
        preformatted: false,
      });
      return true;
    }
    case "missions": {
      focusSection("missions");
      const missions = await apiGet("/v1/missions?limit=25");
      pushConsoleEntry(
        "Missions",
        formatRecordLines(missions, (mission) =>
          `${mission.id} [${mission.status}] ${mission.title} wake_at=${fmt(mission.wake_at)} repeat=${fmt(
            mission.repeat_interval_seconds
          )} watch=${fmt(mission.watch_path)}`
        ),
        { tone: "info", label: "missions" }
      );
      return true;
    }
    case "events": {
      const limit = rawArgs ? parseMaybeInteger(rawArgs, "Event limit") : 10;
      focusSection("events");
      const events = await apiGet(`/v1/events?limit=${limit}`);
      pushConsoleEntry(
        "Events",
        formatRecordLines(events, (entry) => `[${fmtDate(entry.timestamp || entry.created_at)}] ${entry.level || "info"} ${entry.message || entry.text || ""}`),
        { tone: "info", label: "events" }
      );
      return true;
    }
    case "profile": {
      focusSection("profile");
      const profile = await apiGet("/v1/memory/profile");
      pushConsoleEntry(
        "Profile memory",
        formatRecordLines(profile, (memory) => `${memory.id} [${memory.kind}/${memory.scope}] ${memory.subject}\n  ${memory.content}`),
        { tone: "info", label: "profile" }
      );
      return true;
    }
    case "memory":
      return handleMemoryCommand(rawArgs);
    case "remember":
      if (!rawArgs) {
        throw new Error("usage: /remember <text>");
      }
      await apiPost("/v1/memory", {
        kind: "note",
        scope: "global",
        subject: rawArgs.slice(0, 80),
        content: rawArgs,
        confidence: 100,
        source_session_id: state.activeChatSessionId,
        workspace_key: currentChatCwd() || null,
        tags: ["manual"],
        review_status: "accepted",
      });
      await refreshDashboard();
      pushConsoleEntry("Memory", "Stored note in accepted memory.", {
        tone: "good",
        label: "memory",
        preformatted: false,
      });
      return true;
    case "forget":
      if (!rawArgs) {
        throw new Error("usage: /forget <memory-id>");
      }
      await apiDelete(`/v1/memory/${encodeURIComponent(rawArgs)}`);
      await refreshDashboard();
      pushConsoleEntry("Memory", `Forgot memory ${rawArgs}.`, {
        tone: "good",
        label: "memory",
        preformatted: false,
      });
      return true;
    case "skills":
      return handleSkillsCommand(rawArgs);
    case "permissions":
    case "approvals":
      if (!rawArgs) {
        focusSection("permissions");
        pushConsoleEntry("Permissions", `permission_preset=${state.lastData?.permissions || "-"}`, {
          tone: "info",
          label: "permissions",
        });
        return true;
      }
      elements.runTaskPermission.value = rawArgs.toLowerCase() === "default" ? "" : rawArgs.toLowerCase();
      pushConsoleEntry("Permissions", `Set chat permission preset to ${elements.runTaskPermission.value || "default"}.`, {
        tone: "good",
        label: "permissions",
        preformatted: false,
      });
      return true;
    case "attach":
      addChatAttachment(rawArgs);
      return true;
    case "attachments":
      pushConsoleEntry(
        "Attachments",
        state.chatAttachments.length ? state.chatAttachments.map((attachment) => attachment.path).join("\n") : "attachments=(none)",
        { tone: "info", label: "attachments" }
      );
      return true;
    case "detach":
    case "attachments-clear":
      clearChatAttachments();
      pushConsoleEntry("Attachments", "attachments cleared", {
        tone: "good",
        label: "attachments",
        preformatted: false,
      });
      return true;
    case "new":
    case "clear":
      startNewChat();
      pushConsoleEntry("Session", "Started a new chat session.", {
        tone: "good",
        label: "session",
        preformatted: false,
      });
      return true;
    case "diff": {
      const diff = await apiPost("/v1/workspace/diff", { cwd: currentChatCwd() || null });
      pushConsoleEntry("Git diff", diff.diff || "No diff output.", { tone: "info", label: "diff" });
      return true;
    }
    case "copy":
      await copyLatestAssistantOutput();
      return true;
    case "compact":
      await compactActiveChat();
      return true;
    case "init": {
      const result = await apiPost("/v1/workspace/init", { cwd: currentChatCwd() || null });
      pushConsoleEntry(
        "Init",
        result.created ? `Initialized ${result.path}` : `${result.path} already exists.`,
        { tone: "good", label: "workspace", preformatted: false }
      );
      return true;
    }
    case "onboard":
      if (!window.confirm("This will wipe saved config, sessions, logs, and credentials. Continue?")) {
        pushConsoleEntry("Setup", "Onboarding reset cancelled.", {
          tone: "info",
          label: "setup",
          preformatted: false,
        });
        return true;
      }
      {
        const result = await apiPost("/v1/onboarding/reset", { confirmed: true });
        state.chatAttachments = [];
        setChatCwd("");
        startNewChat();
        if (result.daemon_token) {
          elements.tokenInput.value = result.daemon_token;
        }
        await refreshDashboard();
        activateDashboardTab("overview", { scrollToTop: true });
        focusSection("setup");
        pushConsoleEntry(
          "Setup",
          `Reset complete. Cleared ${result.removed_credentials || 0} credential entr${(result.removed_credentials || 0) === 1 ? "y" : "ies"}.`,
          {
            tone: "warn",
            label: "setup",
            preformatted: false,
          }
        );
        if (result.credential_warnings && result.credential_warnings.length) {
          pushConsoleEntry("Setup warnings", result.credential_warnings.join("\n"), {
            tone: "warn",
            label: "setup",
          });
        }
      }
      return true;
    case "alias":
    case "model":
      if (!rawArgs) {
        focusSection("providers");
        pushConsoleEntry(
          "Models",
          formatRecordLines(state.lastData?.aliases || [], (alias) => `${alias.alias} -> ${alias.provider_id} / ${alias.model}`),
          { tone: "info", label: "models" }
        );
        return true;
      }
      {
        const aliasMatch = (state.lastData?.aliases || []).find(
          (alias) => alias.alias.toLowerCase() === rawArgs.toLowerCase()
        );
        if (aliasMatch) {
          elements.runTaskAlias.value = aliasMatch.alias;
          elements.runTaskModel.value = "";
          pushConsoleEntry("Models", `Switched chat alias to ${aliasMatch.alias}.`, {
            tone: "good",
            label: "models",
            preformatted: false,
          });
        } else {
          elements.runTaskModel.value = rawArgs;
          pushConsoleEntry("Models", `Set explicit model override to ${rawArgs}.`, {
            tone: "good",
            label: "models",
            preformatted: false,
          });
        }
      }
      return true;
    case "provider":
    case "providers":
      if (!rawArgs) {
        focusSection("providers");
        pushConsoleEntry(
          "Providers",
          formatRecordLines(
            state.lastData?.providers || [],
            (provider) => `${provider.display_name || provider.id} (${provider.id}) -> ${preferAliasForProvider(provider.id) || "-"}`
          ),
          { tone: "info", label: "providers" }
        );
        return true;
      }
      {
        const aliasName = resolveProviderAliasSelection(rawArgs);
        elements.runTaskAlias.value = aliasName;
        elements.runTaskModel.value = "";
        pushConsoleEntry("Providers", `Switched chat alias to ${aliasName}.`, {
          tone: "good",
          label: "providers",
          preformatted: false,
        });
      }
      return true;
    case "thinking":
      if (!rawArgs) {
        pushConsoleEntry("Thinking", `thinking=${elements.runTaskThinking.value || "default"}`, {
          tone: "info",
          label: "thinking",
        });
        return true;
      }
      {
        const nextValue = rawArgs.toLowerCase();
        elements.runTaskThinking.value = ["default", "none", "minimal", "low", "medium", "high", "xhigh"].includes(nextValue)
          ? (nextValue === "default" ? "" : nextValue)
          : nextValue;
        pushConsoleEntry("Thinking", `thinking=${elements.runTaskThinking.value || "default"}`, {
          tone: "good",
          label: "thinking",
          preformatted: false,
        });
      }
      return true;
    case "mode":
      if (!rawArgs) {
        pushConsoleEntry("Mode", `mode=${elements.runTaskMode.value || "default"}`, {
          tone: "info",
          label: "mode",
        });
        return true;
      }
      {
        const nextValue = rawArgs.toLowerCase();
        if (!["default", "build", "daily"].includes(nextValue)) {
          throw new Error("usage: /mode [build|daily|default]");
        }
        elements.runTaskMode.value = nextValue === "default" ? "" : nextValue;
        pushConsoleEntry("Mode", `mode=${elements.runTaskMode.value || "default"}`, {
          tone: "good",
          label: "mode",
          preformatted: false,
        });
      }
      return true;
    case "fast":
      elements.runTaskThinking.value = "minimal";
      pushConsoleEntry("Thinking", "thinking=minimal", {
        tone: "good",
        label: "thinking",
        preformatted: false,
      });
      return true;
    case "rename": {
      ensureChatRunIdle("renaming chats");
      if (!state.activeChatSessionId) {
        throw new Error("No active chat to rename.");
      }
      const title = rawArgs || window.prompt("New chat title:", "") || "";
      if (!title.trim()) {
        return true;
      }
      await apiPut(`/v1/sessions/${encodeURIComponent(state.activeChatSessionId)}/title`, { title });
      await refreshSessionsSummary();
      await loadChatSession(state.activeChatSessionId);
      pushConsoleEntry("Session", `Renamed chat to ${title}.`, {
        tone: "good",
        label: "session",
        preformatted: false,
      });
      return true;
    }
    case "review": {
      const diff = await apiPost("/v1/workspace/diff", { cwd: currentChatCwd() || null });
      await executeChatPrompt(buildReviewPrompt(diff.diff || "", rawArgs || ""), { taskMode: "build" });
      return true;
    }
    case "exit":
    case "quit":
      pushConsoleEntry("Exit", "Close the browser tab when you are done. The daemon keeps running.", {
        tone: "info",
        label: "session",
        preformatted: false,
      });
      return true;
    default:
      break;
  }

  if (command === "schedule" || command === "repeat" || command === "watch") {
    const [firstToken, ...restTokens] = rawArgs.split(/\s+/).filter(Boolean);
    if (!firstToken || !restTokens.length) {
      throw new Error(
        command === "watch"
          ? "usage: /watch <path> <title>"
          : `usage: /${command} <seconds> <title>`
      );
    }
    const title = restTokens.join(" ");
    const now = new Date().toISOString();
    const mission = {
      id: crypto.randomUUID(),
      title,
      details: "",
      status: command === "watch" ? "waiting" : "scheduled",
      created_at: now,
      updated_at: now,
      alias: elements.runTaskAlias.value.trim() || null,
      requested_model: elements.runTaskModel.value.trim() || null,
      session_id: state.activeChatSessionId,
      workspace_key: currentChatCwd() || null,
      watch_path: null,
      watch_recursive: false,
      watch_fingerprint: null,
      wake_trigger: command === "watch" ? "file_change" : "timer",
      wake_at: null,
      repeat_interval_seconds: null,
      last_error: null,
      retries: 0,
      max_retries: 3,
    };
    if (command === "watch") {
      mission.watch_path = firstToken;
      mission.watch_recursive = true;
    } else {
      const seconds = parseMaybeInteger(firstToken, command === "schedule" ? "Schedule delay" : "Repeat interval");
      mission.wake_at = new Date(Date.now() + seconds * 1000).toISOString();
      mission.repeat_interval_seconds = command === "repeat" ? seconds : null;
    }
    await apiPost("/v1/missions", mission);
    await refreshDashboard();
    pushConsoleEntry(
      command === "watch" ? "Watch mission" : "Mission scheduled",
      command === "watch"
        ? `${title}\nWatching ${firstToken}.`
        : `${title}\n${command === "repeat" ? `Repeats every ${mission.repeat_interval_seconds}s.` : `Wakes at ${mission.wake_at}.`}`,
      { tone: "good", label: "missions" }
    );
    return true;
  }

  if (command === "resume" || command === "fork") {
    const targetSessionId = rawArgs ? resolveSessionTarget(rawArgs) : state.activeChatSessionId;
    if (!targetSessionId) {
      focusSection("sessions");
      pushConsoleEntry("Sessions", `Select a session from the Sessions panel to ${command} it.`, {
        tone: "info",
        label: "session",
        preformatted: false,
      });
      return true;
    }
    if (command === "resume") {
      await loadChatSession(targetSessionId, { focusChat: true });
      pushConsoleEntry("Sessions", `Resumed session ${targetSessionId}.`, {
        tone: "good",
        label: "session",
        preformatted: false,
      });
    } else {
      await forkSessionById(targetSessionId);
    }
    return true;
  }

  throw new Error(`unknown slash command '${line}'. Use /help to list commands.`);
}

export { runSlashCommand };
