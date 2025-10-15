use std::{collections::HashMap, env};

use futures::{SinkExt as _, StreamExt};
use sandwich_finder::{detector::{get_events, LEADER_GROUP_SIZE}, events::{common::Inserter, sandwich::detect}, utils::create_db_pool};
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterBlocksMeta, SubscribeRequestPing}, tonic::transport::Endpoint};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let pool = create_db_pool();
    let inserter = Inserter::new(pool.clone());

    let grpc_url = env::var("GRPC_URL").expect("GRPC_URL is not set");
    println!("connecting to grpc server: {}", grpc_url);
    let mut grpc_client = GeyserGrpcBuilder{
        endpoint: Endpoint::from_shared(grpc_url.to_string()).unwrap(),
        x_token: None,
        x_request_snapshot: false,
        send_compressed: None,
        accept_compressed: None,
        max_decoding_message_size: Some(128 * 1024 * 1024),
        max_encoding_message_size: None,
    }.connect().await.expect("cannon connect to grpc server");
    println!("connected to grpc server!");
    let mut slots = HashMap::new();
    slots.insert("client".to_string(), SubscribeRequestFilterBlocksMeta {});
    let (mut sink, mut stream) = grpc_client.subscribe_with_request(Some(SubscribeRequest {
        blocks_meta: slots,
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
            Some(UpdateOneof::BlockMeta(meta)) => {
                // println!("{:?}", meta);
                let slot = meta.slot;
                if meta.slot % 4 == 3 {
                    let pool = pool.clone();
                    let mut inserter = inserter.clone();
                    tokio::spawn(async move {
                        // Intentionally lag behind slightly to ensure all events are inserted
                        let start_slot = slot - 2 * LEADER_GROUP_SIZE + 1;
                        let end_slot = slot - LEADER_GROUP_SIZE;
                        println!("Processing slots {} - {}", start_slot, end_slot);
                        let (swaps, transfers, txs) = get_events(pool.clone(), start_slot, end_slot).await;
                        let sandwiches = detect(&swaps, &transfers, &txs);
                        println!("Found {} sandwiches in slots {} - {}", sandwiches.len(), start_slot, end_slot);
                        inserter.insert_sandwiches(start_slot, sandwiches).await;
                    });
                }
            },
            Some(UpdateOneof::Ping(_)) => {
                let _ = sink.send(SubscribeRequest {
                    ping: Some(SubscribeRequestPing {id: 1}),
                    ..Default::default()
                }).await;
            },
            _ => {},
        }
    }
}