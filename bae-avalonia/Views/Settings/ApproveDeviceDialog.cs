using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>Display one pairing code, review the joining identity, and admit it.</summary>
internal sealed class ApproveDeviceDialog
{
    private readonly AppService _app;

    public ApproveDeviceDialog(AppService app)
    {
        _app = app;
    }

    public Control Build(Action close)
    {
        BridgeDevicePairingSession? pairing = null;
        var completed = false;
        var approving = false;
        var body = new StackPanel { Spacing = 12 };
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("members.approve.title")));
        column.Children.Add(new ScrollViewer { Content = body, MaxHeight = 520 });
        var done = new Button { Content = Loc.Chrome("action.done") };
        done.Click += (_, _) =>
        {
            if (approving)
            {
                return;
            }
            CancelPairing();
            close();
        };
        column.Children.Add(DialogUi.Actions(done));

        void CancelPairing()
        {
            if (completed || approving || pairing is null)
            {
                return;
            }
            var (current, error) = _app.Sync.CancelDevicePairing(pairing);
            if (current && error is not null)
            {
                BaeDiagnostics.Logger.Error($"Failed to cancel device pairing: {error}");
            }
            pairing = null;
        }

        void ShowError(string message)
        {
            var error = DialogUi.Danger();
            error.Text = message;
            error.IsVisible = true;
            body.Children.Add(error);
        }

        void ShowWaiting(BridgeDevicePairingSession session)
        {
            body.Children.Clear();
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.pairing.scan_hint")));
            var code = session.Code();
            if (QrCode.Image(code) is { } qr)
            {
                body.Children.Add(new Image
                {
                    Source = qr,
                    Width = 220,
                    Height = 220,
                    Stretch = Stretch.Uniform,
                    HorizontalAlignment = HorizontalAlignment.Center,
                });
            }
            var codeBox = new TextBox
            {
                Text = code,
                IsReadOnly = true,
                TextWrapping = TextWrapping.Wrap,
                FontFamily = new FontFamily("monospace"),
            };
            var copy = new Button { Content = Loc.Chrome("action.copy") };
            copy.Click += (_, _) => ClipboardHelper.CopyToClipboard(copy, code);
            body.Children.Add(codeBox);
            body.Children.Add(copy);
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.pairing.waiting")));
        }

        void ShowConfirm(BridgeDevicePairingSession session, BridgePairingDevice device)
        {
            body.Children.Clear();
            var heading = new TextBlock
            {
                Text = Loc.Chrome("members.pairing.confirm_title"),
                FontWeight = FontWeight.SemiBold,
            };
            heading[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
            body.Children.Add(heading);
            body.Children.Add(new TextBlock { Text = device.Fingerprint, FontFamily = new FontFamily("monospace") });
            if (!string.IsNullOrEmpty(device.Email))
            {
                body.Children.Add(DialogUi.Body(device.Email));
            }
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.pairing.confirm_hint")));
            var add = DialogUi.Primary(Loc.Chrome("members.pairing.add"));
            add.Click += async (_, _) => await Approve(session, device, add);
            body.Children.Add(DialogUi.Actions(add));
        }

        async Task WaitForDevice(BridgeDevicePairingSession session)
        {
            var (current, result) = await _app.Sync.WaitForPairingDevice(session);
            if (!current || pairing != session)
            {
                return;
            }
            if (result.Device is { } device)
            {
                ShowConfirm(session, device);
                return;
            }
            ShowError(Loc.Chrome("members.pairing.failed"));
        }

        async Task Approve(
            BridgeDevicePairingSession session,
            BridgePairingDevice device,
            Button add)
        {
            approving = true;
            done.IsEnabled = false;
            add.IsEnabled = false;
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.pairing.adding")));
            var (current, error) = await _app.Sync.ApprovePairingDevice(session);
            if (!current)
            {
                return;
            }
            approving = false;
            done.IsEnabled = true;
            if (error is not null)
            {
                BaeDiagnostics.Logger.Error($"Failed to approve paired device: {error}");
                ShowConfirm(session, device);
                ShowError(Loc.Chrome("members.pairing.failed"));
                return;
            }
            completed = true;
            pairing = null;
            close();
        }

        async Task Start()
        {
            body.Children.Add(new Spinner { Width = 20, Height = 20 });
            var (current, result) = await _app.Sync.StartDevicePairing();
            if (!current)
            {
                return;
            }
            if (result.Session is not { } session)
            {
                body.Children.Clear();
                ShowError(Loc.Chrome("members.pairing.failed"));
                return;
            }
            pairing = session;
            ShowWaiting(session);
            await WaitForDevice(session);
        }

        _ = Start();
        column.DetachedFromVisualTree += (_, _) => CancelPairing();
        return column;
    }
}
