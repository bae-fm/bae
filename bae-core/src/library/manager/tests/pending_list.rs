// The pending list the sync status shows: coven's pending records named by
// the rows they are about, each with the error behind it while this process
// holds one.
//
// `include!`d into `manager::tests`.

/// Follow the pending list as the running library does, and wait for the
/// first list `matches` accepts.
#[cfg(feature = "test-utils")]
async fn pending_list_until(
    values: &mut tokio::sync::watch::Receiver<crate::library::SyncStatusSnapshot>,
    matches: impl Fn(&[PendingItem]) -> bool,
) -> Vec<PendingItem> {
    let reached = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        values.wait_for(|status| {
            matches!(&status.pending, Some(Ok(items)) if matches(items))
        }),
    )
    .await
    .map(|value| match &value.unwrap().pending {
        Some(Ok(items)) => items.clone(),
        other => unreachable!("waited for a read list, got {other:?}"),
    });
    reached.unwrap_or_else(|_| {
        let shown = values.borrow().pending.clone().map(|list| {
            list.map(|items| {
                items
                    .iter()
                    .map(|item| (item.subject.clone(), item.wait.reason.clone(), cause_text(item)))
                    .collect::<Vec<_>>()
            })
        });
        panic!("the pending list never reached the expected value: {shown:?}")
    })
}

/// The text of the error behind `item`, with every cause below it.
#[cfg(feature = "test-utils")]
fn cause_text(item: &PendingItem) -> Option<String> {
    item.wait.cause.as_ref().map(|cause| {
        let cause: &(dyn std::error::Error + 'static) = cause.as_ref();
        bae_util::error_text::CauseChain(cause).to_string()
    })
}

/// A file this device is uploading is named on the pending list by its file
/// name and its album, not by coven's ids.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_waiting_upload_is_named_by_its_file_and_album() {
    let (manager, temp, _release, _file_id) = queued_upload_fixture("named").await;
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    // The upload can't read a source that moved, so it waits on the file.
    std::fs::remove_file(temp.path().join("named/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);

    pending_list_until(&mut values, |items| {
        items.iter().any(|item| {
            item.subject
                == crate::library::PendingSubjectName::ReleaseFile {
                    file_name: "a.flac".into(),
                    album_title: "Test Album".into(),
                }
        })
    })
    .await;
}

/// A retry that fails for the same reason keeps its record as it was, and the
/// list still shows the error the retry met, not the first attempt's.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_retry_that_fails_again_shows_its_own_error() {
    let (manager, _temp, _release, _file_id) = queued_upload_fixture("retried").await;
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    let refuse = |status: u16| coven::Faults {
        fail_next: 1,
        failure: coven::StorageFailure::Network,
        response: Some(status),
        ..coven::Faults::none()
    };

    manager.storage.set_faults(refuse(500)).await;
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    let first = pending_list_until(&mut values, |items| {
        items.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("500")))
    })
    .await;

    manager.storage.set_faults(refuse(503)).await;
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    let again = pending_list_until(&mut values, |items| {
        items.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("503")))
    })
    .await;
    assert_eq!(
        first.iter().map(|item| (&item.subject, &item.wait.reason)).collect::<Vec<_>>(),
        again.iter().map(|item| (&item.subject, &item.wait.reason)).collect::<Vec<_>>(),
        "the record stays as it was"
    );
}

/// The pending list keeps the order its records arose in, whatever coven's
/// ids for them: an upload that failed first stays first after a later one
/// fails, and after it fails again.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_pending_list_keeps_the_order_its_records_arose_in() {
    let (manager, temp, first, _file_id) = queued_upload_fixture("first").await;
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    // A file's record as coven stores it begins with its subject, so two
    // records of one reason sort as their subjects' stored text does.
    let stored_subject = |release: String| {
        let manager = manager.library.clone();
        async move {
            let file = release_files(&manager, &release).await.remove(0);
            let file = manager.release_file_ref(&file.id).await.unwrap();
            serde_json::to_vec(&coven::PendingSubject::File {
                device: file.uploader(),
                file: file.id(),
            })
            .unwrap()
        }
    };
    std::fs::remove_file(temp.path().join("first/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    let first_subject = stored_subject(first.id.clone()).await;

    // A second failure whose stored record sorts before the first's, so an
    // order by record would put it first. Releases that sort after upload.
    let mut attempt = 0;
    let second = loop {
        attempt += 1;
        let folder = format!("second-{attempt}");
        let release = insert_release_with_queued_uploads(
            &manager,
            &temp.path().join(&folder),
            &format!("Album Title {attempt}"),
            &[("b.flac", b"second-bytes")],
        )
        .await;
        if stored_subject(release.id.clone()).await < first_subject {
            std::fs::remove_file(temp.path().join(folder).join("b.flac")).unwrap();
            break release;
        }
    };
    manager.drain_uploads_expecting_work().await.unwrap();

    let album = |title: &str| PendingSubjectName::ReleaseFile {
        file_name: if title == "Test Album" { "a.flac" } else { "b.flac" }.into(),
        album_title: title.into(),
    };
    let second_album = format!("Album Title {attempt}");
    let listed = pending_list_until(&mut values, |items| {
        items.iter().filter(|item| matches!(item.subject, PendingSubjectName::ReleaseFile { .. })).count() == 2
    })
    .await;
    assert_eq!(
        listed
            .iter()
            .map(|item| item.subject.clone())
            .filter(|subject| matches!(subject, PendingSubjectName::ReleaseFile { .. }))
            .collect::<Vec<_>>(),
        vec![album("Test Album"), album(&second_album)],
        "{second:?} failed after the first"
    );
}
