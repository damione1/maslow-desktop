// Shared broadcast-to-proto stream adapters for the machine-event and
// job-progress watch endpoints. Both the gRPC watchers (`grpc/machine.rs`,
// `grpc/job.rs`) and their HTTP SSE equivalents (`http/machine.rs`,
// `http/job.rs`) build their streams on top of the two functions here, so the
// filtering and lag-handling logic is written exactly once and shared, not
// duplicated per transport.

use crate::proto::maslow::v1 as pb;
use crate::service::snapshot::MachineEvent;
use futures_util::{Stream, StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;

/// Adapts a broadcast receiver of domain events into a stream of converted
/// `MachineEvent` proto messages, for `WatchMachineEvents` and its HTTP SSE
/// equivalent.
///
/// Job progress is excluded: it has no case in the `MachineEvent` oneof by
/// design (see `machine.proto`'s comment on the message), since it is
/// high-frequency and has its own `WatchJobProgress` stream. A lagged
/// subscriber (the broadcast channel overflowed before this receiver could
/// keep up) is logged and skipped rather than ending the stream: the lag
/// itself is recoverable, and the caller should keep receiving subsequent
/// events instead of having the stream terminate.
pub fn machine_event_stream(rx: broadcast::Receiver<MachineEvent>) -> impl Stream<Item = pb::MachineEvent> {
    BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(event) => {
                let proto = pb::MachineEvent::from(event);
                proto.payload.is_some().then_some(proto)
            }
            Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                eprintln!("machine event watcher lagged, dropped {skipped} events");
                None
            }
        }
    })
}

/// Same adaptation as `machine_event_stream`, but for `WatchJobProgress` and
/// its HTTP SSE equivalent: filters the broadcast down to only `JobProgress`
/// events (every other event type is discarded silently) and converts the
/// inner `streaming::Progress` to `pb::Job`.
pub fn job_progress_stream(rx: broadcast::Receiver<MachineEvent>) -> impl Stream<Item = pb::Job> {
    BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(MachineEvent::JobProgress(progress)) => Some(pb::Job::from(progress)),
            Ok(_) => None,
            Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                eprintln!("job progress watcher lagged, dropped {skipped} events");
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grbl::MachineStatus;
    use crate::streaming::Progress;

    fn progress(total: usize) -> Progress {
        Progress {
            state: "running".to_string(),
            path: "/x.nc".to_string(),
            name: "x.nc".to_string(),
            sent: 1,
            acked: 0,
            total,
            errors: 0,
        }
    }

    /// A bare `broadcast` channel, without a full `MaslowService`, is enough
    /// to exercise these adapters: they take a `Receiver<MachineEvent>`
    /// directly for exactly this reason (see the module doc comment).
    #[tokio::test]
    async fn machine_event_stream_converts_and_excludes_job_progress() {
        let (tx, rx) = broadcast::channel(16);
        tx.send(MachineEvent::GrblLine("ok".to_string())).unwrap();
        tx.send(MachineEvent::JobProgress(progress(10))).unwrap();
        tx.send(MachineEvent::CalComplete).unwrap();
        drop(tx);

        let items: Vec<pb::MachineEvent> = machine_event_stream(rx).collect().await;

        assert_eq!(items.len(), 2);
        assert!(matches!(
            items[0].payload,
            Some(pb::machine_event::Payload::GrblLine(ref s)) if s == "ok"
        ));
        assert!(matches!(items[1].payload, Some(pb::machine_event::Payload::CalComplete(true))));
    }

    #[tokio::test]
    async fn job_progress_stream_only_yields_job_progress() {
        let (tx, rx) = broadcast::channel(16);
        tx.send(MachineEvent::MachineStatus(MachineStatus::default())).unwrap();
        tx.send(MachineEvent::JobProgress(progress(42))).unwrap();
        tx.send(MachineEvent::GrblLine("noise".to_string())).unwrap();
        drop(tx);

        let items: Vec<pb::Job> = job_progress_stream(rx).collect().await;

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].total, 42);
    }

    #[tokio::test]
    async fn machine_event_stream_skips_lagged_gap_and_keeps_going() {
        // Capacity 1 forces the second send to overwrite the first before it
        // is read, so the receiver observes a `Lagged` error rather than
        // missing the subsequent event entirely.
        let (tx, rx) = broadcast::channel(1);
        tx.send(MachineEvent::GrblLine("dropped".to_string())).unwrap();
        tx.send(MachineEvent::GrblLine("kept".to_string())).unwrap();
        drop(tx);

        let items: Vec<pb::MachineEvent> = machine_event_stream(rx).collect().await;

        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0].payload,
            Some(pb::machine_event::Payload::GrblLine(ref s)) if s == "kept"
        ));
    }
}
