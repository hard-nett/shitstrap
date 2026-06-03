use cw_orch::daemon::networks::{OSMOSIS_1, TERP_MAINNET};
use cw_orch::prelude::*;
use cw_orch_interchain::prelude::*;
use shit_scripts::{CwShitstrapSuite, CwShitstrapSuiteDeployData, ShitInitMsg, UncheckedDenom};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut interchain =
        DaemonInterchain::new(vec![TERP_MAINNET, OSMOSIS_1], &ChannelCreationValidator)?;

    let terp = interchain.get_chain("terp")?;
    let osmo = interchain.get_chain("localosmosis")?;

    // define accepted ibc denoms
    let akt = ibc_denom_hash("channel-0", "transfer", "uthiol");
    let btc = ibc_denom_hash("channel-0", "transfer", "uthiol");
    let atone = ibc_denom_hash("channel-0", "transfer", "uthiol");
    let photon = ibc_denom_hash("channel-0", "transfer", "uthiol");
    let ibc_denoms = vec![akt, btc, atone, photon];

    let dd = new_default_ibc_set(terp.sender_addr(), ibc_denoms);
    let shit = CwShitstrapSuite::deploy_on(terp.clone(), Some(dd))?;

    // create shitstraps with ibc denoms
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
