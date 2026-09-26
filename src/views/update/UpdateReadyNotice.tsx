import { CircleCheck } from "lucide-react";

import { useTranslation } from "@/i18n";
import styles from "./UpdateReadyNotice.module.css";

interface UpdateReadyNoticeProps {
  version: string;
  onWhatsNew: () => void;
  onRestart: () => void;
}

/** A quiet, persistent handoff from a finished background download. */
export function UpdateReadyNotice({ version, onWhatsNew, onRestart }: UpdateReadyNoticeProps) {
  const t = useTranslation();
  return (
    <aside className={styles.notice} role="status" aria-live="polite">
      <CircleCheck className={styles.icon} aria-hidden="true" />
      <p><strong>{t("rbxport v{version} is ready.", { version })}</strong> Restart to finish updating.</p>
      <button type="button" className={styles.link} onClick={onWhatsNew}>What’s new?</button>
      <button type="button" className={styles.restart} onClick={onRestart}>Restart now</button>
    </aside>
  );
}
