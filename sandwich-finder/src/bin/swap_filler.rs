use std::{collections::HashMap, env};

use dashmap::DashMap;
use futures::{SinkExt as _, StreamExt as _};
use sandwich_finder::{swaps::{discoverer::Discoverer, finder::SwapFinderExt as _, fluxbeam::FluxbeamSwapFinder, humidifi::HumidiFiSwapFinder, jup_order_engine::JupOrderEngineSwapFinder, meteora::MeteoraSwapFinder, meteora_damm_v2::MeteoraDammV2Finder, meteora_dbc::MeteoraDBCSwapFinder, meteora_dlmm::MeteoraDLMMSwapFinder, openbook_v2::OpenbookV2SwapFinder, pancake_swap::PancakeSwapSwapFinder, pumpamm::PumpAmmSwapFinder, pumpfun::PumpFunSwapFinder, raydium_cl::RaydiumCLSwapFinder, raydium_lp::RaydiumLPSwapFinder, raydium_v4::RaydiumV4SwapFinder, raydium_v5::RaydiumV5SwapFinder, saros_dlmm::SarosDLMMSwapFinder, solfi::SolFiSwapFinder, whirlpool::{WhirlpoolSwapFinder, WhirlpoolTwoHopSwapFinder1, WhirlpoolTwoHopSwapFinder2, WhirlpoolTwoHopSwapV2Finder1, WhirlpoolTwoHopSwapV2Finder2}, zerofi::ZeroFiSwapFinder}, utils::pubkey_from_slice};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{account::ReadableAccount as _, address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount}, bs58, commitment_config::CommitmentConfig, instruction::{AccountMeta, Instruction}, pubkey::Pubkey};
use tokio::join;
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks, SubscribeRequestPing, SubscribeUpdateTransactionInfo}, tonic::transport::Endpoint};


fn resolve_lut_lookups(lut_cache: &DashMap<Pubkey, AddressLookupTableAccount>, msg: &yellowstone_grpc_proto::prelude::Message) -> (Vec<Pubkey>, Vec<Pubkey>) {
    let mut writable: Vec<Pubkey> = Vec::new();
    let mut readonly: Vec<Pubkey> = Vec::new();
    msg.address_table_lookups.iter().for_each(|table_lookup| {
        let lut_key = pubkey_from_slice(&table_lookup.account_key[0..32]);
        // find the correct lut account
        let lut = lut_cache.get(&lut_key).expect("lut not found");

        table_lookup.writable_indexes.iter().for_each(|index| {
            writable.push(lut.addresses[*index as usize]);
        });

        table_lookup.readonly_indexes.iter().for_each(|index| {
            readonly.push(lut.addresses[*index as usize]);
        });
    });

    (writable, readonly)
}

async fn decompile_tx<'a>(raw_tx: &'a SubscribeUpdateTransactionInfo, rpc_client: &RpcClient, lut_cache: &DashMap<Pubkey, AddressLookupTableAccount>) -> Option<(&'a SubscribeUpdateTransactionInfo, Vec<Instruction>, Vec<Pubkey>)> {
    if let Some(tx) = &raw_tx.transaction {
        if let Some(meta) = &raw_tx.meta {
            if meta.err.is_some() {
                // skip errored transactions
                return None;
            }
            if let Some(msg) = &tx.message {
                if let Some(header) = &msg.header {
                    let lut_keys = msg.address_table_lookups.iter().map(|lut| {
                        pubkey_from_slice(&lut.account_key[0..32])
                    }).collect::<Vec<Pubkey>>();

                    // get the uncached lut accounts, deserialize them and cache them
                    let uncached_luts = lut_keys.iter().filter(|lut_key| !lut_cache.contains_key(lut_key)).map(|x| *x).collect::<Vec<Pubkey>>();
                    if !uncached_luts.is_empty() {
                        let accounts = rpc_client.get_multiple_accounts(uncached_luts.as_slice()).await.expect("unable to get accounts");
                        accounts.iter().enumerate().for_each(|(i, account)| {
                            if let Some(account) = account {
                                let lut = AddressLookupTable::deserialize(&account.data()).expect("unable to deserialize account");
                                lut_cache.insert(uncached_luts[i], AddressLookupTableAccount {
                                    key: uncached_luts[i],
                                    addresses: lut.addresses.to_vec(),
                                });
                            }
                        });
                    }

                    // resolve lookups
                    let (writable, readonly) = resolve_lut_lookups(&lut_cache, &msg);
                    let num_signed_accts = header.num_required_signatures as usize;
                    let num_static_keys = msg.account_keys.len();
                    let num_writable_lut_keys = writable.len();

                    let mut account_keys: Vec<Pubkey> = msg.account_keys.iter().map(|key| pubkey_from_slice(key)).collect();
                    account_keys.extend(writable);
                    account_keys.extend(readonly);

                    // repackage into legacy ixs
                    let ixs = msg.instructions.iter().map(|ix| {
                        let program_id = account_keys[ix.program_id_index as usize];
                        let accounts = ix.accounts.iter().enumerate().map(|(i, index)| {
                            let is_signer = i < num_signed_accts;
                            let is_writable = if i >= num_static_keys {
                                i - num_static_keys < num_writable_lut_keys
                            } else if i >= num_signed_accts {
                                i - num_signed_accts < num_static_keys - num_signed_accts - header.num_readonly_unsigned_accounts as usize
                            } else {
                                i < num_signed_accts - header.num_readonly_signed_accounts as usize
                            };
                            AccountMeta {
                                pubkey: account_keys[*index as usize],
                                is_signer,
                                is_writable,
                            }
                        }).collect::<Vec<AccountMeta>>();
                        Instruction {
                            program_id,
                            accounts,
                            data: ix.data.clone(),
                        }
                    }).collect::<Vec<Instruction>>();
                    return Some((raw_tx, ixs, account_keys));
                }
            }
        }
    }
    None
}

async fn swap_finder() {
    loop {
        swap_finder_loop().await;
        // reconnect in 5secs
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn swap_finder_loop() {
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
                // let now = std::time::Instant::now();
                // let ts = block.block_time.unwrap().timestamp;
                let slot = block.slot;
                let futs = block.transactions.iter().filter_map(|tx| {
                    if tx.is_vote {
                        None
                    } else {
                        Some(decompile_tx(tx, &rpc_client, &lut_cache))
                    }
                }).collect::<Vec<_>>();
                let joined_futs = futures::future::join_all(futs).await;
                let block_txs = joined_futs.iter().filter_map(|tx| {
                    if let Some(tx) = tx {
                        Some(tx)
                    } else {
                        None
                    }
                }).collect::<Vec<_>>();
                // let swap_count = block_txs.iter().map(|tx| tx.swaps().len()).sum::<usize>();
                // block_txs.sort_by_key(|x| x.order());
                block_txs.iter().for_each(|tx| {
                    // println!("processing tx {} in slot {}", bs58::encode(&tx.0.signature).into_string(), slot);
                    let swaps = [
                        RaydiumV4SwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        RaydiumV5SwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        RaydiumLPSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        RaydiumCLSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        PumpFunSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        PumpAmmSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        WhirlpoolSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        WhirlpoolTwoHopSwapFinder1::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        WhirlpoolTwoHopSwapFinder2::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        WhirlpoolTwoHopSwapV2Finder1::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        WhirlpoolTwoHopSwapV2Finder2::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        MeteoraDLMMSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        MeteoraSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        MeteoraDBCSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        MeteoraDammV2Finder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        OpenbookV2SwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        ZeroFiSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        JupOrderEngineSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        PancakeSwapSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        FluxbeamSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        HumidiFiSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        SarosDLMMSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        SolFiSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                    ].concat();
                    if swaps.is_empty() {
                        let swaps = Discoverer::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2);
                        if swaps.is_empty() {
                            return;
                        }
                        println!("[Discoverer] tx {} ix #{} in slot {} triggered program {}", bs58::encode(&tx.0.signature).into_string(), swaps[0].ix_index(), slot, swaps[0].program());
                        return;
                    }
                    println!("found {} swaps in slot {} tx {}", swaps.len(), slot, bs58::encode(&tx.0.signature).into_string());
                    println!("{:?}", swaps);
                });
                
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


#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // let db_pool = create_db_pool();
    join!(
        tokio::spawn(swap_finder()),
    ).0.unwrap();
}
