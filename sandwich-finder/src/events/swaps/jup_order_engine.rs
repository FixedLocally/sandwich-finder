use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::swaps::{addresses::JUP_ORDER_ENGINE_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for JupOrderEngineSwapFinder {}

pub struct JupOrderEngineSwapFinder {}

/// Jup order engine has one variant
/// fill: [discriminant, input: u64, output: u64, expire_at: i64]
/// Special care is required for swaps involving SOL since the "token account" is the program id
/// Need to set that to the corresponding party
/// Orders are created adhoc so there's no amm, we make one up from the traded mints [6, 8] with xor
impl JupOrderEngineSwapFinder {
    fn keys(ix: &Instruction) -> Vec<Pubkey> {
        let mut keys = vec![
            // taker in/out
            ix.accounts[2].pubkey,
            ix.accounts[4].pubkey,
            // maker in/out
            ix.accounts[5].pubkey,
            ix.accounts[3].pubkey,
        ];
        let taker = ix.accounts[0].pubkey;
        let maker = ix.accounts[1].pubkey;
        // if the taker is paying sol: it gets system-transfer'd to the taker's ata (only replace user input)
        // if the taker is receiving sol: the maker transfers the sol to a temp ata, then closed to the taker and system transfer'd (replace both)
        if keys[0] == JUP_ORDER_ENGINE_PUBKEY {
            keys[0] = taker;
        }
        if keys[1] == JUP_ORDER_ENGINE_PUBKEY {
            keys[1] = taker;
            keys[2] = maker;
        }
        keys.iter().enumerate().map(|(i,k)| if *k == JUP_ORDER_ENGINE_PUBKEY { if i < 2 { taker } else { maker } } else { *k }).collect::<Vec<Pubkey>>()
    }

    fn keys_inner(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Vec<Pubkey> {
        let mut keys = vec![
            // taker in/out
            account_keys[inner_ix.accounts[2] as usize],
            account_keys[inner_ix.accounts[4] as usize],
            // maker in/out
            account_keys[inner_ix.accounts[5] as usize],
            account_keys[inner_ix.accounts[3] as usize],
        ];
        let taker = account_keys[inner_ix.accounts[0] as usize];
        let maker = account_keys[inner_ix.accounts[1] as usize];
        if keys[0] == JUP_ORDER_ENGINE_PUBKEY {
            keys[0] = taker;
        }
        if keys[1] == JUP_ORDER_ENGINE_PUBKEY {
            keys[1] = taker;
            keys[2] = maker;
        }
        keys
    }
}

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
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_ORDER_ENGINE_PUBKEY, &[0xa8, 0x60, 0xb7, 0xa3, 0x5c, 0x0a, 0x28, 0xa0], 0, 32),
        ].concat()
    }
}
