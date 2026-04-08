(function () {
  const STORAGE_KEY = "dashboardWorkspacePath";
  const workspaceState = {
    bound: false,
    report: null,
    autoLoaded: false,
  };

  function app() {
    return window.dashboardApp || {};
  }

  function elements() {
    return {
      summary: document.getElementById("workspace-summary"),
      form: document.getElementById("workspace-inspect-form"),
      path: document.getElementById("workspace-path"),
      clear: document.getElementById("workspace-use-daemon-cwd"),
      cards: document.getElementById("workspace-summary-cards"),
      overview: document.getElementById("workspace-overview"),
      hotspots: document.getElementById("workspace-hotspots"),
      languages: document.getElementById("workspace-languages"),
      commits: document.getElementById("workspace-commits"),
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
    return `<span class="badge" data-tone="${escapeHtml(tone)}">${escapeHtml(label)}</span>`;
  }

  function loadSavedPath() {
    try {
      return window.localStorage.getItem(STORAGE_KEY) || "";
    } catch (_) {
      return "";
    }
  }

  function savePath(value) {
    try {
      if (value) {
        window.localStorage.setItem(STORAGE_KEY, value);
      } else {
        window.localStorage.removeItem(STORAGE_KEY);
      }
    } catch (_) {
      // ignore storage failures
    }
  }

  function render(report) {
    workspaceState.report = report || null;
    const ui = elements();
    if (!ui.summary) {
      return;
    }
    if (!report) {
      ui.summary.textContent = "No workspace scan yet";
      ui.cards.innerHTML = renderEmpty("Inspect a repo to get git state, manifests, language mix, and large-file hotspots.");
      ui.overview.innerHTML = renderEmpty("Repo signals appear here.");
      ui.hotspots.innerHTML = renderEmpty("Large source files and focus paths appear here.");
      ui.languages.innerHTML = renderEmpty("Language breakdown appears here.");
      ui.commits.innerHTML = renderEmpty("Recent commits appear here when the workspace is a git repo.");
      return;
    }

    ui.summary.textContent = report.git_root
      ? `${report.git_branch || "detached"} | ${report.dirty_files} modified | ${report.untracked_files} untracked`
      : "Workspace scan complete (no git metadata)";

    ui.cards.innerHTML = [
      ["Workspace root", report.workspace_root, "root"],
      ["Git branch", report.git_branch || "n/a", "branch"],
      ["Manifests", report.manifests.length, "files"],
      ["Languages", report.language_breakdown.length, "detected"],
      ["Dirty files", report.dirty_files, "modified"],
      ["Large files", report.large_source_files.length, "hotspots"],
    ]
      .map(
        ([label, value, hint]) => `
          <article class="stat-card">
            <p class="stat-card__label">${escapeHtml(label)}</p>
            <p class="stat-card__value">${escapeHtml(value)}</p>
            <p class="stat-card__hint">${escapeHtml(hint)}</p>
          </article>
        `
      )
      .join("");

    ui.overview.innerHTML = `
      <article class="stack-card">
        <div class="card-title-row">
          <div>
            <h3>Workspace root</h3>
            <p class="card-subtitle">${escapeHtml(report.workspace_root)}</p>
          </div>
          ${badge(report.git_root ? "git repo" : "plain folder", report.git_root ? "good" : "info")}
        </div>
        <ul class="micro-list">
          <li>requested path: ${escapeHtml(report.requested_path)}</li>
          <li>git root: ${escapeHtml(report.git_root || "none")}</li>
          <li>git commit: ${escapeHtml(report.git_commit || "n/a")}</li>
          <li>staged: ${escapeHtml(report.staged_files)} | modified: ${escapeHtml(report.dirty_files)} | untracked: ${escapeHtml(report.untracked_files)}</li>
        </ul>
      </article>
      <article class="stack-card">
        <h3>Key manifests</h3>
        ${
          report.manifests.length
            ? `<ul class="micro-list">${report.manifests
                .map((entry) => `<li>${escapeHtml(entry)}</li>`)
                .join("")}</ul>`
            : renderEmpty("No common project manifests found.")
        }
      </article>
      <article class="stack-card">
        <h3>Operator notes</h3>
        ${
          report.notes.length
            ? `<ul class="micro-list">${report.notes
                .map((entry) => `<li>${escapeHtml(entry)}</li>`)
                .join("")}</ul>`
            : renderEmpty("No notable repo warnings.")
        }
      </article>
    `;

    ui.hotspots.innerHTML = [
      `
        <article class="stack-card">
          <h3>Focus paths</h3>
          ${
            report.focus_paths.length
              ? `<ul class="micro-list">${report.focus_paths
                  .map(
                    (entry) =>
                      `<li>${escapeHtml(entry.path)}: ${escapeHtml(entry.source_files)} source file(s)</li>`
                  )
                  .join("")}</ul>`
              : renderEmpty("No source-heavy paths detected.")
          }
        </article>
      `,
      `
        <article class="stack-card">
          <h3>Large source files</h3>
          ${
            report.large_source_files.length
              ? `<ul class="micro-list">${report.large_source_files
                  .map(
                    (entry) =>
                      `<li>${escapeHtml(entry.path)}: ${escapeHtml(entry.lines)} line(s)</li>`
                  )
                  .join("")}</ul>`
              : renderEmpty("No large source files detected in the scan window.")
          }
        </article>
      `,
    ].join("");

    ui.languages.innerHTML = report.language_breakdown.length
      ? report.language_breakdown
          .map(
            (entry) => `
              <article class="stack-card">
                <div class="card-title-row">
                  <h3>${escapeHtml(entry.label)}</h3>
                  ${badge(`${entry.files} file(s)`, "info")}
                </div>
              </article>
            `
          )
          .join("")
      : renderEmpty("No language data available.");

    ui.commits.innerHTML = report.recent_commits.length
      ? `<article class="stack-card"><ul class="micro-list">${report.recent_commits
          .map((entry) => `<li>${escapeHtml(entry)}</li>`)
          .join("")}</ul></article>`
      : renderEmpty("No recent commit data available.");
  }

  async function inspectWorkspace(pathValue, successMessage) {
    const dashboard = app();
    const path = String(pathValue || "").trim();
    const report = await dashboard.apiPost("/v1/workspace/inspect", {
      path: path || null,
    });
    savePath(path);
    render(report);
    if (typeof dashboard.setStatus === "function" && successMessage) {
      dashboard.setStatus(successMessage, "ok");
    }
  }

  async function submitInspect(event) {
    event.preventDefault();
    await inspectWorkspace(elements().path?.value || "", "Workspace scan updated.");
  }

  function bind() {
    if (workspaceState.bound) {
      return;
    }
    const ui = elements();
    if (ui.path && !ui.path.value) {
      ui.path.value = loadSavedPath();
    }
    if (ui.form) {
      ui.form.addEventListener("submit", (event) => {
        submitInspect(event).catch((error) => {
          app().setStatus?.(`Workspace scan failed: ${error.message}`, "warn");
        });
      });
    }
    if (ui.clear) {
      ui.clear.addEventListener("click", () => {
        if (ui.path) {
          ui.path.value = "";
        }
        inspectWorkspace("", "Workspace scan reset to the daemon current directory.").catch(
          (error) => app().setStatus?.(`Workspace scan failed: ${error.message}`, "warn")
        );
      });
    }
    workspaceState.bound = true;
  }

  async function handleBootstrap() {
    if (workspaceState.autoLoaded || !app().hasDashboardAuth?.()) {
      return;
    }
    workspaceState.autoLoaded = true;
    await inspectWorkspace(elements().path?.value || "", "");
  }

  window.dashboardWorkspace = {
    bind,
    getReport() {
      return workspaceState.report;
    },
    render,
    reset() {
      workspaceState.report = null;
      workspaceState.autoLoaded = false;
      render(null);
    },
    handleBootstrap,
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bind, { once: true });
  } else {
    bind();
  }
})();
