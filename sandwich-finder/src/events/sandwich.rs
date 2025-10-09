use std::{collections::HashSet, sync::Arc};

use derive_getters::Getters;
use thiserror::Error;

use crate::events::{swap::SwapV2, transaction::TransactionV2, transfer::TransferV2};

#[derive(Debug, Error)]
pub enum SandwichError {
    #[error("Frontrun swaps don't share the same AMM+direction")]
    InvalidFrontrun,
    #[error("Backrun swaps don't share the same AMM+direction")]
    InvalidBackrun,
    #[error("Frontrun and backrun swaps don't share the same non-null wrapper program")]
    MissingWrapperProgram,
    #[error("Frontrun and backrun swaps don't use the same AMM in reverse directions")]
    FrontrunBackrunPairMismatch,
    #[error("Frontrun and backrun swaps don't use the same the same wrapper program")]
    FrontrunBackrunWrapperMismatch,
    #[error("Victim swaps don't share the same AMM+direction as the frontrun")]
    InvalidVictim,
    #[error("Transfers don't connect frontrun output ATAs to backrun input ATAs entirely")]
    InvalidTransfers,
    #[error("The sandwich is not strictly profitable")]
    NonProfitable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Getters)]
pub struct TradePair {
    amm: Arc<str>,
    input_mint: Arc<str>,
    output_mint: Arc<str>,
}

impl TradePair {
    pub fn new(amm: Arc<str>, input_mint: Arc<str>, output_mint: Arc<str>) -> Self {
        Self {
            amm,
            input_mint,
            output_mint,
        }
    }
    pub fn reverse(&self) -> TradePair {
        TradePair {
            amm: self.amm.clone(),
            input_mint: self.output_mint.clone(),
            output_mint: self.input_mint.clone(),
        }
    }
}

/// Components of a sandwich, in chronological order:
/// 1. Frontrun swaps - A->B
/// 2. Optional transfers to another wallet by the frontrunners
/// 3. Victim swaps - A->B
/// 4. Optional transfers to another wallet by the frontrunners
/// 5. Backrun swaps - B->A
/// 
/// Additionally, the profitability constraint is that
/// - # of tokens spent in step 1 <= # of tokens received in step 5
/// - # of tokens received in step 1 >= # of tokens spent in step 5
/// 
/// And obviously, the swapping steps must use the same AMM.
/// To reduce false positives, steps 1 and 5 must use the same non null non well-known aggregator outer program,
/// the justification being well-known aggregators aren't designed for sandwichers to keep track of their tokens across txs.
#[derive(Debug)]
pub struct SandwichCandidate {
    frontrun: Arc<[SwapV2]>,
    victim: Arc<[SwapV2]>,
    backrun: Arc<[SwapV2]>,
    transfers: Arc<[TransferV2]>,
    txs: Arc<[TransactionV2]>,
}

fn pair_from_swaps(swaps: &[SwapV2], check_wrapper: bool) -> Option<(Option<Arc<str>>, TradePair)> {
    if swaps.is_empty() {
        return None;
    }
    let first = &swaps[0];
    let pair = TradePair {
        amm: first.amm().clone(),
        input_mint: first.input_mint().clone(),
        output_mint: first.output_mint().clone(),
    };
    let outer_program = if check_wrapper { first.outer_program().clone() } else { None };
    for swap in swaps.iter() {
        let swap_pair = TradePair {
            amm: swap.amm().clone(),
            input_mint: swap.input_mint().clone(),
            output_mint: swap.output_mint().clone(),
        };
        if swap_pair != pair || (swap.outer_program() != &outer_program && check_wrapper) {
            return None;
        }
    }
    Some((outer_program, pair))
}

impl SandwichCandidate {
    pub fn new(frontrun: &[SwapV2], victim: &[SwapV2], backrun: &[SwapV2], transfers: &[TransferV2], txs: &[TransactionV2]) -> Result<Self, SandwichError> {
        // Sanity checks
        // {Front/back}run directions check - all frontrun swaps has the same pair and the reverse pair for the backrun swaps
        let (frontrun_wrapper, frontrun_pair) = pair_from_swaps(frontrun, true).ok_or(SandwichError::InvalidFrontrun)?;
        let (backrun_wrapper, backrun_pair) = pair_from_swaps(backrun, true).ok_or(SandwichError::InvalidBackrun)?;
        // println!("Frontrun pair: {:?}, Backrun pair: {:?}, Frontrun reversed: {:?}", frontrun_pair, backrun_pair, frontrun_pair.reverse());
        (frontrun_pair.reverse() == backrun_pair).then_some(()).ok_or(SandwichError::FrontrunBackrunPairMismatch)?;
        // Wrapper program check - both must have a non-null outer wrapper program and they must match
        // println!("Frontrun wrapper: {:?}, Backrun wrapper: {:?}", frontrun_wrapper, backrun_wrapper);
        (frontrun_wrapper.is_some() && backrun_wrapper.is_some()).then_some(()).ok_or(SandwichError::MissingWrapperProgram)?;
        (frontrun_wrapper == backrun_wrapper).then_some(()).ok_or(SandwichError::FrontrunBackrunWrapperMismatch)?;
        // Victim direction check - must share the same direction as the frontrun
        let (_, victim_pair) = pair_from_swaps(victim, false).ok_or(SandwichError::InvalidVictim)?;
        (victim_pair == frontrun_pair).then_some(()).ok_or(SandwichError::InvalidVictim)?;
        // Profitability check
        let frontrun_spent = frontrun.iter().map(|s| s.input_amount()).sum::<u64>();
        let frontrun_received = frontrun.iter().map(|s| s.output_amount()).sum::<u64>();
        let backrun_spent = backrun.iter().map(|s| s.input_amount()).sum::<u64>();
        let backrun_received = backrun.iter().map(|s| s.output_amount()).sum::<u64>();
        (frontrun_received >= backrun_spent && backrun_received >= frontrun_spent).then_some(()).ok_or(SandwichError::NonProfitable)?;
        // Transfers check - frontrun output ATAs must match backrun input ATAs either directly or with transfers
        let mut frontrun_set = frontrun.iter().map(|s| s.output_ata()).collect::<HashSet<_>>();
        let mut backrun_set = backrun.iter().map(|s| s.input_ata()).collect::<HashSet<_>>();
        let transfers = transfers.iter().filter(|t| frontrun_set.contains(t.input_ata()) && backrun_set.contains(t.output_ata())).cloned().collect::<Vec<_>>();
        for t in transfers.iter() {
            frontrun_set.remove(t.input_ata());
            backrun_set.remove(t.output_ata());
        }
        (frontrun_set == backrun_set).then_some(()).ok_or(SandwichError::InvalidTransfers)?;
        let tx_orders = [
            frontrun.iter().map(|f| (f.slot(), f.inclusion_order())).collect::<Vec<_>>(),
            victim.iter().map(|v| (v.slot(), v.inclusion_order())).collect::<Vec<_>>(),
            backrun.iter().map(|b| (b.slot(), b.inclusion_order())).collect::<Vec<_>>(),
        ].concat();
        Ok(Self {
            frontrun: Arc::from(frontrun),
            victim: Arc::from(victim),
            backrun: Arc::from(backrun),
            transfers: transfers.into(),
            txs: txs.iter().filter(|tx| tx_orders.contains(&(tx.slot(), tx.inclusion_order())) ).cloned().collect(),
        })
    }
}
