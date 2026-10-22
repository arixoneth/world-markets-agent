//! World's trader store: which World account a permitted trader trades, and
//! the mandate its owner set. World's authorize page writes the record once
//! `allowTradingForAccount(N, K)` confirms on chain and deletes it on revoke;
//! the chain stays the authority (`resolve_account` checks the grant again).
//!
//! `GET {WORLD_STORE_URL}/v1/traders/{trader}?chain_id={id}` answers
//! `200 {"account_id": N, "mandate": {..} | null}`, or `404` when the trader
//! isn't authorized, and the tool replies with the authorize link.
use aomi_sdk::{DynToolCallCtx, Secret};
use serde::Deserialize;
use serde_json::Value;

/// The store's API key, held as an app secret.
pub(crate) const STORE_API_KEY: Secret = Secret::new(
    "WORLD_STORE_API_KEY",
    "API key for World's trader store (which account each agent wallet may trade).",
    true,
);

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TraderRecord {
    pub(crate) account_id: u64,
    #[serde(default)]
    pub(crate) mandate: Option<Value>,
}

/// Where trader records come from: World's store, or fixed ones in tests.
#[derive(Clone, Default)]
pub(crate) enum TraderStore {
    #[default]
    Http,
    #[cfg(test)]
    Fixed(std::sync::Arc<Vec<(String, TraderRecord)>>),
}

impl TraderStore {
    /// The record for `trader` on `chain_id`, or `None` when World has none:
    /// the owner hasn't authorized this wallet yet (or revoked it).
    pub(crate) fn trader(
        &self,
        ctx: &DynToolCallCtx,
        trader: &str,
        chain_id: u64,
    ) -> Result<Option<TraderRecord>, String> {
        match self {
            Self::Http => http(ctx, trader, chain_id),
            #[cfg(test)]
            Self::Fixed(records) => Ok(records
                .iter()
                .find(|(address, _)| address.eq_ignore_ascii_case(trader))
                .map(|(_, record)| record.clone())),
        }
    }
}

fn http(ctx: &DynToolCallCtx, trader: &str, chain_id: u64) -> Result<Option<TraderRecord>, String> {
    let base = std::env::var("WORLD_STORE_URL")
        .map_err(|_| "[world-markets] WORLD_STORE_URL is not configured".to_string())?;
    let key = aomi_sdk::resolve_secret_value(
        ctx,
        None,
        STORE_API_KEY.name,
        "[world-markets] WORLD_STORE_API_KEY is not configured",
    )?;
    let response = reqwest::blocking::Client::new()
        .get(format!(
            "{}/v1/traders/{trader}",
            base.trim_end_matches('/')
        ))
        .query(&[("chain_id", chain_id)])
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(|_| "[world-markets] World's trader store is unreachable".to_string())?;
    match response.status().as_u16() {
        404 => Ok(None),
        200 => response.json::<TraderRecord>().map(Some).map_err(|_| {
            "[world-markets] World's trader store answered an invalid record".to_string()
        }),
        status => Err(format!(
            "[world-markets] World's trader store answered {status}"
        )),
    }
}

/// Where the account owner allows `trader` to trade (`allowTradingForAccount`).
pub(crate) fn authorize_link(trader: &str) -> Result<String, String> {
    let page = std::env::var("WORLD_AUTHORIZE_URL")
        .map_err(|_| "[world-markets] WORLD_AUTHORIZE_URL is not configured".to_string())?;
    Ok(format!("{}?trader={trader}", page.trim_end_matches('/')))
}
