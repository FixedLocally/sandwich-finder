use std::env;

use sandwich_finder::{events::{common::Inserter, event::start_event_processor}, utils::create_db_pool};
use tokio::join;

const CHUNK_SIZE: usize = 1000;

async fn indexer_loop() {
    loop {
        indexer().await;
        // reconnect in 5secs
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn indexer() {
    let rpc_url = env::var("RPC_URL").expect("RPC_URL is not set");
    let grpc_url = env::var("GRPC_URL").expect("GRPC_URL is not set");
    let pool = create_db_pool();
    let mut receiver = start_event_processor(grpc_url, rpc_url);
    let inserter = Inserter::new(pool.clone());
    println!("Started event processor");
    while let Some((_slot, event)) = receiver.recv().await {
        println!("Received batch: {:?}", event.len());
        // process event here
        let mut inserter = inserter.clone();
        tokio::spawn(async move {
            for chunk in event.chunks(CHUNK_SIZE) {
                inserter.insert_events(chunk).await;
            }
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
