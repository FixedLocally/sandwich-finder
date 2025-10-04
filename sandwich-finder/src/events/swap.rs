use std::fmt::Debug;

use derive_getters::Getters;
use serde::Serialize;
use sandwich_finder_derive::HelloMacro;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::{prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta}};

pub trait HelloMacro {
    fn hello_macro(&self);
}

#[derive(Clone, Serialize, Getters, HelloMacro)]
#[serde(rename_all = "camelCase")]
pub struct SwapV2 {
    // The wrapper program for this swap, if any
    outer_program: Option<String>,
    // The actual AMM program
    program: String,
    // The AMM used for this trade
    amm: String,
    // In/out mints of the swap
    input_mint: String,
    output_mint: String,
    // In/out amounts of the swap
    input_amount: u64,
    output_amount: u64,
    // In/out token accounts
    input_ata: String,
    output_ata: String,
    // In/out inner ix indexes
    input_inner_ix_index: Option<u32>,
    output_inner_ix_index: Option<u32>,
    // These fields are meant to be replaced when inserting to the db
    // Tx signature reference
    // Slot that this tx landed
    slot: u64,
    // Order of this tx in the block
    inclusion_order: u32,
    // ix/inner ix index within the tx
    ix_index: u32,
    inner_ix_index: Option<u32>,
}

impl SwapV2 {
    pub fn new(
        outer_program: Option<String>,
        program: String,
        amm: String,
        input_mint: String,
        output_mint: String,
        input_amount: u64,
        output_amount: u64,
        input_ata: String,
        output_ata: String,
        input_inner_ix_index: Option<u32>,
        output_inner_ix_index: Option<u32>,
        slot: u64,
        inclusion_order: u32,
        ix_index: u32,
        inner_ix_index: Option<u32>,
    ) -> Self {
        Self {
            outer_program,
            program,
            amm,
            input_mint,
            output_mint,
            input_amount,
            output_amount,
            input_ata,
            output_ata,
            input_inner_ix_index,
            output_inner_ix_index,
            slot,
            inclusion_order,
            ix_index,
            inner_ix_index,
        }
    }
}

impl Debug for SwapV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // f.debug_struct("SwapV2").field("outer_program", &self.outer_program).field("program", &self.program).field("amm", &self.amm).field("input_mint", &self.input_mint).field("output_mint", &self.output_mint).field("input_amount", &self.input_amount).field("output_amount", &self.output_amount).field("input_ata", &self.input_ata).field("output_ata", &self.output_ata).field("sig_id", &self.sig_id).field("slot", &self.slot).field("inclusion_order", &self.inclusion_order).field("ix_index", &self.ix_index).field("inner_ix_index", &self.inner_ix_index).finish()
        f.write_str("Swap")?;
        f.write_str(&format!(" in slot {} (order {}, ix {}, inner_ix {:?})\n", self.slot, self.inclusion_order, self.ix_index, self.inner_ix_index))?;
        if let Some(outer_program) = &self.outer_program {
            f.write_str(&format!(" via {}\n", outer_program))?;
        }
        f.write_str(&format!(" on {} market {}\n", self.program, self.amm))?;
        f.write_str(&format!(" Route {} -> {}", self.input_mint, self.output_mint))?;
        f.write_str(&format!(" Amounts {} -> {}\n", self.input_amount, self.output_amount))?;
        f.write_str(&format!(" ATAs {} -> {}", self.input_ata, self.output_ata))?;
        Ok(())
    }
}

pub trait SwapFinder {
    /// Returns the swaps utilising a program found in the given instruction and inner instructions.
    /// A swap involves an inner instruction that the user's out ATA sends tokens to the pool's in ATA,
    /// and one that the pool's out ATA sends tokens to the user's in ATA.
    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2>;

    /// Returns the AMM address for the swap instruction. The instruction will have matching program ID, discriminant and enough instruction data.
    fn amm_ix(ix: &Instruction) -> Pubkey;
    /// Like [`SwapFinder::amm_ix`], but takes an inner instruction and the account keys vector for key resolution.
    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey;

    /// Returns the user's in/out ATAs involved in the swap, in that order. The instruction follows the same constraints as above.
    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey);
    /// Like [`SwapFinder::user_ata_ix`], but takes an inner instruction and the account keys vector for key resolution.
    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey);

    /// Returns the pool's in/out ATAs involved in the swap, in that order. The instruction follows the same constraints as above.
    /// Can return [`Pubkey::default()`] to bypass this check.
    fn pool_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        return (
            Pubkey::default(),
            Pubkey::default(),
        );
    }
    /// Like [`SwapFinder::pool_ata_ix`], but takes an inner instruction and the account keys vector for key resolution.
    fn pool_ata_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        return (
            Pubkey::default(),
            Pubkey::default(),
        );
    }

    /// Number of inner instructions to skip before the actual relevant transfers.
    fn ixs_to_skip() -> usize {
        0
    }

    /// The indexes of the accounts that definitely won't be involved in the swap, such as referral/fee accounts.
    fn blacklist_ata_indexs() -> Vec<usize> {
        vec![]
    }
}
