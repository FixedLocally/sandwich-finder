use std::fmt::Debug;

use derive_getters::Getters;
use serde::Serialize;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::{prelude::{InnerInstructions, TransactionStatusMeta}};

use crate::events::common::Timestamp;

#[derive(Clone, Serialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct TransferV2 {
    // The wrapper program for this transfer, if any
    outer_program: Option<String>,
    // The actual token/system program
    program: String,
    // Wallet that authorised the transfer
    authority: String,
    // Mint of the token transferred
    mint: String,
    // Amounts of the transfer
    amount: u64,
    // In/out token accounts
    input_ata: String,
    output_ata: String,
    // These fields are meant to be replaced when inserting to the db
    timestamp: Timestamp,
}

impl Debug for TransferV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // f.debug_struct("SwapV2").field("outer_program", &self.outer_program).field("program", &self.program).field("amm", &self.amm).field("input_mint", &self.input_mint).field("output_mint", &self.output_mint).field("input_amount", &self.input_amount).field("output_amount", &self.output_amount).field("input_ata", &self.input_ata).field("output_ata", &self.output_ata).field("sig_id", &self.sig_id).field("slot", &self.slot).field("inclusion_order", &self.inclusion_order).field("ix_index", &self.ix_index).field("inner_ix_index", &self.inner_ix_index).finish()
        f.write_str("Transfer")?;
        f.write_str(&format!(" in slot {} (order {}, ix {}, inner_ix {:?})\n", self.slot(), self.inclusion_order(), self.ix_index(), self.inner_ix_index()))?;
        if let Some(outer_program) = &self.outer_program {
            f.write_str(&format!(" via {}\n", outer_program))?;
        }
        f.write_str(&format!(" on {} mint {}\n", self.program, self.mint))?;
        f.write_str(&format!(" Amount {}\n", self.amount))?;
        f.write_str(&format!(" ATAs {} -> {}", self.input_ata, self.output_ata))?;
        Ok(())
    }
}

impl TransferV2 {
    pub fn new(
        outer_program: Option<String>,
        program: String,
        authority: String,
        mint: String,
        amount: u64,
        input_ata: String,
        output_ata: String,
        slot: u64,
        inclusion_order: u32,
        ix_index: u32,
        inner_ix_index: Option<u32>,
    ) -> Self {
        Self {
            outer_program,
            program,
            authority,
            mint,
            amount,
            input_ata,
            output_ata,
            timestamp: Timestamp::new(
                slot,
                inclusion_order,
                ix_index,
                inner_ix_index,
            ),
        }
    }

    pub fn slot(&self) -> &u64 {
        self.timestamp.slot()
    }
    pub fn inclusion_order(&self) -> &u32 {
        self.timestamp.inclusion_order()
    }
    pub fn ix_index(&self) -> &u32 {
        self.timestamp.ix_index()
    }
    pub fn inner_ix_index(&self) -> &Option<u32> {
        self.timestamp.inner_ix_index()
    }
}

pub trait TransferFinder {
    /// Returns the transfers utilising a program found in the given instruction and inner instructions.
    fn find_transfers(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<TransferV2>;
}