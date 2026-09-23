import { useId } from "react";
import { usePreferencesContext } from "@/store/usePreferences";
import { Section } from "./controls";
import layout from "./PaneLayout.module.css";
import controls from "./Preferences.module.css";
import styles from "./UsbExportPane.module.css";

export function UsbExportPane() {
  const { preferences, update } = usePreferencesContext();
  const id = useId();
  return <Section title="USB Export">
    <p className={`${layout.help} ${styles.intro}`}>USB import and export options.</p>
    <label className={`${layout.summary} ${styles.option}`}>
      <span className={styles.details}>
        <strong id={`${id}-database-folders`}>Setup PIONEER folder on USB drives</strong>
        <span id={`${id}-database-folders-help`} className={styles.description}>Create the PIONEER folders automatically when a new USB drive is synced for the first time.</span>
      </span>
      <input type="checkbox" role="switch" className={controls.toggle}
        aria-labelledby={`${id}-database-folders`} aria-describedby={`${id}-database-folders-help`}
        checked={preferences.djSystem.createDatabaseFolders}
        onChange={event => update("djSystem", { createDatabaseFolders: event.target.checked })} />
    </label>
    <label className={`${layout.summary} ${styles.option}`}>
      <span className={styles.details}>
        <strong id={`${id}-settings`}>Import CDJ Mixer Settings from USB drives</strong>
        <span id={`${id}-settings-help`} className={styles.description}>Import CDJ settings you stored on the USB stick to rbxport when you sync.</span>
        <span className={styles.default}>Default: Off</span>
      </span>
      <input type="checkbox" role="switch" className={controls.toggle}
        aria-labelledby={`${id}-settings`} aria-describedby={`${id}-settings-help`}
        checked={preferences.usbExport.importSettings}
        onChange={event => update("usbExport", { importSettings: event.target.checked })} />
    </label>
    <label className={`${layout.summary} ${styles.option}`}>
      <span className={styles.details}>
        <strong id={`${id}-history`}>Import Play History</strong>
        <span id={`${id}-history-help`} className={styles.description}>Import history from USB sticks to rbxport when you sync.</span>
        <span className={styles.default}>Default: On</span>
      </span>
      <input type="checkbox" role="switch" className={controls.toggle}
        aria-labelledby={`${id}-history`} aria-describedby={`${id}-history-help`}
        checked={preferences.usbExport.importHistory}
        onChange={event => update("usbExport", { importHistory: event.target.checked })} />
    </label>
    <label className={`${layout.summary} ${styles.option}`}>
      <span className={styles.details}>
        <strong id={`${id}-cleanup`}>Delete music not in any playlist</strong>
        <span id={`${id}-cleanup-help`} className={styles.description}>Free space on your USB stick by removing songs that aren't in any playlist.</span>
        <span className={styles.default}>Default: Off</span>
      </span>
      <input type="checkbox" role="switch" className={controls.toggle}
        aria-labelledby={`${id}-cleanup`} aria-describedby={`${id}-cleanup-help`}
        checked={preferences.usbExport.deleteUnlistedMusic}
        onChange={event => update("usbExport", { deleteUnlistedMusic: event.target.checked })} />
    </label>
    <div className={`${layout.summary} ${styles.conversion}`}>
      <label className={styles.conversionToggle}>
        <span className={styles.details}>
          <strong id={`${id}-compatibility`}>Maximum CDJ compatibility</strong>
          <span id={`${id}-compatibility-help`} className={styles.description}>Save formats like FLAC and M4A as WAV or MP3 on your USB stick for wider CDJ compatibility. Originals stay untouched.</span>
        </span>
        <input type="checkbox" role="switch" className={controls.toggle}
          aria-labelledby={`${id}-compatibility`} aria-describedby={`${id}-compatibility-help`}
          checked={preferences.usbExport.maximumCompatibility}
          onChange={event => update("usbExport", { maximumCompatibility: event.target.checked })} />
      </label>
      <div className={styles.format}>
        <label htmlFor={`${id}-format`}>Convert to</label>
        <select id={`${id}-format`} disabled={!preferences.usbExport.maximumCompatibility}
          value={preferences.usbExport.conversionFormat}
          onChange={event => update("usbExport", { conversionFormat: event.target.value === "mp3" ? "mp3" : "wav" })}>
          <option value="wav">WAV — larger files</option>
          <option value="mp3">MP3 — 320 kbps</option>
        </select>
      </div>
      <p className={styles.description}>WAV: 16-bit / 44.1 kHz. MP3: smaller, lossy files.</p>
    </div>
  </Section>;
}
