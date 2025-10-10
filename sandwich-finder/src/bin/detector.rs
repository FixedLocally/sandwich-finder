use std::{collections::{HashMap, HashSet}, sync::Arc};

use futures::future::join_all;
use mysql::{prelude::Queryable, Pool, Row};
use sandwich_finder::{events::{addresses::is_known_aggregator, common::Timestamp, sandwich::{SandwichCandidate, TradePair}, swap::SwapV2, transaction::TransactionV2, transfer::TransferV2}, utils::create_db_pool};
use serde::{Deserialize, Serialize};

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
    let mut matched_timestamps = HashMap::new(); // to avoid double counting
    for swap in swaps.iter() {
        if matched_timestamps.contains_key(swap.timestamp()) {
            continue;
        }
        let pair = TradePair::new(
            swap.amm().clone(),
            swap.input_mint().clone(),
            swap.output_mint().clone(),
        );
        // println!("Analyzing swap {:?} for sandwiches", swap);
        let rev_pair = pair.reverse();
        let before_swaps = amm_swaps.get(swap.amm()).and_then(|m| m.get(&pair)).map(|v| v.iter().filter(|s| s.timestamp() < swap.timestamp()).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let after_swaps = amm_swaps.get(swap.amm()).and_then(|m| m.get(&rev_pair)).map(|v| v.iter().filter(|s| s.timestamp() > swap.timestamp()).cloned().collect::<Vec<_>>()).unwrap_or_default();
        if before_swaps.is_empty() || after_swaps.is_empty() {
            continue;
        }
        // we then group the swaps before and after by outer program and see if some outer program may be sandwiching this swap
        let before_outer = {
            let mut map: HashMap<Arc<str>, Vec<SwapV2>> = HashMap::new();
            for s in before_swaps.iter() {
                if let Some(outer) = s.outer_program() {
                    map.entry(outer.clone()).or_default().push(s.clone());
                }
            }
            map
        };
        let after_outer = {
            let mut map: HashMap<Arc<str>, Vec<SwapV2>> = HashMap::new();
            for s in after_swaps.iter() {
                if let Some(outer) = s.outer_program() {
                    map.entry(outer.clone()).or_default().push(s.clone());
                }
            }
            map
        };
        for (k, before_swaps) in before_outer.iter() {
            if is_known_aggregator(&k.parse().unwrap()) {
                continue;
            }
            // println!("Looking at outer program {}", k);
            if let Some(after_swaps) = after_outer.get(k) {
                // loop thru all possible contiguous segments of before_swaps and after_swaps and try to contruct a sandwich out of them
                for i in 0..before_swaps.len() {
                    for j in i+1..=before_swaps.len() {
                        for m in 0..after_swaps.len() {
                            for n in m+1..=after_swaps.len() {
                                let frontrun = &before_swaps[i..j];
                                let frontrun_last = before_swaps[j - 1].clone();
                                let backrun = &after_swaps[m..n];
                                let backrun_first = after_swaps[m].clone();
                                let victim = &swaps.iter().filter(|s| s.timestamp() > frontrun_last.timestamp() && s.timestamp() < backrun_first.timestamp() && s.amm() == swap.amm() && s.input_mint() == swap.input_mint() && s.output_mint() == swap.output_mint()).cloned().collect::<Vec<_>>()[..];
                                if let Ok(sandwich) = SandwichCandidate::new(frontrun, victim, backrun, &transfers, &txs) {
                                    println!("Found sandwich {:?}", sandwich);
                                    victim.iter().for_each(|s| { matched_timestamps.insert(*s.timestamp(), ()); });
                                }
                            }
                        }
                    }
                }
            }
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

/*
Found sandwich SandwichCandidate {
  frontrun: [
    Swap in slot 371297968 (order 85, ix 2, inner_ix Some(6))
      via fat2dUTkypDNDT86LtLGmzJDK11FSJ72gfUW35igk7u
      on 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P market 8GbA6iVh6R3qjpmkTn7u7y6kKsg4YqTyhv6DpLG4F7tT
      Route So11111111111111111111111111111111111111112 -> J9AH4Kokzb5G4c4qYKpF78aCG6bPT4oyAwBmAV8Cpump Amounts 992451 -> 27079675008
      ATAs 78NxjahfvAmFEWkNVbkgAwLSBgrKv7uNYtbwYFaW43Mt -> 3RX9gfMfaWnWtELY13CX35jd1vaKzuxeoTuXF6FqdQsH
  ],
  victim: [
    Swap in slot 371297968 (order 1191, ix 3, inner_ix None)
      on 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P market 8GbA6iVh6R3qjpmkTn7u7y6kKsg4YqTyhv6DpLG4F7tT
      Route So11111111111111111111111111111111111111112 -> J9AH4Kokzb5G4c4qYKpF78aCG6bPT4oyAwBmAV8Cpump Amounts 920454546 -> 24462995781353
      ATAs 2hBhRbTDVy7RLhUbTCvCeWxtbbGqQACiFoF2xMf7usY5 -> FTgW5RtnDRMNQTvhgrCQgHftjzcHKk6cBYf2yjZpixFq,
    Swap in slot 371297969 (order 10, ix 4, inner_ix None)
      on 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P market 8GbA6iVh6R3qjpmkTn7u7y6kKsg4YqTyhv6DpLG4F7tT
      Route So11111111111111111111111111111111111111112 -> J9AH4Kokzb5G4c4qYKpF78aCG6bPT4oyAwBmAV8Cpump Amounts 156607921 -> 4036394000000
      ATAs AK47N3ifHtWpHNifV8MGkFzJ28woqygecMF36Km1DpWX -> F7SwFZEka6GXn7JazTmcQ1s6uwcW8C8Xz7Vc1Yoh2jAd,
    Swap in slot 371297969 (order 960, ix 3, inner_ix None)
      on 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P market 8GbA6iVh6R3qjpmkTn7u7y6kKsg4YqTyhv6DpLG4F7tT
      Route So11111111111111111111111111111111111111112 -> J9AH4Kokzb5G4c4qYKpF78aCG6bPT4oyAwBmAV8Cpump Amounts 471469307 -> 11940238239704
      ATAs bB85mL5d7hfVY4CsDkNRttaBMR7hzmcnk9RNuPpw7my -> 8qvnXVUDBkqJNQr428sm1R9gwpzG7ATqrs6C3QMeHB5v
  ],
  backrun: [
    Swap in slot 371297971 (order 1146, ix 2, inner_ix Some(0))
      via fat2dUTkypDNDT86LtLGmzJDK11FSJ72gfUW35igk7u
      on 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P market 8GbA6iVh6R3qjpmkTn7u7y6kKsg4YqTyhv6DpLG4F7tT
      Route J9AH4Kokzb5G4c4qYKpF78aCG6bPT4oyAwBmAV8Cpump -> So11111111111111111111111111111111111111112 Amounts 27079675008 -> 1056624
      ATAs 3RX9gfMfaWnWtELY13CX35jd1vaKzuxeoTuXF6FqdQsH -> 78NxjahfvAmFEWkNVbkgAwLSBgrKv7uNYtbwYFaW43Mt
  ], transfers: [],
  txs: [
    TransactionV2 { slot: 371297968, inclusion_order: 85, sig: "3EaRBsquAhgjYxBq4XRpfhTAi8vVePTmp96WVDFf2sGzD5DfUck8atG7VbnVJ81gywawCTHyYuYU33CsfRgoXjhd", fee: 24500, cu_actual: 135104 },
    TransactionV2 { slot: 371297968, inclusion_order: 1191, sig: "2aAS3dweZz9kKVycjdFEmq6Vb3pmfcaHSwSdbjKi5ShqNitfACugKzvFii5NvGjdpYcYYd8p5hA2rVu6yXjNn71S", fee: 505000, cu_actual: 93257 },
    TransactionV2 { slot: 371297969, inclusion_order: 10, sig: "QNTatzE9i8pWCSC8MjJcc3XA75LhN7wwziGCdBaFQauGb42XJoYrEnvKT31KkBzriKMyFdUXUq2h8U8uMCcxx3r", fee: 5220, cu_actual: 88743 },
    TransactionV2 { slot: 371297969, inclusion_order: 960, sig: "3kvRSbF4XdSxF4SfP5jgmahmSVMZZC2eEWtCiS7HDQnmhhqJCxtVNbouseffecNsBLWWfgZooyzqfvYtgbsibX3y", fee: 105000, cu_actual: 97751 },
    TransactionV2 { slot: 371297971, inclusion_order: 1146, sig: "2gE7PDYNon5Jjc4521aMrzCFaY7qvVTZCVj4grPUPNusSbgvK4Hs3qYDdJLU4TBfW7MA2PUJcr1AbHGrXXjHg8vv", fee: 11500, cu_actual: 86151 }
  ]
}

Found sandwich SandwichCandidate {
  frontrun: [
    Swap in slot 360045483 (order 866, ix 1, inner_ix Some(3))
      via inf69quFVZyuHEsrUXq3APtYLr4iqsNiQdCh5ArGcUp
      on 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 market 4yF23nnZgRUnATEJzUZumg9PJAAp6wq4WVfR72TUpqWA
      Route So11111111111111111111111111111111111111112 -> 7txRrYTGsnwQcHKKebtsUbcA3vTBwNUewMp75mNDtRmP Amounts 82289617903 -> 62059648090860
      ATAs iP2PFWEQ69LTvrQPKsupjuwxHXCdpBC4VVnsdDbgByp -> 8jNGEPu52WoD1khVHJA2sC2ZQDpKcPnE5wHJESku2aQS,
    Swap in slot 360045483 (order 867, ix 1, inner_ix Some(0))
      via inf69quFVZyuHEsrUXq3APtYLr4iqsNiQdCh5ArGcUp
      on 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 market 4yF23nnZgRUnATEJzUZumg9PJAAp6wq4WVfR72TUpqWA
      Route So11111111111111111111111111111111111111112 -> 7txRrYTGsnwQcHKKebtsUbcA3vTBwNUewMp75mNDtRmP Amounts 111131301869 -> 14119534502173
      ATAs iP2PFWEQ69LTvrQPKsupjuwxHXCdpBC4VVnsdDbgByp -> 8jNGEPu52WoD1khVHJA2sC2ZQDpKcPnE5wHJESku2aQS
  ],
  victim: [
    Swap in slot 360045483 (order 869, ix 4, inner_ix None)
      on 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 market 4yF23nnZgRUnATEJzUZumg9PJAAp6wq4WVfR72TUpqWA
      Route So11111111111111111111111111111111111111112 -> 7txRrYTGsnwQcHKKebtsUbcA3vTBwNUewMp75mNDtRmP Amounts 1020158230 -> 67379484587
      ATAs HgzRp9yzD1tQPVcz5KzovzAmDHkNMcqX1AVxEjQyG1yL -> 78YxK8Vg9raKbGX3b1akqdgcu5WJLscJH2aajJ9VpxdX
  ],
  backrun: [
    Swap in slot 360045483 (order 870, ix 0, inner_ix Some(0))
      via inf69quFVZyuHEsrUXq3APtYLr4iqsNiQdCh5ArGcUp
      on 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 market 4yF23nnZgRUnATEJzUZumg9PJAAp6wq4WVfR72TUpqWA
      Route 7txRrYTGsnwQcHKKebtsUbcA3vTBwNUewMp75mNDtRmP -> So11111111111111111111111111111111111111112 Amounts 76179182593033 -> 194216632113
      ATAs BXjuccLetaZjVVD9B7ezZp6Tn4SJBxmKBgBVTwYyga9q -> EUjxwLwjo5PgiFaGAUdhdqU3foXFWAF6J2zY2qjf3Hin
  ],
  transfers: [
    Transfer in slot 360045483 (order 868, ix 0, inner_ix Some(2))
      via inf69quFVZyuHEsrUXq3APtYLr4iqsNiQdCh5ArGcUp
      on TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA mint 7txRrYTGsnwQcHKKebtsUbcA3vTBwNUewMp75mNDtRmP
      Amount 76179182593033
      ATAs 8jNGEPu52WoD1khVHJA2sC2ZQDpKcPnE5wHJESku2aQS -> BXjuccLetaZjVVD9B7ezZp6Tn4SJBxmKBgBVTwYyga9q
  ],
  txs: [
    TransactionV2 { slot: 360045483, inclusion_order: 866, sig: "3y185hmf8ZpFiD2gsqmxr5Xjerri7Y2noGdjBVPLZUdF5wsz7DyETjxZkXRryj9s56FZNwQCdLCuVw81y9DV89M1", fee: 5000, cu_actual: 188971 },
    TransactionV2 { slot: 360045483, inclusion_order: 867, sig: "3mLDZaa4nWgsJdcEc7qmJtGuKDLJvULgMiMqQoAgCPx3Sj9THfPhzh4EKam7tTcTDyBxyGsmjqBufuqGarbGmXcQ", fee: 5000, cu_actual: 175076 },
    TransactionV2 { slot: 360045483, inclusion_order: 869, sig: "3ngZ6P8rJJYRQERvMESwKozBPKDgf23sJiEDvvbZQxFyXQdnp7n5CZkY7vn6bswQib1wb9pdg8FcjtiX8S53B6dz", fee: 5000, cu_actual: 68609 },
    TransactionV2 { slot: 360045483, inclusion_order: 870, sig: "4S2PJhoKWjzz1PqcsHYiyjPHByYLqAFK7TKAXkqFpXW56KicjWsfQX3PjJstw1Uvs4w8wwMAvByzv75udVJaDEam", fee: 5000, cu_actual: 85217 }
  ]
}
 */