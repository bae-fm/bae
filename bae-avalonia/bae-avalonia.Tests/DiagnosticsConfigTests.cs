using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.Tests;

// Telemetry needs every build value: a build missing any one sends nothing,
// rather than telemetry tagged with an empty environment or commit.
public sealed class DiagnosticsConfigTests
{
    private static readonly BaeLogger Logger = new("bae.desktop.tests");

    [Fact]
    public void EveryValuePresentEnablesTelemetry()
    {
        var config = NativeBae.DiagnosticsConfig("site", "token", "linux", "bae", "production", "1.2.3", "bae", "abc1234", Logger);

        var enabled = Assert.IsType<BridgeDiagnosticsConfig.Enabled>(config);
        Assert.Equal("production", enabled.Config.App.Environment);
        Assert.Equal("abc1234", enabled.Config.App.GitCommit);
    }

    [Theory]
    [InlineData(null, "token", "production", "1.2.3", "abc1234")]
    [InlineData("site", null, "production", "1.2.3", "abc1234")]
    [InlineData("site", "token", null, "1.2.3", "abc1234")]
    [InlineData("site", "token", "production", null, "abc1234")]
    [InlineData("site", "token", "production", "1.2.3", null)]
    public void AnyValueMissingLeavesTelemetryOff(string? site, string? token, string? environment, string? version, string? commit)
    {
        var config = NativeBae.DiagnosticsConfig(site, token, "linux", "bae", environment, version, "bae", commit, Logger);

        Assert.IsType<BridgeDiagnosticsConfig.Disabled>(config);
    }
}
