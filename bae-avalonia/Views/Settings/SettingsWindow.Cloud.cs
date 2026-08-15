using System;
using System.Collections.Generic;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;

namespace Bae.Desktop;

// The settings window's cloud sync section: the status line, a two-step disconnect
// and a manual sync, the home-storage mode picker, the OAuth provider sign-in
// buttons this build supports, and the S3-compatible connect form. Cloud errors
// surface on the section's own status line, not the shared config-error line.
internal sealed partial class SettingsWindow
{
    private void BuildCloud(StackPanel content, List<Action<Settings>> renderers)
    {
        var syncStatus = new TextBlock { TextWrapping = TextWrapping.Wrap };
        syncStatus[!TextBlock.ForegroundProperty] =
            new Avalonia.Markup.Xaml.MarkupExtensions.DynamicResourceExtension("BaeTextPrimaryBrush");

        // Opaque (encrypted) vs browsable (stored in the clear), applied to whichever
        // provider connects below. Defaults to the secure choice. Not access control
        // — the bucket's own credentials gate it either way.
        var storagePicker = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.opaque"), Tag = "opaque" });
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.browsable"), Tag = "browsable" });
        storagePicker.SelectedIndex = 0;
        string SelectedStorage() => (storagePicker.SelectedItem as ComboBoxItem)?.Tag as string ?? "opaque";

        // Two-step disconnect: the first click surfaces the data-loss warning (when
        // releases live only in the cloud) inline and arms; the second confirms.
        var disconnect = new Button { Content = Loc.Chrome("settings.sync.disconnect") };
        var disconnectArmed = false;
        disconnect.Click += async (_, _) =>
        {
            if (!disconnectArmed)
            {
                var (warningCurrent, countResult) = await _app.Sync.CloudOnlyReleaseCount();
                if (!warningCurrent)
                {
                    return;
                }
                var (count, countError) = countResult;
                var warning = countError;
                if (warning is null && count > 0)
                {
                    warning = Loc.Core("core.sync.cloud_only_releases", "count", count.Value);
                }
                if (warning is not null)
                {
                    syncStatus.Text = Loc.Chrome("settings.sync.disconnect_confirm", "warning", warning);
                    disconnectArmed = true;
                    return;
                }
            }

            disconnectArmed = false;
            var (disconnectCurrent, error) = await _app.Sync.DisconnectCloudProvider();
            if (!disconnectCurrent)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _app.SettingsStore.Reload();
            }
        };
        var syncNow = new Button { Content = Loc.Chrome("settings.sync.now") };
        syncNow.Click += (_, _) => _app.Sync.TriggerSync();

        // OAuth sign-in runs the browser flow in the core, so it blocks until the
        // user finishes — the service runs it off the UI thread.
        Button CloudButton(string label, string provider)
        {
            var button = new Button { Content = label };
            button.Click += async (_, _) =>
            {
                if (!OAuthCreds.Available)
                {
                    syncStatus.Text = OAuthCreds.RegistrationError ?? Loc.Chrome("cloud.signin.not_configured");
                    return;
                }
                syncStatus.Text = Loc.Chrome("cloud.signin.in_progress", "provider", label);
                var (current, error) = await _app.Sync.SignInCloudProvider(provider, SelectedStorage());
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    syncStatus.Text = error;
                }
                else
                {
                    _app.SettingsStore.Reload();
                }
            };
            return button;
        }

        // Only offer the OAuth providers this build's native library supports. An
        // S3-only build returns just S3, so no sign-in button renders.
        var available = _app.Sync.AvailableCloudProviders();
        var oauthButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var wire in new[] { "google_drive", "dropbox", "onedrive" })
        {
            if (available.Contains(wire))
            {
                oauthButtons.Children.Add(CloudButton(BridgeDisplay.ProviderDisplayName(wire), wire));
            }
        }

        // S3-compatible provider form. The core probes the bucket before saving.
        var bucket = new TextBox();
        var region = new TextBox();
        var endpoint = new TextBox();
        var keyPrefix = new TextBox();
        var accessKey = new TextBox();
        var secretKey = new TextBox { PasswordChar = '•' };
        var connect = new Button { Content = Loc.Chrome("settings.s3.connect") };
        connect.Click += async (_, _) =>
        {
            syncStatus.Text = Loc.Chrome("settings.s3.connecting");
            var (current, error) = await _app.Sync.SaveSyncConfig(
                bucket.Text ?? string.Empty,
                region.Text ?? string.Empty,
                endpoint.Text ?? string.Empty,
                keyPrefix.Text ?? string.Empty,
                accessKey.Text ?? string.Empty,
                secretKey.Text ?? string.Empty,
                SelectedStorage());
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _app.SettingsStore.Reload();
            }
        };
        var s3Form = new StackPanel
        {
            Spacing = 6,
            Children =
            {
                LabeledField(Loc.Chrome("s3.field.bucket"), bucket),
                LabeledField(Loc.Chrome("s3.field.region"), region),
                LabeledField(Loc.Chrome("s3.field.endpoint"), endpoint),
                LabeledField(Loc.Chrome("s3.field.key_prefix"), keyPrefix),
                LabeledField(Loc.Chrome("s3.field.access_key"), accessKey),
                LabeledField(Loc.Chrome("s3.field.secret_key"), secretKey),
                connect,
            },
        };

        content.Children.Add(syncStatus);
        content.Children.Add(ButtonRow(disconnect, syncNow));
        var storageColumn = new StackPanel { Spacing = 4 };
        var storageCaption = SecondaryLabel(Loc.Chrome("settings.storage.mode"));
        storageCaption.FontSize = 12.5;
        storageColumn.Children.Add(storageCaption);
        storageColumn.Children.Add(storagePicker);
        content.Children.Add(storageColumn);
        content.Children.Add(oauthButtons);
        content.Children.Add(s3Form);

        renderers.Add(fresh =>
            syncStatus.Text = fresh.SyncStatusText(_app.SyncStatusStore.SyncReady));
    }
}
