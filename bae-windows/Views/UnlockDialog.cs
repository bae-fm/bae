using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// A locked library (encrypted, key absent on this device): prompt for the
// 64-character hex key. unlock_library stores it in the credential store; a
// successful unlock re-opens the library with sync online. The dialog stays
// open on a bad key; cancelling leaves the library locked.
internal sealed class UnlockDialog
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Action<string> _setStatus;
    private readonly Action<string> _openLibrary;

    public UnlockDialog(Func<XamlRoot?> xamlRoot, Action<string> setStatus, Action<string> openLibrary)
    {
        _xamlRoot = xamlRoot;
        _setStatus = setStatus;
        _openLibrary = openLibrary;
    }

    public async System.Threading.Tasks.Task Show(string libraryId)
    {
        var keyBox = new TextBox { PlaceholderText = Loc.Chrome("library.unlock.key_placeholder"), Width = 360 };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("library.unlock.body"),
            TextWrapping = TextWrapping.Wrap,
        });
        content.Children.Add(keyBox);
        content.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("library.unlock.title"),
            Content = content,
            PrimaryButtonText = Loc.Chrome("library.unlock.confirm"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var key = keyBox.Text?.Trim() ?? string.Empty;
            var error = await System.Threading.Tasks.Task.Run(() => NativeBae.UnlockLibrary(libraryId, key));
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
        {
            _openLibrary(libraryId);
        }
        else
        {
            _setStatus(Loc.Chrome("library.locked"));
        }
    }
}
