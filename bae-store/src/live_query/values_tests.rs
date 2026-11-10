use super::*;

/// A source that answers with each of `results` in turn, then ends.
struct Answers(std::collections::VecDeque<Result<u8, crate::DbError>>);

impl LiveSource<u8> for Answers {
    fn next(&mut self) -> NextValue<'_, u8> {
        let next = self.0.pop_front();
        Box::pin(async move { next })
    }
}

fn bug(message: &str) -> coven::task::Bug {
    coven::task::Bug { operation: "live query", message: message.into() }
}

/// A run that panicked is reported once as the pull hears it; the store
/// failing on an earlier bug, which coven repeats, is only shown; the
/// source then ends.
#[tokio::test]
async fn a_runs_bug_is_reported_once_and_a_repeat_is_only_shown() {
    let heard = coven::task::HeardBugs::shared();
    let values = LiveValues::new(
        Answers(
            [
                Err(coven::DbError::Bug(bug("a run broke")).into()),
                Err(coven::DbError::StoreFailed(bug("earlier")).into()),
            ]
            .into(),
        ),
        CancellationToken::new(),
        heard.clone(),
    );

    assert!(matches!(values.next().await, Err(LiveQueryError::Query(_))));
    assert_eq!(heard.take(), vec![bug("a run broke")]);

    let repeat = values.next().await;
    assert!(matches!(repeat, Err(LiveQueryError::Query(_))));
    assert_eq!(heard.take(), Vec::new());

    assert!(matches!(values.next().await, Err(LiveQueryError::Cancelled)));
}
