using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class BridgeArtworkStepTests
{
    [Fact]
    public void GeneratedArtworkStatesPreserveProgressAndFailureWhenSerialized()
    {
        BridgeArtworkStep[] steps =
        [
            new BridgeArtworkStep.Absent(),
            new BridgeArtworkStep.Reading("cover.jpg", 2, 5, 1, 3),
            new BridgeArtworkStep.Read(5, 1, 3),
            new BridgeArtworkStep.Failed(new BridgeLookupFailure.ArtworkAnalysis(), 2, 5),
        ];
        var converter = FfiConverterTypeBridgeArtworkStep.INSTANCE;
        foreach (var step in steps)
        {
            using var memory = new MemoryStream();
            var stream = new BigEndianStream(memory);
            converter.Write(step, stream);
            Assert.Equal(converter.AllocationSize(step), memory.Length);
            memory.Position = 0;
            Assert.Equal(step, converter.Read(stream));
            Assert.False(stream.HasRemaining());
        }
    }
}
