use bae_util::test_channels::HANG_GUARD;

use crate::{Priority, Volumes};

const LOCAL: usize = 4;
const NETWORK: usize = 2;

/// Whether `request` is queued: it decides on its first poll, so one poll tells.
async fn is_waiting<T>(
    request: &mut std::pin::Pin<Box<impl std::future::Future<Output = T>>>,
) -> bool {
    tokio::select! {
        biased;
        _ = request => false,
        () = async {} => true
    }
}

#[tokio::test]
async fn a_network_volume_takes_fewer_reads_at_once() {
    let volumes = Volumes::new();
    let local = [
        volumes.read_place(b"disk", LOCAL, Priority::Visible).await,
        volumes.read_place(b"disk", LOCAL, Priority::Visible).await,
        volumes.read_place(b"disk", LOCAL, Priority::Visible).await,
        volumes.read_place(b"disk", LOCAL, Priority::Visible).await,
    ];
    let mut fifth = Box::pin(volumes.read_place(b"disk", LOCAL, Priority::Visible));
    assert!(is_waiting(&mut fifth).await);

    let share = [
        volumes.read_place(b"share", NETWORK, Priority::Visible).await,
        volumes.read_place(b"share", NETWORK, Priority::Visible).await,
    ];
    let mut third = Box::pin(volumes.read_place(b"share", NETWORK, Priority::Visible));
    assert!(is_waiting(&mut third).await);
    drop(share);
    drop(local);
    drop(third.await);
    drop(fifth.await);
}

#[tokio::test]
async fn a_visible_request_goes_before_a_prefetch() {
    let volumes = Volumes::new();
    let held = [
        volumes.read_place(b"share", NETWORK, Priority::Visible).await,
        volumes.read_place(b"share", NETWORK, Priority::Visible).await,
    ];
    // The prefetch asks first and the visible request second.
    let mut prefetch = Box::pin(volumes.read_place(b"share", NETWORK, Priority::Prefetch));
    assert!(is_waiting(&mut prefetch).await);
    let mut visible = Box::pin(volumes.read_place(b"share", NETWORK, Priority::Visible));
    assert!(is_waiting(&mut visible).await);

    let [first, second] = held;
    drop(first);
    let place =
        tokio::time::timeout(HANG_GUARD, visible).await.expect("the visible request got the place");
    assert!(is_waiting(&mut prefetch).await, "the prefetch went first");
    drop(second);
    drop(place);
    tokio::time::timeout(HANG_GUARD, prefetch).await.expect("the prefetch got a place");
}

#[tokio::test]
async fn a_request_that_stops_waiting_passes_its_place_on() {
    let volumes = Volumes::new();
    let held = [
        volumes.read_place(b"share", NETWORK, Priority::Visible).await,
        volumes.read_place(b"share", NETWORK, Priority::Visible).await,
    ];
    let mut leaving = Box::pin(volumes.read_place(b"share", NETWORK, Priority::Visible));
    assert!(is_waiting(&mut leaving).await);
    let mut staying = Box::pin(volumes.read_place(b"share", NETWORK, Priority::Visible));
    assert!(is_waiting(&mut staying).await);
    drop(leaving);
    drop(held);
    tokio::time::timeout(HANG_GUARD, staying)
        .await
        .expect("the place reached the request still waiting");
}
