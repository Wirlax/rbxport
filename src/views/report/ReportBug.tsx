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
  const [preview, setPreview] = useState(false);
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
        <button type="button" aria-label="Close report" onClick={onClose}>×</button>
      </header>
      <form className={styles.form} onSubmit={event => {
        event.preventDefault();
        if (busy || (include && attachment === null)) return;
        setBusy(true); setError(""); setSaved(false);
        void getBackend().then(backend => backend.saveBugReport(email, description, include ? attachment ?? "" : ""))
          .then(result => setSaved(result)).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
          .finally(() => setBusy(false));
      }}>
        <label>Your email<input type="email" autoComplete="email" maxLength={320} value={email} onChange={e => setEmail(e.target.value)} /></label>
        <label>What happened?<textarea required maxLength={100000} rows={7} value={description} onChange={e => setDescription(e.target.value)} /></label>
        <label className={styles.toggle}><input type="checkbox" checked={include} onChange={e => setInclude(e.target.checked)} />Attach application log and system information</label>
        <p>The log can contain library paths and track titles. Your report is saved as a local ZIP file.</p>
        <button type="button" disabled={!include || attachment === null} onClick={() => setPreview(on => !on)}>
          {preview ? "Hide attachment" : "Show exactly what will be attached"}
        </button>
        {preview && include ? <pre className={styles.preview} aria-label="Report attachment">{attachment ?? "Reading attachment…"}</pre> : null}
        {error ? <p role="alert">{error}</p> : null}
        {saved ? <p role="status">Report ZIP saved.</p> : null}
        <footer><button type="button" onClick={onClose}>Close</button><button type="submit" disabled={busy || !description.trim() || (include && attachment === null)}>{busy ? "Saving…" : "Save report ZIP…"}</button></footer>
      </form>
    </section>
  );
  return windowed ? body : <div className={styles.backdrop}>{body}</div>;
}

export function ReportWindow() {
  return <ReportBug windowed onClose={() => { void getBackend().then(backend => backend.closeWindow()); }} />;
}
