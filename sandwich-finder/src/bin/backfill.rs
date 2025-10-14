use std::env;

use sandwich_finder::utils::create_db_pool;
use solana_rpc_client_api::config::RpcBlockConfig;
use solana_rpc_client::nonblocking::rpc_client::{RpcClient};
use solana_sdk::commitment_config::CommitmentConfig;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // let pool = create_db_pool();
    let rpc_url = env::var("RPC_URL").expect("RPC_URL is not set");
    let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::processed());
    let mut args = env::args();
    args.next(); // argv[0]
    let slot: u64 = args.next().unwrap().parse().unwrap();
    let block = rpc_client.get_block_with_config(
        slot,
        RpcBlockConfig {
            encoding: None,
            // transaction_details: Some(TransactionDetails::Full),
            transaction_details: None,
            rewards: Some(true),
            commitment: Some(CommitmentConfig::finalized()),
            max_supported_transaction_version: Some(0)
        }).await;
    if let Ok(block) = block {
        println!("Block: {:?}", block);
        // Here you can add logic to process the block and backfill data into the database
        // For example, you might want to insert transactions or accounts into your database
    } else {
        println!("No block found for slot {} {}", slot, block.err().unwrap());
    }
}