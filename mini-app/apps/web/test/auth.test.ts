import { beforeEach, describe, expect, it, vi } from "vitest";

const WALLET = "0x2222222222222222222222222222222222222222";
const fixture = vi.hoisted(() => ({
  verify: vi.fn(), fetch: vi.fn(), resolveAccount: vi.fn(), accountOwner: vi.fn(),
}));
vi.mock("@aomi-telegram/core", () => ({ verifyTelegramInitData: fixture.verify }));
vi.mock("../lib/server.ts", () => ({
  tenants: new Map([["world", {
    id: "world",
    adapter: { resolveAccount: fixture.resolveAccount, accountOwner: fixture.accountOwner },
  }]]),
}));
import { authorize } from "../lib/auth.ts";

/** Aomi names the user's agent wallet; World's store names its account. */
function answers(wallet: string | null, account: number | null) {
  fixture.fetch.mockImplementation(async (input: URL | string) => {
    const url = String(input);
    if (url === "https://api.aomi.test/wallet")
      return Response.json({ wallet: wallet ? { evm: wallet } : null });
    if (url.startsWith(`https://store.world.test/v1/traders/${WALLET}`) && account !== null)
      return Response.json({ account_id: account, mandate: null });
    return new Response(null, { status: 404 });
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubEnv("NODE_ENV", "production");
  vi.stubEnv("TELEGRAM_BOT_ID", "bot-1");
  vi.stubEnv("TELEGRAM_BOT_USERNAME", "worldbot");
  vi.stubEnv("AOMI_TELEGRAM_WALLET_URL", "https://api.aomi.test/wallet");
  vi.stubEnv("WORLD_STORE_URL", "https://store.world.test");
  vi.stubEnv("WORLD_STORE_API_KEY", "store-key");
  fixture.verify.mockReturnValue({ ok: true, launch: { telegramUserId: "123" } });
  fixture.accountOwner.mockResolvedValue("0x1111111111111111111111111111111111111111");
  answers(null, null);
  vi.stubGlobal("fetch", fixture.fetch);
});

describe("mini-app authorization", () => {
  it("rejects invalid Telegram identity before any lookup", async () => {
    fixture.verify.mockReturnValue({ ok: false, reason: "invalid signature" });
    await expect(authorize(new Request("https://mini.example/api/t/world/portfolio"), "world")).rejects.toMatchObject({ status: 401 });
    expect(fixture.fetch).not.toHaveBeenCalled();
  });
  it("derives the wallet from verified initData and the account from World's store, ignoring caller parameters", async () => {
    answers(WALLET, 42);
    fixture.resolveAccount.mockResolvedValue({ accountId: 42n });
    await authorize(new Request("https://mini.example/api/t/world/portfolio?telegram_user_id=999&account_id=victim", { headers: { "x-telegram-init-data": "signed-launch" } }), "world");
    expect(fixture.verify).toHaveBeenCalledWith("signed-launch", "bot-1");
    expect(fixture.fetch).toHaveBeenCalledWith(
      "https://api.aomi.test/wallet",
      expect.objectContaining({ body: JSON.stringify({ telegram_user_id: "123" }) }),
    );
    expect(fixture.resolveAccount).toHaveBeenCalledWith(expect.objectContaining({
      accountId: "42", ownerAddress: "0x1111111111111111111111111111111111111111",
    }));
  });
  it("does not read venue data without a wallet or before the owner allows it", async () => {
    expect(await authorize(new Request("https://mini.example"), "world")).toMatchObject({ binding: null, account: null });
    answers(WALLET, null);
    expect(await authorize(new Request("https://mini.example"), "world")).toMatchObject({ binding: null, account: null });
    expect(fixture.resolveAccount).not.toHaveBeenCalled();
  });
  it("does not fall back to venue data when Aomi's lookup fails", async () => {
    fixture.fetch.mockRejectedValue(new Error("unavailable"));
    await expect(authorize(new Request("https://mini.example"), "world")).rejects.toThrow("unavailable");
    expect(fixture.resolveAccount).not.toHaveBeenCalled();
  });
});
