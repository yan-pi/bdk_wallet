use std::collections::BTreeMap;

use bdk_chain::{
    bitcoin::{hashes::Hash, Amount, OutPoint, ScriptBuf, TxOut, Txid},
    miniscript::{Descriptor, DescriptorPublicKey},
};

use crate::fixture::RecoveryCase;

#[derive(Debug)]
pub struct SyntheticActivity {
    pub index: u32,
    pub outpoint: OutPoint,
    pub txout: TxOut,
}

pub struct SyntheticHistory {
    by_script: BTreeMap<ScriptBuf, SyntheticActivity>,
}

impl SyntheticHistory {
    pub fn new(case: &RecoveryCase, descriptor: &Descriptor<DescriptorPublicKey>) -> Self {
        let mut by_script = BTreeMap::new();

        for &index in case.metadata.used_indexes() {
            let script = descriptor
                .at_derivation_index(index)
                .expect("fixture index must derive")
                .script_pubkey();

            let mut txid_bytes = [0u8; 32];
            txid_bytes[..4].copy_from_slice(&index.to_le_bytes());

            let activity = SyntheticActivity {
                index,
                outpoint: OutPoint::new(Txid::from_byte_array(txid_bytes), 0),
                txout: TxOut {
                    value: Amount::from_sat(5_000),
                    script_pubkey: script.clone(),
                },
            };

            assert!(
                by_script.insert(script, activity).is_none(),
                "fixture scripts must be unique",
            );
        }

        Self { by_script }
    }

    pub fn activity(&self, script: &ScriptBuf) -> Option<&SyntheticActivity> {
        self.by_script.get(script)
    }

    pub fn used_count(&self) -> usize {
        self.by_script.len()
    }
}
