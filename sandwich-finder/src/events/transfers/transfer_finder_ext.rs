use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::{geyser::SubscribeUpdateTransactionInfo, prelude::InnerInstructions};

use crate::events::{transfer::{TransferFinder, TransferV2}, transfers::private};


/// This trait contains helper methods not meant to be overridden by the implementors of [`TransferFinder`].
pub trait TransferFinderExt: private::Sealed {
    /// Finds transfer in this tx utilising the provided program id by iterating through the ixs.
    fn find_transfers_in_tx(slot: u64, raw_tx: &SubscribeUpdateTransactionInfo, ixs: &Vec<Instruction>, account_keys: &Vec<Pubkey>) -> Vec<TransferV2>;
}

impl<T: TransferFinder + private::Sealed> TransferFinderExt for T {
    fn find_transfers_in_tx(slot: u64, raw_tx: &SubscribeUpdateTransactionInfo, ixs: &Vec<Instruction>, account_keys: &Vec<Pubkey>) -> Vec<TransferV2> {
        if let Some(meta) = &raw_tx.meta {
            let mut transfers = vec![];
            ixs.iter().enumerate().for_each(|(i, ix)| {
                let inner_ixs = meta.inner_instructions.iter().find(|x| x.index == i as u32);
                let default = InnerInstructions { index: i as u32, instructions: vec![] };
                let inner_ixs = inner_ixs.unwrap_or(&default);
                // We want to index events here even if there's no inner ixs since that's how plain transfers work
                Self::find_transfers(ix, inner_ixs, account_keys, meta).iter().for_each(|transfer| {
                    let transfer = TransferV2::new(
                        transfer.outer_program().clone(),
                        transfer.program().clone(),
                        transfer.authority().clone(),
                        transfer.mint().clone(),
                        *transfer.amount(),
                        transfer.input_ata().clone(),
                        transfer.output_ata().clone(),
                        slot,
                        raw_tx.index as u32,
                        i as u32,
                        *transfer.inner_ix_index(),
                        *transfer.id(),
                    );
                    transfers.push(transfer);
                });
            });
            return transfers;
        }
        vec![]
    }
}
