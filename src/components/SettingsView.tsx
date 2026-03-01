import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ScreenInfo } from "../types";

interface SettingsViewProps {
  onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
  const [screens, setScreens] = useState<ScreenInfo[]>([]);
  const [selectedScreen, setSelectedScreen] = useState<number | null>(null);
  const [hooksEnabled, setHooksEnabled] = useState(true);
  const [codexHooksEnabled, setCodexHooksEnabled] = useState(true);
  const [soundEnabled, setSoundEnabled] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    // Load screens and current settings in parallel
    invoke<ScreenInfo[]>("list_screens").then(setScreens).catch(console.error);
    invoke<{
      hooksEnabled: boolean;
      codexHooksEnabled: boolean;
      selectedScreen: number | null;
      soundEnabled: boolean;
    }>("get_settings")
      .then((settings) => {
        setHooksEnabled(settings.hooksEnabled);
        setCodexHooksEnabled(settings.codexHooksEnabled);
        setSelectedScreen(settings.selectedScreen);
        setSoundEnabled(settings.soundEnabled);
      })
      .catch(console.error);
  }, []);

  // Refresh screen list when monitors are connected/disconnected
  useEffect(() => {
    const unlisten = listen("screens-changed", () => {
      invoke<ScreenInfo[]>("list_screens").then(setScreens).catch(console.error);
      invoke<{
        hooksEnabled: boolean;
        selectedScreen: number | null;
        soundEnabled: boolean;
      }>("get_settings")
        .then((settings) => {
          setSelectedScreen(settings.selectedScreen);
        })
        .catch(console.error);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const handleScreenSelect = async (index: number | null) => {
    if (saving) return;
    setSelectedScreen(index);
    setSaving(true);
    try {
      await invoke("set_selected_screen", { index });
    } catch (err) {
      console.error("[notchai-ui] set_selected_screen failed", err);
    } finally {
      setSaving(false);
    }
  };

  const handleHooksToggle = async () => {
    if (saving) return;
    const next = !hooksEnabled;
    setSaving(true);
    try {
      await invoke("toggle_hooks_enabled", { enabled: next });
      setHooksEnabled(next);
    } catch (err) {
      console.error("[notchai-ui] toggle_hooks_enabled failed", err);
    } finally {
      setSaving(false);
    }
  };

  const handleCodexHooksToggle = async () => {
    if (saving) return;
    const next = !codexHooksEnabled;
    setSaving(true);
    try {
      await invoke("toggle_codex_hooks_enabled", { enabled: next });
      setCodexHooksEnabled(next);
    } catch (err) {
      console.error("[notchai-ui] toggle_codex_hooks_enabled failed", err);
    } finally {
      setSaving(false);
    }
  };

  const handleSoundToggle = async () => {
    if (saving) return;
    const next = !soundEnabled;
    setSoundEnabled(next);
    setSaving(true);
    try {
      await invoke("save_settings", {
        hooksEnabled,
        codexHooksEnabled,
        selectedScreen,
        soundEnabled: next,
      });
      // Play a test sound when enabling
      if (next) {
        invoke("play_sound", { name: "Tink" }).catch(() => {});
        invoke("play_haptic").catch(() => {});
      }
    } catch (err) {
      console.error("[notchai-ui] save sound setting failed", err);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="expanded-content">
      <div className="settings-header">
        <button className="settings-back-btn" onClick={onBack} title="Back">
          <span className="settings-back-arrow">&#8249;</span>
        </button>
        <span className="settings-title">Settings</span>
      </div>
      <div className="expanded-divider" />

      <div className="settings-body">
        <div className="settings-section">
          <span className="settings-section-label">Display</span>
          <div className="settings-screen-list">
            <button
              className={`settings-screen-item ${selectedScreen === null ? "settings-screen-item--selected" : ""}`}
              onClick={() => handleScreenSelect(null)}
            >
              <span className="settings-screen-name">Auto-detect</span>
              <span className="settings-screen-detail">Default</span>
              {selectedScreen === null && (
                <span className="settings-screen-check">&#10003;</span>
              )}
            </button>
            {screens.map((screen) => (
              <button
                key={screen.index}
                className={`settings-screen-item ${selectedScreen === screen.index ? "settings-screen-item--selected" : ""}`}
                onClick={() => handleScreenSelect(screen.index)}
              >
                <span className="settings-screen-name">
                  {screen.name}
                  {screen.hasNotch && (
                    <span className="settings-screen-badge">notch</span>
                  )}
                </span>
                <span className="settings-screen-detail">
                  {Math.round(screen.width)}x{Math.round(screen.height)}
                  {screen.isPrimary ? " - Primary" : ""}
                </span>
                {selectedScreen === screen.index && (
                  <span className="settings-screen-check">&#10003;</span>
                )}
              </button>
            ))}
          </div>
        </div>

        <div className="settings-section">
          <span className="settings-section-label">Hooks</span>
          <div className="settings-toggle-row" onClick={handleHooksToggle}>
            <span className="settings-toggle-label">
              Claude Hooks
            </span>
            <div
              className={`settings-toggle ${hooksEnabled ? "settings-toggle--on" : ""}`}
            >
              <div className="settings-toggle-knob" />
            </div>
          </div>
          <div className="settings-toggle-row" onClick={handleCodexHooksToggle}>
            <span className="settings-toggle-label">
              Codex Hooks
            </span>
            <div
              className={`settings-toggle ${codexHooksEnabled ? "settings-toggle--on" : ""}`}
            >
              <div className="settings-toggle-knob" />
            </div>
          </div>
        </div>

        <div className="settings-section">
          <span className="settings-section-label">Sounds</span>
          <div className="settings-toggle-row" onClick={handleSoundToggle}>
            <span className="settings-toggle-label">
              Enable notification sounds
            </span>
            <div
              className={`settings-toggle ${soundEnabled ? "settings-toggle--on" : ""}`}
            >
              <div className="settings-toggle-knob" />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
