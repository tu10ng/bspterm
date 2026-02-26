# remote

Client-side remote connection infrastructure with multi-transport support (SSH, WSL, Docker) and state management.

## Module Structure

```
src/
├── remote.rs           # Library root and exports
├── remote_client.rs    # Core remote client entity (1,707 lines)
├── protocol.rs         # Length-prefixed protobuf framing
├── json_log.rs         # Structured JSON logging
├── proxy.rs            # Proxy launch error handling
└── transport/
    ├── mod.rs          # Transport abstraction
    ├── ssh.rs          # SSH/SFTP connection (1,943 lines)
    ├── docker.rs       # Docker/Podman containers
    ├── wsl.rs          # Windows Subsystem for Linux
    └── mock.rs         # Test mock connection
```

## Key Types

| Type | Purpose |
|------|---------|
| `RemoteClient` | Main entity managing connection lifecycle |
| `RemoteConnection` | Trait for transport implementations |
| `RemoteConnectionOptions` | Ssh/Wsl/Docker/Mock variants |
| `ConnectionState` | Connecting/Connected/HeartbeatMissed/Reconnecting/Disconnected |
| `RemoteClientDelegate` | Callbacks for password prompts, status updates |
| `RemotePlatform` | OS + Architecture pair |
| `RemoteOs` / `RemoteArch` | Platform detection enums |
| `SshConnectionOptions` | SSH connection parameters |
| `CommandTemplate` | Remote command specification |

## Dependencies

- `gpui` - Entity management, async context
- `rpc` - Message routing
- `settings` - Configuration access
- Platform-specific SSH libraries

## Common Tasks

**Add a new transport type:**
1. Create module in `transport/`
2. Implement `RemoteConnection` trait
3. Add variant to `RemoteConnectionOptions`

**Modify connection state machine:**
1. Update states in `remote_client.rs`
2. Adjust heartbeat/reconnection logic

## Testing

```sh
cargo test -p remote
```

## Pitfalls

- State machine: Connecting → Connected → HeartbeatMissed → Reconnecting → ReconnectFailed
- Heartbeat: 5-second interval, 5-second timeout, max 5 missed before reconnect
- Max 3 reconnection attempts before giving up
- Protocol uses length-prefixed protobuf messages
- Exit code 90 indicates "server not running" (ProxyLaunchError)
- WSL path translation required between Windows and WSL paths
- SSH supports IPv4, IPv6, and hostname connection hosts
