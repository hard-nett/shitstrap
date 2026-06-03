#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    ensure_eq, from_json, to_json_binary, Addr, Attribute, Binary, Coin, CosmosMsg, Decimal, Deps,
    DepsMut, Env, IbcBasicResponse, IbcDestinationCallbackMsg, MessageInfo, Response, StdAck,
    StdResult, SubMsg, Uint128, Uint256, WasmMsg,
};

use cosmwasm_std::{DecimalRangeExceeded, DivideByZeroError, OverflowError, StdError};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_shit_denom::{AssetUnchecked, CheckedDenom, UncheckedDenom};
use cw_shit_denom::{DenomError, PossibleShit};
use cw_svg::SvgMintCallbackAction;
use ibc_app_transfer_types::proto::transfer::v2::FungibleTokenPacketData;
use msg::*;
use state::*;
use thiserror::Error;

pub mod msg {
    use super::*;
    use cosmwasm_schema::{cw_serde, QueryResponses};
    use cw_shit_denom::AssetUnchecked;
    #[cfg(not(target_arch = "wasm32"))]
    use cw_shit_denom::PossibleShit;
    use cw_svg::SvgMintCallbackAction;
    #[cw_serde]
    pub struct DaoParams {
        /// Dao addr
        pub addr: Addr,
        /// Set to 0 to disable
        pub floor: Uint128,
        /// Set to 0 to disable
        pub ceiling: Uint128,
    }

    // implements terp-speced adr-08 compat memo format:
    // <thing>MemoAdr08
    // - a: Vec<<thing>CallbackAction>
    #[cosmwasm_schema::cw_serde]
    pub struct ShitstrapMemoAdr08 {
        // ADR-8 tag
        pub ibc_callback: String,
        pub a: Vec<CosmosMsg>,
    }
    #[cosmwasm_schema::cw_serde]
    pub struct ShitstrapCallbackAction {
        pub shitstrap: ShitstrapMsg,
        pub nfts: Vec<SvgMintCallbackAction>,
    }

    #[cosmwasm_schema::cw_serde]
    pub struct ShitstrapMsg {
        pub shitter: String,
        pub shit: AssetUnchecked,
        pub recipient: Option<String>,
        pub dao: Option<String>,
    }

    #[cw_serde]
    pub struct InstantiateMsg {
        /// Dao one must be a member of to make use of shitstraps
        pub daos: Vec<DaoParams>,
        /// owner of the shit strap. This address will recieve all shit sent for this shitstrap.
        pub owner: Option<String>,
        /// a list of possible accepted assets, and the shit_rate you would like to set for.
        pub accepted: Vec<PossibleShit>,
        /// Desired cutoff points for shitstrap. 1000000 == 1 token.
        pub cutoff: Uint128,
        /// SHITMOS token address
        pub shitmos: UncheckedDenom,
        /// label for contract & front end
        pub title: String,
        /// description of shitstrap for recordkeeping
        pub description: String,
    }

    #[cw_serde]
    #[cfg_attr(feature = "interface", derive(cw_orch::ExecuteFns))]
    pub enum ExecuteMsg {
        /// Entry point to participate in shit-strap
        ShitStrap {
            recp: Option<String>,
            shit: AssetUnchecked,
            dao: Option<String>,
        },
        /// Admin function to set full-of-shit status to on. *(used for emergencies or early cutoff)*
        Flush {},
        /// Cw20 Entry Point
        Receive(Cw20ReceiveMsg),
        /// Refunds anyone that was the last one to shitstrap, and sent excess funds.
        RefundShitter {},
        /// Atomically deposit tokens and trigger mint from SVG collection if cutoff reached.
        /// Sends funds (native) along with the execute message.
        ShitStrapAndMint {
            shitstrap: ShitstrapMsg,
            mint: SvgMintCallbackAction,
        },
        /// Owner-only: set the SVG collection contract address for minting
        UpdateSvgCollection { address: String },
    }

    #[cw_serde]
    pub enum ReceiveMsg {
        /// Manually register an address for a shit strap when sending cw20 tokens.
        /// This can be a different address than the sender, if desired.
        ShitStrap {
            // will recieve shit unless recp is some.
            shitter: String,
            // overrides shit going to shitter.
            recp: Option<String>,
            // // shit sender is swapping for contracts shit
            dao: Option<String>,
        },
        // Atomically deposit cw20 tokens and trigger mint from SVG collection if cutoff reached.
        ShitStrapAndMint {
            shitter: String,
            recp: Option<String>,
            dao: Option<String>,
            mint: SvgMintCallbackAction,
        },
    }

    #[cw_serde]
    #[derive(QueryResponses)]
    #[cfg_attr(feature = "interface", derive(cw_orch::QueryFns))]
    pub enum QueryMsg {
        /// Returns max possible deposit value for a shit-strap instance
        #[returns(super::state::Config)]
        Config {},
        #[returns(Uint128)]
        /// Current amount of shit value that has been deposited in the shit-strap.
        /// Can be used to calculate how much more is needed for a full-of-shit status.
        HasShit {},
        #[returns(Uint128)]
        /// Current amount of shit value that still is to be shit by the shitstrap.
        #[returns(Uint128)]
        ToShit {},
        #[returns(bool)]
        /// Query if the shit strap contract is no longer active
        FullOfShit {},
        #[returns(Uint128)]
        /// Query the shit conversation ratio for a given asset
        #[returns(Option<Uint128>)]
        ShitRate { asset: String },
        /// Query the shit conversation ratio for a given asset
        #[returns(Option<Vec<PossibleShit>>)]
        ShitRates {},
        // /// Query maximum token to be able to send before shitstrap will become full of shit.
        // LeftToShit { shit: String },
    }
}

use cosmwasm_schema::cw_serde;

pub mod state {
    use super::*;
    use cosmwasm_std::{Addr, CosmosMsg, Uint128, Uint256};
    use cw_shit_denom::{CheckedDenom, PossibleShit};
    use cw_storage_plus::{Item, Map};

    pub const ATOMINC_DECIMALS: u32 = 6u32;
    pub const MAX_DEC_PRECISION: u32 = 18u32;
    pub const SHIT_RATE_SCALE: u128 = 1_000_000;

    #[cw_serde]
    pub struct Config {
        pub owner: Addr,

        pub accepted: Vec<PossibleShit>,
        pub cutoff: Uint256,
        pub shitmos_addr: CheckedDenom,
        pub full_of_shit: bool, // once cutoff is reached, full of shit set to true
        pub title: String,
        pub description: String,
    }

    #[cw_serde]
    pub struct MintTempStorage {
        /// The shitter who triggered the mint
        pub shitter: Addr,
        /// The amount of tokens deposited
        pub shit_amount: Uint256,
        /// The shit value calculated from the deposit
        pub shit_value: Uint256,
        /// The checked denom of the received tokens
        pub received_denom: CheckedDenom,
        /// The shit rate for the matched asset
        pub shit_rate: Uint128,
        /// The string key used in SHITSTRAP_STATE (the denom string)
        pub denom_key: String,
        /// Previous CURRENT_SHITSTRAP_VALUE before this deposit
        pub previous_shit_value: Uint256,
    }

    // version info for migration info
    pub const CW_SHITSTRAP: &str = "cw-shitstrap";
    pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
    pub const MINT_REPLY_ID: u64 = 1;
    pub const CONFIG: Item<Config> = Item::new("s");
    pub const CURRENT_SHITSTRAP_VALUE: Item<Uint256> = Item::new("h");
    pub const REFUND_SHIT: Map<Addr, CosmosMsg> = Map::new("i");
    pub const SHITSTRAP_STATE: Map<String, (Uint256, bool)> = Map::new("t");
    /// map of eligible daos an the floor and celing limits to vp for participation.
    /// 0,0 == no min & no max
    /// 0,y == no min, vp ceiling at y
    /// x,0 == floor at x, no ceiling
    /// x,y == floor at x, ceiling at y
    pub const DAOS: Map<Addr, (Uint256, Uint256)> = Map::new("ty");
    /// SVG collection address for minting NFTs when full_of_shit
    pub const SVG_MINTER: Item<Addr> = Item::new("svg_minter");
    /// Optional mint price override in the native gas denom (0 = use collection's price tier)
    pub const MINT_PRICE: Item<Uint128> = Item::new("mint_price");
    /// Temporary storage for reply handler during ShitStrapAndMint
    pub const MINT_TEMP_STORAGE: Item<MintTempStorage> = Item::new("mint_temp");
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CW_SHITSTRAP, CONTRACT_VERSION)?;
    let owner = match msg.owner {
        Some(o) => deps.api.addr_validate(&o)?,
        None => info.sender,
    };

    if msg.cutoff == Uint128::zero() {
        return Err(ContractError::ShittyCutoffRatio {});
    }
    if msg.title.len() > 100usize {
        return Err(ContractError::ShittyTitle {});
    }
    if msg.description.len() > 1000usize {
        return Err(ContractError::ShittyDescription {});
    }
    if msg.accepted.is_empty() || msg.accepted.len() == 4usize {
        return Err(ContractError::UnnaceptableShitAmount {});
    }

    let mut unique_fee = Vec::new();
    for accepted in msg.accepted.iter() {
        if unique_fee.contains(&accepted) {
            return Err(ContractError::SameShit {});
        } else {
            accepted.token.clone().into_checked(deps.as_ref())?;
            unique_fee.push(accepted);
        }
        if accepted.shit_rate == Uint128::zero() {
            return Err(ContractError::ShittyConversionRatio {});
        };
    }

    let shitmos_addr = msg.shitmos.into_checked(deps.as_ref())?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner,
            accepted: msg.accepted,
            cutoff: msg.cutoff.into(),
            shitmos_addr,
            full_of_shit: false,
            title: msg.title,
            description: msg.description,
        },
    )?;

    if msg.daos.len() > 10 {
        return Err(ContractError::ShittyDaoCount {});
    }
    for dao in msg.daos {
        DAOS.save(
            deps.storage,
            dao.addr,
            &(dao.floor.into(), dao.ceiling.into()),
        )?;
    }
    CURRENT_SHITSTRAP_VALUE.save(deps.storage, &Uint256::zero())?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = info.sender.clone();
    match msg {
        ExecuteMsg::ShitStrap { recp, shit, dao } => {
            execute_shit_strap_internal(deps, info, shit, sender, recp, dao, None)
        }
        ExecuteMsg::Flush {} => execute_flush(deps, sender),
        ExecuteMsg::Receive(cw20_msg) => receive_cw20_message(deps, info, cw20_msg),
        ExecuteMsg::RefundShitter {} => refund_shitter(deps, info),
        ExecuteMsg::ShitStrapAndMint {
            shitstrap: ss,
            mint: m,
        } => {
            let shitter = deps.api.addr_validate(&ss.shitter)?;
            execute_shit_strap_internal(deps, info, ss.shit, shitter, ss.recipient, ss.dao, Some(m))
        }
        ExecuteMsg::UpdateSvgCollection { address } => {
            execute_update_svg_collection(deps, info, address)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::HasShit {} => to_json_binary(&CURRENT_SHITSTRAP_VALUE.load(deps.storage)?),
        QueryMsg::FullOfShit {} => to_json_binary(&CONFIG.load(deps.storage)?.full_of_shit),
        QueryMsg::ShitRate { asset } => to_json_binary(
            &CONFIG
                .load(deps.storage)?
                .accepted
                .into_iter()
                .filter_map(|c| match c.token {
                    UncheckedDenom::Native(n) => {
                        if n == asset {
                            Some(c.shit_rate)
                        } else {
                            None
                        }
                    }
                    UncheckedDenom::Cw20(cw) => {
                        if cw == asset {
                            Some(c.shit_rate)
                        } else {
                            None
                        }
                    }
                })
                .next(),
        ),
        QueryMsg::ShitRates {} => {
            let config = CONFIG.load(deps.storage)?;
            let shit_rates: Vec<PossibleShit> = config
                .accepted
                .into_iter()
                .map(|c| PossibleShit {
                    token: c.token,
                    shit_rate: c.shit_rate,
                })
                .collect();
            to_json_binary(&shit_rates)
        }
        QueryMsg::ToShit {} => {
            let config = CONFIG.load(deps.storage)?;
            let this = CURRENT_SHITSTRAP_VALUE.load(deps.storage)?;
            to_json_binary(&config.cutoff.checked_sub(this)?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_destination_callback(
    deps: DepsMut,
    env: Env,
    msg: IbcDestinationCallbackMsg,
) -> Result<IbcBasicResponse, ContractError> {
    ensure_eq!(
        msg.packet.dest.port_id,
        "transfer",
        StdError::msg("only want to handle transfer packets")
    );

    if !cosmwasm_std::from_json::<StdAck>(&msg.ack.data)?.is_success() {
        return Err(ContractError::AckNotSuccess {});
    }

    let packet_data: FungibleTokenPacketData = from_json(&msg.packet.data)?;
    let funds = &msg.transfer.expect("msg").funds;

    let receiver = deps.api.addr_validate(packet_data.receiver.as_ref())?;
    ensure_eq!(
        receiver,
        env.contract.address,
        ContractError::ReceiverMismatch {
            w: env.contract.address.to_string(),
            f: receiver.to_string(),
        }
    );

    let actions = match !packet_data.memo.is_empty() {
        true => {
            let memo: ShitstrapMemoAdr08 = from_json(packet_data.memo.as_bytes())
                .map_err(|e| ContractError::MemoParseError { e: e.to_string() })?;

            if memo.ibc_callback != env.contract.address.to_string() {
                return Err(ContractError::CallbackAddrMismatch {});
            }

            memo.a
        }
        _ => Default::default(),
    };

    Ok(IbcBasicResponse::new()
        .add_messages(actions)
        .add_attribute("action", "ibc_destination_callback")
        .add_attribute("receiver", receiver.to_string())
        .add_attribute("num_transfers", funds.len().to_string()))
}

// ── Shared core deposit logic ──

/// Result of execute_deposit — metadata that callers use to decide cutoff / mint paths.
struct DepositData {
    shit_value: Uint256,
    received_denom: CheckedDenom,
    new_val: Uint256,
    shit_rate: Uint128,
    denom_key: String,
}

/// Shared deposit core used by BOTH ShitStrap and ShitStrapAndMint.
/// Validates, matches token, checks DAO membership, updates SHITSTRAP_STATE.
/// Does NOT modify config.full_of_shit or CURRENT_SHITSTRAP_VALUE.
fn execute_deposit(
    deps: &mut DepsMut,
    info: &MessageInfo,
    shit: &AssetUnchecked,
    shitter: &Addr,
    dao: &Option<String>,
) -> Result<DepositData, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if config.full_of_shit {
        return Err(ContractError::FullOfShit {});
    }

    // DAO membership check
    let daos: Vec<(Addr, (Uint256, Uint256))> = DAOS
        .range(deps.storage, None, None, cosmwasm_std::Order::Descending)
        .collect::<StdResult<Vec<_>>>()?;

    if let Some(d) = dao {
        #[cfg(feature = "dao")]
        {
            let room = daos
                .iter()
                .find(|(a, _)| a == d)
                .ok_or(ContractError::DidntSendShit {})?;
            check_voting_power(
                deps.as_ref(),
                info.sender(),
                d,
                Uint128::try_from(room.1 .0).unwrap_or(Uint128::zero()),
                Uint128::try_from(room.1 .1).unwrap_or(Uint128::zero()),
            )?;
        }
        #[cfg(not(feature = "dao"))]
        {
            let _ = (daos, d);
            return Err(ContractError::DaoNotEnabled {});
        }
    } else {
        #[cfg(feature = "dao")]
        {
            for (dao_addr, (floor_raw, ceiling_raw)) in &daos {
                check_voting_power(
                    deps.as_ref(),
                    info.sender(),
                    dao_addr,
                    Uint128::try_from(*floor_raw).unwrap_or(Uint128::zero()),
                    Uint128::try_from(*ceiling_raw).unwrap_or(Uint128::zero()),
                )?;
            }
        }
    }

    // Match the deposit token against accepted list
    let matched = config
        .accepted
        .iter()
        .find(|c| c.token == shit.denom)
        .ok_or(ContractError::WrongShit {})?
        .clone();

    // Validate funds sent
    match &matched.token {
        UncheckedDenom::Native(t) => {
            if !info
                .funds
                .iter()
                .any(|c| c.denom == *t && c.amount == shit.amount)
            {
                return Err(ContractError::DidntSendShit {});
            }
        }
        UncheckedDenom::Cw20(c) => {
            if info.sender.to_string() != *c {
                return Err(ContractError::ShittyCw20 {});
            }
        }
    }

    // Calculate shit value (conversion rate)
    let current_shit_value = CURRENT_SHITSTRAP_VALUE.load(deps.storage)?;
    let (shit_value, received_denom) = calculate_shit_value(deps.as_ref(), &matched, shit.amount)?;

    // Update SHITSTRAP_STATE for this denom
    let denom_key = received_denom.to_string();
    SHITSTRAP_STATE.update::<_, ContractError>(deps.storage, denom_key.clone(), |prev| {
        let this = prev.unwrap_or_default();
        let new_amount = this
            .0
            .checked_add(shit.amount)
            .map_err(|e| ContractError::ShitStd(StdError::msg(e)))?;
        Ok((new_amount, this.1))
    })?;

    // Calculate new total value
    let new_val = shit_value + current_shit_value;

    Ok(DepositData {
        shit_value,
        received_denom,
        new_val,
        shit_rate: matched.shit_rate,
        denom_key,
    })
}

// ── Messaging helpers ──

fn build_transfer_msg(
    is_cw20: bool,
    denom: String,
    amount: Uint256,
    recipient: Addr,
) -> Result<CosmosMsg, ContractError> {
    if is_cw20 {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: denom,
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.to_string(),
                amount: Uint128::try_from(amount)
                    .map_err(|_| ContractError::DigginForShitTreasure {})?
                    .into(),
            })?,
            funds: vec![],
        }))
    } else {
        Ok(CosmosMsg::Bank(cosmwasm_std::BankMsg::Send {
            to_address: recipient.to_string(),
            amount: vec![Coin { denom, amount }],
        }))
    }
}

/// Send accumulated tokens to owner. Used on cutoff in both ShitStrap and ShitStrapAndMint reply success.
fn send_accumulated_to_owner(
    deps: &Deps,
    exclude_denom: &str,
    exclude_shit_amount: Uint256,
    overflow: Uint256,
    recipient: &Addr,
) -> Result<Vec<CosmosMsg>, ContractError> {
    let mut msgs = vec![];
    let config = CONFIG.load(deps.storage)?;

    for owned in SHITSTRAP_STATE.range(deps.storage, None, None, cosmwasm_std::Order::Ascending) {
        let tokens = owned?;
        let denom_key = tokens.0;
        let (total_amount, _) = tokens.1;

        let is_cw20 = config.accepted.iter().any(|a| match &a.token {
            UncheckedDenom::Cw20(c) => c == &denom_key,
            _ => false,
        });

        let send_amount = if denom_key == exclude_denom {
            total_amount - (exclude_shit_amount - overflow)
        } else {
            total_amount
        };

        if send_amount.is_zero() {
            continue;
        }

        let msg = build_transfer_msg(is_cw20, denom_key, send_amount, recipient.clone())?;
        msgs.push(msg);
    }

    Ok(msgs)
}

/// Entry point to participate in shitstrap. If mint_config is Some, injects a mint
/// SubMsg when cutoff is reached (ShitStrapAndMint path) instead of the normal
/// owner token distribution (which happens asynchronously in the reply handler).
pub fn execute_shit_strap_internal(
    mut deps: DepsMut,
    info: MessageInfo,
    shit: AssetUnchecked,
    shitter: Addr,
    recp: Option<String>,
    dao: Option<String>,
    nft: Option<SvgMintCallbackAction>,
) -> Result<Response, ContractError> {
    let deposit = execute_deposit(&mut deps, &info, &shit, &shitter, &dao)?;
    let config = CONFIG.load(deps.storage)?;
    let mut msgs: Vec<CosmosMsg> = vec![];
    let mut submsgs: Vec<SubMsg> = vec![];
    let mut attrs: Vec<Attribute> = vec![];

    let new_val = deposit.new_val;
    let cutoff = config.cutoff;

    if new_val >= cutoff {
        // Normal ShitStrap cutoff — distribute to owner, refund overflow
        let (overflow, return_amount) =
            calculate_shit_return(new_val, cutoff, deposit.shit_rate.into())?;

        let own_msgs = send_accumulated_to_owner(
            &deps.as_ref(),
            &deposit.denom_key,
            deposit.shit_value,
            overflow,
            &config.owner,
        )?;
        msgs.extend(own_msgs);
        //  refunds always are obligated to shitter, even if there is some recp.
        let refund_msg = build_transfer_msg(
            matches!(deposit.received_denom, CheckedDenom::Cw20(_)),
            deposit.received_denom.to_string(),
            return_amount,
            shitter.clone(),
        )?;
        REFUND_SHIT.save(deps.storage, shitter.clone(), &refund_msg)?;

        let mut updated = config.clone();
        updated.full_of_shit = true;
        CONFIG.save(deps.storage, &updated)?;

        attrs.push(Attribute::new("cutoff_reached", "true"));
    }

    let send_shitmos = config.shitmos_addr.get_transfer_to_message(
        &match recp {
            Some(r) => deps.api.addr_validate(&r)?,
            None => shitter,
        },
        deposit.shit_value,
    )?;
    msgs.push(send_shitmos);

    // Save updated total value
    CURRENT_SHITSTRAP_VALUE.save(deps.storage, &new_val)?;
    Ok(Response::new()
        .add_messages(msgs)
        .add_submessages(submsgs)
        .add_attributes(attrs))
}

/// Entry point to manually set contract to full of shit. Owner only.
pub fn execute_flush(deps: DepsMut, sender: Addr) -> Result<Response, ContractError> {
    if sender != CONFIG.load(deps.storage)?.owner {
        return Err(ContractError::ShittyAuthorization {});
    }
    let mut config = CONFIG.load(deps.storage)?;
    if config.full_of_shit {
        return Err(ContractError::FullOfShit {});
    }
    config.full_of_shit = true;
    CONFIG.save(deps.storage, &config)?;

    let mut msgs = vec![];

    let current_shit_value = CURRENT_SHITSTRAP_VALUE.load(deps.storage)?;

    // Send leftover SHITMOS to owner
    let send_shitmos = config
        .shitmos_addr
        .get_transfer_to_message(&config.owner, config.cutoff - current_shit_value)?;
    msgs.push(send_shitmos);

    // Send all collected tokens to owner
    let own_msgs = send_accumulated_to_owner(
        &deps.as_ref(),
        "",
        Uint256::zero(),
        Uint256::zero(),
        &config.owner,
    )?;
    msgs.extend(own_msgs);

    Ok(Response::new()
        .add_attribute("flusher", sender.to_string())
        .add_messages(msgs))
}

/// Owner-only: set the SVG collection contract address for minting
pub fn execute_update_svg_collection(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::ShittyAuthorization {});
    }
    let addr = deps.api.addr_validate(&address)?;
    SVG_MINTER.save(deps.storage, &addr)?;
    Ok(Response::new()
        .add_attribute("action", "update_svg_collection")
        .add_attribute("address", address))
}

#[cfg(feature = "dao")]
fn check_voting_power(
    deps: Deps,
    sender: &Addr,
    dao: &Addr,
    floor: Uint128,
    ceiling: Uint128,
) -> Result<(), ContractError> {
    use dao_interface::voting;

    let vp = deps
        .querier
        .query_wasm_smart::<voting::VotingPowerAtHeightResponse>(
            dao,
            &voting::Query::VotingPowerAtHeight {
                address: sender.to_string(),
                height: None,
            },
        )?
        .power;

    if vp.is_zero() {
        return Err(ContractError::DontHaveShitStaked {});
    }
    if floor != Uint128::zero() && floor.gt(&vp) {
        return Err(ContractError::DontHaveShitStaked {});
    }
    if ceiling != Uint128::zero() && ceiling.lt(&vp) {
        return Err(ContractError::DontHaveShitStaked {});
    }
    Ok(())
}

fn refund_shitter(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender.clone();
    let msg = REFUND_SHIT
        .may_load(deps.storage, sender.clone())?
        .ok_or(ContractError::DigginForShitTreasure {})?;
    REFUND_SHIT.remove(deps.storage, sender);
    Ok(Response::new().add_message(msg))
}

fn receive_cw20_message(
    deps: DepsMut,
    info: MessageInfo,
    msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let sender_str = info.sender.to_string();
    match from_json(&msg.msg)? {
        ReceiveMsg::ShitStrap { shitter, recp, dao } => {
            let shitter = deps.api.addr_validate(&shitter)?;
            execute_shit_strap_internal(
                deps,
                info,
                AssetUnchecked {
                    denom: UncheckedDenom::Cw20(sender_str),
                    amount: msg.amount,
                },
                shitter,
                recp,
                dao,
                None,
            )
        }
        ReceiveMsg::ShitStrapAndMint {
            shitter,
            recp,
            dao,
            mint,
        } => {
            let sender = deps.api.addr_validate(&shitter)?;
            execute_shit_strap_internal(
                deps,
                info,
                AssetUnchecked {
                    denom: UncheckedDenom::Cw20(sender_str),
                    amount: msg.amount,
                },
                sender,
                recp,
                dao,
                Some(mint),
            )
        }
    }
}

use cosmwasm_std::Fraction;

fn calculate_shit_value(
    deps: Deps,
    matched: &PossibleShit,
    eligible_shit_amount: Uint256,
) -> Result<(Uint256, CheckedDenom), ContractError> {
    let rate = Decimal::from_atomics(matched.shit_rate, MAX_DEC_PRECISION)?;
    Ok((
        eligible_shit_amount.multiply_ratio(rate.numerator(), rate.denominator()),
        matched.clone().token.into_checked(deps)?,
    ))
}

fn calculate_shit_return(
    new_val: Uint256,
    cutoff: Uint256,
    shit_rate: Uint256,
) -> Result<(Uint256, Uint256), ContractError> {
    let overflow = new_val - cutoff;
    let return_to_shitter_amnt =
        overflow.multiply_ratio(10u128.pow(MAX_DEC_PRECISION as u32), shit_rate);
    Ok((overflow, return_to_shitter_amnt))
}

#[cfg(feature = "interface")]
pub mod interface {
    use super::msg::*;
    use crate::contract::{execute, instantiate, query, CW_SHITSTRAP};
    use cw_orch::prelude::*;

    #[cw_orch::interface(InstantiateMsg, ExecuteMsg, QueryMsg, Empty, id = CW_SHITSTRAP)]
    pub struct CwShitstrap;

    impl<Chain: CwEnv> Uploadable for CwShitstrap<Chain> {
        /// Return the path to the wasm file corresponding to the contract
        fn wasm(_chain: &ChainInfoOwned) -> WasmPath {
            artifacts_dir_from_workspace!()
                .find_wasm_path_from_crates_label(CW_SHITSTRAP)
                .unwrap()
        }
        /// Returns a CosmWasm contract wrapper
        fn wrapper() -> Box<dyn MockContract<Empty>> {
            Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
        }
    }
}
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    ShitStd(#[from] StdError),

    #[error("{0}")]
    ShitDenomError(#[from] DenomError),

    #[error("{0}")]
    DecimalRangeExceeded(#[from] DecimalRangeExceeded),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    DivideByZeroError(#[from] DivideByZeroError),

    #[error("MemoParseError: {e}")]
    MemoParseError { e: String },

    #[error("CallbackAddrMismatch")]
    CallbackAddrMismatch,

    #[error("Wrong Shit.")]
    WrongShit {},

    #[error("Full of Shit.")]
    FullOfShit {},

    #[error("Stake your shit to use this shitstrap (you ain't a DAO member)")]
    DontHaveShitStaked {},

    #[error("Tryna shit with too many DAOs, max 10")]
    ShittyDaoCount {},

    #[error("Did Not Send Shit, Liar.")]
    DidntSendShit {},

    #[error("Unable to claim refund")]
    DigginForShitTreasure {},

    #[error("Dao Feature Not Enabled")]
    DaoNotEnabled {},

    #[error("Unauthorized")]
    ShittyAuthorization {},

    #[error(
        "you are trying to set identical accepted_shit values. Try again with unique accepted_shit"
    )]
    SameShit {},

    #[error("You are trying to set too much shit to be accepted for this shitstrap.")]
    UnnaceptableShitAmount {},

    #[error("Cannot set cutoff as 0")]
    ShittyCutoffRatio {},

    #[error("Cannot set conversion ratio as 0")]
    ShittyConversionRatio {},

    #[error("Shitstrap title set for too long")]
    ShittyTitle {},

    #[error("Shitstrap title set for too long")]
    ShittyDescription {},

    #[error("You are trying to participate with a cw20. use the Cw20RecieveMsg.")]
    ShittyCw20 {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("No SVG collection address configured for minting")]
    NoSvgCollection {},

    #[error("SVG minting failed")]
    MintFailed {},

    #[error("Mint temp storage not found")]
    MintTempNotFound {},

    #[error("IBC ack is not a success")]
    AckNotSuccess {},

    #[error("No transfer data in IBC destination callback")]
    NoTransferData {},

    #[error("Receiver mismatch: expected {w}, got {f}")]
    ReceiverMismatch { w: String, f: String },
}

impl PartialEq for ContractError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "interface")]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_dependencies;

    #[test]
    pub fn unit_shit_rate() {
        let deps = mock_dependencies();
        let matched = PossibleShit {
            shit_rate: Uint128::from(222222000000000000u128),
            token: UncheckedDenom::from(UncheckedDenom::Native("test".into())),
        };
        let shit_amount = Uint128::from(100100u128);

        let result = calculate_shit_value(deps.as_ref(), &matched, shit_amount.into()).unwrap();
        assert_eq!(result.0, Uint256::from(22244u128));
        assert_eq!(result.1, matched.token.into_checked(deps.as_ref()).unwrap());
    }

    #[test]
    pub fn unit_shit_return_rate() {
        let new_val = Uint256::from(100u128);
        let cutoff = Uint256::from(50u128);
        let mut shit_rate = Uint256::from(1000000000000000000u128);

        let result = calculate_shit_return(new_val, cutoff, shit_rate).unwrap();
        assert_eq!(result.0, Uint256::from(50u128));
        assert_eq!(result.1, Uint256::from(50u128));

        shit_rate = Uint256::from(500000000000000000u128);
        let result = calculate_shit_return(new_val, cutoff, shit_rate).unwrap();
        assert_eq!(result.0, Uint256::from(50u128));
        assert_eq!(result.1, Uint256::from(100u128));

        shit_rate = Uint256::from(222222000000000000u128);
        let result = calculate_shit_return(new_val, cutoff, shit_rate).unwrap();
        assert_eq!(result.0, Uint256::from(50u128));
        assert_eq!(result.1, Uint256::from(225u128));
    }

    use cosmwasm_std::{
        coin, to_json_binary, Addr, Decimal, Empty, Event, Fraction, Uint128, Uint256,
    };
    use cw20::{Cw20Coin, Cw20ReceiveMsg};
    use cw20_base::msg::InstantiateMsg as Cw20Init;
    use cw_multi_test::{App, AppResponse, BankSudo, Contract, ContractWrapper, Executor, SudoMsg};
    use cw_shit_denom::UncheckedDenom;

    #[derive(Debug)]
    enum TestError {
        Std(String),
        Other(String),
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self {
                TestError::Std(e) | TestError::Other(e) => write!(f, "{}", e),
            }
        }
    }

    impl From<cosmwasm_std::StdError> for TestError {
        fn from(e: cosmwasm_std::StdError) -> Self {
            TestError::Std(e.to_string())
        }
    }

    impl From<cosmwasm_std::DecimalRangeExceeded> for TestError {
        fn from(e: cosmwasm_std::DecimalRangeExceeded) -> Self {
            TestError::Other(e.to_string())
        }
    }

    type TestResult = Result<(), TestError>;

    pub const DEFAULT_BALANCE: u128 = 1_000_000_000;
    pub const OWNER: &str = "owner";

    // s1: atom
    // s2: shit
    // s3: silk
    pub const SHITTER1: &str = "shitter1";
    pub const SHITTER2: &str = "shitter2";
    pub const SHITTER3: &str = "shitter3";

    fn cw20_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        );
        Box::new(contract)
    }

    fn shitstrap_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    fn init_bad_cw20(app: &mut App, cw20_id: u64, owner: Addr) -> Addr {
        // create another cw20
        let cw20_init = Cw20Init {
            name: "rtuioptaewi".into(),
            symbol: "rtjhtjkr".into(),
            decimals: 6,
            initial_balances: vec![],
            mint: None,
            marketing: None,
        };
        app.instantiate_contract(cw20_id, owner.clone(), &cw20_init, &[], "cw20", None)
            .unwrap()
    }

    fn instantiate_w_cw20(
        mut app: App,
        shit_id: u64,
        init: InstantiateMsg,
        cw20_id: u64,
        cw20: Cw20Init,
        owner: Addr,
        shitters: Vec<Addr>,
    ) -> ShitSuite {
        // init cw20
        let cw20 = app
            .instantiate_contract(
                cw20_id,
                owner.clone(),
                &cw20,
                &[],
                "cw20",
                Some(owner.to_string()),
            )
            .unwrap();
        // init shitstrap
        let shitstrap = app
            .instantiate_contract(
                shit_id,
                owner.clone(),
                &init,
                &[],
                "shitstrap",
                Some(owner.to_string()),
            )
            .unwrap();

        ShitSuite {
            app,
            shitstrap,
            cw20,
            owner,
            shitters,
        }
    }

    fn default_init(mut app: App, possible: Vec<PossibleShit>, cutoff: u128) -> ShitSuite {
        let shitstrap_id = app.store_code(shitstrap_contract());
        let cw20_id = app.store_code(cw20_contract());
        let owner = app.api().addr_make(OWNER);
        let s1 = app.api().addr_make(SHITTER1);
        let s2 = app.api().addr_make(SHITTER2);
        let s3 = app.api().addr_make(SHITTER3);

        let mut shitters = Vec::new();

        // create cw20
        let cw20_init = Cw20Init {
            name: "poo".into(),
            symbol: "POO".into(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: s1.to_string(),
                    amount: 100000000u128.into(),
                },
                Cw20Coin {
                    address: s2.to_string(),
                    amount: 100000000u128.into(),
                },
            ],
            mint: None,
            marketing: None,
        };
        // create shitstrap
        let init = InstantiateMsg {
            owner: Some(owner.to_string()),
            accepted: possible,
            cutoff: cutoff.into(),
            shitmos: cw_shit_denom::UncheckedDenom::Native("ushit".into()),
            title: "yoo".into(),
            description: "yoooooo".into(),
            daos: Vec::new(),
        };
        shitters.extend(vec![s1, s2, s3]);
        // instantiate contract with cw20
        let suite = instantiate_w_cw20(
            app,
            shitstrap_id,
            init,
            cw20_id,
            cw20_init.clone(),
            owner,
            shitters,
        );
        // return suite
        suite
    }

    pub struct ShitSuite {
        pub app: App,
        pub shitstrap: Addr,
        pub cw20: Addr,
        pub owner: Addr,
        pub shitters: Vec<Addr>,
    }

    impl ShitSuite {
        /// funds testing accounts with default balance
        fn setup_default_funds(&mut self, shitstrap: Addr) -> TestResult {
            self.app
                .sudo(SudoMsg::Bank(BankSudo::Mint {
                    to_address: self.owner.clone().to_string(),
                    amount: vec![coin(1_000_000_000u128, "uatom")],
                }))
                .unwrap();
            self.app
                .sudo(SudoMsg::Bank(BankSudo::Mint {
                    to_address: self.shitters[0].to_string(), // s1: atom
                    amount: vec![coin(1_000_000_000u128, "uatom")],
                }))
                .unwrap();
            self.app
                .sudo(SudoMsg::Bank(BankSudo::Mint {
                    to_address: self.shitters[1].to_string(),
                    amount: vec![coin(1_000_000_000u128, "ushit")],
                }))
                .unwrap();
            self.app
                .sudo(SudoMsg::Bank(BankSudo::Mint {
                    to_address: self.shitters[2].to_string(),
                    amount: vec![coin(1_000_000_000u128, "usilk")],
                }))
                .unwrap();
            // fund shitstrap with 1 million shit
            self.app
                .sudo(SudoMsg::Bank(BankSudo::Mint {
                    to_address: shitstrap.to_string(),
                    amount: vec![coin(1_000_000_000_000u128, "ushit")],
                }))
                .unwrap();
            Ok(())
        }

        /// helper function to participates in shitstrap with c20 coin
        fn participate_cw20(
            &mut self,
            sender: &str,
            amount: u128,
            contract: &str,
        ) -> Result<AppResponse, TestError> {
            Ok(self.app.execute_contract(
                Addr::unchecked(contract.to_string()),
                self.shitstrap.clone(),
                &super::msg::ExecuteMsg::Receive(Cw20ReceiveMsg {
                    sender: sender.into(),
                    amount: amount.into(),
                    msg: to_json_binary(&ReceiveMsg::ShitStrap {
                        shitter: sender.to_string(),
                        dao: None,
                        recp: None,
                    })
                    .unwrap(),
                }),
                &vec![],
            )?)
        }

        /// helper function to participate in shitstrap with native coin
        fn participate_native(
            &mut self,
            sender: &str,
            amount: u128,
            denom: &str,
        ) -> Result<AppResponse, TestError> {
            Ok(self.app.execute_contract(
                Addr::unchecked(sender.to_string()),
                self.shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    shit: AssetUnchecked {
                        denom: cw_shit_denom::UncheckedDenom::Native(denom.into()),
                        amount: amount.into(),
                    },
                    dao: None,
                    recp: None,
                },
                &vec![coin(amount, denom)],
            )?)
        }
    }

    #[test]
    fn test_bad_init() -> TestResult {
        let mut app = App::default();
        let mut possible = vec![
            PossibleShit::native_denom("uatom", 1_000_000u128),
            PossibleShit::native_denom("uatom", 1_000_000u128),
            PossibleShit::native_denom("uatom3", 0u128),
            PossibleShit::native_denom("uatom4", 1_000_000u128),
        ];

        let title = "a".repeat(101).into();
        let description = "y".repeat(1001).into();
        let cutoff = Uint128::zero();

        // create default testing suite
        let mut shit = default_init(
            app,
            vec![PossibleShit::native_denom("uatom", 1_000_000u128)], // 1:1 ratio
            222u128,
        );

        let mut init_msg = InstantiateMsg {
            owner: Some(shit.owner.to_string()),
            accepted: possible.clone(),
            cutoff: cutoff.into(),
            shitmos: cw_shit_denom::UncheckedDenom::Native("ushit".into()),
            title,
            description,
            daos: Vec::new(),
        };

        // init shitstrap
        let err = shit
            .app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::ShittyCutoffRatio {}.to_string()));
        init_msg.cutoff = Uint128::from(1_000_000u128);
        let err = shit
            .app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::ShittyTitle {}.to_string()));
        init_msg.title = "shitstrap".into();
        let err = shit
            .app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::ShittyDescription {}.to_string()));
        init_msg.description = "shitstrap description".into();
        let err = shit
            .app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::UnnaceptableShitAmount {}.to_string()));
        possible = possible[..possible.len() - 1].to_vec();
        init_msg.accepted = possible.clone();
        let err = shit
            .app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::SameShit {}.to_string()));
        possible[1].token = UncheckedDenom::Native("uatom2".into());
        init_msg.accepted = possible.clone();
        let err = shit
            .app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::ShittyConversionRatio {}.to_string()));
        possible[2].shit_rate = Uint128::one();
        init_msg.accepted = possible;
        shit.app
            .instantiate_contract(
                1,
                shit.owner.clone(),
                &init_msg.clone(),
                &[],
                "shitstrap",
                Some(shit.owner.to_string()),
            )
            .unwrap();

        Ok(())
    }

    #[test]
    fn test_shitstrap() -> TestResult {
        let mut app = App::default();
        // create default testing suite
        let mut shit = default_init(
            app,
            vec![PossibleShit::native_denom("uatom", 1000000000000000000u128)], // 1:1 ratio
            222000000u128,
        );
        // deposit 1 less than max
        let first_deposit = 221_000_000u128;
        let shitstrap = shit.shitstrap.clone();
        shit.setup_default_funds(shitstrap.clone())?;
        let s1 = shit.shitters[0].clone();

        // error with wrong native token
        let err = shit
            .app
            .execute_contract(
                shit.owner.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    shit: AssetUnchecked::from_native("usilk", first_deposit),
                    dao: None,
                    recp: None,
                },
                &vec![coin(first_deposit, "uatom")],
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::WrongShit {}.to_string()));
        let cw20_id = shit.app.store_code(cw20_contract());
        let cw20 = &init_bad_cw20(&mut shit.app, cw20_id, shit.owner.clone()).to_string();
        // error with wrong cw20 token
        let err = shit
            .participate_cw20(&shit.owner.to_string(), first_deposit, cw20)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::WrongShit {}.to_string()));

        // error without sending token
        let err = shit
            .app
            .execute_contract(
                s1.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    shit: AssetUnchecked::from_native("uatom", first_deposit),
                    dao: None,
                    recp: None,
                },
                &vec![],
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::DidntSendShit {}.to_string()));

        // move forward in time
        let mut block = shit.app.block_info();
        block.height += 1;
        shit.app.set_block(block);

        // error with correct token, but less sent then specified
        shit.app
            .execute_contract(
                s1.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    shit: AssetUnchecked::from_native("uatom", first_deposit),
                    dao: None,
                    recp: None,
                },
                &vec![coin(22, "uatom")],
            )
            .unwrap_err();

        // move forward in time
        let mut block = shit.app.block_info();
        block.height += 1;
        shit.app.set_block(block);

        let og_owner_bal = shit.app.wrap().query_balance(shit.owner.clone(), "uatom")?;
        assert_eq!(og_owner_bal.amount, Uint256::new(DEFAULT_BALANCE));

        // participate in shitstrap with correct token
        shit.participate_native(&s1.to_string(), 221_000_000, "uatom")?;

        // confirm shit_rate is calculated correctly
        let res: Uint128 = shit
            .app
            .wrap()
            .query_wasm_smart(shitstrap.clone(), &super::msg::QueryMsg::HasShit {})?;
        assert_eq!(res, Uint128::new(first_deposit));

        // confirm new balance of shitstrap
        let balance = shit.app.wrap().query_balance(&shitstrap, "uatom")?;
        let shit_rate: Option<Uint128> = shit.app.wrap().query_wasm_smart(
            shitstrap.clone(),
            &super::msg::QueryMsg::ShitRate {
                asset: "uatom".to_string(),
            },
        )?;
        // calulate expected
        let dec = Decimal::from_atomics(shit_rate.unwrap(), MAX_DEC_PRECISION)?;
        let calculated = balance
            .amount
            .multiply_ratio(dec.numerator(), dec.denominator());
        assert_eq!(calculated, Uint256::new(first_deposit));

        // shitstrap reaches limit
        shit.app.execute_contract(
            s1.clone(),
            shitstrap.clone(),
            &super::msg::ExecuteMsg::ShitStrap {
                shit: AssetUnchecked::from_native("uatom", 2_000_000u128),
                dao: None,
                recp: None,
            },
            &vec![coin(2000000u128, "uatom")],
        )?;

        // move forward in time
        let mut block = shit.app.block_info();
        block.height += 1;
        shit.app.set_block(block);

        // confirm contract will not continue to shitstrap
        let res: bool = shit
            .app
            .wrap()
            .query_wasm_smart(shit.shitstrap, &super::msg::QueryMsg::FullOfShit {})?;
        assert_eq!(res, true);

        // confirm balances
        let balance = shit.app.wrap().query_balance(shitstrap.clone(), "uatom")?;
        assert_eq!(balance.amount, Uint256::new(1_000_000u128)); // 1 token is waiting to be redeemed by last shit strapper
        let balance = shit.app.wrap().query_balance(s1.clone(), "uatom")?;
        assert_eq!(balance.amount, Uint256::new(777_000_000u128));
        let owner_bal = shit.app.wrap().query_balance(shit.owner, "uatom")?;
        assert_eq!(owner_bal.amount, Uint256::new(1_222_000_000u128)); // owner received 222 ATOM

        // no more shitstrapping can commence
        let err = shit
            .app
            .execute_contract(
                s1.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    shit: AssetUnchecked::from_native("uatom", 2_000_000u128),
                    dao: None,
                    recp: None,
                },
                &vec![coin(2_000_000u128, "uatom")],
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::FullOfShit {}.to_string()));

        // refund on shitstrapping occurs
        shit.app.execute_contract(
            s1.clone(),
            shitstrap.clone(),
            &super::msg::ExecuteMsg::RefundShitter {},
            &[],
        )?;

        // move forward in time
        let mut block = shit.app.block_info();
        block.height += 1;
        shit.app.set_block(block);

        // should have 1 extra token sent back
        let balance = shit.app.wrap().query_balance(s1.clone(), "uatom")?;
        assert_eq!(balance.amount, Uint256::new(778_000_000u128));
        let balance = shit.app.wrap().query_balance(shitstrap.clone(), "uatom")?;
        assert_eq!(balance.amount, Uint256::zero()); // 1 token is no longer waiting to be redeemed by last shit strapper
        Ok(())
    }

    #[test]
    fn test_fee_destination() -> TestResult {
        let mut app = App::default();
        let initer = app.api().addr_make("eh");
        let cw20_id = app.store_code(cw20_contract());
        let cw20 = &init_bad_cw20(&mut app, cw20_id, initer).to_string();
        // create testing suite
        let first_deposit = 100_000_000u128; // 100
        let cw20_shit_ratio = Uint128::from(640000000000000000u128); // 64%
        let atom_shit_ratio = Uint128::from(360000000000000000u128); // 36%
        let mut shit = default_init(
            app,
            vec![
                PossibleShit::native_denom("uatom", atom_shit_ratio.clone().into()),
                PossibleShit::native_cw20(cw20, cw20_shit_ratio.clone().into()),
            ],
            222000000u128,
        );

        let shitstrap = shit.shitstrap.clone();
        let s1 = shit.shitters[0].clone(); // atom
        let s2 = shit.shitters[1].clone(); //shit
        shit.setup_default_funds(shitstrap.clone())?;
        let dec = Decimal::from_atomics(atom_shit_ratio, MAX_DEC_PRECISION)?;
        let first = Uint128::new(first_deposit).multiply_ratio(dec.numerator(), dec.denominator());

        shit.app
            .sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: shitstrap.to_string(),
                amount: vec![coin(222000000u128, "ushit")],
            }))
            .unwrap();

        // assert shitstrap default balance
        let res = shit.app.wrap().query_balance(shitstrap.clone(), "ushit")?;
        assert_eq!(
            res.amount,
            Uint256::new(222000000u128 + 1_000_000_000_000u128)
        );

        // user 1 funds with native
        shit.participate_native(&s1.to_string(), 100_000_000, "uatom")?;

        // confirm shit_rate is calculated correctly
        let res: Uint128 = shit
            .app
            .wrap()
            .query_wasm_smart(shitstrap.clone(), &super::msg::QueryMsg::HasShit {})?;
        assert_eq!(res, first);

        // confirm funds made it to shitter
        let s1_uatom = shit.app.wrap().query_balance(&s1, "uatom")?;
        let s1_ushit = shit.app.wrap().query_balance(&s1, "ushit")?;
        assert_eq!(
            s1_uatom.amount,
            Uint256::new(DEFAULT_BALANCE - first_deposit)
        );
        assert_eq!(s1_ushit.amount, Uint256::from(first));

        // confirm funds are still in shitstrap
        let strap_uatom = shit.app.wrap().query_balance(shitstrap.clone(), "uatom")?;
        let strap_ushit = shit.app.wrap().query_balance(shitstrap.clone(), "ushit")?;
        assert_eq!(strap_uatom.amount, Uint256::new(first_deposit));
        assert_eq!(
            strap_ushit.amount,
            Uint256::new(1_000_000_000_000u128 + (222000000u128 - first.u128()))
        );

        // end shitstrap early
        let res = shit.app.execute_contract(
            shit.owner.clone(),
            shitstrap.clone(),
            &super::msg::ExecuteMsg::Flush {},
            &[],
        )?;

        // confirm uatom was sent with bank
        res.assert_event(
            &Event::new("transfer")
                .add_attribute("recipient", shit.owner.to_string())
                .add_attribute("sender", shitstrap.to_string())
                .add_attribute(
                    "amount",
                    (222000000u128 - first.u128()).to_string() + "ushit",
                ),
        );
        res.assert_event(
            &Event::new("transfer")
                .add_attribute("recipient", shit.owner.to_string())
                .add_attribute("sender", shitstrap.to_string())
                .add_attribute("amount", first_deposit.to_string() + "uatom"),
        );

        // confirm shitstrap balance is empty
        let strap_uatom = shit.app.wrap().query_balance(shitstrap.clone(), "uatom")?;
        let strap_ushit = shit.app.wrap().query_balance(shitstrap.clone(), "ushit")?;
        assert_eq!(strap_uatom.amount, Uint256::zero());
        assert_eq!(strap_ushit.amount, Uint256::new(1_000_000_000_000u128));

        // confirm shistrap owner now has updated balance
        let owner_uatom = shit.app.wrap().query_balance(&shit.owner, "uatom")?;
        let owner_ushit = shit.app.wrap().query_balance(&shit.owner, "ushit")?;
        assert_eq!(
            owner_uatom.amount,
            Uint256::new(DEFAULT_BALANCE + first_deposit)
        );
        assert_eq!(
            owner_ushit.amount,
            Uint256::new(222000000u128 - first.u128())
        );

        Ok(())
    }

    #[test]
    fn test_mult_participants_mult_possible_shit() -> TestResult {
        let mut app: App = App::default();
        let initer = app.api().addr_make("eh");
        let cw20_id = app.store_code(cw20_contract());
        let cw20 = &init_bad_cw20(&mut app, cw20_id, initer).to_string();
        // create testing suite
        let first_deposit = 100_000_000u128; // 100
        let cw20_shit_ratio = Uint128::from(640000000000000000u128); // 64%
        let atom_shit_ratio = Uint128::from(360000000000000000u128); // 36%

        let mut shit = default_init(
            app,
            vec![
                PossibleShit::native_denom("uatom", atom_shit_ratio.clone().into()),
                PossibleShit::native_cw20(cw20, cw20_shit_ratio.clone().into()),
            ],
            222000000u128,
        );

        let shitstrap = shit.shitstrap.clone();
        shit.setup_default_funds(shitstrap.clone())?;

        let s1 = shit.shitters[0].clone();
        let s2 = shit.shitters[1].clone();
        let s3 = shit.shitters[2].clone();

        // error with wrong native token
        let err = shit
            .app
            .execute_contract(
                s3.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    shit: AssetUnchecked::from_native("usilk", first_deposit),
                    dao: None,
                    recp: None,
                },
                &vec![coin(first_deposit, "usilk")],
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::WrongShit {}.to_string()));

        // create another cw20
        let cw20_init = Cw20Init {
            name: "poo".into(),
            symbol: "POO".into(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: s2.to_string(),
                    amount: 1000u128.into(),
                },
                Cw20Coin {
                    address: s3.to_string(),
                    amount: 1000u128.into(),
                },
            ],
            mint: None,
            marketing: None,
        };

        let bad_cw20 = shit
            .app
            .instantiate_contract(
                1u64,
                shit.owner.clone(),
                &cw20_init,
                &[],
                "cw20",
                Some(shit.owner.to_string()),
            )
            .unwrap();

        // error with wrong cw20
        let err = shit
            .app
            .execute_contract(
                bad_cw20,
                shitstrap.clone(),
                &super::msg::ExecuteMsg::Receive(Cw20ReceiveMsg {
                    sender: s2.to_string(),
                    amount: 200u128.into(),
                    msg: to_json_binary(&ReceiveMsg::ShitStrap {
                        shitter: s2.to_string(),
                        dao: None,
                        recp: None,
                    })
                    .unwrap(),
                }),
                &vec![],
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&ContractError::WrongShit {}.to_string()));

        // user 1 funds with native
        shit.participate_native(&s1.to_string(), 100_000_000, "uatom")?;
        // confirm shit_rate is calculated correctly
        let res: Uint128 = shit
            .app
            .wrap()
            .query_wasm_smart(shitstrap.clone(), &super::msg::QueryMsg::HasShit {})?;
        let dec = Decimal::from_atomics(atom_shit_ratio, MAX_DEC_PRECISION)?;
        assert_eq!(
            res,
            Uint128::new(first_deposit).multiply_ratio(dec.numerator(), dec.denominator())
        );
        // confirm funds made it to shitter
        let dec = Decimal::from_atomics(atom_shit_ratio, MAX_DEC_PRECISION)?;
        let s1_uatom = shit.app.wrap().query_balance(&s1, "uatom")?;
        let s1_ushit = shit.app.wrap().query_balance(&s1, "ushit")?;
        assert_eq!(
            s1_uatom.amount,
            Uint256::new(DEFAULT_BALANCE - first_deposit)
        );
        assert_eq!(
            s1_ushit.amount,
            Uint256::from(
                Uint128::new(first_deposit).multiply_ratio(dec.numerator(), dec.denominator())
            )
        );

        // user 2 funds with coin. should reflect 50% shit weight of native
        shit.participate_cw20(&s3.to_string(), 100_000_000, cw20)?;

        // confirm shit_rate is calculated correctly
        let res: Uint128 = shit
            .app
            .wrap()
            .query_wasm_smart(shitstrap.clone(), &super::msg::QueryMsg::HasShit {})?;
        let dec = Decimal::from_atomics(cw20_shit_ratio, MAX_DEC_PRECISION)?;
        let dec2 = Decimal::from_atomics(atom_shit_ratio, MAX_DEC_PRECISION)?;
        // the expected shit_strapped, after 2 participants
        let expected = (Uint128::new(first_deposit)
            .multiply_ratio(dec.numerator(), dec.denominator()))
            + (Uint128::new(first_deposit).multiply_ratio(dec2.numerator(), dec2.denominator()));

        assert_eq!(res, expected);

        // confirm native token balance is correct
        let dec = Decimal::from_atomics(cw20_shit_ratio, MAX_DEC_PRECISION)?;
        let s3_ushit = shit.app.wrap().query_balance(&s3, "ushit")?;
        let s3_usilk = shit.app.wrap().query_balance(&s3, "usilk")?;
        assert_eq!(
            s3_ushit.amount,
            Uint256::from(
                Uint128::new(first_deposit).multiply_ratio(dec.numerator(), dec.denominator())
            )
        );
        assert_eq!(s3_usilk.amount, Uint256::new(DEFAULT_BALANCE)); // has full balance of non accepted token
                                                                    // we skip checking cw20 balance in this test, done in next step.
        Ok(())
    }

    // test shit strap w/ cw20, not using cw20 recieve
    #[test]
    fn test_cw20_receive() -> TestResult {
        let mut app = App::default();
        let initer = app.api().addr_make("eh");
        let cw20_id = app.store_code(cw20_contract());
        let cw20 = &init_bad_cw20(&mut app, cw20_id, initer).to_string();
        let mut shit = default_init(app, vec![PossibleShit::native_cw20(cw20, 100u128)], 222u128);

        let first_deposit = 100u128;

        let shitstrap = shit.shitstrap.clone();
        let s1 = shit.shitters[0].clone();
        let s2 = shit.shitters[1].clone();
        shit.setup_default_funds(shitstrap.clone())?;

        // cannot directly call shit_strap entry point with cw20
        let err = shit
            .app
            .execute_contract(
                s1.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::ShitStrap {
                    recp: None,
                    shit: AssetUnchecked {
                        denom: cw_shit_denom::UncheckedDenom::Cw20(cw20.clone()),
                        amount: first_deposit.into(),
                    },
                    dao: None,
                },
                &vec![],
            )
            .unwrap_err();

        assert!(err
            .to_string()
            .contains(&ContractError::ShittyCw20 {}.to_string()));

        // only cw20 can call receive entry point
        let err = shit
            .app
            .execute_contract(
                s2.clone(),
                shitstrap.clone(),
                &super::msg::ExecuteMsg::Receive(Cw20ReceiveMsg {
                    sender: s2.to_string(),
                    amount: first_deposit.into(),
                    msg: to_json_binary(&ReceiveMsg::ShitStrap {
                        shitter: s2.to_string(),
                        dao: None,
                        recp: None,
                    })
                    .unwrap(),
                }),
                &vec![],
            )
            .unwrap_err();

        assert!(err
            .to_string()
            .contains(&ContractError::WrongShit {}.to_string()));

        Ok(())
    }
}
