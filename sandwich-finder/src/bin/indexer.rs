use std::env;

use mysql::{prelude::Queryable, TxOpts, Value};
use sandwich_finder::{events::event::{start_event_processor, Event}, utils::create_db_pool};
use tokio::join;

const CHUNK_SIZE: usize = 1000;

async fn indexer_loop() {
    loop {
        indexer().await;
        // reconnect in 5secs
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

fn event_to_vec(event: &Event) -> Vec<Value> {
    match event {
        Event::Swap(swap) => vec![
            Value::from("SWAP"),
            Value::from(swap.slot()),
            Value::from(swap.inclusion_order()),
            Value::from(swap.ix_index()),
            Value::from(swap.inner_ix_index()),
            Value::from(swap.authority()),
            Value::from(swap.outer_program()),
            Value::from(swap.program()),
            Value::from(swap.amm()),
            Value::from(swap.input_mint()),
            Value::from(swap.output_mint()),
            Value::from(swap.input_amount()),
            Value::from(swap.output_amount()),
            Value::from(swap.input_ata()),
            Value::from(swap.output_ata()),
            Value::from(swap.input_inner_ix_index()),
            Value::from(swap.output_inner_ix_index()),
        ],
        Event::Transfer(transfer) => vec![
            Value::from("TRANSFER"),
            Value::from(transfer.slot()),
            Value::from(transfer.inclusion_order()),
            Value::from(transfer.ix_index()),
            Value::from(transfer.inner_ix_index()),
            Value::from(transfer.authority()),
            Value::from(transfer.outer_program()),
            Value::from(transfer.program()),
            Value::from(None::<String>), // amm is None for transfer
            Value::from(transfer.mint()),
            Value::from(transfer.mint()),
            Value::from(transfer.amount()),
            Value::from(transfer.amount()),
            Value::from(transfer.input_ata()),
            Value::from(transfer.output_ata()),
            Value::from(transfer.inner_ix_index()),
            Value::from(transfer.inner_ix_index()),
        ],
        Event::Transaction(_) => vec![], // They belong to another table
    }
}


fn event_to_tx_vec(event: &Event) -> Vec<Value> {
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

async fn indexer() {
    let rpc_url = env::var("RPC_URL").expect("RPC_URL is not set");
    let grpc_url = env::var("GRPC_URL").expect("GRPC_URL is not set");
    let pool = create_db_pool();
    let mut receiver = start_event_processor(grpc_url, rpc_url);
    println!("Started event processor");
    while let Some((_slot, event)) = receiver.recv().await {
        println!("Received batch: {:?}", event.len());
        // process event here
        let mut conn = pool.get_conn().unwrap();
        tokio::spawn(async move {
            let mut tx = conn.start_transaction(TxOpts::default()).unwrap();
            for chunk in event.chunks(CHUNK_SIZE) {
                let event_params: Vec<_> = chunk.iter().flat_map(event_to_vec).collect();
                let event_stmt = format!("insert into events (event_type, slot, inclusion_order, ix_index, inner_ix_index, authority, outer_program, program, amm, input_mint, output_mint, input_amount, output_amount, input_ata, output_ata, input_inner_ix_index, output_inner_ix_index) values {}", "(?, ?, ?, ?, ifnull(?, -1), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ifnull(?, -1), ifnull(?, -1)),".repeat(event_params.len() / 17));
                let tx_params: Vec<_> = chunk.iter().flat_map(event_to_tx_vec).collect();
                let tx_stmt = format!("insert into transactions (slot, inclusion_order, sig, fee, cu_actual) values {}", "(?, ?, ?, ?, ?),".repeat(tx_params.len() / 5));
                if !event_params.is_empty() {
                    tx.exec_drop(event_stmt.trim_end_matches(","), event_params).unwrap();
                }
                if !tx_params.is_empty() {
                    tx.exec_drop(tx_stmt.trim_end_matches(","), tx_params).unwrap();
                }
            }
            tx.commit().unwrap();
        });
    }
    println!("Event processor disconnected");
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // let db_pool = create_db_pool();
    join!(
        tokio::spawn(indexer_loop()),
    ).0.unwrap();
}
