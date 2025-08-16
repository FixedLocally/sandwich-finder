use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::prelude::{InnerInstruction, TransactionStatusMeta};

use crate::swaps::addresses::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};

pub fn mint_of(pubkey: &Pubkey, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Option<String> {
    let target_index = account_keys.iter().position(|key| key == pubkey);
    if target_index.is_none() {
        return None;
    }
    meta.pre_token_balances
        .iter()
        .find(|&balance| balance.account_index == target_index.unwrap() as u32)
        .map_or(None, |balance| Some(balance.mint.clone()))
}

pub fn token_transferred_inner(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Option<(Pubkey, Pubkey, String, u64)> {
    // (from, to, mint, amount)
    if inner_ix.program_id_index >= account_keys.len() as u32 {
        return None;
    }
    let program_id = account_keys[inner_ix.program_id_index as usize];
    if program_id != TOKEN_PROGRAM_ID && program_id != TOKEN_2022_PROGRAM_ID {
        return None;
    }
    // ix, amount[, decimals]
    if inner_ix.data.len() < 9 {
        return None;
    }
    let (from_index, to_index) = match inner_ix.data[0] {
        3 => (inner_ix.accounts[0], inner_ix.accounts[1]), // Transfer
        12 => (inner_ix.accounts[0], inner_ix.accounts[2]), // TransferChecked
        _ => (255, 255), // Not a transfer, will be caught by bounds check
    };
    if from_index as usize >= account_keys.len() || to_index as usize >= account_keys.len() {
        return None;
    }
    let from_mint = mint_of(&account_keys[from_index as usize], &account_keys, &meta);
    let to_mint: Option<String> = mint_of(&account_keys[to_index as usize], &account_keys, &meta);
    if from_mint.is_none() && to_mint.is_none() {
        return None;
    }
    return Some((
        account_keys[from_index as usize],
        account_keys[to_index as usize],
        from_mint.or(to_mint).unwrap(),
        u64::from_le_bytes(inner_ix.data[1..9].try_into().unwrap()),
    ));
}
