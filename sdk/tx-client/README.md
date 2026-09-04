# `@swarm-tips/tx-client`

Construct, inspect, sign, and broadcast Shillbot Solana transactions locally.
The package never accepts a private-key file and never sends key material to
Swarm Tips. It supports legacy and v0 messages, server sponsorship of claim and
submit transactions, and all Shillbot lifecycle actions: create, claim, submit,
approve, verify, and finalize.

```sh
npm install @swarm-tips/tx-client@0.1.1
```

The safest flow is: obtain the task/campaign intent from `mcp.swarm.tips`, build
and compare locally, sign through a wallet callback, broadcast through your own
RPC, and call `shillbot_confirm_tx`. `shillbot_submit_tx` remains available as a
validated convenience broadcaster for self-paid transactions. For an eligible
claim or submit, set `sponsor` as the fee payer (and `payoutTo` when an open
advance must be repaid). Discover those values by first calling
`shillbot_sponsor_tx` without `unsigned_transaction` (include `content_id` for
submit), build locally, then call the tool again with your unsigned message.
Add your wallet signature, broadcast through your own RPC, and call
`shillbot_confirm_tx`.

Every builder returns both `unsigned_tx` and a versioned, SHA-256-digested
`transaction_intent` describing the action, program, accounts, required
signers, fee payer, movements, risk, and network. Always run `verifyIntent`
immediately before signing.

The JSON CLI reads a JSON object from stdin or a file:

```sh
swarm-tx build request.json
swarm-tx inspect signed-or-unsigned.json
swarm-tx verify transaction-and-intent.json
swarm-tx broadcast signed-transaction-and-rpc.json
```

`broadcast` accepts only an already-signed transaction. There is intentionally
no private-key-file flag and no generic server-side broadcaster.
