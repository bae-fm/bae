using System;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ReleaseActionDialogsTests
{
    [AvaloniaFact]
    public async Task MetadataWithoutAppliedSourceOmitsResetToSource()
    {
        var buttons = await Buttons(canResetToSource: false);

        Assert.DoesNotContain(Loc.Chrome("album.edit.reset"), buttons);
    }

    [AvaloniaFact]
    public async Task SourceBackedMetadataOffersResetToSource()
    {
        var buttons = await Buttons(canResetToSource: true);

        Assert.Contains(Loc.Chrome("album.edit.reset"), buttons);
    }

    private static async Task<string[]> Buttons(bool canResetToSource)
    {
        var seed = new BridgeReleaseEditSeed(Edit(), canResetToSource);
        var releaseEditor = new ReleaseEditorService
        {
            ReleaseEditSeed = _ => Task.FromResult(
                (true, ((BridgeReleaseEditSeed?)seed, (string?)null))),
        };
        var app = AppService.Stubbed(
            new SessionStore(Dispatcher.UIThread),
            Dispatcher.UIThread,
            new LibraryService(),
            releaseEditor: releaseEditor);
        var host = new ModalHost();
        var dialogs = new ReleaseActionDialogs(app, host, new LightboxOverlay());

        var presentation = dialogs.ShowEditMetadata("release-1");
        var buttons = host.GetLogicalDescendants()
            .OfType<Button>()
            .Select(button => button.Content as string)
            .Where(label => label is not null)
            .Cast<string>()
            .ToArray();
        host.Close();
        await presentation;
        app.Dispose();
        return buttons;
    }

    private static BridgeRawReleaseEdit Edit() => new(
        "Album Title",
        new BridgeArtistAssignment[]
        {
            new BridgeArtistAssignment.New(
                new BridgeNewArtistSeed("Artist Name", null, null, null)),
        },
        new BridgeRawPressingEdit(
            "1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012"),
        Array.Empty<BridgeRawTrackEdit>());
}
