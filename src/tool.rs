use alloy_primitives::Address;
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::*;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::str::FromStr;

use crate::client::{Account, AccountAccess, Asset, WorldClient, asset_by_symbol};
use crate::guardian::{GuardianPreference, GuardianStore, UnwindCandidate, UnwindPlan};
use crate::loans::LoanView;
use crate::mandate::{Mandate, TradeFacts, Verdict, parse_decimal};
use crate::order_intent::OrderIntent;
use crate::order_word::{LendOrderWord, OrderType, OrderWord, quantity_raw};
use crate::pnl::PnlLedger;
use crate::reporting::EffectPlan;
use crate::size::{ResolvedSize, SizeInput};
use crate::staging::{OrderAction, StagedCall, Venue};

#[derive(Clone, Default)]
pub(crate) struct WorldMarketsApp {
    client: WorldClient,
    pnl_ledger: PnlLedger,
    loan_origins: crate::loans::LoanOriginStore,
    guardian: GuardianStore,
    store: crate::store::TraderStore,
}

pub(crate) struct ListWorldAssets;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoArgs {}

impl JsonSchema for NoArgs {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NoArgs".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        })
        .try_into()
        .expect("empty tool argument schema")
    }
}

pub(crate) struct GetWorldAccount;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldAccountArgs {
    /// World account ID. Ignored when World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Expected owner wallet. This does not replace the acting wallet authorization check.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
    /// `full` includes the raw account dump. Default is the compact card.
    #[serde(default)]
    pub(crate) detail: Option<String>,
}

pub(crate) struct GetHealthSnapshot;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetHealthSnapshotArgs {
    /// World account ID. Ignored when World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Expected owner wallet. This does not replace the acting wallet authorization check.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
    /// Optional position filter for the PnL section (symbol or `perp:SYMBOL`).
    #[serde(default)]
    pub(crate) position: Option<String>,
}

pub(crate) struct GetWorldMarket;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldMarketArgs {
    /// Product type: spot, perp, or lend.
    pub(crate) product: String,
    /// Base asset symbol, such as BTC.b or WETH.
    pub(crate) base_symbol: String,
    /// Quote asset symbol. Required for spot and perp, omitted for lend.
    #[serde(default)]
    pub(crate) quote_symbol: Option<String>,
}

pub(crate) struct GetWorldRates;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldRatesArgs {
    /// Base symbols to include (e.g. ["WETH","WBTC"]). Omit for every listed asset.
    #[serde(default)]
    pub(crate) assets: Option<Vec<String>>,
}

pub(crate) struct GetWorldLoans;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldLoansArgs {
    /// World account ID. Ignored when World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Expected owner wallet. This does not replace the acting wallet authorization check.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

pub(crate) struct GetWorldOpenOrders;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldOpenOrdersArgs {
    /// Product type: spot or perp.
    pub(crate) product: String,
    /// Base asset symbol.
    pub(crate) base_symbol: String,
    /// Quote asset symbol.
    pub(crate) quote_symbol: String,
    /// World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

pub(crate) struct PreviewWorldTrade;
pub(crate) struct CheckWorldMandate;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub(crate) struct WorldTradeArgs {
    /// Product type: spot, perp, or lend.
    pub(crate) product: String,
    /// Spot/perp: buy or sell (long → buy, short → sell). Lend: lend (supply) or borrow.
    pub(crate) side: String,
    /// Base asset symbol.
    pub(crate) base_symbol: String,
    /// Quote asset symbol. Required for spot and perp; a lend book has none (USDT is assumed).
    #[serde(default)]
    pub(crate) quote_symbol: String,
    /// Human-readable base quantity, such as "0.25". Alias for size_base.
    #[serde(default)]
    pub(crate) quantity: String,
    /// Dollar/notional size when the user named dollars. Converted at the preview mark.
    #[serde(default)]
    pub(crate) size_usd: Option<String>,
    /// Base-asset size when the user named the asset unit.
    #[serde(default)]
    pub(crate) size_base: Option<String>,
    /// Limit price, or for lend the annual rate as a decimal fraction ("0.05" = 5% APR). Omit for a market intent (a limit at the mark or best book rate moved by `slippage`).
    #[serde(default)]
    pub(crate) price: Option<String>,
    /// `market` or `limit`. Inferred from `price` when omitted.
    #[serde(default)]
    pub(crate) order_type: Option<String>,
    /// Slippage decimal applied to a market intent, e.g. "0.005" for 0.5%. Default 0.005.
    #[serde(default)]
    pub(crate) slippage: Option<String>,
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
    /// The user's whole sentence, so a dollar vs asset size can be classified server-side.
    #[serde(default)]
    pub(crate) text: Option<String>,
}

pub(crate) struct GetWorldPnl;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldPnlArgs {
    /// World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
    /// Optional position filter: symbol (e.g. "WETH") or id (e.g. "perp:WETH").
    /// Omit for the full account, including recently closed positions this app observed.
    #[serde(default)]
    pub(crate) position: Option<String>,
}

pub(crate) struct ComputeResize;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ComputeResizeArgs {
    /// The engine `rule` code that gated the intent, verbatim.
    pub(crate) rule: String,
}

/// The action tool for a new order: same arguments as the preview. The tool
/// evaluates the mandate itself, so a prior preview call is redundant.
pub(crate) struct ExecuteWorldOrder;

pub(crate) struct CancelWorldOrder;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CancelWorldOrderArgs {
    /// Product type: spot, perp, or lend.
    pub(crate) product: String,
    /// Side of the resting order: buy or sell (long → buy, short → sell); lend or borrow on a lend book.
    pub(crate) side: String,
    /// Base asset symbol.
    pub(crate) base_symbol: String,
    /// Quote asset symbol. Required for spot and perp; omitted for lend.
    #[serde(default)]
    pub(crate) quote_symbol: String,
    /// Resting order id, from `get_world_open_orders`. Required for spot and perp.
    #[serde(default)]
    pub(crate) order_id: Option<u64>,
    /// The resting lend/borrow order's annual rate as a decimal fraction ("0.05"). Required for lend; a lend order is identified by its rate.
    #[serde(default)]
    pub(crate) interest_rate: Option<String>,
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

pub(crate) struct RenewWorldLoan;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RenewWorldLoanArgs {
    /// The borrower loan's `position_id` from `get_world_loans`.
    pub(crate) position_id: String,
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

pub(crate) struct PayWorldLoanInterest;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PayWorldLoanInterestArgs {
    /// The borrower loan's `position_id` from `get_world_loans`.
    pub(crate) position_id: String,
    /// Principal to repay alongside the dues, in position units. Omit to pay dues only.
    #[serde(default)]
    pub(crate) reduce_quantity_raw: Option<String>,
    /// Extend the term after paying. Only when the user asked to extend.
    #[serde(default)]
    pub(crate) extend_period: Option<bool>,
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

pub(crate) struct PreviewAccountEffect;

/// Intent only. Before and after figures are derived from live state in Rust.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PreviewAccountEffectArgs {
    /// Product type: spot, perp, or lend.
    pub(crate) product: String,
    /// buy or sell (long → buy, short → sell); lend or borrow for lend.
    pub(crate) side: String,
    /// Base asset symbol.
    pub(crate) base_symbol: String,
    /// Quote asset symbol.
    pub(crate) quote_symbol: String,
    /// Dollar/notional size when the user named dollars.
    #[serde(default)]
    pub(crate) size_usd: Option<String>,
    /// Base-asset size when the user named the asset unit.
    #[serde(default)]
    pub(crate) size_base: Option<String>,
    /// The user's whole sentence, so a dollar vs asset size can be classified server-side.
    #[serde(default)]
    pub(crate) text: Option<String>,
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

pub(crate) struct GetWorldAgentPermission;

/// The guardian: plan (and on a breach, stage) the cheapest unwind that
/// brings the account back above its floor.
pub(crate) struct GuardianUnwind;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GuardianUnwindArgs {
    /// `true` stages the plan when the account is below its floor or liquidation-eligible. `false` (default) only plans: a fire drill.
    #[serde(default)]
    pub(crate) execute: Option<bool>,
    /// Standing preference from the user's brief: `cheapest_safe` (default) or `protect_eth`.
    #[serde(default)]
    pub(crate) preference: Option<String>,
    /// Holdings the user asked never to touch, by symbol. A veto, never a candidate.
    #[serde(default)]
    pub(crate) protect: Option<Vec<String>>,
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

/// The user checked in after a guardian event: release the hold on
/// risk-adding orders.
pub(crate) struct AcknowledgeGuardian;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AcknowledgeGuardianArgs {
    /// Optional World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Optional expected owner wallet.
    #[serde(default)]
    pub(crate) wallet_address: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetWorldAgentPermissionArgs {
    /// World account ID. Ignored: World's store names the wallet's account.
    #[serde(default)]
    pub(crate) account_id: Option<u64>,
    /// Actor address to inspect. The active trading address is used when omitted.
    #[serde(default)]
    pub(crate) actor_address: Option<String>,
}

/// A preview either resolved a trade or stopped with a complete tool result:
/// a sizing question, a mismatch, or an asset World does not list.
pub(crate) enum PreviewOutcome {
    Trade(Box<TradePreview>),
    Stopped(Value),
}

/// A gated read: `Ok(Ok(ready))` proceeds, `Ok(Err(value))` is a complete
/// tool result that stops the flow (a verdict or a structured error), and
/// `Err` is a tool failure.
type Gated<T> = Result<Result<T, Value>, String>;

/// A borrower loan that passed every gate, with the venue to stage against.
pub(crate) struct ReadyLoan {
    pub(crate) value: Value,
    pub(crate) venue: Venue,
    pub(crate) access: AccountAccess,
    pub(crate) loan: LoanView,
    pub(crate) position_id: u64,
}

impl PreviewOutcome {
    pub(crate) fn to_json(&self) -> Value {
        match self {
            Self::Trade(preview) => preview.to_json(),
            Self::Stopped(value) => value.clone(),
        }
    }
}

/// Everything the execution procedure needs from one previewed intent, with
/// the deterministic mandate verdict. Never executable on its own.
pub(crate) struct TradePreview {
    pub(crate) chain_id: u64,
    pub(crate) exchange: String,
    pub(crate) block_number: u64,
    pub(crate) access: AccountAccess,
    pub(crate) product: &'static str,
    pub(crate) side: String,
    pub(crate) base: Asset,
    pub(crate) quote: Asset,
    pub(crate) order_book: String,
    pub(crate) quantity: Decimal,
    pub(crate) quantity_raw: u64,
    pub(crate) resolved_size: ResolvedSize,
    pub(crate) mark_price: String,
    pub(crate) mark_price_raw: u64,
    pub(crate) intent: OrderIntent,
    /// Lend only: the best taker rate on the book the market intent moved from.
    pub(crate) book_rate: Option<Decimal>,
    /// Lend only: `intent.limit_price` (an APR fraction) in venue ticks.
    pub(crate) rate_raw: Option<u16>,
    pub(crate) estimated_notional: Decimal,
    pub(crate) current_position_quantity: Decimal,
    pub(crate) verdict: Verdict,
}

impl TradePreview {
    pub(crate) fn to_json(&self) -> Value {
        let token = |asset: &Asset| {
            json!({
                "token_id": asset.token_id,
                "symbol": asset.symbol,
                "erc20_address": asset.erc20_address,
                "position_decimals": asset.position_decimals,
            })
        };
        let mut value = json!({
            "source": "world-markets-contract",
            "chain_id": self.chain_id,
            "exchange": self.exchange,
            "block_number": self.block_number,
            "access": {
                "account_id": self.access.account_id,
                "actor": self.access.actor,
                "authorization": self.access.authorization,
            },
            "preview": {
                "account_id": self.access.account_id,
                "product": self.product,
                "side": self.side,
                "base": token(&self.base),
                "quote": token(&self.quote),
                "order_book": self.order_book,
                "quantity": self.quantity.normalize().to_string(),
                "quantity_raw": self.quantity_raw.to_string(),
                "resolved_size": self.resolved_size.to_json(),
                "mark_price": self.mark_price,
                "mark_price_raw": self.mark_price_raw,
                "order_type": self.intent.order_type,
                "slippage": self.intent.slippage.normalize().to_string(),
                "estimated_notional": self.estimated_notional.normalize().to_string(),
                "current_position_quantity": self.current_position_quantity.normalize().to_string(),
                "verdict": {
                    "status": self.verdict.status,
                    "rule": self.verdict.rule,
                    "detail": self.verdict.detail,
                },
                "executable": false,
            },
        });
        // On a lend book the "price" is an annual rate; name it so the model
        // never quotes a rate as a price.
        let preview = value["preview"].as_object_mut().expect("preview object");
        match self.rate_raw {
            Some(raw) => {
                preview.insert(
                    "interest_rate".into(),
                    json!(self.intent.limit_price.normalize().to_string()),
                );
                preview.insert("interest_rate_raw".into(), json!(raw));
                preview.insert(
                    "book_rate".into(),
                    json!(self.book_rate.map(|rate| rate.normalize().to_string())),
                );
            }
            None => {
                preview.insert(
                    "limit_price".into(),
                    json!(self.intent.limit_price.normalize().to_string()),
                );
            }
        }
        value
    }
}

impl WorldMarketsApp {
    /// `WORLD_ACCOUNT_ID` for aomi-run, which has no trader store: it scopes
    /// local lookups to one account. Compiled out of a hosted build.
    #[cfg(feature = "local-dev")]
    fn local_account_id() -> Option<u64> {
        let raw = std::env::var("WORLD_ACCOUNT_ID").ok()?;
        raw.parse::<u64>()
            .ok()
            .or_else(|| raw.strip_prefix("world-")?.parse::<u64>().ok())
    }

    #[cfg(not(feature = "local-dev"))]
    fn local_account_id() -> Option<u64> {
        None
    }

    /// The client for this call: the wallet's chain must be the World chain.
    fn client_for(&self, ctx: &DynToolCallCtx) -> Result<WorldClient, String> {
        if let Some(chain_id) = ctx.attribute_u64(&["domain", "evm", "chain_id"])
            && chain_id != self.client.chain_id()
        {
            return Err(format!(
                "[world-markets] active chain {chain_id} is not the World chain {}",
                self.client.chain_id()
            ));
        }
        Ok(self.client.clone())
    }

    /// The trader is the agent wallet itself (its own address, upgraded in
    /// place); World's store names the account its owner allowed it to
    /// trade, and the chain confirms the grant on every call, so a revoke
    /// blocks the next one. Without a record the reply is the authorize link.
    fn access(
        &self,
        client: &WorldClient,
        account_id: Option<u64>,
        wallet_address: Option<&str>,
        ctx: &DynToolCallCtx,
    ) -> Result<AccountAccess, String> {
        let Some(trader) = ctx.attribute_string(&["domain", "evm", "address"]) else {
            // aomi-run without a wallet: WORLD_ACCOUNT_ID scopes read-only lookups.
            return client.resolve_account(
                Self::local_account_id().or(account_id),
                wallet_address,
                None,
            );
        };
        let record = match Self::local_account_id() {
            Some(account_id) => crate::store::TraderRecord {
                account_id,
                mandate: None,
            },
            None => self
                .store
                .trader(ctx, &trader, client.chain_id())?
                .ok_or_else(|| Self::not_authorized(&trader))?,
        };
        let mut access = client.resolve_account(Some(record.account_id), None, Some(&trader))?;
        access.mandate = record.mandate;
        Ok(access)
    }

    /// The owner's mandate for this wallet, read from the store alone (no
    /// chain read): what a floor or resize reply cites.
    fn stored_mandate(&self, ctx: &DynToolCallCtx) -> Result<Option<Value>, String> {
        let Some(trader) = ctx.attribute_string(&["domain", "evm", "address"]) else {
            return Ok(None);
        };
        Ok(self
            .store
            .trader(ctx, &trader, self.client.chain_id())?
            .and_then(|record| record.mandate))
    }

    /// This wallet may not trade a World account yet: the owner allows it
    /// once, on World's site.
    fn not_authorized(trader: &str) -> String {
        match crate::store::authorize_link(trader) {
            Ok(link) => format!(
                "[world-markets] this wallet ({trader}) isn't allowed to trade a World account yet. The account owner allows it once at {link} [err.type=world_trader_not_authorized]"
            ),
            Err(error) => error,
        }
    }

    fn live_account(
        client: &WorldClient,
        access: &AccountAccess,
    ) -> Result<(Vec<Asset>, Account), String> {
        let assets = client.assets()?;
        let owner = Address::from_str(&access.owner)
            .map_err(|e| format!("[world-markets] invalid owner address: {e}"))?;
        let account = client.account_with_owner(access.account_id, owner, &assets)?;
        Ok((assets, account))
    }

    /// Account card: access, contract account facts, ATLAS metrics and the
    /// derived lookup figures.
    fn inspect_account(
        &self,
        account_id: Option<u64>,
        wallet_address: Option<&str>,
        ctx: &DynToolCallCtx,
    ) -> Result<(Value, Account, AccountAccess, WorldClient), String> {
        let client = self.client_for(ctx)?;
        let access = self.access(&client, account_id, wallet_address, ctx)?;
        let (assets, account) = Self::live_account(&client, &access)?;
        let block_number = client.block_number()?;
        let metrics =
            crate::liquidation_risk::compute_metrics(&client, &account, &assets, block_number)?;
        let lookups = crate::lookups::compute_lookups(
            &client,
            &account,
            &assets,
            &metrics.net_asset_value,
            block_number,
        )?;
        let payload = json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": block_number,
            "access": {
                "account_id": access.account_id,
                "authorization": access.authorization,
            },
            "account": {
                "account_id": account.account_id,
                "eligible_for_liquidation": account.eligible_for_liquidation,
                "risk_adjusted_portfolio_value": account.risk_adjusted_portfolio_value,
            },
            "metrics": metrics,
            "lookups": lookups,
            "guardian": {
                "hold": self.guardian.status(access.account_id)?,
            },
            "mandate": Mandate::optional(access.mandate.as_ref())
                .ok()
                .flatten()
                .and_then(|mandate| {
                    let quote = mandate.min_risk_adjusted_portfolio_value.quote.clone();
                    mandate.floor(&quote).ok().map(|floor| {
                        json!({ "floor": floor.normalize().to_string(), "quote": quote })
                    })
                }),
        });
        Ok((payload, account, access, client))
    }

    fn order_product(product: &str) -> Result<&'static str, String> {
        match product.to_ascii_lowercase().as_str() {
            "spot" => Ok("spot"),
            "perp" | "perpetual" => Ok("perp"),
            "lend" | "lending" => Ok("lend"),
            _ => Err("[world-markets] order tools support spot, perp, or lend".to_string()),
        }
    }

    /// `buy` / `sell` on a spot or perp book; `lend` / `borrow` on a lend book
    /// (a lender is the book's seller, a borrower its buyer).
    fn order_side(product: &str, side: &str) -> Result<String, String> {
        let side = side.trim().to_ascii_lowercase();
        let normalised = match (product, side.as_str()) {
            ("lend", "lend" | "supply" | "sell") => "lend",
            ("lend", "borrow" | "buy") => "borrow",
            ("lend", _) => {
                return Err(
                    "[world-markets] side must be lend or borrow on a lend book. Resend with side set."
                        .to_string(),
                );
            }
            (_, "buy" | "long") => "buy",
            (_, "sell" | "short") => "sell",
            _ => {
                return Err(
                    "[world-markets] side must be buy or sell (short→sell, long→buy). Resend with side set."
                        .to_string(),
                );
            }
        };
        Ok(normalised.to_string())
    }

    /// Lucas's CANT wall: a trade-shaped ask naming an asset World does not
    /// list is an incapacity, never a question and never a symbol guess.
    fn unknown_asset(symbol: &str) -> Value {
        json!({
            "error": "unknown_asset",
            "asset": symbol,
            "message": format!("I can't trade `{symbol}` — it isn't listed on World."),
            "reply_verbatim": true,
            "executable": false,
        })
    }

    fn trade_preview(
        &self,
        args: WorldTradeArgs,
        ctx: &DynToolCallCtx,
    ) -> Result<PreviewOutcome, String> {
        let product = Self::order_product(&args.product)?;
        let side = Self::order_side(product, &args.side)?;

        let client = self.client_for(ctx)?;
        let access = self.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            ctx,
        )?;
        let (assets, account) = Self::live_account(&client, &access)?;
        let Ok(base) = asset_by_symbol(&assets, &args.base_symbol) else {
            return Ok(PreviewOutcome::Stopped(Self::unknown_asset(
                &args.base_symbol,
            )));
        };
        let quote_symbol = match (product, args.quote_symbol.trim()) {
            ("lend", "") => "USDT",
            (_, symbol) => symbol,
        };
        let Ok(quote) = asset_by_symbol(&assets, quote_symbol) else {
            return Ok(PreviewOutcome::Stopped(Self::unknown_asset(quote_symbol)));
        };
        let market = client.market(
            product,
            base.clone(),
            (product != "lend").then(|| quote.clone()),
        )?;
        let mark_price = parse_decimal(&market.mark_price, "mark_price")
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        let current_position_quantity = account.position_quantity(product, &base.symbol)?;

        let quantity_arg = args.quantity.trim();
        let resolved = match (SizeInput {
            sentence: args.text.as_deref(),
            size_usd: args.size_usd.as_deref(),
            size_base: args.size_base.as_deref(),
            quantity: (!quantity_arg.is_empty()).then_some(quantity_arg),
            instrument: Some(&base.symbol),
            held: Some(current_position_quantity),
        })
        .resolve(mark_price)
        {
            Ok(resolved) => resolved,
            Err(err) => return Ok(PreviewOutcome::Stopped(err.to_json())),
        };
        let quantity_raw = quantity_raw(resolved.base_qty, base.position_decimals)?;
        if quantity_raw == 0 {
            return Err(format!(
                "[world-markets] quantity {} is below the {} position precision ({} decimals)",
                resolved.base_qty.normalize(),
                base.symbol,
                base.position_decimals
            ));
        }
        let quantity = Decimal::from(quantity_raw)
            / Decimal::from(10u64.pow(u32::from(base.position_decimals)));

        let price = args
            .price
            .as_deref()
            .map(|raw| parse_decimal(raw, "price"))
            .transpose()
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        let slippage = args
            .slippage
            .as_deref()
            .map(|raw| parse_decimal(raw, "slippage"))
            .transpose()
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        // On a lend book the order's "price" is an annual rate. A market
        // intent moves from the best taker rate: a lender accepts a little
        // less than the best borrow bid, a borrower pays a little more than
        // the best lend ask.
        let (intent, book_rate, rate_raw) = if product == "lend" {
            let rates = client.lend_book_rates(base.token_id)?;
            let book_rate = match side.as_str() {
                "lend" => rates.lend_apr,
                _ => rates.borrow_apr,
            }
            .map(|raw| parse_decimal(&raw, "book_rate"))
            .transpose()
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
            let Some(anchor) = book_rate.or(price) else {
                return Ok(PreviewOutcome::Stopped(json!({
                    "error": "empty_lend_book",
                    "message": format!("The `{}` lend book has no resting rate to move from — name a rate to rest at.", base.symbol),
                    "reply_verbatim": true,
                    "executable": false,
                })));
            };
            let direction = if side == "lend" { "sell" } else { "buy" };
            let intent = OrderIntent::resolve(
                args.order_type.as_deref(),
                price,
                slippage,
                direction,
                anchor,
            )?;
            let rate_raw = LendOrderWord::encode_rate(intent.limit_price)?;
            (intent, book_rate, Some(rate_raw))
        } else {
            (
                OrderIntent::resolve(
                    args.order_type.as_deref(),
                    price,
                    slippage,
                    &side,
                    mark_price,
                )?,
                None,
                None,
            )
        };

        let rapv = parse_decimal(
            &account.risk_adjusted_portfolio_value,
            "risk_adjusted_portfolio_value",
        )
        .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        // A failed ATLAS projection leaves `post_trade_rapv` unproven and the
        // mandate denies with `post_trade_risk_unavailable`.
        let post_trade_rapv = crate::liquidation_risk::project_post_trade(
            &client,
            &account,
            &assets,
            &crate::liquidation_risk::TradeIntent {
                product,
                side: &side,
                base: &base,
                quote: &quote,
                quantity,
                mark: mark_price,
            },
        )
        .ok()
        .map(|projection| projection.rapv);
        let verdict = match Mandate::optional(access.mandate.as_ref()) {
            Ok(None) => Verdict::unbounded(),
            Ok(Some(mandate)) => mandate.evaluate(&TradeFacts {
                product,
                side: &side,
                base: &base.symbol,
                quote: &quote.symbol,
                quantity,
                mark_price,
                current_position_quantity,
                risk_adjusted_portfolio_value: rapv,
                post_trade_risk_adjusted_portfolio_value: post_trade_rapv,
                eligible_for_liquidation: account.eligible_for_liquidation,
            }),
            Err(verdict) => verdict,
        };
        // A guardian hold outranks an allow: after an unwind, nothing that
        // lowers RAPV goes out until the user checks in.
        let verdict = match self.guardian.status(access.account_id)? {
            Some(hold) if verdict.is_allow() => {
                hold.blocks(rapv, post_trade_rapv).unwrap_or(verdict)
            }
            _ => verdict,
        };
        let estimated_notional = quantity.checked_mul(mark_price).ok_or_else(|| {
            "[world-markets] estimated notional exceeds numeric range".to_string()
        })?;

        Ok(PreviewOutcome::Trade(Box::new(TradePreview {
            chain_id: client.chain_id(),
            exchange: client.exchange(),
            block_number: client.block_number()?,
            access,
            product,
            side,
            base,
            quote,
            order_book: market.book,
            quantity,
            quantity_raw,
            resolved_size: resolved,
            mark_price: market.mark_price,
            mark_price_raw: market.mark_price_raw,
            intent,
            book_rate,
            rate_raw,
            estimated_notional,
            current_position_quantity,
            verdict,
        })))
    }

    /// The venue call for an allowed preview. Every field comes from the
    /// preview itself: the book the market resolved, the quantity and limit
    /// the size engine produced, and the order type the intent recorded.
    fn stage_new_order(preview: &TradePreview) -> Result<StagedCall, String> {
        let book = Address::from_str(&preview.order_book)
            .map_err(|e| format!("[world-markets] invalid order-book address: {e}"))?;
        let order_type = if preview.intent.order_type == "limit" {
            OrderType::Limit
        } else {
            OrderType::FillPartialKillRest
        };
        let (word, description) = match preview.rate_raw {
            Some(rate_raw) => (
                LendOrderWord {
                    account_id: preview.access.account_id,
                    quantity_raw: preview.quantity_raw,
                    rate_raw,
                    order_type,
                }
                .pack()?,
                format!(
                    "World {} {} {} at {} APR ({})",
                    preview.side,
                    preview.quantity.normalize(),
                    preview.base.symbol,
                    preview.intent.limit_price.normalize(),
                    preview.intent.order_type
                ),
            ),
            None => (
                OrderWord {
                    account_id: preview.access.account_id,
                    quantity_raw: preview.quantity_raw,
                    limit_price: preview.intent.limit_price,
                    order_type,
                    insertion_hint: 0,
                }
                .pack()?,
                format!(
                    "World {} {} {} {} at limit {} {} ({})",
                    preview.product,
                    preview.side,
                    preview.quantity.normalize(),
                    preview.base.symbol,
                    preview.intent.limit_price.normalize(),
                    preview.quote.symbol,
                    preview.intent.order_type
                ),
            ),
        };
        StagedCall::order(
            Venue {
                exchange: preview.exchange.clone(),
                chain_id: preview.chain_id,
            },
            OrderAction::New,
            preview.product,
            &preview.side,
            book,
            word,
            description,
        )
    }

    /// The venue call that cancels one resting order, packed from the order
    /// as the book shows it with the order id as the insertion hint.
    fn stage_cancel(
        &self,
        args: CancelWorldOrderArgs,
        ctx: &DynToolCallCtx,
    ) -> Gated<(Value, StagedCall)> {
        let product = Self::order_product(&args.product)?;
        let side = Self::order_side(product, &args.side)?;
        let client = self.client_for(ctx)?;
        let access = self.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            ctx,
        )?;
        let base_value = json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "access": access,
            "operation": "cancel_order",
            "executable": false,
        });
        if let Err(verdict) = Mandate::optional(access.mandate.as_ref()) {
            return Ok(Err(Self::with_verdict(base_value, &verdict)));
        }
        let assets = client.assets()?;
        let Ok(base) = asset_by_symbol(&assets, &args.base_symbol) else {
            return Ok(Err(Self::unknown_asset(&args.base_symbol)));
        };
        if product == "lend" {
            // A resting lend/borrow order is identified by its rate, not an id.
            let Some(rate) = args
                .interest_rate
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
            else {
                return Ok(Err(Self::with_error(
                    base_value,
                    "interest_rate_required",
                    "a lend or borrow order is cancelled by its resting rate; pass interest_rate"
                        .to_string(),
                )));
            };
            let rate = parse_decimal(rate, "interest_rate").map_err(|verdict| {
                format!("[world-markets] {}: {}", verdict.rule, verdict.detail)
            })?;
            let market = client.market(product, base.clone(), None)?;
            let book = Address::from_str(&market.book)
                .map_err(|e| format!("[world-markets] invalid order-book address: {e}"))?;
            let word = LendOrderWord {
                account_id: access.account_id,
                quantity_raw: 0,
                rate_raw: LendOrderWord::encode_rate(rate)?,
                order_type: OrderType::Limit,
            }
            .pack()?;
            let staged = StagedCall::order(
                Venue::of(&client),
                OrderAction::Cancel,
                product,
                &side,
                book,
                word,
                format!(
                    "Cancel World {side} order on {} at {} APR",
                    base.symbol,
                    rate.normalize()
                ),
            )?;
            let mut value = base_value;
            if let Some(obj) = value.as_object_mut() {
                obj.insert("order_book".into(), json!(market.book));
                obj.insert("interest_rate".into(), json!(rate.normalize().to_string()));
            }
            return Ok(Ok((value, staged)));
        }
        let Ok(quote) = asset_by_symbol(&assets, &args.quote_symbol) else {
            return Ok(Err(Self::unknown_asset(&args.quote_symbol)));
        };
        let Some(order_id) = args.order_id else {
            return Ok(Err(Self::with_error(
                base_value,
                "order_id_required",
                format!(
                    "a {product} order is cancelled by its order_id from get_world_open_orders"
                ),
            )));
        };
        let market = client.market(product, base.clone(), Some(quote.clone()))?;
        let open_orders = client.open_orders(&market, access.account_id)?;
        let Some(order) = open_orders
            .orders
            .iter()
            .find(|order| order.order_id == order_id && order.side == side)
        else {
            return Ok(Err(Self::with_error(
                base_value,
                "no_such_order",
                format!(
                    "no resting {side} order {order_id} on the {product} {}/{} book",
                    base.symbol, quote.symbol
                ),
            )));
        };
        let price = parse_decimal(&order.price, "price")
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        let book = Address::from_str(&market.book)
            .map_err(|e| format!("[world-markets] invalid order-book address: {e}"))?;
        let word = OrderWord {
            account_id: access.account_id,
            quantity_raw: order.quantity_raw,
            limit_price: price,
            order_type: OrderType::Limit,
            insertion_hint: order.order_id,
        }
        .pack()?;
        let staged = StagedCall::order(
            Venue::of(&client),
            OrderAction::Cancel,
            product,
            &side,
            book,
            word,
            format!(
                "Cancel World {product} {side} order {} ({} {} at {} {})",
                order.order_id, order.quantity, base.symbol, order.price, quote.symbol
            ),
        )?;
        let mut value = base_value;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("order".into(), json!(order));
            obj.insert("order_book".into(), json!(market.book));
        }
        Ok(Ok((value, staged)))
    }

    /// Everything a loan action needs, gated the way the mandate gates trades:
    /// a bound mandate, no liquidation halt, the account above its floor, and
    /// a borrower loan with an on-chain position id.
    fn loan_action(
        &self,
        operation: &str,
        account_id: Option<u64>,
        wallet_address: Option<&str>,
        position_id: &str,
        ctx: &DynToolCallCtx,
    ) -> Gated<ReadyLoan> {
        let client = self.client_for(ctx)?;
        let access = self.access(&client, account_id, wallet_address, ctx)?;
        let value = json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "access": access,
            "operation": operation,
            "executable": false,
        });
        let mandate = match Mandate::optional(access.mandate.as_ref()) {
            Ok(mandate) => mandate,
            Err(verdict) => return Ok(Err(Self::with_verdict(value, &verdict))),
        };
        let (assets, account) = Self::live_account(&client, &access)?;
        if let Some(mandate) = &mandate
            && account.eligible_for_liquidation
            && mandate.halt_if_eligible_for_liquidation
        {
            return Ok(Err(Self::with_verdict(
                value,
                &Verdict {
                    status: "deny",
                    rule: "liquidatable",
                    detail: "The live World account is eligible for liquidation and this mandate requires a halt.".to_string(),
                },
            )));
        }
        let rapv = parse_decimal(
            &account.risk_adjusted_portfolio_value,
            "risk_adjusted_portfolio_value",
        )
        .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        let floor = mandate
            .as_ref()
            .map(|mandate| {
                parse_decimal(&mandate.min_risk_adjusted_portfolio_value.amount, "floor")
            })
            .transpose()
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        if let Some(floor) = floor
            && rapv < floor
        {
            return Ok(Err(Self::with_verdict(
                value,
                &Verdict {
                    status: "deny",
                    rule: "portfolio_floor",
                    detail: format!(
                        "Live risk-adjusted portfolio value {rapv} is below the mandate floor {floor}."
                    ),
                },
            )));
        }
        let snapshot = crate::loans::snapshot(&client, &self.loan_origins, &account, &assets)?;
        let Some(loan) = snapshot
            .loans
            .into_iter()
            .find(|loan| loan.position_id == position_id.trim())
        else {
            return Ok(Err(Self::with_error(
                value,
                "no_such_loan",
                format!("no loan {position_id} on account {}", access.account_id),
            )));
        };
        if loan.side != "borrower" {
            return Ok(Err(Self::with_error(
                value,
                "not_a_borrower_loan",
                format!(
                    "loan {position_id} is a {} position; only borrower loans renew or pay interest",
                    loan.side
                ),
            )));
        }
        let Ok(numeric_id) = loan.position_id.parse::<u64>() else {
            return Ok(Err(Self::with_error(
                value,
                "position_id_unavailable",
                format!(
                    "loan {position_id} is an aggregated view; the exchange does not expose its on-chain lending id, so it cannot be renewed or paid from here"
                ),
            )));
        };
        Ok(Ok(ReadyLoan {
            value,
            venue: Venue::of(&client),
            access,
            loan,
            position_id: numeric_id,
        }))
    }

    fn with_verdict(mut value: Value, verdict: &Verdict) -> Value {
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "verdict".into(),
                json!({
                    "status": verdict.status,
                    "rule": verdict.rule,
                    "detail": verdict.detail,
                }),
            );
        }
        value
    }

    fn with_error(mut value: Value, error: &str, detail: String) -> Value {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("error".into(), json!(error));
            obj.insert("detail".into(), json!(detail));
        }
        value
    }

    /// Snapshot the live account and apply an intent through the same
    /// post-trade path the mandate uses. Intent only; every figure is derived.
    fn effect_plan(
        &self,
        args: PreviewAccountEffectArgs,
        ctx: &DynToolCallCtx,
    ) -> Gated<EffectPlan> {
        let product = match args.product.to_ascii_lowercase().as_str() {
            "spot" => "spot",
            "perp" | "perpetual" => "perp",
            "lend" | "lending" => "lend",
            _ => {
                return Err(
                    "[world-markets] preview_account_effect supports spot, perp, or lend"
                        .to_string(),
                );
            }
        };
        let side = match args.side.trim().to_ascii_lowercase().as_str() {
            "buy" | "long" => "buy",
            "sell" | "short" => "sell",
            "lend" if product == "lend" => "lend",
            "borrow" if product == "lend" => "borrow",
            _ => {
                return Err(
                    "[world-markets] side must be buy/sell (long → buy, short → sell) or lend/borrow"
                        .to_string(),
                );
            }
        };
        let client = self.client_for(ctx)?;
        let access = self.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            ctx,
        )?;
        let (assets, account) = Self::live_account(&client, &access)?;
        let block = client.block_number()?;
        let Ok(base) = asset_by_symbol(&assets, &args.base_symbol) else {
            return Ok(Err(Self::unknown_asset(&args.base_symbol)));
        };
        let Ok(quote) = asset_by_symbol(&assets, &args.quote_symbol) else {
            return Ok(Err(Self::unknown_asset(&args.quote_symbol)));
        };
        let current_qty = account.position_quantity(product, &base.symbol)?;

        let mut missing_mark_symbols = Vec::new();
        let mark = client
            .mark_price(base.token_id)
            .ok()
            .and_then(|(_, price)| parse_decimal(&price, "mark_price").ok());
        if mark.is_none() {
            missing_mark_symbols.push(base.symbol.clone());
        }
        let resolved = match (SizeInput {
            sentence: args.text.as_deref(),
            size_usd: args.size_usd.as_deref(),
            size_base: args.size_base.as_deref(),
            quantity: None,
            instrument: Some(&base.symbol),
            held: Some(current_qty),
        })
        .resolve(mark.unwrap_or(Decimal::ONE))
        {
            Ok(resolved) => resolved,
            Err(err) => return Ok(Err(err.to_json())),
        };
        let quantity = resolved.base_qty;
        if quantity <= Decimal::ZERO {
            return Err("[world-markets] quantity must be greater than zero".to_string());
        }

        let after_qty = if matches!(side, "buy" | "borrow") {
            current_qty + quantity
        } else {
            current_qty - quantity
        };
        let (exposure_before, exposure_after) = match mark {
            Some(price) => (current_qty.abs() * price, after_qty.abs() * price),
            None => (Decimal::ZERO, Decimal::ZERO),
        };
        let available_before = account
            .balances
            .iter()
            .find(|balance| balance.symbol.eq_ignore_ascii_case(&quote.symbol))
            .map(|balance| parse_decimal(&balance.available, "available"))
            .transpose()
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?
            .unwrap_or(Decimal::ZERO);
        let available_after = available_before + (exposure_before - exposure_after);

        let liquidation_risk_before =
            crate::liquidation_risk::compute_metrics(&client, &account, &assets, block)
                .ok()
                .and_then(|metrics| {
                    parse_decimal(&metrics.liquidation_risk, "liquidation_risk").ok()
                });
        let projected = mark
            .or_else(|| (product == "lend").then_some(Decimal::ONE))
            .and_then(|price| {
                crate::liquidation_risk::project_post_trade(
                    &client,
                    &account,
                    &assets,
                    &crate::liquidation_risk::TradeIntent {
                        product,
                        side,
                        base: &base,
                        quote: &quote,
                        quantity,
                        mark: price,
                    },
                )
                .ok()
            });
        let concern_clause =
            EffectPlan::concern_clause(&account, &base.symbol, current_qty, after_qty);
        Ok(Ok(EffectPlan {
            exposure_symbol: base.symbol,
            exposure_before,
            exposure_after,
            available_before,
            available_after,
            quote: quote.symbol,
            liquidation_risk_before,
            liquidation_risk_after: projected
                .as_ref()
                .and_then(|p| parse_decimal(&p.liquidation_risk, "liquidation_risk").ok()),
            estimated_cost: None,
            missing_mark_symbols,
            post_trade_risk_unavailable: projected.is_none(),
            concern_clause,
            baseline: format!(
                "live account snapshot at block {block} versus this intent — derived, not model-typed"
            ),
        }))
    }
}

impl DynAomiTool for ListWorldAssets {
    type App = WorldMarketsApp;
    type Args = NoArgs;
    const NAME: &'static str = "list_world_assets";
    const DESCRIPTION: &'static str = "List live World Markets assets and their token IDs, symbols, addresses, decimals, and risk parameters.";

    fn run(app: &WorldMarketsApp, _args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        Ok(json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "assets": client.assets()?,
        }))
    }
}

impl DynAomiTool for GetWorldAccount {
    type App = WorldMarketsApp;
    type Args = GetWorldAccountArgs;
    const NAME: &'static str = "get_world_account";
    const DESCRIPTION: &'static str = "Inspect a live World account after proving that the active actor is its owner or an on-chain permitted trader. Returns account facts, ATLAS metrics (net asset value, 0–10 liquidation risk) and per-class exposure lookups. Never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let before = app.client.rpc_stats();
        let (mut payload, account, access, client) =
            app.inspect_account(args.account_id, args.wallet_address.as_deref(), &ctx)?;
        if args.detail.as_deref() == Some("full")
            && let Some(obj) = payload.as_object_mut()
        {
            obj.insert("account".into(), json!(account));
            obj.insert("access".into(), json!(access));
        }
        client.attach_rpc_trace(before, &mut payload);
        Ok(payload)
    }
}

impl DynAomiTool for GetHealthSnapshot {
    type App = WorldMarketsApp;
    type Args = GetHealthSnapshotArgs;
    const NAME: &'static str = "get_health_snapshot";
    const DESCRIPTION: &'static str =
        "Health card in one call: the account card plus perpetual PnL. Never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let before = app.client.rpc_stats();
        let (mut payload, account, _access, client) =
            app.inspect_account(args.account_id, args.wallet_address.as_deref(), &ctx)?;
        let pnl = crate::pnl::report(&client, &app.pnl_ledger, &account, args.position.as_deref())?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("pnl".to_string(), json!(pnl));
            obj.insert("executable".to_string(), json!(false));
        }
        client.attach_rpc_trace(before, &mut payload);
        Ok(payload)
    }
}

impl DynAomiTool for GetWorldMarket {
    type App = WorldMarketsApp;
    type Args = GetWorldMarketArgs;
    const NAME: &'static str = "get_world_market";
    const DESCRIPTION: &'static str = "Resolve a live World spot, perpetual, or lending order book and the current configured mark price.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        let assets = client.assets()?;
        let base = asset_by_symbol(&assets, &args.base_symbol)?;
        let quote = args
            .quote_symbol
            .as_deref()
            .map(|symbol| asset_by_symbol(&assets, symbol))
            .transpose()?;
        let product = args.product.to_ascii_lowercase();
        let market = client.market(&product, base, quote)?;
        Ok(json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "market": market,
        }))
    }
}

impl DynAomiTool for GetWorldRates {
    type App = WorldMarketsApp;
    type Args = GetWorldRatesArgs;
    const NAME: &'static str = "get_world_rates";
    const DESCRIPTION: &'static str = crate::rates::RATES_DESCRIPTION;

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        let snapshot = crate::rates::snapshot(&client, args.assets.as_deref())?;
        serde_json::to_value(&snapshot)
            .map_err(|e| format!("[world-markets] failed to encode rates snapshot: {e}"))
    }
}

impl DynAomiTool for GetWorldLoans {
    type App = WorldMarketsApp;
    type Args = GetWorldLoansArgs;
    const NAME: &'static str = "get_world_loans";
    const DESCRIPTION: &'static str = "Individual lend/borrow loans: rate_apr, matures_at, time_remaining_seconds, extensible, counterparty. Aggregates on get_world_account are not enough for roll timing. 10-day term; missing start → first-seen+10d, extensible true. Never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        let access = app.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            &ctx,
        )?;
        let (assets, account) = WorldMarketsApp::live_account(&client, &access)?;
        let snapshot = crate::loans::snapshot(&client, &app.loan_origins, &account, &assets)?;
        serde_json::to_value(&snapshot)
            .map_err(|e| format!("[world-markets] failed to encode loans snapshot: {e}"))
    }
}

impl DynAomiTool for GetWorldOpenOrders {
    type App = WorldMarketsApp;
    type Args = GetWorldOpenOrdersArgs;
    const NAME: &'static str = "get_world_open_orders";
    const DESCRIPTION: &'static str = "Read the authorized World account's resting buy and sell orders for one live spot or perpetual market. Order ids here are what a cancel targets.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let product = match args.product.to_ascii_lowercase().as_str() {
            "spot" => "spot",
            "perp" | "perpetual" => "perp",
            _ => return Err("[world-markets] open orders support spot and perp only".to_string()),
        };
        let client = app.client_for(&ctx)?;
        let access = app.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            &ctx,
        )?;
        let assets = client.assets()?;
        let base = asset_by_symbol(&assets, &args.base_symbol)?;
        let quote = asset_by_symbol(&assets, &args.quote_symbol)?;
        let market = client.market(product, base, Some(quote))?;
        Ok(json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "access": access,
            "open_orders": client.open_orders(&market, access.account_id)?,
        }))
    }
}

impl DynAomiTool for PreviewWorldTrade {
    type App = WorldMarketsApp;
    type Args = WorldTradeArgs;
    const NAME: &'static str = "preview_world_trade";
    const DESCRIPTION: &'static str = "Preview one World spot or perpetual intent from live state: resolved size, book, mark, limit price, and the deterministic mandate verdict. It never stages or executes; for an instruction to trade call execute_world_order instead, which evaluates the same mandate and stages on allow. Pass text=the user's whole sentence so dollar vs asset sizes classify server-side.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Ok(app.trade_preview(args, &ctx)?.to_json())
    }
}

impl DynAomiTool for CheckWorldMandate {
    type App = WorldMarketsApp;
    type Args = WorldTradeArgs;
    const NAME: &'static str = "check_world_mandate";
    const DESCRIPTION: &'static str = "Evaluate one structured World trade intent against the bound mandate and live account/market state. Same body as preview_world_trade: the exact allow or deny rule under `preview.verdict`; it does not execute.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Ok(app.trade_preview(args, &ctx)?.to_json())
    }
}

impl DynAomiTool for GetWorldPnl {
    type App = WorldMarketsApp;
    type Args = GetWorldPnlArgs;
    const NAME: &'static str = "get_world_pnl";
    const DESCRIPTION: &'static str = "Compute account-level and per-position perpetual PnL. Open PnL is mark versus contract entry minus unpaid funding. Position PnL covers that position's lifetime (open to now, or open to close). Realized figures are captured when this app observes a true-up or close. Not a calendar-range report; never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        let access = app.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            &ctx,
        )?;
        let (_assets, account) = WorldMarketsApp::live_account(&client, &access)?;
        let pnl = crate::pnl::report(&client, &app.pnl_ledger, &account, args.position.as_deref())?;
        Ok(json!({
            "source": "world-markets-reporting",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "access": access,
            "executable": false,
            "pnl": pnl,
        }))
    }
}

impl DynAomiTool for ComputeResize {
    type App = WorldMarketsApp;
    type Args = ComputeResizeArgs;
    const NAME: &'static str = "compute_resize";
    const DESCRIPTION: &'static str = "For a blocked intent, return the user's RAPV floor from the signed mandate. A block cites exactly one number: the floor. Never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let resize = Mandate::bound(app.stored_mandate(&ctx)?.as_ref())
            .and_then(|mandate| mandate.floor_solution(&args.rule))
            .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        Ok(json!({
            "source": "world-markets-reporting",
            "resize": resize,
            "executable": false,
        }))
    }
}

impl DynAomiTool for ExecuteWorldOrder {
    type App = WorldMarketsApp;
    type Args = WorldTradeArgs;
    const NAME: &'static str = "execute_world_order";
    const DESCRIPTION: &'static str = "Place one World spot or perpetual order from the user's instruction. Evaluates live state and the signed mandate exactly like preview_world_trade (so no prior preview is needed); on allow it packs the order word, encodes the venue call, and hands it to the host, which stages it, simulates the batch, and commits atomically. A deny verdict stages nothing. Pass text=the user's whole sentence.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Self::run_with_routes(app, args, ctx).map(|routed| routed.value)
    }

    fn run_with_routes(
        app: &WorldMarketsApp,
        args: Self::Args,
        ctx: DynToolCallCtx,
    ) -> Result<ToolReturn, String> {
        let outcome = app.trade_preview(args, &ctx)?;
        let PreviewOutcome::Trade(preview) = &outcome else {
            return Ok(ToolReturn::value(outcome.to_json()));
        };
        let value = preview.to_json();
        if !preview.verdict.is_allow() {
            return Ok(ToolReturn::value(value));
        }
        Ok(WorldMarketsApp::stage_new_order(preview)?.routed(value))
    }
}

impl DynAomiTool for CancelWorldOrder {
    type App = WorldMarketsApp;
    type Args = CancelWorldOrderArgs;
    const NAME: &'static str = "cancel_world_order";
    const DESCRIPTION: &'static str = "Cancel one resting World spot or perpetual order by id (from get_world_open_orders). Requires a bound mandate; reads the order from the book, encodes the venue cancel, and hands it to the host to stage, simulate, and commit. A cancel never trades.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Self::run_with_routes(app, args, ctx).map(|routed| routed.value)
    }

    fn run_with_routes(
        app: &WorldMarketsApp,
        args: Self::Args,
        ctx: DynToolCallCtx,
    ) -> Result<ToolReturn, String> {
        Ok(match app.stage_cancel(args, &ctx)? {
            Ok((value, staged)) => staged.routed(value),
            Err(value) => ToolReturn::value(value),
        })
    }
}

impl DynAomiTool for RenewWorldLoan {
    type App = WorldMarketsApp;
    type Args = RenewWorldLoanArgs;
    const NAME: &'static str = "renew_world_loan";
    const DESCRIPTION: &'static str = "Extend one borrower loan for another term. Requires a bound, unexpired mandate and a live account above its floor and not liquidatable; encodes renewLoan for the loan's on-chain position id and hands it to the host to stage, simulate, and commit.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Self::run_with_routes(app, args, ctx).map(|routed| routed.value)
    }

    fn run_with_routes(
        app: &WorldMarketsApp,
        args: Self::Args,
        ctx: DynToolCallCtx,
    ) -> Result<ToolReturn, String> {
        let ReadyLoan {
            mut value,
            venue,
            access,
            loan,
            position_id,
        } = match app.loan_action(
            "renew_loan",
            args.account_id,
            args.wallet_address.as_deref(),
            &args.position_id,
            &ctx,
        )? {
            Ok(ready) => ready,
            Err(value) => return Ok(ToolReturn::value(value)),
        };
        let staged = StagedCall::renew_loan(
            venue,
            access.account_id,
            position_id,
            format!(
                "Renew World {} borrow {} (loan {}) for another term",
                loan.base_symbol, loan.quantity_raw, loan.position_id
            ),
        );
        if let Some(obj) = value.as_object_mut() {
            obj.insert("loan".into(), json!(loan));
        }
        Ok(staged.routed(value))
    }
}

impl DynAomiTool for PayWorldLoanInterest {
    type App = WorldMarketsApp;
    type Args = PayWorldLoanInterestArgs;
    const NAME: &'static str = "pay_world_loan_interest";
    const DESCRIPTION: &'static str = "Pay the interest and fees due on one borrower loan, optionally repaying principal (reduce_quantity_raw) or extending the term (extend_period). Same mandate and account gates as renew_world_loan; encodes payInterestAndFees and hands it to the host to stage, simulate, and commit.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Self::run_with_routes(app, args, ctx).map(|routed| routed.value)
    }

    fn run_with_routes(
        app: &WorldMarketsApp,
        args: Self::Args,
        ctx: DynToolCallCtx,
    ) -> Result<ToolReturn, String> {
        let reduce = match args.reduce_quantity_raw.as_deref().map(str::trim) {
            None | Some("") => 0,
            Some(raw) => u64::from_str(raw).map_err(|_| {
                "[world-markets] reduce_quantity_raw must be an integer string in position units"
                    .to_string()
            })?,
        };
        let extend = args.extend_period.unwrap_or(false);
        let ReadyLoan {
            mut value,
            venue,
            loan,
            position_id,
            ..
        } = match app.loan_action(
            "pay_loan_interest",
            args.account_id,
            args.wallet_address.as_deref(),
            &args.position_id,
            &ctx,
        )? {
            Ok(ready) => ready,
            Err(value) => return Ok(ToolReturn::value(value)),
        };
        let staged = StagedCall::pay_interest(
            venue,
            position_id,
            reduce,
            extend,
            format!(
                "Pay interest and fees on World {} borrow (loan {}){}{}",
                loan.base_symbol,
                loan.position_id,
                if reduce > 0 {
                    format!(", repaying {reduce} position units")
                } else {
                    String::new()
                },
                if extend { ", extending the term" } else { "" }
            ),
        );
        if let Some(obj) = value.as_object_mut() {
            obj.insert("loan".into(), json!(loan));
        }
        Ok(staged.routed(value))
    }
}

impl DynAomiTool for PreviewAccountEffect {
    type App = WorldMarketsApp;
    type Args = PreviewAccountEffectArgs;
    const NAME: &'static str = "preview_account_effect";
    const DESCRIPTION: &'static str = "Simulate an intent on the live account through the same post-trade path the mandate uses: before/after directional exposure, available-to-deploy, and the 0–10 liquidation risk (omitted when unprovable). Pass only the intent, never figures. For \"what would happen if I…\"; never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let effect = match app.effect_plan(args, &ctx)? {
            Ok(plan) => plan.derive(),
            Err(sizing) => return Ok(sizing),
        };
        Ok(json!({
            "source": "world-markets-reporting",
            "account_effect": effect,
            "concern_line": effect.concern_line,
            "executable": false,
        }))
    }
}

impl DynAomiTool for GuardianUnwind {
    type App = WorldMarketsApp;
    type Args = GuardianUnwindArgs;
    const NAME: &'static str = "guardian_unwind";
    const DESCRIPTION: &'static str = "The guardian. Ranks every closable leg by risk-adjusted value recovered per unit of exit cost and plans the cheapest set that brings the account back above its signed floor. With execute=false it is a fire drill: the plan only. With execute=true and the account below its floor or liquidation-eligible, it stages the ordered legs as one batch the host simulates and commits atomically, and holds all risk-adding orders until the user checks in (acknowledge_guardian). Never executes above the floor.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        Self::run_with_routes(app, args, ctx).map(|routed| routed.value)
    }

    fn run_with_routes(
        app: &WorldMarketsApp,
        args: Self::Args,
        ctx: DynToolCallCtx,
    ) -> Result<ToolReturn, String> {
        let execute = args.execute.unwrap_or(false);
        let preference = GuardianPreference::parse(args.preference.as_deref())?;
        let protect = args.protect.unwrap_or_default();
        let client = app.client_for(&ctx)?;
        let access = app.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            &ctx,
        )?;
        let mut value = json!({
            "source": "world-markets-guardian",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "access": access,
            "mode": if execute { "execute" } else { "drill" },
            "executable": false,
        });
        let mandate = match Mandate::bound(access.mandate.as_ref()) {
            Ok(mandate) => mandate,
            Err(verdict) => {
                return Ok(ToolReturn::value(WorldMarketsApp::with_verdict(
                    value, &verdict,
                )));
            }
        };
        let (assets, account) = WorldMarketsApp::live_account(&client, &access)?;
        let quote_symbol = mandate.min_risk_adjusted_portfolio_value.quote.clone();
        let floor = match mandate.floor(&quote_symbol) {
            Ok(floor) => floor,
            Err(verdict) => {
                return Ok(ToolReturn::value(WorldMarketsApp::with_verdict(
                    value, &verdict,
                )));
            }
        };
        let rapv = parse_decimal(
            &account.risk_adjusted_portfolio_value,
            "risk_adjusted_portfolio_value",
        )
        .map_err(|verdict| format!("[world-markets] {}: {}", verdict.rule, verdict.detail))?;
        let metrics = crate::liquidation_risk::compute_metrics(
            &client,
            &account,
            &assets,
            client.block_number()?,
        )?;
        let breach = account.eligible_for_liquidation || rapv < floor;
        let candidates = UnwindCandidate::from_account(&client, &account, &assets, &protect)?;
        let plan = UnwindPlan::cheapest_safe(&candidates, rapv, floor, preference);
        if let Some(obj) = value.as_object_mut() {
            obj.insert("breach".into(), json!(breach));
            obj.insert(
                "floor".into(),
                json!({ "value": floor.normalize().to_string(), "quote": quote_symbol }),
            );
            obj.insert(
                "risk_adjusted_portfolio_value".into(),
                json!(rapv.normalize().to_string()),
            );
            obj.insert("liquidation_risk".into(), json!(metrics.liquidation_risk));
            obj.insert(
                "eligible_for_liquidation".into(),
                json!(account.eligible_for_liquidation),
            );
            obj.insert("plan".into(), json!(plan));
            obj.insert(
                "hold".into(),
                json!(app.guardian.status(access.account_id)?),
            );
        }
        if !execute || !breach {
            return Ok(ToolReturn::value(value));
        }
        if plan.steps.is_empty() {
            return Ok(ToolReturn::value(WorldMarketsApp::with_error(
                value,
                "no_unwind_available",
                "no closable leg raises risk-adjusted portfolio value; nothing was staged"
                    .to_string(),
            )));
        }

        // Every leg passes the guardian rule on its own projection before
        // anything is staged; one denied leg stops the whole batch.
        let mut calls = Vec::with_capacity(plan.steps.len());
        for step in &plan.steps {
            let leg = &step.leg;
            let post = parse_decimal(&step.post_rapv, "post_rapv").map_err(|verdict| {
                format!("[world-markets] {}: {}", verdict.rule, verdict.detail)
            })?;
            let verdict = mandate.evaluate_unwind(&TradeFacts {
                product: leg.product,
                side: leg.side,
                base: &leg.base.symbol,
                quote: &leg.quote.symbol,
                quantity: leg.quantity,
                mark_price: leg.mark,
                current_position_quantity: account
                    .position_quantity(leg.product, &leg.base.symbol)?,
                risk_adjusted_portfolio_value: rapv,
                post_trade_risk_adjusted_portfolio_value: Some(post),
                eligible_for_liquidation: account.eligible_for_liquidation,
            });
            if !verdict.is_allow() {
                let mut value = WorldMarketsApp::with_verdict(value, &verdict);
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("blocked_leg".into(), json!(step.label));
                }
                return Ok(ToolReturn::value(value));
            }
            let intent = OrderIntent::resolve(
                Some("market"),
                None,
                Some(crate::guardian::EMERGENCY_SLIPPAGE),
                leg.side,
                leg.mark,
            )?;
            let raw = quantity_raw(leg.quantity, leg.base.position_decimals)?;
            if raw == 0 {
                continue;
            }
            let market = client.market(leg.product, leg.base.clone(), Some(leg.quote.clone()))?;
            let book = Address::from_str(&market.book)
                .map_err(|e| format!("[world-markets] invalid order-book address: {e}"))?;
            let word = OrderWord {
                account_id: access.account_id,
                quantity_raw: raw,
                limit_price: intent.limit_price,
                order_type: OrderType::FillPartialKillRest,
                insertion_hint: 0,
            }
            .pack()?;
            calls.push(StagedCall::order(
                Venue::of(&client),
                OrderAction::New,
                leg.product,
                leg.side,
                book,
                word,
                format!(
                    "Guardian: {} at limit {}",
                    step.label,
                    intent.limit_price.normalize()
                ),
            )?);
        }
        let hold = app.guardian.hold(
            access.account_id,
            format!(
                "floor {} {} breached at RAPV {}",
                floor.normalize(),
                quote_symbol,
                rapv.normalize()
            ),
        )?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("hold".into(), json!(hold));
        }
        Ok(StagedCall::routed_batch(calls, value))
    }
}

impl DynAomiTool for AcknowledgeGuardian {
    type App = WorldMarketsApp;
    type Args = AcknowledgeGuardianArgs;
    const NAME: &'static str = "acknowledge_guardian";
    const DESCRIPTION: &'static str = "Release the guardian's hold on risk-adding orders once the user has checked in after an unwind. Returns the hold it released, or none if nothing was held.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        let access = app.access(
            &client,
            args.account_id,
            args.wallet_address.as_deref(),
            &ctx,
        )?;
        let released = app.guardian.release(access.account_id)?;
        Ok(json!({
            "source": "world-markets-guardian",
            "account_id": access.account_id,
            "released": released.is_some(),
            "hold": released,
            "executable": false,
        }))
    }
}

impl DynAomiTool for GetWorldAgentPermission {
    type App = WorldMarketsApp;
    type Args = GetWorldAgentPermissionArgs;
    const NAME: &'static str = "get_world_agent_permission";
    const DESCRIPTION: &'static str = "Read the World account owner and permitted-trader list to say whether the active trading address is the owner, a delegated trader, or revoked. Never executes.";

    fn run(app: &WorldMarketsApp, args: Self::Args, ctx: DynToolCallCtx) -> Result<Value, String> {
        let client = app.client_for(&ctx)?;
        let actor = args
            .actor_address
            .or_else(|| ctx.attribute_string(&["domain", "evm", "address"]))
            .ok_or_else(|| {
                "[world-markets] no trading address is available for the permission check"
                    .to_string()
            })?;
        // No store record: not allowed yet, and the answer is where to allow it.
        let account_id = match WorldMarketsApp::local_account_id().or(args.account_id) {
            Some(id) => id,
            None => match app.store.trader(&ctx, &actor, client.chain_id())? {
                Some(record) => record.account_id,
                None => {
                    return Ok(json!({
                        "source": "world-markets-store",
                        "chain_id": client.chain_id(),
                        "permission": { "actor": actor, "authorized": false, "authorization": "none" },
                        "authorize_url": crate::store::authorize_link(&actor)?,
                        "executable": false,
                    }));
                }
            },
        };
        Ok(json!({
            "source": "world-markets-contract",
            "chain_id": client.chain_id(),
            "exchange": client.exchange(),
            "block_number": client.block_number()?,
            "permission": client.agent_permission(account_id, &actor)?,
            "executable": false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(attributes: Value) -> DynToolCallCtx {
        DynToolCallCtx {
            session_id: "test".to_string(),
            tool_name: "get_world_account".to_string(),
            call_id: "test-1".to_string(),
            state_attributes: attributes.as_object().unwrap().clone(),
            secrets: Default::default(),
        }
    }

    fn mandate_json(account: Option<u64>) -> Value {
        let mut mandate = json!({
            "version": 1,
            "markets": [{ "product": "perp", "base": "WETH", "quote": "USDT" }],
            "max_position_notional": { "amount": "25000", "quote": "USDT" },
            "max_leverage": "3",
            "min_risk_adjusted_portfolio_value": { "amount": "6000", "quote": "USDT" },
            "halt_if_eligible_for_liquidation": true,
            "can_withdraw": false
        });
        if let Some(id) = account {
            mandate["account"] = json!({ "id": id });
        }
        mandate
    }

    /// A venue whose account 21 is owned by 0x1111… and, while `granted`,
    /// lets 0x2222… trade it.
    fn venue(granted: bool) -> WorldClient {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Venue(AtomicUsize, bool);
        impl crate::rpc::RpcExecutor for Venue {
            fn post_json(&self, body: &Value) -> Result<Value, String> {
                assert_eq!(body["method"], "eth_call");
                assert!(
                    body["params"][0]["data"]
                        .as_str()
                        .unwrap()
                        .ends_with(&format!("{:064x}", 21))
                );
                let result = match self.0.fetch_add(1, Ordering::SeqCst) {
                    0 => format!("0x{:0>64}", "1111111111111111111111111111111111111111"),
                    1 if self.1 => format!(
                        "0x{:064x}{:064x}{:0>64}",
                        32, 1, "2222222222222222222222222222222222222222"
                    ),
                    1 => format!("0x{:064x}{:064x}", 32, 0),
                    _ => panic!("unexpected venue read"),
                };
                Ok(json!({"jsonrpc":"2.0", "id":body["id"], "result":result}))
            }
        }
        WorldClient::with_rpc(crate::rpc::RpcTransport::with_executor(Arc::new(Venue(
            AtomicUsize::new(0),
            granted,
        ))))
    }

    const TRADER: &str = "0x2222222222222222222222222222222222222222";

    fn app_with_store(records: Vec<(&str, u64, Option<Value>)>) -> WorldMarketsApp {
        WorldMarketsApp {
            store: crate::store::TraderStore::Fixed(std::sync::Arc::new(
                records
                    .into_iter()
                    .map(|(trader, account_id, mandate)| {
                        (
                            trader.to_string(),
                            crate::store::TraderRecord {
                                account_id,
                                mandate,
                            },
                        )
                    })
                    .collect(),
            )),
            ..WorldMarketsApp::default()
        }
    }

    fn trader_ctx() -> DynToolCallCtx {
        ctx_with(json!({ "domain": { "evm": { "address": TRADER, "chain_id": 2092151908u64 } } }))
    }

    /// T7: the agent wallet itself is the trader; World's store names its
    /// account (model arguments never override it), the chain confirms the
    /// grant on every call, and a revoke blocks the next one.
    #[test]
    fn the_wallet_trades_the_account_the_store_names_until_revoked() {
        let app = app_with_store(vec![(TRADER, 21, Some(mandate_json(None)))]);
        let ctx = trader_ctx();
        assert!(app.client_for(&ctx).is_ok());
        let access = app
            .access(
                &venue(true),
                Some(99),
                Some("0x3333333333333333333333333333333333333333"),
                &ctx,
            )
            .unwrap();
        assert_eq!(access.account_id, 21);
        assert_eq!(access.owner, "0x1111111111111111111111111111111111111111");
        assert_eq!(access.actor, TRADER);
        assert_eq!(access.authorization, "delegated_trader");
        assert!(access.mandate.is_some());
        let error = app.access(&venue(false), None, None, &ctx).unwrap_err();
        assert!(error.contains("neither the owner"), "{error}");
    }

    /// No store record: nothing trades, and the reply is where the owner
    /// allows this wallet.
    #[test]
    fn an_unauthorized_wallet_gets_the_authorize_link() {
        unsafe { std::env::set_var("WORLD_AUTHORIZE_URL", "https://world.example/authorize") };
        let app = app_with_store(Vec::new());
        let error = app
            .access(&WorldClient::default(), None, None, &trader_ctx())
            .unwrap_err();
        assert!(
            error.contains(&format!("https://world.example/authorize?trader={TRADER}")),
            "{error}"
        );
        assert!(error.contains("world_trader_not_authorized"), "{error}");
    }

    #[test]
    fn client_for_rejects_a_foreign_chain() {
        let app = WorldMarketsApp::default();
        let foreign = ctx_with(json!({ "domain": { "evm": { "chain_id": 1 } } }));
        assert!(app.client_for(&foreign).is_err());
        assert!(app.client_for(&trader_ctx()).is_ok());
    }

    #[test]
    fn preview_contract_snapshot() {
        let weth = Asset {
            token_id: 4,
            symbol: "WETH".to_string(),
            name: "Wrapped Ether".to_string(),
            token_type: "erc20".to_string(),
            erc20_address: "0x2222222222222222222222222222222222222222".to_string(),
            erc20_decimals: 18,
            vault_decimals: 8,
            position_decimals: 4,
            risk_price_percent: 10,
            risk_slippage_percent: 1.0,
        };
        let usdt = Asset {
            token_id: 1,
            symbol: "USDT".to_string(),
            name: "Tether".to_string(),
            token_type: "erc20".to_string(),
            erc20_address: "0x3333333333333333333333333333333333333333".to_string(),
            erc20_decimals: 6,
            vault_decimals: 6,
            position_decimals: 2,
            risk_price_percent: 0,
            risk_slippage_percent: 0.0,
        };
        let mark = Decimal::from_str("2465.71").unwrap();
        let resolved = SizeInput {
            sentence: Some("sell 0.01 WETH spot at market"),
            instrument: Some("WETH"),
            ..SizeInput::default()
        }
        .resolve(mark)
        .unwrap();
        let quantity = Decimal::from_str("0.01").unwrap();
        let preview = TradePreview {
            chain_id: 2092151908,
            exchange: "0xf6b54e033bb45a583aa642924bcef78b804588ae".to_string(),
            block_number: 12345,
            access: AccountAccess {
                account_id: 1577,
                owner: "0x4444444444444444444444444444444444444444".to_string(),
                actor: "0x5555555555555555555555555555555555555555".to_string(),
                authorization: "delegated_trader".to_string(),

                mandate: None,
            },
            product: "spot",
            side: "sell".to_string(),
            base: weth,
            quote: usdt,
            order_book: "0x6666666666666666666666666666666666666666".to_string(),
            quantity,
            quantity_raw: 100,
            resolved_size: resolved,
            mark_price: "2465.71".to_string(),
            mark_price_raw: 7890274,
            intent: OrderIntent::resolve(None, None, None, "sell", mark).unwrap(),
            book_rate: None,
            rate_raw: None,
            estimated_notional: quantity * mark,
            current_position_quantity: Decimal::from_str("0.05").unwrap(),
            verdict: Verdict {
                status: "allow",
                rule: "mandate_v1",
                detail: "Mandate v1 permits spot sell 0.01 WETH.".to_string(),
            },
        }
        .to_json();

        assert_eq!(
            preview,
            json!({
                "source": "world-markets-contract",
                "chain_id": 2092151908u64,
                "exchange": "0xf6b54e033bb45a583aa642924bcef78b804588ae",
                "block_number": 12345,
                "access": {
                    "account_id": 1577,
                    "actor": "0x5555555555555555555555555555555555555555",
                    "authorization": "delegated_trader",
                },
                "preview": {
                    "account_id": 1577,
                    "product": "spot",
                    "side": "sell",
                    "base": {
                        "token_id": 4,
                        "symbol": "WETH",
                        "erc20_address": "0x2222222222222222222222222222222222222222",
                        "position_decimals": 4,
                    },
                    "quote": {
                        "token_id": 1,
                        "symbol": "USDT",
                        "erc20_address": "0x3333333333333333333333333333333333333333",
                        "position_decimals": 2,
                    },
                    "order_book": "0x6666666666666666666666666666666666666666",
                    "quantity": "0.01",
                    "quantity_raw": "100",
                    "resolved_size": {
                        "input": "0.01",
                        "denomination": "base",
                        "mark": "2465.71",
                        "base_qty": "0.01",
                        "notional": "24.6571",
                        "notional_rendered": "`$24.66`",
                    },
                    "mark_price": "2465.71",
                    "mark_price_raw": 7890274,
                    "order_type": "market",
                    "limit_price": "2453.38145",
                    "slippage": "0.005",
                    "estimated_notional": "24.6571",
                    "current_position_quantity": "0.05",
                    "verdict": {
                        "status": "allow",
                        "rule": "mandate_v1",
                        "detail": "Mandate v1 permits spot sell 0.01 WETH.",
                    },
                    "executable": false,
                },
            })
        );
    }

    #[test]
    fn preview_fails_closed_without_account_context() {
        let app = WorldMarketsApp::default();
        let err = app
            .trade_preview(
                WorldTradeArgs {
                    product: "perp".to_string(),
                    side: "buy".to_string(),
                    base_symbol: "WETH".to_string(),
                    quote_symbol: "USDT".to_string(),
                    quantity: "0.1".to_string(),
                    ..WorldTradeArgs::default()
                },
                &ctx_with(json!({ "domain": { "evm": { "chain_id": 1 } } })),
            )
            .err()
            .expect("foreign chain fails closed");
        assert!(err.contains("not the World chain"), "{err}");
    }

    #[test]
    fn compute_resize_carries_floor_and_rule() {
        let app = app_with_store(vec![(TRADER, 21, Some(mandate_json(None)))]);
        let value = ComputeResize::run(
            &app,
            ComputeResizeArgs {
                rule: "portfolio_floor".to_string(),
            },
            trader_ctx(),
        )
        .unwrap();
        assert_eq!(value["resize"]["rule"], "portfolio_floor");
        assert_eq!(value["resize"]["floor"]["value"], "6000");
        assert_eq!(value["executable"], false);

        let missing = ComputeResize::run(
            &app,
            ComputeResizeArgs {
                rule: "portfolio_floor".to_string(),
            },
            ctx_with(json!({})),
        )
        .unwrap_err();
        assert!(missing.contains("missing_mandate"), "{missing}");
    }

    #[test]
    fn allowed_preview_stages_the_matching_venue_call_from_its_own_fields() {
        let preview = snapshot_preview("limit", Some("2460"));
        let staged = WorldMarketsApp::stage_new_order(&preview).unwrap();
        assert_eq!(staged.signature, "newSpotSellOrder(address,uint256)");
        assert_eq!(staged.exchange, preview.exchange);
        assert_eq!(staged.chain_id, 2092151908);
        assert_eq!(staged.args[0], preview.order_book);
        // account 1577, quantity 100, limit 2460 packed as a resting limit order
        let word = OrderWord {
            account_id: 1577,
            quantity_raw: 100,
            limit_price: Decimal::from(2460),
            order_type: OrderType::Limit,
            insertion_hint: 0,
        }
        .pack()
        .unwrap();
        assert_eq!(staged.args[1], format!("0x{word:064x}"));
        assert!(staged.description.contains("limit 2460 USDT (limit)"));

        // A market ask carries the slippage-moved limit as an immediate-or-cancel word.
        let market = snapshot_preview("market", None);
        let staged = WorldMarketsApp::stage_new_order(&market).unwrap();
        let word = OrderWord {
            account_id: 1577,
            quantity_raw: 100,
            limit_price: market.intent.limit_price,
            order_type: OrderType::FillPartialKillRest,
            insertion_hint: 0,
        }
        .pack()
        .unwrap();
        assert_eq!(staged.args[1], format!("0x{word:064x}"));

        let routed = staged.routed(market.to_json());
        assert_eq!(routed.routes[0].tool, "evm_stage_tx");
        assert_eq!(
            routed.value["staged"]["signature"],
            "newSpotSellOrder(address,uint256)"
        );
        assert_eq!(routed.value["preview"]["verdict"]["status"], "allow");
    }

    fn snapshot_preview(order_type: &str, price: Option<&str>) -> TradePreview {
        let weth = Asset {
            token_id: 4,
            symbol: "WETH".to_string(),
            name: "Wrapped Ether".to_string(),
            token_type: "erc20".to_string(),
            erc20_address: "0x2222222222222222222222222222222222222222".to_string(),
            erc20_decimals: 18,
            vault_decimals: 8,
            position_decimals: 4,
            risk_price_percent: 10,
            risk_slippage_percent: 1.0,
        };
        let usdt = Asset {
            token_id: 1,
            symbol: "USDT".to_string(),
            name: "Tether".to_string(),
            token_type: "erc20".to_string(),
            erc20_address: "0x3333333333333333333333333333333333333333".to_string(),
            erc20_decimals: 6,
            vault_decimals: 6,
            position_decimals: 2,
            risk_price_percent: 0,
            risk_slippage_percent: 0.0,
        };
        let mark = Decimal::from_str("2465.71").unwrap();
        let quantity = Decimal::from_str("0.01").unwrap();
        TradePreview {
            chain_id: 2092151908,
            exchange: "0xf6b54e033bb45a583aa642924bcef78b804588ae".to_string(),
            block_number: 12345,
            access: AccountAccess {
                account_id: 1577,
                owner: "0x4444444444444444444444444444444444444444".to_string(),
                actor: "0x5555555555555555555555555555555555555555".to_string(),
                authorization: "delegated_trader".to_string(),

                mandate: None,
            },
            product: "spot",
            side: "sell".to_string(),
            base: weth,
            quote: usdt,
            order_book: "0x6666666666666666666666666666666666666666".to_string(),
            quantity,
            quantity_raw: 100,
            resolved_size: SizeInput {
                sentence: Some("sell 0.01 WETH spot"),
                instrument: Some("WETH"),
                ..SizeInput::default()
            }
            .resolve(mark)
            .unwrap(),
            mark_price: "2465.71".to_string(),
            mark_price_raw: 7890274,
            intent: OrderIntent::resolve(
                Some(order_type),
                price.map(|p| Decimal::from_str(p).unwrap()),
                None,
                "sell",
                mark,
            )
            .unwrap(),
            book_rate: None,
            rate_raw: None,
            estimated_notional: quantity * mark,
            current_position_quantity: Decimal::from_str("0.05").unwrap(),
            verdict: Verdict {
                status: "allow",
                rule: "mandate_v1",
                detail: "Mandate v1 permits spot sell 0.01 WETH.".to_string(),
            },
        }
    }

    #[test]
    fn unknown_asset_is_a_cant_wall_not_a_question() {
        let value = WorldMarketsApp::unknown_asset("DOGE");
        assert_eq!(value["error"], "unknown_asset");
        assert_eq!(
            value["message"],
            "I can't trade `DOGE` — it isn't listed on World."
        );
        assert_eq!(value["reply_verbatim"], true);
        assert_eq!(value["executable"], false);
    }

    #[test]
    fn lend_side_and_product_normalise_and_a_lend_preview_stages_the_lend_word() {
        assert_eq!(WorldMarketsApp::order_product("lending").unwrap(), "lend");
        assert_eq!(
            WorldMarketsApp::order_side("lend", "supply").unwrap(),
            "lend"
        );
        assert_eq!(
            WorldMarketsApp::order_side("lend", "buy").unwrap(),
            "borrow"
        );
        assert!(WorldMarketsApp::order_side("lend", "long").is_err());
        assert_eq!(
            WorldMarketsApp::order_side("perp", "short").unwrap(),
            "sell"
        );

        let mut preview = snapshot_preview("limit", Some("0.05"));
        preview.product = "lend";
        preview.side = "lend".to_string();
        preview.rate_raw = Some(LendOrderWord::encode_rate(preview.intent.limit_price).unwrap());
        preview.book_rate = Some(Decimal::from_str("0.052").unwrap());
        let json = preview.to_json();
        assert_eq!(json["preview"]["interest_rate"], "0.05");
        assert_eq!(json["preview"]["interest_rate_raw"], 500);
        assert_eq!(json["preview"]["book_rate"], "0.052");
        assert!(json["preview"].get("limit_price").is_none());

        let staged = WorldMarketsApp::stage_new_order(&preview).unwrap();
        assert_eq!(staged.signature, "newLendOrder(address,uint256)");
        let word = LendOrderWord {
            account_id: 1577,
            quantity_raw: 100,
            rate_raw: 500,
            order_type: OrderType::Limit,
        }
        .pack()
        .unwrap();
        assert_eq!(staged.args[1], format!("0x{word:064x}"));
        assert!(staged.description.contains("lend 0.01 WETH at 0.05 APR"));
    }

    #[test]
    #[ignore = "requires live UniFi RPC"]
    fn live_preview_uses_actor_account_and_mandate_context() {
        let app = WorldMarketsApp::default();
        let account_id = 1_577;
        let owner = app.client.owner_for(account_id).unwrap();
        let ctx = ctx_with(json!({
            "domain": { "evm": { "address": format!("{owner:#x}"), "chain_id": 2092151908u64 } },
            "handover": { "account_ref": account_id, "mandate": mandate_json(Some(account_id)) }
        }));
        let value = app
            .trade_preview(
                WorldTradeArgs {
                    product: "perp".to_string(),
                    side: "buy".to_string(),
                    base_symbol: "WETH".to_string(),
                    quote_symbol: "USDT".to_string(),
                    quantity: "0.01".to_string(),
                    ..WorldTradeArgs::default()
                },
                &ctx,
            )
            .unwrap()
            .to_json();
        assert_eq!(value["access"]["authorization"], "owner");
        assert_eq!(value["preview"]["verdict"]["status"], "deny");
        assert_eq!(value["preview"]["verdict"]["rule"], "portfolio_floor");
        assert_eq!(value["preview"]["executable"], false);
    }
}
