# `@swarm-tips/contracts`

Public, generated low-level contract bindings for the Swarm Tips Solana
programs. The package includes the raw Anchor IDLs and generated TypeScript
types for Coordination Game and Shillbot, their program IDs, canonical
Shillbot PDA helpers, and IDL-driven instruction encoding.

```sh
npm install @swarm-tips/contracts@0.1.0
```

Most applications should install `@swarm-tips/tx-client` instead. It re-exports
these bindings and adds transaction intent inspection, semantic verification,
sponsorship, wallet-callback signing, direct RPC broadcasting, and a JSON CLI.
Use this package directly when you specifically need the low-level IDL or
instruction interface.

The generated files are synchronized from `target/idl` and `target/types` after
Anchor builds. `pnpm check:generated` fails if a committed binding differs from
the current build output.
