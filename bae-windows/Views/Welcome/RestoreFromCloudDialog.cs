using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Restore a library from its restore code. The code carries everything the
// restore needs — which library, which cloud home, that home's credentials, and
// the encryption key — so there is nothing to enter by hand. OAuth tokens are
// the one thing it cannot carry (they expire), so an OAuth-backed provider
// re-authenticates; that picker lands with the rest of the OAuth port, and until
// it does such a library reports that this build can't complete its restore.
internal sealed class RestoreFromCloudDialog
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    public RestoreFromCloudDialog(Func<XamlRoot?> xamlRoot, Action dismissWelcome, Action<string> openLibrary)
    {
        _xamlRoot = xamlRoot;
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public async System.Threading.Tasks.Task Show()
    {
        var content = new StackPanel { Spacing = 8, MinWidth = 360 };

        var codeBox = new TextBox
        {
            Header = Loc.Chrome("restore.code_label"),
            PlaceholderText = Loc.Chrome("restore.code_placeholder"),
        };
        content.Children.Add(codeBox);

        var preview = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(preview);

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(status);

        var restoreButton = new Button
        {
            Content = Loc.Chrome("restore.confirm"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            IsEnabled = false,
        };
        content.Children.Add(restoreButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("restore.from_cloud_title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 520 },
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };

        BridgeRestoreCodeInfo? decoded = null;

        // Decode as the user types: the button turns on only for a code that
        // parsed, and the preview names the library it would restore, so the
        // pasted code is confirmed before it pulls anything down.
        void DecodeCode(string code)
        {
            decoded = null;
            preview.Visibility = Visibility.Collapsed;
            status.Visibility = Visibility.Collapsed;
            restoreButton.IsEnabled = false;
            if (string.IsNullOrWhiteSpace(code))
            {
                return;
            }

            BridgeRestoreCodeInfo info;
            try
            {
                info = NativeBae.DecodeRestoreCode(code);
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to decode restore code.", exception);
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                status.Text = Loc.Chrome("restore.invalid_code");
                status.Visibility = Visibility.Visible;
                return;
            }

            decoded = info;
            preview.Text =
                $"{info.LibraryName} · {BridgeDisplay.ProviderDisplayName(info.CloudProvider)}";
            preview.Visibility = Visibility.Visible;
            // An OAuth home needs a browser sign-in this build has no picker for
            // yet, so its restore can't be completed here.
            restoreButton.IsEnabled = !info.NeedsOauth;
            if (info.NeedsOauth)
            {
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                status.Text = Loc.Chrome("restore.failed");
                status.Visibility = Visibility.Visible;
            }
        }

        codeBox.TextChanged += (_, _) => DecodeCode(codeBox.Text?.Trim() ?? string.Empty);

        restoreButton.Click += async (_, _) =>
        {
            if (decoded is null)
            {
                return;
            }

            restoreButton.IsEnabled = false;
            status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            status.Text = Loc.Chrome("restore.in_progress");
            status.Visibility = Visibility.Visible;

            var code = codeBox.Text?.Trim() ?? string.Empty;
            string restoredLibraryId;
            try
            {
                restoredLibraryId = await System.Threading.Tasks.Task.Run(
                    () => NativeBae.RestoreFromCode(code, null));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to restore library from its code.", exception);
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                status.Text = Loc.Chrome("restore.failed");
                restoreButton.IsEnabled = true;
                return;
            }

            dialog.Hide();
            _dismissWelcome();
            _openLibrary(restoredLibraryId);
        };

        await dialog.ShowAsync();
    }
}
