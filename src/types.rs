// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use alloc::boxed::Box;
#[cfg(feature = "elias-fano")]
use alloc::string::String;
use alloc::vec::Vec;
use chain::{ChainPosition, ConfirmationBlockTime};
use core::convert::AsRef;
use core::fmt;

use bitcoin::transaction::{OutPoint, Sequence, TxOut};
use bitcoin::{psbt, Weight};

use serde::{Deserialize, Serialize};

/// Types of keychains
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum KeychainKind {
    /// External keychain, used for deriving recipient addresses.
    External = 0,
    /// Internal keychain, used for deriving change addresses.
    Internal = 1,
}

impl KeychainKind {
    /// Return [`KeychainKind`] as a byte
    pub fn as_byte(&self) -> u8 {
        match self {
            KeychainKind::External => b'e',
            KeychainKind::Internal => b'i',
        }
    }
}

impl fmt::Display for KeychainKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeychainKind::External => write!(f, "External"),
            KeychainKind::Internal => write!(f, "Internal"),
        }
    }
}

impl AsRef<[u8]> for KeychainKind {
    fn as_ref(&self) -> &[u8] {
        match self {
            KeychainKind::External => b"e",
            KeychainKind::Internal => b"i",
        }
    }
}

/// An unspent output owned by a [`Wallet`].
///
/// [`Wallet`]: crate::Wallet
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalOutput {
    /// Reference to a transaction output
    pub outpoint: OutPoint,
    /// Transaction output
    pub txout: TxOut,
    /// Type of keychain
    pub keychain: KeychainKind,
    /// Whether this UTXO is spent or not
    pub is_spent: bool,
    /// The derivation index for the script pubkey in the wallet
    pub derivation_index: u32,
    /// The position of the output in the blockchain.
    pub chain_position: ChainPosition<ConfirmationBlockTime>,
}

/// A [`Utxo`] with its `satisfaction_weight`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedUtxo {
    /// The weight of the witness data and `scriptSig` expressed in [weight units]. This is used to
    /// properly maintain the feerate when adding this input to a transaction during coin
    /// selection.
    ///
    /// [weight units]: https://en.bitcoin.it/wiki/Weight_units
    pub satisfaction_weight: Weight,
    /// The UTXO
    pub utxo: Utxo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An unspent transaction output (UTXO).
pub enum Utxo {
    /// A UTXO owned by the local wallet.
    Local(LocalOutput),
    /// A UTXO owned by another wallet.
    Foreign {
        /// The location of the output.
        outpoint: OutPoint,
        /// The nSequence value to set for this input.
        sequence: Sequence,
        /// The information about the input we require to add it to a PSBT.
        // Box it to stop the type being too big.
        psbt_input: Box<psbt::Input>,
    },
}

impl Utxo {
    /// Get the location of the UTXO
    pub fn outpoint(&self) -> OutPoint {
        match &self {
            Utxo::Local(local) => local.outpoint,
            Utxo::Foreign { outpoint, .. } => *outpoint,
        }
    }

    /// Get the `TxOut` of the UTXO
    pub fn txout(&self) -> &TxOut {
        match &self {
            Utxo::Local(local) => &local.txout,
            Utxo::Foreign {
                outpoint,
                psbt_input,
                ..
            } => psbt_input.witness_utxo.as_ref().unwrap_or_else(|| {
                psbt_input
                    .non_witness_utxo
                    .as_ref()
                    .and_then(|tx| tx.output.get(outpoint.vout as usize))
                    .expect("Foreign UTXOs should have one of witness_utxo, non_witness_utxo set")
            }),
        }
    }

    /// Get the sequence number if an explicit sequence number has to be set for this input.
    pub fn sequence(&self) -> Option<Sequence> {
        match self {
            Utxo::Local(_) => None,
            Utxo::Foreign { sequence, .. } => Some(*sequence),
        }
    }
}

/// Derivation index metadata for a single keychain descriptor.
///
/// Captures the sorted list of used derivation indexes as flat integer data
/// suitable for encoding and export formats.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpkMetadata {
    /// The keychain this metadata belongs to.
    keychain: KeychainKind,
    /// Highest derivation index revealed by the wallet.
    last_revealed: Option<u32>,
    /// Sorted derivation indexes that have been used (have on-chain `TxOut`s).
    used_indexes: Vec<u32>,
}

/// Error returned when descriptor metadata contains inconsistent indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpkMetadataError {
    /// A used index exists outside the revealed descriptor range.
    UsedIndexBeyondLastRevealed {
        /// Highest index marked as used.
        highest_used: u32,
        /// Highest revealed index, or `None` when nothing was revealed.
        last_revealed: Option<u32>,
    },
}

impl fmt::Display for SpkMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UsedIndexBeyondLastRevealed {
                highest_used,
                last_revealed: Some(last_revealed),
            } => write!(
                f,
                "highest used index {highest_used} exceeds last revealed index {last_revealed}"
            ),
            Self::UsedIndexBeyondLastRevealed {
                highest_used,
                last_revealed: None,
            } => write!(
                f,
                "used index {highest_used} exists without a revealed descriptor range"
            ),
        }
    }
}

impl core::error::Error for SpkMetadataError {}

impl SpkMetadata {
    /// Build script pubkey metadata for a keychain.
    ///
    /// The provided indexes are zero-based BDK derivation indexes. They are
    /// normalized by sorting and removing duplicates.
    pub fn new(keychain: KeychainKind, used_indexes: impl Into<Vec<u32>>) -> Self {
        let used_indexes = Self::normalize_used_indexes(used_indexes);
        let last_revealed = used_indexes.last().copied();

        Self {
            keychain,
            last_revealed,
            used_indexes,
        }
    }

    /// Build metadata with an explicit highest revealed derivation index.
    ///
    /// Returns an error when a used index is greater than `last_revealed`, or
    /// when used indexes are provided without a revealed descriptor range.
    pub fn with_last_revealed(
        keychain: KeychainKind,
        last_revealed: Option<u32>,
        used_indexes: impl Into<Vec<u32>>,
    ) -> Result<Self, SpkMetadataError> {
        let used_indexes = Self::normalize_used_indexes(used_indexes);

        if let Some(highest_used) = used_indexes.last().copied() {
            let used_is_revealed = last_revealed
                .map(|last_revealed| highest_used <= last_revealed)
                .unwrap_or(false);

            if !used_is_revealed {
                return Err(SpkMetadataError::UsedIndexBeyondLastRevealed {
                    highest_used,
                    last_revealed,
                });
            }
        }

        Ok(Self {
            keychain,
            last_revealed,
            used_indexes,
        })
    }

    /// Return the keychain this metadata belongs to.
    pub fn keychain(&self) -> KeychainKind {
        self.keychain
    }

    /// Return the highest revealed derivation index.
    pub fn last_revealed(&self) -> Option<u32> {
        self.last_revealed
    }

    /// Return the sorted zero-based derivation indexes with known wallet activity.
    pub fn used_indexes(&self) -> &[u32] {
        &self.used_indexes
    }

    /// Return whether this metadata contains no revealed or used indexes.
    pub fn is_empty(&self) -> bool {
        self.last_revealed.is_none() && self.used_indexes.is_empty()
    }

    /// Consume this metadata and return its individual parts.
    pub fn into_parts(self) -> (KeychainKind, Option<u32>, Vec<u32>) {
        (self.keychain, self.last_revealed, self.used_indexes)
    }

    /// Consume this metadata and return the used indexes.
    pub fn into_used_indexes(self) -> Vec<u32> {
        self.used_indexes
    }

    fn normalize_used_indexes(used_indexes: impl Into<Vec<u32>>) -> Vec<u32> {
        let mut used_indexes = used_indexes.into();
        used_indexes.sort_unstable();
        used_indexes.dedup();
        used_indexes
    }

    /// Build [`SpkMetadata`] from a [`KeychainTxOutIndex`] for the given keychain.
    ///
    /// The metadata captures both the highest revealed derivation index and the
    /// normalized indexes with known wallet outputs.
    ///
    /// [`KeychainTxOutIndex`]: chain::indexer::keychain_txout::KeychainTxOutIndex
    pub fn from_index(
        index: &chain::indexer::keychain_txout::KeychainTxOutIndex<KeychainKind>,
        keychain: KeychainKind,
    ) -> Self {
        let used_indexes = Self::normalize_used_indexes(
            index
                .keychain_outpoints(keychain)
                .map(|(index, _)| index)
                .collect::<Vec<_>>(),
        );

        let last_revealed = index
            .last_revealed_index(keychain)
            .max(used_indexes.last().copied());

        Self {
            keychain,
            last_revealed,
            used_indexes,
        }
    }
}

#[cfg(feature = "elias-fano")]
const SPK_METADATA_BECH32_HRP: &str = "spkmeta";

#[cfg(feature = "elias-fano")]
#[derive(Serialize, Deserialize)]
struct SpkMetadataPayload {
    last_revealed: Option<u32>,
    used_indexes: Option<sux::prelude::EliasFano>,
}

#[cfg(feature = "elias-fano")]
impl SpkMetadata {
    /// Encode `used_indexes` as an Elias-Fano representation.
    ///
    /// Returns `None` if there are no used indexes.
    pub fn encode_elias_fano(&self) -> Option<sux::prelude::EliasFano> {
        use sux::prelude::EliasFanoBuilder;

        let n = self.used_indexes.len();
        if n == 0 {
            return None;
        }

        let upper_bound = self
            .used_indexes
            .last()
            .copied()
            .expect("metadata is not empty") as usize
            + 1;

        let mut builder = EliasFanoBuilder::new(n, upper_bound);

        for &index in &self.used_indexes {
            builder.push(index as usize);
        }

        Some(builder.build())
    }

    /// Encode this metadata as a base64 transport string.
    ///
    /// Returns `Ok(None)` only when the metadata contains neither revealed nor
    /// used indexes.
    pub fn encode_base64(&self) -> Result<Option<String>, SpkMetadataEncodingError> {
        use bitcoin::base64::prelude::{Engine as _, BASE64_STANDARD};

        let payload = match self.encode_payload()? {
            Some(payload) => payload,
            None => return Ok(None),
        };

        Ok(Some(BASE64_STANDARD.encode(payload)))
    }

    /// Decode base64 metadata for the given keychain.
    ///
    /// Returns an error if the input is not valid base64 or does not contain a
    /// supported metadata payload.
    pub fn decode_base64(
        encoded: &str,
        keychain: KeychainKind,
    ) -> Result<Self, SpkMetadataEncodingError> {
        use bitcoin::base64::prelude::{Engine as _, BASE64_STANDARD};

        let payload = BASE64_STANDARD
            .decode(encoded)
            .map_err(SpkMetadataEncodingError::Base64)?;

        Self::decode_payload(&payload, keychain)
    }

    /// Encode this metadata as a bech32 transport string.
    ///
    /// The encoded string uses the `spkmeta` human-readable part. Returns
    /// `Ok(None)` only when the metadata contains neither revealed nor used
    /// indexes.
    pub fn encode_bech32(&self) -> Result<Option<String>, SpkMetadataEncodingError> {
        use bitcoin::bech32::{self, Bech32, Hrp};

        let payload = match self.encode_payload()? {
            Some(payload) => payload,
            None => return Ok(None),
        };

        let hrp = Hrp::parse_unchecked(SPK_METADATA_BECH32_HRP);

        bech32::encode::<Bech32>(hrp, &payload)
            .map(Some)
            .map_err(SpkMetadataEncodingError::Bech32Encode)
    }

    /// Decode bech32 metadata for the given keychain.
    ///
    /// Returns an error if the input is not valid bech32, if the
    /// human-readable part is not `spkmeta`, or if the payload version is not
    /// supported.
    pub fn decode_bech32(
        encoded: &str,
        keychain: KeychainKind,
    ) -> Result<Self, SpkMetadataEncodingError> {
        let (hrp, payload) =
            bitcoin::bech32::decode(encoded).map_err(SpkMetadataEncodingError::Bech32Decode)?;

        if hrp.as_str() != SPK_METADATA_BECH32_HRP {
            return Err(SpkMetadataEncodingError::InvalidBech32Hrp {
                expected: SPK_METADATA_BECH32_HRP,
                actual: String::from(hrp.as_str()),
            });
        }

        Self::decode_payload(&payload, keychain)
    }

    fn encode_payload(&self) -> Result<Option<Vec<u8>>, SpkMetadataEncodingError> {
        if self.is_empty() {
            return Ok(None);
        }

        let payload = SpkMetadataPayload {
            last_revealed: self.last_revealed,
            used_indexes: self.encode_elias_fano(),
        };

        serde_json::to_vec(&payload)
            .map(Some)
            .map_err(SpkMetadataEncodingError::Json)
    }

    fn decode_payload(
        encoded: &[u8],
        keychain: KeychainKind,
    ) -> Result<Self, SpkMetadataEncodingError> {
        let payload: SpkMetadataPayload =
            serde_json::from_slice(encoded).map_err(SpkMetadataEncodingError::Json)?;
        let used_indexes = match payload.used_indexes {
            Some(elias_fano) => Self::decode_used_indexes(elias_fano)?,
            None => Vec::new(),
        };

        Self::with_last_revealed(keychain, payload.last_revealed, used_indexes)
            .map_err(SpkMetadataEncodingError::InvalidMetadata)
    }

    fn decode_used_indexes(
        elias_fano: sux::prelude::EliasFano,
    ) -> Result<Vec<u32>, SpkMetadataEncodingError> {
        elias_fano
            .into_iter()
            .map(|index| {
                u32::try_from(index).map_err(|_| SpkMetadataEncodingError::IndexOverflow(index))
            })
            .collect()
    }
}

/// Error returned when encoding or decoding [`SpkMetadata`] transport formats.
#[cfg(feature = "elias-fano")]
#[derive(Debug)]
pub enum SpkMetadataEncodingError {
    /// Failed to decode a base64 transport string.
    Base64(bitcoin::base64::DecodeError),
    /// Failed to serialize or deserialize the metadata JSON payload.
    Json(serde_json::Error),
    /// The decoded metadata contains inconsistent indexes.
    InvalidMetadata(SpkMetadataError),
    /// Failed to encode a bech32 transport string.
    Bech32Encode(bitcoin::bech32::EncodeError),
    /// Failed to decode a bech32 transport string.
    Bech32Decode(bitcoin::bech32::DecodeError),
    /// Bech32 human-readable part does not match the expected metadata HRP.
    InvalidBech32Hrp {
        /// Expected bech32 human-readable part.
        expected: &'static str,
        /// Actual bech32 human-readable part found in the encoded string.
        actual: String,
    },
    /// Decoded index does not fit in a BDK `u32` derivation index.
    IndexOverflow(usize),
}

#[cfg(feature = "elias-fano")]
impl fmt::Display for SpkMetadataEncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base64(err) => write!(f, "base64 decoding error: {err}"),
            Self::Json(err) => write!(f, "metadata JSON payload error: {err}"),
            Self::InvalidMetadata(err) => write!(f, "invalid metadata payload: {err}"),
            Self::Bech32Encode(err) => write!(f, "bech32 encoding error: {err}"),
            Self::Bech32Decode(err) => write!(f, "bech32 decoding error: {err}"),
            Self::InvalidBech32Hrp { expected, actual } => {
                write!(f, "invalid bech32 HRP: expected {expected}, got {actual}")
            }
            Self::IndexOverflow(idx) => {
                write!(f, "decoded derivation index {idx} does not fit in u32")
            }
        }
    }
}

#[cfg(feature = "elias-fano")]
impl core::error::Error for SpkMetadataEncodingError {}

/// Index out of bounds error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOutOfBoundsError {
    /// The index that is out of range.
    pub index: usize,
    /// The length of the container.
    pub len: usize,
}

impl IndexOutOfBoundsError {
    /// Create a new `IndexOutOfBoundsError`.
    pub fn new(index: usize, len: usize) -> Self {
        Self { index, len }
    }
}

impl fmt::Display for IndexOutOfBoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Index out of bounds: index {} is greater than or equal to length {}",
            self.index, self.len
        )
    }
}

impl core::error::Error for IndexOutOfBoundsError {}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use bitcoin::{
        absolute, transaction, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
        Witness,
    };

    fn build_tx(txout: TxOut) -> Transaction {
        Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::default(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![txout],
        }
    }

    #[test]
    fn test_spk_metadata_construction() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 1, 3]);
        assert_eq!(meta.keychain(), KeychainKind::External);
        assert_eq!(meta.last_revealed(), Some(3));
        assert_eq!(meta.used_indexes(), &[0, 1, 3]);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_elias_fano_round_trip() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 2, 5, 7]);

        // Encode to EliasFano and verify values
        let ef = meta
            .encode_elias_fano()
            .expect("non-empty metadata should encode");
        let decoded: Vec<usize> = ef.into_iter().collect();
        assert_eq!(decoded, vec![0, 2, 5, 7]);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_base64_round_trip() {
        let metadata =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(100), vec![0, 2, 5, 7])
                .expect("metadata should be valid");

        let encoded = metadata
            .encode_base64()
            .expect("metadata encoding should succeed")
            .expect("non-empty metadata should produce base64");

        let decoded = SpkMetadata::decode_base64(&encoded, KeychainKind::External)
            .expect("encoded metadata should decode");

        assert_eq!(decoded, metadata);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_bech32_round_trip() {
        let metadata =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(100), vec![0, 20, 50])
                .expect("metadata should be valid");

        let encoded = metadata
            .encode_bech32()
            .expect("metadata encoding should succeed")
            .expect("non-empty metadata should produce bech32");

        let decoded = SpkMetadata::decode_bech32(&encoded, KeychainKind::External)
            .expect("encoded metadata should decode");

        assert_eq!(decoded, metadata);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_base64_round_trip_revealed_but_unused() {
        let metadata =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(100), Vec::new())
                .expect("revealed metadata without used indexes should be valid");

        let encoded = metadata
            .encode_base64()
            .expect("metadata encoding should succeed")
            .expect("revealed metadata should produce base64");

        let decoded = SpkMetadata::decode_base64(&encoded, KeychainKind::External)
            .expect("encoded metadata should decode");

        assert_eq!(decoded, metadata);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_bech32_round_trip_revealed_but_unused() {
        let metadata =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(100), Vec::new())
                .expect("revealed metadata without used indexes should be valid");

        let encoded = metadata
            .encode_bech32()
            .expect("metadata encoding should succeed")
            .expect("revealed metadata should produce bech32");

        let decoded = SpkMetadata::decode_bech32(&encoded, KeychainKind::External)
            .expect("encoded metadata should decode");

        assert_eq!(decoded, metadata);
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_bech32_empty() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![]);

        assert_eq!(
            meta.encode_bech32()
                .expect("empty metadata encoding should succeed"),
            None
        );
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_decode_bech32_rejects_invalid_input() {
        let err = SpkMetadata::decode_bech32("not bech32!", KeychainKind::External)
            .expect_err("invalid bech32 should fail");

        assert!(matches!(err, SpkMetadataEncodingError::Bech32Decode(_)));
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_decode_bech32_rejects_invalid_hrp() {
        use bitcoin::bech32::{self, Bech32, Hrp};

        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 20, 50]);

        let encoded = meta
            .encode_bech32()
            .expect("metadata encoding should succeed")
            .expect("non-empty metadata should produce bech32");

        let (_, payload) =
            bech32::decode(&encoded).expect("encoded metadata should be valid bech32");
        let wrong_hrp = Hrp::parse_unchecked("badmeta");
        let wrong_hrp_encoded =
            bech32::encode::<Bech32>(wrong_hrp, &payload).expect("re-encoding should succeed");

        let err = SpkMetadata::decode_bech32(&wrong_hrp_encoded, KeychainKind::External)
            .expect_err("wrong HRP should fail");

        assert!(matches!(
            err,
            SpkMetadataEncodingError::InvalidBech32Hrp { .. }
        ));
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_decode_base64_rejects_invalid_input() {
        let err = SpkMetadata::decode_base64("not base64!", KeychainKind::External)
            .expect_err("invalid base64 should fail");

        assert!(matches!(err, SpkMetadataEncodingError::Base64(_)));
    }

    #[test]
    #[cfg(feature = "elias-fano")]
    fn test_spk_metadata_elias_fano_empty() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![]);
        assert!(meta.encode_elias_fano().is_none());
        assert_eq!(
            meta.encode_base64()
                .expect("empty metadata encoding should succeed"),
            None
        );
    }

    #[test]
    fn txout_foreign_returns_witness_utxo() {
        let txout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::default(),
        };
        let utxo = Utxo::Foreign {
            outpoint: OutPoint::null(),
            sequence: Sequence::MAX,
            psbt_input: Box::new(psbt::Input {
                witness_utxo: Some(txout.clone()),
                ..Default::default()
            }),
        };
        assert_eq!(utxo.txout(), &txout);
    }

    #[test]
    fn txout_foreign_returns_non_witness_utxo() {
        let txout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::default(),
        };
        let prev_tx = build_tx(txout.clone());
        let utxo = Utxo::Foreign {
            outpoint: OutPoint {
                txid: prev_tx.compute_txid(),
                vout: 0,
            },
            sequence: Sequence::MAX,
            psbt_input: Box::new(psbt::Input {
                non_witness_utxo: Some(prev_tx),
                ..Default::default()
            }),
        };
        assert_eq!(utxo.txout(), &txout);
    }

    #[test]
    #[should_panic(
        expected = "Foreign UTXOs should have one of witness_utxo, non_witness_utxo set"
    )]
    fn txout_foreign_panics_with_empty_psbt_input() {
        let utxo = Utxo::Foreign {
            outpoint: OutPoint::null(),
            sequence: Sequence::MAX,
            psbt_input: Box::new(psbt::Input::default()),
        };
        utxo.txout();
    }

    #[test]
    fn test_spk_metadata_new_normalizes_used_indexes() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![50, 0, 20, 20]);

        assert_eq!(meta.keychain(), KeychainKind::External);
        assert_eq!(meta.used_indexes(), &[0, 20, 50]);
    }

    #[test]
    fn test_spk_metadata_preserves_zero_based_indexes() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![0, 1, 3]);
        assert_eq!(meta.used_indexes(), &[0, 1, 3]);
    }

    #[test]
    fn test_spk_metadata_empty() {
        let meta = SpkMetadata::new(KeychainKind::Internal, Vec::new());

        assert_eq!(meta.keychain(), KeychainKind::Internal);
        assert_eq!(meta.last_revealed(), None);
        assert!(meta.is_empty());
    }

    #[test]
    fn test_spk_metadata_into_used_indexes() {
        let meta = SpkMetadata::new(KeychainKind::External, vec![3, 1, 1]);

        assert_eq!(meta.into_used_indexes(), vec![1, 3]);
    }

    #[test]
    fn test_spk_metadata_preserves_last_revealed() {
        let metadata =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(100), vec![1, 20, 50])
                .expect("metadata should be valid");

        assert_eq!(metadata.last_revealed(), Some(100));
        assert_eq!(metadata.used_indexes(), &[1, 20, 50]);
    }

    #[test]
    fn test_spk_metadata_supports_revealed_but_unused() {
        let metadata =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(100), Vec::new())
                .expect("revealed metadata without used indexes should be valid");

        assert_eq!(metadata.last_revealed(), Some(100));
        assert!(metadata.used_indexes().is_empty());
        assert!(!metadata.is_empty());
    }

    #[test]
    fn test_spk_metadata_rejects_used_index_beyond_last_revealed() {
        let error =
            SpkMetadata::with_last_revealed(KeychainKind::External, Some(20), vec![1, 20, 50])
                .expect_err("index 50 is beyond last revealed index 20");

        assert_eq!(
            error,
            SpkMetadataError::UsedIndexBeyondLastRevealed {
                highest_used: 50,
                last_revealed: Some(20),
            }
        );
    }

    #[test]
    fn test_spk_metadata_rejects_used_index_without_revealed_state() {
        let error = SpkMetadata::with_last_revealed(KeychainKind::External, None, vec![1])
            .expect_err("used indexes require a revealed range");

        assert_eq!(
            error,
            SpkMetadataError::UsedIndexBeyondLastRevealed {
                highest_used: 1,
                last_revealed: None,
            }
        );
    }

    #[test]
    fn test_spk_metadata_new_infers_last_revealed_from_used_indexes() {
        let metadata = SpkMetadata::new(KeychainKind::External, vec![1, 20, 50]);

        assert_eq!(metadata.last_revealed(), Some(50));
    }
}
