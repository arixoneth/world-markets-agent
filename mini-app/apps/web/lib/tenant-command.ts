import { createHmac, timingSafeEqual } from "node:crypto";
import type { CommandReply, Tenant } from "@aomi-telegram/core";
import { bindWallet } from "./binding.ts";

const MAX_CLOCK_SKEW_SECONDS = 300;

type CommandRequest = {
  application: { id: number; name: string };
  thread_id: string;
  command: string;
  arguments: string;
  args: string[];
  /** The Telegram user's agent wallet for this app, named by Aomi. */
  wallet: null | { evm?: string | null };
  telegram: {
    user_id?: string | null;
    handle?: string | null;
    chat_id: string;
    message_id: number;
  };
};

function authenticate(request: Request, body: Uint8Array): boolean {
  const secret = process.env.AOMI_TELEGRAM_TENANT_SECRET;
  const timestamp = request.headers.get("x-aomi-timestamp") ?? "";
  const supplied = request.headers.get("x-aomi-signature")?.replace(/^sha256=/, "") ?? "";
  const seconds = Number(timestamp);
  if (!secret || !Number.isSafeInteger(seconds) || Math.abs(Date.now() / 1_000 - seconds) > MAX_CLOCK_SKEW_SECONDS) return false;

  const expected = createHmac("sha256", secret).update(timestamp).update(".").update(body).digest();
  let actual: Buffer;
  try {
    actual = Buffer.from(supplied, "hex");
  } catch {
    return false;
  }
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}

function parse(body: Uint8Array): CommandRequest | null {
  try {
    const value = JSON.parse(Buffer.from(body).toString("utf8")) as Partial<CommandRequest>;
    if (
      typeof value.command !== "string" ||
      typeof value.arguments !== "string" ||
      !Array.isArray(value.args) ||
      !value.args.every((arg) => typeof arg === "string") ||
      typeof value.thread_id !== "string" ||
      !value.telegram ||
      typeof value.telegram.chat_id !== "string"
    ) return null;
    return value as CommandRequest;
  } catch {
    return null;
  }
}

export async function runTenantCommand<A>(request: Request, routeCommand: string, tenant: Tenant<A>): Promise<Response> {
  const raw = new Uint8Array(await request.arrayBuffer());
  if (!authenticate(request, raw)) return Response.json({ error: "unauthorized" }, { status: 401 });

  const input = parse(raw);
  if (!input || input.command !== routeCommand) return Response.json({ error: "invalid_request" }, { status: 400 });
  const command = tenant.commands.find((candidate) => candidate.name === routeCommand);
  if (!command) return Response.json({ error: "unknown_command" }, { status: 404 });

  const binding = input.telegram.user_id
    ? await bindWallet(tenant, input.telegram.user_id, input.wallet?.evm)
    : null;
  const account = binding ? await tenant.adapter.resolveAccount(binding) : null;
  const base = new URL("/mini-app", request.url);
  const miniAppUrl = (view: string, query: Record<string, string> = {}) => {
    const url = new URL(`${base.pathname}${view ? `/${view}` : ""}`, base);
    for (const [name, value] of Object.entries(query)) url.searchParams.set(name, value);
    return url.toString();
  };
  const reply: CommandReply = await command.render({ binding, account, args: input.args, miniAppUrl });
  if (!reply.text.trim() || [...reply.text].length > command.budget) {
    console.error(JSON.stringify({ event: "tenant.command_budget", command: routeCommand, length: [...reply.text].length, budget: command.budget }));
    return Response.json({ error: "render_failed" }, { status: 502 });
  }
  return Response.json(reply, { headers: { "cache-control": "no-store" } });
}

