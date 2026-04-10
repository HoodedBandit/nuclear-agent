import type { FormEvent } from "react";
import { useState } from "react";
import { createDashboardSession } from "../../api/client";

export function ConnectScreen(props: {
  onConnected: () => Promise<void>;
}) {
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await createDashboardSession(token.trim());
      setToken("");
      await props.onConnected();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Connection failed.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <main className="connect-screen">
      <div className="connect-screen__frame">
        <div className="connect-screen__copy">
          <p className="eyebrow">Resident operator console</p>
          <h1>Nuclear Agent cockpit</h1>
          <p>
            Connect with the daemon token to open the new tactical dashboard build on
            the parallel route. The current production dashboard remains available on
            the stable route while this one hardens.
          </p>
        </div>
        <form className="connect-screen__form" onSubmit={handleSubmit}>
          <label className="field">
            <span>Daemon token</span>
            <input
              id="token-input"
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="Paste daemon token"
              value={token}
              onChange={(event) => setToken(event.target.value)}
            />
          </label>
          <div className="button-row">
            <button
              id="connect-button"
              type="submit"
              disabled={submitting || !token.trim()}
            >
              {submitting ? "Connecting..." : "Connect"}
            </button>
          </div>
          <p className="helper-copy">
            The browser keeps a short-lived cookie-backed session. The token itself is not
            stored long-term in dashboard state.
          </p>
          {error ? <p className="error-copy">{error}</p> : null}
        </form>
      </div>
    </main>
  );
}
