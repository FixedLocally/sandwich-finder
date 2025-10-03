use std::env;

use sandwich_finder::events::event::start_event_processor;
use tokio::join;


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
    let mut receiver = start_event_processor(grpc_url, rpc_url);
    println!("Started event processor");
    while let Some(event) = receiver.recv().await {
        println!("Received batch: {:?}", event.len());
        // process event here
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
