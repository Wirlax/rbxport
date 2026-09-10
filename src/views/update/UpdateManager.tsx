/**
 * The Update Manager window.
 *
 * rekordbox's is a window of its own with a title bar and a row of buttons
 * at the foot; the strings here are its own, from `german.lang`: "Update
 * Manager", "The current version", "The latest version", "To get a new
 * version, click Download.", "Downloading", "The latest version has been
 * downloaded.", "Update", "An error occurred. Please try later."
 *
 * Between the versions and the buttons sits what changed: every changelog
 * section between the version running and the one on offer, so somebody
 * two releases behind reads both. The download's progress is a bar with
 * the bytes beside it, and the install that follows has no progress to
 * give, so its bar just moves.
 */
import { useEffect, useRef } from "react";

import { formatBytes, parseChangelog, spans, type ChangelogBlock } from "@/lib/changelog";
import type { UpdaterState } from "@/store/useUpdater";
import styles from "./UpdateManager.module.css";

export interface UpdateManagerProps {
  state: UpdaterState;
  onCheck: () => void;
  onInstall: () => void;
  onClose: () => void;
}

/** The section's blocks, drawn. */
function Changes({ markdown }: { markdown: string }) {
  return (
    <>
      {parseChangelog(markdown).map((block, i) => (
        <Block key={i} block={block} />
      ))}
    </>
  );
}

function Inline({ text }: { text: string }) {
  return (
    <>
      {spans(text).map((span, i) =>
        span.code ? <code key={i}>{span.text}</code> : <span key={i}>{span.text}</span>,
      )}
    </>
  );
}

function Block({ block }: { block: ChangelogBlock }) {
  switch (block.kind) {
    case "release":
      return (
        <h3 className={styles.release}>
          Version {block.version}
          {block.date ? <span className={styles.date}>{block.date}</span> : null}
        </h3>
      );
    case "heading":
      return <h4 className={styles.heading}>{block.text}</h4>;
    case "paragraph":
      return (
        <p className={styles.paragraph}>
          <Inline text={block.text} />
        </p>
      );
    case "list":
      return (
        <ul className={styles.list}>
          {block.items.map((item, i) => (
            <li key={i}>
              <Inline text={item} />
            </li>
          ))}
        </ul>
      );
  }
}

function Progress({ downloaded, total }: { downloaded: number; total: number | null }) {
  const fraction = total !== null && total > 0 ? Math.min(downloaded / total, 1) : null;
  return (
    <div className={styles.progressRow}>
      <div
        className={styles.progress}
        role="progressbar"
        aria-label="Downloading"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction === null ? undefined : Math.round(fraction * 100)}
        data-indeterminate={fraction === null || undefined}
      >
        <span className={styles.progressFill} style={fraction === null ? undefined : { width: `${fraction * 100}%` }} />
      </div>
      <span className={styles.progressText}>
        {total !== null
          ? `${formatBytes(downloaded)} of ${formatBytes(total)}`
          : formatBytes(downloaded)}
      </span>
    </div>
  );
}

export function UpdateManager({ state, onCheck, onInstall, onClose }: UpdateManagerProps) {
  const panel = useRef<HTMLDivElement>(null);

  const check = state.phase === "available" || state.phase === "downloading" ||
    state.phase === "installing" || (state.phase === "failed" && state.check)
    ? state.check
    : null;
  const busy = state.phase === "downloading" || state.phase === "installing";

  // Escape closes as the close button does, so it is held back while the
  // close button is: a download or install runs on with the window open.
  useEffect(() => {
    panel.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  return (
    <div className={styles.backdrop} onMouseDown={busy ? undefined : onClose} role="presentation">
      <div
        ref={panel}
        className={styles.window}
        onMouseDown={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Update Manager"
        tabIndex={-1}
      >
        <header className={styles.titlebar}>
          {!busy ? (
            <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
              ✕
            </button>
          ) : null}
          Update Manager
        </header>

        <div className={styles.body}>
          {state.phase === "checking" ? (
            <p className={styles.status}>Checking for updates…</p>
          ) : state.phase === "upToDate" ? (
            <p className={styles.status}>
              rekordbox-lite {state.currentVersion} is the latest version.
            </p>
          ) : check ? (
            <>
              <dl className={styles.versions}>
                <dt>The current version</dt>
                <dd>{check.currentVersion}</dd>
                <dt>The latest version</dt>
                <dd>{check.version}</dd>
              </dl>
              {state.phase === "available" ? (
                <p className={styles.status}>To get a new version, click Download.</p>
              ) : state.phase === "downloading" ? (
                <>
                  <p className={styles.status}>Downloading…</p>
                  <Progress downloaded={state.progress?.downloaded ?? 0} total={state.progress?.total ?? null} />
                </>
              ) : state.phase === "installing" ? (
                <>
                  <p className={styles.status}>The latest version has been downloaded. Installing…</p>
                  <Progress downloaded={0} total={null} />
                </>
              ) : (
                <p className={`${styles.status} ${styles.failed}`} role="alert">
                  An error occurred. Please try later.
                  {state.phase === "failed" && state.message ? (
                    <span className={styles.detail}>{state.message}</span>
                  ) : null}
                </p>
              )}
              <section className={styles.changes} aria-label="What's new">
                {check.changes.length > 0 ? (
                  check.changes.map((change) => <Changes key={change.version} markdown={change.body} />)
                ) : (
                  <p className={styles.paragraph}>No release notes for this version.</p>
                )}
              </section>
            </>
          ) : (
            <p className={`${styles.status} ${styles.failed}`} role="alert">
              An error occurred. Please try later.
              {state.phase === "failed" && state.message ? (
                <span className={styles.detail}>{state.message}</span>
              ) : null}
            </p>
          )}
        </div>

        <footer className={styles.buttons}>
          {state.phase === "available" ? (
            <>
              <button type="button" className={styles.button} onClick={onClose}>Later</button>
              <button type="button" className={`${styles.button} ${styles.primary}`} onClick={onInstall}>
                Download
              </button>
            </>
          ) : state.phase === "failed" ? (
            <>
              <button type="button" className={styles.button} onClick={onClose}>Close</button>
              <button
                type="button"
                className={`${styles.button} ${styles.primary}`}
                onClick={state.check ? onInstall : onCheck}
              >
                {state.check ? "Try Again" : "Check Again"}
              </button>
            </>
          ) : busy ? null : (
            <button type="button" className={`${styles.button} ${styles.primary}`} onClick={onClose}>
              OK
            </button>
          )}
        </footer>
      </div>
    </div>
  );
}
