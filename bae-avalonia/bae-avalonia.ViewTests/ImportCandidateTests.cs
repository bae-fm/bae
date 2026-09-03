using System;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ImportCandidateTests
{
    [Fact]
    public void ChangedFileTagsIdentityInvalidatesLoadedPreview()
    {
        var existing = Candidate("aaaaaaaa");
        var session = Assert.IsType<object>(existing.BeginFileTagsPreview());
        Assert.True(existing.CompleteFileTagsPreview(session, FileTagsEdit()));

        var replacement = Candidate("bbbbbbbb");
        replacement.PreserveSessionState(existing);

        Assert.Equal(ImportFileTagsPreviewStatus.Unloaded, replacement.FileTagsPreviewStatus);
        Assert.Null(replacement.FileTagsPreview);
    }

    private static ImportCandidate Candidate(string fileTagsIdentity) => new()
    {
        Files = new BridgeCandidateFiles(
            fileTagsIdentity,
            [
                new BridgeCandidateFile(
                    new BridgeFileInfo(
                        Name: "01.flac",
                        Size: 100,
                        DirPrefix: null,
                        FileName: "01.flac",
                        LocalPath: "/music/01.flac",
                        AudioFormat: null),
                    new BridgeFileRole.Audio(),
                    new BridgeFileBecomes.Slots(1, 1),
                    [],
                    null),
            ],
            SourceAudio: null),
    };

    private static BridgeReleaseUserEdit FileTagsEdit() => new(
        "Album Title",
        [
            new BridgeArtistAssignment.New(
                new BridgeNewArtistSeed("Artist Name", null, null, null)),
        ],
        1991,
        new BridgePressingEdit(2001, "CD", "Label Name", "CAT-1", "US", "0123456789012"),
        Array.Empty<BridgeTrackUserEdit>());
}
