using System;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The identity-claim state behind the import confirmation and re-identify
// dialogs: a source-backed claim offers an exact-vs-metadata-only choice and
// disables the pressing fields when metadata-only; an unknown claim (no source
// release) offers no choice and keeps the pressing fields editable.
public sealed class ImportIdentityModelTests
{
    [Fact]
    public void SourceBacked_DefaultsToExact()
    {
        var model = new ImportIdentityModel(hasSourceRelease: true);
        Assert.False(model.MetadataOnly);
        Assert.True(model.ShowsExactnessChoice);
        Assert.False(model.ShowsMetadataOnlyNote);
        Assert.True(model.PressingFieldsEnabled);
    }

    [Fact]
    public void SourceBacked_FlipToMetadataOnly_ShowsNoteAndDisablesPressingFields()
    {
        var model = new ImportIdentityModel(hasSourceRelease: true);
        model.SetMetadataOnly(true);
        Assert.True(model.MetadataOnly);
        Assert.True(model.ShowsExactnessChoice);
        Assert.True(model.ShowsMetadataOnlyNote);
        Assert.False(model.PressingFieldsEnabled);
    }

    [Fact]
    public void SourceBacked_FlipBackToExact_HidesNoteAndReenablesPressingFields()
    {
        var model = new ImportIdentityModel(hasSourceRelease: true);
        model.SetMetadataOnly(true);
        model.SetMetadataOnly(false);
        Assert.False(model.MetadataOnly);
        Assert.False(model.ShowsMetadataOnlyNote);
        Assert.True(model.PressingFieldsEnabled);
    }

    [Fact]
    public void Unknown_HasNoExactnessChoiceAndKeepsPressingFieldsEditable()
    {
        var model = new ImportIdentityModel(hasSourceRelease: false);
        Assert.False(model.MetadataOnly);
        Assert.False(model.ShowsExactnessChoice);
        Assert.False(model.ShowsMetadataOnlyNote);
        Assert.True(model.PressingFieldsEnabled);
    }

    [Fact]
    public void Unknown_SetMetadataOnly_Throws()
    {
        var model = new ImportIdentityModel(hasSourceRelease: false);
        Assert.Throws<InvalidOperationException>(() => model.SetMetadataOnly(true));
    }
}
