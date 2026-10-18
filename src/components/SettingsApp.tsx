import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { GeneralSettings } from "./GeneralSettings";
import { NvimEditSettings } from "./NvimEditSettings";

export interface ShortcutModifiers {
  shift: boolean;
  control: boolean;
  option: boolean;
  command: boolean;
}

export interface NvimEditSettings {
  enabled: boolean;
  shortcut_key: string;
  shortcut_modifiers: ShortcutModifiers;
  terminal: string;
  terminal_path: string;
  editor: string;
  nvim_path: string;
  popup_mode: boolean;
  popup_width: number;
  popup_height: number;
  clipboard_mode: boolean;
  domain_filetypes: Record<string, string>;
}

export interface Settings {
  show_in_menu_bar: boolean;
  show_in_dock: boolean;
  nvim_edit: NvimEditSettings;
}

type TabId = "general" | "nvim-config" | "nvim-window";

export function SettingsApp() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [activeTab, setActiveTab] = useState<TabId>("general");

  useEffect(() => {
    // Load settings and domain filetypes separately
    // (domain_filetypes are stored in a separate file)
    Promise.all([
      invoke<Settings>("get_settings"),
      invoke<Record<string, string>>("get_domain_filetypes"),
    ])
      .then(([loadedSettings, domainFiletypes]) => {
        // Merge domain_filetypes into nvim_edit settings
        setSettings({
          ...loadedSettings,
          nvim_edit: {
            ...loadedSettings.nvim_edit,
            domain_filetypes: domainFiletypes,
          },
        });
      })
      .catch((e) => console.error("Failed to load settings:", e));
  }, []);

  const updateSettings = async (updates: Partial<Settings>) => {
    if (!settings) return;

    const newSettings = { ...settings, ...updates };
    setSettings(newSettings);

    try {
      await invoke("set_settings", { newSettings });
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  };

  if (!settings) {
    return <div className="loading">Loading settings...</div>;
  }

  const editPopupTabs: { id: TabId; label: string; icon: string }[] = [
    { id: "nvim-config", label: "Config", icon: "gear" },
    { id: "nvim-window", label: "Window", icon: "window" },
  ];

  return (
    <div className="settings-container">
      <div className="tabs">
        <button
          className={`tab ${activeTab === "general" ? "active" : ""}`}
          onClick={() => setActiveTab("general")}
        >
          <span className="tab-icon">{getIcon("gear")}</span>
          General
        </button>

        <div className="tab-group">
          <span className="tab-group-label">Edit Popup</span>
          <div className="tab-group-tabs">
            {editPopupTabs.map((tab) => (
              <button
                key={tab.id}
                className={`tab ${activeTab === tab.id ? "active" : ""}`}
                onClick={() => setActiveTab(tab.id)}
              >
                <span className="tab-icon">{getIcon(tab.icon)}</span>
                {tab.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="tab-content">
        {activeTab === "general" && (
          <GeneralSettings settings={settings} onUpdate={updateSettings} />
        )}
        {(activeTab === "nvim-config" || activeTab === "nvim-window") && (
          <NvimEditSettings settings={settings} onUpdate={updateSettings} activeTab={activeTab} />
        )}
      </div>

    </div>
  );
}

function getIcon(name: string): string {
  const icons: Record<string, string> = {
    gear: "\u2699",
    window: "\u25A1",
  };
  return icons[name] || "";
}
