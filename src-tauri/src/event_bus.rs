use tokio::sync::broadcast;

use crate::models::NormalizedEvent;

const CHANNEL_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<NormalizedEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    pub fn publish(&self, event: NormalizedEvent) {
        // Ignore send errors — they only occur when there are no active receivers,
        // which is fine (events are dropped silently).
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedEvent> {
        self.sender.subscribe()
    }
}
