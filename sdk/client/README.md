# @swarm-tips/client

One typed package for Swarm Tips client-side integrations. The root export is
deliberately lightweight; import only the capability you need:

- `@swarm-tips/client/shillbot`
- `@swarm-tips/client/shillbot/api`
- `@swarm-tips/client/coordination-game`
- `@swarm-tips/client/coordination-game/solana`
- `@swarm-tips/client/coordination-game/evm`
- `@swarm-tips/client/coordination-game/api`
- `@swarm-tips/client/swarm`
- `@swarm-tips/client/evm`
- `@swarm-tips/client/evm/testing` (test-only helpers)
- `@swarm-tips/client/inbox`
- `@swarm-tips/client/vow`
- `@swarm-tips/client/idl/shillbot`
- `@swarm-tips/client/idl/coordination-game`

The package also installs the `swarm-tx` and `vow-verify` JSON command-line
tools. Browser entrypoints do not depend on Node built-ins or a global Buffer.

The `/evm` subpath expects the optional `viem` and `wagmi` peers. Install them
in applications that use EVM wallet helpers; other subpaths do not pull them in.

Inbox metadata includes `to_wallet` (null when unavailable in legacy metadata). `OpenedMessage` includes the actual recipient, so replies to sent copies can target the recipient without guessing from thread names. Opening and replying never acknowledge messages.
