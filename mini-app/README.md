# World Mini App

One Vercel application owned by World, living in this repository beside the
World Markets Aomi app (`../src`). Aomi owns the Telegram webhook and bot
runtime; this directory owns World's Mini App and deterministic read commands.

```
Telegram ──► Aomi backend ──┬── configured /b /p /r ... ──signed POST──► /mini-app/<command>
                            └── normal messages and built-ins ─────────► Aomi runtime
Telegram Mini App ─────────────────────────────────────────────────────► /mini-app
```

## Layout

- `packages/core` — tenant contract, Telegram initData verification and shared formatting
- `apps/web` — the only deployable: Next.js Mini App, deterministic command routes and BFF
- `tenants/world` — World Markets on UniFi testnet

## Run locally

All commands run from this `mini-app/` directory.

```bash
pnpm install
cp .env.example apps/web/.env.local
pnpm test
pnpm --filter @aomi-telegram/web dev
```

## Web app (mini app + BFF)

`apps/web` is a Next.js app. Screens are generic over the tenant contract:
`/t/<tenant>` (compact home), `/ledger`, `/portfolio`, `/products`,
`/chart?symbol=&period=`, `/watch`. Every BFF route under `/api/t/<tenant>/`
verifies Mini App initData against Telegram's public key, then resolves the
canonical Aomi bot binding. For local work
`WEB_DEV_TELEGRAM_USER_ID` (ignored in production) acts as that Telegram user.

```bash
pnpm --filter @aomi-telegram/web dev   # http://localhost:8791/t/world
```

In Aomi Build, configure the bot application with base URL
`https://api.world.inc/mini-app` and commands such as `b,p,r,a,d,chart,tasks`.
Aomi POSTs only those commands to `/mini-app/<command>` with a short-lived HMAC.

## World tenant

Reads go through the partner's own SDK (`@wcm-inc/sdk`) against UniFi testnet.
Env overrides: `WORLD_RPC_URL`, `WORLD_EXCHANGE_ADDRESS`, `WORLD_CHAIN_ID`.
`WORLD_LIVE=1 pnpm --filter @aomi-telegram/tenant-world test` runs the live
reads against account 20 on the public RPC.

## Local network note

Off-venue fetches (Kraken candles) honour `HTTPS_PROXY` / `NO_PROXY` through an
undici proxy agent; venue RPC calls stay direct.

## Deploying

See `docs/deploy.md`. There is one Vercel deployment and no Telegram worker or
service database.

## Invariants

- Aomi is the only Telegram webhook owner.
- Custom commands are configured per application, slash-only, DM-only and deterministic.
- Aomi names each Telegram user's agent wallet; World's trader store names the account it may trade.
- A shared HMAC secret authenticates Aomi command calls. Bot tokens and webhook capabilities are never forwarded.
- A command that renders over its character budget fails; it is never trimmed.
