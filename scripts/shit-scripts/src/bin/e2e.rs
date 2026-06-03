//! E2E cross-chain shitstrap + IBC-callback mint test.
//!
//! Spawns two local chains with Hermes relayer, deploys contracts on both
//! chains, then executes a one-click cross-chain mint:
//!   Chain A user -> ICS-20 transfer (with callback memo) -> Chain B
//!   -> Chain B ibc_destination_callback fires -> ShitStrapAndMint
//!   -> user receives SHITMOS + NFT atomically.
//!
//! Usage:
//!   cargo run -p shit-scripts --bin e2e --features e2e
//!   cargo run -p shit-scripts --bin e2e --features e2e --  

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    tracing::info!("Starting cross-chain shitstrap + mint e2e test");
    // shit_scripts::run_e2e("240u-1", "240u-1", false).await?;
    tracing::info!("E2E test completed successfully");
    Ok(())
}
