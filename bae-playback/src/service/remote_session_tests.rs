//! The session a remote renderer is played through: which events are its own.

use super::*;
use crate::renderer::channel::FakeChannel;
use crate::renderer::{RendererFlavor, RendererSessionEvent};
use crate::service::tests::*;

pub(super) fn remote_connect(channel: FakeChannel) -> RemoteConnect {
    RemoteConnect::new(
        channel.polled(),
        test_device(),
        crate::renderer::RendererMediaSource::new(
            test_stream_provider(),
            Arc::new(|cover| {
                format!("http://renderer.local/cover?id={}&v={}", cover.id, cover.version)
            }),
            RendererFlavor::Cast,
        ),
    )
}

/// The id of the session `service` is playing through.
pub(super) fn live_session(service: &PlaybackService) -> crate::renderer::SessionId {
    service.renderer.live_session().expect("the service plays on a remote renderer")
}

/// Events of a session that is no longer the live one change nothing: they were
/// waiting in the command queue when it was dropped.
#[tokio::test]
async fn an_event_of_an_earlier_session_is_ignored() {
    let (_home, mut service, _state, mut rx) = remote_over_fake().await;
    let earlier = live_session(&service);
    service.handle_stop_remote().await;
    service.handle_play_on(remote_connect(FakeChannel::new())).await;
    assert_ne!(live_session(&service), earlier, "a new session has a new id");
    while rx.try_recv().is_ok() {}

    service
        .handle_remote_event(RendererSessionEvent::Ended {
            session: earlier,
            error: crate::renderer::RendererError::Connection("the old link".to_string()),
        })
        .await;
    service
        .handle_remote_event(RendererSessionEvent::CommandFailed {
            session: earlier,
            error: crate::renderer::RendererError::Command("the old load".to_string()),
        })
        .await;

    assert!(service.renderer.is_remote(), "the live session goes on");
    assert!(rx.try_recv().is_err(), "nothing is announced or reported");
}
