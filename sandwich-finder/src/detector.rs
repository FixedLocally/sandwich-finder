use std::{collections::{HashMap, HashSet}, sync::Arc};

use mysql::{prelude::Queryable, Pool, Row, Value};
use uuid::Uuid;
use crate::events::{common::Timestamp, sandwich::SandwichCandidate, swap::SwapV2, transaction::TransactionV2, transfer::TransferV2};

pub const LEADER_GROUP_SIZE: u64 = 4; // slots per leader group

pub async fn get_events(conn: Pool, start_slot: u64, end_slot: u64) -> (Vec<SwapV2>, Vec<TransferV2>, Vec<TransactionV2>) {
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

pub async fn insert_sandwiches(pool: Pool, slot: u64, sandwiches: Arc<[SandwichCandidate]>) {
    let mut conn = pool.get_conn().unwrap();
    let args: Vec<_> = sandwiches.iter().flat_map(|s| {
        // deterministic id for each sandwich
        let name: Vec<u8> = [
            s.frontrun().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
            s.backrun().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
            s.victim().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
            s.transfers().iter().flat_map(|sw| sw.id().to_le_bytes()).collect::<Vec<_>>(),
        ].concat();
        // println!("name {}", hex::encode(&name));
        let uuid = &*Uuid::new_v5(&Uuid::NAMESPACE_DNS, &name).to_string();
        [
            s.frontrun().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("FRONTRUN")]).collect::<Vec<_>>(),
            s.backrun().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("BACKRUN")]).collect::<Vec<_>>(),
            s.victim().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("VICTIM")]).collect::<Vec<_>>(),
            s.transfers().iter().flat_map(|sw| vec![Value::from(uuid), Value::from(sw.id()), Value::from("TRANSFER")]).collect::<Vec<_>>(),
        ].concat()
    }).collect();
    if !args.is_empty() {
        let stmt = format!("insert into sandwiches (id, event_id, role) values {}", "(?, ?, ?),".repeat(args.len() / 3));
        let stmt = stmt.trim_end_matches(",").to_string();
        if let Err(r) = conn.exec_drop(stmt, args) {
            eprintln!("Failed to insert sandwiches for slots {} to {}: {}", slot, slot + LEADER_GROUP_SIZE - 1, r);
            eprintln!("{:?}", sandwiches);
        }
    }
}