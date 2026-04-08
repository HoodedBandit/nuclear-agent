import { FormEvent, useState } from "react";

import { createDashboardSession } from "../../api/client";
import { Surface } from "../../components/Surface";
import styles from "./ConnectScreen.module.css";

interface ConnectScreenProps {
  onConnected: () => Promise<void> | void;
}

export function ConnectScreen({ onConnected }: ConnectScreenProps) {
  const [token, setToken] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await createDashboardSession({ token });
      setToken("");
      await onConnected();
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : "Unable to establish a dashboard session."
      );
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <main className={styles.shell} data-testid="modern-connect-screen">
      <section className={styles.hero}>
        <div className={styles.heroEyebrow}>Nuclear Operator Cockpit</div>
        <h1 className={styles.heroTitle}>Modern control room for sessions, tools, integrations, and system state.</h1>
        <p className={styles.heroBody}>
          Sign into the operator cockpit for sessions, providers, connectors, missions, memory, and
          system controls.
        </p>
      </section>

      <Surface className={styles.formSurface} emphasis="accent">
        <form className={styles.form} onSubmit={handleSubmit}>
          <label className={styles.label} htmlFor="modern-token-input">
            Dashboard token
          </label>
          <input
            id="modern-token-input"
            className={styles.input}
            name="token"
            type="password"
            autoComplete="current-password"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            placeholder="Paste the daemon token"
            required
          />
          <button
            type="submit"
            className={styles.button}
            disabled={submitting || token.trim().length === 0}
            data-testid="modern-connect-button"
          >
            {submitting ? "Connecting…" : "Enter cockpit"}
          </button>
          {error ? <p className={styles.error}>{error}</p> : null}
          <p className={styles.hint}>Use the daemon token from the terminal or launch flow.</p>
        </form>
      </Surface>
    </main>
  );
}
