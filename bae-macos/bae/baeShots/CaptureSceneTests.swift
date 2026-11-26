import Testing

/// Takes bae's Mac shots: the builds harness draws each scene the run asks for
/// in bae's look, and writes `<board>@macos.png` with the elements its view
/// lists beside it as `<board>@macos.json` (`ShotRunner`). `builds shots` runs
/// this test through `bae-macos/shots-capture.sh`.
@MainActor
struct CaptureSceneTests {
    @Test
    func captureAllScenes() async throws {
        try await ShotRunner.capture(ShotScene.all, in: BaeShotLook())
    }
}
