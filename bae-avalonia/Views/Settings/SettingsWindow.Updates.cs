using System;
using Avalonia.Controls;
using Avalonia.Threading;

namespace Bae.Desktop;

// The settings window's updates section: the installed version, and — for a
// Velopack install — a manual check that downloads in the background and applies
// on restart. A dev run or a loose-zip copy is not an install; off one, only the
// version line shows, reading the build's git commit instead of a package version.
internal sealed partial class SettingsWindow
{
    // Returns the unsubscribe for the service's state notifications, which the
    // window's Closed runs; null when there was nothing to subscribe to.
    private Action? BuildUpdates(StackPanel content)
    {
        content.Children.Add(SectionLabel(Loc.Chrome("settings.updates.title")));
        var installed = _updates.InstalledVersion is { } version
            ? UpdateFlowDisplay.VersionDisplay(version)
            : AppMetadata.ConfiguredString("BaeGitCommit");
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.updates.version", "version", installed)));

        if (!_updates.IsAvailable)
        {
            return null;
        }

        var status = SecondaryLabel(string.Empty);
        var check = new Button { Content = Loc.Chrome("settings.updates.check") };
        var restart = new Button { Content = Loc.Chrome("settings.updates.restart") };
        check.Click += async (_, _) => await _updates.CheckAsync();
        restart.Click += async (_, _) =>
        {
            _window?.Close();
            await _applyUpdateAndRestart();
        };

        // The whole section is driven by the flow's display layer; nothing here
        // decides what a state means.
        void Render(UpdateFlowState state)
        {
            if (UpdateFlowDisplay.StatusFor(state) is { } mapped)
            {
                status.Text = mapped.Args is { } args
                    ? Loc.Chrome(mapped.Key, args)
                    : Loc.Chrome(mapped.Key);
                status.IsVisible = true;
            }
            else
            {
                status.Text = string.Empty;
                status.IsVisible = false;
            }
            check.IsEnabled = UpdateFlowDisplay.CheckEnabled(state);
            restart.IsVisible = UpdateFlowDisplay.RestartVisible(state);
        }

        // Transitions arrive on the worker thread running the check; marshal to
        // the UI thread.
        void OnStateChanged(UpdateFlowState state) => Dispatcher.UIThread.Post(() => Render(state));
        _updates.StateChanged += OnStateChanged;

        // The phase lives on the service, not on this window, so opening settings
        // renders a download that finished while it was closed.
        Render(_updates.State);
        content.Children.Add(status);
        content.Children.Add(ButtonRow(check, restart));
        return () => _updates.StateChanged -= OnStateChanged;
    }
}
