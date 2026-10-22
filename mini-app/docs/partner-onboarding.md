# Partner onboarding

1. Deploy this repository to Vercel and set the variables in `.env.example`.
2. In Aomi Build, register the Telegram bot, select the World application, set its tenant base URL to `https://api.world.inc/mini-app`, and list the supported commands.
3. Give the Aomi backend and this Vercel project the same `AOMI_TELEGRAM_TENANT_SECRET`.
4. Set `AOMI_TELEGRAM_WALLET_URL` in Vercel to the bot-scoped Aomi wallet endpoint (`/telegram/<webhook secret>/wallet`), and `WORLD_STORE_URL` + `WORLD_STORE_API_KEY` to World's trader store. Keep these server-side.
5. Aomi receives `/start`, makes the user's account and agent wallet, and remains the only Telegram webhook owner. The account owner allows that wallet once on World's authorize page (`allowTradingForAccount`), which writes World's trader store.

Deploy the backend first, then the Vercel app. No worker, webhook takeover, onboarding CLI, service database or scheduler is involved.

Verify real Telegram claim, mini-app display, activation, trade receipt, and revoked/expired access before declaring delivery complete.

Partner commands remain slash-only and DM-only, excluding Aomi's built-ins, with a hard character budget per reply.
