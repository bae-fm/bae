use super::*;

use crate::service::tests::*;

/// What an ended service's parts sent reaches no later service, however late
/// it arrives, while the handle's commands reach it in order. Sabotage: serve
/// own commands whatever their number, and the halt reaches the fresh service.
#[tokio::test]
async fn a_service_reads_none_of_what_an_ended_service_sent_itself() {
    let (ended, mut messages) = OwnCommands::channel();
    let fresh = ended.of_the_next_service();
    dispatch_command(&ended, PlaybackCommand::HaltOnError);
    ended.channel.send(PlaybackMessage::Asked(PlaybackCommand::Pause)).unwrap();
    dispatch_command(&ended, PlaybackCommand::OutputInvalidated);
    dispatch_command(&fresh, PlaybackCommand::Stop);

    let first = next_command(&mut messages, fresh.service).await;
    assert!(matches!(first, Some(PlaybackCommand::Pause)), "{first:?}");
    let second = next_command(&mut messages, fresh.service).await;
    assert!(matches!(second, Some(PlaybackCommand::Stop)), "{second:?}");
}

/// A fresh service takes up the repeat mode and the level the tables and rows
/// hold though the resume row is empty, as it is after a stop. Sabotage: read
/// them only with a resume row, and it plays at full volume under a screen
/// showing the person's level.
#[tokio::test]
async fn a_fresh_service_takes_up_the_repeat_mode_and_level_of_a_stopped_queue() {
    let (store, mut service, _progress) = seeded_playback_service(&[]).await;
    let db = store.store();
    crate::tables::PlaybackQueueTables::new(db.clone())
        .set_repeat(RepeatMode::Track)
        .await
        .unwrap();
    crate::tables::PlaybackNowRow::new(db.clone())
        .apply(&crate::progress::PlaybackChange::Volume(0.3))
        .await
        .unwrap();
    crate::tables::PlaybackNowRow::new(db.clone())
        .apply(&crate::progress::PlaybackChange::Muted(true))
        .await
        .unwrap();

    service.restore_after_an_end().await;

    assert_eq!(service.playback_queue.repeat_mode(), RepeatMode::Track);
    assert_eq!(
        service.volume,
        OutputVolume::Muted { level: crate::Volume::new(0.3).expect("a level") }
    );
}
