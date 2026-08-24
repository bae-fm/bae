using System.Collections.Generic;
using System.Globalization;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// What <see cref="UiEventRouter"/> does with the transient events core sends
/// it. A failed watched-folder scan has to reach the error dialog: the folder
/// list's own mark is behind a menu nobody opens after adding a folder, so the
/// dialog is the only thing that says the folder was never read.
/// </summary>
public sealed class UiEventRouterTests
{
    private static (string Title, string Body) Route(BridgeUiEvent evt)
    {
        (string, string) shown = (string.Empty, string.Empty);
        var router = new UiEventRouter(
            new PlaybackStore(),
            (title, body) => shown = (title, body),
            _ => Assert.Fail("the import lane must not see this event"));
        router.Route(evt);
        return shown;
    }

    [Fact]
    public void AFailedFolderScanRaisesTheErrorDialogNamingTheFolder()
    {
        var previous = CultureInfo.CurrentUICulture;
        CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("en-US");
        try
        {
            var (title, body) = Route(
                new BridgeUiEvent.WatchedFolderScanFailed(
                    "/Users/dima/Music/Rips",
                    "Database error: no such column: author"));

            Assert.Equal("Couldn't scan /Users/dima/Music/Rips", title);
            Assert.Equal("Database error: no such column: author", body);
        }
        finally
        {
            CultureInfo.CurrentUICulture = previous;
        }
    }

    /// The headline is localized; the failure's own text never is.
    [Fact]
    public void TheScanFailureHeadlineFollowsTheLocale()
    {
        var previous = CultureInfo.CurrentUICulture;
        CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("de-DE");
        try
        {
            var (title, body) = Route(
                new BridgeUiEvent.WatchedFolderScanFailed("/Musik/Rips", "no such column: author"));

            Assert.Equal("/Musik/Rips konnte nicht gescannt werden", title);
            Assert.Equal("no such column: author", body);
        }
        finally
        {
            CultureInfo.CurrentUICulture = previous;
        }
    }
}
