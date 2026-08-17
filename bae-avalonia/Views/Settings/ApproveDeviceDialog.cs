using System;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Owner-side approve flow, presented in the settings window's modal host: a single
// dialog whose body swaps between steps — capture (scan or paste the new device's
// join-request code) → confirm (its fingerprint) → invited (the sealed
// invitation to enter on the new device). The owner-side driver stays alive
// until that exact device joins or the dialog withdraws the invitation.
internal sealed class ApproveDeviceDialog
{
    private readonly AppService _app;

    public ApproveDeviceDialog(AppService app)
    {
        _app = app;
    }

    public Control Build(Action close)
    {
        byte[]? invitation = null;
        var dismissed = false;
        var body = new StackPanel { Spacing = 12 };

        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("members.approve.title")));
        column.Children.Add(new ScrollViewer { Content = body, MaxHeight = 520 });
        var done = new Button { Content = Loc.Chrome("action.done") };
        done.Click += async (_, _) => await WithdrawAndClose();
        column.Children.Add(DialogUi.Actions(done));

        async Task WithdrawAndClose()
        {
            dismissed = true;
            if (invitation is { } created)
            {
                var (current, error) = await _app.Sync.CancelDeviceInvite(created);
                if (current && error is not null)
                {
                    BaeDiagnostics.Logger.Error($"Failed to cancel device invitation: {error}");
                }
            }
            close();
        }

        void ShowCapture()
        {
            body.Children.Clear();
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.approve.capture_hint")));

            var pasteBox = new TextBox
            {
                Watermark = Loc.Chrome("members.approve.paste_placeholder"),
                HorizontalAlignment = HorizontalAlignment.Stretch,
            };
            var decode = new Button { Content = Loc.Chrome("members.approve.decode") };
            var scan = new Button { Content = Loc.Chrome("action.scan") };
            var captureRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Children = { pasteBox, decode, scan } };
            body.Children.Add(captureRow);

            var error = DialogUi.Danger();
            body.Children.Add(error);

            void TryDecode(string code)
            {
                if (string.IsNullOrWhiteSpace(code))
                {
                    return;
                }

                BridgeJoinRequestInfo info;
                try
                {
                    info = _app.Sync.DecodeJoinRequest(code);
                }
                catch (BridgeException exception)
                {
                    BaeDiagnostics.Logger.Error("Failed to decode join request.", exception);
                    error.Text = Loc.Chrome("members.approve.invalid_request");
                    error.IsVisible = true;
                    return;
                }

                ShowConfirm(code, info);
            }

            decode.Click += (_, _) => TryDecode(pasteBox.Text?.Trim() ?? string.Empty);
            scan.Click += async (_, _) =>
            {
                var scanned = await QrScanner.ScanFromFileAsync(scan);
                if (scanned is not null)
                {
                    TryDecode(scanned.Trim());
                }
            };
        }

        void ShowConfirm(string joinRequestCode, BridgeJoinRequestInfo info)
        {
            body.Children.Clear();
            var heading = new TextBlock { Text = Loc.Chrome("members.approve.confirm_title"), FontWeight = FontWeight.SemiBold };
            heading[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
            body.Children.Add(heading);
            body.Children.Add(new TextBlock { Text = info.Fingerprint, FontFamily = new FontFamily("monospace") });
            if (!string.IsNullOrEmpty(info.Email))
            {
                body.Children.Add(DialogUi.Body(info.Email));
            }
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.approve.confirm_hint")));

            var error = DialogUi.Danger();

            var back = new Button { Content = Loc.Chrome("action.back") };
            back.Click += (_, _) => ShowCapture();
            var approve = DialogUi.Primary(Loc.Chrome("members.approve.confirm"));
            approve.Click += async (_, _) =>
            {
                approve.IsEnabled = false;
                back.IsEnabled = false;
                error.Text = Loc.Chrome("members.approve.in_progress");
                error[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
                error.IsVisible = true;

                var (current, result) = await _app.Sync.BeginDeviceInvite(joinRequestCode);
                if (!current)
                {
                    return;
                }
                if (result.Invitation is not { } created)
                {
                    if (result.Error is { } invitationError)
                    {
                        BaeDiagnostics.Logger.Error($"Failed to create device invitation: {invitationError}");
                    }
                    error.Text = Loc.Chrome("members.approve.failed");
                    error[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
                    error.IsVisible = true;
                    approve.IsEnabled = true;
                    back.IsEnabled = true;
                    return;
                }

                invitation = created;
                if (dismissed)
                {
                    await _app.Sync.CancelDeviceInvite(created);
                    return;
                }
                ShowInvited(created);
            };

            body.Children.Add(DialogUi.Actions(back, approve));
            body.Children.Add(error);
        }

        void ShowInvited(byte[] created)
        {
            var code = Convert.ToBase64String(created);
            body.Children.Clear();
            body.Children.Add(DialogUi.Body(Loc.Chrome("members.approve.invited_hint")));

            var codeBox = new TextBox
            {
                Text = code,
                IsReadOnly = true,
                TextWrapping = TextWrapping.Wrap,
                FontFamily = new FontFamily("monospace"),
                HorizontalAlignment = HorizontalAlignment.Stretch,
            };
            var copy = new Button { Content = Loc.Chrome("action.copy") };
            copy.Click += (_, _) => ClipboardHelper.CopyToClipboard(copy, code);
            body.Children.Add(codeBox);
            body.Children.Add(copy);

            var status = DialogUi.Danger();
            var retry = new Button { Content = Loc.Chrome("library.retry"), IsVisible = false };
            retry.Click += async (_, _) => await DriveJoin(created, status, retry);
            body.Children.Add(status);
            body.Children.Add(DialogUi.Actions(retry));
            _ = DriveJoin(created, status, retry);
        }

        async Task DriveJoin(byte[] created, TextBlock status, Button retry)
        {
            retry.IsVisible = false;
            status.Text = Loc.Chrome("members.approve.in_progress");
            status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            status.IsVisible = true;

            var (current, error) = await _app.Sync.DriveDeviceJoin(created);
            if (!current || dismissed)
            {
                return;
            }
            if (error is not null)
            {
                status.Text = Loc.Chrome("members.approve.failed");
                status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
                retry.IsVisible = true;
                return;
            }

            invitation = null;
            close();
        }

        ShowCapture();
        return column;
    }
}
