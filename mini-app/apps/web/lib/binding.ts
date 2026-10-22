import type { AccountBinding, Tenant } from "@aomi-telegram/core";

const ADDRESS = /^0x[0-9a-fA-F]{40}$/;
const DEFAULT_CHAIN_ID = 2092151908;

/**
 * The World account a Telegram user's agent wallet may trade. Aomi names the
 * wallet; World's trader store (written by the authorize page once
 * `allowTradingForAccount(N, K)` confirms) names the account; the chain
 * names its owner. No store record means the owner hasn't allowed it yet.
 */
export async function bindWallet<A>(
  tenant: Tenant<A>,
  telegramUserId: string,
  wallet: string | null | undefined,
): Promise<AccountBinding | null> {
  if (!wallet || !ADDRESS.test(wallet)) return null;
  const store = process.env.WORLD_STORE_URL;
  const key = process.env.WORLD_STORE_API_KEY;
  if (!store || !key) throw new Error("World's trader store is not configured");
  const chainId = Number(process.env.WORLD_CHAIN_ID ?? DEFAULT_CHAIN_ID);
  const url = new URL(`/v1/traders/${wallet}`, store);
  url.searchParams.set("chain_id", String(chainId));
  const response = await fetch(url, {
    redirect: "error",
    cache: "no-store",
    signal: AbortSignal.timeout(10_000),
    headers: { authorization: `Bearer ${key}` },
  });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error("trader lookup failed");
  const record = (await response.json()) as { account_id?: unknown };
  if (typeof record.account_id !== "number" || !Number.isSafeInteger(record.account_id))
    throw new Error("invalid trader record");
  const accountId = String(record.account_id);
  const ownerAddress = await tenant.adapter.accountOwner?.(accountId);
  if (!ownerAddress) return null;
  return { tenant: tenant.id, telegramUserId, accountId, chainId, ownerAddress };
}
