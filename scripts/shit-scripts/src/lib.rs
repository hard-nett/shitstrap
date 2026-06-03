mod e2e;

use cosmwasm_std::Decimal;
use cw_orch::prelude::*;
use cw_shitstrap::contract::interface::CwShitstrap;
use cw_shitstrap_factory::{interface::CwShitstrapFactory, msg::ExecuteMsgFns as _};

pub use cw_shit_denom::UncheckedDenom;
pub use cw_shitstrap::contract::msg::InstantiateMsg as ShitInitMsg;
pub use cw_shitstrap_factory::msg::InstantiateMsg as ShitFactoryInitMsg;
#[derive(Clone, Debug, Default)]
pub struct CwShitstrapSuiteDeployData {
    pub shit: Vec<ShitInitMsg>,
    pub admin: Option<Addr>,
}

pub struct CwShitstrapSuite<Chain> {
    pub chain: Chain,
    pub shitstrap: CwShitstrap<Chain>,
    pub factory: CwShitstrapFactory<Chain>,
}

impl<Chain: CwEnv> CwShitstrapSuite<Chain> {
    pub fn new(chain: Chain) -> CwShitstrapSuite<Chain> {
        CwShitstrapSuite::<Chain> {
            chain: chain.clone(),
            shitstrap: CwShitstrap::new(chain.clone()),
            factory: CwShitstrapFactory::new(chain.clone()),
        }
    }
    pub fn upload(&self) -> Result<(), CwOrchError> {
        self.shitstrap.upload()?;
        self.factory.upload()?;
        Ok(())
    }
}

impl<Chain: CwEnv> cw_orch::contract::Deploy<Chain> for CwShitstrapSuite<Chain> {
    type Error = CwOrchError;
    type DeployData = Option<CwShitstrapSuiteDeployData>;

    fn store_on(chain: Chain) -> Result<Self, Self::Error> {
        let suite = CwShitstrapSuite::new(chain.clone());
        suite.upload()?;
        Ok(suite)
    }

    fn get_contracts_mut(&mut self) -> Vec<Box<&mut dyn ContractInstance<Chain>>> {
        vec![Box::new(&mut self.shitstrap), Box::new(&mut self.factory)]
    }

    fn load_from(_chain: Chain) -> Result<Self, Self::Error> {
        todo!()
    }

    fn deploy_on(chain: Chain, data: Self::DeployData) -> Result<Self, Self::Error> {
        let mut suite = CwShitstrapSuite::store_on(chain.clone())?;
        match data {
            Some(d) => {
                // Always create a factory on deploy
                suite.factory.instantiate(
                    &ShitFactoryInitMsg {
                        owner: d
                            .admin
                            .as_ref()
                            .map(|d| Some(d.to_string()))
                            .unwrap_or_default(),
                        shitstrap_id: suite.shitstrap.code_id()?,
                    },
                    d.admin.as_ref(),
                    &[],
                )?;

                for (i, init) in d.shit.iter().enumerate() {
                    let res = suite
                        .factory
                        .create_native_shit_strap_contract(init.clone(), format!("{i}",))?;
                }

                Ok(suite)
            }
            None => Err(CwOrchError::NotImplemented {}),
        }
    }
}

// ---------------------------------------------------------------------------
// shitstrap deploy data builders
// ---------------------------------------------------------------------------

/// Single shitstrap: 20 THIOL in -> 1 TERP out.
pub fn shit_deploy_data_single(admin: Addr) -> Option<CwShitstrapSuiteDeployData> {
    let mut dd = CwShitstrapSuiteDeployData::default();
    dd.admin = Some(admin.clone());
    dd.shit = vec![ShitInitMsg {
        daos: Vec::new(),
        owner: Some(admin.to_string()),
        accepted: vec![cw_shit_denom::PossibleShit::native_denom(
            "uthiol",
            50_000_000_000_000_000u128,
        )],
        cutoff: 500_000_000_000u128.into(),
        shitmos: UncheckedDenom::Native("uterp".into()),
        title: "terp".into(),
        description: "terp".into(),
    }];
    Some(dd)
}

/// Multiple shistraps with spot-price-based exchange rates.
pub fn shit_deploy_data_full(admin: Addr) -> Option<CwShitstrapSuiteDeployData> {
    let mut dd = CwShitstrapSuiteDeployData::default();
    let cut = 710_000_000_000u128;

    let mut shit = Vec::new();

    // atom @ $1.81
    shit.push(build_shit_init(
        &admin,
        &tf_denom(&admin, "atom"),
        calc_rates(Decimal::from_ratio(181u128, 100u128)),
        cut,
        "atom",
    ));

    // btc @ $66,350
    shit.push(build_shit_init(
        &admin,
        &tf_denom(&admin, "btc"),
        calc_rates(Decimal::from_ratio(66350u128, 1u128)),
        cut,
        "btc",
    ));

    // akt @ $0.29
    shit.push(build_shit_init(
        &admin,
        &tf_denom(&admin, "akt"),
        calc_rates(Decimal::from_ratio(29u128, 100u128)),
        cut,
        "akt",
    ));

    // um @ $0.007
    shit.push(build_shit_init(
        &admin,
        &tf_denom(&admin, "um"),
        calc_rates(Decimal::from_ratio(7u128, 1000u128)),
        cut,
        "um",
    ));

    // eth @ $1,956.12
    shit.push(build_shit_init(
        &admin,
        &tf_denom(&admin, "eth"),
        calc_rates(Decimal::from_ratio(195612u128, 100u128)),
        cut,
        "eth",
    ));

    dd.admin = Some(admin);
    dd.shit = shit;
    Some(dd)
}

pub fn calc_rates(price: Decimal) -> u128 {
    price.atomics().u128() * 100
}

pub fn tf_denom(creator: &Addr, subdenom: &str) -> String {
    format!("factory/{}/{}", creator, subdenom)
}

pub fn build_shit_init(
    admin: &Addr,
    native_denom: &str,
    shit_rate: u128,
    cutoff: u128,
    title: &str,
) -> ShitInitMsg {
    ShitInitMsg {
        daos: Vec::new(),
        owner: Some(admin.to_string()),
        accepted: vec![cw_shit_denom::PossibleShit::native_denom(
            native_denom,
            shit_rate,
        )],
        cutoff: cutoff.into(),
        shitmos: UncheckedDenom::Native("uthiol".into()),
        title: title.into(),
        description: title.into(),
    }
}
