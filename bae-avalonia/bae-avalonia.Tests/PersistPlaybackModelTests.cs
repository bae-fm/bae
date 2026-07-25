using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the token parse/serialize for the "restore on launch" playback
/// preference: round-trip, and the off-by-default fallback for missing or
/// unrecognized tokens.
/// </summary>
public sealed class PersistPlaybackModelTests
{
    [Fact]
    public void Token_OnIsOn() =>
        Assert.Equal("on", PersistPlaybackModel.Token(true));

    [Fact]
    public void Token_OffIsOff() =>
        Assert.Equal("off", PersistPlaybackModel.Token(false));

    [Fact]
    public void PersistFromToken_RoundTripsOn() =>
        Assert.True(PersistPlaybackModel.PersistFromToken(PersistPlaybackModel.Token(true)));

    [Fact]
    public void PersistFromToken_RoundTripsOff() =>
        Assert.False(PersistPlaybackModel.PersistFromToken(PersistPlaybackModel.Token(false)));

    [Fact]
    public void PersistFromToken_NullIsFalse() =>
        Assert.False(PersistPlaybackModel.PersistFromToken(null));

    [Fact]
    public void PersistFromToken_EmptyIsFalse() =>
        Assert.False(PersistPlaybackModel.PersistFromToken(""));

    [Fact]
    public void PersistFromToken_UnknownIsFalse() =>
        Assert.False(PersistPlaybackModel.PersistFromToken("yes"));

    [Fact]
    public void PersistFromToken_TrimsWhitespace() =>
        Assert.True(PersistPlaybackModel.PersistFromToken(" on\n"));

    [Fact]
    public void PersistFromToken_IsCaseSensitive() =>
        Assert.False(PersistPlaybackModel.PersistFromToken("On"));
}
