use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::LIMO_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for LimoSwapFinder {}

pub struct LimoSwapFinder {}

/// Limo has one variant
/// take order: [discriminant, input: u64, output: u64, expire_at: i64]
/// Orders are created adhoc so there's no amm, we make one up from the traded mints [5, 6] with xor
impl LimoSwapFinder {
    fn keys(ix: &Instruction) -> Vec<Pubkey> {
        vec![
            // maker in/out
            ix.accounts[7].pubkey,
            if ix.accounts[10].pubkey == LIMO_PUBKEY {
                ix.accounts[11].pubkey
            } else {
                ix.accounts[10].pubkey
            },
            // taker in/out
            ix.accounts[9].pubkey,
            ix.accounts[8].pubkey,
        ]
    }

    fn keys_inner(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Vec<Pubkey> {
        vec![
            // maker in/out
            account_keys[inner_ix.accounts[7] as usize],
            if account_keys[inner_ix.accounts[10] as usize] == LIMO_PUBKEY {
                account_keys[inner_ix.accounts[11] as usize]
            } else {
                account_keys[inner_ix.accounts[10] as usize]
            },
            // taker in/out
            account_keys[inner_ix.accounts[9] as usize],
            account_keys[inner_ix.accounts[8] as usize],
        ]
    }
}

impl SwapFinder for LimoSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        let in_mint = ix.accounts[5].pubkey;
        let out_mint = ix.accounts[6].pubkey;
        in_mint.to_bytes().iter().zip(out_mint.to_bytes().iter()).map(|(a, b)| a ^ b).collect::<Vec<u8>>()[..].try_into().expect("wrong length")
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        let in_mint = account_keys[inner_ix.accounts[5] as usize];
        let out_mint = account_keys[inner_ix.accounts[6] as usize];
        in_mint.to_bytes().iter().zip(out_mint.to_bytes().iter()).map(|(a, b)| a ^ b).collect::<Vec<u8>>()[..].try_into().expect("wrong length")
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let keys = Self::keys(ix);
        (keys[0], keys[1])
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let keys = Self::keys_inner(inner_ix, account_keys);
        (keys[0], keys[1])
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let keys = Self::keys(ix);
        (keys[2], keys[3])
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let keys = Self::keys_inner(inner_ix, account_keys);
        (keys[2], keys[3])
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // fill
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &LIMO_PUBKEY, &[0xa3, 0xd0, 0x14, 0xac, 0xdf, 0x41, 0xff, 0xe4], 0, 32),
        ].concat()
    }
}
