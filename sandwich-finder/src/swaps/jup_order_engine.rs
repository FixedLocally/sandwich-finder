use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::JUP_ORDER_ENGINE_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for JupOrderEngineSwapFinder {}

pub struct JupOrderEngineSwapFinder {}

/// Jup order engine has one variant
/// fill: [discriminant, input: u64, output: u64, expire_at: i64]
/// Orders are created adhoc so there's no amm, we make one up from the traded mints [6, 8] with xor

impl SwapFinder for JupOrderEngineSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        let in_mint = ix.accounts[6].pubkey;
        let out_mint = ix.accounts[8].pubkey;
        in_mint.to_bytes().iter().zip(out_mint.to_bytes().iter()).map(|(a, b)| a ^ b).collect::<Vec<u8>>()[..].try_into().expect("wrong length")
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        let in_mint = account_keys[inner_ix.accounts[6] as usize];
        let out_mint = account_keys[inner_ix.accounts[8] as usize];
        in_mint.to_bytes().iter().zip(out_mint.to_bytes().iter()).map(|(a, b)| a ^ b).collect::<Vec<u8>>()[..].try_into().expect("wrong length")
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[2].pubkey,
            ix.accounts[4].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[2] as usize],
            account_keys[inner_ix.accounts[4] as usize],
        )
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[5].pubkey,
            ix.accounts[3].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[5] as usize],
            account_keys[inner_ix.accounts[3] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // fill
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_ORDER_ENGINE_PUBKEY, &[0xa8, 0x60, 0xb7, 0xa3, 0x5c, 0x0a, 0x28, 0xa0], 32),
        ].concat()
    }
}
