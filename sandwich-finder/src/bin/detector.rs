use std::{collections::{HashMap, HashSet}, sync::{atomic::{AtomicU64, Ordering}, Arc}};

use mysql::{prelude::Queryable, Pool, Row, Value};
use sandwich_finder::{events::{common::Timestamp, sandwich::detect, swap::SwapV2, transaction::TransactionV2, transfer::TransferV2}, utils::create_db_pool};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use uuid::Uuid;

const MAX_CHUNK_SIZE: u64 = 1000; // max slots to fetch at a time
const LEADER_GROUP_SIZE: u64 = 4; // slots per leader group

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
    let res: Vec<Row> = conn.exec("select id, event_type, slot, inclusion_order, ix_index, inner_ix_index, authority, outer_program, program, amm, input_mint, output_mint, input_amount, output_amount, input_ata, output_ata, input_inner_ix_index, output_inner_ix_index from events where slot between ? and ?", vec![start_slot, end_slot]).unwrap();
    let mut swaps = vec![];
    let mut transfers = vec![];
    let mut txs = vec![];
    for row in res {
        let id: u64 = row.get("id").unwrap();
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
                swaps.push(SwapV2::new(outer_program, program, authority, amm.unwrap(), input_mint, output_mint, input_amount, output_amount, input_ata, output_ata, input_inner_ix_index, output_inner_ix_index, slot, inclusion_order, ix_index, inner_ix_index, id));
            },
            "TRANSFER" => {
                transfers.push(TransferV2::new(outer_program, program, authority, input_mint, input_amount, input_ata, output_ata, slot, inclusion_order, ix_index, inner_ix_index, id));
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
    let start_slot = start_slot / LEADER_GROUP_SIZE * LEADER_GROUP_SIZE;
    let end_slot = end_slot / LEADER_GROUP_SIZE * LEADER_GROUP_SIZE + LEADER_GROUP_SIZE - 1;
    // fetch events for up to 1k slots at a time and process in groups of 4 slots
    let pool = create_db_pool();
    let chunk_size = ((end_slot - start_slot + 1) / 16).min(MAX_CHUNK_SIZE - LEADER_GROUP_SIZE) / LEADER_GROUP_SIZE * LEADER_GROUP_SIZE + LEADER_GROUP_SIZE;
    println!("Processing slots {} to {} ({} leader groups)", start_slot, end_slot, (end_slot - start_slot + 1) / LEADER_GROUP_SIZE);
    let progress = Arc::from(AtomicU64::new(0));
    let mut set = JoinSet::new();
    for chunk_start in (start_slot..=end_slot).step_by(chunk_size as usize) {
        let chunk_end = (chunk_start + chunk_size - 1).min(end_slot);
        let pool = pool.clone(); // docs said this is cloneable
        let progress = progress.clone();
        set.spawn(async move {
            println!("Fetching events for slots {} to {}", chunk_start, chunk_end);
            let (swaps, transfers, txs) = get_events(pool.clone(), chunk_start, chunk_end).await;
            let mut swaps_start = 0;
            let mut transfers_start = 0;
            let mut txs_start = 0;
            let mut conn = pool.get_conn().unwrap();
            for slot in (chunk_start..=chunk_end).step_by(LEADER_GROUP_SIZE as usize) {
                let swaps_end = swaps.iter().skip(swaps_start).position(|s| *s.slot() >= slot + LEADER_GROUP_SIZE).map(|n| n + swaps_start).unwrap_or(swaps.len());
                let transfers_end = transfers.iter().skip(transfers_start).position(|t| *t.slot() >= slot + LEADER_GROUP_SIZE).map(|n| n + transfers_start).unwrap_or(transfers.len());
                let txs_end = txs.iter().skip(txs_start).position(|t| *t.slot() >= slot + LEADER_GROUP_SIZE).map(|n| n + txs_start).unwrap_or(txs.len());

                let slot_swaps = &swaps[swaps_start..swaps_end];
                let slot_transfers = &transfers[transfers_start..transfers_end];
                let slot_txs = &txs[txs_start..txs_end];
                println!("Processing slots {} to {}", slot, slot + LEADER_GROUP_SIZE - 1);
                // println!("Swaps: {:#?}", slot_swaps.len());
                // println!("Transfers: {:#?}", slot_transfers.len());
                // println!("Txs: {:#?}", slot_txs.len());
                let sandwiches = detect(slot_swaps, slot_transfers, slot_txs);
                // for sandwich in sandwiches.iter() {
                //     println!("Detected sandwich: {:#?}", sandwich);
                // }

                let args: Vec<_> = sandwiches.iter().flat_map(|s| {
                    // deterministic id for each sandwich
                    let name: Vec<u8> = sandwiches.iter().flat_map(|s| {
                        [
                            s.frontrun().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                            s.backrun().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                            s.victim().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                            s.transfers().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                        ].concat()
                    }).collect();
                    let uuid = &*Uuid::new_v5(&Uuid::NAMESPACE_DNS, &name).to_string();
                    [
                        s.frontrun().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("FRONTRUN")]).collect::<Vec<_>>(),
                        s.backrun().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("BACKRUN")]).collect::<Vec<_>>(),
                        s.victim().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("VICTIM")]).collect::<Vec<_>>(),
                        s.transfers().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("TRANSFER")]).collect::<Vec<_>>(),
                    ].concat()
                }).collect();

                swaps_start = swaps_end;
                transfers_start = transfers_end;
                txs_start = txs_end;
                let completed = progress.fetch_add(1, Ordering::AcqRel);
                // if completed % 100 == 0 {
                    println!("{}/{}", completed, (end_slot - start_slot + 1) / LEADER_GROUP_SIZE);
                // }

                if !args.is_empty() {
                    let stmt = format!("insert into sandwiches (id, event_id, role) values {}", "(?, ?, ?),".repeat(args.len() / 3));
                    let stmt = stmt.trim_end_matches(",").to_string();
                    if let Err(r) = conn.exec_drop(stmt, args) {
                        eprintln!("Failed to insert sandwiches for slots {} to {}: {}", slot, slot + LEADER_GROUP_SIZE - 1, r);
                    }
                }
            }
        });
        if set.len() >= 16 {
            set.join_next().await;
        }
    }
}
