using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Restore a library from its restore code. The code carries everything the
// restore needs — which library, which cloud home, that home's credentials, and
// the encryption key — so there is nothing to enter by hand. OAuth tokens are
// the one thing it cannot carry (they expire), so an OAuth-backed provider
// re-authenticates; that picker lands with the rest of the OAuth port, and until
// it does such a library reports that this build can't complete its restore.
// Presented in the window's modal host.
internal sealed class RestoreFromCloudDialog
{
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    public RestoreFromCloudDialog(Action dismissWelcome, Action<string> openLibrary)
    {
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public Control Build(Action close)
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("restore.from_cloud_title")));
        column.Children.Add(DialogUi.Field(Loc.Chrome("restore.code_label"), out var codeBox));
        codeBox.Watermark = Loc.Chrome("restore.code_placeholder");

        var preview = DialogUi.Body(string.Empty);
        preview.IsVisible = false;
        column.Children.Add(preview);

        var status = DialogUi.Danger();
        var restore = DialogUi.Primary(Loc.Chrome("restore.confirm"));
        restore.IsEnabled = false;
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        column.Children.Add(status);
        column.Children.Add(DialogUi.Actions(cancel, restore));

        BridgeRestoreCodeInfo? decoded = null;

        void ShowStatus(string text)
        {
            status.Text = text;
            status.IsVisible = true;
        }

        // Decode as the user types: the button turns on only for a code that
        // parsed, and the preview names the library it would restore, so the
        // pasted code is confirmed before it pulls anything down.
        void DecodeCode(string code)
        {
            decoded = null;
            preview.IsVisible = false;
            status.IsVisible = false;
            restore.IsEnabled = false;
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
                ShowStatus(Loc.Chrome("restore.invalid_code"));
                return;
            }

            decoded = info;
            preview.Text = $"{info.LibraryName} · {BridgeDisplay.ProviderDisplayName(info.CloudProvider)}";
            preview.IsVisible = true;
            // An OAuth home needs a browser sign-in this build has no picker for
            // yet, so its restore can't be completed here.
            restore.IsEnabled = !info.NeedsOauth;
            if (info.NeedsOauth)
            {
                ShowStatus(Loc.Chrome("restore.failed"));
            }
        }

        codeBox.TextChanged += (_, _) => DecodeCode(codeBox.Text?.Trim() ?? string.Empty);
        cancel.Click += (_, _) => close();
        restore.Click += async (_, _) =>
        {
            if (decoded is null)
            {
                return;
            }

            restore.IsEnabled = false;
            status.Text = Loc.Chrome("restore.in_progress");
            status.IsVisible = true;

            var code = codeBox.Text?.Trim() ?? string.Empty;
            string restoredLibraryId;
            try
            {
                restoredLibraryId = await Task.Run(() => NativeBae.RestoreFromCode(code, null));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to restore library from its code.", exception);
                status.Text = Loc.Chrome("restore.failed");
                restore.IsEnabled = true;
                return;
            }

            close();
            _dismissWelcome();
            _openLibrary(restoredLibraryId);
        };

        return new ScrollViewer { Content = column, MaxHeight = 560 };
    }
}
