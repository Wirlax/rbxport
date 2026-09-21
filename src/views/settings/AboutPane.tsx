/**
 * About: what this is, which version, who made it, and under what terms.
 * Not a pane rekordbox has — its version is under the application menu.
 */
import { useEffect, useState, type SVGProps } from "react";
import { Github, Globe, Instagram, Twitch } from "lucide-react";

import { getBackend } from "@/ipc/client";
import { usePreferencesContext } from "@/store/usePreferences";
import layout from "./PaneLayout.module.css";
import styles from "./Preferences.module.css";
import { Button, Select, Toggle } from "./controls";

// Discord brand mark from Simple Icons (CC0).
function DiscordIcon({ size = 19, ...props }: SVGProps<SVGSVGElement> & { size?: number }) {
  return <svg {...props} width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z" />
  </svg>;
}

/** Where the links go. */
export const ABOUT_LINKS = [
  { label: "Instagram", Icon: Instagram, url: "https://instagram.com/triodeofficial" },
  { label: "Twitch", Icon: Twitch, url: "https://twitch.tv/triodeofficial" },
  { label: "Discord", Icon: DiscordIcon, url: "https://discord.gg/72jpY49tNU" },
  { label: "GitHub", Icon: Github, url: "https://github.com/chrisle" },
  { label: "Web", Icon: Globe, url: "https://triodeofficial.com" },
];

export function AboutPane() {
  const { preferences, update } = usePreferencesContext();
  const { checkUpdates, updateFrequency } = preferences.advanced;
  const [openingUpdates, setOpeningUpdates] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void getBackend()
      .then((backend) => backend.appVersion())
      .then((found) => {
        if (live) setVersion(found);
      })
      .catch(() => {
        // A build with no shell behind it has no version to give; the pane
        // shows a dash rather than an error nobody asked about.
      });
    return () => {
      live = false;
    };
  }, []);

  const open = (url: string) => {
    void getBackend().then((backend) => backend.openUrl(url)).catch(() => {});
  };

  const checkForUpdates = () => {
    setOpeningUpdates(true);
    setUpdateError(null);
    void getBackend()
      .then((backend) => backend.requestPreferencesReset("updates"))
      .catch(() => setUpdateError("Couldn’t open updates. Please try again."))
      .finally(() => setOpeningUpdates(false));
  };

  return (
    <>
      <section className={styles.section} aria-label="About">
        <div className={styles.aboutHeader}>
          <div>
            <h1 className={styles.aboutName}>rbxport</h1>
            <p className={styles.aboutVersion} data-testid="about-version">v{version ?? "—"}</p>
          </div>
          <Button disabled={openingUpdates} onClick={checkForUpdates}>
            {openingUpdates ? "Opening…" : "Check for updates"}
          </Button>
        </div>
        <div className={styles.aboutUpdates}>
          <div className={styles.updateControls}>
            <Toggle
              label="Automatic updates"
              checked={checkUpdates}
              onChange={(checkUpdates) => update("advanced", { checkUpdates })}
            />
            <Select
              label="Update frequency"
              disabled={!checkUpdates}
              value={updateFrequency}
              choices={[
                { value: "start", label: "Every launch" },
                { value: "daily", label: "Daily" },
                { value: "weekly", label: "Weekly" },
              ]}
              onChange={(updateFrequency) => update("advanced", { updateFrequency })}
            />
          </div>
          <p className={layout.help}>Updates are downloaded in the background and applied after you exit.</p>
          {updateError ? <p className={styles.updateError} role="alert">{updateError}</p> : null}
        </div>
      </section>
      <div className={styles.aboutLegal}>
        <details>
          <summary>Licence</summary>
          <p className={styles.aboutLine}>
            rbxport is free software, licensed under the GNU General Public License version 2 or, at your
            option, any later version. It comes with no warranty. It includes the Rubber Band Library
            by Particular Programs Ltd., under the same licence.
          </p>
        </details>
        <details>
          <summary>Disclaimer</summary>
          <p className={styles.aboutLine}>
            rbxport is an independent project and is not affiliated with, endorsed by, or sponsored by
            AlphaTheta Corporation, Pioneer DJ, or any other third party. rekordbox, CDJ, XDJ and
            PRO DJ LINK are trademarks of their respective owners. Back up your library before using
            any feature that writes to it.
          </p>
        </details>
      </div>
      <footer className={styles.aboutFoot}>
        <div className={styles.aboutLinks} role="group" aria-label="Author links">
          {ABOUT_LINKS.map(({ label, url, Icon }) => (
            <button key={label} type="button" className={styles.aboutSocial} aria-label={label} title={label} onClick={() => open(url)}>
              <Icon size={19} aria-hidden="true" />
            </button>
          ))}
        </div>
        <p className={styles.aboutMade}>Made by TRIODE with ❤️ in California</p>
      </footer>
    </>
  );
}
