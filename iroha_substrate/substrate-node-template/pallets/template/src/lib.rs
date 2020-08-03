//! A demonstration of an offchain worker that sends onchain callbacks

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

#[cfg(test)]
pub mod mock;

use core::{convert::TryInto, fmt};
use frame_support::{
    debug, decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult, traits::Get,
};
use parity_scale_codec::{Decode, Encode};

use alt_serde::{Deserialize, Deserializer};
use frame_support::dispatch::Weight;
use frame_support::traits::Currency;
use frame_support::traits::ExistenceRequirement;
use frame_system::offchain::{Account, SignMessage, SigningTypes};
use frame_system::{
    self as system, ensure_none, ensure_signed,
    offchain::{
        AppCrypto, CreateSignedTransaction, SendSignedTransaction, Signer, SubmitTransaction,
    },
};
use iroha_client_no_std::account;
use iroha_client_no_std::account::isi::AccountInstruction;
use iroha_client_no_std::asset::isi::AssetInstruction;
use iroha_client_no_std::asset::query::GetAccountAssets;
use iroha_client_no_std::block::{BlockHeader, Message as BlockMessage, ValidBlock};
use iroha_client_no_std::crypto::{PublicKey, Signature, Signatures};
use iroha_client_no_std::isi::prelude::PeerInstruction;
use iroha_client_no_std::peer::PeerId;
use iroha_client_no_std::prelude::*;
use iroha_client_no_std::tx::{Payload, RequestedTransaction};
use sp_core::crypto::KeyTypeId;
use sp_core::ed25519::Signature as SpSignature;
use sp_runtime::offchain::http::Request;
use sp_runtime::traits::{Hash, StaticLookup};
use sp_runtime::{
    offchain as rt_offchain,
    offchain::storage::StorageValueRef,
    transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    },
    MultiSignature,
};
use sp_std::convert::TryFrom;
use sp_std::prelude::*;
use sp_std::str;
// use core::alloc::format;

/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"demo");
pub const KEY_TYPE_2: KeyTypeId = KeyTypeId(*b"dem0");
pub const NUM_VEC_LEN: usize = 10;

pub const INSTRUCTION_ENDPOINT: &[u8] = b"http://127.0.0.1:7878/instruction";
pub const BLOCK_ENDPOINT: &[u8] = b"http://127.0.0.1:7878/block";
pub const QUERY_ENDPOINT: &[u8] = b"http://127.0.0.1:7878/query";
pub const HTTP_HEADER_USER_AGENT: &[u8] = b"jimmychu0807";

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrappers.
/// We can use from supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// the types with this pallet-specific identifier.
pub mod crypto {
    use crate::KEY_TYPE;
    use sp_core::ecdsa::Signature as EcdsaSignature;
    use sp_core::ed25519::{Public as EdPublic, Signature as Ed25519Signature};
    use sp_core::sr25519::Signature as Sr25519Signature;

    use sp_runtime::{
        app_crypto::{app_crypto, ecdsa, ed25519, sr25519},
        traits::Verify,
        MultiSignature, MultiSigner,
    };

    app_crypto!(sr25519, KEY_TYPE);

    pub struct TestAuthId;

    // implemented for ocw-runtime
    impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
        type RuntimeAppPublic = Public;
        type GenericSignature = sp_core::sr25519::Signature;
        type GenericPublic = sp_core::sr25519::Public;
    }

    // implemented for mock runtime in test
    // impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature>
    // for TestAuthId
    // {
    //     type RuntimeAppPublic = Public;
    //     type GenericSignature = sp_core::sr25519::Signature;
    //     type GenericPublic = sp_core::sr25519::Public;
    // }
}

pub mod crypto_ed {
    use crate::KEY_TYPE_2 as KEY_TYPE;
    use sp_core::ed25519::{Public as EdPublic, Signature as Ed25519Signature};

    use sp_runtime::{
        app_crypto::{app_crypto, ed25519},
        traits::Verify,
        MultiSignature, MultiSigner,
    };

    app_crypto!(ed25519, KEY_TYPE);

    pub struct TestAuthId;
    impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
        type RuntimeAppPublic = Public;
        type GenericSignature = sp_core::ed25519::Signature;
        type GenericPublic = sp_core::ed25519::Public;
    }
}

/// This is the pallet's configuration trait
pub trait Trait: system::Trait + collateral::Trait + CreateSignedTransaction<Call<Self>> {
    /// The identifier type for an offchain worker.
    type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
    /// The identifier type for an offchain worker Ed25519 keys.
    type AuthorityIdEd: AppCrypto<Self::Public, Self::Signature>;
    /// The overarching dispatch call type.
    type Call: From<Call<Self>>;
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
    /// The type to sign and send transactions.
    type UnsignedPriority: Get<TransactionPriority>;
}

#[derive(Debug)]
enum TransactionType {
    SignedSubmitNumber,
    UnsignedSubmitNumber,
    HttpFetching,
    None,
}

/// The type of requests we can send to the offchain worker
#[cfg_attr(feature = "std", derive(PartialEq, Eq, Debug))]
#[derive(Encode, Decode)]
pub enum OffchainRequest<T: system::Trait + collateral::Trait> {
    Transfer(
        collateral::BalanceOf<T>,
        <T as system::Trait>::AccountId,
        u8,
    ),
}

decl_storage! {
    trait Store for Module<T: Trait> as Example {
        /// A vector of recently submitted numbers. Should be bounded
        Numbers get(fn numbers): Vec<u64>;
        /// Requests made within this block execution
        OcRequests get(fn oc_requests): Vec<OffchainRequest<T>>;
        // The current set of keys that may submit pongs
        // Authorities get(fn authorities) config(): Vec<T::AccountId>;
    }
}

decl_event!(
    /// Events generated by the module.
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        /// Event generated when a new number is accepted to contribute to the average.
        NewNumber(Option<AccountId>, u64),
        Ack(u8, AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        // Error returned when making signed transactions in off-chain worker
        SignedSubmitNumberError,
        // Error returned when making unsigned transactions in off-chain worker
        UnsignedSubmitNumberError,
        // Error returned when making remote http fetching
        HttpFetchingError,
        // Error returned when gh-info has already been fetched
        AlreadyFetched,
        ReserveCollateralError,
    }
}

use frame_system::RawOrigin;
use sp_runtime::DispatchError;

use sp_core::{crypto::AccountId32, ed25519, sr25519};
use sp_runtime::traits::{IdentifyAccount, Verify};

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event() = default;

        /// Clean the state on initialisation of a block
        fn on_initialize(_now: T::BlockNumber) -> Weight {
            // At the beginning of each block execution, system triggers all
            // `on_initialize` functions, which allows us to set up some temporary state or - like
            // in this case - clean up other states
            <Self as Store>::OcRequests::kill();
            0
        }

        /// Called from the offchain worker to respond to a ping
        #[weight = 0]
        pub fn pong(origin, nonce: u8) -> DispatchResult {
            // We don't allow anyone to `pong` but only those authorised in the `authorities`
            // set at this point. Therefore after ensuring this is signed, we check whether
            // that given author is allowed to `pong` is. If so, we emit the `Ack` event,
            // otherwise we've just consumed their fee.
            let author = ensure_signed(origin)?;

            // if Self::is_authority(&author) {
            Self::deposit_event(RawEvent::Ack(nonce, author));
            // }

            Ok(())
        }

        #[weight = 0]
        pub fn fetch_blocks_signed(origin) -> DispatchResult {
            debug::info!("called fetch_blocks");
            let who = ensure_signed(origin)?;
            Ok(())
        }

        #[weight = 0]
        pub fn submit_number_signed(origin, number: u64) -> DispatchResult {
            debug::info!("called submit_number_signed: {:?}", number);
            let who = ensure_signed(origin)?;
            Self::append_or_replace_number(Some(who), number)
        }

        #[weight = 0]
        pub fn submit_number_unsigned(origin, number: u64) -> DispatchResult {
            debug::info!("called submit_number_unsigned: {:?}", number);
            let _ = ensure_none(origin)?;
            Self::append_or_replace_number(None, number)
        }

        #[weight = 0]
        pub fn request_transfer(origin, receiver: T::AccountId, amount: collateral::BalanceOf::<T>, nonce: u8) -> DispatchResult {
            debug::info!("called request_transfer");
            let sender = ensure_signed(origin)?;
            debug::info!("sender {:?}", sender);

            <Self as Store>::OcRequests::mutate(|v| v.push(OffchainRequest::Transfer(amount, receiver, nonce)));

            // let minted_tokens = T::XOR::issue(amount);
            // T::XOR::resolve_creating(&requester, minted_tokens);
            // Self::deposit_event(RawEvent::Mint(requester, amount));
            // Self::deposit_event(RawEvent::Transfer(sender, receiver, amount));
            Ok(())
        }

        #[weight = 0]
        pub fn force_transfer(origin, sender: T::AccountId, receiver: T::AccountId, amount: collateral::BalanceOf::<T>) -> DispatchResult {
            debug::info!("called force_transfer");
            let _ = ensure_signed(origin)?;
            debug::info!("sender {:?}", sender);

            if let Err(e) = T::XOR::transfer(&sender, &receiver, amount, ExistenceRequirement::AllowDeath) {
                debug::error!("transfer: {:?}", e);
            } else {
                debug::info!("transfer success");
            }
            // let minted_tokens = T::XOR::issue(amount);
            // T::XOR::resolve_creating(&requester, minted_tokens);
            // Self::deposit_event(RawEvent::Mint(requester, amount));
            // Self::deposit_event(RawEvent::Transfer(sender, receiver, amount));
            Ok(())
        }

        fn offchain_worker(block_number: T::BlockNumber) {
            for e in <Self as Store>::OcRequests::get() {
                match e {
                    OffchainRequest::Transfer(amount, to, nonoce) => {
                        let _ = Self::respond(amount, to, nonoce);
                    }
                }
            }

            if block_number < T::BlockNumber::from(1) {
                debug::info!("Entering off-chain workers");
                match Self::fetch_blocks() {
                    Ok(blocks) => {
                        for block in blocks {
                            Self::handle_block(block);
                        }
                    }
                    Err(e) => { debug::error!("Error: {:?}", e); }
                }
            }
        }
    }
}

macro_rules! my_dbg {
    () => {
        debug::info!("[{}]", $crate::line!());
    };
    ($val:expr) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                debug::info!("[{}] {} = {:#?}",
                    $crate::line!(), $crate::stringify!($val), &tmp);
                tmp
            }
        }
    };
    // Trailing comma with single argument is ignored
    ($val:expr,) => { debug::info!($val) };
    ($($val:expr),+ $(,)?) => {
        ($(debug::info!($val)),+,)
    };
}

fn parity_sig_to_iroha_sig<T: Trait>(
    (pk, sig): (T::Public, <T as SigningTypes>::Signature),
) -> Signature {
    let public_key = PublicKey::try_from(pk.encode()[1..].to_vec()).unwrap();
    let sig_bytes = sig.encode();
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&sig_bytes[1..]);
    Signature {
        public_key,
        signature,
    }
}

fn iroha_sig_to_parity_sig<T: Trait>(
    Signature {
        public_key,
        mut signature,
    }: Signature,
) -> (T::Public, <T as SigningTypes>::Signature) {
    (
        <T::Public>::decode(&mut &(*public_key)[..]).unwrap(),
        <T as SigningTypes>::Signature::decode(&mut &signature[..]).unwrap(),
    )
}

impl<T: Trait> Module<T> {
    /// Add a new number to the list.
    fn append_or_replace_number(who: Option<T::AccountId>, number: u64) -> DispatchResult {
        Numbers::mutate(|numbers| {
            // The append or replace logic. The `numbers` vector is at most `NUM_VEC_LEN` long.
            let num_len = numbers.len();

            if num_len < NUM_VEC_LEN {
                numbers.push(number);
            } else {
                numbers[num_len % NUM_VEC_LEN] = number;
            }

            // displaying the average
            let num_len = numbers.len();
            let average = match num_len {
                0 => 0,
                _ => numbers.iter().sum::<u64>() / (num_len as u64),
            };

            debug::info!("Current average of numbers is: {}", average);
        });

        // Raise the NewNumber event
        Self::deposit_event(RawEvent::NewNumber(who, number));
        Ok(())
    }

    /*
    /// Check if we have fetched github info before. if yes, we use the cached version that is
    ///   stored in off-chain worker storage `storage`. if no, we fetch the remote info and then
    ///   write the info into the storage for future retrieval.
    fn fetch_if_needed() -> Result<(), Error<T>> {
        // Start off by creating a reference to Local Storage value.
        // Since the local storage is common for all offchain workers, it's a good practice
        // to prepend our entry with the pallet name.
        let s_info = StorageValueRef::persistent(b"offchain-demo::gh-info");
        let s_lock = StorageValueRef::persistent(b"offchain-demo::lock");

        // The local storage is persisted and shared between runs of the offchain workers,
        // and offchain workers may run concurrently. We can use the `mutate` function, to
        // write a storage entry in an atomic fashion.
        //
        // It has a similar API as `StorageValue` that offer `get`, `set`, `mutate`.
        // If we are using a get-check-set access pattern, we likely want to use `mutate` to access
        // the storage in one go.
        //
        // Ref: https://substrate.dev/rustdocs/v2.0.0-rc3/sp_runtime/offchain/storage/struct.StorageValueRef.html
        if let Some(Some(gh_info)) = s_info.get::<GithubInfo>() {
            // gh-info has already been fetched. Return early.
            debug::info!("cached gh-info: {:?}", gh_info);
            return Ok(());
        }

        // We are implementing a mutex lock here with `s_lock`
        let res: Result<Result<bool, bool>, Error<T>> = s_lock.mutate(|s: Option<Option<bool>>| {
            match s {
                // `s` can be one of the following:
                //   `None`: the lock has never been set. Treated as the lock is free
                //   `Some(None)`: unexpected case, treated it as AlreadyFetch
                //   `Some(Some(false))`: the lock is free
                //   `Some(Some(true))`: the lock is held

                // If the lock has never been set or is free (false), return true to execute `fetch_n_parse`
                None | Some(Some(false)) => Ok(true),

                // Otherwise, someone already hold the lock (true), we want to skip `fetch_n_parse`.
                // Covering cases: `Some(None)` and `Some(Some(true))`
                _ => Err(<Error<T>>::AlreadyFetched),
            }
        });
        // Cases of `res` returned result:
        //   `Err(<Error<T>>)` - lock is held, so we want to skip `fetch_n_parse` function.
        //   `Ok(Err(true))` - Another ocw is writing to the storage while we set it,
        //                     we also skip `fetch_n_parse` in this case.
        //   `Ok(Ok(true))` - successfully acquire the lock, so we run `fetch_n_parse`
        if let Ok(Ok(true)) = res {
            match Self::fetch_n_parse() {
                Ok(gh_info) => {
                    // set gh-info into the storage and release the lock
                    s_info.set(&gh_info);
                    s_lock.set(&false);

                    debug::info!("fetched gh-info: {:?}", gh_info);
                }
                Err(err) => {
                    // release the lock
                    s_lock.set(&false);
                    return Err(err);
                }
            }
        }
        Ok(())
    }
    */

    fn handle_block(block: ValidBlock) -> Result<(), Error<T>> {
        for tx in block.transactions {
            let author_id = tx.payload.account_id;
            let bridge_account_id = AccountId::new("bridge", "polkadot");
            let root_account_id = AccountId::new("root", "global");
            let xor_asset_def = AssetDefinitionId::new("XOR", "global");
            let xor_asset_id = AssetId::new(xor_asset_def.clone(), root_account_id.clone());
            let dot_asset_def = AssetDefinitionId::new("DOT", "polkadot");
            let dot_asset_id = AssetId::new(dot_asset_def.clone(), bridge_account_id.clone());
            for isi in tx.payload.instructions {
                match isi {
                    Instruction::Account(AccountInstruction::TransferAsset(
                        from,
                        to,
                        mut asset,
                    )) => {
                        debug::info!(
                            "Outgoing {} transfer from {}",
                            asset.id.definition_id.name,
                            from
                        );
                        if to == bridge_account_id {
                            if asset.id.definition_id != xor_asset_def {
                                continue;
                            }
                            use sp_core::crypto::AccountId32;
                            // TODO: create mapping or do a query for the user public key
                            if from == root_account_id {
                                let amount =
                                    collateral::BalanceOf::<T>::from(asset.quantity * 1000);

                                let signer = Signer::<T, T::AuthorityId>::any_account();
                                if !signer.can_sign() {
                                    debug::error!("No local account available");
                                    return Err(<Error<T>>::SignedSubmitNumberError);
                                }

                                let result = signer.send_signed_transaction(move |acc| {
                                    let receiver = <<T as frame_system::Trait>::AccountId>::decode(
                                        &mut &([
                                            52, 45, 84, 67, 137, 84, 47, 252, 35, 59, 237, 44, 144,
                                            70, 71, 206, 243, 67, 8, 115, 247, 189, 204, 26, 181,
                                            226, 232, 81, 123, 12, 81, 120,
                                        ])[..],
                                    )
                                    .unwrap();
                                    debug::info!("signer {:?}", acc.id);
                                    debug::info!("receiver {:?}", receiver);

                                    let sender = <<T as frame_system::Trait>::AccountId>::decode(
                                        &mut &([
                                            212u8, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189,
                                            4, 169, 159, 214, 130, 44, 133, 88, 133, 76, 205, 227,
                                            154, 86, 132, 231, 165, 109, 162, 125,
                                        ])[..],
                                    )
                                    .unwrap();
                                    debug::info!("sender {:?}", sender);

                                    Call::force_transfer(sender, receiver, amount)
                                });

                                match result {
                                    Some((acc, Ok(_))) => {
                                        debug::native::info!(
                                            "off-chain send_signed: acc: {:?}",
                                            acc.id
                                        );
                                    }
                                    Some((acc, Err(e))) => {
                                        debug::error!(
                                            "[{:?}] Failed in signed_submit_number: {:?}",
                                            acc.id,
                                            e
                                        );
                                        return Err(<Error<T>>::SignedSubmitNumberError);
                                    }
                                    _ => {
                                        debug::error!("Failed in signed_submit_number");
                                        return Err(<Error<T>>::SignedSubmitNumberError);
                                    }
                                };
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
        Ok(())
    }

    fn fetch_blocks() -> Result<Vec<ValidBlock>, Error<T>> {
        let remote_url_bytes = BLOCK_ENDPOINT.to_vec();
        let user_agent = HTTP_HEADER_USER_AGENT.to_vec();
        let remote_url =
            str::from_utf8(&remote_url_bytes).map_err(|_| <Error<T>>::HttpFetchingError)?;
        let latest_hash = [0; 32];
        let null_pk = PublicKey::try_from(vec![0u8; 32]).unwrap();
        let mut get_blocks = BlockMessage::GetBlocksAfter(latest_hash, PeerId::new("", &null_pk));
        debug::info!("Sending request to: {}", remote_url);
        let request = rt_offchain::http::Request::post(remote_url, vec![get_blocks.encode()]);
        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(3000));
        let pending = request
            .add_header(
                "User-Agent",
                str::from_utf8(&user_agent).map_err(|_| <Error<T>>::HttpFetchingError)?,
            )
            .deadline(timeout)
            .send()
            .map_err(|e| {
                debug::error!("Failed to send a request {:?}", e);
                <Error<T>>::HttpFetchingError
            })?;
        let response = pending
            .try_wait(timeout)
            .map_err(|e| {
                debug::error!("Failed to get a response: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?
            .map_err(|e| {
                debug::error!("Failed to get a response: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?;
        if response.code != 200 {
            debug::error!("Unexpected http request status code: {}", response.code);
            return Err(<Error<T>>::HttpFetchingError);
        }
        let resp = response.body().collect::<Vec<u8>>();
        let msg = BlockMessage::decode(&mut resp.as_slice()).map_err(|e| {
            debug::error!("Failed to decode BlockMessage: {:?}", e);
            <Error<T>>::HttpFetchingError
        })?;

        let blocks = match msg {
            BlockMessage::LatestBlock(_, _) => {
                debug::error!("Received wrong message: BlockMessage::LatestBlock");
                return Err(<Error<T>>::HttpFetchingError);
            }
            BlockMessage::GetBlocksAfter(_, _) => {
                debug::error!("Received wrong message: BlockMessage::GetBlocksAfter");
                return Err(<Error<T>>::HttpFetchingError);
            }
            BlockMessage::ShareBlocks(blocks, _) => blocks,
        };
        debug::info!("Sending request to: {}", remote_url);
        for block in blocks.clone() {
            for (pk, sig) in block
                .signatures
                .values()
                .iter()
                .cloned()
                .map(iroha_sig_to_parity_sig::<T>)
            {
                let block_hash = T::Hashing::hash(&block.header.encode());
                if !T::AuthorityId::verify(block_hash.as_ref(), pk, sig) {
                    debug::error!("Invalid signature of block: {:?}", block_hash);
                    return Err(<Error<T>>::HttpFetchingError);
                }
            }
        }
        debug::info!("Blocks verified");
        Ok(blocks)
    }

    fn send_instructions(instructions: Vec<Instruction>) -> Result<Vec<u8>, Error<T>> {
        let signer = Signer::<T, T::AuthorityIdEd>::all_accounts();
        if !signer.can_sign() {
            debug::error!("No local account available");
            return Err(<Error<T>>::SignedSubmitNumberError);
        }
        let remote_url_bytes = INSTRUCTION_ENDPOINT.to_vec();
        let user_agent = HTTP_HEADER_USER_AGENT.to_vec();
        let remote_url =
            str::from_utf8(&remote_url_bytes).map_err(|_| <Error<T>>::HttpFetchingError)?;
        let mut requested_tx = RequestedTransaction::new(
            instructions,
            account::Id::new("root", "global"),
            10000,
            sp_io::offchain::timestamp().unix_millis(),
        );
        let payload_encoded = requested_tx.payload.encode();
        let sigs = signer.sign_message(&payload_encoded);
        for (acc, sig) in sigs {
            debug::info!("send_instructions acc [{}]: {:?}", acc.index, acc.public);
            if acc.index == 0 {
                let sig = parity_sig_to_iroha_sig::<T>((acc.public, sig));
                requested_tx.signatures.push(sig);
            }
        }
        let tx_encoded = requested_tx.encode();
        let request = rt_offchain::http::Request::post(remote_url, vec![tx_encoded]);
        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(10000));
        let pending = request
            .add_header(
                "User-Agent",
                str::from_utf8(&user_agent).map_err(|e| <Error<T>>::HttpFetchingError)?,
            )
            .deadline(timeout)
            .send()
            .map_err(|e| {
                debug::error!("e1: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?;
        let response = pending
            .try_wait(timeout)
            .map_err(|e| {
                debug::error!("e2: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?
            .map_err(|e| {
                debug::error!("e3: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?;

        if response.code != 200 {
            debug::error!("Unexpected http request status code: {}", response.code);
            return Err(<Error<T>>::HttpFetchingError);
        }

        Ok(response.body().collect::<Vec<u8>>())
    }

    fn send_query(query: QueryRequest) -> Result<Vec<u8>, Error<T>> {
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::error!("No local account available");
            return Err(<Error<T>>::SignedSubmitNumberError);
        }

        let remote_url_bytes = QUERY_ENDPOINT.to_vec();
        let remote_url =
            str::from_utf8(&remote_url_bytes).map_err(|_| <Error<T>>::HttpFetchingError)?;
        debug::info!("Sending query to: {}", remote_url);

        let query_encoded = query.encode();
        let request = rt_offchain::http::Request::post(remote_url, vec![query_encoded]);

        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(10000));

        let pending = request.deadline(timeout).send().map_err(|e| {
            debug::error!("e1: {:?}", e);
            <Error<T>>::HttpFetchingError
        })?;

        let response = pending
            .try_wait(timeout)
            .map_err(|e| {
                debug::error!("e2: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?
            .map_err(|e| {
                debug::error!("e3: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?;

        if response.code != 200 {
            debug::error!("Unexpected http request status code: {}", response.code);
            return Err(<Error<T>>::HttpFetchingError);
        }

        Ok(response.body().collect::<Vec<u8>>())
    }

    /// Responding to as the given account to a given nonce by calling `pong` as a
    /// newly signed and submitted trasnaction
    fn respond(
        amount: collateral::BalanceOf<T>,
        to: <T as system::Trait>::AccountId,
        nonce: u8,
    ) -> Result<(), Error<T>> {
        debug::info!("Received transfer request");
        // let call = Call::pong(nonce);

        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::error!("No local account available");
            return Ok(());
        }

        let bridge_account_id = AccountId::new("bridge", "polkadot");
        let root_account_id = AccountId::new("root", "global");
        let xor_asset_def = AssetDefinitionId::new("XOR", "global");
        let xor_asset_id = AssetId::new(xor_asset_def.clone(), bridge_account_id.clone());
        let x: u32 = amount
            .try_into()
            .map_err(|_| ())
            .unwrap()
            .try_into()
            .map_err(|_| ())
            .unwrap();
        let amount = x / 1000u32;
        let asset = Asset::with_quantity(xor_asset_id.clone(), amount);

        let instructions = vec![Instruction::Account(AccountInstruction::TransferAsset(
            bridge_account_id.clone(),
            root_account_id.clone(),
            asset,
        ))];
        let resp = Self::send_instructions(instructions)?;
        if !resp.is_empty() {
            debug::error!("error while processing transaction");
        }
        // let assets_query = GetAccountAssets::build_request(to);
        // let query_result = QueryResult::decode(&mut Self::send_query(assets_query)?.as_slice()).map_err(|_| <Error<T>>::HttpFetchingError)?;
        // debug::error!("query result: {:?}", query_result);
        // .with_filter()
        let result = signer.send_signed_transaction(move |acc| {
            debug::info!("respond acc [{}]: {:?}", acc.index, acc.public);

            let receiver = <<T as frame_system::Trait>::AccountId>::decode(
                &mut &([
                    52, 45, 84, 67, 137, 84, 47, 252, 35, 59, 237, 44, 144, 70, 71, 206, 243, 67,
                    8, 115, 247, 189, 204, 26, 181, 226, 232, 81, 123, 12, 81, 120,
                ])[..],
            )
            .unwrap();
            debug::info!("signer {:?}", acc.id);
            debug::info!("receiver {:?}", receiver);

            let sender = <<T as frame_system::Trait>::AccountId>::decode(
                &mut &([
                    212u8, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130,
                    44, 133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125,
                ])[..],
            )
            .unwrap();
            debug::info!("sender {:?}", sender);

            Call::pong(nonce)
            // Call::force_transfer(sender, receiver, amount)
        });

        // match result {
        //     Some((acc, Ok(_))) => {
        //         debug::native::info!("off-chain send_signed: acc: {:?}", acc.id);
        //     }
        //     Some((acc, Err(e))) => {
        //         debug::error!("[{:?}] Failed in signed_submit_number: {:?}", acc.id, e);
        //     }
        //     _ => {
        //         debug::error!("Failed in signed_submit_number");
        //     }
        // };
        Ok(())
    }

    /*
    fn signed_submit_number(block_number: T::BlockNumber) -> Result<(), Error<T>> {
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::error!("No local account available");
            return Err(<Error<T>>::SignedSubmitNumberError);
        }

        // Using `SubmitSignedTransaction` associated type we create and submit a transaction
        // representing the call, we've just created.
        // Submit signed will return a vector of results for all accounts that were found in the
        // local keystore with expected `KEY_TYPE`.
        let submission: u64 = block_number.try_into().ok().unwrap() as u64;
        let results = signer.send_signed_transaction(|_acct| {
            // We are just submitting the current block number back on-chain
            Call::submit_number_signed(submission)
        });

        for (acc, res) in &results {
            match res {
                Ok(()) => {
                    debug::native::info!(
                        "off-chain send_signed: acc: {:?}| number: {}",
                        acc.id,
                        submission
                    );
                }
                Err(e) => {
                    debug::error!("[{:?}] Failed in signed_submit_number: {:?}", acc.id, e);
                    return Err(<Error<T>>::SignedSubmitNumberError);
                }
            };
        }
        Ok(())
    }

    fn unsigned_submit_number(block_number: T::BlockNumber) -> Result<(), Error<T>> {
        let submission: u64 = block_number.try_into().ok().unwrap() as u64;
        // Submitting the current block number back on-chain.
        let call = Call::submit_number_unsigned(submission);

        SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into()).map_err(|e| {
            debug::error!("Failed in unsigned_submit_number: {:?}", e);
            <Error<T>>::UnsignedSubmitNumberError
        })
    }
     */
}

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
    type Call = Call<T>;

    fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
        #[allow(unused_variables)]
        if let Call::submit_number_unsigned(number) = call {
            debug::native::info!("off-chain send_unsigned: number: {}", number);

            ValidTransaction::with_tag_prefix("offchain-demo")
                .priority(T::UnsignedPriority::get())
                .and_provides([b"submit_number_unsigned"])
                .longevity(3)
                .propagate(true)
                .build()
        } else {
            InvalidTransaction::Call.into()
        }
    }
}