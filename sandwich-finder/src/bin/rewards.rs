use std::{collections::HashMap, env};

use futures::StreamExt as _;
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterBlocks}, tonic::transport::Endpoint};

#[tokio::main]
pub async fn main() {
    let grpc_url = env::var("GRPC_URL").expect("GRPC_URL is not set");
    let mut grpc_client = GeyserGrpcBuilder{
        endpoint: Endpoint::from_shared(grpc_url.to_string()).unwrap(),
        x_token: None,
        x_request_snapshot: false,
        send_compressed: None,
        accept_compressed: None,
        max_decoding_message_size: Some(128 * 1024 * 1024),
        max_encoding_message_size: None,
    }.connect().await.expect("cannon connect to grpc server");
    let mut blocks = HashMap::new();
    blocks.insert("client".to_string(), SubscribeRequestFilterBlocks {
        account_include: vec![],
        include_transactions: Some(false),
        include_accounts: Some(false),
        include_entries: Some(false),
    });
    let (mut sink, mut stream) = grpc_client.subscribe_with_request(Some(SubscribeRequest {
        blocks,
        commitment: Some(CommitmentLevel::Confirmed as i32),
        ..Default::default()
    })).await.expect("unable to subscribe");
    while let Some(msg) = stream.next().await {
        if msg.is_err() {
            println!("grpc error: {:?}", msg.err());
            break;
        }
        let msg = msg.unwrap();
        match msg.update_oneof {
            Some(UpdateOneof::Block(block)) => {
                println!("Rewards: {:?}", block.rewards);
            },
            _ => {}
        };
    }
}