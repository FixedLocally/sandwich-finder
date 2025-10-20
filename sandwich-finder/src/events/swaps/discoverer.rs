use std::{collections::HashSet, sync::Arc};

use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, utils::token_transferred_inner}};

const BLACKLISTED_COMBINATIONS: &[(Pubkey, &[u8], usize)] = &[ // program, discriminant, offset
    (Pubkey::from_str_const("DDZDcYdQFEMwcu2Mwo75yGFjJ1mUQyyXLWzhZLEVFcei"), &[], 0), // appears to be something that does smth with the audio token
    (Pubkey::from_str_const("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"), &[], 0), // metaplex
    (Pubkey::from_str_const("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB"), &[0xa9, 0x20, 0x4f, 0x89, 0x88, 0xe8, 0x46, 0x89], 0), // meteora claim fees
    (Pubkey::from_str_const("dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN"), &[0x9c, 0xa9, 0xe6, 0x67, 0x35, 0xe4, 0x50, 0x40], 0), // dbc migrate
    (Pubkey::from_str_const("mmm3XBJg5gk8XJxEKBvdgptZz6SgK4tXvn36sodowMc"), &[], 0), // metaplex mmm, nft trades that we aren't interested in
    (Pubkey::from_str_const("M2mx93ekt1fmXSVkTrUL9xVFHkmME8HTUi5Cyc5aF7K"), &[], 0), // magic eden
    (Pubkey::from_str_const("APR1MEny25pKupwn72oVqMH4qpDouArsX8zX4VwwfoXD"), &[], 0), // star atlas stuff
    (Pubkey::from_str_const("SAGE2HAwep459SNq61LHvjxPk4pLPEJLoMETef7f7EE"), &[], 0), // star atlas stuff
    (Pubkey::from_str_const("Cargo2VNTPPTi9c1vq1Jw5d3BWUNr18MjRtSupAghKEk"), &[], 0), // star atlas stuff
];

impl Sealed for Discoverer {}

pub struct Discoverer {}

/// Outputs txid and program that triggered >=2 swaps in its inner instructions and emit a special swap event.
impl SwapFinder for Discoverer {
    fn amm_ix(_ix: &Instruction) -> Pubkey {
        Pubkey::default()
    }

    fn amm_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> Pubkey {
        Pubkey::default()
    }

    fn user_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    fn user_ata_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        // ignore known programs
        match ix.program_id {
            // RAYDIUM_V4_PUBKEY | RAYDIUM_V5_PUBKEY | RAYDIUM_LP_PUBKEY | RAYDIUM_CL_PUBKEY | PDF_PUBKEY | PDF2_PUBKEY | WHIRLPOOL_PUBKEY | DLMM_PUBKEY | METEORA_PUBKEY => vec![],
            _ => {
                let mut transfer_count = 0;
                let mut authorities = HashSet::new();
                let mut mints = HashSet::new();
                for comb in BLACKLISTED_COMBINATIONS {
                    if ix.program_id == comb.0 {
                        if ix.data.len() >= comb.2 + comb.1.len() {
                            if &ix.data[comb.2..comb.2 + comb.1.len()] == comb.1 {
                                return vec![];
                            }
                        }
                    }
                }
                for inner_ix in &inner_ixs.instructions {
                    if let Some((_from, _to, _auth, mint, _amount)) = token_transferred_inner(&inner_ix, &account_keys, &meta) {
                        transfer_count += 1;
                        match inner_ix.data[0] {
                            2 => { // System transfer
                                if inner_ix.accounts.len() >= 1 {
                                    let authority = account_keys[inner_ix.accounts[0] as usize];
                                    authorities.insert(authority);
                                }
                            },
                            3 => { // Transfer
                                if inner_ix.accounts.len() >= 3 {
                                    let authority = account_keys[inner_ix.accounts[2] as usize];
                                    authorities.insert(authority);
                                }
                            },
                            12 => { // TransferChecked
                                if inner_ix.accounts.len() >= 4 {
                                    let authority = account_keys[inner_ix.accounts[3] as usize];
                                    authorities.insert(authority);
                                }
                            },
                            _ => {}
                        }
                        mints.insert(mint);
                    }
                }
                if transfer_count >= 2 && authorities.len() >= 2 && mints.len() >= 2 {
                    let empty_str: Arc<str> = Arc::from("");
                    return vec![
                        SwapV2::new(None, ix.program_id.to_string().into(), empty_str.clone(), empty_str.clone(), empty_str.clone(), empty_str.clone(), 0, 0, empty_str.clone(), empty_str, None, None, 0, 0, 0, None, 0),
                    ];
                }
                vec![]
            }
        }
    }
}