use super::*;

/// Poison `buffer`'s state the way a panic partway through a change does.
fn poison(buffer: &SharedSparseBuffer) {
    let poisoner = buffer.clone();
    let panicked = std::thread::spawn(move || {
        let _held = poisoner.inner.lock().unwrap();
        panic!("merge index out of range");
    })
    .join();
    assert!(panicked.is_err());
    assert!(buffer.inner.is_poisoned());
}

/// A panic that poisoned the buffer's state leaves bytes that can't be
/// trusted: they are dropped, and the buffer goes on serving what the fill
/// fetches again, with no reader, stop or drop panicking on the poison.
/// Sabotage: go back to `lock().unwrap()`, and the reader's drop panics.
#[test]
fn a_poisoned_buffer_drops_its_bytes_and_serves_on() {
    let buffer = create_sparse_buffer(8);
    buffer.append_at(0, &[1, 2, 3, 4]);
    let mut reader = buffer.new_reader();
    poison(&buffer);

    assert!(!buffer.is_buffered(0), "the bytes the panic left are dropped");
    assert!(!buffer.inner.is_poisoned());
    buffer.append_at(0, &[5, 6, 7, 8, 9, 10, 11, 12]);
    let mut read = [0u8; 4];
    assert!(matches!(reader.read(&mut read), Ok(4)));
    assert_eq!(read, [5, 6, 7, 8]);
    drop(reader);

    poison(&buffer);
    buffer.cancel();
    assert!(buffer.is_cancelled());
}
