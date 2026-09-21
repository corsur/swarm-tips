//! Public reputation lookup: validate before I/O and distinguish absent records
//! from unavailable storage or chain reads.
use axum::{http::StatusCode, response::IntoResponse, Json};
use firestore::FirestoreDb;
use std::{collections::HashMap, sync::Arc};

fn wallet_address(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    let native = if raw.contains(':') {
        let account = chain_core::AccountId::parse(raw)
            .map_err(|_| "Enter a valid Solana or EVM wallet address")?;
        let evm = account.chain().namespace() == chain_core::Namespace::Eip155;
        if evm != account.address().starts_with("0x") {
            return Err("Wallet address does not match its chain family");
        }
        account.address().to_owned()
    } else {
        raw.to_owned()
    };
    if native.starts_with("0x") {
        crate::xchain::evm_account_id(&native).map_err(|_| "Enter a valid EVM wallet address")?;
        return Ok(native.to_lowercase());
    }
    native
        .parse::<solana_sdk::pubkey::Pubkey>()
        .map_err(|_| "Enter a valid Solana or EVM wallet address")?;
    Ok(native)
}

fn network(raw: Option<&String>) -> Result<&str, &'static str> {
    match raw.map(String::as_str) {
        None | Some("mainnet") => Ok("mainnet"),
        Some("devnet") => Ok("devnet"),
        _ => Err("network must be mainnet or devnet"),
    }
}

fn response(status: StatusCode, body: serde_json::Value) -> axum::response::Response {
    (status, [("Access-Control-Allow-Origin", "*")], Json(body)).into_response()
}

pub fn handler(
    db: Arc<FirestoreDb>,
    mainnet: String,
    devnet: String,
) -> axum::routing::MethodRouter {
    let client = reqwest::Client::new();
    axum::routing::get(
        move |axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>| {
            let db = Arc::clone(&db);
            let client = client.clone();
            let mainnet = mainnet.clone();
            let devnet = devnet.clone();
            async move {
                let wallet =
                    match wallet_address(query.get("wallet").map(String::as_str).unwrap_or("")) {
                        Ok(wallet) => wallet,
                        Err(error) => {
                            return response(
                                StatusCode::BAD_REQUEST,
                                serde_json::json!({"error":error}),
                            )
                        }
                    };
                let rpc = match network(query.get("network")) {
                    Ok("mainnet") => &mainnet,
                    Ok(_) => &devnet,
                    Err(error) => {
                        return response(
                            StatusCode::BAD_REQUEST,
                            serde_json::json!({"error":error}),
                        )
                    }
                };
                let result = async {
                let (web_position, extensions_received) = if wallet.starts_with("0x") { (None,0) } else {
                    crate::web_position::try_agent_web_position(&client,rpc,&wallet).await?
                };
                let eigentrust = crate::reputation::try_get_agent_reputation(&db,&wallet).await?;
                Ok::<_,anyhow::Error>(serde_json::json!({"wallet":wallet,"web_position":web_position,
                    "extensions_received":extensions_received,"has_standing":web_position.is_some() && extensions_received >= 1,"eigentrust":eigentrust}))
            }.await;
                match result {
                    Ok(body) => response(StatusCode::OK, body),
                    Err(error) => {
                        tracing::warn!(%error,"reputation lookup unavailable");
                        response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            serde_json::json!({"error":"Reputation is temporarily unavailable. Please retry."}),
                        )
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_wallets_and_unknown_networks_are_rejected_before_reads() {
        for raw in [
            "",
            "nonsense",
            "0x123",
            "eip155:84532:bad",
            "solana:mainnet:0x1234567890abcdef1234567890abcdef12345678",
            "<script>",
            "../wallet",
        ] {
            assert!(wallet_address(raw).is_err(), "{raw}");
        }
        assert!(network(Some(&"staging".into())).is_err());
        assert_eq!(network(None).unwrap(), "mainnet");
    }
    #[test]
    fn valid_wallets_are_normalized_without_conflating_chain_families() {
        let sol = "11111111111111111111111111111111";
        assert_eq!(wallet_address(sol).unwrap(), sol);
        let evm = "0x1234567890AbCdEf1234567890AbCdEf12345678";
        assert_eq!(wallet_address(evm).unwrap(), evm.to_lowercase());
        assert_eq!(
            wallet_address(&format!("eip155:84532:{evm}")).unwrap(),
            evm.to_lowercase()
        );
    }
}
