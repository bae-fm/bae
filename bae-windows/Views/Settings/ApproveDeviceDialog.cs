using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Owner-side approve flow: a single dialog whose body swaps between steps —
// capture (scan or paste the new device's join-request code) → confirm (its
// fingerprint) → invited (the invite code to enter on the new device). Approve
// wraps the library key to the device and signs a membership entry; the invite
// code it returns is the new device's way in. Mirrors macOS's ApproveDeviceSheet.
internal sealed class ApproveDeviceDialog
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly SessionStore _session;

    public ApproveDeviceDialog(Func<XamlRoot?> xamlRoot, Func<IntPtr> windowHandle, SessionStore session)
    {
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _session = session;
    }

    public async System.Threading.Tasks.Task Show()
    {
        var body = new StackPanel { Spacing = 12, MinWidth = 360 };
        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("members.approve.title"),
            Content = new ScrollViewer { Content = body, MaxHeight = 560 },
            CloseButtonText = Loc.Chrome("action.done"),
            XamlRoot = _xamlRoot(),
        };

        void ShowCapture()
        {
            body.Children.Clear();
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.capture_hint"),
                TextWrapping = TextWrapping.Wrap,
            });

            var pasteBox = new TextBox
            {
                PlaceholderText = Loc.Chrome("members.approve.paste_placeholder"),
                HorizontalAlignment = HorizontalAlignment.Stretch,
            };
            var decode = new Button { Content = Loc.Chrome("members.approve.decode") };
            var scan = new Button { Content = Loc.Chrome("action.scan") };
            var captureRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            captureRow.Children.Add(pasteBox);
            captureRow.Children.Add(decode);
            captureRow.Children.Add(scan);
            body.Children.Add(captureRow);

            var error = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
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
                    info = NativeBae.DecodeJoinRequest(code);
                }
                catch (BridgeException exception)
                {
                    BaeDiagnostics.Logger.Error("Failed to decode join request.", exception);
                    error.Text = Loc.Chrome("members.approve.invalid_request");
                    error.Visibility = Visibility.Visible;
                    return;
                }

                ShowConfirm(info);
            }

            decode.Click += (_, _) => TryDecode(pasteBox.Text?.Trim() ?? string.Empty);
            scan.Click += async (_, _) =>
            {
                var scanned = await QrScanner.ScanFromFileAsync(_windowHandle());
                if (scanned is not null)
                {
                    TryDecode(scanned.Trim());
                }
            };
        }

        void ShowConfirm(BridgeJoinRequestInfo info)
        {
            body.Children.Clear();
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.confirm_title"),
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            });
            body.Children.Add(new TextBlock
            {
                Text = info.Fingerprint,
                FontFamily = new FontFamily("Consolas"),
            });
            if (!string.IsNullOrEmpty(info.Email))
            {
                body.Children.Add(new TextBlock
                {
                    Text = info.Email,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                });
            }
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.confirm_hint"),
                TextWrapping = TextWrapping.Wrap,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });

            var error = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };

            var back = new Button { Content = Loc.Chrome("action.back") };
            back.Click += (_, _) => ShowCapture();
            var approve = new Button
            {
                Content = Loc.Chrome("members.approve.confirm"),
                Style = Application.Current.Resources["AccentButtonStyle"] as Style,
            };
            approve.Click += async (_, _) =>
            {
                approve.IsEnabled = false;
                back.IsEnabled = false;
                error.Visibility = Visibility.Collapsed;
                error.Text = Loc.Chrome("members.approve.in_progress");
                error.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                error.Visibility = Visibility.Visible;

                var pubkey = info.Pubkey;
                var (current, code) = await _session.RunForCurrentHandle(
                    handle => NativeBae.InviteMember(handle, pubkey));
                if (!current)
                {
                    return;
                }
                if (code is null)
                {
                    error.Text = Loc.Chrome("members.approve.failed");
                    error.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    error.Visibility = Visibility.Visible;
                    approve.IsEnabled = true;
                    back.IsEnabled = true;
                    return;
                }

                ShowInvited(code);
            };

            var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            buttons.Children.Add(back);
            buttons.Children.Add(approve);
            body.Children.Add(buttons);
            body.Children.Add(error);
        }

        void ShowInvited(string code)
        {
            body.Children.Clear();
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.invited_hint"),
                TextWrapping = TextWrapping.Wrap,
            });
            body.Children.Add(DialogPrimitives.BuildCodeDisplay(code));
        }

        ShowCapture();
        await dialog.ShowAsync();
    }
}
