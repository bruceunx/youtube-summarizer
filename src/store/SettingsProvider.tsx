import * as React from "react";
import type { SettingsType } from "types/settings";

import { invoke } from "@tauri-apps/api/core";

const defaultSettings: SettingsType = {
  apiKey: null,
  aiUrl: null,
  aiModelName: null,
  whisperApiKey: null,
  whisperUrl: null,
  whisperModelName: null,
  proxy: null,
  tubeApiUrl: null,
};

interface SettingsContextType {
  settings: SettingsType;
  updateSettings: (newSettings: Partial<SettingsType>) => void;
  resetSettings: () => void;
}

const SettingsContext = React.createContext<SettingsContextType>({
  settings: defaultSettings,
  updateSettings: () => {},
  resetSettings: () => {},
});

export const SettingsProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [settings, setSettings] = React.useState<SettingsType>(defaultSettings);

  React.useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      const storedSettings = (await invoke("load_settings")) as SettingsType;
      console.log(storedSettings);
      if (storedSettings) {
        setSettings({ ...defaultSettings, ...storedSettings });
      }
    } catch (error) {
      console.error("Failed to load settings:", error);
    }
  };

  // Update settings and save to Tauri store
  const updateSettings = async (newSettings: Partial<SettingsType>) => {
    const updatedSettings = { ...settings, ...newSettings };
    try {
      await invoke("save_settings", { settings: updatedSettings });
      setSettings(updatedSettings);
    } catch (error) {
      console.error("Failed to save settings:", error);
    }
  };

  // Reset to default settings
  const resetSettings = async () => {
    try {
      await invoke("save_settings", { settings: defaultSettings });
      setSettings(defaultSettings);
    } catch (error) {
      console.error("Failed to reset settings:", error);
    }
  };

  return (
    <SettingsContext.Provider
      value={{ settings, updateSettings, resetSettings }}
    >
      {children}
    </SettingsContext.Provider>
  );
};

// Custom hook to use settings
export const useSettings = () => {
  const context = React.useContext(SettingsContext);
  if (!context) {
    throw new Error("useSettings must be used within a SettingsProvider");
  }
  return context;
};
