/** A Telegram user bound to one venue account inside one tenant. */
export interface AccountBinding {
  tenant: string;
  telegramUserId: string;
  accountId: string;
  chainId: number;
  ownerAddress: string;
}

export type Band = "safe" | "elevated" | "high" | "liquidation";

export interface Portfolio {
  /** Net asset value in quote units, exact decimal string. */
  nav: string;
  /** Risk-adjusted portfolio value, exact decimal string. Negative means liquidatable. */
  rapv: string;
  quote: string;
  eligibleForLiquidation: boolean;
}

export interface Holding { symbol: string; balance: string; available: string }
export interface PerpPosition { symbol: string; side: "long" | "short"; qty: string; entry: string; mark: string | null }
export interface LoanPosition { symbol: string; qty: string; rate: string }
export interface Positions {
  holdings: Holding[];
  perps: PerpPosition[];
  lent: LoanPosition[];
  borrowed: LoanPosition[];
}

export interface Order { id: string; symbol: string; market: "spot" | "perp" | "lend"; side: "buy" | "sell"; price: string; qty: string; kind: string }
export interface Fill { symbol: string; side: "buy" | "sell"; price: string; qty: string; at: Date; txHash?: string }
/** One indexed trade event, before the service stores it. */
export interface FillEvent extends Fill { book: string; market: "spot" | "perp" | "lend"; block: bigint; logIndex: number; txHash: string }
export interface Product { symbol: string; product: "spot" | "perp" | "lend"; book: string; tokenId: number; positionDecimals: number }
export interface Candle { t: number; o: string; h: string; l: string; c: string }

/**
 * Everything a tenant can read about its venue. Plain data in, plain data
 * out, exact decimals as strings. The Vercel command routes and Mini App BFF
 * share one implementation.
 */
export interface TenantAdapter<A = unknown> {
  resolveAccount(binding: AccountBinding): Promise<A>;
  portfolio(account: A): Promise<Portfolio>;
  positions(account: A): Promise<Positions>;
  openOrders(account: A): Promise<Order[]>;
  fills(account: A, since: Date): Promise<Fill[]>;
  markPrice(symbol: string): Promise<string>;
  riskBand(account: A): Promise<{ score: number; band: Band }>;
  products(): Promise<Product[]>;
  chart(symbol: string, period: ChartPeriod): Promise<Candle[]>;
  /** Current chain head, for fill scanning. */
  headBlock?(): Promise<bigint>;
  /** Trade events for one account over a block range. The service persists them and keeps the cursor. */
  scanFills?(account: A, fromBlock: bigint, toBlock: bigint): Promise<FillEvent[]>;
  /** Block to start a fresh account's fill scan from, normally the venue's deployment block. */
  fillsGenesis?: bigint;
  /** On-chain owner of a venue account: the binding for a wallet World's store maps to it. */
  accountOwner?(accountId: string): Promise<string | null>;
}

export type ChartPeriod = "d" | "w" | "m";

export interface CommandCtx<A = unknown> {
  binding: AccountBinding | null;
  account: A | null;
  args: string[];
  /** Public URL of a mini app view on this tenant deployment. */
  miniAppUrl(view: string, query?: Record<string, string>): string;
}

export interface CommandReply {
  text: string;
  /** Structured facts recorded into Aomi's thread beside the displayed text. */
  context?: Record<string, unknown>;
  button?: { text: string; url: string };
}

/**
 * One vendor slash command. `budget` is a hard character cap on `text`;
 * a render that exceeds it is a bug, not a truncation.
 */
export interface TenantCommand<A = unknown> {
  name: string;
  description: string;
  budget: number;
  render(ctx: CommandCtx<A>): Promise<CommandReply>;
}

export interface WatchKind {
  kind: string;
  description: string;
}

export interface TenantCopy {
  unmapped: string;
  renderFailed: string;
  /** Product name shown in the mini app header. */
  title: string;
  /** One line under the title, the partner's register. */
  tagline: string;
}

/**
 * Chat drafts the mini app can open. The user always edits and sends them in
 * Telegram; nothing here reaches the agent by itself.
 */
export interface TenantCompose {
  buy(symbol: string): string;
  sell(symbol: string): string;
  long(symbol: string): string;
  short(symbol: string): string;
  lend(symbol: string): string;
  cancelOrder(order: Order): string;
}

export interface Tenant<A = unknown> {
  id: string;
  adapter: TenantAdapter<A>;
  commands: TenantCommand<A>[];
  watches: WatchKind[];
  copy: TenantCopy;
  compose: TenantCompose;
}

export function commandNames(tenant: Pick<Tenant, "commands">): Set<string> {
  return new Set(tenant.commands.map((command) => command.name));
}
