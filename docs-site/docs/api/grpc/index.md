---
id: index
title: gRPC API
sidebar_position: 1
slug: /api/grpc
---

Five services, one per `proto/maslow/v1/*.proto` file with a `service` declaration (`common.proto` defines only
shared message types, pulled in transitively wherever they're referenced). Listens on `50051` by default. See
[Using the API](../using-the-api.md) for authentication.

**These per-service pages are generated** by reading the compiled `FileDescriptorSet`
(`src-tauri/src/generated/maslow_descriptor.bin`, produced by `build.rs` from the `.proto` files) with the
`prost-reflect` crate: every method, message, field, and enum listed is read from the actual compiled schema, and
the `.proto` files' own doc comments come along for the ride (`prost_build` retains `SourceCodeInfo` by default).

- [MachineService](./machine.md)
- [JobService](./job.md)
- [ConfigService](./config.md)
- [FilesService](./files.md)
- [CalibrationService](./calibration.md)

Each page's "Types" section covers every message and enum reachable from that service's RPCs, including shared
types defined in `common.proto` (`MachineStatus`, `Anchors`, `ActionPolicy`, and so on).
