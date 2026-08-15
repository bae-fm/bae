using System;
using System.Threading.Tasks;
using Avalonia.Controls;

namespace Bae.Desktop;

// A locked library (encrypted, key absent on this device): prompt for the
// 64-character hex key. The retained library handle owns the unlock; the dialog
// stays open on a bad key and cancelling releases that handle.
internal sealed class UnlockDialog
{
    private readonly Action<string> _setStatus;
    private readonly Func<string, Task<string?>> _unlock;
    private readonly Action _onUnlocked;
    private readonly Func<Task> _cancelUnlock;

    public UnlockDialog(
        Action<string> setStatus,
        Func<string, Task<string?>> unlock,
        Action onUnlocked,
        Func<Task> cancelUnlock)
    {
        _setStatus = setStatus;
        _unlock = unlock;
        _onUnlocked = onUnlocked;
        _cancelUnlock = cancelUnlock;
    }

    public Control Build(Action close)
    {
        var keyBox = new TextBox { Watermark = Loc.Chrome("library.unlock.key_placeholder"), Width = 360 };
        var status = DialogUi.Danger();
        var confirm = DialogUi.Primary(Loc.Chrome("library.unlock.confirm"));
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };

        confirm.Click += async (_, _) =>
        {
            confirm.IsEnabled = false;
            var key = keyBox.Text?.Trim() ?? string.Empty;
            string? error;
            try
            {
                error = await _unlock(key);
            }
            catch (OperationCanceledException)
            {
                close();
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status.IsVisible = true;
                confirm.IsEnabled = true;
                return;
            }

            close();
            _onUnlocked();
        };
        cancel.Click += async (_, _) =>
        {
            await _cancelUnlock();
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
