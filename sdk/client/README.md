# @swarm-tips/client

One typed package for Swarm Tips client-side integrations. The root export is
deliberately lightweight; import only the capability you need:

- `@swarm-tips/client/shillbot`
- `@swarm-tips/client/coordination-game`
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
