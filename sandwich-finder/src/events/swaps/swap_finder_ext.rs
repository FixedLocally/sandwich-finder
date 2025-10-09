use std::sync::Arc;

use debug_print::debug_println;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::{geyser::SubscribeUpdateTransactionInfo, prelude::{InnerInstructions, TransactionStatusMeta}};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{private, utils::token_transferred_inner}};


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
            let mut input_index = None;
            let mut output_index = None;
            let mut authority = "".to_string();
            let (input_ata, output_ata) = Self::user_ata_ix(ix);
            let (pool_input_ata, pool_output_ata) = Self::pool_ata_ix(ix);
            let blacklist_atas: Vec<Pubkey> = blacklist_ata_indexes.iter().filter_map(|&i| ix.accounts.get(i).map(|acc| acc.pubkey)).collect();
            debug_println!("{} -> {} {} -> {}", input_ata, pool_output_ata, pool_input_ata, output_ata);
            inner_ixs.instructions.iter().skip(ixs_to_skip).enumerate().for_each(|(i, inner_ix)| {
                if let Some((from, to, auth, mint, amount)) = token_transferred_inner(&inner_ix, &account_keys, &meta) {
                    debug_println!("token transferred: {} -> {} (mint: {}, amount: {})", from, to, mint, amount);
                    if blacklist_atas.contains(&from) || blacklist_atas.contains(&to) {
                        return; // Skip blacklisted ATAs
                    }
                    if from == input_ata && (to == pool_output_ata || pool_output_ata == Pubkey::default()) {
                        input_mint = Some(mint);
                        input_amount = amount;
                        input_index = Some(i as u32 + ixs_to_skip as u32);
                        authority = auth.to_string();
                    } else if to == output_ata && (from == pool_input_ata || pool_input_ata == Pubkey::default()) {
                        output_mint = Some(mint);
                        output_amount = amount;
                        output_index = Some(i as u32 + ixs_to_skip as u32);
                    }
                }
            });
            // Sometimes the output tx may not exist due to tiny input that rounds the output to 0.
            return vec![
                SwapV2::new(
                    None,
                    ix.program_id.to_string().into(),
                    authority.into(),
                    Self::amm_ix(ix).to_string().into(),
                    input_mint.unwrap_or_default().into(),
                    output_mint.unwrap_or_default().into(),
                    input_amount,
                    output_amount,
                    input_ata.to_string().into(),
                    output_ata.to_string().into(),
                    input_index,
                    output_index,
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
            let mut input_index = None;
            let mut output_index = None;
            let mut authority: Arc<str> = "".into();
            let (input_ata, output_ata) = Self::user_ata_inner_ix(inner_ix, account_keys);
            let (pool_input_ata, pool_output_ata) = Self::pool_ata_inner_ix(inner_ix, account_keys);
            debug_println!("{} -> {} (pool: {} -> {})", input_ata, output_ata, pool_input_ata, pool_output_ata);
            for j in i + ixs_to_skip..inner_ixs.instructions.len() {
                let next_inner_ix = &inner_ixs.instructions[j];
                if next_inner_ix.program_id_index >= account_keys.len() as u32 {
                    continue;
                }
                if let Some((from, to, auth, mint, amount)) = token_transferred_inner(&next_inner_ix, &account_keys, &meta) {
                    let blacklist_atas: Vec<Pubkey> = blacklist_ata_indexes.iter().filter_map(|&i| next_inner_ix.accounts.get(i).map(|acc| account_keys[*acc as usize])).collect();
                    if blacklist_atas.contains(&from) || blacklist_atas.contains(&to) {
                        continue; // Skip blacklisted ATAs
                    }
                    if from == input_ata && (to == pool_output_ata || pool_output_ata == Pubkey::default()) {
                        input_mint = Some(mint);
                        input_amount = amount;
                        input_index = Some(j as u32);
                        authority = auth.to_string().into();
                    } else if to == output_ata && (from == pool_input_ata || pool_input_ata == Pubkey::default()) {
                        output_mint = Some(mint);
                        output_amount = amount;
                        output_index = Some(j as u32);
                    }
                }
                if input_mint.is_some() && output_mint.is_some() {
                    // Found both input and output mints
                    swaps.push(SwapV2::new(
                        Some(ix.program_id.to_string().into()),
                        program_id.to_string().into(),
                        authority,
                        Self::amm_inner_ix(inner_ix, account_keys).to_string().into(),
                        input_mint.clone().unwrap().into(),
                        output_mint.clone().unwrap().into(),
                        input_amount,
                        output_amount,
                        input_ata.to_string().into(),
                        output_ata.to_string().into(),
                        input_index,
                        output_index,
                        0,
                        0,
                        0,
                        Some(i as u32),
                    ));
                    next_logical_ix = j + 1;
                    return;
                }
            }
            // Still push in case we can't find one of the legs - rounded to zero or bug somewhere?
            swaps.push(SwapV2::new(
                Some(ix.program_id.to_string().into()),
                program_id.to_string().into(),
                authority,
                Self::amm_inner_ix(inner_ix, account_keys).to_string().into(),
                input_mint.clone().unwrap_or_default().into(),
                output_mint.clone().unwrap_or_default().into(),
                input_amount,
                output_amount,
                input_ata.to_string().into(),
                output_ata.to_string().into(),
                input_index,
                output_index,
                0,
                0,
                0,
                Some(i as u32),
            ));
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
                        let swap = SwapV2::new(
                            swap.outer_program().clone(),
                            swap.program().clone(),
                            swap.authority().clone(),
                            swap.amm().clone(),
                            swap.input_mint().clone(),
                            swap.output_mint().clone(),
                            *swap.input_amount(),
                            *swap.output_amount(),
                            swap.input_ata().clone(),
                            swap.output_ata().clone(),
                            *swap.input_inner_ix_index(),
                            *swap.output_inner_ix_index(),
                            slot,
                            raw_tx.index as u32,
                            i as u32,
                            *swap.inner_ix_index(),
                        );
                        swaps.push(swap);
                    });
                }
            });
            return swaps;
        }
        vec![]
    }
}
