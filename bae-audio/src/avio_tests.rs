use super::*;

const SEEK_SET: c_int = 0;
const SEEK_CUR: c_int = 1;
const SEEK_END: c_int = 2;

fn streaming(len: usize) -> StreamingAvioContext {
    StreamingAvioContext::new(
        crate::test_samples::filled_sparse_buffer(&vec![0; len]),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}

#[test]
fn a_streaming_seek_moves_from_the_start_the_position_and_the_end() {
    let ctx = streaming(100);
    assert_eq!(ctx.seek(40, SEEK_SET), 40);
    assert_eq!(ctx.seek(10, SEEK_CUR), 50);
    assert_eq!(ctx.seek(-30, SEEK_END), 70);
}

#[test]
fn a_streaming_seek_before_the_start_is_refused_and_leaves_the_position() {
    let ctx = streaming(100);
    assert_eq!(ctx.seek(20, SEEK_SET), 20);
    assert_eq!(ctx.seek(-30, SEEK_CUR), -1);
    assert_eq!(ctx.seek(-130, SEEK_END), -1);
    assert_eq!(ctx.seek(-1, SEEK_SET), -1);
    assert_eq!(ctx.seek(0, SEEK_CUR), 20);
}
