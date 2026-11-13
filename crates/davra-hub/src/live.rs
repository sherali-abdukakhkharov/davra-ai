//! Live delivery: a Postgres trigger NOTIFYs `davra_messages` with the project
//! id on every stored message; one listener per hub fans it out to waiting
//! inbox requests through a broadcast channel.

use std::time::Duration;

use sqlx::postgres::PgListener;
use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

pub type Events = broadcast::Sender<Uuid>;

/// The longest an inbox request may wait (stays under proxy idle timeouts).
pub const MAX_WAIT: Duration = Duration::from_secs(25);

pub fn channel() -> Events {
    broadcast::channel(256).0
}

/// Forwards NOTIFYs into `events` forever, reconnecting after errors.
pub async fn run_listener(pool: PgPool, events: Events) {
    loop {
        match listen(&pool, &events).await {
            Ok(()) => {}
            Err(e) => tracing::warn!(error = %e, "message listener failed, retrying in 2s"),
        }
        // Waiters re-check the database on any event, so a spurious wake-up
        // after a reconnect costs one query and loses nothing.
        let _ = events.send(Uuid::nil());
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn listen(pool: &PgPool, events: &Events) -> anyhow::Result<()> {
    let mut l = PgListener::connect_with(pool).await?;
    l.listen("davra_messages").await?;
    tracing::info!("listening for new messages");
    loop {
        let n = l.recv().await?;
        if let Ok(id) = n.payload().parse::<Uuid>() {
            // No receivers is normal: nobody is waiting right now.
            let _ = events.send(id);
        }
    }
}

/// Waits until `project` may have a new message, or `deadline` passes.
/// Returns false on timeout. Subscribe before the first database check.
pub async fn wait_for(
    rx: &mut broadcast::Receiver<Uuid>,
    project: Uuid,
    deadline: tokio::time::Instant,
) -> bool {
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Err(_) => return false,
            Ok(Ok(id)) if id == project || id.is_nil() => return true,
            Ok(Ok(_)) => continue,
            // Missed some events: check the database to be safe.
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => return true,
            Ok(Err(broadcast::error::RecvError::Closed)) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waits_for_its_project_only() {
        let tx = channel();
        let mut rx = tx.subscribe();
        let mine = Uuid::new_v4();
        tx.send(Uuid::new_v4()).unwrap();
        let soon = tokio::time::Instant::now() + Duration::from_millis(50);
        assert!(
            !wait_for(&mut rx, mine, soon).await,
            "another project's event"
        );
        tx.send(mine).unwrap();
        let later = tokio::time::Instant::now() + Duration::from_secs(1);
        assert!(wait_for(&mut rx, mine, later).await);
    }
}
