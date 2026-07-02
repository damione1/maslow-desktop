// Conversions between the domain types used by the rest of the app and the
// generated protobuf/gRPC message types. Kept in one place so every service
// implementation shares the same mapping and field-width casts.

use crate::calibration;
use crate::connection;
use crate::fluidnc;
use crate::grbl;
use crate::maslow;
use crate::proto::maslow::v1 as pb;
use crate::service::snapshot;
use crate::streaming;
use crate::toolpath;

impl From<grbl::MachineStatus> for pb::MachineStatus {
    fn from(s: grbl::MachineStatus) -> Self {
        pb::MachineStatus {
            state: s.state,
            substate: s.substate,
            mpos: s.mpos,
            wpos: s.wpos,
            wco: s.wco,
            feed: s.feed,
            spindle: s.spindle,
            buffer_blocks: s.buffer_blocks,
            buffer_bytes: s.buffer_bytes,
            ov: s.ov,
        }
    }
}

impl From<maslow::ActionPolicy> for pb::ActionPolicy {
    fn from(p: maslow::ActionPolicy) -> Self {
        pb::ActionPolicy {
            jog: p.jog,
            home: p.home,
            unlock: p.unlock,
            zero: p.zero,
            run: p.run,
            hold: p.hold,
            resume: p.resume,
            reset: p.reset,
            retract: p.retract,
            extend: p.extend,
            take_slack: p.take_slack,
            calibrate: p.calibrate,
            comply: p.comply,
            stop: p.stop,
            estop: p.estop,
        }
    }
}

impl From<maslow::MaslowInfo> for pb::MaslowInfo {
    fn from(i: maslow::MaslowInfo) -> Self {
        pb::MaslowInfo {
            homed: i.homed,
            calibration_in_progress: i.calibration_in_progress,
            tl: i.tl,
            tr: i.tr,
            br: i.br,
            bl: i.bl,
            etl: i.etl,
            etr: i.etr,
            ebr: i.ebr,
            ebl: i.ebl,
            extended: i.extended,
        }
    }
}

impl From<maslow::Anchors> for pb::Anchors {
    fn from(a: maslow::Anchors) -> Self {
        pb::Anchors {
            tl_x: a.tl_x,
            tl_y: a.tl_y,
            tr_x: a.tr_x,
            tr_y: a.tr_y,
            bl_x: a.bl_x,
            bl_y: a.bl_y,
            br_x: a.br_x,
            br_y: a.br_y,
            valid: a.valid,
            calibrated: a.calibrated,
        }
    }
}

impl From<pb::Anchors> for maslow::Anchors {
    fn from(a: pb::Anchors) -> Self {
        maslow::Anchors {
            tl_x: a.tl_x,
            tl_y: a.tl_y,
            tr_x: a.tr_x,
            tr_y: a.tr_y,
            bl_x: a.bl_x,
            bl_y: a.bl_y,
            br_x: a.br_x,
            br_y: a.br_y,
            valid: a.valid,
            calibrated: a.calibrated,
        }
    }
}

impl From<maslow::StatePolicy> for pb::StatePolicy {
    fn from(s: maslow::StatePolicy) -> Self {
        pb::StatePolicy {
            code: s.code,
            label: s.label,
            busy: s.busy,
        }
    }
}

impl From<maslow::Waypoint> for pb::Waypoint {
    fn from(w: maslow::Waypoint) -> Self {
        pb::Waypoint {
            n: w.n as i32,
            x: w.x,
            y: w.y,
        }
    }
}

impl From<calibration::Measurement> for pb::Measurement {
    fn from(m: calibration::Measurement) -> Self {
        pb::Measurement {
            tl: m.tl,
            tr: m.tr,
            bl: m.bl,
            br: m.br,
        }
    }
}

impl From<pb::Measurement> for calibration::Measurement {
    fn from(m: pb::Measurement) -> Self {
        calibration::Measurement {
            tl: m.tl,
            tr: m.tr,
            bl: m.bl,
            br: m.br,
        }
    }
}

impl From<calibration::AnchorParams> for pb::AnchorParams {
    fn from(p: calibration::AnchorParams) -> Self {
        pb::AnchorParams {
            tl_x: p.tl_x,
            tl_y: p.tl_y,
            tr_x: p.tr_x,
            tr_y: p.tr_y,
            br_x: p.br_x,
        }
    }
}

impl From<calibration::Sled> for pb::Sled {
    fn from(s: calibration::Sled) -> Self {
        pb::Sled { x: s.x, y: s.y }
    }
}

impl From<calibration::Fitness> for pb::Fitness {
    fn from(f: calibration::Fitness) -> Self {
        pb::Fitness {
            rms: f.rms,
            max_residual: f.max_residual,
            per_anchor: f.per_anchor.to_vec(),
            converged: f.converged,
        }
    }
}

impl From<calibration::FirmwareFit> for pb::FirmwareFit {
    fn from(f: calibration::FirmwareFit) -> Self {
        pb::FirmwareFit {
            rms: f.rms,
            max_residual: f.max_residual,
            per_anchor: f.per_anchor.to_vec(),
            converged: f.converged,
        }
    }
}

impl From<calibration::SolveResult> for pb::SolveResult {
    fn from(r: calibration::SolveResult) -> Self {
        pb::SolveResult {
            solver: r.solver,
            ok: r.ok,
            // proto3 has no optional-string distinction here: an absent
            // gate_error becomes an empty string, which is an accepted
            // simplification (empty is never a real gate error message).
            gate_error: r.gate_error.unwrap_or_default(),
            anchors: Some(r.anchors.into()),
            params: Some(r.params.into()),
            fitness: Some(r.fitness.into()),
            sled: r.sled.into_iter().map(Into::into).collect(),
            residuals: r
                .residuals
                .into_iter()
                .map(|row| pb::ResidualRow { values: row.to_vec() })
                .collect(),
            kept_indices: r.kept_indices.into_iter().map(|i| i as u32).collect(),
        }
    }
}

impl From<fluidnc::ConfigKind> for pb::ConfigKind {
    fn from(k: fluidnc::ConfigKind) -> Self {
        match k {
            fluidnc::ConfigKind::Bool => pb::ConfigKind::Bool,
            fluidnc::ConfigKind::Int => pb::ConfigKind::Int,
            fluidnc::ConfigKind::Float => pb::ConfigKind::Float,
            fluidnc::ConfigKind::Text => pb::ConfigKind::Text,
        }
    }
}

impl From<fluidnc::ConfigEntry> for pb::ConfigEntry {
    fn from(e: fluidnc::ConfigEntry) -> Self {
        pb::ConfigEntry {
            path: e.path,
            value: e.value,
            kind: pb::ConfigKind::from(e.kind) as i32,
        }
    }
}

impl From<streaming::Progress> for pb::Job {
    fn from(p: streaming::Progress) -> Self {
        pb::Job {
            state: p.state,
            path: p.path,
            name: p.name,
            sent: p.sent as u64,
            acked: p.acked as u64,
            total: p.total as u64,
            errors: p.errors as u64,
        }
    }
}

impl From<streaming::SavedJob> for pb::SavedJob {
    fn from(s: streaming::SavedJob) -> Self {
        pb::SavedJob {
            path: s.path,
            name: s.name,
            total: s.total as u64,
            acked: s.acked as u64,
            state: s.state,
            updated_at: s.updated_at,
        }
    }
}

impl From<toolpath::Segment> for pb::Segment {
    fn from(s: toolpath::Segment) -> Self {
        pb::Segment {
            x0: s.x0,
            y0: s.y0,
            x1: s.x1,
            y1: s.y1,
            rapid: s.rapid,
            line: s.line as u32,
        }
    }
}

impl From<toolpath::Toolpath> for pb::Toolpath {
    fn from(t: toolpath::Toolpath) -> Self {
        pb::Toolpath {
            segments: t.segments.into_iter().map(Into::into).collect(),
            min_x: t.min_x,
            min_y: t.min_y,
            max_x: t.max_x,
            max_y: t.max_y,
            has_bounds: t.has_bounds,
        }
    }
}

impl From<snapshot::WsState> for pb::WsState {
    fn from(s: snapshot::WsState) -> Self {
        match s {
            snapshot::WsState::Disconnected => pb::WsState::Disconnected,
            snapshot::WsState::Connected => pb::WsState::Connected,
        }
    }
}

impl From<connection::Discord> for pb::Discord {
    fn from(d: connection::Discord) -> Self {
        pb::Discord {
            kind: d.kind.to_string(),
            from: d.from,
            to: d.to,
            from_label: d.from_label,
            to_label: d.to_label,
        }
    }
}

/// Converts one domain `MachineEvent` into the `oneof`-wrapped proto message
/// that `WatchMachineEvents` (gRPC) and its HTTP SSE equivalent stream.
///
/// `JobProgress` has no case in the `MachineEvent` oneof by design (see
/// `machine.proto`'s comment on the message): job progress is high-frequency
/// and has its own `WatchJobProgress` stream, so a telemetry-only watcher
/// should not also receive it. `grpc::stream::machine_event_stream` filters
/// it out before it reaches a subscriber; this arm exists only to keep the
/// match exhaustive.
impl From<snapshot::MachineEvent> for pb::MachineEvent {
    fn from(event: snapshot::MachineEvent) -> Self {
        use pb::machine_event::Payload;
        let payload = match event {
            snapshot::MachineEvent::WsState(s) => Some(Payload::WsState(pb::WsState::from(s) as i32)),
            snapshot::MachineEvent::WsError(e) => Some(Payload::WsError(e)),
            snapshot::MachineEvent::GrblLine(l) => Some(Payload::GrblLine(l)),
            snapshot::MachineEvent::MachineStatus(s) => Some(Payload::Status(s.into())),
            snapshot::MachineEvent::WsControl(c) => Some(Payload::WsControl(c)),
            snapshot::MachineEvent::ActionPolicy(p) => Some(Payload::ActionPolicy(p.into())),
            snapshot::MachineEvent::Anchors(a) => Some(Payload::Anchors(a.into())),
            snapshot::MachineEvent::MaslowInfo(i) => Some(Payload::MaslowInfo(i.into())),
            snapshot::MachineEvent::MaslowState(s) => Some(Payload::MaslowState(s.into())),
            snapshot::MachineEvent::Waypoint(w) => Some(Payload::Waypoint(w.into())),
            snapshot::MachineEvent::Discord(d) => Some(Payload::Discord(d.into())),
            snapshot::MachineEvent::CalMeasurements(measurements) => Some(Payload::CalMeasurements(pb::MeasurementList {
                measurements: measurements.into_iter().map(Into::into).collect(),
            })),
            snapshot::MachineEvent::CalFirmwareFit(f) => Some(Payload::CalFirmwareFit(f.into())),
            snapshot::MachineEvent::CalFirmwareAnchors(p) => Some(Payload::CalFirmwareAnchors(p.into())),
            snapshot::MachineEvent::CalComplete => Some(Payload::CalComplete(true)),
            snapshot::MachineEvent::ConfigDump(entries) => Some(Payload::ConfigDump(pb::ConfigEntryList {
                entries: entries.into_iter().map(Into::into).collect(),
            })),
            snapshot::MachineEvent::ConfigDumpError(e) => Some(Payload::ConfigDumpError(e)),
            snapshot::MachineEvent::JobProgress(_) => None,
        };
        pb::MachineEvent { payload }
    }
}

/// Build the `Snapshot` aggregate response from the cached telemetry. Fields
/// with nothing observed yet on the current connection are simply left unset.
pub fn snapshot_to_proto(snapshot: &snapshot::MachineSnapshot) -> pb::Snapshot {
    pb::Snapshot {
        ws_state: pb::WsState::from(snapshot.ws_state.clone()) as i32,
        status: snapshot.status.clone().map(Into::into),
        action_policy: snapshot.action_policy.clone().map(Into::into),
        maslow_info: snapshot.maslow_info.clone().map(Into::into),
        maslow_state: snapshot.maslow_state.clone().map(Into::into),
        anchors: snapshot.anchors.clone().map(Into::into),
        config_entries: snapshot
            .config_entries
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_result_wraps_residuals_and_maps_gate_error() {
        let domain = calibration::SolveResult {
            solver: "levenberg-marquardt".to_string(),
            ok: false,
            anchors: maslow::Anchors::default(),
            params: calibration::AnchorParams {
                tl_x: 1.0,
                tl_y: 2.0,
                tr_x: 3.0,
                tr_y: 4.0,
                br_x: 5.0,
            },
            fitness: calibration::Fitness {
                rms: 0.5,
                max_residual: 1.5,
                per_anchor: [0.1, 0.2, 0.3, 0.4],
                converged: true,
            },
            sled: vec![calibration::Sled { x: 10.0, y: 20.0 }],
            residuals: vec![[1.0, 2.0, 3.0, 4.0]],
            kept_indices: vec![0, 2, 5],
            gate_error: Some("rms too high".to_string()),
        };

        let proto: pb::SolveResult = domain.into();

        assert_eq!(proto.gate_error, "rms too high");
        assert_eq!(proto.residuals.len(), 1);
        assert_eq!(proto.residuals[0].values, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(proto.kept_indices, vec![0, 2, 5]);
        assert_eq!(proto.params.unwrap().tr_y, 4.0);
    }

    #[test]
    fn solve_result_missing_gate_error_becomes_empty_string() {
        let domain = calibration::SolveResult {
            solver: "levenberg-marquardt".to_string(),
            ok: true,
            anchors: maslow::Anchors::default(),
            params: calibration::AnchorParams {
                tl_x: 0.0,
                tl_y: 0.0,
                tr_x: 0.0,
                tr_y: 0.0,
                br_x: 0.0,
            },
            fitness: calibration::Fitness {
                rms: 0.0,
                max_residual: 0.0,
                per_anchor: [0.0; 4],
                converged: true,
            },
            sled: Vec::new(),
            residuals: Vec::new(),
            kept_indices: Vec::new(),
            gate_error: None,
        };

        let proto: pb::SolveResult = domain.into();
        assert_eq!(proto.gate_error, "");
    }

    #[test]
    fn fitness_and_firmware_fit_convert_array_to_vec() {
        let fitness = calibration::Fitness {
            rms: 1.0,
            max_residual: 2.0,
            per_anchor: [1.0, 2.0, 3.0, 4.0],
            converged: false,
        };
        let proto: pb::Fitness = fitness.into();
        assert_eq!(proto.per_anchor, vec![1.0, 2.0, 3.0, 4.0]);

        let firmware_fit = calibration::FirmwareFit {
            rms: 5.0,
            max_residual: 6.0,
            per_anchor: [5.0, 6.0, 7.0, 8.0],
            converged: true,
        };
        let proto: pb::FirmwareFit = firmware_fit.into();
        assert_eq!(proto.per_anchor, vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn config_entry_maps_kind_enum() {
        let entry = fluidnc::ConfigEntry {
            path: "axes/x/steps_per_mm".to_string(),
            value: "100".to_string(),
            kind: fluidnc::ConfigKind::Int,
        };
        let proto: pb::ConfigEntry = entry.into();
        assert_eq!(proto.path, "axes/x/steps_per_mm");
        assert_eq!(proto.kind, pb::ConfigKind::Int as i32);
    }

    #[test]
    fn snapshot_to_proto_leaves_unset_fields_absent() {
        let snap = snapshot::MachineSnapshot::default();
        let proto = snapshot_to_proto(&snap);
        assert!(proto.status.is_none());
        assert!(proto.action_policy.is_none());
        assert!(proto.anchors.is_none());
        assert!(proto.config_entries.is_empty());
        assert_eq!(proto.ws_state, pb::WsState::Disconnected as i32);
    }

    #[test]
    fn snapshot_to_proto_carries_observed_fields() {
        let snap = snapshot::MachineSnapshot {
            status: Some(grbl::MachineStatus {
                state: "Idle".to_string(),
                ..Default::default()
            }),
            config_entries: Some(vec![fluidnc::ConfigEntry {
                path: "board".to_string(),
                value: "Maslow".to_string(),
                kind: fluidnc::ConfigKind::Text,
            }]),
            ws_state: snapshot::WsState::Connected,
            ..Default::default()
        };

        let proto = snapshot_to_proto(&snap);
        assert_eq!(proto.status.unwrap().state, "Idle");
        assert_eq!(proto.config_entries.len(), 1);
        assert_eq!(proto.ws_state, pb::WsState::Connected as i32);
    }

    #[test]
    fn anchors_round_trip_between_domain_and_proto() {
        let domain = maslow::Anchors {
            tl_x: -27.6,
            tl_y: 2064.9,
            tr_x: 2924.3,
            tr_y: 2066.5,
            bl_x: 0.0,
            bl_y: 0.0,
            br_x: 2953.2,
            br_y: 0.0,
            valid: true,
            calibrated: false,
        };
        let proto: pb::Anchors = domain.clone().into();
        let back: maslow::Anchors = proto.into();
        assert_eq!(domain, back);
    }

    #[test]
    fn machine_event_maps_plain_string_variant() {
        let proto: pb::MachineEvent = snapshot::MachineEvent::GrblLine("ok".to_string()).into();
        match proto.payload {
            Some(pb::machine_event::Payload::GrblLine(line)) => assert_eq!(line, "ok"),
            other => panic!("expected GrblLine payload, got {other:?}"),
        }
    }

    #[test]
    fn machine_event_wraps_measurements_in_measurement_list() {
        let measurements = vec![calibration::Measurement {
            tl: 1.0,
            tr: 2.0,
            bl: 3.0,
            br: 4.0,
        }];
        let proto: pb::MachineEvent = snapshot::MachineEvent::CalMeasurements(measurements).into();
        match proto.payload {
            Some(pb::machine_event::Payload::CalMeasurements(list)) => {
                assert_eq!(list.measurements.len(), 1);
                assert_eq!(list.measurements[0].tr, 2.0);
            }
            other => panic!("expected CalMeasurements payload, got {other:?}"),
        }
    }

    #[test]
    fn machine_event_cal_complete_maps_to_bool_payload() {
        let proto: pb::MachineEvent = snapshot::MachineEvent::CalComplete.into();
        assert!(matches!(proto.payload, Some(pb::machine_event::Payload::CalComplete(true))));
    }

    #[test]
    fn machine_event_job_progress_has_no_oneof_case() {
        let progress = streaming::Progress {
            state: "running".to_string(),
            path: "/x.nc".to_string(),
            name: "x.nc".to_string(),
            sent: 1,
            acked: 0,
            total: 10,
            errors: 0,
        };
        let proto: pb::MachineEvent = snapshot::MachineEvent::JobProgress(progress).into();
        assert!(proto.payload.is_none());
    }

    #[test]
    fn discord_conversion_carries_all_fields() {
        let domain = connection::Discord {
            kind: "straggler",
            from: 2,
            to: 4,
            from_label: "Calibrating".to_string(),
            to_label: "Complete".to_string(),
        };
        let proto: pb::Discord = domain.into();
        assert_eq!(proto.kind, "straggler");
        assert_eq!(proto.from, 2);
        assert_eq!(proto.to, 4);
        assert_eq!(proto.from_label, "Calibrating");
        assert_eq!(proto.to_label, "Complete");
    }
}
