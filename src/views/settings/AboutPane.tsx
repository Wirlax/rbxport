/**
 * About: what this is, which version, who made it, and under what terms.
 * Not a pane rekordbox has — its version is under the application menu.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import styles from "./Preferences.module.css";
import { Section } from "./controls";

/** Where the links go. */
export const ABOUT_LINKS: readonly { label: string; url: string }[] = [
  { label: "Instagram", url: "https://instagram.com/triodeofficial" },
  { label: "Web", url: "https://iamchrisle.com" },
  { label: "Twitch", url: "https://twitch.tv/triodeofficial" },
];

export function AboutPane() {
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

  return (
    <>
      <Section title="About">
        <div className={styles.aboutHead}>
          <span className={styles.aboutName}>rbxport</span>
          <span className={styles.aboutVersion}>
            Version <span data-testid="about-version">{version ?? "—"}</span>
          </span>
        </div>
        <p className={styles.aboutLine}>
          A DJ library manager: analyse, export to a USB stick, and play on a CDJ. By @TRIODEOfficial.
        </p>
        <div className={styles.aboutLinks} role="group" aria-label="Links">
          {ABOUT_LINKS.map((link) => (
            <button key={link.label} type="button" className={styles.aboutLink} onClick={() => open(link.url)}>
              {link.label}
            </button>
          ))}
        </div>
      </Section>
      <Section title="Licence">
        <p className={styles.aboutLine}>
          rbxport is free software, licensed under the GNU General Public License version 2 or, at your
          option, any later version. It comes with no warranty. It includes the Rubber Band Library
          by Particular Programs Ltd., under the same licence.
        </p>
      </Section>
      <Section title="Disclaimer">
        <p className={styles.aboutLine}>
          rbxport is an independent project and is not affiliated with, endorsed by, or sponsored by
          AlphaTheta Corporation, Pioneer DJ, or any other third party. rekordbox, CDJ, XDJ and
          PRO DJ LINK are trademarks of their respective owners. Back up your library before using
          any feature that writes to it.
        </p>
      </Section>
      <p className={styles.aboutFoot}>Made with ❤️ in California</p>
    </>
  );
}
