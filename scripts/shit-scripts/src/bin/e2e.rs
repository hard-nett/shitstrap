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
//!   cargo run -p shit-scripts --bin e2e --features e2e -- --keep-containers

use clap::Parser;
use shit_scripts::e2e::run_e2e;

#[derive(Parser, Debug)]
#[command(name = "shitstrap-e2e", about = "Cross-chain shitstrap + mint e2e test")]
struct Args {
    /// Keep Docker containers running after test (for debugging)
    #[arg(long)]
    keep_containers: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let args = Args::parse();

    tracing::info!("Starting cross-chain shitstrap + mint e2e test");

    let chain_a_id = "120u-1";
    let chain_b_id = "120u-2";

    let keep = args.keep_containers;
    run_e2e(chain_a_id, chain_b_id, keep).await?;

    tracing::info!("E2E test completed successfully");
    Ok(())
}