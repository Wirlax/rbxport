import { useEffect, useState } from "react";
import { getBackend } from "@/ipc/client";
import { loadPreferences } from "@/lib/preferences";
import { startWindowDrag } from "@/lib/windowDrag";
import styles from "./ReportBug.module.css";

export function ReportBug({ onClose, windowed = false }: { onClose: () => void; windowed?: boolean }) {
  const [email, setEmail] = useState("");
  const [description, setDescription] = useState("");
  const [include, setInclude] = useState(() => loadPreferences().advanced.usageStats);
  const [attachment, setAttachment] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    if (!include) { setAttachment(null); return; }
    let live = true;
    void getBackend().then(backend => backend.reportAttachment()).then(text => {
      if (live) { setAttachment(text); setError(""); }
    }).catch((e: unknown) => { if (live) setError(e instanceof Error ? e.message : String(e)); });
    return () => { live = false; };
  }, [include]);
  const body = (
    <section className={styles.dialog} role="dialog" aria-label="Report bug" aria-modal={windowed ? undefined : true}>
      <header className={styles.title} onMouseDown={windowed ? startWindowDrag : undefined}>
        Report bug
        {windowed ? null : <button type="button" aria-label="Close report" onClick={onClose}>×</button>}
      </header>
      <form className={styles.form} onSubmit={event => {
        event.preventDefault();
        if (busy || (include && attachment === null)) return;
        setBusy(true); setError(""); setSaved(false);
        void getBackend().then(backend => backend.saveBugReport(email, description, include ? attachment ?? "" : ""))
          .then(result => setSaved(result)).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
          .finally(() => setBusy(false));
      }}>
        <div className={styles.fields}>
          <label className={styles.email}>
            <span>Your email <span className={styles.optional}>(optional)</span></span>
            <input type="email" autoComplete="email" placeholder="you@example.com" maxLength={320} value={email} onChange={e => setEmail(e.target.value)} />
          </label>
          <label className={styles.description}>What happened?
            <textarea required maxLength={100000} rows={7} placeholder="What were you doing, what went wrong, and what did you expect?" value={description} onChange={e => setDescription(e.target.value)} />
          </label>
          <div className={styles.attachments}>
            <div className={styles.attachmentControls}>
              <label className={styles.toggle}><input type="checkbox" checked={include} onChange={e => setInclude(e.target.checked)} />Attach log and system information</label>
              <button type="button" className={styles.previewToggle} disabled={!include || attachment === null || opening} onClick={() => {
                if (attachment === null) return;
                setOpening(true);
                setError("");
                void getBackend().then(backend => backend.openReportAttachment(attachment))
                  .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
                  .finally(() => setOpening(false));
              }}>
                {opening ? "Opening…" : "Open attachment"}
              </button>
            </div>
            <p className={styles.hint}>The log may include library paths and track titles.</p>
          </div>
          {error ? <p className={styles.error} role="alert">{error}</p> : null}
          {saved ? <p role="status">Report ZIP saved.</p> : null}
        </div>
        <footer>
          <span className={styles.hint}>Saved locally as a ZIP file.</span>
          <button type="button" onClick={onClose}>Close</button>
          <button className={styles.save} type="submit" disabled={busy || !description.trim() || (include && attachment === null)}>{busy ? "Saving…" : "Save report ZIP…"}</button>
        </footer>
      </form>
    </section>
  );
  return windowed ? body : <div className={styles.backdrop}>{body}</div>;
}

export function ReportWindow() {
  return <ReportBug windowed onClose={() => { void getBackend().then(backend => backend.closeWindow()); }} />;
}
