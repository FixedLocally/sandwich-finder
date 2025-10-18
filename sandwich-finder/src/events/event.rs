use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use debug_print::debug_println;
use futures::{SinkExt as _, StreamExt as _};
use serde::Serialize;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount}, bs58, commitment_config::CommitmentConfig};
use tokio::sync::mpsc;
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks, SubscribeRequestPing}, tonic::transport::Endpoint};

use crate::{events::{addresses::{DONT_FRONT_END, DONT_FRONT_START}, swap::SwapV2, swaps::{apesu::ApesuSwapFinder, aqua::AquaSwapFinder, discoverer::Discoverer, dooar::DooarSwapFinder, fluxbeam::FluxbeamSwapFinder, goonfi::GoonFiSwapFinder, humidifi::HumidiFiSwapFinder, jup_order_engine::JupOrderEngineSwapFinder, jup_perps::JupPerpsSwapFinder, lifinity_v2::LifinityV2SwapFinder, meteora::MeteoraSwapFinder, meteora_damm_v2::MeteoraDammV2Finder, meteora_dbc::MeteoraDBCSwapFinder, meteora_dlmm::MeteoraDLMMSwapFinder, onedex::OneDexSwapFinder, openbook_v2::OpenbookV2SwapFinder, pancake_swap::PancakeSwapSwapFinder, pumpamm::PumpAmmSwapFinder, pumpfun::PumpFunSwapFinder, pumpup::PumpupSwapFinder, raydium_cl::RaydiumCLSwapFinder, raydium_lp::RaydiumLPSwapFinder, raydium_v4::RaydiumV4SwapFinder, raydium_v5::RaydiumV5SwapFinder, saros_dlmm::SarosDLMMSwapFinder, solfi::SolFiSwapFinder, stabble_weighted::StabbleWeightedSwapFinder, sugar::SugarSwapFinder, sv2e::Sv2eSwapFinder, swap_finder_ext::SwapFinderExt as _, tessv::TessVSwapFinder, whirlpool::{WhirlpoolSwapFinder, WhirlpoolTwoHopSwapFinder1, WhirlpoolTwoHopSwapFinder2, WhirlpoolTwoHopSwapV2Finder1, WhirlpoolTwoHopSwapV2Finder2}, zerofi::ZeroFiSwapFinder}, transaction::TransactionV2, transfer::TransferV2, transfers::{stake::StakeProgramTransferfinder, system::SystemProgramTransferfinder, token::TokenProgramTransferFinder, transfer_finder_ext::TransferFinderExt as _}}, utils::{decompile_tx, pubkey_from_slice}};


#[derive(Clone, Debug, Serialize)]
pub enum Event {
    Swap(SwapV2),
    Transfer(TransferV2),
    Transaction(TransactionV2),
}

pub fn start_event_processor(grpc_url: String, rpc_url: String) -> mpsc::Receiver<(u64, Arc<[Event]>)> {
    // Initialize event processing system
    let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::processed());
    let lut_cache = DashMap::new();
    let (sender, receiver) = mpsc::channel::<_>(100);
    tokio::spawn(async move {
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
                    let mut events = vec![];
                    block_txs.iter().for_each(|tx| {
                        // println!("processing tx {} in slot {}", bs58::encode(&tx.0.signature).into_string(), slot);
                        let swaps: Vec<Event> = [
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
                            GoonFiSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            SugarSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            TessVSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            Sv2eSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            LifinityV2SwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            ApesuSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            OneDexSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            AquaSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            StabbleWeightedSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            JupPerpsSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            DooarSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                            PumpupSwapFinder::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2),
                        ].concat().into_iter().map(|s| Event::Swap(s)).collect();
                        let transfers: Vec<Event> = [
                            SystemProgramTransferfinder::find_transfers_in_tx(slot, tx.0, &tx.1, &tx.2),
                            TokenProgramTransferFinder::find_transfers_in_tx(slot, tx.0, &tx.1, &tx.2),
                            StakeProgramTransferfinder::find_transfers_in_tx(slot, tx.0, &tx.1, &tx.2),
                        ].concat().into_iter().map(|t| Event::Transfer(t)).collect();
                        if swaps.is_empty() {
                            let swaps = Discoverer::find_swaps_in_tx(slot, tx.0, &tx.1, &tx.2);
                            if !swaps.is_empty() {
                                println!("[Discoverer] tx {} ix #{} in slot {} triggered program {}", bs58::encode(&tx.0.signature).into_string(), swaps[0].ix_index(), slot, swaps[0].program());
                                debug_println!("{:?}", &tx);
                            }
                        }
                        let mut tx_events = swaps;
                        tx_events.extend(transfers);
                        // println!("found {} swaps in slot {} tx {}", swaps.len(), slot, bs58::encode(&tx.0.signature).into_string());
                        // println!("found {} transfers in slot {} tx {}", transfers.len(), slot, bs58::encode(&tx.0.signature).into_string());
                        // println!("{:?}", swaps);
                        if tx_events.len() > 0 {
                            let dont_front = tx.2.iter().any(|k| k.to_bytes() >= DONT_FRONT_START && k.to_bytes() < DONT_FRONT_END);
                            if let Some(meta) = &tx.0.meta {
                                tx_events.push(Event::Transaction(TransactionV2::new(
                                    slot,
                                    tx.0.index as u32,
                                    bs58::encode(&tx.0.signature).into_string().into(),
                                    meta.fee,
                                    meta.compute_units_consumed.unwrap_or(0),
                                    dont_front,
                                )));
                            } else {
                                tx_events.push(Event::Transaction(TransactionV2::new(
                                    slot,
                                    tx.0.index as u32,
                                    bs58::encode(&tx.0.signature).into_string().into(),
                                    0,
                                    0,
                                    dont_front,
                                )));
                            }
                        }
                        events.extend(tx_events);
                    });
                    let event_len = events.len();
                    tokio::spawn({
                        let sender = sender.clone();
                        async move {
                            let _ = sender.send((slot, events.into())).await;
                            println!("sent {} events from slot {}", event_len, slot);
                        }
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
        println!("event processor grpc stream ended");
    });
    return receiver;
}