using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Join a library that already lives on another device. This device generates its
// join-request code (its public key) and shows it as a QR + text + short
// fingerprint; an existing owner approves it (in their Settings → Devices) and
// reads back an invite code, which the user pastes or scans here. Decoding the
// invite runs the OAuth sign-in when the provider needs it, then JoinFromCode
// pulls the library down. Mirrors the restore-from-code flow's non-cancellable
// shape.
internal sealed class JoinLibraryDialog
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    public JoinLibraryDialog(
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        Action dismissWelcome,
        Action<string> openLibrary)
    {
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public async System.Threading.Tasks.Task Show()
    {
        var content = new StackPanel { Spacing = 12, MinWidth = 360 };
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.intro"),
            TextWrapping = TextWrapping.Wrap,
        });

        // This device's code section, filled once it's generated below.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.this_device_code"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var deviceCodeHost = new StackPanel { Spacing = 8 };
        deviceCodeHost.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        content.Children.Add(deviceCodeHost);
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.show_to_member"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });

        // Invite-code section.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.invite_label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.invite_hint"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        var inviteBox = new TextBox
        {
            PlaceholderText = Loc.Chrome("join.invite_placeholder"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var scanButton = new Button { Content = Loc.Chrome("action.scan") };
        var inviteRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        inviteRow.Children.Add(inviteBox);
        inviteRow.Children.Add(scanButton);
        content.Children.Add(inviteRow);

        var invitePreview = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(invitePreview);

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(status);

        var joinButton = new Button
        {
            Content = Loc.Chrome("join.confirm"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            IsEnabled = false,
        };
        content.Children.Add(joinButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("join.title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 560 },
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };

        // The decoded invite and the OAuth token (when the provider needed one):
        // both feed the Join click and gate the button.
        BridgeInviteCodeInfo? decoded = null;
        string? oauthTokenJson = null;

        void ShowStatus(string message)
        {
            status.Text = message;
            status.Visibility = Visibility.Visible;
        }

        void ClearStatus()
        {
            status.Text = string.Empty;
            status.Visibility = Visibility.Collapsed;
        }

        // The button is ready once a valid invite is decoded and — for an OAuth
        // provider — the sign-in has produced a token.
        void Revalidate()
        {
            joinButton.IsEnabled = decoded is not null && (!decoded.NeedsOauth || oauthTokenJson is not null);
        }

        // Decode the typed/scanned invite and preview the library it joins. The
        // decode is an in-memory parse, so it runs on the UI thread; OAuth
        // sign-in is deferred to the Join click so the user isn't sent to the
        // browser just for typing.
        void DecodeInvite(string code)
        {
            decoded = null;
            oauthTokenJson = null;
            invitePreview.Visibility = Visibility.Collapsed;
            ClearStatus();
            Revalidate();
            if (string.IsNullOrWhiteSpace(code))
            {
                return;
            }

            BridgeInviteCodeInfo info;
            try
            {
                info = NativeBae.DecodeInviteCode(code);
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to decode invite code.", exception);
                ShowStatus(Loc.Chrome("join.invalid_invite"));
                return;
            }

            decoded = info;
            invitePreview.Text = Loc.Chrome("join.invite_for", new Dictionary<string, object?>
            {
                ["name"] = info.LibraryName,
                ["provider"] = BridgeDisplay.ProviderDisplayName(info.CloudProvider),
                ["fingerprint"] = info.OwnerFingerprint,
            });
            invitePreview.Visibility = Visibility.Visible;
            Revalidate();
        }

        inviteBox.TextChanged += (_, _) => DecodeInvite(inviteBox.Text?.Trim() ?? string.Empty);
        scanButton.Click += async (_, _) =>
        {
            var scanned = await QrScanner.ScanFromFileAsync(_windowHandle());
            if (scanned is not null)
            {
                inviteBox.Text = scanned.Trim();
            }
        };

        joinButton.Click += async (_, _) =>
        {
            if (decoded is null)
            {
                return;
            }

            var info = decoded;
            ClearStatus();

            // OAuth providers (Google Drive, Dropbox, OneDrive) need a sign-in
            // first: the joining device authorizes its own cloud account, exactly
            // as restore-from-code does.
            if (info.NeedsOauth)
            {
                if (!NativeBae.IsCloudProviderAvailable(info.CloudProvider))
                {
                    ShowStatus(Loc.Chrome("cloud.unsupported_provider", "provider", BridgeDisplay.ProviderDisplayName(info.CloudProvider)));
                    return;
                }
                if (!OAuthCreds.Available)
                {
                    ShowStatus(OAuthCreds.RegistrationError ?? Loc.Chrome("cloud.signin.not_configured"));
                    return;
                }

                joinButton.IsEnabled = false;
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                ShowStatus(Loc.Chrome("cloud.signin.in_progress", "provider", BridgeDisplay.ProviderDisplayName(info.CloudProvider)));
                try
                {
                    oauthTokenJson = await System.Threading.Tasks.Task.Run(() => NativeBae.OAuthAuthorize(info.CloudProvider));
                }
                catch (BridgeException exception)
                {
                    BaeDiagnostics.Logger.Error("Failed to authorize cloud provider for join.", exception);
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    ShowStatus(Loc.Chrome("cloud.signin.failed"));
                    Revalidate();
                    return;
                }
            }

            joinButton.IsEnabled = false;
            status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            ShowStatus(Loc.Chrome("join.in_progress", "name", info.LibraryName));
            var code = inviteBox.Text?.Trim() ?? string.Empty;
            var token = oauthTokenJson;
            string libraryId;
            try
            {
                libraryId = await System.Threading.Tasks.Task.Run(() => NativeBae.JoinFromCode(code, token));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to join library from invite.", exception);
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                ShowStatus(Loc.Chrome("join.failed"));
                joinButton.IsEnabled = true;
                return;
            }

            dialog.Hide();
            _dismissWelcome();
            _openLibrary(libraryId);
        };

        // Generate this device's join-request code off the UI thread, then render
        // it (QR + text + Copy) and its fingerprint. A failure leaves the device
        // section showing only an error — the invite half is unaffected.
        _ = GenerateJoinCode(deviceCodeHost);

        await dialog.ShowAsync();
    }

    // Fill the join screen's device-code section: generate the join-request code
    // and its fingerprint, then render the code display. Runs the blocking generated
    // bridge off the UI thread.
    private async System.Threading.Tasks.Task GenerateJoinCode(StackPanel host)
    {
        BridgeJoinRequest request;
        try
        {
            request = await System.Threading.Tasks.Task.Run(() => NativeBae.GenerateJoinRequest());
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to generate join request.", exception);
            host.Children.Clear();
            host.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("join.generate_failed"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        host.Children.Clear();

        host.Children.Add(DialogPrimitives.BuildCodeDisplay(request.Code));

        // The same short form the approving device sees, so the user can confirm
        // they're pairing the right device.
        host.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.fingerprint", "fingerprint", request.Fingerprint),
            FontFamily = new FontFamily("Consolas"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
    }
}
