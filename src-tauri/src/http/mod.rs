// HTTP/JSON transport adapter: a second, parallel adapter over the same
// `MaslowService` (and free functions) the gRPC layer in `grpc/` calls. Not a
// proxy in front of gRPC: both transports call the identical Rust methods
// directly, avoiding a network round trip to itself and a "gateway up,
// backend down" failure mode that cannot otherwise happen in a single
// process. One `axum::Router` per service, merged into a single app here,
// following the AIP-136 custom-method convention (a ":verb" suffix on the
// resource path) for actions that are not plain CRUD.

pub mod calibration;
pub mod config;
pub mod error;
pub mod files;
pub mod job;
pub mod machine;

use crate::http::error::ApiError;
use crate::service::machine::MaslowService;
use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use std::future::Future;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

/// Rejects any request without a valid `Authorization: Bearer <key>` header,
/// checked against `svc.api_settings` live on every request (not a value
/// captured once at server-spawn time), so a key regenerated or a toggle
/// flipped while the server is running takes effect on the very next
/// request.
async fn auth_middleware(State(svc): State<Arc<MaslowService>>, req: Request, next: Next) -> Response {
    let key = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let authorized = key.is_some_and(|k| {
        let settings = svc.api_settings.read().unwrap();
        crate::auth::check_key(&settings, k)
    });
    if authorized {
        next.run(req).await
    } else {
        ApiError::unauthorized("missing or invalid API key").into_response()
    }
}

fn build_router(svc: Arc<MaslowService>) -> Router {
    Router::new()
        .merge(machine::router())
        .merge(job::router())
        .merge(config::router())
        .merge(files::router())
        .merge(calibration::router())
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(svc.clone(), auth_middleware))
        .with_state(svc)
}

/// Start the HTTP gateway on a background Tauri task, listening on
/// `svc.api_settings`'s current `port_http` (loopback only). Shuts down
/// gracefully when `shutdown` resolves, which lets a settings change
/// (enable/disable, key regeneration) restart the server on demand rather
/// than only at app exit.
pub fn spawn_server(svc: Arc<MaslowService>, shutdown: impl Future<Output = ()> + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        let port = svc.api_settings.read().unwrap().port_http;
        let app = build_router(svc);
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => listener,
            Err(e) => {
                eprintln!("HTTP gateway failed to bind 127.0.0.1:{port}: {e}");
                return;
            }
        };
        eprintln!("HTTP gateway listening on 127.0.0.1:{port}");
        if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(shutdown).await {
            eprintln!("HTTP gateway exited with error: {e}");
        }
    });
}

// Routing-layer tests.
//
// The production routers above are typed as `Router<Arc<MaslowService>>`,
// and exercising them end to end needs an actual `Arc<MaslowService>` value.
// `MaslowService` holds a live `tauri::AppHandle`, which defaults to the
// `Wry` runtime; unlike Tauri's `MockRuntime` (a different `Runtime` type),
// there is no way to obtain a `Wry` handle without a real windowing system,
// so it cannot be constructed in this crate's unit tests (see the same
// limitation noted in `service/snapshot.rs`).
//
// These tests instead build small stand-in routers over unit state (`()`),
// using the exact URL patterns the production routers register and the real
// `pb` request/response types, to exercise axum's actual routing/extraction
// machinery (`Router::oneshot`, `Query`, `Path`, `Json`) for a representative
// sample of the route shapes: a no-body action, a body-carrying action, a
// query-param endpoint, and two wildcard-path endpoints. This proves the
// route table and JSON (de)serialization are wired correctly; it does not
// exercise `MaslowService` behavior, which is already covered elsewhere.
#[cfg(test)]
mod tests {
    use crate::proto::maslow::v1 as pb;
    use axum::extract::{Path, Query};
    use axum::http::{Request, StatusCode};
    use axum::routing::{delete, get, post};
    use axum::{body::Body, Json, Router};
    use serde::Deserialize;
    use tower::ServiceExt;

    async fn send(app: Router, method: &str, uri: &str, body: Option<serde_json::Value>) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(v) => {
                builder = builder.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&v).unwrap())
            }
            None => Body::empty(),
        };
        let response = app.oneshot(builder.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, json)
    }

    /// No-body action, matching "/v1/machines/default:home".
    #[tokio::test]
    async fn routes_no_body_action() {
        async fn home() -> Json<pb::HomeResponse> {
            Json(pb::HomeResponse {})
        }
        let app = Router::new().route("/v1/machines/default:home", post(home));
        let (status, _) = send(app, "POST", "/v1/machines/default:home", None).await;
        assert_eq!(status, StatusCode::OK);
    }

    /// Body-carrying action, matching "/v1/machines/default:jog". Echoes the
    /// request back so the test can confirm the JSON round-trips through
    /// axum's `Json` extractor and response encoding unchanged.
    #[tokio::test]
    async fn routes_body_carrying_action_and_round_trips_json() {
        async fn jog(Json(req): Json<pb::JogRequest>) -> Json<pb::JogRequest> {
            Json(req)
        }
        let app = Router::new().route("/v1/machines/default:jog", post(jog));
        // dz is intentionally non-zero: pbjson's proto3 JSON mapping omits
        // default-valued (0.0) fields on serialization, so a zero dz would
        // round-trip as an absent key rather than an explicit 0.0.
        let sent = serde_json::json!({"dx": 12.5, "dy": -3.0, "dz": 4.0, "feed": 600.0});
        let (status, got) = send(app, "POST", "/v1/machines/default:jog", Some(sent.clone())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, sent);
    }

    /// Query-param endpoint, matching "/v1/configEntries?forceRefresh=true".
    /// `forceRefresh` uses the pbjson camelCase convention, same as the real
    /// `config::ListConfigQuery`.
    #[tokio::test]
    async fn routes_query_param_endpoint() {
        #[derive(Deserialize)]
        struct ListQuery {
            #[serde(default, rename = "forceRefresh")]
            force_refresh: bool,
        }
        async fn list_config_entries(Query(q): Query<ListQuery>) -> Json<bool> {
            Json(q.force_refresh)
        }
        let app = Router::new().route("/v1/configEntries", get(list_config_entries));

        let (status, got) = send(app.clone(), "GET", "/v1/configEntries?forceRefresh=true", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, serde_json::json!(true));

        let (status, got) = send(app, "GET", "/v1/configEntries", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, serde_json::json!(false));
    }

    /// Wildcard-path endpoint, matching "/v1/configEntries/*path": the path
    /// param must capture every remaining segment, joined by '/', not just
    /// one.
    #[tokio::test]
    async fn routes_multi_segment_wildcard_path() {
        async fn get_config_entry(Path(path): Path<String>) -> Json<String> {
            Json(path)
        }
        let app = Router::new().route("/v1/configEntries/*path", get(get_config_entry));
        let (status, got) = send(app, "GET", "/v1/configEntries/axes/x/steps_per_mm", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, serde_json::json!("axes/x/steps_per_mm"));
    }

    /// Second wildcard-path endpoint, matching "/v1/files/*path", exercised
    /// with DELETE rather than GET to confirm the pattern also works for a
    /// non-GET method.
    #[tokio::test]
    async fn routes_wildcard_path_delete() {
        async fn delete_file(Path(path): Path<String>) -> Json<String> {
            Json(path)
        }
        let app = Router::new().route("/v1/files/*path", delete(delete_file));
        let (status, got) = send(app, "DELETE", "/v1/files/sub/job.nc", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, serde_json::json!("sub/job.nc"));
    }
}
