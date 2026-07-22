using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The welcome chooser, shown before any library is open: on first run (no library
// on disk) and after closing one. Lists the libraries already on disk to reopen,
// and offers to create a new one or restore from a code or the cloud directly.
// Creating writes the new library's keys (Windows Credential Manager) and on-disk
// layout; restoring pulls an existing library from the cloud onto this device.
internal sealed class WelcomeView
{
    private readonly Panel _host;
    private readonly Action<string> _setStatus;
    private readonly Func<List<BridgeLibrary>> _loadLibraries;
    private readonly Func<Action<string>, string?> _createLibrary;
    private readonly Action<string> _openLibrary;
    private readonly Func<System.Threading.Tasks.Task> _showJoinLibrary;
    private readonly Func<System.Threading.Tasks.Task> _showRestoreFromCloud;

    // The welcome chooser controls (the on-disk library list plus create /
    // restore), shown when no library is open; removed once one is opened.
    private StackPanel? _welcome;

    public WelcomeView(
        Panel host,
        Action<string> setStatus,
        Func<List<BridgeLibrary>> loadLibraries,
        Func<Action<string>, string?> createLibrary,
        Action<string> openLibrary,
        Func<System.Threading.Tasks.Task> showJoinLibrary,
        Func<System.Threading.Tasks.Task> showRestoreFromCloud)
    {
        _host = host;
        _setStatus = setStatus;
        _loadLibraries = loadLibraries;
        _createLibrary = createLibrary;
        _openLibrary = openLibrary;
        _showJoinLibrary = showJoinLibrary;
        _showRestoreFromCloud = showRestoreFromCloud;
    }

    public void Show()
    {
        // Re-entrant safety: drop any welcome panel from a previous showing so we
        // don't stack two.
        Dismiss();

        var libraries = _loadLibraries();
        _setStatus(libraries.Count > 0
            ? Loc.Chrome("welcome.choose_library")
            : Loc.Chrome("welcome.no_library"));

        _welcome = new StackPanel { Spacing = 8, HorizontalAlignment = HorizontalAlignment.Center };

        if (libraries.Count > 0)
        {
            _welcome.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("welcome.your_libraries"),
                HorizontalAlignment = HorizontalAlignment.Center,
            });
            foreach (var library in libraries)
            {
                var id = library.Id;
                var openButton = new Button
                {
                    Content = library.Name,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                };
                openButton.Click += (_, _) => _openLibrary(id);
                _welcome.Children.Add(openButton);
            }
        }

        var createButton = new Button
        {
            Content = Loc.Chrome("welcome.create_library"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        createButton.Click += (_, _) =>
        {
            var libraryId = _createLibrary(message => _setStatus(message));
            if (libraryId is null)
            {
                return;
            }

            _openLibrary(libraryId);
        };

        var codeBox = new TextBox { PlaceholderText = Loc.Chrome("restore.code_placeholder"), Width = 320 };
        var restoreButton = new Button
        {
            Content = Loc.Chrome("restore.from_code"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        restoreButton.Click += async (_, _) => await RestoreFromCode(codeBox.Text ?? string.Empty);

        // Join a library that already exists on another device: this device shows
        // its join-request code, an existing owner approves it, and the invite
        // code it returns brings the library down here.
        var joinButton = new Button
        {
            Content = Loc.Chrome("welcome.join_library"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        joinButton.Click += async (_, _) => await _showJoinLibrary();

        // Restore by entering the cloud location and credentials directly, when
        // there's no restore code (the code can't carry secrets like S3 keys).
        var restoreCloudButton = new Button
        {
            Content = Loc.Chrome("restore.from_cloud"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        restoreCloudButton.Click += async (_, _) => await _showRestoreFromCloud();

        _welcome.Children.Add(createButton);
        _welcome.Children.Add(codeBox);
        _welcome.Children.Add(restoreButton);
        _welcome.Children.Add(joinButton);
        _welcome.Children.Add(restoreCloudButton);
        _host.Children.Add(_welcome);
    }

    public void Dismiss()
    {
        if (_welcome is not null)
        {
            _host.Children.Remove(_welcome);
            _welcome = null;
        }
    }

    private async System.Threading.Tasks.Task RestoreFromCode(string code)
    {
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
            _setStatus(Loc.Chrome("restore.invalid_code"));
            return;
        }

        // OAuth providers (Google Drive, Dropbox, OneDrive) need a sign-in first: the
        // core opens the browser and captures the 127.0.0.1 redirect, returning a
        // token JSON that the restore pull authenticates with. Credential providers
        // pass no token.
        string? oauthTokenJson = null;
        if (info.NeedsOauth)
        {
            // The provider list is the build's support boundary: an S3-only
            // build cannot restore an OAuth-provider code here.
            if (!NativeBae.IsCloudProviderAvailable(info.CloudProvider))
            {
                _setStatus(Loc.Chrome("cloud.unsupported_provider", "provider", BridgeDisplay.ProviderDisplayName(info.CloudProvider)));
                return;
            }
            if (!OAuthCreds.Available)
            {
                _setStatus(OAuthCreds.RegistrationError
                    ?? Loc.Chrome("cloud.signin.not_configured"));
                return;
            }
            _setStatus(Loc.Chrome("cloud.signin.in_progress", "provider", BridgeDisplay.ProviderDisplayName(info.CloudProvider)));
            try
            {
                oauthTokenJson = await System.Threading.Tasks.Task.Run(() => NativeBae.OAuthAuthorize(info.CloudProvider));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to authorize cloud provider for restore.", exception);
                _setStatus(Loc.Chrome("cloud.signin.failed"));
                return;
            }
        }

        _setStatus(Loc.Chrome("restore.in_progress_named", "name", info.LibraryName));
        string libraryId;
        try
        {
            libraryId = await System.Threading.Tasks.Task.Run(() => NativeBae.RestoreFromCode(code, oauthTokenJson));
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to restore library from code.", exception);
            _setStatus(Loc.Chrome("restore.failed"));
            return;
        }

        Dismiss();
        _openLibrary(libraryId);
    }
}
