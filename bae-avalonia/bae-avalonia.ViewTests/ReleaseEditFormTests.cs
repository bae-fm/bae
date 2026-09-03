using System;
using Avalonia.Headless.XUnit;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ReleaseEditFormTests
{
    [AvaloniaFact]
    public void ReseedingReplacesTheAlbumArtistAssignments()
    {
        var form = new ReleaseEditForm(
            Edit("First Artist"),
            500,
            new LibraryService());

        form.Seed(Edit("Replacement Artist"));

        var assignment = Assert.Single(form.ReadBack().AlbumArtistAssignments);
        Assert.Equal(
            "Replacement Artist",
            Assert.IsType<BridgeArtistAssignment.New>(assignment).Seed.Name);
    }

    private static BridgeRawReleaseEdit Edit(string artist) => new(
        "Album Title",
        new BridgeArtistAssignment[]
        {
            new BridgeArtistAssignment.New(
                new BridgeNewArtistSeed(artist, null, null, null)),
        },
        "1991",
        new BridgeRawPressingEdit(
            "1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012"),
        Array.Empty<BridgeRawTrackEdit>());
}
