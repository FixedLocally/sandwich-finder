use sandwich_finder::utils::{block_stats, create_db_pool, decompile, find_sandwiches, pubkey_from_slice, DbMessage, DecompiledTransaction, Sandwich, Swap, SwapType};
use std::{collections::{HashMap, VecDeque}, env, net::SocketAddr, sync::{Arc, RwLock}, vec};
use axum::{extract::{ws::{Message, WebSocket}, Path, State, WebSocketUpgrade}, response::IntoResponse, routing::get, Json, Router};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use mysql::{prelude::Queryable, Pool, TxOpts, Value};

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount}, commitment_config::CommitmentConfig};
use tokio::sync::{broadcast, mpsc};
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequestFilterAccounts, SubscribeRequestPing}, prelude::{SubscribeRequest, SubscribeRequestFilterBlocks}, tonic::transport::Endpoint};

#[derive(Clone)]
struct AppState {
    message_history: Arc<RwLock<VecDeque<Sandwich>>>,
    sender: broadcast::Sender<Sandwich>,
    pool: Pool,
}

async fn sandwich_finder(sender: mpsc::Sender<Sandwich>, db_sender: mpsc::Sender<DbMessage>) {
    loop {
        sandwich_finder_loop(sender.clone(), db_sender.clone()).await;
        // reconnect in 5secs
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn sandwich_finder_loop(sender: mpsc::Sender<Sandwich>, db_sender: mpsc::Sender<DbMessage>) {
    let rpc_url = env::var("RPC_URL").expect("RPC_URL is not set");
    let grpc_url = env::var("GRPC_URL").expect("GRPC_URL is not set");
    let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::processed());
    let lut_cache = DashMap::new();
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
    let mut blocks = HashMap::new();
    blocks.insert("client".to_string(), SubscribeRequestFilterBlocks {
        account_include: vec![],
        include_transactions: Some(true),
        include_accounts: Some(true),
        include_entries: Some(false),
    });
    let mut accounts = HashMap::new();
    accounts.insert("client".to_string(), SubscribeRequestFilterAccounts {
        account: vec![],
        owner: vec!["AddressLookupTab1e1111111111111111111111111".to_string()],
        filters: vec![],
        nonempty_txn_signature: Some(true),
    });
    let (mut sink, mut stream) = grpc_client.subscribe_with_request(Some(SubscribeRequest {
        accounts,
        blocks,
        commitment: Some(CommitmentLevel::Confirmed as i32),
        ..Default::default()
    })).await.expect("unable to subscribe");
    println!("subscription request sent!");
    while let Some(msg) = stream.next().await {
        if msg.is_err() {
            println!("grpc error: {:?}", msg.err());
            break;
        }
        let msg = msg.unwrap();
        match msg.update_oneof {
            Some(UpdateOneof::Block(block)) => {
                // println!("new block {}, {} txs", block.slot, block.transactions.len());
                let now = std::time::Instant::now();
                let ts = block.block_time.unwrap().timestamp;
                let slot = block.slot;
                let mut bundle_count = 0;
                db_sender.send(block_stats(&block)).await.unwrap();
                let futs = block.transactions.iter().filter_map(|tx| {
                    if tx.is_vote {
                        None
                    } else {
                        Some(decompile(tx, &rpc_client, &lut_cache))
                    }
                }).collect::<Vec<_>>();
                let joined_futs = futures::future::join_all(futs).await;
                let mut block_txs = joined_futs.iter().filter_map(|tx| {
                    if let Some(tx) = tx {
                        Some(tx)
                    } else {
                        None
                    }
                }).collect::<Vec<&DecompiledTransaction>>();
                let swap_count = block_txs.iter().map(|tx| tx.swaps().len()).sum::<usize>();
                block_txs.sort_by_key(|x| x.order());
                // criteria for sandwiches:
                // 1. has 3 txs of strictly increasing inclusion order (frontrun-victim-backrun)
                // 2. the 1st and 2nd are in the same direction, the 3rd is in reverse
                // 3. output of 3rd tx >= input of 1st tx && output of 1st tx >= input of 3rd tx (profitability constraint)
                // 4. all 3 txs use the same amm
                // 5. 2nd tx's swapper is different from the 1st and 3rd
                // 6. a wrapper program is present in the 1st and 3rd txs and are the same

                // group swaps by amm
                let mut amm_swaps: HashMap<&String, Vec<&Swap>> = HashMap::new();
                block_txs.iter().for_each(|tx| {
                    tx.swaps().iter().for_each(|swap| {
                        let swaps = amm_swaps.entry(swap.amm()).or_default();
                        swaps.push(swap);
                    });
                });

                // check #4
                amm_swaps.iter().for_each(|(_amm, swaps)| {
                    if swaps.len() < 3 {
                        return;
                    }
                    // within the group, further group by direction (input token)
                    let mut input_swaps: HashMap<&String, Vec<&Swap>> = HashMap::new();
                    swaps.iter().for_each(|swap| {
                        let input_swaps = input_swaps.entry(swap.input_mint()).or_default();
                        input_swaps.push(swap);
                    });
                    // bail out if there's not exactly 2 directions
                    if input_swaps.len() != 2 {
                        return;
                    }
                    let mut iter = input_swaps.iter();
                    let dir0 = iter.next().unwrap();
                    let dir1 = iter.next().unwrap();
                    // look for 0-0-1 sandwiches (check #2)
                    find_sandwiches(dir0.1, dir1.1, slot, ts).iter().for_each(|sandwich| {
                        let sender = sender.clone();
                        let db_sender = db_sender.clone();
                        let sandwich = sandwich.clone();
                        tokio::spawn(async move {
                            sender.send(sandwich.clone()).await.unwrap();
                            db_sender.send(DbMessage::Sandwich(sandwich)).await.unwrap();
                        });
                        bundle_count += 1;
                    });
                    // look for 1-1-0 sandwiches (check #2)
                    find_sandwiches(dir1.1, dir0.1, slot, ts).iter().for_each(|sandwich| {
                        let sender = sender.clone();
                        let db_sender = db_sender.clone();
                        let sandwich = sandwich.clone();
                        tokio::spawn(async move {
                            sender.send(sandwich.clone()).await.unwrap();
                            db_sender.send(DbMessage::Sandwich(sandwich)).await.unwrap();
                        });
                        bundle_count += 1;
                    });
                });
                if bundle_count >= 1 {
                    println!("block {} processed in {}us, {} swaps found, {} bundles found", block.slot, now.elapsed().as_micros(), swap_count, bundle_count);
                }
            }
            Some(UpdateOneof::Account(account)) => {
                if let Some(account_info) = account.account {
                    let lut = AddressLookupTable::deserialize(&account_info.data).expect("unable to deserialize account");
                    let key = pubkey_from_slice(&account_info.pubkey[0..32]);
                    // println!("lut updated: {:?}", key);
                    // refuse to shorten luts
                    if let Some(existing_entry) = lut_cache.get(&key) {
                        let existing_len = existing_entry.addresses.len();
                        if existing_len > lut.addresses.len() {
                            continue;
                        }
                    }
                    lut_cache.insert(key, AddressLookupTableAccount {
                        key,
                        addresses: lut.addresses.to_vec(),
                    });
                }
            }
            Some(UpdateOneof::Ping(_)) => {
                let _ = sink.send(SubscribeRequest {
                    ping: Some(SubscribeRequestPing {id: 1}),
                    ..Default::default()
                }).await;
            }
            _ => {}
        }
    }
}

async fn store_to_db(pool: Pool, mut receiver: mpsc::Receiver<DbMessage>) {
    let mut conn = pool.get_conn().unwrap();
    let insert_block_stmt = conn.prep("insert into block (slot, timestamp, tx_count, vote_count, reward_lamports, successful_cu, total_cu) values (?, ?, ?, ?, ?, ?, ?)").unwrap();
    let insert_tx_stmt = conn.prep("insert into transaction (tx_hash, signer, slot, order_in_block, dont_front) values (?, ?, ?, ?, ?)").unwrap();
    let insert_swap_stmt = conn.prep("insert into swap (sandwich_id, outer_program, inner_program, amm, subject, input_mint, output_mint, input_amount, output_amount, tx_id, swap_type) values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)").unwrap();

    let mut tx_db_id_cache: HashMap<String, u64> = HashMap::new();
    while let Some(msg) = receiver.recv().await {
        match msg {
            DbMessage::Block(block) => {
                conn.exec_drop(&insert_block_stmt, (block.slot(), block.ts(), block.tx_count(), block.vote_count(), block.reward_lamports(), block.successful_cu(), block.total_cu())).unwrap();
            }
            DbMessage::Sandwich(sandwich) => {
                let mut dbtx = conn.start_transaction(TxOpts::default()).unwrap();
                // obtain an id for this sandwich
                dbtx.query_drop("insert into sandwich values ()").unwrap();
                let sandwich_id = dbtx.last_insert_id();
                let mut swaps = Vec::new();
                swaps.push((sandwich.frontrun(), SwapType::Frontrun));
                swaps.extend(sandwich.victim().iter().map(|x| (x, SwapType::Victim)));
                swaps.push((sandwich.backrun(), SwapType::Backrun));
                // figure out which txs are new to the db
                let args: Vec<_> = swaps.iter().filter_map(|swap| {
                    if tx_db_id_cache.contains_key(swap.0.sig()) {
                        None
                    } else {
                        Some((swap.0.sig(), swap.0.signer(), sandwich.slot(), swap.0.order(), swap.0.dont_front()))
                    }
                }).collect();
                if !args.is_empty() {
                    dbtx.exec_batch(&insert_tx_stmt, &args).unwrap();
                    // populate the cache with a select
                    let tx_hashes = args.iter().map(|(tx_hash, _, _, _, _)| tx_hash).collect::<Vec<_>>();
                    let q_marks = tx_hashes.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    let stmt = dbtx.prep(format!("select id, tx_hash from transaction where tx_hash in ({q_marks})")).unwrap();
                    let _ = dbtx.exec_map(&stmt, tx_hashes, |(id, tx_hash)| {
                        tx_db_id_cache.insert(tx_hash, id);
                    }).unwrap();
                }
                // insert the swaps in this sandwich into the db
                dbtx.exec_batch(&insert_swap_stmt, swaps.iter().map(|swap| {
                    let tx_id = tx_db_id_cache.get(swap.0.sig()).unwrap();
                    (sandwich_id, swap.0.outer_program().as_deref(), swap.0.program().as_str(), swap.0.amm().as_str(), swap.0.subject().as_str(), swap.0.input_mint().as_str(), swap.0.output_mint().as_str(), swap.0.input_amount(), swap.0.output_amount(), tx_id, swap.1.clone())
                })).unwrap();
                dbtx.commit().unwrap();
            }
        }
    }
}

async fn handle_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
) {
    let mut receiver = state.sender.subscribe();
    while let Ok(msg) = receiver.recv().await {
        if socket.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() {
            break; // Client disconnected
        }
    }
}

async fn handle_history(State(state): State<AppState>) -> Json<Vec<Sandwich>> {
    let snapshot = {
        let history = state.message_history.try_read().unwrap();
        history.iter().cloned().collect()
    };
    Json(snapshot)
}

async fn handle_search_tx(State(state): State<AppState>, Path(txid): Path<String>) -> Json<Option<Sandwich>> {
    let mut conn = state.pool.get_conn().unwrap();
    // look for a valid sandwich
    let stmt = conn.prep("SELECT sandwich_id, (max(order_in_block)-min(order_in_block))/count(*) as ratio FROM `sandwich_view` v where sandwich_id in (select sandwich_id from sandwich_view where tx_hash=?) GROUP by sandwich_id order by ratio asc limit 1;").unwrap();
    let sandwich_id = conn.exec_first(&stmt, (txid,)).unwrap().map(|(sandwich_id, _): (u64, f64)| {
        sandwich_id
    });
    if sandwich_id.is_none() {
        return Json(None);
    }
    let stmt = conn.prep("SELECT tx_hash, signer, slot, timestamp, order_in_block, outer_program, inner_program, amm, subject, input_amount, input_mint, output_amount, output_mint, swap_type, dont_front FROM `sandwich_view` where sandwich_id = ?").unwrap();
    let mut frontrun = None;
    let mut victims = vec![];
    let mut backrun = None;
    let mut slot = 0;
    let mut ts = 0;
    let res = conn.exec_iter(&stmt, (sandwich_id.unwrap(),)).unwrap();
    for row in res {
        let row = row.unwrap();
        let tx_hash: String = row.get(0).unwrap();
        let signer: String = row.get(1).unwrap();
        let slot_: u64 = row.get(2).unwrap();
        let ts_: i64 = row.get(3).unwrap();
        let order_in_block: u64 = row.get(4).unwrap();
        let outer_program: Option<String> = row.get(5).unwrap();
        let inner_program: String = row.get(6).unwrap();
        let amm: String = row.get(7).unwrap();
        let subject: String = row.get(8).unwrap();
        let input_amount: u64 = row.get(9).unwrap();
        let input_mint: String = row.get(10).unwrap();
        let output_amount: u64 = row.get(11).unwrap();
        let output_mint: String = row.get(12).unwrap();
        let swap_type: String = row.get(13).unwrap();
        let dont_front: bool = match row.get(14).unwrap() {
            Value::Bytes(bytes) if bytes.len() == 1 => bytes[0] != 0,
            _ => false,
        };
        let swap = Swap::new(
            outer_program,
            inner_program,
            amm,
            signer,
            subject,
            input_mint,
            output_mint,
            input_amount,
            output_amount,
            order_in_block,
            tx_hash.clone(),
            dont_front,
        );
        slot = slot_;
        ts = ts_;
        match swap_type.into() {
            SwapType::Frontrun => frontrun = Some(swap),
            SwapType::Victim => victims.push(swap),
            SwapType::Backrun => backrun = Some(swap),
        };
    }
    if frontrun.is_some() && backrun.is_some() && !victims.is_empty() {
        let sandwich = Sandwich::new(
            slot,
            frontrun.unwrap(),
            victims,
            backrun.unwrap(),
            ts,
        );
        return Json(Some(sandwich));
    }

    Json(None)
}
async fn start_web_server(sender: broadcast::Sender<Sandwich>, message_history: Arc<RwLock<VecDeque<Sandwich>>>, pool: Pool) {
    let app = Router::new()
        .route("/", get(handle_websocket))
        .route("/history", get(handle_history))
        .route("/search/{txid}", get(handle_search_tx))
        .with_state(AppState {
            message_history,
            sender,
            pool,
        });
    let api_port = env::var("API_PORT").unwrap_or_else(|_| "11000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{api_port}"))
        .await
        .unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let db_pool = create_db_pool();
    let (sender, mut receiver) = mpsc::channel::<Sandwich>(100);
    let (db_sender, db_receiver) = mpsc::channel::<DbMessage>(100);
    tokio::spawn(sandwich_finder(sender, db_sender));
    let message_history = Arc::new(RwLock::new(VecDeque::<Sandwich>::with_capacity(100)));
    let (sender, _) = broadcast::channel::<Sandwich>(100);
    tokio::spawn(start_web_server(sender.clone(), message_history.clone(), db_pool.clone()));
    tokio::spawn(store_to_db(db_pool, db_receiver));
    while let Some(message) = receiver.recv().await {
        // println!("Received: {:?}", message);
        let mut hist = message_history.write().unwrap();
        if hist.len() == 100 {
            hist.pop_front();
        }
        hist.push_back(message.clone());
        drop(hist);
        let _ = sender.send(message);
    }
}