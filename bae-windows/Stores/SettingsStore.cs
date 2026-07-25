using System;

namespace Bae.Windows;

// Mirror of the app's settings (Config): the current settings snapshot and a
// re-read that notifies. The settings dialog reads Current and re-renders on
// Changed; a config invalidation (or an in-dialog connect/disconnect) calls
// Reload while the dialog is open. Mirrors macOS's ConfigStore.
internal sealed class SettingsStore
{
    private readonly SettingsService _settings;

    public Settings? Current { get; private set; }

    public event Action? Changed;

    public SettingsStore(SettingsService settings)
    {
        _settings = settings;
    }

    // Re-read the settings from the current handle and notify subscribers. A gone
    // handle leaves the prior snapshot and skips the notification.
    public void Reload()
    {
        var (current, fresh) = _settings.GetSettings();
        if (!current)
        {
            return;
        }

        Current = fresh;
        Changed?.Invoke();
    }
}
