import { useCallback, useEffect, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import { submitBugReport } from "@/lib/bugReport";
import { startWindowDrag } from "@/lib/windowDrag";
import styles from "./ReportBug.module.css";

// Public sitekey; the matching secret exists only in the report Worker.
const TURNSTILE_SITE_KEY = "0x4AAAAAAFAzF9GiS4tEQvN6";

interface TurnstileApi {
  render(container: HTMLElement, options: {
    sitekey: string;
    callback: (token: string) => void;
    "expired-callback": () => void;
    "error-callback": () => void;
    theme: "dark";
    size: "invisible";
    action: "bug_report";
  }): string;
  remove(widgetId: string): void;
  reset(widgetId: string): void;
}

declare global {
  interface Window { turnstile?: TurnstileApi; }
}

function Turnstile({ onToken, onError, resetCount }: { onToken: (token: string) => void; onError: () => void; resetCount: number }) {
  const [element, setElement] = useState<HTMLDivElement | null>(null);
  const widgetIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!element || !TURNSTILE_SITE_KEY) return;
    let widgetId: string | undefined;
    let live = true;
    const render = () => {
      if (!live || !window.turnstile || widgetId) return;
      widgetId = window.turnstile.render(element, {
        sitekey: TURNSTILE_SITE_KEY,
        callback: onToken,
        "expired-callback": () => onToken(""),
        "error-callback": onError,
        theme: "dark",
        size: "invisible",
        action: "bug_report",
      });
      widgetIdRef.current = widgetId;
    };
    const existing = document.querySelector<HTMLScriptElement>("script[data-rbxport-turnstile]");
    if (window.turnstile) render();
    else if (existing) existing.addEventListener("load", render, { once: true });
    else {
      const script = document.createElement("script");
      script.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
      script.async = true;
      script.defer = true;
      script.dataset.rbxportTurnstile = "";
      script.addEventListener("load", render, { once: true });
      script.addEventListener("error", onError, { once: true });
      document.head.append(script);
    }
    return () => {
      live = false;
      if (widgetId && window.turnstile) window.turnstile.remove(widgetId);
      widgetIdRef.current = null;
    };
  }, [element, onError, onToken]);
  useEffect(() => {
    if (resetCount > 0 && widgetIdRef.current && window.turnstile) window.turnstile.reset(widgetIdRef.current);
  }, [resetCount]);
  return <div className={styles.turnstile} ref={setElement} aria-label="Human verification" />;
}

export function ReportBug({ onClose, windowed = false }: { onClose: () => void; windowed?: boolean }) {
  const [email, setEmail] = useState("");
  const [description, setDescription] = useState("");
  const [include, setInclude] = useState(true);
  const [attachment, setAttachment] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [turnstileToken, setTurnstileToken] = useState("");
  const [receipt, setReceipt] = useState<{ key: string; attachmentAdded: boolean } | null>(null);
  const [resetCount, setResetCount] = useState(0);
  const receiveTurnstileToken = useCallback((token: string) => { setTurnstileToken(token); if (token) setError(""); }, []);
  const reportTurnstileError = useCallback(() => setError("Human verification could not load. Check your connection and try again."), []);
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
        setBusy(true); setError(""); setReceipt(null);
        void submitBugReport({ email, description, attachment: include ? attachment ?? "" : "", turnstileToken })
          .then(setReceipt).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
          .finally(() => { setBusy(false); setTurnstileToken(""); setResetCount(count => count + 1); });
      }}>
        <div className={styles.fields}>
          <label className={styles.email}>
            <span>Your email <span className={styles.optional}>(optional)</span></span>
            <input type="email" autoComplete="email" placeholder="you@example.com" maxLength={320} value={email} onChange={e => setEmail(e.target.value)} />
          </label>
          <label className={styles.description}>What happened?
            <textarea required maxLength={30000} rows={7} placeholder="What were you doing, what went wrong, and what did you expect?" value={description} onChange={e => setDescription(e.target.value)} />
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
                {opening ? "Opening…" : "Show log"}
              </button>
            </div>
            <p className={styles.hint}>The log may include library paths and track titles.</p>
          </div>
          {TURNSTILE_SITE_KEY ? <Turnstile onToken={receiveTurnstileToken} onError={reportTurnstileError} resetCount={resetCount} /> : <p className={styles.error}>Bug reporting is temporarily unavailable.</p>}
          {error ? <p className={styles.error} role="alert">{error}</p> : null}
          {receipt ? <p role="status">Report {receipt.key} submitted.{receipt.attachmentAdded ? "" : " The log attachment could not be added."}</p> : null}
        </div>
        <footer>
          <span className={styles.hint}>Reports are sent to TRIODE. I read every report, but please don’t expect a personal reply.</span>
          <button type="button" onClick={onClose}>Close</button>
          <button className={styles.save} type="submit" disabled={busy || !TURNSTILE_SITE_KEY || !turnstileToken || !description.trim() || (include && attachment === null)}>{busy ? "Sending…" : "Send report"}</button>
        </footer>
      </form>
    </section>
  );
  return windowed ? body : <div className={styles.backdrop}>{body}</div>;
}

export function ReportWindow() {
  return <ReportBug windowed onClose={() => { void getBackend().then(backend => backend.closeWindow()); }} />;
}
