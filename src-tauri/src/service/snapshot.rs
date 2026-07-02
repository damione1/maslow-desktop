// In-memory cache of the latest machine telemetry, plus a broadcast channel
// that mirrors every Tauri event emitted from `connection.rs`/`streaming.rs`.
// This lets a future query/streaming transport (gRPC/HTTP/MCP) answer "what is
// the machine's status right now" and subscribe to live updates, without
// touching how the frontend receives its events today.

use crate::service::machine::MaslowService;
use std::sync::{Arc, RwLock};

/// Connection state of the live WebSocket. Only the two states the connection
/// loop actually reports today: nothing currently emits a "connecting" state.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum WsState {
    #[default]
    Disconnected,
    Connected,
}

/// Latest known value of every piece of telemetry the connection loop and the
/// job streamer emit. Updated in lock-step with the corresponding Tauri event,
/// so a reader can ask for current state without waiting for the next push.
#[derive(Default)]
pub struct MachineSnapshot {
    pub status: Option<crate::grbl::MachineStatus>,
    pub action_policy: Option<crate::maslow::ActionPolicy>,
    pub job_progress: Option<crate::streaming::Progress>,
    pub maslow_info: Option<crate::maslow::MaslowInfo>,
    pub maslow_state: Option<crate::maslow::StatePolicy>,
    pub anchors: Option<crate::maslow::Anchors>,
    pub config_entries: Option<Vec<crate::fluidnc::ConfigEntry>>,
    pub ws_state: WsState,
}

/// Shared handle to the snapshot, held by `MaslowService` and safe to read
/// from any thread. A plain `std::sync::RwLock` is enough here: every writer
/// updates a single field and never holds the lock across an `.await`.
pub type SharedSnapshot = Arc<RwLock<MachineSnapshot>>;

/// One update, mirroring an existing Tauri event 1:1. Carries the same payload
/// that was (or is about to be) emitted to the frontend.
///
/// Several variants carry a payload nothing in this crate reads yet: this PR
/// only wires up publishing (snapshot cache + broadcast), not a consumer. The
/// future gRPC/HTTP/MCP query and streaming endpoints are what will read
/// these fields, so `dead_code` is expected here rather than a sign of an
/// unused variant.
#[allow(dead_code)]
#[derive(Clone)]
pub enum MachineEvent {
    WsState(WsState),
    WsError(String),
    GrblLine(String),
    MachineStatus(crate::grbl::MachineStatus),
    WsControl(String),
    ActionPolicy(crate::maslow::ActionPolicy),
    Anchors(crate::maslow::Anchors),
    MaslowInfo(crate::maslow::MaslowInfo),
    MaslowState(crate::maslow::StatePolicy),
    Waypoint(crate::maslow::Waypoint),
    Discord(crate::connection::Discord),
    CalMeasurements(Vec<crate::calibration::Measurement>),
    CalFirmwareFit(crate::calibration::FirmwareFit),
    CalFirmwareAnchors(crate::calibration::AnchorParams),
    CalComplete,
    ConfigDump(Vec<crate::fluidnc::ConfigEntry>),
    ConfigDumpError(String),
    JobProgress(crate::streaming::Progress),
}

impl std::fmt::Debug for MachineEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            MachineEvent::WsState(_) => "WsState",
            MachineEvent::WsError(_) => "WsError",
            MachineEvent::GrblLine(_) => "GrblLine",
            MachineEvent::MachineStatus(_) => "MachineStatus",
            MachineEvent::WsControl(_) => "WsControl",
            MachineEvent::ActionPolicy(_) => "ActionPolicy",
            MachineEvent::Anchors(_) => "Anchors",
            MachineEvent::MaslowInfo(_) => "MaslowInfo",
            MachineEvent::MaslowState(_) => "MaslowState",
            MachineEvent::Waypoint(_) => "Waypoint",
            MachineEvent::Discord(_) => "Discord",
            MachineEvent::CalMeasurements(_) => "CalMeasurements",
            MachineEvent::CalFirmwareFit(_) => "CalFirmwareFit",
            MachineEvent::CalFirmwareAnchors(_) => "CalFirmwareAnchors",
            MachineEvent::CalComplete => "CalComplete",
            MachineEvent::ConfigDump(_) => "ConfigDump",
            MachineEvent::ConfigDumpError(_) => "ConfigDumpError",
            MachineEvent::JobProgress(_) => "JobProgress",
        };
        f.write_str(name)
    }
}

/// Update the in-memory snapshot (for the variants that represent durable
/// state) and broadcast the event to any subscriber. The lock is held only
/// long enough to update the relevant field, and is released before the
/// broadcast send.
pub fn publish(svc: &Arc<MaslowService>, event: MachineEvent) {
    apply(&svc.snapshot, &svc.events, event);
}

/// The actual snapshot-update-and-broadcast logic, factored out of `publish`
/// so it can be unit tested against a bare snapshot and channel: building a
/// real `MaslowService` needs a live `AppHandle`, which (unlike Tauri's mock
/// runtime, which is a different `Runtime` type from the `Wry` one this app
/// uses) is not available in a unit test.
fn apply(
    snapshot: &SharedSnapshot,
    events: &tokio::sync::broadcast::Sender<MachineEvent>,
    event: MachineEvent,
) {
    {
        let mut snap = snapshot.write().unwrap();
        match &event {
            MachineEvent::MachineStatus(s) => snap.status = Some(s.clone()),
            MachineEvent::ActionPolicy(p) => snap.action_policy = Some(p.clone()),
            MachineEvent::JobProgress(p) => snap.job_progress = Some(p.clone()),
            MachineEvent::MaslowInfo(i) => snap.maslow_info = Some(i.clone()),
            MachineEvent::MaslowState(s) => snap.maslow_state = Some(s.clone()),
            MachineEvent::Anchors(a) => snap.anchors = Some(a.clone()),
            MachineEvent::ConfigDump(c) => snap.config_entries = Some(c.clone()),
            MachineEvent::WsState(w) => snap.ws_state = w.clone(),
            _ => {}
        }
    }
    let _ = events.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grbl::MachineStatus;
    use crate::streaming::Progress;

    fn fixture() -> (SharedSnapshot, tokio::sync::broadcast::Sender<MachineEvent>) {
        let snapshot: SharedSnapshot = Arc::new(RwLock::new(MachineSnapshot::default()));
        let (events, _rx) = tokio::sync::broadcast::channel(16);
        (snapshot, events)
    }

    #[test]
    fn publish_updates_machine_status_snapshot() {
        let (snapshot, events) = fixture();
        let status = MachineStatus {
            state: "Idle".to_string(),
            ..Default::default()
        };
        apply(&snapshot, &events, MachineEvent::MachineStatus(status.clone()));

        let snap = snapshot.read().unwrap();
        assert_eq!(snap.status.as_ref().map(|s| s.state.as_str()), Some("Idle"));
    }

    #[test]
    fn publish_updates_job_progress_snapshot() {
        let (snapshot, events) = fixture();
        let progress = Progress {
            state: "running".to_string(),
            path: "/x.nc".to_string(),
            name: "x.nc".to_string(),
            sent: 1,
            acked: 0,
            total: 10,
            errors: 0,
        };
        apply(&snapshot, &events, MachineEvent::JobProgress(progress));

        let snap = snapshot.read().unwrap();
        assert_eq!(snap.job_progress.as_ref().map(|p| p.total), Some(10));
    }

    #[test]
    fn publish_updates_ws_state_snapshot() {
        let (snapshot, events) = fixture();
        assert_eq!(snapshot.read().unwrap().ws_state, WsState::Disconnected);

        apply(&snapshot, &events, MachineEvent::WsState(WsState::Connected));

        assert_eq!(snapshot.read().unwrap().ws_state, WsState::Connected);
    }

    #[test]
    fn publish_grbl_line_does_not_touch_snapshot() {
        let (snapshot, events) = fixture();
        apply(&snapshot, &events, MachineEvent::GrblLine("hello".to_string()));

        let snap = snapshot.read().unwrap();
        assert!(snap.status.is_none());
        assert!(snap.action_policy.is_none());
        assert!(snap.job_progress.is_none());
        assert!(snap.maslow_info.is_none());
        assert!(snap.maslow_state.is_none());
        assert!(snap.anchors.is_none());
        assert!(snap.config_entries.is_none());
        assert_eq!(snap.ws_state, WsState::Disconnected);
    }

    #[test]
    fn publish_broadcasts_to_subscriber() {
        let (snapshot, events) = fixture();
        let mut rx = events.subscribe();

        apply(&snapshot, &events, MachineEvent::GrblLine("hello".to_string()));

        match rx.try_recv() {
            Ok(MachineEvent::GrblLine(line)) => assert_eq!(line, "hello"),
            other => panic!("expected GrblLine broadcast, got {other:?}"),
        }
    }

    #[test]
    fn publish_broadcasts_machine_status() {
        let (snapshot, events) = fixture();
        let mut rx = events.subscribe();
        let status = MachineStatus {
            state: "Run".to_string(),
            ..Default::default()
        };

        apply(&snapshot, &events, MachineEvent::MachineStatus(status));

        match rx.try_recv() {
            Ok(MachineEvent::MachineStatus(s)) => assert_eq!(s.state, "Run"),
            other => panic!("expected MachineStatus broadcast, got {other:?}"),
        }
    }
}
