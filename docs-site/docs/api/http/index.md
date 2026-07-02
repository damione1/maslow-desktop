---
id: index
title: HTTP API
sidebar_position: 1
slug: /api/http
---

A REST-ish JSON gateway over the same `MaslowService` operations gRPC exposes, mounted on its own port (`8642` by
default) alongside the MCP server (`/mcp`). See [Using the API](../using-the-api.md) for authentication.

Routes follow an AIP-136-style convention: plain resource paths for reads (`GET /v1/machines/default`), and a
`:verb` suffix for actions that aren't CRUD (`POST /v1/machines/default:jog`). Streaming endpoints are exposed as
Server-Sent Events rather than the gRPC streaming RPC.

**These per-domain pages are generated** from the real axum route registrations in `src-tauri/src/http/*.rs` (see
each page's banner for exactly how): the method, path, and request/response type columns are read straight out of
that source, not maintained separately.

- [Machine](./machine.md)
- [Job](./job.md)
- [Config](./config.md)
- [Files](./files.md)
- [Calibration](./calibration.md)

Every request/response body is the pbjson JSON mapping of the identically named proto message from
`proto/maslow/v1/*.proto`; see the [gRPC reference](../grpc/machine.md) for field-level detail on those messages.
