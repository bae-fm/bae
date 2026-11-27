use bae_util::test_channels::HANG_GUARD;
use tokio::sync::watch;

use crate::{AskedTurn, PictureTurn, Volumes};

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
        volumes.read_place(b"disk", LOCAL, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"disk", LOCAL, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"disk", LOCAL, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"disk", LOCAL, AskedTurn::unchanging(PictureTurn::Visible)).await,
    ];
    let mut fifth =
        Box::pin(volumes.read_place(b"disk", LOCAL, AskedTurn::unchanging(PictureTurn::Visible)));
    assert!(is_waiting(&mut fifth).await);

    let share = [
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
    ];
    let mut third = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
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
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
    ];
    // The prefetch asks first and the visible request second.
    let mut prefetch = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Prefetch),
    ));
    assert!(is_waiting(&mut prefetch).await);
    let mut visible = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
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
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
    ];
    let mut leaving = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
    assert!(is_waiting(&mut leaving).await);
    let mut staying = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
    assert!(is_waiting(&mut staying).await);
    drop(leaving);
    drop(held);
    tokio::time::timeout(HANG_GUARD, staying)
        .await
        .expect("the place reached the request still waiting");
}

/// Every place a read of any turn takes on `share`, as large reads hold them.
async fn fill(volumes: &Volumes) -> [crate::Permit; NETWORK] {
    [
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Visible)).await,
    ]
}

#[tokio::test]
async fn a_focused_read_never_waits_for_the_reads_that_run() {
    let volumes = Volumes::new();
    let running = fill(&volumes).await;
    let mut visible = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
    assert!(is_waiting(&mut visible).await);

    let focused = tokio::time::timeout(
        HANG_GUARD,
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Focused)),
    )
    .await
    .expect("the focused read got its place while the others were taken");
    assert!(is_waiting(&mut visible).await, "only a focused read takes that place");
    drop(focused);
    assert!(is_waiting(&mut visible).await, "the place stays for the next focused read");
    drop(running);
    tokio::time::timeout(HANG_GUARD, visible).await.expect("the visible read got a place");
}

#[tokio::test]
async fn a_focused_read_goes_before_every_read_that_waits() {
    let volumes = Volumes::new();
    let [first, second] = fill(&volumes).await;
    let reserved =
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Focused)).await;
    let mut visible = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
    assert!(is_waiting(&mut visible).await);
    let mut focused = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Focused),
    ));
    assert!(is_waiting(&mut focused).await);

    drop(first);
    let place =
        tokio::time::timeout(HANG_GUARD, focused).await.expect("the focused read got the place");
    assert!(is_waiting(&mut visible).await, "the visible read went first");
    drop((second, reserved, place));
    tokio::time::timeout(HANG_GUARD, visible).await.expect("the visible read got a place");
}

#[tokio::test]
async fn a_waiting_read_that_becomes_focused_takes_the_focused_place() {
    let volumes = Volumes::new();
    let _running = fill(&volumes).await;
    let (turn, asked) = watch::channel(PictureTurn::Prefetch);
    let mut request = Box::pin(volumes.read_place(b"share", NETWORK, asked.into()));
    assert!(is_waiting(&mut request).await);

    turn.send_replace(PictureTurn::Focused);
    tokio::time::timeout(HANG_GUARD, request).await.expect("the read took the focused place");
}

#[tokio::test]
async fn a_waiting_read_that_stops_being_focused_waits_its_new_turn() {
    let volumes = Volumes::new();
    let [first, second] = fill(&volumes).await;
    let reserved =
        volumes.read_place(b"share", NETWORK, AskedTurn::unchanging(PictureTurn::Focused)).await;
    let (turn, asked) = watch::channel(PictureTurn::Focused);
    let mut left = Box::pin(volumes.read_place(b"share", NETWORK, asked.into()));
    assert!(is_waiting(&mut left).await);
    let mut visible = Box::pin(volumes.read_place(
        b"share",
        NETWORK,
        AskedTurn::unchanging(PictureTurn::Visible),
    ));
    assert!(is_waiting(&mut visible).await);

    // The person moved on, so the visible read that asked later goes first.
    turn.send_replace(PictureTurn::Prefetch);
    drop(reserved);
    assert!(is_waiting(&mut left).await, "a prefetch took the focused place");
    drop(first);
    let place =
        tokio::time::timeout(HANG_GUARD, visible).await.expect("the visible read got the place");
    assert!(is_waiting(&mut left).await, "the prefetch went first");
    drop((second, place));
    tokio::time::timeout(HANG_GUARD, left).await.expect("the prefetch got a place");
}
