---
id: using-the-api
title: Using the API
sidebar_position: 1
---

Everything the app's own UI can do is also reachable over three transports, all backed by the exact same
`MaslowService` Rust methods:

- [gRPC](./grpc/machine.md) - `MachineService`, `JobService`, `ConfigService`, `FilesService`, `CalibrationService`,
  defined in `proto/maslow/v1/*.proto`.
- [HTTP/JSON](./http/machine.md) - a REST-ish gateway over the same operations, following an AIP-136-style
  convention (a `:verb` suffix on the resource path for actions that aren't plain CRUD, e.g.
  `POST /v1/machines/default:jog`).
- [MCP](./mcp/machine.md) - a Model Context Protocol server so an LLM can call the same operations as tools,
  mounted at `/mcp` on the HTTP gateway.

None of the three are reachable until you enable the API and generate a key.

## Enabling the API

Both servers are off by default. In the app's **Config &rarr; Settings** tab:

1. Toggle the API **on**.
2. Click **Generate key** (or **Regenerate key**) to get a plaintext API key. This is the only time the plaintext
   key is ever shown: only its SHA-256 hash is persisted to disk, so store the key somewhere safe.
3. Note the ports shown: HTTP defaults to `8642`, gRPC to `50051`. Both servers bind to `127.0.0.1` only (loopback),
   not to your LAN interface.

Toggling the API or regenerating the key takes effect immediately: the running servers are restarted in place, no
app restart required.

## Authenticating requests

Every request on every transport needs the same bearer token:

```
Authorization: Bearer <your-api-key>
```

- **HTTP**: an `Authorization: Bearer <key>` header, checked by an axum middleware in front of every route
  (including `/mcp`).
- **gRPC**: an `authorization` metadata entry of the form `Bearer <key>`, checked by a per-service tonic
  interceptor.
- **MCP**: MCP requests ride the same axum router as the HTTP gateway (mounted at `/mcp`), so they go through the
  identical `Authorization: Bearer <key>` header check; there is no separate MCP-specific auth path.

A request with a missing or invalid key gets `401 Unauthorized` (HTTP) or `UNAUTHENTICATED` (gRPC). The check is
against the current live settings on every request, not a value cached at server startup, so a key you just
regenerated (or an API you just disabled) takes effect on the very next request.

## Example

```bash
curl -H "Authorization: Bearer $MASLOW_API_KEY" http://127.0.0.1:8642/v1/machines/default:snapshot
```

See the [HTTP](./http/machine.md), [gRPC](./grpc/machine.md), and [MCP](./mcp/machine.md) reference pages for the
full set of operations, request/response shapes, and (for MCP) tool parameter schemas.
