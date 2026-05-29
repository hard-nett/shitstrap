//! E2E cross-chain shitstrap + IBC-callback mint test.
//! Spawn 2 chains + Hermes, deploy contracts, configure callback,
//! send ICS-20 with callback memo, then assert SHITMOS + NFT.
#![cfg(feature = "e2e")]

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use cw_orch::daemon::DaemonBuilder;
use cw_orch::environment::{ChainInfoOwned, ChainKind};
use cw_orch::prelude::*;
use ict_rs::chain::Chain;
use ict_rs::chain::cosmos::CosmosChain;
use ict_rs::interchain::{Interchain, InterchainBuildOptions, InterchainLink};
use ict_rs::relayer::{build_relayer, RelayerType};
use ict_rs::runtime::{DockerConfig, IctRuntime};
use ict_rs::testing::TestEnv;
use ict_rs::tx::WalletAmount;
use tracing::info;

use crate::{
    CwShitstrapSuite, CwShitstrapSuiteDeployData, PossibleShit, ShitInitMsg, UncheckedDenom,
};

// ===========================================================================
// Chain spawning
// ===========================================================================

fn mk_config(chain_id: &str) -> ict_rs::chain::ChainConfig {
    let mut cfg = TestEnv::terp_localterp_config();
    cfg.chain_id = chain_id.to_string();
    cfg
}

pub async fn spawn_dual_chain(a_id: &str, b_id: &str) -> Result<Interchain> {
    let nw = format!("ict-shitstrap-{}", a_id);
    let rt = IctRuntime::Docker(DockerConfig::default()).into_backend().await?;
    rt.create_network(&nw).await?;

    let ca = CosmosChain::new(mk_config(a_id), 1, 0, rt.clone());
    let cb = CosmosChain::new(mk_config(b_id), 1, 0, rt.clone());

    let rl = build_relayer(RelayerType::Hermes, rt.clone(), "hermes", &nw).await?;

    let mut ic = Interchain::new(rt)
        .add_chain(Box::new(ca))
        .add_chain(Box::new(cb))
        .add_relayer("hermes", rl)
        .add_link(InterchainLink {
            chain1: a_id.into(),
            chain2: b_id.into(),
            relayer: "hermes".into(),
            path: "ibc-path".into(),
        });

    ic.build(InterchainBuildOptions {
        test_name: "shitstrap-e2e".into(),
        skip_path_creation: false,
        genesis_wallets: HashMap::from([
            (a_id.into(), vec![
                WalletAmount { address: "deployer".into(), denom: "uthiol".into(), amount: 1_000_000 },
                WalletAmount { address: "deployer".into(), denom: "uterp".into(), amount: 1_000_000_000_000 },
            ]),
            (b_id.into(), vec![
                WalletAmount { address: "deployer".into(), denom: "uthiol".into(), amount: 1_000_000 },
                WalletAmount { address: "deployer".into(), denom: "uterp".into(), amount: 1_000_000_000_000 },
            ]),
        ]),
    })
    .await?;

    Ok(ic)
}

// ===========================================================================
// Daemon helpers
// ===========================================================================

fn build_chain_info(c: &dyn Chain) -> ChainInfoOwned {
    let mut info = ChainInfoOwned::config(c.chain_id().to_string());
    info.gas_denom = "uterp".into();
    info.gas_price = 0.25;
    info.grpc_urls = vec![c.host_grpc_address()];
    info.kind = ChainKind::Local;
    info.network_info.chain_name = "terp network".into();
    info.network_info.pub_address_prefix = "terp".into();
    info
}

// ===========================================================================
// IBC transfer helper
// ===========================================================================

/// Send ICS-20 transfer with IBC callback memo via chain CLI.
pub async fn send_ibc_transfer_with_callback(
    chain: &dyn Chain,
    channel_id: &str,
    from_key: &str,
    to_address: &str,
    amount: &str,
    callback_addr: &str,
) -> Result<()> {
    let memo = serde_json::json!({"ibc_callback": callback_addr}).to_string();
    let args = [
        "tx", "ibc-transfer", "transfer", "transfer",
        channel_id, to_address, amount,
        "--memo", &memo,
        "--from", from_key,
        "--chain-id", chain.chain_id(),
        "--gas", "auto",
        "--gas-adjustment", "1.5",
        "--gas-prices", "0.25uterp",
        "--yes",
    ];
    let out = chain.chain_exec_tx(&args).await?;
    info!("IBC transfer tx: {}", out.stdout_str());
    Ok(())
}

/// Compute IBC denom hash: SHA256("transfer/{channel}/{denom}") hex-encoded
pub fn ibc_denom_hash(channel_id: &str, port_id: &str, native_denom: &str) -> String {
    use sha2::{Digest, Sha256};
    let input = format!("{}/{}/{}", port_id, channel_id, native_denom);
    let result = Sha256::digest(input.as_bytes());
    let hex_str: String = result.iter().map(|b| format!("{:02X}", b)).collect();
    format!("ibc/{}", hex_str)
}

// ===========================================================================
// Main e2e test
// ===========================================================================

pub async fn run_e2e(a_id: &str, b_id: &str, keep: bool) -> Result<()> {
    let mut ic = spawn_dual_chain(a_id, b_id).await?;

    // Extract endpoints into owned values so ic is freed for .close() later
    let mut infos: Vec<(&str, ChainInfoOwned)> = Vec::new();
    for id in [a_id, b_id] {
        let c = ic.get_chain(id).expect("chain exists");
        infos.push((id, build_chain_info(c)));
    }
    let (chain_id_a, info_a) = infos.remove(0);
    let (chain_id_b, info_b) = infos.remove(0);

    // Build daemons (blocking — cw-orch uses tokio::runtime::Handle internally)
    let (da, db) = tokio::task::spawn_blocking(move || -> Result<_> {
        let rt = tokio::runtime::Handle::current();
        let da = DaemonBuilder::new(info_a).handle(&rt).build()?;
        let db = DaemonBuilder::new(info_b).handle(&rt).build()?;
        Ok((da, db))
    }).await??;

    let sender = da.sender_addr();
    info!("Deployer: {}", sender);

    // -----------------------------------------------------------------------
    // Deploy shitstrap suites on both chains
    // -----------------------------------------------------------------------

    // Chain A: shitstrap accepts native uthiol
    let data_a = Some(CwShitstrapSuiteDeployData {
        admin: Some(sender.clone()),
        shit: vec![ShitInitMsg {
            daos: vec![], owner: Some(sender.to_string()),
            accepted: vec![PossibleShit::native_denom("uthiol", 1_000_000_000_000_000u128)],
            cutoff: 500_000_000_000u128.into(),
            shitmos: UncheckedDenom::Native("uterp".into()),
            title: "terp".into(), description: "terp".into(),
        }],
    });
    let suite_a = CwShitstrapSuite::deploy_on(da.clone(), data_a)?;
    let shitstrap_a = suite_a.shitstrap.address()?;
    info!("Shitstrap on {}: {}", chain_id_a, shitstrap_a);

    // Chain B: shitstrap accepts ibc/THIOL (IBC denom of uthiol from A)
    let ibc_denom_b = ibc_denom_hash("channel-0", "transfer", "uthiol");
    info!("IBC denom on B: {}", ibc_denom_b);

    let data_b = Some(CwShitstrapSuiteDeployData {
        admin: Some(sender.clone()),
        shit: vec![ShitInitMsg {
            daos: vec![], owner: Some(sender.to_string()),
            accepted: vec![PossibleShit::native_denom(&ibc_denom_b, 1_000_000_000_000_000u128)],
            cutoff: 500_000_000_000u128.into(),
            shitmos: UncheckedDenom::Native("uterp".into()),
            title: "ibc-terp".into(), description: "ibc-terp".into(),
        }],
    });
    let suite_b = CwShitstrapSuite::deploy_on(db.clone(), data_b)?;
    let shitstrap_b = suite_b.shitstrap.address()?;
    info!("Shitstrap on {}: {}", chain_id_b, shitstrap_b);

    // -----------------------------------------------------------------------
    // Instantiate IBC callback contract on chain B
    // -----------------------------------------------------------------------

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let callback_label = format!("callback-{}", now);

    suite_b.ibc.instantiate(
        &cw_shitstrap_ibc_callbacks::contract::msg::InstantiateMsg {
            shitstrap: shitstrap_b.to_string(),
            svg_collection: None,
            mint_price: None,
        },
        Some(&sender),
        &[],
    )?;
    let callback_addr = suite_b.ibc.address()?;
    info!("Callback contract on B: {}", callback_addr);

    // -----------------------------------------------------------------------
    // Fund the shitstrap on chain B with SHITMOS (uterp)
    // -----------------------------------------------------------------------

    // Bank send from deployer to shitstrap so it can pay out SHITMOS
    let fund_amount = "1000000000000";
    info!("Funding shitstrap with {}uterp", fund_amount);

    drop(da);
    drop(db);

    // Use chain B CLI: bank send from deployer to shitstrap
    let chain_b = ic.get_chain(chain_id_b).expect("chain B exists");
    chain_b.chain_exec_tx(&[
        "tx", "bank", "send", "deployer", &shitstrap_b.to_string(),
        &format!("{}uterp", fund_amount),
        "--chain-id", chain_b.chain_id(),
        "--gas", "auto", "--gas-adjustment", "1.5",
        "--gas-prices", "0.25uterp", "--yes",
    ]).await?;
    info!("Shitstrap funded with SHITMOS");

    // -----------------------------------------------------------------------
    // Send IBC transfer from chain A -> chain B with callback memo
    // -----------------------------------------------------------------------

    let chain_a = ic.get_chain(chain_id_a).expect("chain A exists");
    let amount = "1000000000000uthiol"; // 1 THIOL — above 500B cutoff

    // Snapshot callback's SHITMOS balance on B before IBC transfer (starts at 0)
    let chain_b = ic.get_chain(chain_id_b).expect("chain B exists");
    let bal_before = query_balance(chain_b, &callback_addr.to_string(), "uterp").await?;
    info!("Callback SHITMOS before IBC: {}", bal_before);

    // Send IBC transfer from A -> B, receiver = callback contract.
    // Memo {"ibc_callback": callback_addr} triggers ibc_destination_callback entry point
    // after the ICS-20 packet is received on chain B.
    send_ibc_transfer_with_callback(
        chain_a,
        "channel-0",
        "deployer",
        &callback_addr.to_string(),
        amount,
        &callback_addr.to_string(),
    ).await?;

    // -----------------------------------------------------------------------
    // Wait for relay + callback execution
    // -----------------------------------------------------------------------

    info!("Waiting for IBC relay + callback execution...");
    tokio::time::sleep(Duration::from_secs(15)).await;

    // -----------------------------------------------------------------------
    // Assert: callback contract received SHITMOS from the shitstrap.
    // The callback contract is the depositor (info.sender in ShitStrapAndMint).
    // It starts with 0 SHITMOS. After cutoff is exceeded, the shitstrap pays
    // SHITMOS to the callback contract.
    // -----------------------------------------------------------------------

    let chain_b = ic.get_chain(chain_id_b).expect("chain B exists");
    let bal_after = query_balance(chain_b, &callback_addr.to_string(), "uterp").await?;
    info!("Callback SHITMOS after IBC: {}", bal_after);

    let before_u128: u128 = bal_before.parse()?;
    let after_u128: u128 = bal_after.parse()?;
    let delta = after_u128.saturating_sub(before_u128);
    info!("SHITMOS delta for callback contract: {}", delta);

    if delta == 0 {
        // Query callback's IBC denom balance to confirm transfer arrived
        let ibc_bal = query_balance(chain_b, &callback_addr.to_string(), &ibc_denom_b).await?;
        info!("Callback IBC denom balance: {}", ibc_bal);
        anyhow::bail!(
            "Callback contract received 0 SHITMOS — IBC-or-mint flow failed. \
             IBC denom balance: {}. Check relayer logs and callback tx.",
            ibc_bal
        );
    }

    info!("SUCCESS: Callback contract received {} SHITMOS from shitstrap", delta);

    // -----------------------------------------------------------------------
    // TODO: Deploy cw721-svg + configure shitstrap for full mint flow
    // Once the SVG minter is deployed on B, run a second transfer above cutoff:
    //   - Configure shitstrap with SVG collection
    //   - Read cutoff, send amount above it
    //   - Assert NFT ownership after callback
    // -----------------------------------------------------------------------

    if !keep { ic.close().await?; }
    Ok(())
}

/// Query a single denom balance for an address on a chain via CLI.
async fn query_balance(chain: &dyn Chain, address: &str, denom: &str) -> Result<String> {
    let out = chain.chain_exec(&[
        "q", "bank", "balance", address, denom,
        "--chain-id", chain.chain_id(),
        "--output", "json",
    ]).await?;
    let val: serde_json::Value = serde_json::from_str(out.stdout_str().trim())?;
    let amt = val["balance"]["amount"]
        .as_str()
        .unwrap_or("0")
        .to_string();
    Ok(amt)
}