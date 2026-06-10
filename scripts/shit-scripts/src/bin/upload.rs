use cw_orch::prelude::*;
use shit_scripts::{CwShitstrapSuite, CwShitstrapSuiteDeployData, ShitInitMsg, UncheckedDenom};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    dotenv::dotenv().ok();
    let terp = Daemon::builder(networks::TERP_MAINNET).build()?;
    let shit = CwShitstrapSuite::new(terp.clone());
    shit.upload()?;

    let id = terp.state().get_all_code_ids()?;
    println!("{:#?}", id);

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

use cw_shitstrap::PossibleShit;

pub const DAB: u128 = 1_000_000_000_000_000u128;
pub const CUTOFF: u128 = 500_000_000_000u128;

pub fn new_default_ibc_set(admin: Addr, denoms: Vec<String>) -> CwShitstrapSuiteDeployData {
    CwShitstrapSuiteDeployData {
        admin: Some(admin.clone()),
        shit: vec![ShitInitMsg {
            daos: vec![],
            owner: Some(admin.to_string()),
            accepted: vec![PossibleShit::native_denom("uthiol", DAB)],
            cutoff: CUTOFF.into(),
            shitmos: UncheckedDenom::Native("uterp".into()),
            title: "terp".into(),
            description: "terp".into(),
        }],
    }
}
