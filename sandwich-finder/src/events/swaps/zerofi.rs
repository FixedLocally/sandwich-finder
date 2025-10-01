use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::swaps::{addresses::ZEROFI_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for ZeroFiSwapFinder {}

pub struct ZeroFiSwapFinder {}

/// ZeroFi doesn't have any published IDL so it's guesswork from solscan/jup txs
/// swap: [0x06, input: u64, min_output: u64]

impl SwapFinder for ZeroFiSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[0].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[0] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[5].pubkey,
            ix.accounts[6].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[5] as usize],
            account_keys[inner_ix.accounts[6] as usize],
        )
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[4].pubkey,
            ix.accounts[2].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[4] as usize],
            account_keys[inner_ix.accounts[2] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &ZEROFI_PUBKEY, &[0x06], 0, 17),
        ].concat()
    }
}
