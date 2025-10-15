use std::{collections::HashSet, sync::Arc};

use dashmap::DashMap;
use derive_getters::Getters;
use mysql::{prelude::Queryable as _, Pool, Row, TxOpts, Value};
use serde::Serialize;
use uuid::Uuid;

use crate::{detector::LEADER_GROUP_SIZE, events::{event::Event, sandwich::SandwichCandidate}};

#[derive(Debug, Clone, Copy, Getters, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Timestamp {
    slot: u64,
    inclusion_order: u32,
    ix_index: u32,
    inner_ix_index: Option<u32>,
}

impl Timestamp {
    pub fn new(slot: u64, inclusion_order: u32, ix_index: u32, inner_ix_index: Option<u32>) -> Self {
        Self {
            slot,
            inclusion_order,
            ix_index,
            inner_ix_index,
        }
    }
}

#[derive(Clone)]
pub struct Inserter {
    pool: Pool,
    address_lookup_table: Arc<DashMap<Arc<str>, u32>>,
}

impl Inserter {
    pub fn new(pool: Pool) -> Self {
        let address_lookup_table = Arc::from(DashMap::new());
        address_lookup_table.insert(Arc::from(""), 0);
        Self {
            pool: pool.clone(),
            address_lookup_table,
        }
    }

    /// Also caches the corresponding ids in the address_lookup_table
    fn insert_addresses(&mut self, addresses: Arc<[&str]>) {
        if addresses.is_empty() {
            return;
        }
        let mut conn = self.pool.get_conn().unwrap();
        for batch in addresses.chunks(1000) {
            let args: Vec<_> = batch.iter().map(|&addr| vec![Value::from(addr)]).flatten().collect();
            let stmt = format!("insert ignore into address_lookup_table (address) values {}", "(?),".repeat(batch.len()));
            let stmt = stmt.trim_end_matches(",").to_string();
            conn.exec_drop(stmt, args).unwrap();
        }
        self.retrieve_addresses(addresses);
    }

    fn retrieve_addresses(&mut self, addresses: Arc<[&str]>) {
        let mut conn = self.pool.get_conn().unwrap();
        let args: Vec<_> = addresses.iter().map(|&addr| Value::from(addr)).collect();
        let stmt = format!("select id, address from address_lookup_table where address in ({})", "?,".repeat(addresses.len()).trim_end_matches(","));
        let res: Vec<Row> = conn.exec(stmt, args).unwrap();
        for row in res {
            let id: u32 = row.get("id").unwrap();
            let address: Arc<str> = row.get("address").unwrap();
            self.address_lookup_table.insert(address, id);
        }
    }

    fn to_event_vec(&self, event: &Event) -> Vec<Value> {
        match event {
            Event::Swap(swap) => vec![
                Value::from("SWAP"),
                Value::from(swap.slot()),
                Value::from(swap.inclusion_order()),
                Value::from(swap.ix_index()),
                Value::from(swap.inner_ix_index()),
                Value::from(self.address_lookup_table.get(swap.authority()).map(|v| *v.value()).unwrap()),
                Value::from(swap.outer_program().clone().map(|p| self.address_lookup_table.get(&p).map(|v| *v.value()).unwrap())),
                Value::from(self.address_lookup_table.get(swap.program()).map(|v| *v.value()).unwrap()),
                Value::from(self.address_lookup_table.get(swap.amm()).map(|v| *v.value()).unwrap()),
                Value::from(self.address_lookup_table.get(swap.input_mint()).map(|v| *v.value()).unwrap()),
                Value::from(self.address_lookup_table.get(swap.output_mint()).map(|v| *v.value()).unwrap()),
                Value::from(swap.input_amount()),
                Value::from(swap.output_amount()),
                Value::from(self.address_lookup_table.get(swap.input_ata()).map(|v| *v.value()).unwrap()),
                Value::from(self.address_lookup_table.get(swap.output_ata()).map(|v| *v.value()).unwrap()),
                Value::from(swap.input_inner_ix_index()),
                Value::from(swap.output_inner_ix_index()),
            ],
            Event::Transfer(transfer) => vec![
                Value::from("TRANSFER"),
                Value::from(transfer.slot()),
                Value::from(transfer.inclusion_order()),
                Value::from(transfer.ix_index()),
                Value::from(transfer.inner_ix_index()),
                Value::from(self.address_lookup_table.get(transfer.authority()).map(|v| *v.value()).unwrap()),
                Value::from(transfer.outer_program().clone().map(|p| self.address_lookup_table.get(&p).map(|v| *v.value()).unwrap())),
                Value::from(self.address_lookup_table.get(transfer.program()).map(|v| *v.value()).unwrap()),
                Value::from(None::<String>), // amm is None for transfer
                Value::from(self.address_lookup_table.get(transfer.mint()).map(|v| *v.value()).unwrap()),
                Value::from(self.address_lookup_table.get(transfer.mint()).map(|v| *v.value()).unwrap()),
                Value::from(transfer.amount()),
                Value::from(transfer.amount()),
                Value::from(self.address_lookup_table.get(transfer.input_ata()).map(|v| *v.value()).unwrap()),
                Value::from(self.address_lookup_table.get(transfer.output_ata()).map(|v| *v.value()).unwrap()),
                Value::from(transfer.inner_ix_index()),
                Value::from(transfer.inner_ix_index()),
            ],
            Event::Transaction(_) => vec![], // They belong to another table
        }
    }

    fn to_tx_vec(&self, event: &Event) -> Vec<Value> {
        match event {
            Event::Transaction(tx) => vec![
                Value::from(tx.slot()),
                Value::from(tx.inclusion_order()),
                Value::from(tx.sig()),
                Value::from(tx.fee()),
                Value::from(tx.cu_actual()),
            ],
            _ => vec![], // They belong to another table
        }
    }

    pub async fn insert_sandwiches(&mut self, slot: u64, sandwiches: Arc<[SandwichCandidate]>) {
        let mut conn = self.pool.get_conn().unwrap();
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

    pub async fn insert_events(&mut self, events: &[Event]) {
        let conn = &mut self.pool.get_conn().unwrap();
        let mut tx = conn.start_transaction(TxOpts::default()).unwrap();
        // 5, 6, 7, 8, 9, 10, 13, 14
        let addresses = events.iter().map(|e| {
            match e {
                Event::Swap(s) => vec![
                    s.authority().as_ref(),
                    s.outer_program().as_ref().map(|s| s.as_ref()).unwrap_or(""),
                    s.program().as_ref(),
                    s.amm().as_ref(),
                    s.input_mint().as_ref(),
                    s.output_mint().as_ref(),
                    s.input_ata().as_ref(),
                    s.output_ata().as_ref(),
                ],
                Event::Transfer(t) => vec![
                    t.authority().as_ref(),
                    t.outer_program().as_ref().map(|s| s.as_ref()).unwrap_or(""),
                    t.program().as_ref(),
                    t.mint().as_ref(),
                    t.input_ata().as_ref(),
                    t.output_ata().as_ref(),
                ],
                _ => vec![],
            }
        }).flatten().filter(|&s| !s.is_empty()).collect::<HashSet<_>>();
        self.insert_addresses(addresses.into_iter().collect());
        let event_vecs = events.iter().map(|e| self.to_event_vec(e)).collect::<Vec<_>>();
        let event_params: Vec<_> = event_vecs.iter().flat_map(|e| e).collect();
        let event_stmt = format!("insert into events_with_id (event_type, slot, inclusion_order, ix_index, inner_ix_index, authority_id, outer_program_id, program_id, amm_id, input_mint_id, output_mint_id, input_amount, output_amount, input_ata_id, output_ata_id, input_inner_ix_index, output_inner_ix_index) values {}", "(?, ?, ?, ?, ifnull(?, -1), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ifnull(?, -1), ifnull(?, -1)),".repeat(event_params.len() / 17));
        let tx_params: Vec<_> = events.iter().flat_map(|e| self.to_tx_vec(e)).collect();
        let tx_stmt = format!("insert into transactions (slot, inclusion_order, sig, fee, cu_actual) values {}", "(?, ?, ?, ?, ?),".repeat(tx_params.len() / 5));
        if !event_params.is_empty() {
            tx.exec_drop(event_stmt.trim_end_matches(","), event_params).unwrap();
        }
        if !tx_params.is_empty() {
            tx.exec_drop(tx_stmt.trim_end_matches(","), tx_params).unwrap();
        }
        tx.commit().unwrap();
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_timestamp_ordering() {
        let t1 = Timestamp::new(1, 0, 0, None);
        let t2 = Timestamp::new(1, 0, 1, None);
        let t3 = Timestamp::new(1, 1, 0, None);
        let t4 = Timestamp::new(2, 0, 0, None);
        let t5 = Timestamp::new(2, 0, 0, Some(0));
        let t6 = Timestamp::new(2, 0, 0, Some(1));

        assert!(t1 < t2);
        assert!(t2 < t3);
        assert!(t3 < t4);
        assert!(t4 < t5);
        assert!(t5 < t6);
    }
}