use std::sync::Mutex;

#[derive(uniffi::Object)]
pub struct LiveSubscription {
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl LiveSubscription {
    pub(crate) fn new(task: tokio::task::JoinHandle<()>) -> Self {
        Self {
            task: Mutex::new(Some(task)),
        }
    }

    fn abort(&self) {
        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
        }
    }
}

#[uniffi::export]
impl LiveSubscription {
    pub fn cancel(&self) {
        self.abort();
    }
}

impl Drop for LiveSubscription {
    fn drop(&mut self) {
        self.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::pending;
    use tokio::sync::oneshot;

    struct DropSignal(Option<oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    fn pending_task() -> (
        tokio::task::JoinHandle<()>,
        oneshot::Receiver<()>,
        oneshot::Receiver<()>,
    ) {
        let (started_tx, started_rx) = oneshot::channel();
        let (dropped_tx, dropped_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _drop_signal = DropSignal(Some(dropped_tx));
            started_tx.send(()).expect("test receives task start");
            pending::<()>().await;
        });
        (task, started_rx, dropped_rx)
    }

    #[tokio::test]
    async fn cancel_aborts_the_delivery_task() {
        let (task, started, dropped) = pending_task();
        let subscription = LiveSubscription::new(task);
        started.await.expect("delivery task starts");

        subscription.cancel();

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
            .await
            .expect("cancel aborts the task")
            .expect("task owns the drop signal");
    }

    #[tokio::test]
    async fn drop_aborts_the_delivery_task() {
        let (task, started, dropped) = pending_task();
        let subscription = LiveSubscription::new(task);
        started.await.expect("delivery task starts");

        drop(subscription);

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
            .await
            .expect("drop aborts the task")
            .expect("task owns the drop signal");
    }
}
