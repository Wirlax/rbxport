import { usePreferencesContext } from "@/store/usePreferences";
import { Section, Toggle } from "./controls";
import layout from "./PaneLayout.module.css";

export function UsbExportPane() {
  const { preferences, update } = usePreferencesContext();
  return <Section title="USB Export">
    <p className={layout.help}>When you connect a USB device:</p>
    <Toggle label="Automatically import CDJ/Mixer settings from USB to rbxport"
      checked={preferences.usbExport.importSettings}
      onChange={(importSettings) => update("usbExport", { importSettings })} />
    <Toggle label="Automatically import history from USB to rbxport"
      checked={preferences.usbExport.importHistory}
      onChange={(importHistory) => update("usbExport", { importHistory })} />
  </Section>;
}
