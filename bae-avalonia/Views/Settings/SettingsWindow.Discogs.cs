using System;
using System.Collections.Generic;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;

namespace Bae.Desktop;

// The settings window's Discogs section: a key state machine. The token input is
// the only local draft; the configured/valid state comes from the settings
// re-read. not_configured/rejected → editable input + Save; valid → "connected" +
// Remove; unvalidated → that label + Re-check + Remove. Save and Re-check validate
// over the network, so they run off the UI thread and show "Validating…" while in
// flight. The persisted status line is separate from the shared action-error line
// so a config re-render can't wipe a rejection note.
internal sealed partial class SettingsWindow
{
    private void BuildDiscogs(StackPanel content, List<Action<Settings>> renderers)
    {
        var label = new TextBlock { Text = Loc.Chrome("settings.discogs.label") };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");

        var status = SecondaryLabel(string.Empty);
        var tokenBox = new TextBox
        {
            Watermark = Loc.Chrome("settings.discogs.token_placeholder"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var save = new Button { Content = Loc.Chrome("settings.discogs.save") };
        var recheck = new Button { Content = Loc.Chrome("settings.discogs.recheck") };
        var remove = new Button { Content = Loc.Chrome("settings.discogs.remove") };

        save.Click += async (_, _) =>
        {
            var token = tokenBox.Text ?? string.Empty;
            if (string.IsNullOrEmpty(token) || _discogsBusy)
            {
                return;
            }
            _discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, outcome) = await _app.Discogs.SaveToken(token);
            _discogsBusy = false;
            if (!current)
            {
                return;
            }
            switch (outcome)
            {
                case "valid":
                case "unvalidated":
                    // Stored: a config re-read settles the controls and label.
                    status.Text = string.Empty;
                    break;
                case "rejected":
                    // Nothing stored, so no config invalidation fires — keep the
                    // draft and surface the rejection.
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.rejected"));
                    break;
                default:
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.save_failed"));
                    break;
            }
        };
        recheck.Click += async (_, _) =>
        {
            if (_discogsBusy)
            {
                return;
            }
            _discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, error) = await _app.Discogs.RevalidateToken();
            _discogsBusy = false;
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // On success a config re-read settles the controls and label.
        };
        remove.Click += async (_, _) =>
        {
            if (_discogsBusy)
            {
                return;
            }
            ClearSettingsError();
            // Removing clears the config flag, firing a config invalidation — the
            // re-read restores the editable input. Nothing is patched inline here.
            var (current, error) = await _app.Discogs.RemoveToken();
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
        };

        content.Children.Add(label);
        content.Children.Add(tokenBox);
        content.Children.Add(ButtonRow(save, recheck, remove));
        content.Children.Add(status);

        // Drive the controls from the persisted status: which buttons show, whether
        // the input is editable, and the status line. The draft text and the shared
        // action-error line are left alone — they belong to the user's in-progress
        // input, not the stored state.
        renderers.Add(fresh =>
        {
            if (_discogsBusy)
            {
                return;
            }
            tokenBox.IsVisible = !fresh.DiscogsConfigured;
            save.IsVisible = !fresh.DiscogsConfigured;
            remove.IsVisible = fresh.DiscogsConfigured;
            recheck.IsVisible = fresh.DiscogsNeedsRecheck;
            status.Text = fresh.DiscogsConfigured ? fresh.DiscogsStatusText : string.Empty;
        });
    }
}
