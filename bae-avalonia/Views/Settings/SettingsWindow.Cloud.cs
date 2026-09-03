using System;
using System.Collections.Generic;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
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
        syncStatus[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");

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
        var oauthStatus = new ConnectStatus();
        Button CloudButton(string label, string provider)
        {
            var button = new Button { Content = label };
            button.Click += async (_, _) =>
            {
                if (!OAuthCreds.Available)
                {
                    oauthStatus.Failure(OAuthCreds.RegistrationError ?? Loc.Chrome("cloud.signin.not_configured"));
                    return;
                }
                oauthStatus.Progress(Loc.Chrome("cloud.signin.in_progress", "provider", label));
                var (current, error) = await _app.Sync.SignInCloudProvider(provider, SelectedStorage());
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    oauthStatus.Failure(error);
                }
                else
                {
                    oauthStatus.Clear();
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
        var s3Status = new ConnectStatus();
        connect.Click += async (_, _) =>
        {
            s3Status.Progress(Loc.Chrome("settings.s3.connecting"));
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
                s3Status.Failure(error);
            }
            else
            {
                s3Status.Clear();
                _app.SettingsStore.Reload();
            }
        };

        // The probe's outcome sits under the button that starts it, inside the
        // settings window's one long scroller. A user who scrolled down to fill
        // these fields reads the failure without scrolling back up.
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
                s3Status.Line,
            },
        };

        // A failure belongs to the provider this library is configured for: the
        // row offers to retry that provider's connection, and the recorded error
        // outlives a disconnect (nothing clears it once the loop is gone), so a
        // disconnected library would otherwise wear a stale failure over its
        // set-up button. macOS and iOS gate it the same way.
        var syncFailure = new SyncFailureView(async () =>
        {
            var (current, error) = await _app.Sync.ReconnectSync();
            if (current && error is not null)
            {
                syncStatus.Text = error;
            }
        });

        // The retry returns the refusal (or null); the row that started it renders
        // it. Nothing else here needs to know, so it does not touch syncStatus.
        var syncBlocked = new BlockedSyncOperationsView(async id =>
        {
            var (current, error) = await _app.Sync.RetryBlockedSyncOperation(id);
            return current ? error : null;
        });

        content.Children.Add(syncStatus);
        content.Children.Add(syncFailure);
        content.Children.Add(syncBlocked);
        content.Children.Add(ButtonRow(disconnect, syncNow));
        var storageColumn = new StackPanel { Spacing = 4 };
        var storageCaption = SecondaryLabel(Loc.Chrome("settings.storage.mode"));
        storageCaption.FontSize = 12.5;
        storageColumn.Children.Add(storageCaption);
        storageColumn.Children.Add(storagePicker);
        content.Children.Add(storageColumn);
        content.Children.Add(oauthButtons);
        content.Children.Add(oauthStatus.Line);
        content.Children.Add(s3Form);

        renderers.Add(fresh =>
        {
            syncStatus.Text = fresh.SyncStatusText(_app.SyncStatusStore.SyncReady);
            syncFailure.Render(
                fresh.SyncProvider is null ? null : _app.SyncStatusStore.ErrorText,
                _app.SyncStatusStore.ErrorDetail);
            syncBlocked.Render(
                fresh.SyncProvider is null
                    ? Array.Empty<uniffi.bae_bridge.BridgeBlockedSyncOperation>()
                    : _app.SyncStatusStore.Blocked);
        });
    }

    // Feedback for one connect action, rendered directly beneath the control that
    // starts it: progress in the secondary tone, a failure in the danger one. The
    // settings window is a single long scroller, so an outcome written anywhere
    // but next to its own button is read only by a user who thinks to scroll
    // looking for it.
    private sealed class ConnectStatus
    {
        internal TextBlock Line { get; } =
            new() { TextWrapping = TextWrapping.Wrap, IsVisible = false };

        internal void Progress(string text) => Show(text, "BaeTextSecondaryBrush");

        internal void Failure(string text) => Show(text, "BaeDangerBrush");

        internal void Clear() => Line.IsVisible = false;

        private void Show(string text, string brush)
        {
            Line.Text = text;
            Line[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brush);
            Line.IsVisible = true;
        }
    }
}
