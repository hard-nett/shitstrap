use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, CosmosMsg, Uint128};
use cw_denom::CheckedDenom;
use cw_storage_plus::{Item, Map};

use crate::msg::PossibleShit;

pub const ATOMINC_DECIMALS: u32 = 6u32;

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub accepted: Vec<PossibleShit>,
    pub cutoff: Uint128,
    pub shitmos_addr: CheckedDenom,
    pub full_of_shit: bool, // once cutoff is reached, full of shit set to true
    pub title: String,
    pub description: String,
}

pub const CONFIG: Item<Config> = Item::new("s");
pub const CURRENT_SHITSTRAP_VALUE: Item<Uint128> = Item::new("h");
pub const REFUND_SHIT: Map<Addr, CosmosMsg> = Map::new("i");
pub const SHITSTRAP_STATE: Map<String, Uint128> = Map::new("t");
