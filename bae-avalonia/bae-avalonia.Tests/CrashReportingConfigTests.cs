using System.Diagnostics;
using Xunit;

namespace Bae.Desktop.Tests;

// Crash reporting needs every build value; a build missing one leaves it off
// and says so in the log, naming what was missing.
public sealed class CrashReportingConfigTests
{
    private static readonly BaeLogger Logger = new("bae.desktop.tests");

    [Fact]
    public void EveryValuePresentEnablesCrashReporting()
    {
        var config = BaeCrashReporting.Config("1.2.3", "https://key@sentry.example/1", "production", "abc1234", Logger);

        Assert.NotNull(config);
        Assert.Equal("bae@1.2.3+abc1234", config.Release);
    }

    [Fact]
    public void AMissingValueLeavesCrashReportingOffWithAWarning()
    {
        var trace = new StringWriter();
        var listener = new TextWriterTraceListener(trace);
        Trace.Listeners.Add(listener);
        try
        {
            var config = BaeCrashReporting.Config("1.2.3", null, "production", "abc1234", Logger);

            Assert.Null(config);
            listener.Flush();
            Assert.Contains("BaeSentryDsn", trace.ToString());
        }
        finally
        {
            Trace.Listeners.Remove(listener);
        }
    }
}
