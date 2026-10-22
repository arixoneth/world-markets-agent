# World Markets — execution procedure

One action tool per instruction: it reads live state, evaluates the signed mandate, and on allow encodes the exact venue call and hands it to the host, which stages, simulates, and commits it atomically. That chain is host-enforced. Run it in silence; stop at the first failure.

Every staged call goes `to` the exchange `0xf6b54e033bb45a583aa642924bcef78b804588ae` on chain 2092151908; the guard admits only the fourteen trading functions.

## New order (spot, perp, or lend book)

1. `execute_world_order` with product, side, symbols, the user's whole sentence as `text` (plus `price` / `order_type` / `slippage` when named). Do not preview first: it evaluates the same mandate and returns the authoritative `preview.verdict`. On a lend book `side` is `lend` (supply) or `borrow` and `price` is the annual rate as a decimal fraction (`0.05` = 5% APR); a market intent moves from the best book rate, and the result names `interest_rate`, never a price.
   - `deny` → stop; nothing is staged; report the block. A stop with a `message` (`unknown_asset`, `size_ambiguous`, `size_no_position`, `empty_lend_book`) → paste it verbatim; `size_denomination_mismatch` → resend with the field it names.
   - `allow` → the result carries `staged` (`signature`, `args` `[book, word]`, `calldata`, `description`) and the host has queued the next step.
2. `evm_stage_tx` exactly as the host suggests it, every field and the routed plan id verbatim.
3. `simulate_batch` then `evm_commit_txs` are enforced by the host. A revert or guard block stops the chain; report the reason verbatim. The host's receipt hash is the only fill evidence.


## Cancel a resting order

Spot or perp: `get_world_open_orders` for the pair → `order_id` and `side` (none → say so), then `cancel_world_order` with product, side, symbols, `order_id`; `error: no_such_order` → stop. Lend book: a resting order is identified by its rate — `cancel_world_order { product: lend, side, base_symbol, interest_rate }`. Stage as suggested; simulate and commit are enforced. A cancel never trades.

## Loans

Borrower loans only, one per call, gated on the mandate (when set), its floor, and liquidation eligibility; a stop comes back as `verdict` (`liquidatable`, `portfolio_floor`, or a mandate-absent rule).

- Renew: `renew_world_loan { position_id }` → `renewLoan(account, account, position_id)`.
- Pay dues: `pay_world_loan_interest { position_id, reduce_quantity_raw?, extend_period? }` → `payInterestAndFees`; repay principal or extend only when the user asked.
- `error: position_id_unavailable` → the exchange exposes only an aggregated view of that loan; say its id is not available. Never guess an id.

## Never

- Never call `batchCommands`, `liquidate`, `bankruptcy`, or any deposit, withdrawal, transfer, approval, or bridge; a guard block is a report, not a retry.
- Never call `evm_stage_tx` with data you typed, never stage to another address or chain, never edit a word, selector, or calldata, never re-type a number between tools.
- Never stage after `deny`, `no_such_order`, or a failed simulation; never split a batch, commit twice, or fall back to EOA execution.

After commit: the RECEIPT (reporting skill) from the host receipt and a fresh `get_world_account`; no hash → never claim a fill.
