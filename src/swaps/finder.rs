use derive_getters::Getters;
use serde::Serialize;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::{geyser::SubscribeUpdateTransactionInfo, prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta}};

use crate::swaps::{utils::token_transferred_inner, private};


#[derive(Clone, Serialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct TransactionV2 {

}

#[derive(Clone, Debug, Serialize, Getters)]
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
    // These fields are meant to be replaced when inserting to the db
    // Tx signature reference
    sig_id: u64,
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
        sig_id: u64,
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
            sig_id,
            slot,
            inclusion_order,
            ix_index,
            inner_ix_index,
        }
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
}

/// This trait contains helper methods not meant to be overridden by the implementors of [`SwapFinder`].
pub trait SwapFinderExt: private::Sealed {
    /// Finds swaps in this ix utilising the provided program id, determining trade directions automatically.
    /// Relies on the correctness of in/out ATAs and will not return any swaps if the order is wrong.
    fn find_swaps_generic(
        ix: &Instruction,
        inner_ixs: &InnerInstructions,
        account_keys: &Vec<Pubkey>,
        meta: &TransactionStatusMeta,
        program_id: &Pubkey,
        discriminant: &[u8],
        data_length: usize,
    ) -> Vec<SwapV2>;

    /// Finds swaps in this tx utilising the provided program id by iterating through the ixs.
    fn find_swaps_in_tx(slot: u64, raw_tx: &SubscribeUpdateTransactionInfo, ixs: &Vec<Instruction>, account_keys: &Vec<Pubkey>) -> Vec<SwapV2>;
}

impl<T: SwapFinder + private::Sealed> SwapFinderExt for T {
    fn find_swaps_generic(
        ix: &Instruction,
        inner_ixs: &InnerInstructions,
        account_keys: &Vec<Pubkey>,
        meta: &TransactionStatusMeta,
        program_id: &Pubkey,
        discriminant: &[u8],
        data_length: usize,
    ) -> Vec<SwapV2> {
        if ix.program_id == *program_id {
            // data size check
            if data_length < discriminant.len() || ix.data.len() < data_length {
                return vec![];
            }
            // discriminant check
            if ix.data[0..discriminant.len()] != discriminant[..] {
                return vec![];
            }
            let mut input_amount = 0;
            let mut output_amount = 0;
            let mut input_mint = None;
            let mut output_mint = None;
            let (input_ata, output_ata) = Self::user_ata_ix(ix);
            let (pool_input_ata, pool_output_ata) = Self::pool_ata_ix(ix);
            println!("{input_ata} -> {pool_output_ata}, {pool_input_ata} -> {output_ata}");
            inner_ixs.instructions.iter().for_each(|inner_ix| {
                if let Some((from, to, mint, amount)) = token_transferred_inner(&inner_ix, &account_keys, &meta) {
                    if from == input_ata && (to == pool_output_ata || pool_output_ata == Pubkey::default()) {
                        input_mint = Some(mint);
                        input_amount = amount;
                    } else if to == output_ata && (from == pool_input_ata || pool_input_ata == Pubkey::default()) {
                        output_mint = Some(mint);
                        output_amount = amount;
                    }
                }
            });
            return vec![
                SwapV2::new(
                    None,
                    ix.program_id.to_string(),
                    Self::amm_ix(ix).to_string(),
                    input_mint.unwrap_or_default(),
                    output_mint.unwrap_or_default(),
                    input_amount,
                    output_amount,
                    input_ata.to_string(),
                    output_ata.to_string(),
                    0,
                    0,
                    0,
                    0,
                    None,
                )
            ];
        }
        let mut swaps = vec![];
        let mut next_logical_ix = 0;
        inner_ixs.instructions.iter().enumerate().for_each(|(i, inner_ix)| {
            if i < next_logical_ix {
                return; // Skip already processed instructions
            }
            if inner_ix.program_id_index >= account_keys.len() as u32 {
                return;
            }
            // program id check
            if account_keys[inner_ix.program_id_index as usize] != *program_id {
                return;
            }
            // data size & discriminant check
            if inner_ix.data.len() < data_length || inner_ix.data[0..discriminant.len()] != discriminant[..] {
                return;
            }

            let mut input_amount = 0;
            let mut output_amount = 0;
            let mut input_mint = None;
            let mut output_mint = None;
            let (input_ata, output_ata) = Self::user_ata_inner_ix(inner_ix, account_keys);
            let (pool_input_ata, pool_output_ata) = Self::pool_ata_inner_ix(inner_ix, account_keys);
            for j in i..inner_ixs.instructions.len() {
                let next_inner_ix = &inner_ixs.instructions[j];
                if next_inner_ix.program_id_index >= account_keys.len() as u32 {
                    continue;
                }
                if let Some((from, to, mint, amount)) = token_transferred_inner(&next_inner_ix, &account_keys, &meta) {
                    if from == input_ata && (to == pool_input_ata || pool_input_ata == Pubkey::default()) {
                        input_mint = Some(mint);
                        input_amount = amount;
                    } else if to == output_ata && (from == pool_output_ata || pool_output_ata == Pubkey::default()) {
                        output_mint = Some(mint);
                        output_amount = amount;
                    }
                }
                if input_mint.is_some() && output_mint.is_some() {
                    // Found both input and output mints
                    swaps.push(SwapV2::new(
                        Some(ix.program_id.to_string()),
                        program_id.to_string(),
                        Self::amm_inner_ix(inner_ix, account_keys).to_string(),
                        input_mint.clone().unwrap_or_default(),
                        output_mint.clone().unwrap_or_default(),
                        input_amount,
                        output_amount,
                        input_ata.to_string(),
                        output_ata.to_string(),
                        0,
                        0,
                        0,
                        0,
                        Some(i as u32),
                    ));
                    next_logical_ix = j + 1;
                    return;
                }
            }
        });
        swaps
    }

    fn find_swaps_in_tx(slot: u64, raw_tx: &SubscribeUpdateTransactionInfo, ixs: &Vec<Instruction>, account_keys: &Vec<Pubkey>) -> Vec<SwapV2> {
        if let Some(meta) = &raw_tx.meta {
            let mut swaps = vec![];
            ixs.iter().enumerate().for_each(|(i, ix)| {
                let inner_ixs = meta.inner_instructions.iter().find(|x| x.index == i as u32);
                if let Some(inner_ixs) = inner_ixs {
                    Self::find_swaps(ix, inner_ixs, account_keys, meta).iter().for_each(|swap| {
                        let mut swap = swap.clone();
                        swap.slot = slot;
                        swap.inclusion_order = raw_tx.index as u32;
                        swap.ix_index = i as u32;
                        swaps.push(swap);
                    });
                }
            });
            return swaps;
        }
        vec![]
    }
}
