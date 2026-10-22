# Deploying

This directory has one deployable: `apps/web` on Vercel. Telegram updates go
directly to the Aomi backend; there is no worker, Docker image, Fly application,
host process, service database or webhook takeover.

## Vercel

Import `World-Markets-Inc/aomi`, set the root directory to `mini-app/apps/web`,
and use the Next.js preset. The Rust app in the same repository is deployed
separately through Aomi Build and ignores this directory. Configure:

- `AOMI_TELEGRAM_TENANT_SECRET` — shared with the Aomi backend, used only to
  authenticate command requests.
- `TELEGRAM_BOT_ID` and `TELEGRAM_BOT_USERNAME` — public bot identity used for
  Mini App initData verification and chat links.
- `AOMI_TELEGRAM_WALLET_URL` — private, bot-scoped Aomi wallet lookup (a Telegram user to their agent wallet).
- `WORLD_STORE_URL`, `WORLD_STORE_API_KEY` — World's trader store (an agent wallet to its World account).
- Optional `WORLD_RPC_URL`, `WORLD_EXCHANGE_ADDRESS`, and `WORLD_CHAIN_ID`.

Never set `WEB_DEV_TELEGRAM_USER_ID` in production.

The public tenant base URL is `https://<domain>/mini-app`. It serves:

- `GET /mini-app` — World Mini App entry.
- `GET /mini-app/portfolio`, `/chart`, `/ledger` — Mini App views.
- `GET /mini-app/signing` — tenant-owned signing entry; currently lands in the
  World Mini App until World supplies a dedicated Para/Privy screen.
- `POST /mini-app/<command>` — HMAC-authenticated deterministic command reads.

## Aomi Build

On the bot's primary application configure:

```text
Tenant Mini App URL: https://<domain>/mini-app
Custom commands: b, p, r, a, d, chart, tasks
```

The backend caches this application configuration on the bot instance. A config
change increments the bot registration version, so the existing refresh loop
replaces the cached command set without restarting the process.

## Rollout order

1. Deploy the backend with `AOMI_TELEGRAM_TENANT_SECRET`.
2. Deploy this Vercel app with the same secret and its bot identity/binding env.
3. Save the tenant URL and command list in Aomi Build.
4. Confirm Telegram still points directly at the Aomi webhook.
5. Exercise `/b`, `/p`, `/r`, `/chart WETH w`, Mini App launch, binding failure,
   HMAC failure, and a normal free-text agent turn.

Trading remains an Aomi agent action and uses Aomi's normal policy/signing path;
tenant command endpoints are read-only display handlers.
