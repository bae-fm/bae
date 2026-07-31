using System;
using System.Threading.Tasks;
using Avalonia.Controls;

namespace Bae.Desktop;

// A locked library (encrypted, key absent on this device): prompt for the
// 64-character hex key. unlock_library stores it in the credential store; a
// successful unlock re-opens the library with sync online. The dialog stays open
// on a bad key; cancelling leaves the library locked. Presented in the window's
// modal host.
internal sealed class UnlockDialog
{
    private readonly Action<string> _setStatus;
    private readonly Action<string> _openLibrary;

    public UnlockDialog(Action<string> setStatus, Action<string> openLibrary)
    {
        _setStatus = setStatus;
        _openLibrary = openLibrary;
    }

    public Control Build(string libraryId, Action close)
    {
        var keyBox = new TextBox { Watermark = Loc.Chrome("library.unlock.key_placeholder"), Width = 360 };
        var status = DialogUi.Danger();
        var confirm = DialogUi.Primary(Loc.Chrome("library.unlock.confirm"));
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };

        confirm.Click += async (_, _) =>
        {
            confirm.IsEnabled = false;
            var key = keyBox.Text?.Trim() ?? string.Empty;
            var error = await Task.Run(() => NativeBae.UnlockLibrary(libraryId, key));
            if (error is not null)
            {
                status.Text = error;
                status.IsVisible = true;
                confirm.IsEnabled = true;
                return;
            }

            close();
            _openLibrary(libraryId);
        };
        cancel.Click += (_, _) =>
        {
            close();
            _setStatus(Loc.Chrome("library.locked"));
        };

        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("library.unlock.title")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("library.unlock.body")));
        column.Children.Add(keyBox);
        column.Children.Add(status);
        column.Children.Add(DialogUi.Actions(cancel, confirm));
        return column;
    }
}
