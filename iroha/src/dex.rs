//! This module contains functionality related to `DEX`.

use crate::permission::*;
use crate::prelude::*;
use integer_sqrt::*;
use iroha_derive::Io;
use parity_scale_codec::{Decode, Encode};
use std::cmp;
use std::collections::BTreeMap;
use std::mem;

const STORAGE_ACCOUNT_NAME: &str = "STORE";
const XYK_POOL: &str = "XYKPOOL";
const MINIMUM_LIQUIDITY: u32 = 1000;
const MAX_BASIS_POINTS: u16 = 10000;

type Name = String;

/// Identification of a DEX, consists of its domain name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct DEXId {
    domain_name: Name,
}

impl DEXId {
    /// Default DEX identifier constructor.
    pub fn new(domain_name: &str) -> Self {
        DEXId {
            domain_name: domain_name.to_owned(),
        }
    }
}

/// `DEX` entity encapsulates data and logic for management of
/// decentralized exchanges in domains.
#[derive(Encode, Decode, Debug, Clone, PartialEq, Eq, Io)]
pub struct DEX {
    /// An identification of the `DEX`.
    pub id: <DEX as Identifiable>::Id,
    /// DEX owner's account Identification. Only this account will be able to manipulate the DEX.
    pub owner_account_id: <Account as Identifiable>::Id,
    /// Token Pairs belonging to this dex.
    pub token_pairs: BTreeMap<<TokenPair as Identifiable>::Id, TokenPair>,
    /// Base Asset identification.
    pub base_asset_id: <AssetDefinition as Identifiable>::Id,
}

impl DEX {
    /// Default `DEX` entity constructor.
    pub fn new(
        domain_name: &str,
        owner_account_id: <Account as Identifiable>::Id,
        base_asset_id: <AssetDefinition as Identifiable>::Id,
    ) -> Self {
        DEX {
            id: DEXId::new(domain_name),
            owner_account_id,
            token_pairs: BTreeMap::new(),
            base_asset_id,
        }
    }
}

impl Identifiable for DEX {
    type Id = DEXId;
}

/// Identification of a Token Pair. Consists of underlying asset ids.
#[derive(Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Io)]
pub struct TokenPairId {
    /// Containing DEX identifier.
    pub dex_id: <DEX as Identifiable>::Id,
    /// Base token of exchange.
    pub base_asset_id: <AssetDefinition as Identifiable>::Id,
    /// Target token of exchange.
    pub target_asset_id: <AssetDefinition as Identifiable>::Id,
}

impl TokenPairId {
    /// Default Token Pair identifier constructor.
    pub fn new(
        dex_id: <DEX as Identifiable>::Id,
        base_asset_id: <AssetDefinition as Identifiable>::Id,
        target_asset_id: <AssetDefinition as Identifiable>::Id,
    ) -> Self {
        TokenPairId {
            dex_id,
            base_asset_id,
            target_asset_id,
        }
    }
    /// Symbol representation of the Token Pair.
    pub fn get_symbol(&self) -> String {
        format!(
            "{}-{}/{}-{}",
            self.base_asset_id.name,
            self.base_asset_id.domain_name,
            self.target_asset_id.name,
            self.target_asset_id.domain_name
        )
    }
}

/// `TokenPair` represents an exchange pair between two assets in a domain. Assets are
/// identified by their AssetDefinitionId's. Containing DEX is identified by domain name.
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug)]
pub struct TokenPair {
    /// An Identification of the `TokenPair`, holds pair of token Ids.
    pub id: <TokenPair as Identifiable>::Id,
    /// Liquidity Sources belonging to this TokenPair. At most one instance
    /// of each type.
    pub liquidity_sources: BTreeMap<<LiquiditySource as Identifiable>::Id, LiquiditySource>,
}

impl Identifiable for TokenPair {
    type Id = TokenPairId;
}

impl TokenPair {
    /// Default Token Pair constructor.
    pub fn new(
        dex_id: <DEX as Identifiable>::Id,
        base_asset: <AssetDefinition as Identifiable>::Id,
        target_asset: <AssetDefinition as Identifiable>::Id,
    ) -> Self {
        TokenPair {
            id: TokenPairId::new(dex_id, base_asset, target_asset),
            liquidity_sources: BTreeMap::new(),
        }
    }
}

/// Identification of a Liquidity Source. Consists of Token Pair Id and underlying
/// liquidity source type.
#[derive(Encode, Decode, Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Io)]
pub struct LiquiditySourceId {
    /// Identifier of containing token pair.
    pub token_pair_id: <TokenPair as Identifiable>::Id,
    /// Type of liquidity source.
    pub liquidity_source_type: LiquiditySourceType,
}

impl LiquiditySourceId {
    /// Default constructor for Liquidity Source identifier.
    pub fn new(
        token_pair_id: <TokenPair as Identifiable>::Id,
        liquidity_source_type: LiquiditySourceType,
    ) -> Self {
        LiquiditySourceId {
            token_pair_id,
            liquidity_source_type,
        }
    }
}

/// Enumration representing types of liquidity sources.
#[derive(Encode, Decode, Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Io)]
pub enum LiquiditySourceType {
    /// X*Y=K model liquidity pool.
    XYKPool,
    /// Regular order book.
    OrderBook,
}

/// Data storage for XYK liquidity source.
#[derive(Encode, Decode, Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Io)]
pub struct XYKPoolData {
    /// Asset definition of pool token belonging to given pool.
    pool_token_asset_definition_id: <AssetDefinition as Identifiable>::Id,
    /// Account that is used to store exchanged tokens, i.e. actual liquidity.
    storage_account_id: <Account as Identifiable>::Id,
    /// Account that will receive protocol fee part if enabled.
    fee_to: Option<<Account as Identifiable>::Id>,
    /// Fee for swapping tokens on pool, expressed in basis points.
    fee: u16,
    /// Fee fraction which is deduced from `fee` as protocol fee, expressed in basis points.
    protocol_fee_part: u16,
    /// Amount of active pool tokens.
    pool_token_total_supply: u32,
    /// Amount of base tokens in the pool (currently stored in storage account).
    base_asset_reserve: u32,
    /// Amount of target tokens in the pool (currently stored in storage account).
    target_asset_reserve: u32,
    /// K (constant product) value, updated by latest liquidity operation.
    k_last: u32,
}

impl XYKPoolData {
    /// Default constructor for XYK Pool Data entity.
    pub fn new(
        pool_token_asset_definition_id: <AssetDefinition as Identifiable>::Id,
        storage_account_id: <Account as Identifiable>::Id,
    ) -> Self {
        XYKPoolData {
            pool_token_asset_definition_id,
            storage_account_id,
            fee_to: None,
            fee: 30,
            protocol_fee_part: 0,
            pool_token_total_supply: 0,
            base_asset_reserve: 0,
            target_asset_reserve: 0,
            k_last: 0,
        }
    }
}

/// Try to unwrap reference `XYKPoolData` from `LiquiditySourceData` enum of `LiquiditySource` entity.
pub fn expect_xyk_pool_data(liquidity_source: &LiquiditySource) -> Result<&XYKPoolData, String> {
    match &liquidity_source.data {
        LiquiditySourceData::XYKPool(data) => Ok(data),
        _ => Err("wrong liquidity source data".to_owned()),
    }
}

/// Try to unwrap mutable reference `XYKPoolData` from `LiquiditySourceData` enum of `LiquiditySource` entity.
pub fn expect_xyk_pool_data_mut(
    liquidity_source: &mut LiquiditySource,
) -> Result<&mut XYKPoolData, String> {
    match &mut liquidity_source.data {
        LiquiditySourceData::XYKPool(data) => Ok(data),
        _ => Err("wrong liquidity source data".to_owned()),
    }
}

/// `LiquiditySource` represents an exchange pair between two assets in a domain. Assets are
/// identified by their AssetDefinitionId's. Containing DEX is identified by domain name.
#[derive(Encode, Decode, Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Io)]
pub enum LiquiditySourceData {
    /// Data representing state of the XYK liquidity pool.
    XYKPool(XYKPoolData),
    /// Data representing state of the Order Book.
    OrderBook, // this option currently is to prevent `irrefutable if-let pattern` warning
}

/// Liquidity Source entity belongs to particular Token Pair, exchange operations
/// are handled through it.
#[derive(Encode, Decode, Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Io)]
pub struct LiquiditySource {
    /// Identification of Liquidity source.
    pub id: <LiquiditySource as Identifiable>::Id,
    /// Varients represent LiquiditySourceType-specific data set for Liquidity Source.
    pub data: LiquiditySourceData,
}

impl Identifiable for LiquiditySource {
    type Id = LiquiditySourceId;
}

impl LiquiditySource {
    /// Default XYK Pool constructor.
    pub fn new_xyk_pool(
        token_pair_id: <TokenPair as Identifiable>::Id,
        pool_token_asset_definition_id: <AssetDefinition as Identifiable>::Id,
        storage_account_id: <Account as Identifiable>::Id,
    ) -> Self {
        let data = LiquiditySourceData::XYKPool(XYKPoolData::new(
            pool_token_asset_definition_id,
            storage_account_id,
        ));
        let id = LiquiditySourceId::new(token_pair_id, LiquiditySourceType::XYKPool);
        LiquiditySource { id, data }
    }
}

fn xyk_pool_token_asset_name(token_pair_id: &<TokenPair as Identifiable>::Id) -> String {
    format!("{} {}", XYK_POOL, token_pair_id.get_symbol())
}

fn xyk_pool_storage_account_name(token_pair_id: &<TokenPair as Identifiable>::Id) -> String {
    format!(
        "{} {} {}",
        STORAGE_ACCOUNT_NAME,
        XYK_POOL,
        token_pair_id.get_symbol()
    )
}

/// Iroha Special Instructions module provides helper-methods for `Peer` for operating DEX,
/// Token Pairs and Liquidity Sources.
pub mod isi {
    use super::*;
    use crate::dex::query::*;
    use crate::isi::prelude::*;
    use crate::permission::isi::PermissionInstruction;
    use std::collections::btree_map::Entry;

    /// Enumeration of all legal DEX related Instructions.
    #[derive(Clone, Debug, Io, Encode, Decode)]
    pub enum DEXInstruction {
        /// Variant of instruction to initialize `DEX` entity in `Domain`.
        InitializeDEX(DEX, <Domain as Identifiable>::Id),
        /// Variant of instruction to create new `TokenPair` entity in `DEX`.
        CreateTokenPair(TokenPair, <DEX as Identifiable>::Id),
        /// Variant of instruction to remove existing `TokenPair` entity from `DEX`.
        RemoveTokenPair(<TokenPair as Identifiable>::Id, <DEX as Identifiable>::Id),
        /// Variant of instruction to create liquidity source for existing `TokenPair`.
        CreateLiquiditySource(LiquiditySource, <TokenPair as Identifiable>::Id),
        /// Variant of instruction to deposit tokens to liquidity pool.
        /// `LiquiditySource` <-- Quantity Base Desired, Quantity Target Desired, Quantity Base Min, Quantity Target Min
        AddLiquidityToXYKPool(<LiquiditySource as Identifiable>::Id, u32, u32, u32, u32),
        /// Variant of instruction to withdraw tokens from liquidity pool by burning pool token.
        /// `LiquiditySource` --> Liquidity, Quantity Base Min, Quantity Target Min
        RemoveLiquidityFromXYKPool(<LiquiditySource as Identifiable>::Id, u32, u32, u32),
        /// Variant of instruction to swap with exact quantity of input tokens and receive corresponding quantity of output tokens.
        /// `AssetDefinition`'s, Input Quantity --> Output Quantity Min
        SwapExactTokensForTokensOnXYKPool(
            <DEX as Identifiable>::Id,
            Vec<<AssetDefinition as Identifiable>::Id>,
            u32,
            u32,
        ),
        /// Variant of instruction to swap with exact quantity of output tokens and send corresponding quantity of input tokens.
        /// `AssetDefinition`'a, Output Quantity --> Input Quantity Max
        SwapTokensForExactTokensOnXYKPool(
            <DEX as Identifiable>::Id,
            Vec<<AssetDefinition as Identifiable>::Id>,
            u32,
            u32,
        ),
        /// Variant of instruction to set value in basis points that is deduced from input quantity on swaps.
        SetFeeOnXYKPool(<LiquiditySource as Identifiable>::Id, u16),
        /// Variant of instruction to set value in basis points that is deduced from swap fees as protocol fee.
        SetProtocolFeePartOnXYKPool(<LiquiditySource as Identifiable>::Id, u16),
        /// Variant of instruction to mint permissions for account.
        /// TODO: this isi is debug-only and should be deleted when permission minting will be avaiable in core
        AddTransferPermissionForAccount(
            <AssetDefinition as Identifiable>::Id,
            <Account as Identifiable>::Id,
        ),
    }

    impl DEXInstruction {
        /// Executes `DEXInstruction` on the given `WorldStateView`.
        /// Returns `Ok(())` if execution succeeded and `Err(String)` with error message if not.
        pub fn execute(
            &self,
            authority: <Account as Identifiable>::Id,
            world_state_view: &mut WorldStateView,
        ) -> Result<(), String> {
            match self {
                DEXInstruction::InitializeDEX(dex, domain_name) => {
                    Register::new(dex.clone(), domain_name.clone())
                        .execute(authority, world_state_view)
                }
                DEXInstruction::CreateTokenPair(token_pair, dex_id) => {
                    Add::new(token_pair.clone(), dex_id.clone())
                        .execute(authority, world_state_view)
                }
                DEXInstruction::RemoveTokenPair(token_pair_id, dex_id) => {
                    Remove::new(token_pair_id.clone(), dex_id.clone())
                        .execute(authority, world_state_view)
                }
                DEXInstruction::CreateLiquiditySource(liquidity_source, token_pair_id) => {
                    Add::new(liquidity_source.clone(), token_pair_id.clone())
                        .execute(authority, world_state_view)
                }
                DEXInstruction::AddLiquidityToXYKPool(
                    liquidity_source_id,
                    amount_a_desired,
                    amount_b_desired,
                    amount_a_min,
                    amount_b_min,
                ) => xyk_pool_add_liquidity_execute(
                    liquidity_source_id.clone(),
                    amount_a_desired.clone(),
                    amount_b_desired.clone(),
                    amount_a_min.clone(),
                    amount_b_min.clone(),
                    authority.clone(),
                    authority,
                    world_state_view,
                ),
                DEXInstruction::RemoveLiquidityFromXYKPool(
                    liquidity_source_id,
                    liquidity,
                    amount_a_min,
                    amount_b_min,
                ) => xyk_pool_remove_liquidity_execute(
                    liquidity_source_id.clone(),
                    liquidity.clone(),
                    amount_a_min.clone(),
                    amount_b_min.clone(),
                    authority.clone(),
                    authority,
                    world_state_view,
                ),
                DEXInstruction::SwapExactTokensForTokensOnXYKPool(
                    dex_id,
                    path,
                    amount_in,
                    amount_out_min,
                ) => xyk_pool_swap_exact_tokens_for_tokens_execute(
                    dex_id.clone(),
                    &path,
                    amount_in.clone(),
                    amount_out_min.clone(),
                    authority.clone(),
                    authority,
                    world_state_view,
                ),
                DEXInstruction::SwapTokensForExactTokensOnXYKPool(
                    dex_id,
                    path,
                    amount_out,
                    amount_in_max,
                ) => xyk_pool_swap_tokens_for_exact_tokens_execute(
                    dex_id.clone(),
                    &path,
                    amount_out.clone(),
                    amount_in_max.clone(),
                    authority.clone(),
                    authority,
                    world_state_view,
                ),
                DEXInstruction::SetFeeOnXYKPool(liquidity_source_id, fee) => {
                    xyk_pool_set_fee_execute(
                        liquidity_source_id.clone(),
                        fee.clone(),
                        authority,
                        world_state_view,
                    )
                }

                DEXInstruction::SetProtocolFeePartOnXYKPool(
                    liquidity_source_id,
                    protocol_fee_part,
                ) => xyk_pool_set_protocol_fee_part_execute(
                    liquidity_source_id.clone(),
                    protocol_fee_part.clone(),
                    authority,
                    world_state_view,
                ),
                DEXInstruction::AddTransferPermissionForAccount(
                    asset_definition_id,
                    account_id,
                ) => add_transfer_permission_for_account_execute(
                    asset_definition_id.clone(),
                    account_id.clone(),
                    authority,
                    world_state_view,
                ),
            }
        }
    }

    /// Constructor of `Register<Domain, DEX>` ISI.
    ///
    /// Initializes DEX for the domain.
    pub fn initialize_dex(
        domain_name: &str,
        owner_account_id: <Account as Identifiable>::Id,
        base_asset_id: <AssetDefinition as Identifiable>::Id,
    ) -> Register<Domain, DEX> {
        Register {
            object: DEX::new(domain_name, owner_account_id, base_asset_id),
            destination_id: domain_name.to_owned(),
        }
    }

    impl Register<Domain, DEX> {
        pub(crate) fn execute(
            self,
            authority: <Account as Identifiable>::Id,
            world_state_view: &mut WorldStateView,
        ) -> Result<(), String> {
            let dex = self.object;
            let domain_name = self.destination_id;
            PermissionInstruction::CanManageDEX(authority, Some(domain_name.clone()))
                .execute(world_state_view)?;
            world_state_view
                .read_account(&dex.owner_account_id)
                .ok_or("account not found")?;
            let domain = get_domain_mut(&domain_name, world_state_view)?;
            if domain.dex.is_none() {
                domain.dex = Some(dex);
                Ok(())
            } else {
                Err("dex is already initialized for domain".to_owned())
            }
        }
    }

    impl From<Register<Domain, DEX>> for Instruction {
        fn from(instruction: Register<Domain, DEX>) -> Self {
            Instruction::DEX(DEXInstruction::InitializeDEX(
                instruction.object,
                instruction.destination_id,
            ))
        }
    }

    /// Constructor of `Add<DEX, TokenPair>` ISI.
    ///
    /// Creates new Token Pair via given asset ids for the DEX
    /// identified by its domain name.
    pub fn create_token_pair(
        base_asset: <AssetDefinition as Identifiable>::Id,
        target_asset: <AssetDefinition as Identifiable>::Id,
        domain_name: &str,
    ) -> Add<DEX, TokenPair> {
        let dex_id = DEXId::new(domain_name);
        Add {
            object: TokenPair::new(dex_id.clone(), base_asset, target_asset),
            destination_id: dex_id,
        }
    }

    impl Add<DEX, TokenPair> {
        pub(crate) fn execute(
            self,
            authority: <Account as Identifiable>::Id,
            world_state_view: &mut WorldStateView,
        ) -> Result<(), String> {
            let token_pair = self.object;
            let domain_name = self.destination_id.domain_name;
            PermissionInstruction::CanManageDEX(authority, Some(domain_name.clone()))
                .execute(world_state_view)?;
            let base_asset_definition = &token_pair.id.base_asset_id;
            let target_asset_definition = &token_pair.id.target_asset_id;
            let dex_base_asset_id = get_dex(&domain_name, world_state_view)?
                .base_asset_id
                .clone();
            let base_asset_domain =
                get_domain(&base_asset_definition.domain_name, world_state_view)?;
            let target_asset_domain =
                get_domain(&target_asset_definition.domain_name, world_state_view)?;
            if !base_asset_domain
                .asset_definitions
                .contains_key(base_asset_definition)
            {
                return Err(format!(
                    "base asset definition: {:?} not found",
                    base_asset_definition
                ));
            }
            if base_asset_definition != &dex_base_asset_id {
                return Err(format!(
                    "base asset definition is incorrect: {} != {}",
                    base_asset_definition, dex_base_asset_id
                ));
            }
            if !target_asset_domain
                .asset_definitions
                .contains_key(target_asset_definition)
            {
                return Err(format!(
                    "target asset definition: {:?} not found",
                    target_asset_definition
                ));
            }
            if base_asset_definition == target_asset_definition {
                return Err("assets in token pair must be different".to_owned());
            }
            let dex = get_dex_mut(&domain_name, world_state_view)?;
            match dex.token_pairs.entry(token_pair.id.clone()) {
                Entry::Occupied(_) => Err("token pair already exists".to_owned()),
                Entry::Vacant(entry) => {
                    entry.insert(token_pair);
                    Ok(())
                }
            }
        }
    }

    impl From<Add<DEX, TokenPair>> for Instruction {
        fn from(instruction: Add<DEX, TokenPair>) -> Self {
            Instruction::DEX(DEXInstruction::CreateTokenPair(
                instruction.object,
                instruction.destination_id,
            ))
        }
    }

    /// Constructor of `Remove<DEX, TokenPairId>` ISI.
    ///
    /// Removes existing Token Pair by its id from the DEX.
    pub fn remove_token_pair(
        token_pair_id: <TokenPair as Identifiable>::Id,
    ) -> Remove<DEX, <TokenPair as Identifiable>::Id> {
        let dex_id = DEXId::new(&token_pair_id.dex_id.domain_name);
        Remove {
            object: token_pair_id,
            destination_id: dex_id,
        }
    }

    impl Remove<DEX, <TokenPair as Identifiable>::Id> {
        pub(crate) fn execute(
            self,
            authority: <Account as Identifiable>::Id,
            world_state_view: &mut WorldStateView,
        ) -> Result<(), String> {
            let token_pair_id = self.object;
            PermissionInstruction::CanManageDEX(
                authority,
                Some(token_pair_id.dex_id.domain_name.clone()),
            )
            .execute(world_state_view)?;
            let dex = get_dex_mut(&token_pair_id.dex_id.domain_name, world_state_view)?;
            match dex.token_pairs.entry(token_pair_id.clone()) {
                Entry::Occupied(entry) => {
                    entry.remove();
                    Ok(())
                }
                Entry::Vacant(_) => Err("token pair does not exist".to_owned()),
            }
        }
    }

    impl From<Remove<DEX, <TokenPair as Identifiable>::Id>> for Instruction {
        fn from(instruction: Remove<DEX, <TokenPair as Identifiable>::Id>) -> Self {
            Instruction::DEX(DEXInstruction::RemoveTokenPair(
                instruction.object,
                instruction.destination_id,
            ))
        }
    }

    /// Constructor of `Add<DEX, LiquiditySource>` ISI.
    ///
    /// Add new XYK Liquidity Pool for DEX with given `TokenPair`.
    pub fn create_xyk_pool(token_pair_id: <TokenPair as Identifiable>::Id) -> Instruction {
        let domain_name = token_pair_id.dex_id.domain_name.clone();
        let asset_name = xyk_pool_token_asset_name(&token_pair_id);
        let pool_token_asset_definition =
            AssetDefinition::new(AssetDefinitionId::new(&asset_name, &domain_name));
        let account_name = xyk_pool_storage_account_name(&token_pair_id);
        let storage_account = Account::new(&account_name, &domain_name);
        Instruction::If(
            Box::new(Instruction::ExecuteQuery(IrohaQuery::GetTokenPair(
                GetTokenPair {
                    token_pair_id: token_pair_id.clone(),
                },
            ))),
            Box::new(Instruction::Sequence(vec![
                // register storage account for pool
                Register {
                    object: pool_token_asset_definition.clone(),
                    destination_id: domain_name.clone(),
                }
                .into(),
                // register asset definition for pool_token
                Register {
                    object: storage_account.clone(),
                    destination_id: domain_name.clone(),
                }
                .into(),
                // create xyk pool for pair
                Add {
                    object: LiquiditySource::new_xyk_pool(
                        token_pair_id.clone(),
                        pool_token_asset_definition.id.clone(),
                        storage_account.id.clone(),
                    ),
                    destination_id: token_pair_id,
                }
                .into(),
            ])),
            Some(Box::new(Instruction::Fail(
                "token pair not found".to_string(),
            ))),
        )
    }

    impl Add<TokenPair, LiquiditySource> {
        pub(crate) fn execute(
            self,
            authority: <Account as Identifiable>::Id,
            world_state_view: &mut WorldStateView,
        ) -> Result<(), String> {
            let liquidity_source = self.object;
            let token_pair_id = &liquidity_source.id.token_pair_id;
            PermissionInstruction::CanManageDEX(
                authority,
                Some(token_pair_id.dex_id.domain_name.clone()),
            )
            .execute(world_state_view)?;
            let token_pair = get_token_pair_mut(token_pair_id, world_state_view)?;
            match token_pair
                .liquidity_sources
                .entry(liquidity_source.id.clone())
            {
                Entry::Occupied(_) => {
                    Err("liquidity source already exists for token pair".to_owned())
                }
                Entry::Vacant(entry) => {
                    entry.insert(liquidity_source);
                    Ok(())
                }
            }
        }
    }

    impl From<Add<TokenPair, LiquiditySource>> for Instruction {
        fn from(instruction: Add<TokenPair, LiquiditySource>) -> Self {
            Instruction::DEX(DEXInstruction::CreateLiquiditySource(
                instruction.object,
                instruction.destination_id,
            ))
        }
    }

    /// Constructor if `AddLiquidityToXYKPool` ISI.
    pub fn xyk_pool_add_liquidity(
        liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        amount_a_desired: u32,
        amount_b_desired: u32,
        amount_a_min: u32,
        amount_b_min: u32,
    ) -> Instruction {
        Instruction::DEX(DEXInstruction::AddLiquidityToXYKPool(
            liquidity_source_id,
            amount_a_desired,
            amount_b_desired,
            amount_a_min,
            amount_b_min,
        ))
    }

    /// Core logic of `AddLiquidityToXYKPool` ISI, called by its `execute` function.
    ///
    /// `liquidity_source_id` - should be xyk pool.
    /// `amount_a_desired` - desired base asset quantity (maximum) to be deposited.
    /// `amount_b_desired` - desired target asset quantity (maximum) to be deposited.
    /// `amount_a_min` - lower bound for base asset quantity to be deposited.
    /// `amount_b_min` - lower bound for target asset quantity to be deposited.
    /// `to` - account to receive pool tokens for deposit.
    /// `authority` - permorms the operation, actual tokens are withdrawn from this account.
    fn xyk_pool_add_liquidity_execute(
        liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        amount_a_desired: u32,
        amount_b_desired: u32,
        amount_a_min: u32,
        amount_b_min: u32,
        to: <Account as Identifiable>::Id,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let liquidity_source = get_liquidity_source(&liquidity_source_id, world_state_view)?;
        let mut data = expect_xyk_pool_data(liquidity_source)?.clone();
        // calculate appropriate deposit quantities to preserve pool proportions
        let (amount_a, amount_b) = xyk_pool_get_optimal_deposit_amounts(
            data.base_asset_reserve.clone(),
            data.target_asset_reserve.clone(),
            amount_a_desired,
            amount_b_desired,
            amount_a_min,
            amount_b_min,
        )?;
        // deposit tokens into the storage account
        transfer_from(
            liquidity_source_id.token_pair_id.base_asset_id.clone(),
            authority.clone(),
            data.storage_account_id.clone(),
            amount_a.clone(),
            authority.clone(),
            world_state_view,
        )?;
        transfer_from(
            liquidity_source_id.token_pair_id.target_asset_id.clone(),
            authority.clone(),
            data.storage_account_id.clone(),
            amount_b.clone(),
            authority.clone(),
            world_state_view,
        )?;
        // mint pool_token for sender based on deposited amount
        xyk_pool_mint_pool_token_with_fee(
            to,
            liquidity_source_id.token_pair_id.clone(),
            amount_a,
            amount_b,
            world_state_view,
            &mut data,
        )?;
        // update pool data
        let liquidity_source = get_liquidity_source_mut(&liquidity_source_id, world_state_view)?;
        mem::replace(expect_xyk_pool_data_mut(liquidity_source)?, data);
        Ok(())
    }

    /// Based on given reserves, desired and minimal amounts to add liquidity, either return
    /// optimal values (needed to preserve reserves proportion) or error if it's not possible
    /// to keep proportion with proposed amounts.
    fn xyk_pool_get_optimal_deposit_amounts(
        reserve_a: u32,
        reserve_b: u32,
        amount_a_desired: u32,
        amount_b_desired: u32,
        amount_a_min: u32,
        amount_b_min: u32,
    ) -> Result<(u32, u32), String> {
        Ok(if reserve_a == 0u32 && reserve_b == 0u32 {
            (amount_a_desired, amount_b_desired)
        } else {
            let amount_b_optimal = xyk_pool_quote(
                amount_a_desired.clone(),
                reserve_a.clone(),
                reserve_b.clone(),
            )?;
            if amount_b_optimal <= amount_b_desired {
                if !(amount_b_optimal >= amount_b_min) {
                    return Err("insufficient b amount".to_owned());
                }
                (amount_a_desired, amount_b_optimal)
            } else {
                let amount_a_optimal =
                    xyk_pool_quote(amount_b_desired.clone(), reserve_b, reserve_a)?;
                assert!(amount_a_optimal <= amount_a_desired); // TODO: consider not using assert
                if !(amount_a_optimal >= amount_a_min) {
                    return Err("insufficient a amount".to_owned());
                }
                (amount_a_optimal, amount_b_desired)
            }
        })
    }

    /// Given some amount of an asset and pair reserves, returns an equivalent amount of the other Asset.
    fn xyk_pool_quote(amount_a: u32, reserve_a: u32, reserve_b: u32) -> Result<u32, String> {
        if !(amount_a > 0u32) {
            return Err("insufficient amount".to_owned());
        }
        if !(reserve_a > 0u32 && reserve_b > 0u32) {
            return Err("insufficient liquidity".to_owned());
        }
        Ok((amount_a * reserve_b) / reserve_a) // calculate amount_b via proportion
    }

    /// Helper function for performing token transfers.
    fn transfer_from(
        token: <AssetDefinition as Identifiable>::Id,
        from: <Account as Identifiable>::Id,
        to: <Account as Identifiable>::Id,
        value: u32,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let asset_id = AssetId::new(token, from.clone());
        AccountInstruction::TransferAsset(from, to, Asset::with_quantity(asset_id, value))
            .execute(authority, world_state_view)
    }

    /// Mint pool_token tokens representing liquidity in pool for depositing account.
    fn xyk_pool_mint_pool_token_with_fee(
        to: <Account as Identifiable>::Id,
        token_pair_id: <TokenPair as Identifiable>::Id,
        amount_a: u32,
        amount_b: u32,
        world_state_view: &mut WorldStateView,
        data: &mut XYKPoolData,
    ) -> Result<(), String> {
        let balance_a = get_asset_quantity(
            data.storage_account_id.clone(),
            token_pair_id.base_asset_id.clone(),
            world_state_view,
        )?;
        let balance_b = get_asset_quantity(
            data.storage_account_id.clone(),
            token_pair_id.target_asset_id.clone(),
            world_state_view,
        )?;

        xyk_pool_mint_pool_token_fee(world_state_view, data)?;

        let liquidity;
        if data.pool_token_total_supply == 0u32 {
            liquidity = (amount_a * amount_b).integer_sqrt() - MINIMUM_LIQUIDITY;
            data.pool_token_total_supply = MINIMUM_LIQUIDITY;
        } else {
            liquidity = cmp::min(
                (amount_a * data.pool_token_total_supply) / data.base_asset_reserve,
                (amount_b * data.pool_token_total_supply) / data.target_asset_reserve,
            );
        };
        if !(liquidity > 0u32) {
            return Err("insufficient liquidity minted".to_owned());
        }
        xyk_pool_mint_pool_token(to, liquidity, world_state_view, data)?;
        xyk_pool_update(balance_a, balance_b, world_state_view, data)?;
        if data.fee_to.is_some() {
            data.k_last = data.base_asset_reserve * data.target_asset_reserve;
        }
        Ok(())
    }

    fn xyk_pool_mint_pool_token_fee(
        world_state_view: &mut WorldStateView,
        data: &mut XYKPoolData,
    ) -> Result<(), String> {
        if let Some(fee_to) = data.fee_to.clone() {
            if data.k_last != 0u32 {
                let root_k = (data.base_asset_reserve * data.target_asset_reserve).integer_sqrt();
                let root_k_last = (data.k_last).integer_sqrt();
                if root_k > root_k_last {
                    let numerator = data.pool_token_total_supply * (root_k - root_k_last);
                    let demonimator = 5 * root_k + root_k_last;
                    let liquidity = numerator / demonimator;
                    if liquidity > 0u32 {
                        xyk_pool_mint_pool_token(fee_to, liquidity, world_state_view, data)?;
                    }
                }
            }
        } else if data.k_last != 0u32 {
            data.k_last = 0u32;
        }
        Ok(())
    }

    fn xyk_pool_mint_pool_token(
        to: <Account as Identifiable>::Id,
        quantity: u32,
        world_state_view: &mut WorldStateView,
        data: &mut XYKPoolData,
    ) -> Result<(), String> {
        let asset_id = AssetId::new(data.pool_token_asset_definition_id.clone(), to);
        mint_asset_unchecked(asset_id, quantity, world_state_view)?;
        data.pool_token_total_supply += quantity;
        Ok(())
    }

    /// Low-level function, should be called from function which performs important safety checks.
    fn mint_asset_unchecked(
        asset_id: <Asset as Identifiable>::Id,
        quantity: u32,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        world_state_view
            .asset_definition(&asset_id.definition_id)
            .ok_or("Failed to find asset.")?;
        match world_state_view.asset(&asset_id) {
            Some(asset) => {
                asset.quantity += quantity;
            }
            None => world_state_view.add_asset(Asset::with_quantity(asset_id.clone(), quantity)),
        }
        Ok(())
    }

    /// Update reserves records up to actual token balance.
    fn xyk_pool_update(
        balance_a: u32,
        balance_b: u32,
        _world_state_view: &mut WorldStateView,
        data: &mut XYKPoolData,
    ) -> Result<(), String> {
        data.base_asset_reserve = balance_a;
        data.target_asset_reserve = balance_b;

        // TODO: implement updates for oracle functionality
        Ok(())
    }

    /// Constructor if `RemoveLiquidityFromXYKPool` ISI.
    pub fn xyk_pool_remove_liquidity(
        liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        liquidity: u32,
        amount_a_min: u32,
        amount_b_min: u32,
    ) -> Instruction {
        Instruction::DEX(DEXInstruction::RemoveLiquidityFromXYKPool(
            liquidity_source_id,
            liquidity,
            amount_a_min,
            amount_b_min,
        ))
    }

    /// Core logic of `RemoveLiquidityToXYKPool` ISI, called by its `execute` function.
    ///
    /// `liquidity_source_id` - should be xyk pool.
    /// `liquidity` - desired pool token quantity (maximum) to be burned.
    /// `amount_a_min` - lower bound for base asset quantity to be withdrawn.
    /// `amount_b_min` - lower bound for target asset quantity to be withdrawn.
    /// `to` - account to receive pool tokens for deposit.
    /// `authority` - permorms the operation, actual tokens are withdrawn from this account.
    fn xyk_pool_remove_liquidity_execute(
        liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        liquidity: u32,
        amount_a_min: u32,
        amount_b_min: u32,
        to: <Account as Identifiable>::Id,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let liquidity_source = get_liquidity_source(&liquidity_source_id, world_state_view)?;
        let mut data = expect_xyk_pool_data(liquidity_source)?.clone();

        transfer_from(
            data.pool_token_asset_definition_id.clone(),
            authority.clone(),
            data.storage_account_id.clone(),
            liquidity,
            authority.clone(),
            world_state_view,
        )?;

        let (amount_a, amount_b) = xyk_pool_burn_pool_token_with_fee(
            to.clone(),
            liquidity_source_id.token_pair_id.clone(),
            liquidity,
            world_state_view,
            &mut data,
        )?;
        if !(amount_a >= amount_a_min) {
            return Err("insufficient a amount".to_owned());
        }
        if !(amount_b >= amount_b_min) {
            return Err("insufficient b amount".to_owned());
        }
        let liquidity_source = get_liquidity_source_mut(&liquidity_source_id, world_state_view)?;
        mem::replace(expect_xyk_pool_data_mut(liquidity_source)?, data);

        Ok(())
    }

    // returns (amount_a, amount_b)
    fn xyk_pool_burn_pool_token_with_fee(
        to: <Account as Identifiable>::Id,
        token_pair_id: <TokenPair as Identifiable>::Id,
        liquidity: u32,
        world_state_view: &mut WorldStateView,
        data: &mut XYKPoolData,
    ) -> Result<(u32, u32), String> {
        let balance_a = get_asset_quantity(
            data.storage_account_id.clone(),
            token_pair_id.base_asset_id.clone(),
            world_state_view,
        )?;
        let balance_b = get_asset_quantity(
            data.storage_account_id.clone(),
            token_pair_id.target_asset_id.clone(),
            world_state_view,
        )?;

        xyk_pool_mint_pool_token_fee(world_state_view, data)?;

        let amount_a = liquidity * balance_a / data.pool_token_total_supply;
        let amount_b = liquidity * balance_b / data.pool_token_total_supply;
        if !(amount_a > 0 && amount_b > 0) {
            return Err("insufficient liqudity burned".to_owned());
        }
        xyk_pool_burn_pool_token(
            data.storage_account_id.clone(),
            liquidity,
            world_state_view,
            data,
        )?;
        transfer_from_unchecked(
            token_pair_id.base_asset_id.clone(),
            data.storage_account_id.clone(),
            to.clone(),
            amount_a,
            world_state_view,
        )?;
        transfer_from_unchecked(
            token_pair_id.target_asset_id.clone(),
            data.storage_account_id.clone(),
            to.clone(),
            amount_b,
            world_state_view,
        )?;
        // update balances after transfers
        let balance_a = get_asset_quantity(
            data.storage_account_id.clone(),
            token_pair_id.base_asset_id.clone(),
            world_state_view,
        )?;
        let balance_b = get_asset_quantity(
            data.storage_account_id.clone(),
            token_pair_id.target_asset_id.clone(),
            world_state_view,
        )?;

        xyk_pool_update(balance_a, balance_b, world_state_view, data)?;

        if !(data.base_asset_reserve > 0 && data.target_asset_reserve > 0) {
            // TODO: this check is not present in original contract, consider if it's needed
            return Err("Insufficient reserves.".to_string());
        }
        if data.fee_to.is_some() {
            data.k_last = data.base_asset_reserve * data.target_asset_reserve;
        }
        Ok((amount_a, amount_b))
    }

    fn xyk_pool_burn_pool_token(
        from: <Account as Identifiable>::Id,
        value: u32,
        world_state_view: &mut WorldStateView,
        data: &mut XYKPoolData,
    ) -> Result<(), String> {
        let asset_id = AssetId::new(data.pool_token_asset_definition_id.clone(), from);
        burn_asset_unchecked(asset_id, value, world_state_view)?;
        data.pool_token_total_supply -= value;
        Ok(())
    }

    /// Helper function for burning tokens.
    /// Low-level function, should be called from function which performs important safety checks.
    fn burn_asset_unchecked(
        asset_id: <Asset as Identifiable>::Id,
        quantity: u32,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        world_state_view
            .asset_definition(&asset_id.definition_id)
            .ok_or("Failed to find asset definition.")?;
        match world_state_view.asset(&asset_id) {
            Some(asset) => {
                if quantity > asset.quantity {
                    return Err("Insufficient asset quantity to burn.".to_string());
                }
                asset.quantity -= quantity;
            }
            None => return Err("Account does not contain the asset.".to_string()),
        }
        Ok(())
    }

    /// Helper function for performing asset transfers.
    /// Low-level function, should be called from function which performs important safety checks.
    fn transfer_from_unchecked(
        asset_definition_id: <AssetDefinition as Identifiable>::Id,
        from: <Account as Identifiable>::Id,
        to: <Account as Identifiable>::Id,
        value: u32,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let asset_id = AssetId::new(asset_definition_id.clone(), from.clone());
        let asset = Asset::with_quantity(asset_id.clone(), value);
        let source = world_state_view
            .account(&from)
            .ok_or("Failed to find accounts.")?
            .assets
            .get_mut(&asset_id)
            .ok_or("Asset's component was not found.")?;
        let quantity_to_transfer = asset.quantity;
        if source.quantity < quantity_to_transfer {
            return Err(format!("Not enough assets: {:?}, {:?}.", source, asset));
        }
        source.quantity -= quantity_to_transfer;
        let transferred_asset = {
            let mut object = asset.clone();
            object.id.account_id = to.clone();
            object
        };

        world_state_view
            .account(&to)
            .ok_or("Failed to find destination account.")?
            .assets
            .entry(transferred_asset.id.clone())
            .and_modify(|asset| asset.quantity += quantity_to_transfer)
            .or_insert(transferred_asset);
        Ok(())
    }

    /// Constructor of `SwapExactTokensForTokensOnXYKPool` ISI.
    pub fn xyk_pool_swap_exact_tokens_for_tokens(
        dex_id: <DEX as Identifiable>::Id,
        path: Vec<<AssetDefinition as Identifiable>::Id>,
        amount_in: u32,
        amount_out_min: u32,
    ) -> Instruction {
        Instruction::DEX(DEXInstruction::SwapExactTokensForTokensOnXYKPool(
            dex_id,
            path,
            amount_in,
            amount_out_min,
        ))
    }

    /// Core logic of `SwapExactTokensForTokensOnXYKPool` ISI, called by its `execute` function.
    ///
    /// `path` - chain of tokens, each pair represents a token pair with active xyk pool.
    /// `amount_in` - desired input asset quantity.
    /// `amount_out_min` - minimum expected output asset quantity.
    /// `to` - account to receive output tokens.
    /// `authority` - performs the operation, actual tokens are withdrawn from this account.
    fn xyk_pool_swap_exact_tokens_for_tokens_execute(
        dex_id: <DEX as Identifiable>::Id,
        path: &[<AssetDefinition as Identifiable>::Id],
        amount_in: u32,
        amount_out_min: u32,
        to: <Account as Identifiable>::Id,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let amounts = xyk_pool_get_amounts_out(dex_id.clone(), amount_in, path, world_state_view)?;
        if !(amounts.last().unwrap() >= &amount_out_min) {
            return Err("insufficient output amount".to_owned());
        }
        xyk_pool_swap_tokens_execute(dex_id, path, &amounts, to, authority, world_state_view)
    }

    /// Constructor of `SwapTokensForExactTokensOnXYKPool` ISI.
    pub fn xyk_pool_swap_tokens_for_exact_tokens(
        dex_id: <DEX as Identifiable>::Id,
        path: Vec<<AssetDefinition as Identifiable>::Id>,
        amount_out: u32,
        amount_in_max: u32,
    ) -> Instruction {
        Instruction::DEX(DEXInstruction::SwapTokensForExactTokensOnXYKPool(
            dex_id,
            path,
            amount_out,
            amount_in_max,
        ))
    }

    /// Core logic of `SwapTokensForExactTokensOnXYKPool` ISI, called by its `execute` function.
    ///
    /// `path` - chain of tokens, each pair represents a token pair with active xyk pool.
    /// `amount_out` - desired output asset quantity.
    /// `amount_in_max` - maximum expected input asset quantity.
    /// `to` - account to receive output tokens.
    /// `authority` - performs the operation, actual tokens are withdrawn from this account.
    fn xyk_pool_swap_tokens_for_exact_tokens_execute(
        dex_id: <DEX as Identifiable>::Id,
        path: &[<AssetDefinition as Identifiable>::Id],
        amount_out: u32,
        amount_in_max: u32,
        to: <Account as Identifiable>::Id,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let amounts = xyk_pool_get_amounts_in(dex_id.clone(), amount_out, path, world_state_view)?;
        if !(amounts.first().unwrap() <= &amount_in_max) {
            return Err("excessive input amount".to_owned());
        }
        xyk_pool_swap_tokens_execute(dex_id, path, &amounts, to, authority, world_state_view)
    }

    /// Entry point for SwapTokens-related ISI's.
    ///
    /// `path` - chain of tokens, each pair represents a token pair with active xyk pool.
    /// `amounts` - amounts of each token to be swapped on pools.
    /// `to` - account to receive output tokens.
    /// `authority` - performs the operation, actual tokens are withdrawn from this account.
    fn xyk_pool_swap_tokens_execute(
        dex_id: <DEX as Identifiable>::Id,
        path: &[<AssetDefinition as Identifiable>::Id],
        amounts: &[u32],
        to: <Account as Identifiable>::Id,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let first_pool_id = liquidity_source_id_for_tokens(
            path.get(0).unwrap().clone(),
            path.get(1).unwrap().clone(),
            LiquiditySourceType::XYKPool,
            dex_id.clone(),
            world_state_view,
        )?;
        let first_pool = get_liquidity_source(&first_pool_id, world_state_view)?;
        let first_pool_data = expect_xyk_pool_data(&first_pool)?;
        transfer_from(
            path.first().unwrap().clone(),
            authority.clone(),
            first_pool_data.storage_account_id.clone(),
            amounts.first().unwrap().clone(),
            authority.clone(),
            world_state_view,
        )?;
        xyk_pool_swap_all(dex_id, amounts, path, to, world_state_view)?;
        Ok(())
    }

    /// Given unordered pair of asset ids, construct id for
    /// corresponding liquidity source.
    fn liquidity_source_id_for_tokens(
        asset_a_id: <AssetDefinition as Identifiable>::Id,
        asset_b_id: <AssetDefinition as Identifiable>::Id,
        liquidity_source_type: LiquiditySourceType,
        dex_id: <DEX as Identifiable>::Id,
        world_state_view: &WorldStateView,
    ) -> Result<<LiquiditySource as Identifiable>::Id, String> {
        let dex = get_dex(&dex_id.domain_name, world_state_view)?;
        // sort tokens - find base and target
        let (base_asset, target_asset) = if asset_a_id == dex.base_asset_id {
            (asset_a_id, asset_b_id)
        } else if asset_b_id == dex.base_asset_id {
            (asset_b_id, asset_a_id)
        } else {
            return Err("neither of tokens is base asset".to_owned());
        };
        let token_pair_id = TokenPairId::new(dex_id, base_asset, target_asset);
        Ok(LiquiditySourceId::new(token_pair_id, liquidity_source_type))
    }

    /// Given ordered pair of asset ids, return ordered reserve quantities
    /// from corresponding xyk pool.
    fn xyk_pool_get_reserves(
        dex_id: <DEX as Identifiable>::Id,
        asset_a_id: <AssetDefinition as Identifiable>::Id,
        asset_b_id: <AssetDefinition as Identifiable>::Id,
        world_state_view: &WorldStateView,
    ) -> Result<(u32, u32), String> {
        let xyk_pool_id = liquidity_source_id_for_tokens(
            asset_a_id.clone(),
            asset_b_id.clone(),
            LiquiditySourceType::XYKPool,
            dex_id.clone(),
            world_state_view,
        )?;
        let xyk_pool = get_liquidity_source(&xyk_pool_id, world_state_view)?;
        let data = expect_xyk_pool_data(&xyk_pool)?;
        let dex = get_dex(&dex_id.domain_name, world_state_view)?;
        if asset_a_id == dex.base_asset_id {
            Ok((data.base_asset_reserve, data.target_asset_reserve))
        } else if asset_b_id == dex.base_asset_id {
            Ok((data.target_asset_reserve, data.base_asset_reserve))
        } else {
            Err("neither of tokens is base asset".to_owned())
        }
    }

    /// Performs chained get_amount_out calculations on any number of pairs.
    pub fn xyk_pool_get_amounts_out(
        dex_id: <DEX as Identifiable>::Id,
        amount_in: u32,
        path: &[<AssetDefinition as Identifiable>::Id],
        world_state_view: &WorldStateView,
    ) -> Result<Vec<u32>, String> {
        if !(path.len() >= 2) {
            return Err("invalid path".to_owned());
        }

        let mut amounts = Vec::new();
        amounts.push(amount_in);
        for i in 0..path.len() - 1 {
            let (reserve_in, reserve_out) = xyk_pool_get_reserves(
                dex_id.clone(),
                path.get(i).unwrap().clone(),
                path.get(i + 1).unwrap().clone(),
                world_state_view,
            )?;
            amounts.push(xyk_pool_get_amount_out(
                amounts.last().unwrap().clone(),
                reserve_in,
                reserve_out,
            )?);
        }
        Ok(amounts)
    }

    /// Given an input amount of an asset and pair reserves, returns
    /// the maximum output amount of the other asset.
    pub fn xyk_pool_get_amount_out(
        amount_in: u32,
        reserve_in: u32,
        reserve_out: u32,
    ) -> Result<u32, String> {
        if !(amount_in > 0) {
            return Err("insufficient input amount".to_owned());
        }
        if !(reserve_in > 0 && reserve_out > 0) {
            return Err("insufficient liquidity".to_owned());
        }
        let amount_in_with_fee = amount_in as u128 * 997;
        let numerator = amount_in_with_fee * reserve_out as u128;
        let denominator = (reserve_in as u128 * 1000) + amount_in_with_fee;
        Ok((numerator / denominator) as u32)
    }

    /// Performs chained get_amount_in calculations on any number of pairs.
    pub fn xyk_pool_get_amounts_in(
        dex_id: <DEX as Identifiable>::Id,
        amount_out: u32,
        path: &[<AssetDefinition as Identifiable>::Id],
        world_state_view: &WorldStateView,
    ) -> Result<Vec<u32>, String> {
        if !(path.len() >= 2) {
            return Err("invalid path".to_owned());
        }
        let mut amounts = Vec::new();
        amounts.push(amount_out);
        for i in (1..path.len()).rev() {
            let (reserve_in, reserve_out) = xyk_pool_get_reserves(
                dex_id.clone(),
                path.get(i - 1).unwrap().clone(),
                path.get(i).unwrap().clone(),
                world_state_view,
            )?;
            amounts.push(xyk_pool_get_amount_in(
                amounts.last().unwrap().clone(),
                reserve_in,
                reserve_out,
            )?);
        }
        amounts.reverse();
        Ok(amounts)
    }

    /// Given an output amount of an asset and pair reserves, returns a required
    /// input amount of the other asset.
    pub fn xyk_pool_get_amount_in(
        amount_out: u32,
        reserve_in: u32,
        reserve_out: u32,
    ) -> Result<u32, String> {
        if !(amount_out > 0) {
            return Err("insufficient output amount".to_owned());
        }
        if !(reserve_in > 0 && reserve_out > 0) {
            return Err("insufficient liquidity".to_owned());
        }
        if !(reserve_out != amount_out) {
            return Err("can't withdraw full reserve".to_owned());
        }
        let numerator = reserve_in as u128 * amount_out as u128 * 1000;
        let denominator = (reserve_out as u128 - amount_out as u128) * 997;
        Ok(((numerator / denominator) + 1) as u32)
    }

    /// Iterate through the path with according amounts and perform swaps on
    /// each of xyk pools.
    fn xyk_pool_swap_all(
        dex_id: <DEX as Identifiable>::Id,
        amounts: &[u32],
        path: &[<AssetDefinition as Identifiable>::Id],
        to: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let dex = get_dex(&dex_id.domain_name, world_state_view)?;
        let dex_base_asset_id = dex.base_asset_id.clone();
        for i in 0..path.len() - 1 {
            let (input_asset_id, output_asset_id) =
                (path.get(i).unwrap(), path.get(i + 1).unwrap());
            let amount_out = amounts.get(i + 1).unwrap().clone();
            // sort tokens
            let ((base_quantity_out, target_quantity_out), (base_asset_id, target_asset_id)) =
                if input_asset_id == &dex_base_asset_id {
                    ((0u32, amount_out), (input_asset_id, output_asset_id))
                } else if output_asset_id == &dex_base_asset_id {
                    ((amount_out, 0u32), (output_asset_id, input_asset_id))
                } else {
                    return Err("neither of tokens is base asset".to_owned());
                };
            // determine either next pair to receive swapped token or recepient account
            let next_account = if i < path.len() - 2 {
                let xyk_pool_id = liquidity_source_id_for_tokens(
                    output_asset_id.clone(),
                    path.get(i + 2).unwrap().clone(),
                    LiquiditySourceType::XYKPool,
                    dex_id.clone(),
                    world_state_view,
                )?;
                let xyk_pool = get_liquidity_source(&xyk_pool_id, world_state_view)?;
                expect_xyk_pool_data(&xyk_pool)?.storage_account_id.clone()
            } else {
                to.clone()
            };
            // perform swap on pool for pair
            xyk_pool_swap(
                dex_id.clone(),
                base_asset_id.clone(),
                target_asset_id.clone(),
                base_quantity_out,
                target_quantity_out,
                next_account,
                world_state_view,
            )?;
        }
        Ok(())
    }

    /// Assuming that input tokens are already deposited into pool,
    /// withdraw corresponding output tokens.
    fn xyk_pool_swap(
        dex_id: <DEX as Identifiable>::Id,
        base_asset_id: <AssetDefinition as Identifiable>::Id,
        target_asset_id: <AssetDefinition as Identifiable>::Id,
        base_amount_out: u32,
        target_amount_out: u32,
        to: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        if !(base_amount_out > 0 || target_amount_out > 0) {
            return Err("insufficient output amount".to_owned());
        }
        let xyk_pool_id = liquidity_source_id_for_tokens(
            base_asset_id.clone(),
            target_asset_id.clone(),
            LiquiditySourceType::XYKPool,
            dex_id,
            world_state_view,
        )?;
        let xyk_pool = get_liquidity_source(&xyk_pool_id, world_state_view)?;
        let mut data = expect_xyk_pool_data(xyk_pool)?.clone();
        if !(base_amount_out < data.base_asset_reserve
            && target_amount_out < data.target_asset_reserve)
        {
            return Err("insufficient liquidity".to_owned());
        }
        if base_amount_out > 0 {
            transfer_from_unchecked(
                base_asset_id.clone(),
                data.storage_account_id.clone(),
                to.clone(),
                base_amount_out,
                world_state_view,
            )?;
        }
        if target_amount_out > 0 {
            transfer_from_unchecked(
                target_asset_id.clone(),
                data.storage_account_id.clone(),
                to.clone(),
                target_amount_out,
                world_state_view,
            )?;
        }
        let base_balance = get_asset_quantity(
            data.storage_account_id.clone(),
            base_asset_id.clone(),
            world_state_view,
        )?;
        let target_balance = get_asset_quantity(
            data.storage_account_id.clone(),
            target_asset_id.clone(),
            world_state_view,
        )?;
        let base_amount_in = if base_balance > data.base_asset_reserve - base_amount_out {
            base_balance - (data.base_asset_reserve - base_amount_out)
        } else {
            0
        };
        let target_amount_in = if target_balance > data.target_asset_reserve - target_amount_out {
            target_balance - (data.target_asset_reserve - target_amount_out)
        } else {
            0
        };
        if !(base_amount_in > 0 || target_amount_in > 0) {
            return Err("insufficient input amount".to_owned());
        }
        let base_balance_adjusted = (base_balance as u128 * 1000) - (base_amount_in as u128 * 3);
        let target_balance_adjusted =
            (target_balance as u128 * 1000) - (target_amount_in as u128 * 3);
        if !(base_balance_adjusted * target_balance_adjusted
            >= data.base_asset_reserve as u128
                * data.target_asset_reserve as u128
                * 1000u128.pow(2))
        {
            return Err("k error".to_owned());
        }
        xyk_pool_update(base_balance, target_balance, world_state_view, &mut data)?;
        let xyk_pool = get_liquidity_source_mut(&xyk_pool_id, world_state_view)?;
        mem::replace(expect_xyk_pool_data_mut(xyk_pool)?, data);
        Ok(())
    }

    /// Constructor of `AddTransferPermissionForAccount` ISI
    pub fn add_transfer_permission_for_account(
        asset_definition_id: <AssetDefinition as Identifiable>::Id,
        account_id: <Account as Identifiable>::Id,
    ) -> Instruction {
        Instruction::DEX(DEXInstruction::AddTransferPermissionForAccount(
            asset_definition_id,
            account_id,
        ))
    }

    /// Mint permission for account.
    /// TODO: this is temporary function made for debug purposes, remove when permission minting is avaiable in core
    fn add_transfer_permission_for_account_execute(
        asset_definition_id: <AssetDefinition as Identifiable>::Id,
        account_id: <Account as Identifiable>::Id,
        _authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let domain_name = account_id.domain_name.clone();
        let domain = get_domain_mut(&domain_name, world_state_view)?;
        let asset_id = AssetId {
            definition_id: permission_asset_definition_id(),
            account_id: account_id.clone(),
        };
        domain
            .accounts
            .get_mut(&account_id)
            .ok_or("failed to find account")?
            .assets
            .entry(asset_id.clone())
            .and_modify(|asset| {
                let permission = Permission::TransferAsset(None, Some(asset_definition_id.clone()));
                if !asset.permissions.origin.contains(&permission) {
                    asset.permissions.origin.push(permission);
                }
            })
            .or_insert(Asset::with_permission(
                asset_id.clone(),
                Permission::TransferAsset(None, Some(asset_definition_id.clone())),
            ));
        Ok(())
    }

    fn xyk_pool_set_fee_execute(
        liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        fee: u16,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let token_pair_id = &liquidity_source_id.token_pair_id;
        PermissionInstruction::CanManageDEX(
            authority,
            Some(token_pair_id.dex_id.domain_name.clone()),
        )
        .execute(world_state_view)?;
        let liquidity_source = get_liquidity_source_mut(&liquidity_source_id, world_state_view)?;
        let xyk_pool = expect_xyk_pool_data_mut(liquidity_source)?;
        if fee > MAX_BASIS_POINTS {
            return Err("fee could not be greater than 100 percent".to_owned());
        }
        xyk_pool.fee = fee;
        Ok(())
    }

    fn xyk_pool_set_protocol_fee_part_execute(
        liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        protocol_fee_part: u16,
        authority: <Account as Identifiable>::Id,
        world_state_view: &mut WorldStateView,
    ) -> Result<(), String> {
        let token_pair_id = &liquidity_source_id.token_pair_id;
        PermissionInstruction::CanManageDEX(
            authority,
            Some(token_pair_id.dex_id.domain_name.clone()),
        )
        .execute(world_state_view)?;
        let liquidity_source = get_liquidity_source_mut(&liquidity_source_id, world_state_view)?;
        let xyk_pool = expect_xyk_pool_data_mut(liquidity_source)?;
        if protocol_fee_part > MAX_BASIS_POINTS {
            return Err("protocol fee fraction could not be greater than 100 percent".to_owned());
        }
        xyk_pool.protocol_fee_part = protocol_fee_part;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::account::query::*;
        use crate::peer::PeerId;
        use crate::query::QueryResult;
        use std::collections::BTreeMap;

        struct TestKit {
            world_state_view: WorldStateView,
            root_account_id: <Account as Identifiable>::Id,
            dex_owner_account_id: <Account as Identifiable>::Id,
            domain_name: <Domain as Identifiable>::Id,
            base_asset_id: <AssetDefinition as Identifiable>::Id,
        }

        impl TestKit {
            pub fn new() -> Self {
                let domain_name = "Soramitsu".to_string();
                let base_asset_id = AssetDefinitionId::new("XOR", &domain_name);
                let key_pair = KeyPair::generate().expect("Failed to generate KeyPair.");
                let mut asset_definitions = BTreeMap::new();
                let mut accounts = BTreeMap::new();

                let permission_asset_definition_id = permission_asset_definition_id();
                asset_definitions.insert(
                    permission_asset_definition_id.clone(),
                    AssetDefinition::new(permission_asset_definition_id.clone()),
                );

                let root_account_id = AccountId::new("root", &domain_name);
                let asset_id = AssetId {
                    definition_id: permission_asset_definition_id.clone(),
                    account_id: root_account_id.clone(),
                };
                let asset = Asset::with_permission(asset_id.clone(), Permission::Anything);
                let mut account = Account::with_signatory(
                    &root_account_id.name,
                    &root_account_id.domain_name,
                    key_pair.public_key.clone(),
                );
                account.assets.insert(asset_id.clone(), asset.clone());
                accounts.insert(root_account_id.clone(), account);

                let key_pair = KeyPair::generate().expect("Failed to generate KeyPair.");
                let dex_owner_account_id = AccountId::new("dex owner", &domain_name);
                let asset_id = AssetId {
                    definition_id: permission_asset_definition_id.clone(),
                    account_id: dex_owner_account_id.clone(),
                };

                let asset = Asset::with_permissions(
                    asset_id.clone(),
                    &[
                        Permission::ManageDEX(Some(dex_owner_account_id.domain_name.clone())),
                        Permission::RegisterAccount(None),
                        Permission::RegisterAssetDefinition(None),
                    ],
                );
                let mut account = Account::with_signatory(
                    &dex_owner_account_id.name,
                    &dex_owner_account_id.domain_name,
                    key_pair.public_key.clone(),
                );

                account.assets.insert(asset_id.clone(), asset);
                accounts.insert(dex_owner_account_id.clone(), account);

                let domain = Domain {
                    name: domain_name.clone(),
                    accounts,
                    asset_definitions,
                    ..Default::default()
                };
                let mut domains = BTreeMap::new();
                domains.insert(domain_name.clone(), domain);
                let address = "127.0.0.1:8080".to_string();
                let world_state_view = WorldStateView::new(Peer::with_domains(
                    PeerId {
                        address: address.clone(),
                        public_key: key_pair.public_key,
                    },
                    &Vec::new(),
                    domains,
                ));
                TestKit {
                    world_state_view,
                    root_account_id,
                    dex_owner_account_id,
                    domain_name,
                    base_asset_id,
                }
            }

            fn initialize_dex(&mut self) {
                let world_state_view = &mut self.world_state_view;

                // initialize dex in domain
                initialize_dex(
                    &self.domain_name,
                    self.dex_owner_account_id.clone(),
                    self.base_asset_id.clone(),
                )
                .execute(self.dex_owner_account_id.clone(), world_state_view)
                .expect("failed to initialize dex");
            }

            fn register_domain(&mut self, domain_name: &str) {
                let domain = Domain::new(domain_name.to_owned());
                self.world_state_view.add_domain(domain);
            }

            fn register_asset(&mut self, asset: &str) -> <AssetDefinition as Identifiable>::Id {
                let world_state_view = &mut self.world_state_view;
                let asset_definition = AssetDefinition::new(AssetDefinitionId::from(asset));
                let domain = world_state_view
                    .read_domain(&asset_definition.id.domain_name)
                    .expect("domain not found")
                    .clone();

                domain
                    .register_asset(asset_definition.clone())
                    .execute(self.root_account_id.clone(), world_state_view)
                    .expect("failed to register asset");
                asset_definition.id.clone()
            }

            fn create_token_pair(
                &mut self,
                base_asset: &str,
                target_asset: &str,
            ) -> <TokenPair as Identifiable>::Id {
                let world_state_view = &mut self.world_state_view;
                let asset_definition_a_id = AssetDefinitionId::from(base_asset);
                let asset_definition_b_id = AssetDefinitionId::from(target_asset);

                // register pair for exchange assets
                create_token_pair(
                    asset_definition_a_id.clone(),
                    asset_definition_b_id.clone(),
                    &self.domain_name,
                )
                .execute(self.dex_owner_account_id.clone(), world_state_view)
                .expect("create token pair failed");

                // create resulting token pair id
                let token_pair_id = TokenPairId::new(
                    DEXId::new(&self.domain_name),
                    asset_definition_a_id.clone(),
                    asset_definition_b_id.clone(),
                );
                token_pair_id
            }

            fn mint_asset(&mut self, asset: &str, account: &str, quantity: u32) {
                let asset_definition_id = AssetDefinitionId::from(asset);
                let account_id = AccountId::from(account);
                let asset_id = AssetId::new(asset_definition_id, account_id);
                Mint::new(quantity, asset_id)
                    .execute(self.root_account_id.clone(), &mut self.world_state_view)
                    .expect("mint asset failed");
            }

            fn xyk_pool_create(
                &mut self,
                token_pair_id: <TokenPair as Identifiable>::Id,
            ) -> (
                <LiquiditySource as Identifiable>::Id,
                <Account as Identifiable>::Id,
                <AssetDefinition as Identifiable>::Id,
            ) {
                create_xyk_pool(token_pair_id.clone())
                    .execute(
                        self.dex_owner_account_id.clone(),
                        &mut self.world_state_view,
                    )
                    .expect("create xyk pool failed");
                let xyk_pool_id =
                    LiquiditySourceId::new(token_pair_id.clone(), LiquiditySourceType::XYKPool);
                let storage_account_id = AccountId::new(
                    &xyk_pool_storage_account_name(&token_pair_id),
                    &self.domain_name,
                );
                let pool_token_asset_definition_id = AssetDefinitionId::new(
                    &xyk_pool_token_asset_name(&token_pair_id),
                    &self.domain_name,
                );
                (
                    xyk_pool_id,
                    storage_account_id,
                    pool_token_asset_definition_id,
                )
            }

            fn create_account(&mut self, account: &str) -> <Account as Identifiable>::Id {
                let world_state_view = &mut self.world_state_view;
                let domain = world_state_view
                    .read_domain(&self.domain_name)
                    .expect("domain not found")
                    .clone();
                let key_pair = KeyPair::generate().expect("Failed to generate KeyPair.");
                let account_id = AccountId::from(account);
                let account = Account::with_signatory(
                    &account_id.name,
                    &account_id.domain_name,
                    key_pair.public_key.clone(),
                );
                domain
                    .register_account(account)
                    .execute(self.root_account_id.clone(), world_state_view)
                    .expect("failed to create account");
                account_id
            }

            fn add_transfer_permission(&mut self, account: &str, asset: &str) {
                let world_state_view = &mut self.world_state_view;
                let account_id = AccountId::from(account);
                let asset_definition_id = AssetDefinitionId::from(asset);
                add_transfer_permission_for_account(asset_definition_id, account_id)
                    .execute(self.root_account_id.clone(), world_state_view)
                    .expect("failed to add transfer permission");
            }

            fn check_xyk_pool_state(
                &self,
                pool_token_total_supply: u32,
                base_asset_reserve: u32,
                target_asset_reserve: u32,
                k_last: u32,
                liquidity_source_id: &<LiquiditySource as Identifiable>::Id,
            ) {
                let liquidity_source =
                    get_liquidity_source(liquidity_source_id, &self.world_state_view).unwrap();
                let pool_data = expect_xyk_pool_data(liquidity_source).unwrap();
                assert_eq!(pool_data.pool_token_total_supply, pool_token_total_supply);
                assert_eq!(pool_data.base_asset_reserve, base_asset_reserve);
                assert_eq!(pool_data.target_asset_reserve, target_asset_reserve);
                assert_eq!(pool_data.k_last, k_last);
            }

            fn check_xyk_pool_storage_account(
                &self,
                base_asset_balance: u32,
                target_asset_balance: u32,
                liquidity_source_id: &<LiquiditySource as Identifiable>::Id,
            ) {
                let liquidity_source =
                    get_liquidity_source(liquidity_source_id, &self.world_state_view).unwrap();
                let pool_data = expect_xyk_pool_data(liquidity_source).unwrap();
                if let QueryResult::GetAccount(account_result) =
                    GetAccount::build_request(pool_data.storage_account_id.clone())
                        .query
                        .execute(&self.world_state_view)
                        .expect("failed to query token pair")
                {
                    let storage_base_asset_id = AssetId::new(
                        liquidity_source_id.token_pair_id.base_asset_id.clone(),
                        pool_data.storage_account_id.clone(),
                    );
                    let storage_target_asset_id = AssetId::new(
                        liquidity_source_id.token_pair_id.target_asset_id.clone(),
                        pool_data.storage_account_id.clone(),
                    );
                    let account = account_result.account;
                    let base_asset = account
                        .assets
                        .get(&storage_base_asset_id)
                        .expect("failed to get base asset");
                    let target_asset = account
                        .assets
                        .get(&storage_target_asset_id)
                        .expect("failed to get target asset");
                    assert_eq!(base_asset.quantity.clone(), base_asset_balance);
                    assert_eq!(target_asset.quantity.clone(), target_asset_balance);
                } else {
                    panic!("wrong enum variant returned for GetAccount");
                }
            }

            fn check_asset_amount(&self, account: &str, asset: &str, amount: u32) {
                let account_id = AccountId::from(account);
                let asset_definition_id = AssetDefinitionId::from(asset);
                let quantity =
                    get_asset_quantity(account_id, asset_definition_id, &self.world_state_view)
                        .unwrap();
                assert_eq!(quantity, amount);
            }
        }

        #[test]
        fn test_initialize_dex_should_pass() {
            let mut testkit = TestKit::new();
            let domain_name = testkit.domain_name.clone();

            // get world state view and dex domain
            let world_state_view = &mut testkit.world_state_view;

            initialize_dex(
                &domain_name,
                testkit.dex_owner_account_id.clone(),
                AssetDefinitionId::new("XOR", &domain_name),
            )
            .execute(testkit.dex_owner_account_id.clone(), world_state_view)
            .expect("failed to initialize dex");

            let dex_query_result =
                get_dex(&domain_name, world_state_view).expect("query dex failed");
            assert_eq!(&dex_query_result.id.domain_name, &domain_name);

            if let QueryResult::GetDEXList(dex_list_result) = GetDEXList::build_request()
                .query
                .execute(world_state_view)
                .expect("failed to query dex list")
            {
                assert_eq!(&dex_list_result.dex_list, &[dex_query_result.clone()])
            } else {
                panic!("wrong enum variant returned for GetDEXList");
            }
        }

        #[test]
        fn test_initialize_dex_should_fail_with_permission_not_found() {
            let mut testkit = TestKit::new();
            let domain_name = testkit.domain_name.clone();

            // create dex owner account
            let dex_owner_public_key = KeyPair::generate()
                .expect("Failed to generate KeyPair.")
                .public_key;
            let dex_owner_account =
                Account::with_signatory("dex_owner", &domain_name, dex_owner_public_key);

            // get world state view and dex domain
            let world_state_view = &mut testkit.world_state_view;
            let domain = world_state_view
                .domain(&domain_name)
                .expect("domain not found")
                .clone();

            // register dex owner account
            let register_account = domain.register_account(dex_owner_account.clone());
            register_account
                .execute(testkit.root_account_id.clone(), world_state_view)
                .expect("failed to register dex owner account");

            assert!(initialize_dex(
                &domain_name,
                dex_owner_account.id.clone(),
                AssetDefinitionId::new("XOR", &domain_name)
            )
            .execute(dex_owner_account.id.clone(), world_state_view)
            .unwrap_err()
            .contains("Error: Permission not found."));
        }

        #[test]
        fn test_create_and_delete_token_pair_should_pass() {
            let mut testkit = TestKit::new();
            let domain_name = testkit.domain_name.clone();

            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            let token_pair_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");

            // TODO: rewrite into iroha query calls
            let token_pair = query_token_pair(token_pair_id.clone(), &mut testkit.world_state_view)
                .expect("failed to query token pair")
                .clone();
            assert_eq!(&token_pair_id, &token_pair.id);

            if let QueryResult::GetTokenPairList(token_pair_list_result) =
                GetTokenPairList::build_request(domain_name.clone())
                    .query
                    .execute(&mut testkit.world_state_view)
                    .expect("failed to query token pair list")
            {
                assert_eq!(
                    &token_pair_list_result.token_pair_list,
                    &[token_pair.clone()]
                )
            } else {
                panic!("wrong enum variant returned for GetTokenPairList");
            }

            if let QueryResult::GetTokenPairCount(token_pair_count_result) =
                GetTokenPairCount::build_request(DEXId::new(&domain_name))
                    .query
                    .execute(&mut testkit.world_state_view)
                    .expect("failed to query token pair count")
            {
                assert_eq!(token_pair_count_result.count, 1);
            } else {
                panic!("wrong token pair count");
            }

            remove_token_pair(token_pair_id.clone())
                .execute(
                    testkit.dex_owner_account_id.clone(),
                    &mut testkit.world_state_view,
                )
                .expect("remove token pair failed");

            if let QueryResult::GetTokenPairList(token_pair_list_result) =
                GetTokenPairList::build_request(domain_name.clone())
                    .query
                    .execute(&mut testkit.world_state_view)
                    .expect("failed to query token pair list")
            {
                assert!(&token_pair_list_result.token_pair_list.is_empty());
            } else {
                panic!("wrong enum variant returned for GetTokenPairList");
            }

            if let QueryResult::GetTokenPairCount(token_pair_count_result) =
                GetTokenPairCount::build_request(DEXId::new(&domain_name))
                    .query
                    .execute(&mut testkit.world_state_view)
                    .expect("failed to query token pair count")
            {
                assert_eq!(token_pair_count_result.count, 0);
            } else {
                panic!("wrong token pair count");
            }
        }

        #[test]
        fn test_xyk_pool_create_should_pass() {
            let mut testkit = TestKit::new();

            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            let token_pair_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");
            let (xyk_pool_id, storage_account_id, pool_token_asset_definition_id) =
                testkit.xyk_pool_create(token_pair_id.clone());

            let xyk_pool = get_liquidity_source(&xyk_pool_id, &testkit.world_state_view).unwrap();
            let xyk_pool_data = expect_xyk_pool_data(&xyk_pool).unwrap();

            assert_eq!(&storage_account_id, &xyk_pool_data.storage_account_id);
            assert_eq!(
                &pool_token_asset_definition_id,
                &xyk_pool_data.pool_token_asset_definition_id
            );
            assert_eq!(0u32, xyk_pool_data.base_asset_reserve);
            assert_eq!(0u32, xyk_pool_data.target_asset_reserve);
            assert_eq!(0u32, xyk_pool_data.k_last);
            assert_eq!(0u32, xyk_pool_data.pool_token_total_supply);
            assert_eq!(None, xyk_pool_data.fee_to)
        }

        #[test]
        fn test_xyk_pool_add_liquidity_should_pass() {
            let mut testkit = TestKit::new();

            // prepare environment
            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            let token_pair_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");
            let (xyk_pool_id, _, pool_token_id) = testkit.xyk_pool_create(token_pair_id.clone());
            let account_id = testkit.create_account("Trader@Soramitsu");
            testkit.add_transfer_permission(
                "Trader@Soramitsu",
                &token_pair_id.base_asset_id.to_string(),
            );
            testkit.add_transfer_permission(
                "Trader@Soramitsu",
                &token_pair_id.target_asset_id.to_string(),
            );
            testkit.mint_asset("XOR#Soramitsu", "Trader@Soramitsu", 5000u32);
            testkit.mint_asset("DOT#Polkadot", "Trader@Soramitsu", 7000u32);

            // add minted tokens to the pool from account
            xyk_pool_add_liquidity(xyk_pool_id.clone(), 5000, 7000, 4000, 6000)
                .execute(account_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");

            testkit.check_xyk_pool_state(5916, 5000, 7000, 0, &xyk_pool_id);
            testkit.check_xyk_pool_storage_account(5000, 7000, &xyk_pool_id);
            testkit.check_asset_amount("Trader@Soramitsu", "XOR#Soramitsu", 0);
            testkit.check_asset_amount("Trader@Soramitsu", "DOT#Polkadot", 0);
            testkit.check_asset_amount("Trader@Soramitsu", &pool_token_id.to_string(), 4916);
        }

        #[test]
        fn test_xyk_pool_mint_pool_token_should_pass() {
            let mut testkit = TestKit::new();
            let world_state_view = &mut testkit.world_state_view;
            let domain_name = testkit.domain_name.clone();
            let domain = world_state_view
                .domain(&domain_name)
                .expect("domain not found")
                .clone();
            let account_public_key = KeyPair::generate()
                .expect("Failed to generate KeyPair.")
                .public_key;
            let account = Account::with_signatory("user", &domain_name, account_public_key);
            let register_account = domain.register_account(account.clone());
            register_account
                .execute(testkit.root_account_id.clone(), world_state_view)
                .expect("failed to register account");

            // register assets
            let base_asset_definition_id = AssetDefinitionId::new("XOR", &domain_name);
            let target_asset_definition_id = AssetDefinitionId::new("DOT", &domain_name);
            let dex_id = DEXId::new(&domain_name);
            let token_pair_id =
                TokenPairId::new(dex_id, base_asset_definition_id, target_asset_definition_id);
            let asset_name = xyk_pool_token_asset_name(&token_pair_id);
            let pool_token_asset_definition_id = AssetDefinitionId::new(&asset_name, &domain_name);
            domain
                .register_asset(AssetDefinition::new(pool_token_asset_definition_id.clone()))
                .execute(testkit.root_account_id.clone(), world_state_view)
                .expect("failed to register asset");
            let mut data =
                XYKPoolData::new(pool_token_asset_definition_id.clone(), account.id.clone());

            // set initial total supply to 100
            data.pool_token_total_supply = 100u32;
            xyk_pool_mint_pool_token(account.id.clone(), 100u32, world_state_view, &mut data)
                .expect("failed to mint pool token");
            // after minting 100 pool token, total supply should be 200
            assert_eq!(data.pool_token_total_supply.clone(), 200u32);

            if let QueryResult::GetAccount(account_result) =
                GetAccount::build_request(account.id.clone())
                    .query
                    .execute(world_state_view)
                    .expect("failed to query token pair")
            {
                let account = account_result.account;
                let pool_token_asset_id =
                    AssetId::new(pool_token_asset_definition_id.clone(), account.id.clone());
                let pool_token_asset = account
                    .assets
                    .get(&pool_token_asset_id)
                    .expect("failed to get pool token asset");
                // account should contain 100 pool token
                assert_eq!(pool_token_asset.quantity, 100);
            } else {
                panic!("wrong enum variant returned for GetAccount");
            }
        }

        #[test]
        fn test_xyk_pool_optimal_liquidity_should_pass() {
            // zero reserves return desired amounts
            let (amount_a, amount_b) =
                xyk_pool_get_optimal_deposit_amounts(0, 0, 10000, 5000, 10000, 5000)
                    .expect("failed to get optimal asset amounts");
            assert_eq!(amount_a, 10000);
            assert_eq!(amount_b, 5000);
            // add liquidity with same proportions
            let (amount_a, amount_b) =
                xyk_pool_get_optimal_deposit_amounts(10000, 5000, 10000, 5000, 10000, 5000)
                    .expect("failed to get optimal asset amounts");
            assert_eq!(amount_a, 10000);
            assert_eq!(amount_b, 5000);
            // add liquidity with different proportions
            let (amount_a, amount_b) =
                xyk_pool_get_optimal_deposit_amounts(10000, 5000, 5000, 10000, 0, 0)
                    .expect("failed to get optimal asset amounts");
            assert_eq!(amount_a, 5000);
            assert_eq!(amount_b, 2500);
            // add liquidity `b_optimal>b_desired` branch
            let (amount_a, amount_b) =
                xyk_pool_get_optimal_deposit_amounts(10000, 5000, 5000, 2000, 0, 0)
                    .expect("failed to get optimal asset amounts");
            assert_eq!(amount_a, 4000);
            assert_eq!(amount_b, 2000);
        }

        #[test]
        fn test_xyk_pool_quote_should_pass() {
            let amount_b_optimal =
                xyk_pool_quote(2000, 5000, 10000).expect("failed to calculate proportion");
            assert_eq!(amount_b_optimal, 4000);
            let amount_b_optimal =
                xyk_pool_quote(1, 5000, 10000).expect("failed to calculate proportion");
            assert_eq!(amount_b_optimal, 2);
            let result = xyk_pool_quote(0, 5000, 10000).unwrap_err();
            assert_eq!(result, "insufficient amount");
            let result = xyk_pool_quote(1000, 5000, 0).unwrap_err();
            assert_eq!(result, "insufficient liquidity");
            let result = xyk_pool_quote(1000, 0, 10000).unwrap_err();
            assert_eq!(result, "insufficient liquidity");
        }

        // TODO: tests with multiple consecutive AddLiquidity
        // TODO: tests for AddLiquidity with feeOn

        #[test]
        fn test_xyk_pool_remove_liquidity_should_pass() {
            let mut testkit = TestKit::new();

            // prepare environment
            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            let token_pair_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");
            let (xyk_pool_id, _, pool_token_id) = testkit.xyk_pool_create(token_pair_id.clone());
            let account_id = testkit.create_account("Trader@Soramitsu");
            testkit.add_transfer_permission("Trader@Soramitsu", &pool_token_id.to_string());
            testkit.add_transfer_permission("Trader@Soramitsu", "XOR#Soramitsu");
            testkit.add_transfer_permission("Trader@Soramitsu", "DOT#Polkadot");
            testkit.mint_asset("XOR#Soramitsu", "Trader@Soramitsu", 5000u32);
            testkit.mint_asset("DOT#Polkadot", "Trader@Soramitsu", 7000u32);

            // add minted tokens to the pool from account
            xyk_pool_add_liquidity(xyk_pool_id.clone(), 5000, 7000, 4000, 6000)
                .execute(account_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");

            // burn minted pool token to receive pool tokens back
            xyk_pool_remove_liquidity(xyk_pool_id.clone(), 4916, 0, 0)
                .execute(account_id.clone(), &mut testkit.world_state_view)
                .expect("remove liquidity failed");

            testkit.check_xyk_pool_state(1000, 846, 1184, 0, &xyk_pool_id);
            testkit.check_xyk_pool_storage_account(846, 1184, &xyk_pool_id);
            testkit.check_asset_amount("Trader@Soramitsu", "XOR#Soramitsu", 4154);
            testkit.check_asset_amount("Trader@Soramitsu", "DOT#Polkadot", 5816);
            testkit.check_asset_amount("Trader@Soramitsu", &pool_token_id.to_string(), 0);
        }

        #[test]
        fn test_xyk_pool_swap_assets_in_should_pass() {
            let mut testkit = TestKit::new();

            // prepare environment
            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            let token_pair_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");
            let (xyk_pool_id, _, pool_token_id) = testkit.xyk_pool_create(token_pair_id.clone());
            let account_id = testkit.create_account("Trader@Soramitsu");
            testkit.add_transfer_permission("Trader@Soramitsu", &pool_token_id.to_string());
            testkit.add_transfer_permission("Trader@Soramitsu", "XOR#Soramitsu");
            testkit.add_transfer_permission("Trader@Soramitsu", "DOT#Polkadot");
            testkit.mint_asset("XOR#Soramitsu", "Trader@Soramitsu", 7000u32);
            testkit.mint_asset("DOT#Polkadot", "Trader@Soramitsu", 7000u32);

            // add minted tokens to the pool from account
            xyk_pool_add_liquidity(xyk_pool_id.clone(), 5000, 7000, 4000, 6000)
                .execute(account_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");

            xyk_pool_swap_exact_tokens_for_tokens(
                DEXId::new(&testkit.domain_name),
                vec![
                    token_pair_id.base_asset_id.clone(),
                    token_pair_id.target_asset_id.clone(),
                ],
                2000,
                0,
            )
            .execute(account_id.clone(), &mut testkit.world_state_view)
            .expect("swap exact tokens for tokens failed");

            testkit.check_xyk_pool_state(5916, 7000, 5005, 0, &xyk_pool_id);
            testkit.check_xyk_pool_storage_account(7000, 5005, &xyk_pool_id);
            testkit.check_asset_amount("Trader@Soramitsu", "XOR#Soramitsu", 0);
            testkit.check_asset_amount("Trader@Soramitsu", "DOT#Polkadot", 1995);
            testkit.check_asset_amount("Trader@Soramitsu", &pool_token_id.to_string(), 4916);
        }

        #[test]
        fn test_xyk_pool_get_amount_out_should_pass() {
            // regular input
            let amount_out = xyk_pool_get_amount_out(2000, 5000, 5000).unwrap();
            assert_eq!(amount_out, 1425);
            // zero inputs
            let amount_out = xyk_pool_get_amount_out(0, 5000, 7000).unwrap_err();
            assert_eq!(amount_out, "insufficient input amount");
            let amount_out = xyk_pool_get_amount_out(2000, 0, 7000).unwrap_err();
            assert_eq!(amount_out, "insufficient liquidity");
            let amount_out = xyk_pool_get_amount_out(2000, 5000, 0).unwrap_err();
            assert_eq!(amount_out, "insufficient liquidity");
            // max values
            let amount_out = xyk_pool_get_amount_out(500000, std::u32::MAX, std::u32::MAX).unwrap();
            assert_eq!(amount_out, 498442);
            let amount_out =
                xyk_pool_get_amount_out(250000, std::u32::MAX / 2, std::u32::MAX / 2).unwrap();
            assert_eq!(amount_out, 249221);
            let amount_out =
                xyk_pool_get_amount_out(std::u32::MAX, std::u32::MAX, std::u32::MAX).unwrap();
            assert_eq!(amount_out, 2144257582);
        }

        #[test]
        fn test_xyk_pool_swap_assets_out_should_pass() {
            let mut testkit = TestKit::new();

            // prepare environment
            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            let token_pair_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");
            let (xyk_pool_id, _, pool_token_id) = testkit.xyk_pool_create(token_pair_id.clone());
            let account_id = testkit.create_account("Trader@Soramitsu");
            testkit.add_transfer_permission("Trader@Soramitsu", &pool_token_id.to_string());
            testkit.add_transfer_permission("Trader@Soramitsu", "XOR#Soramitsu");
            testkit.add_transfer_permission("Trader@Soramitsu", "DOT#Polkadot");
            testkit.mint_asset("XOR#Soramitsu", "Trader@Soramitsu", 7000u32);
            testkit.mint_asset("DOT#Polkadot", "Trader@Soramitsu", 7000u32);

            // add minted tokens to the pool from account
            xyk_pool_add_liquidity(xyk_pool_id.clone(), 5000, 7000, 4000, 6000)
                .execute(account_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");

            xyk_pool_swap_tokens_for_exact_tokens(
                DEXId::new(&testkit.domain_name),
                vec![
                    token_pair_id.base_asset_id.clone(),
                    token_pair_id.target_asset_id.clone(),
                ],
                1995,
                std::u32::MAX,
            )
            .execute(account_id.clone(), &mut testkit.world_state_view)
            .expect("swap exact tokens for tokens failed");

            testkit.check_xyk_pool_state(5916, 7000, 5005, 0, &xyk_pool_id);
            testkit.check_xyk_pool_storage_account(7000, 5005, &xyk_pool_id);
            testkit.check_asset_amount("Trader@Soramitsu", "XOR#Soramitsu", 0);
            testkit.check_asset_amount("Trader@Soramitsu", "DOT#Polkadot", 1995);
            testkit.check_asset_amount("Trader@Soramitsu", &pool_token_id.to_string(), 4916);
        }

        #[test]
        fn test_xyk_pool_get_amount_in_should_pass() {
            // regular input
            let amount_out = xyk_pool_get_amount_in(2000, 5000, 5000).unwrap();
            assert_eq!(amount_out, 3344);
            // zero inputs
            let amount_out = xyk_pool_get_amount_in(0, 5000, 7000).unwrap_err();
            assert_eq!(amount_out, "insufficient output amount");
            let amount_out = xyk_pool_get_amount_in(2000, 0, 7000).unwrap_err();
            assert_eq!(amount_out, "insufficient liquidity");
            let amount_out = xyk_pool_get_amount_in(2000, 5000, 0).unwrap_err();
            assert_eq!(amount_out, "insufficient liquidity");
            // max values
            let amount_out = xyk_pool_get_amount_in(500000, std::u32::MAX, std::u32::MAX).unwrap();
            assert_eq!(amount_out, 501563);
            let amount_out =
                xyk_pool_get_amount_in(250000, std::u32::MAX / 2, std::u32::MAX / 2).unwrap();
            assert_eq!(amount_out, 250782);
            let amount_out =
                xyk_pool_get_amount_in(std::u32::MAX, std::u32::MAX, std::u32::MAX).unwrap_err();
            assert_eq!(amount_out, "can't withdraw full reserve");
        }

        #[test]
        fn test_xyk_pool_two_liquidity_providers_one_trader_should_pass() {
            let mut testkit = TestKit::new();
            testkit.initialize_dex();
            testkit.register_asset("XOR#Soramitsu");
            testkit.register_domain("Polkadot");
            testkit.register_asset("DOT#Polkadot");
            testkit.register_domain("Kusama");
            testkit.register_asset("KSM#Kusama");
            let token_pair_a_id = testkit.create_token_pair("XOR#Soramitsu", "DOT#Polkadot");
            let token_pair_b_id = testkit.create_token_pair("XOR#Soramitsu", "KSM#Kusama");
            let (xyk_pool_a_id, _, pool_token_a_id) =
                testkit.xyk_pool_create(token_pair_a_id.clone());
            let (xyk_pool_b_id, _, pool_token_b_id) =
                testkit.xyk_pool_create(token_pair_b_id.clone());
            let account_a_id = testkit.create_account("User A@Soramitsu");
            testkit.add_transfer_permission("User A@Soramitsu", &pool_token_a_id.to_string());
            testkit.add_transfer_permission("User A@Soramitsu", &pool_token_b_id.to_string());
            testkit.add_transfer_permission("User A@Soramitsu", "XOR#Soramitsu");
            testkit.add_transfer_permission("User A@Soramitsu", "DOT#Polkadot");
            testkit.add_transfer_permission("User A@Soramitsu", "KSM#Kusama");
            testkit.mint_asset("XOR#Soramitsu", "User A@Soramitsu", 12000u32);
            testkit.mint_asset("DOT#Polkadot", "User A@Soramitsu", 4000u32);
            testkit.mint_asset("KSM#Kusama", "User A@Soramitsu", 3000u32);
            let account_b_id = testkit.create_account("User B@Soramitsu");
            testkit.add_transfer_permission("User B@Soramitsu", &pool_token_a_id.to_string());
            testkit.add_transfer_permission("User B@Soramitsu", "XOR#Soramitsu");
            testkit.add_transfer_permission("User B@Soramitsu", "DOT#Polkadot");
            testkit.mint_asset("XOR#Soramitsu", "User B@Soramitsu", 500u32);
            testkit.mint_asset("DOT#Polkadot", "User B@Soramitsu", 500u32);
            let account_c_id = testkit.create_account("User C@Soramitsu");
            testkit.add_transfer_permission("User C@Soramitsu", "KSM#Kusama");
            testkit.mint_asset("KSM#Kusama", "User C@Soramitsu", 2000u32);

            testkit.check_xyk_pool_state(0, 0, 0, 0, &xyk_pool_a_id);
            testkit.check_xyk_pool_state(0, 0, 0, 0, &xyk_pool_b_id);
            testkit.check_asset_amount("User A@Soramitsu", "XOR#Soramitsu", 12000);
            testkit.check_asset_amount("User A@Soramitsu", "DOT#Polkadot", 4000);
            testkit.check_asset_amount("User A@Soramitsu", "KSM#Kusama", 3000);
            testkit.check_asset_amount("User B@Soramitsu", "XOR#Soramitsu", 500);
            testkit.check_asset_amount("User B@Soramitsu", "DOT#Polkadot", 500);
            testkit.check_asset_amount("User C@Soramitsu", "KSM#Kusama", 2000);

            xyk_pool_add_liquidity(xyk_pool_a_id.clone(), 6000, 4000, 0, 0)
                .execute(account_a_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");
            xyk_pool_add_liquidity(xyk_pool_b_id.clone(), 6000, 3000, 0, 0)
                .execute(account_a_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");
            xyk_pool_add_liquidity(xyk_pool_a_id.clone(), 500, 500, 0, 0)
                .execute(account_b_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");

            xyk_pool_swap_exact_tokens_for_tokens(
                DEXId::new("Soramitsu"),
                vec![
                    AssetDefinitionId::from("KSM#Kusama"),
                    AssetDefinitionId::from("XOR#Soramitsu"),
                    AssetDefinitionId::from("DOT#Polkadot"),
                ],
                2000,
                0u32,
            )
            .execute(account_c_id.clone(), &mut testkit.world_state_view)
            .expect("swap exact tokens for tokens failed");

            xyk_pool_remove_liquidity(xyk_pool_a_id.clone(), 407, 0, 0)
                .execute(account_b_id.clone(), &mut testkit.world_state_view)
                .expect("add liquidity failed");

            testkit.check_xyk_pool_state(4898, 8213, 2926, 0, &xyk_pool_a_id);
            testkit.check_xyk_pool_state(4242, 3605, 5000, 0, &xyk_pool_b_id);
            testkit.check_xyk_pool_storage_account(8213, 2926, &xyk_pool_a_id);
            testkit.check_xyk_pool_storage_account(3605, 5000, &xyk_pool_b_id);
            testkit.check_asset_amount("User A@Soramitsu", "XOR#Soramitsu", 0);
            testkit.check_asset_amount("User A@Soramitsu", "DOT#Polkadot", 0);
            testkit.check_asset_amount("User A@Soramitsu", "KSM#Kusama", 0);
            testkit.check_asset_amount("User A@Soramitsu", &pool_token_a_id.to_string(), 3898);
            testkit.check_asset_amount("User A@Soramitsu", &pool_token_b_id.to_string(), 3242);
            testkit.check_asset_amount("User B@Soramitsu", "XOR#Soramitsu", 682);
            testkit.check_asset_amount("User B@Soramitsu", "DOT#Polkadot", 410);
            testkit.check_asset_amount("User B@Soramitsu", &pool_token_a_id.to_string(), 0);
            testkit.check_asset_amount("User C@Soramitsu", "KSM#Kusama", 0);
            testkit.check_asset_amount("User C@Soramitsu", "DOT#Polkadot", 1164);
        }
    }
}

/// Query module provides functions for performing dex-related queries.
pub mod query {
    use super::isi::*;
    use super::*;
    use crate::query::*;
    use iroha_derive::*;
    use std::time::SystemTime;

    /// Helper function to get reference to `Domain` by its name.
    pub fn get_domain<'a>(
        domain_name: &str,
        world_state_view: &'a WorldStateView,
    ) -> Result<&'a Domain, String> {
        Ok(world_state_view
            .read_domain(domain_name)
            .ok_or("domain not found")?)
    }

    /// Helper function to get mutable reference to `Domain` by its name.
    pub fn get_domain_mut<'a>(
        domain_name: &str,
        world_state_view: &'a mut WorldStateView,
    ) -> Result<&'a mut Domain, String> {
        Ok(world_state_view
            .domain(domain_name)
            .ok_or("domain not found")?)
    }

    /// Helper function to get reference to DEX by name
    /// of containing domain.
    pub fn get_dex<'a>(
        domain_name: &str,
        world_state_view: &'a WorldStateView,
    ) -> Result<&'a DEX, String> {
        Ok(get_domain(domain_name, world_state_view)?
            .dex
            .as_ref()
            .ok_or("dex not initialized for domain")?)
    }

    /// Helper function to get mutable reference to DEX by name
    /// of containing domain.
    pub fn get_dex_mut<'a>(
        domain_name: &str,
        world_state_view: &'a mut WorldStateView,
    ) -> Result<&'a mut DEX, String> {
        Ok(get_domain_mut(domain_name, world_state_view)?
            .dex
            .as_mut()
            .ok_or("dex not initialized for domain")?)
    }

    /// Helper function to get reference to `TokenPair` by its identifier.
    pub fn get_token_pair<'a>(
        token_pair_id: &<TokenPair as Identifiable>::Id,
        world_state_view: &'a WorldStateView,
    ) -> Result<&'a TokenPair, String> {
        Ok(
            get_dex(&token_pair_id.dex_id.domain_name, world_state_view)?
                .token_pairs
                .get(token_pair_id)
                .ok_or("token pair not found")?,
        )
    }
    /// Helper function to get mutable reference to `TokenPair` by its identifier.
    pub fn get_token_pair_mut<'a>(
        token_pair_id: &<TokenPair as Identifiable>::Id,
        world_state_view: &'a mut WorldStateView,
    ) -> Result<&'a mut TokenPair, String> {
        Ok(
            get_dex_mut(&token_pair_id.dex_id.domain_name, world_state_view)?
                .token_pairs
                .get_mut(token_pair_id)
                .ok_or("token pair not found")?,
        )
    }

    /// Helper function to get reference to `LiquiditySource` by its identifier.
    pub fn get_liquidity_source<'a>(
        liquidity_source_id: &<LiquiditySource as Identifiable>::Id,
        world_state_view: &'a WorldStateView,
    ) -> Result<&'a LiquiditySource, String> {
        Ok(
            get_token_pair(&liquidity_source_id.token_pair_id, world_state_view)?
                .liquidity_sources
                .get(liquidity_source_id)
                .ok_or("liquidity source not found")?,
        )
    }

    /// Helper function to get mutable reference to `LiquiditySource` by its identifier.
    pub fn get_liquidity_source_mut<'a>(
        liquidity_source_id: &<LiquiditySource as Identifiable>::Id,
        world_state_view: &'a mut WorldStateView,
    ) -> Result<&'a mut LiquiditySource, String> {
        Ok(
            get_token_pair_mut(&liquidity_source_id.token_pair_id, world_state_view)?
                .liquidity_sources
                .get_mut(liquidity_source_id)
                .ok_or("liquidity source not found")?,
        )
    }

    /// Get balance of asset for specified account.
    pub fn get_asset_quantity(
        account_id: <Account as Identifiable>::Id,
        asset_definition_id: <AssetDefinition as Identifiable>::Id,
        world_state_view: &WorldStateView,
    ) -> Result<u32, String> {
        let asset_id = AssetId::new(asset_definition_id, account_id.clone());
        Ok(world_state_view
            .read_account(&account_id)
            .ok_or("account not found")?
            .assets
            .get(&asset_id)
            .ok_or("asset not found")?
            .quantity)
    }

    /// Helper function to construct default `QueryRequest` with empty signature.
    fn unsigned_query_request(query: IrohaQuery) -> QueryRequest {
        QueryRequest {
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("Failed to get System Time.")
                .as_millis()
                .to_string(),
            signature: Option::None,
            query,
        }
    }

    /// Get DEX information.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetDEX {
        /// Domain name to which DEX belongs.
        pub domain_name: <Domain as Identifiable>::Id,
    }
    /// Result of `GetDEX` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetDEXResult {
        /// `DEX` entity.
        pub dex: DEX,
    }

    impl GetDEX {
        /// Build a `GetDEX` query in the form of a `QueryRequest`.
        pub fn build_request(domain_name: <Domain as Identifiable>::Id) -> QueryRequest {
            let query = GetDEX { domain_name };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetDEX {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let dex = get_dex(&self.domain_name, world_state_view)?;
            Ok(QueryResult::GetDEX(GetDEXResult { dex: dex.clone() }))
        }
    }

    /// Get list of active DEX in the network.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetDEXList;

    /// Result of the `GetDEXList` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetDEXListResult {
        /// List of DEX.
        pub dex_list: Vec<DEX>,
    }

    impl GetDEXList {
        /// Build a `GetDEXList` query in the form of a `QueryRequest`.
        pub fn build_request() -> QueryRequest {
            let query = GetDEXList;
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetDEXList {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let dex_list = query_dex_list(world_state_view).cloned().collect();
            Ok(QueryResult::GetDEXList(GetDEXListResult { dex_list }))
        }
    }

    /// A query to get a list of all active DEX in network.
    fn query_dex_list<'a>(world_state_view: &'a WorldStateView) -> impl Iterator<Item = &DEX> + 'a {
        world_state_view
            .peer
            .domains
            .iter()
            .filter_map(|(_, domain)| domain.dex.as_ref())
    }

    /// Get DEX information.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetTokenPair {
        /// Identifier of TokenPair.
        pub token_pair_id: <TokenPair as Identifiable>::Id,
    }
    /// Result of `GetDEX` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetTokenPairResult {
        /// `TokenPair` information.
        pub token_pair: TokenPair,
    }

    impl GetTokenPair {
        /// Build a `GetTokenPair` query in the form of a `QueryRequest`.
        pub fn build_request(token_pair_id: <TokenPair as Identifiable>::Id) -> QueryRequest {
            let query = GetTokenPair { token_pair_id };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetTokenPair {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let token_pair = get_token_pair(&self.token_pair_id, world_state_view)?;
            Ok(QueryResult::GetTokenPair(GetTokenPairResult {
                token_pair: token_pair.clone(),
            }))
        }
    }

    /// A query to get a particular `TokenPair` identified by its id.
    pub fn query_token_pair(
        token_pair_id: <TokenPair as Identifiable>::Id,
        world_state_view: &WorldStateView,
    ) -> Option<&TokenPair> {
        get_token_pair(&token_pair_id, world_state_view).ok()
    }

    /// Get list of active Token Pairs for Domain by its name.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetTokenPairList {
        domain_name: <Domain as Identifiable>::Id,
    }

    /// Result of the `GetTokenPairList` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetTokenPairListResult {
        /// List of DEX.
        pub token_pair_list: Vec<TokenPair>,
    }

    impl GetTokenPairList {
        /// Build a `GetTokenPairList` query in the form of a `QueryRequest`.
        pub fn build_request(domain_name: <Domain as Identifiable>::Id) -> QueryRequest {
            let query = GetTokenPairList { domain_name };
            unsigned_query_request(query.into())
        }
    }

    /// Add indirect Token Pairs through base asset
    /// Example: for actual pairs -
    /// BASE:TARGET_A, BASE:TARGET_B, BASE:TARGET_C
    /// query will return -
    /// BASE:TARGET_A, BASE:TARGET_B, BASE:TARGET_C,
    /// TARGET_A:TARGET_B, TARGET_A:TARGET_C, TARGET_B:TARGET_C
    impl Query for GetTokenPairList {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let mut token_pair_list = query_token_pair_list(&self.domain_name, world_state_view)
                .ok_or(format!(
                    "No domain with name: {:?} found in the current world state: {:?}",
                    &self.domain_name, world_state_view
                ))?
                .cloned()
                .collect::<Vec<_>>();

            let target_assets = token_pair_list
                .iter()
                .map(|token_pair| token_pair.id.target_asset_id.clone())
                .collect::<Vec<_>>();
            for token_pair in
                get_permuted_pairs(&target_assets)
                    .iter()
                    .map(|(base_asset, target_asset)| {
                        TokenPair::new(
                            DEXId::new(&base_asset.domain_name),
                            base_asset.clone(),
                            target_asset.clone(),
                        )
                    })
            {
                token_pair_list.push(token_pair);
            }
            Ok(QueryResult::GetTokenPairList(GetTokenPairListResult {
                token_pair_list,
            }))
        }
    }

    /// This function returns all combinations of two elements from given sequence.
    /// Combinations are unique without ordering in pairs, i.e. (A,B) and (B,A) considered the same.
    fn get_permuted_pairs<T: Clone>(sequence: &[T]) -> Vec<(T, T)> {
        let mut result = Vec::new();
        for i in 0..sequence.len() {
            for j in i + 1..sequence.len() {
                result.push((
                    sequence.get(i).unwrap().clone(),
                    sequence.get(j).unwrap().clone(),
                ));
            }
        }
        result
    }

    /// A query to get a list of all active `TokenPair`s of a DEX identified by its domain name.
    fn query_token_pair_list<'a>(
        domain_name: &str,
        world_state_view: &'a WorldStateView,
    ) -> Option<impl Iterator<Item = &'a TokenPair>> {
        let dex = world_state_view.read_domain(domain_name)?.dex.as_ref()?;
        Some(dex.token_pairs.iter().map(|(_, value)| value))
    }

    /// Get count of active Token Pairs in DEX.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetTokenPairCount {
        /// Identifier of DEX.
        pub dex_id: <DEX as Identifiable>::Id,
    }
    /// Result of `GetTokenPairCount` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetTokenPairCountResult {
        /// Count of active Token Pairs in DEX.
        pub count: u64,
    }

    impl GetTokenPairCount {
        /// Build a `GetTokenPairList` query in the form of a `QueryRequest`.
        pub fn build_request(dex_id: <DEX as Identifiable>::Id) -> QueryRequest {
            let query = GetTokenPairCount { dex_id };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetTokenPairCount {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let dex = get_dex(&self.dex_id.domain_name, world_state_view)?;
            Ok(QueryResult::GetTokenPairCount(GetTokenPairCountResult {
                count: dex.token_pairs.len() as u64,
            }))
        }
    }

    /// Get information about XYK Pool.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetXYKPoolInfo {
        /// Identifier of Liquidity Source.
        pub liquidity_source_id: <LiquiditySource as Identifiable>::Id,
    }

    /// Result of `GetXYKPoolInfo` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetXYKPoolInfoResult {
        /// Information about XYK Pool.
        pub pool_data: XYKPoolData, // TODO: change to custom fields
    }

    impl GetXYKPoolInfo {
        /// Build a `GetXYKPoolInfo` query in the form of a `QueryRequest`.
        pub fn build_request(
            liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        ) -> QueryRequest {
            let query = GetXYKPoolInfo {
                liquidity_source_id,
            };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetXYKPoolInfo {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let liquidity_source =
                get_liquidity_source(&self.liquidity_source_id, world_state_view)?;
            let pool_data = expect_xyk_pool_data(liquidity_source)?;
            Ok(QueryResult::GetXYKPoolInfo(GetXYKPoolInfoResult {
                pool_data: pool_data.clone(),
            }))
        }
    }

    /// Get fee fraction set to be deduced from swaps.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetFeeOnXYKPool {
        /// Identifier of XYK Pool.
        pub liquidity_source_id: <LiquiditySource as Identifiable>::Id,
    }

    /// Result of `GetFeeOnXYKPool` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetFeeOnXYKPoolResult {
        /// Fee fraction expressed in basis points.
        pub fee: u16,
    }

    impl GetFeeOnXYKPool {
        /// Build a `GetFeeOnXYKPool` query in the form of a `QueryRequest`.
        pub fn build_request(
            liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        ) -> QueryRequest {
            let query = GetFeeOnXYKPool {
                liquidity_source_id,
            };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetFeeOnXYKPool {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let liquidity_source =
                get_liquidity_source(&self.liquidity_source_id, world_state_view)?;
            let pool_data = expect_xyk_pool_data(liquidity_source)?;
            Ok(QueryResult::GetFeeOnXYKPool(GetFeeOnXYKPoolResult {
                fee: pool_data.fee,
            }))
        }
    }

    /// Get protocol fee part set to be deducted from regular fee.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetProtocolFeePartOnXYKPool {
        /// Identifier of XYK Pool.
        pub liquidity_source_id: <LiquiditySource as Identifiable>::Id,
    }

    /// Result of `GetProtocolFeePartOnXYKPool` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetProtocolFeePartOnXYKPoolResult {
        /// Protocol fee part expressed as fraction of regular fee in basis points.
        pub protocol_fee_part: u16,
    }

    impl GetProtocolFeePartOnXYKPool {
        /// Build a `GetProtocolFeePartOnXYKPool` query in the form of a `QueryRequest`.
        pub fn build_request(
            liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        ) -> QueryRequest {
            let query = GetProtocolFeePartOnXYKPool {
                liquidity_source_id,
            };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetProtocolFeePartOnXYKPool {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let liquidity_source =
                get_liquidity_source(&self.liquidity_source_id, world_state_view)?;
            let pool_data = expect_xyk_pool_data(liquidity_source)?;
            Ok(QueryResult::GetProtocolFeePartOnXYKPool(
                GetProtocolFeePartOnXYKPoolResult {
                    protocol_fee_part: pool_data.protocol_fee_part,
                },
            ))
        }
    }

    /// Get quantity of first asset in path needed to get 1 unit of last asset in path.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetSpotPriceOnXYKPool {
        /// Identifier of DEX.
        pub dex_id: <DEX as Identifiable>::Id,
        /// Path of exchange.
        pub path: Vec<<AssetDefinition as Identifiable>::Id>,
    }

    /// Result of `GetSpotPriceOnXYKPool` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetSpotPriceOnXYKPoolResult {
        /// Price of the asset without fee.
        pub price: u32,
        /// Price of the asset with fee.
        pub price_with_fee: u32,
    }

    impl GetSpotPriceOnXYKPool {
        /// Build a `GetSpotPriceOnXYKPool` query in the form of a `QueryRequest`.
        pub fn build_request(
            dex_id: <DEX as Identifiable>::Id,
            path: Vec<<AssetDefinition as Identifiable>::Id>,
        ) -> QueryRequest {
            let query = GetSpotPriceOnXYKPool { dex_id, path };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetSpotPriceOnXYKPool {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            if self.path.len() < 2 {
                return Err("exchange path should contain at least one pair".to_owned());
            }
            let amounts =
                xyk_pool_get_amounts_in(self.dex_id.clone(), 1, &self.path, world_state_view)?;
            Ok(QueryResult::GetSpotPriceOnXYKPool(
                GetSpotPriceOnXYKPoolResult {
                    price: amounts.last().unwrap() / amounts.first().unwrap(), // TODO: only works with fractional numbers
                    price_with_fee: 0, // TODO: implement fee calculation
                },
            ))
        }
    }

    /// Get quantities of base and target assets that correspond to owned pool tokens.
    #[derive(Clone, Debug, Io, IntoQuery, Encode, Decode)]
    pub struct GetOwnedLiquidityOnXYKPoolInfo {
        /// Identifier of XYK Pool.
        pub liquidity_source_id: <LiquiditySource as Identifiable>::Id,
        /// Account holding pool tokens.
        pub account_id: <Account as Identifiable>::Id,
    }

    /// Result of `GetOwnedLiquidityOnXYKPool` execution.
    #[derive(Clone, Debug, Encode, Decode)]
    pub struct GetOwnedLiquidityOnXYKPoolInfoResult {
        /// Quantity of base asset.
        pub base_asset_quantity: u32,
        /// Quantity of target asset.
        pub target_asset_quantity: u32,
        /// Quantity of pool token.
        pub pool_token_quantity: u32,
    }

    impl GetOwnedLiquidityOnXYKPoolInfo {
        /// Build a `GetOwnedLiquidityOnXYKPool` query in the form of a `QueryRequest`.
        pub fn build_request(
            liquidity_source_id: <LiquiditySource as Identifiable>::Id,
            account_id: <Account as Identifiable>::Id,
        ) -> QueryRequest {
            let query = GetOwnedLiquidityOnXYKPoolInfo {
                liquidity_source_id,
                account_id,
            };
            unsigned_query_request(query.into())
        }
    }

    impl Query for GetOwnedLiquidityOnXYKPoolInfo {
        #[log]
        fn execute(&self, world_state_view: &WorldStateView) -> Result<QueryResult, String> {
            let liquidity_source =
                get_liquidity_source(&self.liquidity_source_id, world_state_view)?;
            let pool_data = expect_xyk_pool_data(liquidity_source)?;
            let base_asset_balance = get_asset_quantity(
                pool_data.storage_account_id.clone(),
                self.liquidity_source_id.token_pair_id.base_asset_id.clone(),
                world_state_view,
            )?;
            let target_asset_balance = get_asset_quantity(
                pool_data.storage_account_id.clone(),
                self.liquidity_source_id
                    .token_pair_id
                    .target_asset_id
                    .clone(),
                world_state_view,
            )?;
            let pool_token_quantity = get_asset_quantity(
                self.account_id.clone(),
                pool_data.pool_token_asset_definition_id.clone(),
                world_state_view,
            )?;
            let base_asset_quantity =
                pool_token_quantity * base_asset_balance / pool_data.pool_token_total_supply;
            let target_asset_quantity =
                pool_token_quantity * target_asset_balance / pool_data.pool_token_total_supply;
            Ok(QueryResult::GetOwnedLiquidityOnXYKPoolInfo(
                GetOwnedLiquidityOnXYKPoolInfoResult {
                    base_asset_quantity,
                    target_asset_quantity,
                    pool_token_quantity,
                },
            ))
        }
    }
}
