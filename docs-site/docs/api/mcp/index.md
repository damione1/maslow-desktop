---
id: index
title: MCP Tools
sidebar_position: 1
slug: /api/mcp
---

A [Model Context Protocol](https://modelcontextprotocol.io/) server, so an LLM client can call the machine's
control operations as tools. Built on the official `rmcp` SDK's Streamable HTTP transport, mounted at `/mcp` on
the same HTTP gateway (`8642` by default) and gated behind the same `Authorization: Bearer <key>` check as every
other route: see [Using the API](../using-the-api.md).

**Every tool listed here is a thin wrapper over the identical operation the [HTTP](../http/machine.md) and
[gRPC](../grpc/machine.md) APIs expose**: same underlying `MaslowService` method, same effect on the machine, just
a different transport picked for LLM tool-calling convenience. Each tool's page cross-references the matching HTTP
route and gRPC method.

**These per-domain pages are generated** by querying the same `rmcp::ToolRouter` the running MCP server builds
(`McpServer::tool_router_*` in `src-tauri/src/mcp/`) for its registered tool list, name, description, and
parameter JSON schema: this is the live registry a real MCP client would see, not a separately maintained list.

- [Machine](./machine.md)
- [Job](./job.md)
- [Config](./config.md)
- [Files](./files.md)
- [Calibration](./calibration.md)

:::caution Real hardware
Tool descriptions call out when an action physically moves the machine. Check `get_action_policy` or
`get_snapshot` first to confirm an action is currently allowed before calling it, the same way the MCP server's
own instructions tell a connecting client.
:::
