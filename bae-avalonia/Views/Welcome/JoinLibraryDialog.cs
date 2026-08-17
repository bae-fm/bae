using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>Join by scanning or pasting the one pairing code shown on an existing device.</summary>
internal sealed class JoinLibraryDialog
{
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    public JoinLibraryDialog(Action dismissWelcome, Action<string> openLibrary)
    {
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public Control Build(Action close)
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("join.title")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("join.pairing_intro")));

        var codeBox = new TextBox
        {
            Watermark = Loc.Chrome("join.pairing_placeholder"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            AcceptsReturn = true,
            MaxLines = 5,
        };
        var scan = new Button { Content = Loc.Chrome("action.scan"), Margin = new Thickness(8, 0, 0, 0) };
        var codeRow = new DockPanel { LastChildFill = true };
        DockPanel.SetDock(scan, Dock.Right);
        codeRow.Children.Add(scan);
        codeRow.Children.Add(codeBox);
        column.Children.Add(codeRow);

        var preview = DialogUi.Body(string.Empty);
        preview.IsVisible = false;
        column.Children.Add(preview);
        var fingerprint = DialogUi.Body(string.Empty);
        fingerprint.IsVisible = false;
        column.Children.Add(fingerprint);
        var progress = new ContentControl();
        progress.IsVisible = false;
        column.Children.Add(progress);
        var status = DialogUi.Danger();
        column.Children.Add(status);

        var join = DialogUi.Primary(Loc.Chrome("join.confirm"));
        join.IsEnabled = false;
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        column.Children.Add(DialogUi.Actions(cancel, join));

        BridgeDevicePairingOffer? decoded = null;
        JoinDevicePairingOperation? joinOperation = null;
        var joined = false;
        string? oauthTokenJson = null;
        var revision = 0;

        void CancelJoin()
        {
            if (joined || joinOperation is null)
            {
                return;
            }
            NativeBae.CancelJoinDevicePairing(joinOperation);
            joinOperation.Dispose();
            joinOperation = null;
        }

        column.DetachedFromVisualTree += (_, _) => CancelJoin();

        void ShowStatus(string message)
        {
            status.Text = message;
            status.IsVisible = true;
            progress.IsVisible = false;
        }

        void ShowProgress(string message)
        {
            progress.Content = DialogUi.Body(message);
            progress.IsVisible = true;
            status.IsVisible = false;
        }

        async Task DecodePairing(string code)
        {
            var ownRevision = ++revision;
            decoded = null;
            oauthTokenJson = null;
            preview.IsVisible = false;
            fingerprint.IsVisible = false;
            progress.IsVisible = false;
            status.IsVisible = false;
            join.IsEnabled = false;
            if (string.IsNullOrWhiteSpace(code))
            {
                return;
            }

            BridgeDevicePairingOffer offer;
            try
            {
                offer = NativeBae.DecodeDevicePairingOffer(code);
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to decode pairing code.", exception);
                ShowStatus(Loc.Chrome("join.invalid_pairing"));
                return;
            }

            decoded = offer;
            preview.Text = Loc.Chrome("join.pairing_for", new Dictionary<string, object?>
            {
                ["name"] = offer.LibraryName,
                ["provider"] = BridgeDisplay.ProviderDisplayName(offer.CloudProvider),
            });
            preview.IsVisible = true;

            if (!offer.NeedsOauth)
            {
                join.IsEnabled = true;
                return;
            }
            if (!NativeBae.IsCloudProviderAvailable(offer.CloudProvider))
            {
                ShowStatus(Loc.Chrome("cloud.unsupported_provider", "provider", BridgeDisplay.ProviderDisplayName(offer.CloudProvider)));
                return;
            }
            if (!OAuthCreds.Available)
            {
                ShowStatus(OAuthCreds.RegistrationError ?? Loc.Chrome("cloud.signin.not_configured"));
                return;
            }

            ShowProgress(Loc.Chrome("cloud.signin.in_progress", "provider", BridgeDisplay.ProviderDisplayName(offer.CloudProvider)));
            try
            {
                var token = await Task.Run(() => NativeBae.OAuthAuthorize(offer.CloudProvider));
                if (ownRevision != revision)
                {
                    return;
                }
                oauthTokenJson = token;
                progress.IsVisible = false;
                join.IsEnabled = true;
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to authorize pairing provider.", exception);
                ShowStatus(Loc.Chrome("cloud.signin.failed"));
            }
        }

        codeBox.TextChanged += (_, _) => _ = DecodePairing(codeBox.Text?.Trim() ?? string.Empty);
        scan.Click += async (_, _) =>
        {
            var scanned = await QrScanner.ScanFromFileAsync(scan);
            if (scanned is not null)
            {
                codeBox.Text = scanned.Trim();
            }
        };
        cancel.Click += (_, _) =>
        {
            CancelJoin();
            close();
        };

        join.Click += async (_, _) =>
        {
            var offer = decoded;
            if (offer is null)
            {
                return;
            }
            join.IsEnabled = false;
            ShowProgress(Loc.Chrome("join.starting_pairing"));
            try
            {
                var operation = await NativeBae.PrepareJoinDevicePairing(
                    codeBox.Text?.Trim() ?? string.Empty,
                    oauthTokenJson);
                joinOperation = operation;
                fingerprint.Text = Loc.Chrome("join.fingerprint", "fingerprint", operation.Fingerprint());
                fingerprint.IsVisible = true;
                ShowProgress(Loc.Chrome("join.waiting_approval"));
                var progressDelivery = new LatestUiValue<BridgeJoiningDeviceJoinProgress>(
                    value =>
                    {
                        progress.Content = DeviceJoinProgressView.Build(value);
                        status.IsVisible = false;
                        progress.IsVisible = true;
                    });
                var libraryId = await Task.Run(() =>
                    NativeBae.JoinDevicePairing(operation, progressDelivery.Offer));
                joined = true;
                joinOperation = null;
                operation.Dispose();
                close();
                _dismissWelcome();
                _openLibrary(libraryId);
            }
            catch (BridgeException exception)
            {
                joinOperation?.Dispose();
                joinOperation = null;
                BaeDiagnostics.Logger.Error("Failed to join through device pairing.", exception);
                ShowStatus(Loc.Chrome("join.failed"));
                join.IsEnabled = true;
            }
        };

        return new ScrollViewer { Content = column, MaxHeight = 560 };
    }
}
