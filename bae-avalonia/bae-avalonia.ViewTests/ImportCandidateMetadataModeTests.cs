using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ImportCandidateMetadataModeTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void UnseededCandidateUsesTheResolvedConfigMode(int index)
    {
        var configured = Modes[index];
        Assert.Equal(
            configured,
            ImportCandidate.ResolvePresentedMetadataMode(null, configured));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void StoredSeedOverridesEveryUnseededDefault(int index)
    {
        var configured = Modes[index];
        Assert.Equal(
            BridgeImportMetadataMode.Manual,
            ImportCandidate.ResolvePresentedMetadataMode(
                new BridgeMetadataSeed.Manual(),
                configured));
    }

    [Fact]
    public void DetailRefreshPreservesModeButAnotherCandidateResolvesItsOwn()
    {
        var existing = new ImportCandidate();
        existing.ResolvePresentedMetadataMode(BridgeImportMetadataMode.FileTags);
        existing.PresentMetadataMode(BridgeImportMetadataMode.Manual);

        var refreshed = new ImportCandidate();
        refreshed.ResolvePresentedMetadataMode(BridgeImportMetadataMode.Lookup);
        refreshed.PreserveSessionState(existing);

        var different = new ImportCandidate();
        different.ResolvePresentedMetadataMode(BridgeImportMetadataMode.Lookup);

        Assert.Equal(BridgeImportMetadataMode.Manual, refreshed.PresentedMetadataMode);
        Assert.Equal(BridgeImportMetadataMode.Lookup, different.PresentedMetadataMode);
    }

    [Fact]
    public void DetailRefreshPreservesFileTagsPreviewOnlyForTheSameFiles()
    {
        var existing = new ImportCandidate();
        var session = Assert.IsType<object>(existing.BeginFileTagsPreview());
        Assert.True(existing.CompleteFileTagsPreview(session, FileTagsEdit()));

        var sameFiles = new ImportCandidate();
        sameFiles.PreserveSessionState(existing);

        var changedFiles = new ImportCandidate
        {
            Files = new BridgeCandidateFiles([], "FLAC", []),
        };
        changedFiles.PreserveSessionState(existing);

        Assert.Equal(ImportFileTagsPreviewStatus.Loaded, sameFiles.FileTagsPreviewStatus);
        Assert.NotNull(sameFiles.FileTagsPreview);
        Assert.Equal(ImportFileTagsPreviewStatus.Unloaded, changedFiles.FileTagsPreviewStatus);
        Assert.Null(changedFiles.FileTagsPreview);
    }

    private static BridgeReleaseUserEdit FileTagsEdit() => new(
        "Album Title",
        [new BridgeArtistAssignment.New(
            new BridgeNewArtistSeed("Artist Name", null, null, null))],
        new BridgePressingEdit(null, null, null, null, null, null),
        []);

    private static BridgeImportMetadataMode[] Modes =>
    [
        BridgeImportMetadataMode.Lookup,
        BridgeImportMetadataMode.FileTags,
        BridgeImportMetadataMode.Manual,
    ];
}
