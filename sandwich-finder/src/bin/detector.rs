use std::sync::{atomic::{AtomicU64, Ordering}, Arc};

use mysql::{prelude::Queryable, Value};
use sandwich_finder::{detector::{get_events, LEADER_GROUP_SIZE}, events::sandwich::detect, utils::create_db_pool};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use uuid::Uuid;

const MAX_CHUNK_SIZE: u64 = 1000; // max slots to fetch at a time

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
                    let name: Vec<u8> = [
                        s.frontrun().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                        s.backrun().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                        s.victim().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                        s.transfers().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
                    ].concat();
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
                    if let Err(r) = conn.exec_drop(stmt, &args) {
                        eprintln!("Failed to insert sandwiches for slots {} to {}: {}", slot, slot + LEADER_GROUP_SIZE - 1, r);
                    }
                }
            }
        });
        if set.len() >= 16 {
            set.join_next().await;
        }
    }
    set.join_all().await;
}
