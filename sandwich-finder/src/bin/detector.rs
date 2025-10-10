use std::{collections::{HashMap, HashSet}, sync::Arc};

use futures::future::join_all;
use mysql::{prelude::Queryable, Pool, Row};
use sandwich_finder::{events::{addresses::is_known_aggregator, common::Timestamp, sandwich::{SandwichCandidate, TradePair}, swap::SwapV2, transaction::TransactionV2, transfer::TransferV2}, utils::create_db_pool};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Serialize, Deserialize)]
struct GraphNode {
    id: String,
    label: String,
    #[serde(rename = "type")]
    node_type: String, // "token_account" or "market"
    value: Option<u64>,
    mint: Option<String>, // For token accounts
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphEdge {
    source: String,
    target: String,
    label: String,
    amount: u64,
    timestamp: String, // Serialized timestamp for ordering
    order: usize,
    edge_type: String, // "swap" or "transfer"
    trading_pair: Option<String>, // For swaps
}

#[derive(Debug, Serialize, Deserialize)]
struct TransferGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    slot: u64,
}

async fn get_events(conn: Pool, start_slot: u64, end_slot: u64) -> (Vec<SwapV2>, Vec<TransferV2>, Vec<TransactionV2>) {
    let conn = &mut conn.get_conn().unwrap();
    let res: Vec<Row> = conn.exec("select event_type, slot, inclusion_order, ix_index, inner_ix_index, authority, outer_program, program, amm, input_mint, output_mint, input_amount, output_amount, input_ata, output_ata, input_inner_ix_index, output_inner_ix_index from events where slot between ? and ?", vec![start_slot, end_slot]).unwrap();
    let mut swaps = vec![];
    let mut transfers = vec![];
    let mut txs = vec![];
    for row in res {
        let event_type: Arc<str> = row.get("event_type").unwrap();
        let slot: u64 = row.get("slot").unwrap();
        let inclusion_order: u32 = row.get("inclusion_order").unwrap();
        let ix_index: u32 = row.get("ix_index").unwrap();
        let inner_ix_index: Option<i32> = row.get("inner_ix_index").unwrap();
        let authority: Arc<str> = row.get("authority").unwrap();
        let outer_program: Option<Arc<str>> = row.get("outer_program").unwrap();
        let program: Arc<str> = row.get("program").unwrap();
        let amm: Option<Arc<str>> = row.get("amm").unwrap();
        let input_mint: Arc<str> = row.get("input_mint").unwrap();
        let output_mint: Arc<str> = row.get("output_mint").unwrap();
        let input_amount: u64 = row.get("input_amount").unwrap();
        let output_amount: u64 = row.get("output_amount").unwrap();
        let input_ata: Arc<str> = row.get("input_ata").unwrap();
        let output_ata: Arc<str> = row.get("output_ata").unwrap();
        let input_inner_ix_index: Option<i32> = row.get("input_inner_ix_index").unwrap();
        let output_inner_ix_index: Option<i32> = row.get("output_inner_ix_index").unwrap();
        let inner_ix_index = inner_ix_index.filter(|&x| x >= 0).map(|x| x as u32);
        let input_inner_ix_index = input_inner_ix_index.filter(|&x| x >= 0).map(|x| x as u32);
        let output_inner_ix_index = output_inner_ix_index.filter(|&x| x >= 0).map(|x| x as u32);
        match event_type.as_ref() {
            "SWAP" => {
                swaps.push(SwapV2::new(outer_program, program, authority, amm.unwrap(), input_mint, output_mint, input_amount, output_amount, input_ata, output_ata, input_inner_ix_index, output_inner_ix_index, slot, inclusion_order, ix_index, inner_ix_index));
            },
            "TRANSFER" => {
                transfers.push(TransferV2::new(outer_program, program, authority, input_mint, input_amount, input_ata, output_ata, slot, inclusion_order, ix_index, inner_ix_index));
            },
            _ => {},
        }
    }
    let res: Vec<Row> = conn.exec("select slot, inclusion_order, sig, fee, cu_actual from transactions where slot between ? and ?", vec![start_slot, end_slot]).unwrap();
    for row in res {
        let slot: u64 = row.get("slot").unwrap();
        let inclusion_order: u32 = row.get("inclusion_order").unwrap();
        let sig: String = row.get("sig").unwrap();
        let fee: u64 = row.get("fee").unwrap();
        let cu_actual: u64 = row.get("cu_actual").unwrap();
        txs.push(TransactionV2::new(slot, inclusion_order, sig.into(), fee, cu_actual));
    }

    // Filter out swap leg transfers
    let mut transfer_map: HashMap<Timestamp, TransferV2> = transfers.into_iter()
        .map(|t| (*t.timestamp(), t))
        .collect();
    for ele in swaps.iter() {
        if let Some(input_inner_ix) = ele.input_inner_ix_index() {
            transfer_map.remove(&Timestamp::new(*ele.slot(), *ele.inclusion_order(), *ele.ix_index(), Some(*input_inner_ix)));
        }
        if let Some(output_inner_ix) = ele.output_inner_ix_index() {
            transfer_map.remove(&Timestamp::new(*ele.slot(), *ele.inclusion_order(), *ele.ix_index(), Some(*output_inner_ix)));
        }
    }
    let transfers: Vec<_> = transfer_map.into_iter().map(|(_k, v)| v).collect();

    // Filter out transfers from AMMs (gets rid of some noise from fees)
    let amms = swaps.iter().map(|s| s.amm()).collect::<HashSet<_>>();
    let mut transfers: Vec<TransferV2> = transfers.into_iter().filter(|t| !amms.contains(t.input_ata()) && !amms.contains(t.output_ata()) && !amms.contains(t.authority())).collect();

    // Sort events in chronological order
    swaps.sort_by_cached_key(|s| *s.timestamp());
    transfers.sort_by_cached_key(|t| *t.timestamp());
    txs.sort_by_cached_key(|t| Timestamp::new(*t.slot(), *t.inclusion_order(), 0, None));

    (swaps, transfers, txs)
}

/// This function expects the events to be sorted in chronological order (which [get_event] does)
fn detect_main(swaps: &[SwapV2], transfers: &[TransferV2], txs: &[TransactionV2]) {
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
            let sandwich = candidates.last().unwrap();
            println!("Found sandwich {:?}", sandwich);
        }
    }
}
// Swap in slot 371237175 (order 1242, ix 1, inner_ix Some(1))
// Swap in slot 371237175 (order 1247, ix 5, inner_ix None)
// Swap in slot 371237175 (order 1248, ix 2, inner_ix Some(0))

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // let slot = 371237175;
    // parse the 1st arg for slot
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <slot>", args[0]);
        return;
    }
    let start_slot: u64 = args[1].parse().expect("Invalid slot");
    let end_slot: u64 = if args.len() >= 3 {
        args[2].parse().expect("Invalid slot")
    } else {
        start_slot
    };
    // alignment
    let start_slot = start_slot / 4 * 4;
    let end_slot = end_slot / 4 * 4 + 3;
    // fetch events for 1k slots at a time and process in groups of 4 slots
    let pool = create_db_pool();
    let handles: Vec<_> = (start_slot..=end_slot).step_by(1000).map(|chunk_start| {
        let chunk_end = (chunk_start + 999).min(end_slot);
        let pool = pool.clone(); // docs said this is cloneable
        tokio::spawn(async move {
            let (swaps, transfers, txs) = get_events(pool, chunk_start, chunk_end).await;
            let mut swaps_start = 0;
            let mut transfers_start = 0;
            let mut txs_start = 0;
            for slot in (chunk_start..=chunk_end).step_by(4) {
                let swaps_end = swaps.iter().skip(swaps_start).position(|s| *s.slot() >= slot + 4).map(|n| n + swaps_start).unwrap_or(swaps.len());
                let transfers_end = transfers.iter().skip(transfers_start).position(|t| *t.slot() >= slot + 4).map(|n| n + transfers_start).unwrap_or(transfers.len());
                let txs_end = txs.iter().skip(txs_start).position(|t| *t.slot() >= slot + 4).map(|n| n + txs_start).unwrap_or(txs.len());

                let slot_swaps = &swaps[swaps_start..swaps_end];
                let slot_transfers = &transfers[transfers_start..transfers_end];
                let slot_txs = &txs[txs_start..txs_end];
                println!("Processing slots {} to {}", slot, slot + 3);
                println!("Swaps: {:#?}", slot_swaps.len());
                println!("Transfers: {:#?}", slot_transfers.len());
                println!("Txs: {:#?}", slot_txs.len());
                detect_main(slot_swaps, slot_transfers, slot_txs);

                swaps_start = swaps_end;
                transfers_start = transfers_end;
                txs_start = txs_end;
            }
        })
    }).collect();
    join_all(handles).await;
}
