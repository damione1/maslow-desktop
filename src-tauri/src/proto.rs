//! Generated protobuf/gRPC types, built by `build.rs` from `proto/maslow/v1/*.proto`.
//!
//! Nothing in this crate implements the generated service traits yet; this
//! module only makes the generated request/response/event types available
//! for later work to build on.

pub mod maslow {
    pub mod v1 {
        include!("generated/maslow.v1.rs");
        include!("generated/maslow.v1.serde.rs");
    }
}

#[cfg(test)]
mod tests {
    use super::maslow::v1::{JogRequest, ZeroRequest};

    #[test]
    fn generated_request_types_are_constructible() {
        let jog = JogRequest {
            dx: 1.0,
            dy: 2.0,
            dz: 0.0,
            feed: 500.0,
        };
        assert_eq!(jog.dx, 1.0);
        assert_eq!(jog.dy, 2.0);
        assert_eq!(jog.dz, 0.0);
        assert_eq!(jog.feed, 500.0);

        let zero = ZeroRequest {
            axes: vec!["X".to_string(), "Y".to_string()],
        };
        assert_eq!(zero.axes, vec!["X".to_string(), "Y".to_string()]);
    }
}
