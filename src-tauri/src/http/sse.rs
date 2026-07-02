// Shared helper for turning a `Stream` of proto messages into an axum SSE
// response. Used by the `:watch` handlers in `http/machine.rs` and
// `http/job.rs`, which build the underlying stream from
// `grpc::stream::machine_event_stream`/`job_progress_stream` (the same
// adapters the gRPC watch RPCs use) and just need it wrapped for SSE here.

use axum::response::sse::{Event, Sse};
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use std::convert::Infallible;

/// Wraps a stream of serializable items as an SSE response, one JSON-encoded
/// `data:` event per item. Matches the JSON shape the rest of the HTTP
/// gateway already uses (`pb` types serialize via pbjson).
///
/// An item that fails to serialize is logged and skipped rather than ending
/// the stream, so the `Sse<...>` return type can honestly promise
/// `Result<Event, Infallible>`: this function never actually produces the
/// `Err` case.
pub fn json_event_stream<S, T>(stream: S) -> Sse<impl Stream<Item = Result<Event, Infallible>>>
where
    S: Stream<Item = T> + Send + 'static,
    T: Serialize + Send + 'static,
{
    let events = stream.filter_map(|item| async move {
        match Event::default().json_data(item) {
            Ok(event) => Some(Ok(event)),
            Err(e) => {
                eprintln!("failed to serialize SSE event: {e}");
                None
            }
        }
    });
    Sse::new(events)
}
