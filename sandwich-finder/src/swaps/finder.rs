use std::fmt::Debug;

use debug_print::debug_println;
use derive_getters::Getters;
use serde::Serialize;
use sandwich_finder_derive::HelloMacro;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::{geyser::SubscribeUpdateTransactionInfo, prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta}};

use crate::swaps::{utils::token_transferred_inner, private};

pub trait HelloMacro {
    fn hello_macro(&self);
}

#[derive(Clone, Serialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct TransactionV2 {

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
        discriminant_offset: usize,
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
        discriminant_offset: usize,
        data_length: usize,
    ) -> Vec<SwapV2> {
        debug_println!("looking for swaps in ix #{} with program id {} and discriminant {:?}", inner_ixs.index, program_id, discriminant);
        let ixs_to_skip = Self::ixs_to_skip();
        let blacklist_ata_indexes = Self::blacklist_ata_indexs();
        if inner_ixs.instructions.len() <= ixs_to_skip {
            debug_println!("too few inner ixs");
            return vec![];
        }
        if ix.program_id == *program_id {
            // data size check
            if data_length < discriminant_offset + discriminant.len() || ix.data.len() < data_length {
                debug_println!("too little data");
                return vec![];
            }
            // discriminant check
            if ix.data[discriminant_offset..discriminant_offset + discriminant.len()] != discriminant[..] {
                debug_println!("wrong discriminant");
                return vec![];
            }
            let mut input_amount = 0;
            let mut output_amount = 0;
            let mut input_mint = None;
            let mut output_mint = None;
            let (input_ata, output_ata) = Self::user_ata_ix(ix);
            let (pool_input_ata, pool_output_ata) = Self::pool_ata_ix(ix);
            let blacklist_atas: Vec<Pubkey> = blacklist_ata_indexes.iter().filter_map(|&i| ix.accounts.get(i).map(|acc| acc.pubkey)).collect();
            println!("{} -> {} {} -> {}", input_ata, pool_output_ata, pool_input_ata, output_ata);
            inner_ixs.instructions.iter().skip(ixs_to_skip).for_each(|inner_ix| {
                if let Some((from, to, mint, amount)) = token_transferred_inner(&inner_ix, &account_keys, &meta) {
                    println!("token transferred: {} -> {} (mint: {}, amount: {})", from, to, mint, amount);
                    if blacklist_atas.contains(&from) || blacklist_atas.contains(&to) {
                        return; // Skip blacklisted ATAs
                    }
                    if from == input_ata && (to == pool_output_ata || pool_output_ata == Pubkey::default()) {
                        input_mint = Some(mint);
                        input_amount = amount;
                    } else if to == output_ata && (from == pool_input_ata || pool_input_ata == Pubkey::default()) {
                        output_mint = Some(mint);
                        output_amount = amount;
                    }
                }
            });
            // Sometimes the output tx may not exist due to tiny input that rounds the output to 0.
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
                debug_println!("inner: skipping inner ix {} due to already processed", i);
                return; // Skip already processed instructions
            }
            if inner_ix.program_id_index >= account_keys.len() as u32 {
                debug_println!("inner: too few accounts");
                return;
            }
            // program id check
            if account_keys[inner_ix.program_id_index as usize] != *program_id {
                debug_println!("inner: wrong program id");
                return;
            }
            // data size & discriminant check
            if inner_ix.data.len() < data_length || inner_ix.data[discriminant_offset..discriminant_offset + discriminant.len()] != discriminant[..] {
                debug_println!("inner: too few data/wrong discriminant {:?}/{:?}", inner_ix.data, discriminant);
                return;
            }

            let mut input_amount = 0;
            let mut output_amount = 0;
            let mut input_mint = None;
            let mut output_mint = None;
            let (input_ata, output_ata) = Self::user_ata_inner_ix(inner_ix, account_keys);
            let (pool_input_ata, pool_output_ata) = Self::pool_ata_inner_ix(inner_ix, account_keys);
            debug_println!("{} -> {} (pool: {} -> {})", input_ata, output_ata, pool_input_ata, pool_output_ata);
            for j in i + ixs_to_skip..inner_ixs.instructions.len() {
                let next_inner_ix = &inner_ixs.instructions[j];
                if next_inner_ix.program_id_index >= account_keys.len() as u32 {
                    continue;
                }
                if let Some((from, to, mint, amount)) = token_transferred_inner(&next_inner_ix, &account_keys, &meta) {
                    let blacklist_atas: Vec<Pubkey> = blacklist_ata_indexes.iter().filter_map(|&i| next_inner_ix.accounts.get(i).map(|acc| account_keys[*acc as usize])).collect();
                    if blacklist_atas.contains(&from) || blacklist_atas.contains(&to) {
                        continue; // Skip blacklisted ATAs
                    }
                    if from == input_ata && (to == pool_output_ata || pool_output_ata == Pubkey::default()) {
                        input_mint = Some(mint);
                        input_amount = amount;
                    } else if to == output_ata && (from == pool_input_ata || pool_input_ata == Pubkey::default()) {
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
