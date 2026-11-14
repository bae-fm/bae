use super::frame_clock_ns;

#[test]
fn the_frame_clock_moves_forward_with_real_time() {
    let before = frame_clock_ns();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let after = frame_clock_ns();
    let elapsed_ms = (after - before) / 1_000_000;
    assert!((20..1_000).contains(&elapsed_ms), "{elapsed_ms} ms passed");
}
