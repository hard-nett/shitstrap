use cw_orch::{anyhow, prelude::*};
use cw_shitstrap::{
    interface::CwShitstrap,
    msg::{AsyncQueryMsgFns as _, ExecuteMsgFns as _, QueryMsgFns as _},
};
use cw_shitstrap_factory::{
    interface::CwShitstrapFactory,
    msg::{AsyncQueryMsgFns as _, ExecuteMsgFns as _, QueryMsgFns as _},
};

#[derive(Clone, Debug)]
pub struct CwShitstrapSuiteDeployData {
    pub shit: Vec<cw_shitstrap::msg::InstantiateMsg>,
    pub factory: Option<cw_shitstrap_factory::msg::InstantiateMsg>,
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

    fn load_from(chain: Chain) -> Result<Self, Self::Error> {
        todo!()
    }

    fn deploy_on(chain: Chain, data: Self::DeployData) -> Result<Self, Self::Error> {
        let mut suite = CwShitstrapSuite::store_on(chain.clone())?;
        match data {
            Some(d) => {
                if let Some(i) = d.factory {
                    suite.factory.instantiate(&i, d.admin.as_ref(), &[])?;
                }
                for (i, init) in d.shit.iter().enumerate() {
                    suite
                        .factory
                        .create_native_shit_strap_contract(init.clone(), format!("{i}",))?;
                }

                Ok(suite)
            }
            None => Err(CwOrchError::NotImplemented {}),
        }
    }
}
