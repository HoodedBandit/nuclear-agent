(function () {
  const pluginState = {
    bound: false,
    plugins: [],
    reports: [],
  };

  function app() {
    return window.dashboardApp || {};
  }

  function elements() {
    return {
      summary: document.getElementById("plugins-summary"),
      installForm: document.getElementById("plugin-install-form"),
      installPath: document.getElementById("plugin-install-path"),
      installEnabled: document.getElementById("plugin-install-enabled"),
      installTrusted: document.getElementById("plugin-install-trusted"),
      installPinned: document.getElementById("plugin-install-pinned"),
      installGrantShell: document.getElementById("plugin-install-grant-shell"),
      installGrantNetwork: document.getElementById("plugin-install-grant-network"),
      installGrantFullDisk: document.getElementById("plugin-install-grant-full-disk"),
      health: document.getElementById("plugins-health"),
      list: document.getElementById("plugins-list"),
    };
  }

  function escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }

  function renderEmpty(message) {
    if (typeof app().renderEmpty === "function") {
      return app().renderEmpty(message);
    }
    return `<p class="panel__meta">${escapeHtml(message)}</p>`;
  }

  function badge(label, tone = "info") {
    return `<span class="badge badge--${escapeHtml(tone)}">${escapeHtml(label)}</span>`;
  }

  function actionButton(label, action, pluginId, extraClass = "", extraAttrs = {}) {
    const className = ["button-small", extraClass].filter(Boolean).join(" ");
    const dataAttrs = Object.entries(extraAttrs)
      .map(
        ([key, value]) =>
          ` data-${escapeHtml(key)}="${escapeHtml(value)}"`
      )
      .join("");
    return `<button type="button" class="${className}" data-plugin-action="${escapeHtml(
      action
    )}" data-plugin-id="${escapeHtml(pluginId)}"${dataAttrs}>${escapeHtml(label)}</button>`;
  }

  function reportMap(reports) {
    return new Map((reports || []).map((report) => [report.id, report]));
  }

  function capabilitySummary(plugin) {
    return `${plugin.manifest.tools.length} tool(s), ${plugin.manifest.connectors.length} connector declaration(s), ${plugin.manifest.provider_adapters.length} provider adapter declaration(s)`;
  }

  function reviewCurrent(plugin) {
    return (
      !!plugin?.trusted &&
      !!plugin?.reviewed_integrity_sha256 &&
      plugin.reviewed_integrity_sha256 === plugin.integrity_sha256
    );
  }

  function permissions(value) {
    return {
      shell: !!value?.shell,
      network: !!value?.network,
      full_disk: !!value?.full_disk,
    };
  }

  function permissionSummary(value) {
    const current = permissions(value);
    const labels = [];
    if (current.shell) {
      labels.push("shell");
    }
    if (current.network) {
      labels.push("network");
    }
    if (current.full_disk) {
      labels.push("full_disk");
    }
    return labels.length ? labels.join(", ") : "none";
  }

  function reviewedAt(plugin) {
    if (!plugin?.reviewed_at) {
      return "not reviewed";
    }
    const date = new Date(plugin.reviewed_at);
    return Number.isNaN(date.getTime()) ? plugin.reviewed_at : date.toLocaleString();
  }

  function shortHash(value) {
    const text = String(value ?? "").trim();
    if (!text) {
      return "unrecorded";
    }
    return text.length > 16 ? `${text.slice(0, 16)}...` : text;
  }

  function renderReports(reports) {
    const { health } = elements();
    if (!health) {
      return;
    }
    health.innerHTML = reports.length
      ? reports
          .map(
            (report) => `
              <article class="stack-card">
                <div class="card-title-row">
                  <div>
                    <h3>${escapeHtml(report.name)}</h3>
                    <p class="card-subtitle">${escapeHtml(report.id)} | ${escapeHtml(report.version)}</p>
                  </div>
                  ${badge(report.ok ? "ready" : "attention", report.ok ? "good" : "warn")}
                </div>
                <p class="card-copy">${escapeHtml(report.detail)}</p>
                <ul class="micro-list">
                  <li>runtime ready: ${escapeHtml(report.runtime_ready ? "yes" : "no")}</li>
                  <li>declared permissions: ${escapeHtml(permissionSummary(report.declared_permissions))}</li>
                  <li>granted permissions: ${escapeHtml(permissionSummary(report.granted_permissions))}</li>
                </ul>
              </article>
            `
          )
          .join("")
      : renderEmpty("Plugin doctor results appear here after a refresh.");
  }

  function renderPlugins(plugins, reports) {
    pluginState.plugins = plugins || [];
    pluginState.reports = reports || [];
    const ui = elements();
    if (!ui.summary || !ui.list) {
      return;
    }

    const reportById = reportMap(pluginState.reports);
    ui.summary.textContent = pluginState.plugins.length
      ? `${pluginState.plugins.length} plugin(s) installed`
      : "No plugins installed";
    ui.list.innerHTML = pluginState.plugins.length
      ? pluginState.plugins
          .map((plugin) => {
            const report = reportById.get(plugin.id);
            const declaredPermissions = permissions(report?.declared_permissions);
            const grantedPermissions = permissions(
              plugin.granted_permissions || report?.granted_permissions
            );
            const reviewBadge = badge(
              reviewCurrent(plugin) ? "review current" : "review needed",
              reviewCurrent(plugin) ? "good" : "warn"
            );
            const enabledBadge = plugin.enabled
              ? badge(
                  report?.runtime_ready ? "enabled" : "enabled / blocked",
                  report?.runtime_ready ? "good" : "warn"
                )
              : badge("disabled");
            const trustedBadge = badge(
              plugin.trusted ? "trusted" : "untrusted",
              plugin.trusted ? "good" : "warn"
            );
            const permissionBadge = badge(
              `grants ${permissionSummary(grantedPermissions)}`,
              permissionSummary(grantedPermissions) === "none" ? "info" : "good"
            );
            const pinnedBadge = plugin.pinned ? badge("pinned", "info") : "";
            const doctorBadge = report
              ? badge(report.ok ? "doctor ready" : "doctor issue", report.ok ? "good" : "warn")
              : "";
            return `
              <article class="stack-card">
                <div class="card-title-row">
                  <div>
                    <h3>${escapeHtml(plugin.manifest.name)}</h3>
                    <p class="card-subtitle">${escapeHtml(plugin.id)} | ${escapeHtml(plugin.manifest.version)}</p>
                  </div>
                  ${enabledBadge}
                </div>
                <p class="card-copy">${escapeHtml(plugin.manifest.description)}</p>
                <div class="badge-row">
                  ${trustedBadge}
                  ${reviewBadge}
                  ${permissionBadge}
                  ${pinnedBadge}
                  ${doctorBadge}
                </div>
                <ul class="micro-list">
                  <li>${escapeHtml(capabilitySummary(plugin))}</li>
                  <li>source kind: ${escapeHtml(plugin.source_kind || "local_path")}</li>
                  <li>source: ${escapeHtml(plugin.source_reference || plugin.source_path)}</li>
                  <li>resolved source: ${escapeHtml(plugin.source_path)}</li>
                  <li>install: ${escapeHtml(plugin.install_dir)}</li>
                  <li>integrity: ${escapeHtml(shortHash(plugin.integrity_sha256))}</li>
                  <li>reviewed at: ${escapeHtml(reviewedAt(plugin))}</li>
                  <li>declared permissions: ${escapeHtml(permissionSummary(report?.declared_permissions))}</li>
                  <li>granted permissions: ${escapeHtml(permissionSummary(plugin.granted_permissions))}</li>
                  <li>detail: ${escapeHtml(report?.detail || "No doctor report yet.")}</li>
                </ul>
                <div class="inline-actions">
                  ${
                    plugin.enabled
                      ? actionButton("Disable", "disable", plugin.id, "button-small--ghost")
                      : actionButton("Enable", "enable", plugin.id)
                  }
                  ${
                    plugin.trusted
                      ? actionButton("Untrust", "untrust", plugin.id, "button-small--ghost")
                      : actionButton("Trust", "trust", plugin.id)
                  }
                  ${
                    plugin.pinned
                      ? actionButton("Unpin", "unpin", plugin.id, "button-small--ghost")
                      : actionButton("Pin", "pin", plugin.id, "button-muted")
                  }
                  ${actionButton("Update", "update", plugin.id, "button-muted")}
                  ${actionButton("Doctor", "doctor", plugin.id, "button-muted")}
                  ${actionButton("Remove", "remove", plugin.id, "button-small--ghost")}
                </div>
                <div class="inline-actions">
                  ${
                    declaredPermissions.shell
                      ? actionButton(
                          grantedPermissions.shell ? "Revoke shell" : "Grant shell",
                          grantedPermissions.shell ? "revoke" : "grant",
                          plugin.id,
                          grantedPermissions.shell ? "button-small--ghost" : "button-muted",
                          { "plugin-permission": "shell" }
                        )
                      : ""
                  }
                  ${
                    declaredPermissions.network
                      ? actionButton(
                          grantedPermissions.network ? "Revoke network" : "Grant network",
                          grantedPermissions.network ? "revoke" : "grant",
                          plugin.id,
                          grantedPermissions.network ? "button-small--ghost" : "button-muted",
                          { "plugin-permission": "network" }
                        )
                      : ""
                  }
                  ${
                    declaredPermissions.full_disk
                      ? actionButton(
                          grantedPermissions.full_disk ? "Revoke full-disk" : "Grant full-disk",
                          grantedPermissions.full_disk ? "revoke" : "grant",
                          plugin.id,
                          grantedPermissions.full_disk ? "button-small--ghost" : "button-muted",
                          { "plugin-permission": "full-disk" }
                        )
                      : ""
                  }
                </div>
              </article>
            `;
          })
          .join("")
      : renderEmpty("Install a local plugin package to project additional tools into the daemon.");

    renderReports(pluginState.reports);
  }

  async function refreshPlugins(message) {
    const dashboard = app();
    if (typeof dashboard.refreshDashboard === "function") {
      await dashboard.refreshDashboard({ silent: true, includeLoadedPanels: true, includeHealth: true });
    }
    if (message && typeof dashboard.setStatus === "function") {
      dashboard.setStatus(message, "ok");
    }
  }

  async function submitInstall(event) {
    event.preventDefault();
    const ui = elements();
    const dashboard = app();
    if (!ui.installPath) {
      return;
    }
    const sourcePath = ui.installPath.value.trim();
    if (!sourcePath) {
      throw new Error("Plugin package path is required.");
    }
    await dashboard.apiPost("/v1/plugins/install", {
      source: sourcePath,
      enabled: ui.installEnabled?.checked ?? true,
      trusted: ui.installTrusted?.checked ?? false,
      granted_permissions: {
        shell: ui.installGrantShell?.checked ?? false,
        network: ui.installGrantNetwork?.checked ?? false,
        full_disk: ui.installGrantFullDisk?.checked ?? false,
      },
      pinned: ui.installPinned?.checked ?? false,
    });
    ui.installForm.reset();
    if (ui.installEnabled) {
      ui.installEnabled.checked = true;
    }
    await refreshPlugins(`Plugin installed from ${sourcePath}.`);
  }

  async function updatePlugin(pluginId, payload, successMessage) {
    await app().apiPut(`/v1/plugins/${encodeURIComponent(pluginId)}`, payload);
    await refreshPlugins(successMessage);
  }

  async function refreshPluginPackage(pluginId) {
    await app().apiPost(`/v1/plugins/${encodeURIComponent(pluginId)}/update`, {});
    await refreshPlugins(`Updated plugin ${pluginId}.`);
  }

  function findPlugin(pluginId) {
    return pluginState.plugins.find((plugin) => plugin.id === pluginId) || null;
  }

  async function updatePluginPermission(pluginId, permission, grant) {
    const plugin = findPlugin(pluginId);
    if (!plugin) {
      throw new Error(`Unknown plugin ${pluginId}.`);
    }
    const grantedPermissions = permissions(plugin.granted_permissions);
    if (permission === "shell") {
      grantedPermissions.shell = grant;
    } else if (permission === "network") {
      grantedPermissions.network = grant;
    } else if (permission === "full-disk") {
      grantedPermissions.full_disk = grant;
    }
    await updatePlugin(
      pluginId,
      {
        trusted: grant ? true : undefined,
        granted_permissions: grantedPermissions,
      },
      `${grant ? "Granted" : "Revoked"} ${permission} for ${pluginId}.`
    );
  }

  async function removePlugin(pluginId) {
    if (!window.confirm(`Remove plugin '${pluginId}'?`)) {
      return;
    }
    await app().apiDelete(`/v1/plugins/${encodeURIComponent(pluginId)}`);
    await refreshPlugins(`Removed plugin ${pluginId}.`);
  }

  async function showDoctor(pluginId) {
    const report = await app().apiGet(`/v1/plugins/${encodeURIComponent(pluginId)}/doctor`);
    const currentReports = reportMap(pluginState.reports);
    currentReports.set(pluginId, report);
    renderPlugins(
      pluginState.plugins,
      Array.from(currentReports.values()).sort((left, right) => left.id.localeCompare(right.id))
    );
    if (typeof app().setStatus === "function") {
      app().setStatus(`Doctor report refreshed for ${pluginId}.`, report.ok ? "ok" : "warn");
    }
  }

  async function handleAction(event) {
    const button = event.target.closest("[data-plugin-action]");
    if (!button) {
      return;
    }
    const dashboard = app();
    if (!dashboard.hasDashboardAuth || !dashboard.hasDashboardAuth()) {
      return;
    }

    const pluginId = button.dataset.pluginId;
    if (!pluginId) {
      return;
    }

    try {
      switch (button.dataset.pluginAction) {
        case "enable":
          await updatePlugin(pluginId, { enabled: true }, `Enabled plugin ${pluginId}.`);
          break;
        case "disable":
          await updatePlugin(pluginId, { enabled: false }, `Disabled plugin ${pluginId}.`);
          break;
        case "trust":
          await updatePlugin(pluginId, { trusted: true }, `Trusted plugin ${pluginId}.`);
          break;
        case "untrust":
          await updatePlugin(pluginId, { trusted: false }, `Untrusted plugin ${pluginId}.`);
          break;
        case "pin":
          await updatePlugin(pluginId, { pinned: true }, `Pinned plugin ${pluginId}.`);
          break;
        case "unpin":
          await updatePlugin(pluginId, { pinned: false }, `Unpinned plugin ${pluginId}.`);
          break;
        case "grant":
          await updatePluginPermission(pluginId, button.dataset.pluginPermission, true);
          break;
        case "revoke":
          await updatePluginPermission(pluginId, button.dataset.pluginPermission, false);
          break;
        case "update":
          await refreshPluginPackage(pluginId);
          break;
        case "remove":
          await removePlugin(pluginId);
          break;
        case "doctor":
          await showDoctor(pluginId);
          break;
        default:
          break;
      }
    } catch (error) {
      if (typeof dashboard.setStatus === "function") {
        dashboard.setStatus(`Plugin action failed: ${error.message}`, "warn");
      }
    }
  }

  function bind() {
    if (pluginState.bound) {
      return;
    }
    const ui = elements();
    if (ui.installForm) {
      ui.installForm.addEventListener("submit", (event) => {
        submitInstall(event).catch((error) => {
          if (typeof app().setStatus === "function") {
            app().setStatus(`Plugin install failed: ${error.message}`, "warn");
          }
        });
      });
    }
    document.addEventListener("click", handleAction);
    pluginState.bound = true;
  }

  window.dashboardPlugins = {
    render(plugins, reports) {
      renderPlugins(plugins, reports);
    },
    reset() {
      pluginState.plugins = [];
      pluginState.reports = [];
      renderPlugins([], []);
    },
    bind,
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bind, { once: true });
  } else {
    bind();
  }
})();
