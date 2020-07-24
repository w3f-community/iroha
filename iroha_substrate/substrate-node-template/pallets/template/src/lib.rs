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

use frame_system::{
    self as system, ensure_none, ensure_signed,
    offchain::{
        AppCrypto, CreateSignedTransaction, SendSignedTransaction, Signer, SubmitTransaction,
    },
};
use sp_core::crypto::KeyTypeId;
use sp_core::ed25519::Signature as SpSignature;
use sp_runtime::{
    offchain as rt_offchain,
    offchain::storage::StorageValueRef,
    transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    },
};
use sp_std::prelude::*;
use sp_std::str;
// We use `alt_serde`, and Xanewok-modified `serde_json` so that we can compile the program
//   with serde(features `std`) and alt_serde(features `no_std`).
use alt_serde::{Deserialize, Deserializer};
use frame_system::offchain::{Account, SignMessage, SigningTypes};
use iroha_client_no_std::account;
use iroha_client_no_std::block::{Message as BlockMessage, ValidBlock};
use iroha_client_no_std::crypto::{PublicKey, Signature};
use iroha_client_no_std::isi::prelude::PeerInstruction;
use iroha_client_no_std::peer::PeerId;
use iroha_client_no_std::prelude::*;
use iroha_client_no_std::tx::RequestedTransaction;
use sp_runtime::offchain::http::Request;
use sp_runtime::traits::Hash;
use sp_std::convert::TryFrom;
use iroha_client_no_std::account::isi::AccountInstruction;
use iroha_client_no_std::asset::isi::AssetInstruction;
use iroha_client_no_std::asset::query::GetAccountAssets;

/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"demo");
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
    use sp_core::ed25519::Signature as Ed25519Signature;
    use sp_runtime::{
        app_crypto::{app_crypto, ed25519},
        traits::Verify,
        MultiSignature, MultiSigner,
    };

    app_crypto!(ed25519, KEY_TYPE);

    pub struct TestAuthId;
    // implemented for ocw-runtime
    impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
        type RuntimeAppPublic = Public;
        type GenericSignature = sp_core::ed25519::Signature;
        type GenericPublic = sp_core::ed25519::Public;
    }

    // implemented for mock runtime in test
    impl frame_system::offchain::AppCrypto<<Ed25519Signature as Verify>::Signer, Ed25519Signature>
        for TestAuthId
    {
        type RuntimeAppPublic = Public;
        type GenericSignature = sp_core::ed25519::Signature;
        type GenericPublic = sp_core::ed25519::Public;
    }
}

/// This is the pallet's configuration trait
pub trait Trait: system::Trait + CreateSignedTransaction<Call<Self>> {
    /// The identifier type for an offchain worker.
    type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
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

decl_storage! {
    trait Store for Module<T: Trait> as Example {
        /// A vector of recently submitted numbers. Should be bounded
        Numbers get(fn numbers): Vec<u64>;
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
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event() = default;

        #[weight = 0]
        pub fn fetch_blocks_signed(origin) -> DispatchResult {
            debug::info!("fetch_blocks");
            let who = ensure_signed(origin)?;
            Ok(())
        }

        #[weight = 0]
        pub fn submit_number_signed(origin, number: u64) -> DispatchResult {
            debug::info!("submit_number_signed: {:?}", number);
            let who = ensure_signed(origin)?;
            Self::append_or_replace_number(Some(who), number)
        }

        #[weight = 0]
        pub fn submit_number_unsigned(origin, number: u64) -> DispatchResult {
            debug::info!("submit_number_unsigned: {:?}", number);
            let _ = ensure_none(origin)?;
            Self::append_or_replace_number(None, number)
        }

        fn offchain_worker(block_number: T::BlockNumber) {
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
 044a489ca7b2 42abebbe016f bf6ec47ac5fd bf6ec47ac5fd bf6ec47ac5fd 4136ba27e279 4136ba27e279 4136ba27e279 af4fdf2df06c af4fdf2df06c af4fdf2df06c eefa22c7b7e7 eefa22c7b7e7 eefa22c7b7e7 52bb8d969801 52bb8d969801 52bb8d969801 8f2b5101881b 8f2b5101881b 8f2b5101881b 3e83cbc5b332 3e83cbc5b332 3e83cbc5b332 3b96a893c1e4 3b96a893c1e4 3b96a893c1e4 ede9389347db ede9389347db ede9389347db caaae0474ef2 caaae0474ef2 caaae0474ef2 d369d4eaa0fd d369d4eaa0fd d369d4eaa0fd c35726322d15

dev-peer0.org2.example.com-fabcar_1-65710fa851d5c73690faa4709ef40b798c085e7210c46d44f8b1e2d5a062c9b0-4e2b229875474d0d0f1825ba1a81136efd9f4ec456c771bd40ea58269ecf4550   latest              b23a5b4bcfc9        2 months ago        22.5MB
dev-peer0.org1.example.com-fabcar_1-65710fa851d5c73690faa4709ef40b798c085e7210c46d44f8b1e2d5a062c9b0-0ca9287fd994c9e3127d7fbba2f50ee23bae41fca7349e5a02c07da5bccc19d2   latest              42abebbe016f        2 months ago        22.5MB
hyperledger/fabric-tools                                                                                                                                                2.1                 bf6ec47ac5fd        3 months ago        522MB
hyperledger/fabric-tools                                                                                                                                                2.1.0               bf6ec47ac5fd        3 months ago        522MB
hyperledger/fabric-tools                                                                                                                                                latest              bf6ec47ac5fd        3 months ago        522MB
hyperledger/fabric-peer                                                                                                                                                 2.1                 4136ba27e279        3 months ago        56.6MB
hyperledger/fabric-peer                                                                                                                                                 2.1.0               4136ba27e279        3 months ago        56.6MB
hyperledger/fabric-peer                                                                                                                                                 latest              4136ba27e279        3 months ago        56.6MB
hyperledger/fabric-orderer                                                                                                                                              2.1                 af4fdf2df06c        3 months ago        39.4MB
hyperledger/fabric-orderer                                                                                                                                              2.1.0               af4fdf2df06c        3 months ago        39.4MB
hyperledger/fabric-orderer                                                                                                                                              latest              af4fdf2df06c        3 months ago        39.4MB
hyperledger/fabric-ccenv                                                                                                                                                2.1                 eefa22c7b7e7        3 months ago        554MB
hyperledger/fabric-ccenv                                                                                                                                                2.1.0               eefa22c7b7e7        3 months ago        554MB
hyperledger/fabric-ccenv                                                                                                                                                latest              eefa22c7b7e7        3 months ago        554MB
hyperledger/fabric-baseos                                                                                                                                               2.1                 52bb8d969801        3 months ago        6.94MB
hyperledger/fabric-baseos                                                                                                                                               2.1.0               52bb8d969801        3 months ago        6.94MB
hyperledger/fabric-baseos                                                                                                                                               latest              52bb8d969801        3 months ago        6.94MB
hyperledger/fabric-nodeenv                                                                                                                                              2.1                 8f2b5101881b        3 months ago        292MB
hyperledger/fabric-nodeenv                                                                                                                                              2.1.0               8f2b5101881b        3 months ago        292MB
hyperledger/fabric-nodeenv                                                                                                                                              latest              8f2b5101881b        3 months ago        292MB
hyperledger/fabric-javaenv                                                                                                                                              2.1                 3e83cbc5b332        3 months ago        505MB
hyperledger/fabric-javaenv                                                                                                                                              2.1.0               3e83cbc5b332        3 months ago        505MB
hyperledger/fabric-javaenv                                                                                                                                              latest              3e83cbc5b332        3 months ago        505MB
hyperledger/fabric-ca                                                                                                                                                   1.4                 3b96a893c1e4        4 months ago        150MB
hyperledger/fabric-ca                                                                                                                                                   1.4.6               3b96a893c1e4        4 months ago        150MB
hyperledger/fabric-ca                                                                                                                                                   latest              3b96a893c1e4        4 months ago        150MB
hyperledger/fabric-zookeeper                                                                                                                                            0.4                 ede9389347db        8 months ago        276MB
hyperledger/fabric-zookeeper                                                                                                                                            0.4.18              ede9389347db        8 months ago        276MB
hyperledger/fabric-zookeeper                                                                                                                                            latest              ede9389347db        8 months ago        276MB
hyperledger/fabric-kafka                                                                                                                                                0.4                 caaae0474ef2        8 months ago        270MB
hyperledger/fabric-kafka                                                                                                                                                0.4.18              caaae0474ef2        8 months ago        270MB
hyperledger/fabric-kafka                                                                                                                                                latest              caaae0474ef2        8 months ago        270MB
hyperledger/fabric-couchdb                                                                                                                                              0.4                 d369d4eaa0fd        8 months ago        261MB
hyperledger/fabric-couchdb                                                                                                                                              0.4.18              d369d4eaa0fd        8 months ago        261MB
hyperledger/fabric-couchdb                                                                                                                                              latest              d369d4eaa0fd        8 months ago        261MB
tyz910/holdem                                                                                                                                                           latest              c35726322d15        20 months ago       652MB
/*
        /// Transfer an amount of PolkaBTC (without fees)
        ///
        /// # Arguments
        ///
        /// * `origin` - sender of the transaction
        /// * `receiver` - receiver of the transaction
        /// * `amount` - amount of PolkaBTC
        #[weight = 1000]
        fn transfer(origin, receiver: T::AccountId, amount: BalanceOf<T>)
            -> DispatchResult
        {
            let sender = ensure_signed(origin)?;

            T::PolkaBTC::transfer(&sender, &receiver, amount, KeepAlive)
                .map_err(|_| Error::InsufficientFunds)?;

            // Self::deposit_event(RawEvent::Transfer(sender, receiver, amount));

            Ok(())
        }

        #[weight = 1000]
        fn issue(origin, receiver: T::AccountId, amount: BalanceOf<T>)
            -> DispatchResult
        {
            let sender = ensure_signed(origin)?;

            // adds the amount to the total balance of tokens
            let minted_tokens = T::PolkaBTC::issue(amount);
            // adds the added amount to the requester's balance
            T::PolkaBTC::resolve_creating(&requester, minted_tokens);

            // Self::deposit_event(RawEvent::Mint(requester, amount));
            Ok(())
        }
 */
    }
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
    fn choose_tx_type(block_number: T::BlockNumber) -> TransactionType {
        // Decide what type of transaction to send based on block number.
        // Each block the offchain worker will send one type of transaction back to the chain.
        // First a signed transaction, then an unsigned transaction, then an http fetch and json parsing.
        match block_number.try_into().ok().unwrap() % 3 {
            0 => TransactionType::SignedSubmitNumber,
            1 => TransactionType::UnsignedSubmitNumber,
            2 => TransactionType::HttpFetching,
            _ => TransactionType::None,
        }
    }
    */

    // /// Check if we have fetched github info before. if yes, we use the cached version that is
    // ///   stored in off-chain worker storage `storage`. if no, we fetch the remote info and then
    // ///   write the info into the storage for future retrieval.
    // fn fetch_if_needed() -> Result<(), Error<T>> {
    //     // Start off by creating a reference to Local Storage value.
    //     // Since the local storage is common for all offchain workers, it's a good practice
    //     // to prepend our entry with the pallet name.
    //     let s_info = StorageValueRef::persistent(b"offchain-demo::gh-info");
    //     let s_lock = StorageValueRef::persistent(b"offchain-demo::lock");
    //
    //     // The local storage is persisted and shared between runs of the offchain workers,
    //     // and offchain workers may run concurrently. We can use the `mutate` function, to
    //     // write a storage entry in an atomic fashion.
    //     //
    //     // It has a similar API as `StorageValue` that offer `get`, `set`, `mutate`.
    //     // If we are using a get-check-set access pattern, we likely want to use `mutate` to access
    //     // the storage in one go.
    //     //
    //     // Ref: https://substrate.dev/rustdocs/v2.0.0-rc3/sp_runtime/offchain/storage/struct.StorageValueRef.html
    //     if let Some(Some(gh_info)) = s_info.get::<GithubInfo>() {
    //         // gh-info has already been fetched. Return early.
    //         debug::info!("cached gh-info: {:?}", gh_info);
    //         return Ok(());
    //     }
    //
    //     // We are implementing a mutex lock here with `s_lock`
    //     let res: Result<Result<bool, bool>, Error<T>> = s_lock.mutate(|s: Option<Option<bool>>| {
    //         match s {
    //             // `s` can be one of the following:
    //             //   `None`: the lock has never been set. Treated as the lock is free
    //             //   `Some(None)`: unexpected case, treated it as AlreadyFetch
    //             //   `Some(Some(false))`: the lock is free
    //             //   `Some(Some(true))`: the lock is held
    //
    //             // If the lock has never been set or is free (false), return true to execute `fetch_n_parse`
    //             None | Some(Some(false)) => Ok(true),
    //
    //             // Otherwise, someone already hold the lock (true), we want to skip `fetch_n_parse`.
    //             // Covering cases: `Some(None)` and `Some(Some(true))`
    //             _ => Err(<Error<T>>::AlreadyFetched),
    //         }
    //     });
    //     // Cases of `res` returned result:
    //     //   `Err(<Error<T>>)` - lock is held, so we want to skip `fetch_n_parse` function.
    //     //   `Ok(Err(true))` - Another ocw is writing to the storage while we set it,
    //     //                     we also skip `fetch_n_parse` in this case.
    //     //   `Ok(Ok(true))` - successfully acquire the lock, so we run `fetch_n_parse`
    //     if let Ok(Ok(true)) = res {
    //         match Self::fetch_n_parse() {
    //             Ok(gh_info) => {
    //                 // set gh-info into the storage and release the lock
    //                 s_info.set(&gh_info);
    //                 s_lock.set(&false);
    //
    //                 debug::info!("fetched gh-info: {:?}", gh_info);
    //             }
    //             Err(err) => {
    //                 // release the lock
    //                 s_lock.set(&false);
    //                 return Err(err);
    //             }
    //         }
    //     }
    //     Ok(())
    // }
    /*
    /// Fetch from remote and deserialize the JSON to a struct
    fn fetch_n_parse() -> Result<GithubInfo, Error<T>> {
        let resp_bytes = Self::fetch_from_remote().map_err(|e| {
            debug::error!("fetch_from_remote error: {:?}", e);
            <Error<T>>::HttpFetchingError
        })?;

        let resp_str = str::from_utf8(&resp_bytes).map_err(|_| <Error<T>>::HttpFetchingError)?;
        // Print out our fetched JSON string
        debug::info!("{}", resp_str);

        // Deserializing JSON to struct, thanks to `serde` and `serde_derive`
        let gh_info: GithubInfo =
            serde_json::from_str(&resp_str).map_err(|_| <Error<T>>::HttpFetchingError)?;
        Ok(gh_info)
    }
    */

    fn handle_block(block: ValidBlock) -> Result<(), Error<T>> {
        for tx in block.transactions {
            let author_id = tx.payload.account_id;
            let bridge_account_id = AccountId::new("bridge", "polkadot");
            let root_account_id = AccountId::new("root", "global");
            let xor_asset_id = AssetId::new(AssetDefinitionId::new("XOR", "global"), root_account_id);
            let dot_asset_def = AssetDefinitionId::new("DOT", "polkadot");
            let dot_asset_id = AssetId::new(dot_asset_def, bridge_account_id.clone());
            for isi in tx.payload.instructions {
                debug::info!("ISI: {:?}", isi);
                match isi {
                    Instruction::Account(AccountInstruction::TransferAsset(from, to, mut asset)) => {
                        debug::info!("outgoing transfer from {:?}: {:?}", from, asset);
                        debug::info!("to ID: {:?}", to);
                        if to == bridge_account_id {
                            if asset.id != xor_asset_id {
                                continue;
                            }
                            let (from, to) = (to, from);
                            let exchange_rate = 2;
                            let amount = asset.quantity * exchange_rate;
                            let exchanged_asset = Asset::with_quantity(dot_asset_id.clone(), amount);
                            // asset.id.definition_id.name = "213".into();
                            let instructions = vec![
                                Instruction::Asset(AssetInstruction::MintAsset(amount, dot_asset_id.clone())),
                                Instruction::Account(AccountInstruction::TransferAsset(from, to.clone(), exchanged_asset)),
                            ];
                            let resp = Self::send_instructions(instructions)?;
                            if !resp.is_empty() {
                                debug::error!("error while processing transaction");
                            }
                            let assets_query = GetAccountAssets::build_request(to);
                            let query_result = QueryResult::decode(&mut Self::send_query(assets_query)?.as_slice()).map_err(|_| <Error<T>>::HttpFetchingError)?;
                            debug::error!("query result: {:?}", query_result);
                        }
                    }
                    _ => ()
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
        // let mut get_blocks = BlockMessage::B(latest_hash, PeerId::new("", &null_pk));
        let mut get_blocks = BlockMessage::GetBlocksAfter(latest_hash, PeerId::new("", &null_pk));
        debug::info!("sending request to: {}", remote_url);
        let request = rt_offchain::http::Request::post(remote_url, vec![get_blocks.encode()]);
        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(3000));
        let pending = request
            .add_header(
                "User-Agent",
                str::from_utf8(&user_agent).map_err(|_| <Error<T>>::HttpFetchingError)?,
            )
            .deadline(timeout)
            .send()
            .map_err(|_| <Error<T>>::HttpFetchingError)?;
        let response = pending
            .try_wait(timeout)
            .map_err(|_| <Error<T>>::HttpFetchingError)?
            .map_err(|_| <Error<T>>::HttpFetchingError)?;
        if response.code != 200 {
            debug::error!("Unexpected http request status code: {}", response.code);
            return Err(<Error<T>>::HttpFetchingError);
        }
        let resp = response.body().collect::<Vec<u8>>();
        let msg = BlockMessage::decode(&mut resp.as_slice())
            .map_err(|_| <Error<T>>::HttpFetchingError)?;
        let blocks = match msg {
            BlockMessage::LatestBlock(_, _) => {
                return Err(<Error<T>>::HttpFetchingError);
            }
            BlockMessage::GetBlocksAfter(_, _) => {
                return Err(<Error<T>>::HttpFetchingError);
            }
            BlockMessage::ShareBlocks(blocks, _) => blocks,
        };
        debug::info!("sending request to: {}", remote_url);
        for block in blocks.clone() {
            for (pk, sig) in block
                .signatures
                .values()
                .iter()
                .cloned()
                .map(iroha_sig_to_parity_sig::<T>)
            {
                if !T::AuthorityId::verify(
                    T::Hashing::hash(&block.header.encode()).as_ref(),
                    pk,
                    sig,
                ) {
                    debug::error!("Invalid block signature");
                    return Err(<Error<T>>::HttpFetchingError);
                }
            }
        }
        debug::info!("Blocks verified");
        Ok(blocks)
    }

    fn send_instructions(instructions: Vec<Instruction>) -> Result<Vec<u8>, Error<T>> {
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::error!("No local account available");
            return Err(<Error<T>>::SignedSubmitNumberError);
        }

        let remote_url_bytes = INSTRUCTION_ENDPOINT.to_vec();
        let user_agent = HTTP_HEADER_USER_AGENT.to_vec();
        let remote_url =
            str::from_utf8(&remote_url_bytes).map_err(|_| <Error<T>>::HttpFetchingError)?;
        // let pk = [
        //     101u8, 170, 80, 164, 103, 38, 73, 61, 223, 133, 83, 139, 247, 77, 176, 84, 117, 15, 22,
        //     28, 155, 125, 80, 226, 40, 26, 61, 248, 40, 159, 58, 53,
        // ];
        //
        // let pk = PublicKey::try_from((&pk[..]).to_vec()).unwrap();
        let mut requested_tx =
            RequestedTransaction::new(instructions, account::Id::new("root", "global"), 10000, sp_io::offchain::timestamp().unix_millis());

        let payload_encoded = requested_tx.payload.encode();
        let sigs = signer.sign_message(&payload_encoded);
        for (acc, sig) in sigs {
            let sig = parity_sig_to_iroha_sig::<T>((acc.public, sig));
            requested_tx.signatures.push(sig);
        }

        debug::info!("sending request to: {}", remote_url);

        let tx_encoded = requested_tx.encode();
        debug::info!("tx bytes: {:?}", tx_encoded);
        let request = rt_offchain::http::Request::post(remote_url, vec![tx_encoded]);

        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(10000));

        let pending = request
            .add_header(
                "User-Agent",
                str::from_utf8(&user_agent).map_err(|e| {
                    debug::error!("e4: {:?}", e);
                    <Error<T>>::HttpFetchingError
                })?,
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
        debug::info!("sending query to: {}", remote_url);

        let query_encoded = query.encode();
        let request = rt_offchain::http::Request::post(remote_url, vec![query_encoded]);

        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(10000));

        let pending = request
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
