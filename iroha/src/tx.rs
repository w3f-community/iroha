//! This module contains Transaction related functionality of the Iroha.
//!
//! `RequestedTransaction` is the start of the Transaction lifecycle.

use crate::prelude::*;
use iroha_derive::Io;
use parity_scale_codec::{Decode, Encode};
use std::time::SystemTime;

/// This structure represents transaction in non-trusted form.
///
/// `Iroha` and its' clients use `RequestedTransaction` to send transactions via network.
/// Direct usage in business logic is strongly prohibited. Before any interactions
/// `accept`.
#[derive(Clone, Debug, Io, Encode, Decode)]
pub struct RequestedTransaction {
    payload: Payload,
    signatures: Vec<Signature>,
}

#[derive(Clone, Debug, Io, Encode, Decode)]
struct Payload {
    /// Account ID of transaction creator.
    account_id: <Account as Identifiable>::Id,
    /// An ordered set of instructions.
    instructions: Vec<Instruction>,
    /// Time of creation (unix time, in milliseconds).
    creation_time: String,
}

impl RequestedTransaction {
    /// Default `RequestedTransaction` constructor.
    pub fn new(
        instructions: Vec<Instruction>,
        account_id: <Account as Identifiable>::Id,
    ) -> RequestedTransaction {
        RequestedTransaction {
            payload: Payload {
                instructions,
                account_id,
                creation_time: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("Failed to get System Time.")
                    .as_millis()
                    .to_string(),
            },
            signatures: Vec::new(),
        }
    }

    /// Transaction acceptance will check that transaction signatures are valid and move state one
    /// step forward.
    ///
    /// Returns `Ok(AcceptedTransaction)` if succeeded and `Err(String)` if failed.
    pub fn accept(self) -> Result<AcceptedTransaction, String> {
        for signature in &self.signatures {
            if let Err(e) = signature.verify(&Vec::from(&self.payload)) {
                return Err(format!("Failed to verify signatures: {}", e));
            }
        }
        Ok(AcceptedTransaction {
            payload: self.payload,
            signatures: self.signatures,
        })
    }
}

/// An ordered set of instructions, which is applied to the ledger atomically.
///
/// Transactions received by `Iroha` from external resources (clients, peers, etc.)
/// go through several steps before will be added to the blockchain and stored.
/// Starting in form of `RequestedTransaction` transaction it changes state based on interactions
/// with `Iroha` subsystems.
#[derive(Clone, Debug, Io, Encode, Decode)]
pub struct AcceptedTransaction {
    payload: Payload,
    signatures: Vec<Signature>,
}

impl AcceptedTransaction {
    /// Sign transaction with the provided key pair.
    ///
    /// Returns `Ok(SignedTransaction)` if succeeded and `Err(String)` if failed.
    pub fn sign(
        self,
        public_key: &PublicKey,
        private_key: &PrivateKey,
    ) -> Result<SignedTransaction, String> {
        let mut signatures = self.signatures.clone();
        signatures.push(Signature::new(
            *public_key,
            &Vec::from(&self.payload),
            private_key,
        )?);
        Ok(SignedTransaction {
            payload: self.payload,
            signatures,
        })
    }

    /// Calculate transaction `Hash`.
    pub fn hash(&self) -> Hash {
        use ursa::blake2::{
            digest::{Input, VariableOutput},
            VarBlake2b,
        };
        let bytes: Vec<u8> = self.payload.clone().into();
        let vec_hash = VarBlake2b::new(32)
            .expect("Failed to initialize variable size hash")
            .chain(bytes)
            .vec_result();
        let mut hash = [0; 32];
        hash.copy_from_slice(&vec_hash);
        hash
    }
}

/// `SignedTransaction` represents transaction with signatures accumulated from Peer/Peers.
#[derive(Clone, Debug, Io, Encode, Decode)]
pub struct SignedTransaction {
    payload: Payload,
    signatures: Vec<Signature>,
}

impl SignedTransaction {
    /// Add additional Signatures.
    pub fn sign(self, signatures: Vec<Signature>) -> Result<SignedTransaction, String> {
        Ok(SignedTransaction {
            payload: self.payload,
            signatures: vec![self.signatures, signatures]
                .into_iter()
                .flatten()
                .collect(),
        })
    }

    /// Move transaction lifecycle forward by checking an ability to apply instructions to the
    /// `WorldStateView`.
    ///
    /// Returns `Ok(ValidTransaction)` if succeeded and `Err(String)` if failed.
    pub fn validate(
        self,
        world_state_view: &mut WorldStateView,
    ) -> Result<ValidTransaction, String> {
        for instruction in &self.payload.instructions {
            instruction.execute(self.payload.account_id.clone(), world_state_view)?;
        }
        Ok(ValidTransaction {
            payload: self.payload,
            signatures: self.signatures,
        })
    }

    /// Calculate transaction `Hash`.
    pub fn hash(&self) -> Hash {
        use ursa::blake2::{
            digest::{Input, VariableOutput},
            VarBlake2b,
        };
        let bytes: Vec<u8> = self.into();
        let vec_hash = VarBlake2b::new(32)
            .expect("Failed to initialize variable size hash")
            .chain(bytes)
            .vec_result();
        let mut hash = [0; 32];
        hash.copy_from_slice(&vec_hash);
        hash
    }
}

/// `ValidTransaction` represents trustfull Transaction state.
#[derive(Clone, Debug, Io, Encode, Decode)]
pub struct ValidTransaction {
    payload: Payload,
    signatures: Vec<Signature>,
}

impl ValidTransaction {
    /// Apply instructions to the `WorldStateView`.
    pub fn proceed(&self, world_state_view: &mut WorldStateView) -> Result<(), String> {
        for instruction in &self.payload.instructions {
            if let Err(e) = instruction.execute(self.payload.account_id.clone(), world_state_view) {
                eprintln!("Failed to invoke instruction on WSV: {}", e);
            }
        }
        Ok(())
    }
}

impl From<&AcceptedTransaction> for RequestedTransaction {
    fn from(transaction: &AcceptedTransaction) -> RequestedTransaction {
        let transaction = transaction.clone();
        RequestedTransaction {
            payload: transaction.payload,
            signatures: transaction.signatures,
        }
    }
}

impl From<&SignedTransaction> for RequestedTransaction {
    fn from(transaction: &SignedTransaction) -> RequestedTransaction {
        let transaction = transaction.clone();
        RequestedTransaction::from(transaction)
    }
}

impl From<SignedTransaction> for RequestedTransaction {
    fn from(transaction: SignedTransaction) -> RequestedTransaction {
        RequestedTransaction {
            payload: transaction.payload,
            signatures: transaction.signatures,
        }
    }
}

impl From<&ValidTransaction> for RequestedTransaction {
    fn from(transaction: &ValidTransaction) -> RequestedTransaction {
        let transaction = transaction.clone();
        RequestedTransaction {
            payload: transaction.payload,
            signatures: transaction.signatures,
        }
    }
}
