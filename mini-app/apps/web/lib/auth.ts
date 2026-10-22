import { verifyTelegramInitData, type AccountBinding, type Tenant } from "@aomi-telegram/core";
import { bindWallet } from "./binding.ts";
import { tenants } from "./server.ts";

export const INIT_DATA_HEADER = "x-telegram-init-data";

export type Authorized<A = unknown> = {
  tenant: Tenant<A>;
  botUsername: string;
  telegramUserId: string;
  binding: AccountBinding | null;
  account: A | null;
};

export class AuthError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
  }
}

/**
 * Every BFF call carries the Mini App initData. It is verified against
 * Telegram's public key with the tenant's bot id; Aomi names the Telegram
 * user's agent wallet for this bot, and World's store names its account.
 */
export async function authorize<A = unknown>(request: Request, tenantId: string): Promise<Authorized<A>> {
  const tenant = tenants.get(tenantId) as Tenant<A> | undefined;
  if (!tenant || tenantId !== "world") throw new AuthError(404, "unknown tenant");
  const botId = process.env.TELEGRAM_BOT_ID;
  const botUsername = process.env.TELEGRAM_BOT_USERNAME;
  const walletUrl = process.env.AOMI_TELEGRAM_WALLET_URL;
  if (!botId || !botUsername || !walletUrl) throw new Error("Telegram tenant environment is incomplete");

  let telegramUserId: string;
  const devUser = process.env.NODE_ENV !== "production" ? process.env.WEB_DEV_TELEGRAM_USER_ID : undefined;
  if (devUser) {
    telegramUserId = devUser;
  } else {
    const initData = request.headers.get(INIT_DATA_HEADER) ?? "";
    const verified = verifyTelegramInitData(initData, botId);
    if (!verified.ok) throw new AuthError(401, verified.reason);
    telegramUserId = verified.launch.telegramUserId;
  }

  const binding = await bindWallet(tenant, telegramUserId, await resolveWallet(walletUrl, telegramUserId));
  const account = binding ? await tenant.adapter.resolveAccount(binding) : null;
  return { tenant, botUsername, telegramUserId, binding, account };
}

/** The Telegram user's agent wallet for this bot, from Aomi. */
async function resolveWallet(url: string, telegramUserId: string): Promise<string | null> {
  try {
    const response = await fetch(url, {
      method: "POST",
      redirect: "error",
      cache: "no-store",
      signal: AbortSignal.timeout(10_000),
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ telegram_user_id: telegramUserId }),
    });
    if (!response.ok) throw new Error("lookup failed");
    const value = (await response.json()) as { wallet?: null | { evm?: unknown } };
    const evm = value.wallet?.evm;
    if (evm === undefined || evm === null) return null;
    if (typeof evm !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(evm)) throw new Error("invalid lookup response");
    return evm;
  } catch {
    throw new Error("Account linking is unavailable. Please try again.");
  }
}

export function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body, (_k, v) => (typeof v === "bigint" ? v.toString() : v)), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

/** Wraps a route so auth failures and adapter errors become JSON, never HTML error pages. */
export function route<A = unknown>(handler: (auth: Authorized<A>, request: Request, params: Record<string, string>) => Promise<Response>) {
  return async (request: Request, context: { params: Promise<Record<string, string>> }): Promise<Response> => {
    const params = await context.params;
    const startedAt = Date.now();
    const observed = (response: Response) => {
      // No headers, launch payload, Telegram identity or query string in logs.
      console.info(JSON.stringify({ event: "bff.response", path: new URL(request.url).pathname, status: response.status, ms: Date.now() - startedAt }));
      return response;
    };
    try {
      const auth = await authorize<A>(request, params.tenant ?? "");
      return observed(await handler(auth, request, params));
    } catch (error) {
      if (error instanceof AuthError) return observed(json({ error: error.message }, error.status));
      const cause = (error as { cause?: { code?: string; message?: string } }).cause;
      console.error(JSON.stringify({ event: "bff.error", error: String(error), cause: cause ? `${cause.code ?? ""} ${cause.message ?? ""}`.trim() : undefined }));
      return observed(json({ error: "venue_read_failed" }, 502));
    }
  };
}
