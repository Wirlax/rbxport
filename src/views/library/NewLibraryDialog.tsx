import { useEffect, useRef, useState } from "react";
import styles from "./NewLibraryDialog.module.css";

/**
 * The one question a machine with no rekordbox library gets: make one, or
 * quit. Nothing else in the window works without a library, so there is no
 * third way out; Escape does nothing.
 */
export function NewLibraryDialog({ masterDb, onCreate, onQuit }: {
  masterDb: string;
  onCreate: () => Promise<void>;
  onQuit: () => void;
}) {
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);

  const create = () => {
    setCreating(true);
    setError("");
    onCreate().catch((e: unknown) => {
      setCreating(false);
      setError(errorText(e));
    });
  };

  return (
    <dialog ref={dialog} className={styles.dialog} aria-labelledby="new-library-title"
      aria-describedby="new-library-text"
      onCancel={event => event.preventDefault()}
      onKeyDown={event => event.stopPropagation()}>
      <form onSubmit={event => { event.preventDefault(); if (!creating) create(); }}>
        <h2 id="new-library-title" className={styles.title}>No rekordbox Library</h2>
        <p id="new-library-text" className={styles.text}>
          rekordbox isn&apos;t installed and there is no rekordbox database.
          Would you like to create a new database?
        </p>
        <p className={styles.path} title={masterDb}>{masterDb}</p>
        {error ? <p className={styles.error} role="alert">{error}</p> : null}
        <div className={styles.buttons}>
          <button type="submit" disabled={creating} autoFocus>{creating ? "Creating…" : "Create"}</button>
          <button type="button" onClick={onQuit} disabled={creating}>Quit</button>
        </div>
      </form>
    </dialog>
  );
}

/** The backend's message when it sent one, which says what went wrong. */
function errorText(e: unknown): string {
  if (typeof e === "object" && e !== null && "message" in e && typeof e.message === "string") return e.message;
  if (typeof e === "string") return e;
  return "Could not create the database.";
}
