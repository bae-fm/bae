using System.Collections.Generic;
using System.Threading.Tasks;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ImportSheetBindingTests
{
    [Theory]
    [InlineData("audio.flac")]
    [InlineData(null)]
    public async Task BindingWritesCarryTheExactFileReference(string? audio)
    {
        var calls = new List<(string, string, string, string?)>();
        var service = new ImportService
        {
            SetSheetBinding = (candidate, sheet, reference, file) =>
            {
                calls.Add((candidate, sheet, reference, file));
                return Task.FromResult((true, (string?)null));
            },
        };
        using var store = new ImportStore(service, (_, _) => Assert.Fail("Unexpected error"), action => action());
        Assert.True(await store.SetSheetBinding("candidate", "album.cue", "second.wav", audio));
        Assert.Equal(new[] { ("candidate", "album.cue", "second.wav", audio) }, calls);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task BindingErrorsOnlySurfaceForTheCurrentSession(bool current)
    {
        var errors = new List<string>();
        var service = new ImportService
        {
            SetSheetBinding = (_, _, _, _) => Task.FromResult((current, (string?)"binding failed")),
        };
        using var store = new ImportStore(service, (_, error) => errors.Add(error), action => action());
        Assert.False(await store.SetSheetBinding("candidate", "album.cue", "second.wav", null));
        Assert.Equal(current ? new[] { "binding failed" } : [], errors);
    }

    [Fact]
    public async Task OptionsRetainReferencesAndCurrentAssociations()
    {
        var references = new List<BridgeSheetReferenceOptions>
        {
            new("first.wav", "audio.flac", []),
            new("second.wav", null, []),
        };
        var service = new ImportService
        {
            SheetBindingOptions = (candidate, sheet) =>
            {
                Assert.Equal("candidate", candidate);
                Assert.Equal("album.cue", sheet);
                return Task.FromResult((true, ((List<BridgeSheetReferenceOptions>?)references, (string?)null)));
            },
        };
        using var store = new ImportStore(service, (_, _) => Assert.Fail("Unexpected error"), action => action());
        Assert.Same(references, await store.SheetBindingOptions("candidate", "album.cue"));
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task OptionErrorsOnlySurfaceForTheCurrentSession(bool current)
    {
        var errors = new List<string>();
        var service = new ImportService
        {
            SheetBindingOptions = (_, _) => Task.FromResult((current,
                ((List<BridgeSheetReferenceOptions>?)null, (string?)"options failed"))),
        };
        using var store = new ImportStore(service, (_, error) => errors.Add(error), action => action());
        Assert.Empty(await store.SheetBindingOptions("candidate", "album.cue"));
        Assert.Equal(current ? new[] { "options failed" } : [], errors);
    }
}
