#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, IbcBasicResponse,
    IbcDestinationCallbackMsg, MessageInfo, Response, StdAck, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use error::*;
use msg::*;
use state::*;

use cw_shit_denom::UncheckedDenom;
use cw_shitstrap::contract::msg::{AssetUnchecked, ExecuteMsg as ShitStrapExecuteMsg};

// version info for migration info
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let shitstrap = deps.api.addr_validate(&msg.shitstrap)?;
    SHITSTRAP_ADDR.save(deps.storage, &shitstrap)?;

    if let Some(svg) = &msg.svg_collection {
        let svg_addr = deps.api.addr_validate(svg)?;
        SVG_COLLECTION_ADDR.save(deps.storage, &svg_addr)?;
    }

    let mint_price = msg.mint_price.unwrap_or(Uint128::zero());
    MINT_PRICE.save(deps.storage, &mint_price)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("shitstrap", msg.shitstrap))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            shitstrap,
            svg_collection,
            mint_price,
        } => execute_update_config(deps, shitstrap, svg_collection, mint_price),
    }
}

fn execute_update_config(
    deps: DepsMut,
    shitstrap: Option<String>,
    svg_collection: Option<String>,
    mint_price: Option<Uint128>,
) -> Result<Response, ContractError> {
    if let Some(s) = shitstrap {
        let addr = deps.api.addr_validate(&s)?;
        SHITSTRAP_ADDR.save(deps.storage, &addr)?;
    }
    if let Some(s) = svg_collection {
        let addr = deps.api.addr_validate(&s)?;
        SVG_COLLECTION_ADDR.save(deps.storage, &addr)?;
    }
    if let Some(p) = mint_price {
        MINT_PRICE.save(deps.storage, &p)?;
    }

    Ok(Response::new().add_attribute("action", "update_config"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => {
            let shitstrap = SHITSTRAP_ADDR.load(deps.storage)?;
            let svg_collection = SVG_COLLECTION_ADDR.may_load(deps.storage)?;
            let mint_price = MINT_PRICE.load(deps.storage)?;
            let cfg = Config {
                shitstrap: shitstrap.to_string(),
                svg_collection: svg_collection.map(|a| a.to_string()),
                mint_price,
            };
            Ok(to_json_binary(&cfg)?)
        }
    }
}



pub mod error {
    use cosmwasm_std::StdError;
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum ContractError {
        #[error("{0}")]
        Std(#[from] StdError),

        #[error("Unauthorized")]
        Unauthorized {},

        #[error("IBC ack is not a success")]
        AckNotSuccess {},

        #[error("No transfer data in IBC destination callback")]
        NoTransferData {},

        #[error("Receiver mismatch: expected {expected}, got {got}")]
        ReceiverMismatch { expected: String, got: String },
    }
}

pub mod msg {
    use cosmwasm_schema::{cw_serde, QueryResponses};
    use cosmwasm_std::Uint128;

    #[cw_serde]
    pub struct InstantiateMsg {
        pub shitstrap: String,
        pub svg_collection: Option<String>,
        pub mint_price: Option<Uint128>,
    }

    #[cw_serde]
    #[cfg_attr(feature = "interface", derive(cw_orch::ExecuteFns))]
    pub enum ExecuteMsg {
        UpdateConfig {
            shitstrap: Option<String>,
            svg_collection: Option<String>,
            mint_price: Option<Uint128>,
        },
    }

    #[cw_serde]
    #[derive(QueryResponses)]
    #[cfg_attr(feature = "interface", derive(cw_orch::QueryFns))]
    pub enum QueryMsg {
        #[returns(super::Config)]
        Config {},
    }
}

pub mod state {
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::{Addr, Uint128};
    use cw_storage_plus::Item;

    pub const SHITSTRAP_ADDR: Item<Addr> = Item::new("shitstrap");
    pub const SVG_COLLECTION_ADDR: Item<Addr> = Item::new("svg_collection");
    pub const MINT_PRICE: Item<Uint128> = Item::new("mint_price");

    #[cw_serde]
    pub struct Config {
        pub shitstrap: String,
        pub svg_collection: Option<String>,
        pub mint_price: Uint128,
    }
}

#[cfg(feature = "interface")]
pub mod interface {
    use super::msg::*;
    use crate::contract::{execute, instantiate, query, CONTRACT_NAME};
    use cw_orch::prelude::*;

    #[cw_orch::interface(InstantiateMsg, ExecuteMsg, QueryMsg, Empty, id = CONTRACT_NAME)]
    pub struct CwShitstrapCallback;

    impl<Chain: CwEnv> Uploadable for CwShitstrapCallback<Chain> {
        /// Return the path to the wasm file corresponding to the contract
        fn wasm(_chain: &ChainInfoOwned) -> WasmPath {
            artifacts_dir_from_workspace!()
                .find_wasm_path_from_crates_label(CONTRACT_NAME)
                .unwrap()
        }
        /// Returns a CosmWasm contract wrapper
        fn wrapper() -> Box<dyn MockContract<Empty>> {
            Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
        }
    }
}
