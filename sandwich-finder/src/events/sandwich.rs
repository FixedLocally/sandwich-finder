use std::{collections::{HashMap, HashSet}, sync::Arc};

use derive_getters::Getters;
use solana_sdk::pubkey::Pubkey;
use thiserror::Error;

use crate::events::{addresses::is_known_aggregator, swap::SwapV2, transaction::TransactionV2, transfer::TransferV2};

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
    #[error("Victim swaps don't share the same AMM+direction as the frontrun or share a wrapper program with the frontrun/backrun")]
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
/// Victim swaps also can't use the same wrapper program as the frontrun/backrun swaps.
#[derive(Clone, Debug, Getters)]
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
        // Wrapper program check - wrapper program must match
        // println!("Frontrun wrapper: {:?}, Backrun wrapper: {:?}", frontrun_wrapper, backrun_wrapper);
        // (frontrun_wrapper.is_some() && backrun_wrapper.is_some()).then_some(()).ok_or(SandwichError::MissingWrapperProgram)?;
        (frontrun_wrapper == backrun_wrapper).then_some(()).ok_or(SandwichError::FrontrunBackrunWrapperMismatch)?;
        // Victim direction check - must share the same direction as the frontrun
        let (_, victim_pair) = pair_from_swaps(victim, false).ok_or(SandwichError::InvalidVictim)?;
        (victim_pair == frontrun_pair).then_some(()).ok_or(SandwichError::InvalidVictim)?;
        // Victim wrapper check - must not share the same wrapper program as the frontrun/backrun unless it's None
        victim.iter().all(|s| s.outer_program().is_none() || s.outer_program() != &frontrun_wrapper).then_some(()).ok_or(SandwichError::InvalidVictim)?;
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

/// This function expects the events to be sorted in chronological order
pub fn detect(swaps: &[SwapV2], transfers: &[TransferV2], txs: &[TransactionV2]) -> Arc<[SandwichCandidate]> {
    // Group swaps by AMM then direction also by outer program
    let mut amm_swaps: HashMap<Arc<str>, HashMap<TradePair, Vec<SwapV2>>> = HashMap::new();
    for swap in swaps.iter() {
        let pair = TradePair::new(
            swap.amm().clone(),
            swap.input_mint().clone(),
            swap.output_mint().clone(),
        );
        amm_swaps.entry(swap.amm().clone()).or_default().entry(pair.clone()).or_default().push(swap.clone());
    }

    // for each swap, we want to match it with a series of swaps before it in the same direction and a series of swaps after it in the opposite direction
    let mut matched_timestamps = HashSet::new(); // to avoid double counting
    let mut sandwiches = vec![];
    for swap in swaps.iter() {
        if matched_timestamps.contains(swap.timestamp()) {
            continue;
        }
        let pair = TradePair::new(
            swap.amm().clone(),
            swap.input_mint().clone(),
            swap.output_mint().clone(),
        );
        let rev_pair = pair.reverse();
        let before_swaps = amm_swaps.get(swap.amm()).and_then(|m| m.get(&pair)).map(|v| v.iter().filter(|s| s.timestamp() < swap.timestamp()).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let after_swaps = amm_swaps.get(swap.amm()).and_then(|m| m.get(&rev_pair)).map(|v| v.iter().filter(|s| s.timestamp() > swap.timestamp()).cloned().collect::<Vec<_>>()).unwrap_or_default();
        if before_swaps.is_empty() || after_swaps.is_empty() {
            continue;
        }
        // println!("Analyzing swap at {:?} for sandwiches {:?} {:?}", swap.timestamp(), before_swaps, after_swaps);
        // we then group the swaps before and after by outer program and see if some outer program may be sandwiching this swap
        let before_outer = {
            let mut map: HashMap<Option<Arc<str>>, Vec<SwapV2>> = HashMap::new();
            for s in before_swaps.iter() {
                map.entry(s.outer_program().clone()).or_default().push(s.clone());
            }
            map
        };
        let after_outer = {
            let mut map: HashMap<Option<Arc<str>>, Vec<SwapV2>> = HashMap::new();
            for s in after_swaps.iter() {
                map.entry(s.outer_program().clone()).or_default().push(s.clone());
            }
            map
        };
        let mut candidates = vec![];
        for (k, before_swaps) in before_outer.iter() {
            if k.is_some() && is_known_aggregator(&Pubkey::from_str_const(k.as_ref().unwrap())) {
                continue;
            }
            if let Some(after_swaps) = after_outer.get(k) {
                // loop thru all possible contiguous segments of before_swaps and after_swaps and try to contruct a sandwich out of them
                // println!("Looking at outer program {:?} {} {}", k, before_swaps.len(), after_swaps.len());
                for i in 0..before_swaps.len() {
                    for j in i+1..=before_swaps.len() {
                        for m in 0..after_swaps.len() {
                            for n in m+1..=after_swaps.len() {
                                let frontrun = &before_swaps[i..j];
                                let frontrun_last = before_swaps[j - 1].clone();
                                let backrun = &after_swaps[m..n];
                                let backrun_first = after_swaps[m].clone();
                                let victim = &swaps.iter().filter(|s| s.timestamp() > frontrun_last.timestamp() && s.timestamp() < backrun_first.timestamp() && s.amm() == swap.amm() && s.input_mint() == swap.input_mint() && s.output_mint() == swap.output_mint()).cloned().collect::<Vec<_>>()[..];
                                match SandwichCandidate::new(frontrun, victim, backrun, &transfers, &txs) {
                                    Ok(sandwich) => {
                                        candidates.push(sandwich);
                                        victim.iter().for_each(|s| { matched_timestamps.insert(*s.timestamp()); });
                                    }
                                    // Err(e) => println!("Failed to create sandwich candidate: {},{},{},{} {:?}", i,j,m,n,e),
                                    Err(_) => {},
                                }
                            }
                        }
                    }
                }
            }
        }
        // if there are multiple candidates, we pick the one with the most victims, then the one with the most swaps
        if !candidates.is_empty() {
            candidates.sort_by_cached_key(|c| (c.victim().len(), c.frontrun().len() + c.backrun().len()));
            sandwiches.push(candidates.last().unwrap().clone());
        }
    }

    sandwiches.into()
}
/*
SandwichCandidate {
  frontrun: [
    Swap in slot 372367924 (order 1121, ix 5, inner_ix None)
      on dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN market 8QhobkasSgim5hxmUgF7xEvm9piz4KyBp4ATNPYRusHa
      Route So11111111111111111111111111111111111111112 -> GUCwGWVATG5jZxagJWwhJgFasV2XH7DgqF9gJgveV3oJ Amounts 31432000 -> 27238764881
      ATAs Ha4yHUP5P9ye8J2nuWDuckYvsK8RAsXXqxkBjm4dfm7V -> F6hLzeLQ4vnrdEnikXvUcRfNF2E4zWFxVKfTfanSHNVu
  ],
  victim: [
    Swap in slot 372367925 (order 1136, ix 5, inner_ix None)
      on dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN market 8QhobkasSgim5hxmUgF7xEvm9piz4KyBp4ATNPYRusHa
      Route So11111111111111111111111111111111111111112 -> GUCwGWVATG5jZxagJWwhJgFasV2XH7DgqF9gJgveV3oJ Amounts 1418439693 -> 1262993306810
      ATAs 6kMydTUPUK9ntXyAdSypft9es4mqH2CaxjoYKcCuqfS9 -> F9Kzc4LG5XQsmY8JrimnKU1TA16jednkvnDEmH2Xj2d8,
    Swap in slot 372367925 (order 1147, ix 5, inner_ix None)
      on dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN market 8QhobkasSgim5hxmUgF7xEvm9piz4KyBp4ATNPYRusHa
      Route So11111111111111111111111111111111111111112 -> GUCwGWVATG5jZxagJWwhJgFasV2XH7DgqF9gJgveV3oJ Amounts 1406197017 -> 1206419070141
      ATAs 6TALPjXACbQnhHHWoCJUypHVM7AqXzmHdnXb2nm4sh7u -> Kk6tDPgYpGnKY7c4yBidPywJKBr4XHjmnqyuwA1BtCN,
    Swap in slot 372367926 (order 1101, ix 5, inner_ix None)
      on dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN market 8QhobkasSgim5hxmUgF7xEvm9piz4KyBp4ATNPYRusHa
      Route So11111111111111111111111111111111111111112 -> GUCwGWVATG5jZxagJWwhJgFasV2XH7DgqF9gJgveV3oJ Amounts 1436087902 -> 1249015798317
      ATAs HknctZFeNjkwdSawyj85yBAtaxSJAscVNKBpZwZo9beY -> 2VWFWgk2e9bemJvfLDoKQNtt53bPfGa6puhmCb82YoSU
  ],
  backrun: [
    Swap in slot 372367926 (order 1326, ix 3, inner_ix None)
      on dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN market 8QhobkasSgim5hxmUgF7xEvm9piz4KyBp4ATNPYRusHa
      Route GUCwGWVATG5jZxagJWwhJgFasV2XH7DgqF9gJgveV3oJ -> So11111111111111111111111111111111111111112 Amounts 27238764881 -> 31894629
      ATAs F6hLzeLQ4vnrdEnikXvUcRfNF2E4zWFxVKfTfanSHNVu -> Ha4yHUP5P9ye8J2nuWDuckYvsK8RAsXXqxkBjm4dfm7V
  ],
  transfers: [],
  txs: [
    TransactionV2 { slot: 372367924, inclusion_order: 1121, sig: "3p4cmmRUfEL3GFEtaTNgifNmDpF6fpoxvgM2p5Pf32rzFyu2BpM3qEy8ZoghfURgvgg3n45ZN978s3voyAX8qVYb", fee: 6200, cu_actual: 75297 },
    TransactionV2 { slot: 372367925, inclusion_order: 1136, sig: "28pQvdt1xrFTxk2G15WQZu8aVtPdQzsDEVyXBjuKk22n2X6zn7pFkeDzfjvJVN9jSBVVkcVkMj17TvnTxmjpac2B", fee: 6200, cu_actual: 76791 },
    TransactionV2 { slot: 372367925, inclusion_order: 1147, sig: "42RwgPEvRc2rwTixBrRKKfuBUKh8k6QaW4KKyyds44bseyGxkTnkvqrqzdaiXUsgGHtQVZXBGkBdaB81KakCHyXD", fee: 6200, cu_actual: 76797 },
    TransactionV2 { slot: 372367926, inclusion_order: 1101, sig: "3Jb6uRtCtwTpvk5wndPvR4zmyxMgskhoDAHkgmAcmBpJiQfjecAE23DhuoM1iZ7ujcAAi5bneZiN5DFFEJa7s1Br", fee: 6200, cu_actual: 75294 },
    TransactionV2 { slot: 372367926, inclusion_order: 1326, sig: "LP29LGbuePvufokqmMW2yVvQGX48PA6D32XDX4MPKhdGrpvDuhDZBXVSkmM4XX8xMPEKFNGqWaukfk1g6GQgevq", fee: 6200, cu_actual: 69165 }
  ]
}
 */