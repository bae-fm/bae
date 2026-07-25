using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Join a library that already lives on another device. This device generates its
// join-request code (its public key); an existing owner approves it and reads
// back an invite code, which the user pastes here. Decoding the invite runs the
// OAuth sign-in when the provider needs it, then JoinFromCode pulls the library
// down. Presented in the window's modal host.
//
// The QR image for this device's code, and scanning an invite from an image, are
// the QrCode/QrScanner rebuilds scheduled for the parity port; here the code is
// selectable text with a copy button and the invite is pasted.
internal sealed class JoinLibraryDialog
{
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    // The code this device's own join request minted, kept so the join can hand it
    // back — coven promotes that pending identity into the store's custody with it.
    private string? _joinRequestCode;

    public JoinLibraryDialog(Action dismissWelcome, Action<string> openLibrary)
    {
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public Control Build(Action close)
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("join.title")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("join.intro")));

        column.Children.Add(SectionLabel(Loc.Chrome("join.this_device_code")));
        var deviceCodeHost = new StackPanel { Spacing = 8 };
        deviceCodeHost.Children.Add(new Spinner { Width = 20, Height = 20 });
        column.Children.Add(deviceCodeHost);
        column.Children.Add(DialogUi.Body(Loc.Chrome("join.show_to_member")));

        column.Children.Add(SectionLabel(Loc.Chrome("join.invite_label")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("join.invite_hint")));
        var inviteBox = new TextBox
        {
            Watermark = Loc.Chrome("join.invite_placeholder"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        column.Children.Add(inviteBox);

        var invitePreview = DialogUi.Body(string.Empty);
        invitePreview.IsVisible = false;
        column.Children.Add(invitePreview);
        var status = DialogUi.Danger();
        column.Children.Add(status);

        var join = DialogUi.Primary(Loc.Chrome("join.confirm"));
        join.IsEnabled = false;
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        column.Children.Add(DialogUi.Actions(cancel, join));

        BridgeInviteCodeInfo? decoded = null;
        string? oauthTokenJson = null;

        void ShowStatus(string message)
        {
            status.Text = message;
            status.IsVisible = true;
        }

        void Revalidate() =>
            join.IsEnabled = decoded is not null && (!decoded.NeedsOauth || oauthTokenJson is not null);

        void DecodeInvite(string code)
        {
            decoded = null;
            oauthTokenJson = null;
            invitePreview.IsVisible = false;
            status.IsVisible = false;
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
            invitePreview.IsVisible = true;
            Revalidate();
        }

        inviteBox.TextChanged += (_, _) => DecodeInvite(inviteBox.Text?.Trim() ?? string.Empty);
        cancel.Click += (_, _) => close();

        join.Click += async (_, _) =>
        {
            if (decoded is null)
            {
                return;
            }

            var info = decoded;
            status.IsVisible = false;

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

                join.IsEnabled = false;
                ShowStatus(Loc.Chrome("cloud.signin.in_progress", "provider", BridgeDisplay.ProviderDisplayName(info.CloudProvider)));
                try
                {
                    oauthTokenJson = await Task.Run(() => NativeBae.OAuthAuthorize(info.CloudProvider));
                }
                catch (BridgeException exception)
                {
                    BaeDiagnostics.Logger.Error("Failed to authorize cloud provider for join.", exception);
                    ShowStatus(Loc.Chrome("cloud.signin.failed"));
                    Revalidate();
                    return;
                }
            }

            join.IsEnabled = false;
            ShowStatus(Loc.Chrome("join.in_progress", "name", info.LibraryName));
            var code = inviteBox.Text?.Trim() ?? string.Empty;
            var token = oauthTokenJson;
            var joinRequestCode = _joinRequestCode;
            if (joinRequestCode is null)
            {
                join.IsEnabled = true;
                return;
            }

            string libraryId;
            try
            {
                libraryId = await Task.Run(() => NativeBae.JoinFromCode(code, joinRequestCode, token));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to join library from invite.", exception);
                ShowStatus(Loc.Chrome("join.failed"));
                join.IsEnabled = true;
                return;
            }

            close();
            _dismissWelcome();
            _openLibrary(libraryId);
        };

        _ = GenerateJoinCode(deviceCodeHost);

        return new ScrollViewer { Content = column, MaxHeight = 560 };
    }

    // Generate this device's join-request code off the UI thread, then render it as
    // selectable text with a copy button, plus its fingerprint.
    private async Task GenerateJoinCode(StackPanel host)
    {
        BridgeJoinRequest request;
        try
        {
            request = await Task.Run(() => NativeBae.GenerateJoinRequest());
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to generate join request.", exception);
            host.Children.Clear();
            var error = DialogUi.Danger();
            error.Text = Loc.Chrome("join.generate_failed");
            error.IsVisible = true;
            host.Children.Add(error);
            return;
        }

        _joinRequestCode = request.Code;
        host.Children.Clear();

        // The QR is a visual transport of the same code shown below it, scanned by
        // the approving device's camera. It falls back to the copyable text when the
        // encode fails.
        if (QrCode.Image(request.Code) is { } qr)
        {
            host.Children.Add(new Image
            {
                Source = qr,
                Width = 180,
                Height = 180,
                Stretch = Stretch.Uniform,
                HorizontalAlignment = HorizontalAlignment.Center,
            });
        }

        var codeBox = new TextBox
        {
            Text = request.Code,
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("monospace"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var copy = new Button { Content = Loc.Chrome("action.copy") };
        copy.Click += (_, _) => ClipboardHelper.CopyToClipboard(copy, request.Code);
        host.Children.Add(codeBox);
        host.Children.Add(copy);

        var fingerprint = DialogUi.Body(Loc.Chrome("join.fingerprint", "fingerprint", request.Fingerprint));
        fingerprint.FontFamily = new FontFamily("monospace");
        host.Children.Add(fingerprint);
    }

    private static TextBlock SectionLabel(string text)
    {
        var t = new TextBlock { Text = text, FontWeight = FontWeight.SemiBold };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        return t;
    }
}
