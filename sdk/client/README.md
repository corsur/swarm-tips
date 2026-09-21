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

### Signed wallet sessions for Shillbot

A wallet address is not an API credential. Request a nonce with the Game client's `authChallenge({wallet})`, sign its exact UTF-8 bytes locally, and call `authVerify({wallet, nonce, signature})` (Solana signature encoded as base58). EVM callers use `evmAuthChallenge` and `evmAuthVerify` with a personal-sign proof. Supply the returned token to `ShillbotApiClient` through `getToken`. Never provide a private key to either API.

Sessions last 24 hours. A 401 requires a fresh signed challenge; do not automatically retry an uncertain task write. Clear credentials and account-owned state when the wallet changes. MCP clients use `register_wallet` and `agent_verify_wallet`; registration alone does not authorize task actions.
