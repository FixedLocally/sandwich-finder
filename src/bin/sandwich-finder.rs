use std::{collections::{HashMap, VecDeque}, fmt::Debug, net::SocketAddr, str::FromStr, sync::{Arc, Mutex}};
use axum::{extract::{ws::{Message, WebSocket}, State, WebSocketUpgrade}, response::IntoResponse, routing::get, Json, Router};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use serde::{ser::SerializeStruct, Serialize};

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{account::ReadableAccount, address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount}, bs58, commitment_config::CommitmentConfig, instruction::{AccountMeta, Instruction}, pubkey::Pubkey};
use tokio::sync::{broadcast, mpsc};
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequestFilterAccounts, SubscribeRequestPing, SubscribeUpdateTransactionInfo}, prelude::{InnerInstruction, InnerInstructions, SubscribeRequest, SubscribeRequestFilterBlocks, TransactionStatusMeta}, tonic::transport::Endpoint};

use sandwich_finder::loss_calc::Bundle;

const RAYDIUM_V4_PUBKEY: Pubkey = Pubkey::from_str_const("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
const RAYDIUM_V5_PUBKEY: Pubkey = Pubkey::from_str_const("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
const RAYDIUM_V4_PARAMS: (&str, f64, &[&f64]) = ("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", 0.88, &[&0.0025]);
const RAYDIUM_V5_PARAMS: (&str, f64, &[&f64]) = ("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C", 0.84, &[&0.0025, &0.01, &0.02, &0.04]);
// discriminant/amm_index/send_ix_index/recv_ix_index/data_len
// 09/1/+1/+2/17
// 8fbe5adac41e33de/3/+1/+2/24
// 37d96256a34ab4ad/3/+1/+2/24

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Swap {
    outer_program: Option<String>,
    program: String,
    amm: String,
    signer: String,
    subject: String,
    input_mint: String,
    output_mint: String,
    input_amount: u64,
    output_amount: u64,
    pool_pre_balances: (u64, u64),
    order: u64,
    sig: String,
}

#[derive(Debug, Clone)]
pub struct Sandwich {
    slot: u64,
    frontrun: Swap,
    victim: Swap,
    backrun: Swap,
    ts: i64,
    victim_loss: Option<Option<(u64, u64)>>,
}

impl Sandwich {
    pub fn new(slot: u64, frontrun: Swap, victim: Swap, backrun: Swap, ts: i64) -> Self {
        Self {
            slot,
            frontrun,
            victim,
            backrun,
            ts,
            victim_loss: None,
        }
    }

    pub fn estimate_victim_loss(&mut self) -> Option<(u64, u64)> {
        if let Some(victim_loss) = self.victim_loss {
            return victim_loss;
        }
        let params = match self.victim.program.as_str() {
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8" => RAYDIUM_V4_PARAMS,
            "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C" => RAYDIUM_V5_PARAMS,
            _ => unreachable!(),
        };
        let mut bundle = Bundle::new(
            self.frontrun.pool_pre_balances.0 as f64,
            self.frontrun.pool_pre_balances.1 as f64,
            self.frontrun.input_amount as f64,
            self.frontrun.output_amount as f64,
            self.victim.input_amount as f64,
            self.victim.output_amount as f64,
            self.backrun.input_amount as f64,
            self.backrun.output_amount as f64,
            0.003,
            params.1,
        );
        let victim_loss = if let Ok(()) = bundle.update_initial_balances(params.2) {
            Some(bundle.user_losses())
        } else {
            None
        };
        self.victim_loss = Some(victim_loss);
        victim_loss
    }
}

impl Serialize for Sandwich {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let mut state = serializer.serialize_struct("Sandwich", 6)?;
        state.serialize_field("slot", &self.slot)?;
        state.serialize_field("ts", &self.ts)?;
        state.serialize_field("frontrun", &self.frontrun)?;
        state.serialize_field("victim", &self.victim)?;
        state.serialize_field("backrun", &self.backrun)?;
        if let Some(Some((loss_a, loss_b))) = self.victim_loss {
            state.serialize_field("estLoss", &vec![loss_a, loss_b])?;
        } else {
            state.serialize_field("estLoss", &vec![0, 0])?;
        }
        state.end()
    }
}

impl Debug for Swap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("{\n")?;
        f.write_str(&format!("  outer_program: \"{:?}\",\n", self.outer_program))?;
        f.write_str(&format!("  program: \"{:?}\",\n", self.program))?;
        f.write_str(&format!("  amm: \"{:?}\",\n", self.amm))?;
        f.write_str(&format!("  signer: \"{:?}\",\n", self.signer))?;
        f.write_str(&format!("  subject: \"{:?}\",\n", self.subject))?;
        f.write_str(&format!("  input_mint: \"{:?}\",\n", self.input_mint))?;
        f.write_str(&format!("  output_mint: \"{:?}\",\n", self.output_mint))?;
        f.write_str(&format!("  input_amount: {},\n", self.input_amount))?;
        f.write_str(&format!("  output_amount: {},\n", self.output_amount))?;
        f.write_str(&format!("  order: {},\n", self.order))?;
        f.write_str(&format!("  sig: \"{}\",\n", self.sig))?;
        f.write_str("}")?;
        Ok(())
    }
}

pub struct DecompiledTransaction {
    sig: String,
    instructions: Vec<Instruction>,
    swaps: Vec<Swap>,
    payer: Pubkey,
    order: u64,
}

#[derive(Clone)]
struct AppState {
    message_history: Arc<Mutex<VecDeque<Sandwich>>>,
    sender: broadcast::Sender<Sandwich>,
}

fn pubkey_from_slice(slice: &[u8]) -> Pubkey {
    Pubkey::new_from_array(slice.try_into().expect("slice with incorrect length"))
}

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

fn find_transferred_token(ix: &InnerInstruction, meta: &TransactionStatusMeta, pool_is_sender: bool) -> Option<(Pubkey, u64, u64)> {
    let amount = u64::from_le_bytes(ix.data[1..9].try_into().expect("slice with incorrect length"));
    // transfer: 1/0; transferChecked: 2/0
    let (i1, i0) = match ix.data[0] {
        3 => (ix.accounts[1], ix.accounts[0]), // transfer
        12 => (ix.accounts[2], ix.accounts[0]), // transferChecked
        _ => return None,
    };
    let target_idx = if pool_is_sender { i0 } else { i1 };
    return meta.pre_token_balances.iter().filter(|x| x.account_index == target_idx as u32).map(|x| {
        (Pubkey::from_str(&x.mint).expect("invalid pubkey"), amount, x.ui_token_amount.as_ref().unwrap().amount.parse().unwrap())
    }).next();
}

fn find_swaps(ix: &Instruction, inner_ix: &InnerInstructions, swap_program: &Pubkey, discriminant: &[u8], amm_index: usize, send_ix_index: usize, recv_ix_index: usize, data_len: usize, meta: &TransactionStatusMeta, account_keys: &Vec<Pubkey>, sig: String, tx_index: u64) -> Vec<Swap> {
    let mut swaps: Vec<Swap> = Vec::new();
    // case 1
    if ix.program_id == *swap_program && ix.data.len() == data_len && ix.data[0..discriminant.len()] == *discriminant {
        let send_inner_ix = &inner_ix.instructions[send_ix_index - 1];
        let recv_inner_ix = &inner_ix.instructions[recv_ix_index - 1];
        let input = find_transferred_token(send_inner_ix, meta, false).unwrap();
        let output = find_transferred_token(recv_inner_ix, meta, true).unwrap();
        swaps.push(Swap {
            outer_program: None,
            program: ix.program_id.to_string(),
            amm: ix.accounts[amm_index].pubkey.to_string(),
            signer: account_keys[0].to_string(),
            subject: account_keys[send_inner_ix.accounts[2] as usize].to_string(),
            input_mint: input.0.to_string(),
            output_mint: output.0.to_string(),
            input_amount: input.1,
            output_amount: output.1,
            pool_pre_balances: (input.2, output.2),
            sig: sig.clone(),
            order: tx_index,
        });
    }
    // loop thru the inner ixs to find a swap
    inner_ix.instructions.iter().enumerate().for_each(|(j, inner)| {
        let program_id = account_keys[inner.program_id_index as usize];
        if program_id == *swap_program {
            if inner.data.len() != data_len || inner.data[0..discriminant.len()] != *discriminant {
                return; // not a swap
            }
            let send_inner_ix = &inner_ix.instructions[j + send_ix_index];
            let recv_inner_ix = &inner_ix.instructions[j + recv_ix_index];
            let input = find_transferred_token(send_inner_ix, meta, false).unwrap();
            let output = find_transferred_token(recv_inner_ix, meta, true).unwrap();
            swaps.push(Swap {
                outer_program: Some(ix.program_id.to_string()),
                program: program_id.to_string(),
                amm: account_keys[inner.accounts[amm_index] as usize].to_string(),
                signer: account_keys[0].to_string(),
                subject: account_keys[send_inner_ix.accounts[2] as usize].to_string(),
                input_mint: input.0.to_string(),
                output_mint: output.0.to_string(),
                input_amount: input.1,
                output_amount: output.1,
                pool_pre_balances: (input.2, output.2),
                sig: sig.clone(),
                order: tx_index,
            });
        }
    });
    swaps
}

async fn decompile(raw_tx: &SubscribeUpdateTransactionInfo, rpc_client: &RpcClient, lut_cache: &DashMap<Pubkey, AddressLookupTableAccount>) -> Option<DecompiledTransaction> {
    if let Some(tx) = &raw_tx.transaction {
        if let Some(meta) = &raw_tx.meta {
            // no swaps in failed txs
            if meta.err.is_some() {
                return None;
            }
            if let Some(msg) = &tx.message {
                if let Some(header) = &msg.header {
                    let sig = bs58::encode(&raw_tx.signature).into_string();
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
                    
                    // find swaps from the ixs
                    // we're looking for raydium swaps, those swaps can occur in 2 forms:
                    // 1. as a direct call to the raydium program, in that case we should see 2 inner ixs corresponding to the send/receive
                    // 2. as a cpi, in that case we should see 3 inner ixs, the raydium call and the transfers
                    // raydium swap txs has this call data: 09/amountIn u64/minOut u64, and the 2nd account is the amm id
                    let mut inner_ix_map: HashMap<usize, &InnerInstructions> = HashMap::new();
                    meta.inner_instructions.iter().for_each(|inner_ix| {
                        inner_ix_map.insert(inner_ix.index as usize, inner_ix);
                    });
                    let mut swaps: Vec<Swap> = Vec::new();
                    ixs.iter().enumerate().for_each(|(i, ix)| {
                        let inner_ix = inner_ix_map.get(&i);
                        if let Some(inner_ix) = inner_ix {
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_V4_PUBKEY, &[0x09], 1, 1, 2, 17, meta, &account_keys, sig.clone(), raw_tx.index));
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_V5_PUBKEY, &[0x8f, 0xbe, 0x5a, 0xda, 0xc4, 0x1e, 0x33, 0xde], 3, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index));
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_V5_PUBKEY, &[0x37, 0xd9, 0x62, 0x56, 0xa3, 0x4a, 0xb4, 0xad], 3, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index));
                        }                        
                    });
                    return Some(DecompiledTransaction {
                        sig,
                        instructions: ixs,
                        swaps,
                        payer: account_keys[0],
                        order: raw_tx.index,
                    });
                }
            }
        }
    }
    None    
}

fn is_valid_sandwich(s0: &Swap, s1: &Swap, s2: &Swap) -> bool {
    // criteria for sandwiches:
    // 1. has 3 txs of strictly increasing inclusion order (frontrun-victim-backrun)
    // 2. the 1st and 2nd are in the same direction, the 3rd is in reverse
    // 3. output of 3rd tx >= input of 1st tx && output of 1st tx >= input of 3rd tx (profitability constraint)
    // 4. all 3 txs use the same amm
    // 5. 2nd tx's swapper is different from the 1st and 3rd
    // 6. a wrapper program is present in the 1st and 3rd txs and are the same
    // check #1
    if s1.order >= s2.order || s0.order >= s1.order {
        return false;
    }
    // check #2
    if s0.input_mint != s1.input_mint || s0.output_mint != s1.output_mint {
        return false;
    }
    if s2.input_mint != s0.output_mint || s2.output_mint != s0.input_mint {
        return false;
    }
    // check #3
    if s2.output_amount < s0.input_amount || s0.output_amount < s2.input_amount {
        return false;
    }
    // check #4
    if s0.amm != s1.amm || s1.amm != s2.amm {
        return false;
    }
    // check #5
    if s0.signer == s1.signer || s1.signer == s2.signer {
        return false;
    }
    // check #6
    if s0.outer_program != s2.outer_program || s0.outer_program.is_none() || s2.outer_program.is_none() {
        return false;
    }
    true
}

async fn sandwich_finder(sender: mpsc::Sender<Sandwich>) {
    let rpc_url = "http://127.0.0.1:6969";
    let grpc_url = "http://127.0.0.1:10000";
    let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::processed());
    let lut_cache = DashMap::new();
    println!("connecting to grpc server: {}", grpc_url);
    let mut grpc_client = GeyserGrpcBuilder{
        endpoint: Endpoint::from_shared(grpc_url.to_string()).unwrap(),
        x_token: None,
        x_request_snapshot: false,
        send_compressed: None,
        accept_compressed: None,
        max_decoding_message_size: Some(64 * 1024 * 1024),
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
            continue;
        }
        let msg = msg.unwrap();
        match msg.update_oneof {
            Some(UpdateOneof::Block(block)) => {
                println!("new block {}", block.slot);
                let now = std::time::SystemTime::now();
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
                block_txs.sort_by_key(|x| x.order);
                // criteria for sandwiches:
                // 1. has 3 txs of strictly increasing inclusion order (frontrun-victim-backrun)
                // 2. the 1st and 2nd are in the same direction, the 3rd is in reverse
                // 3. output of 3rd tx >= input of 1st tx && output of 1st tx >= input of 3rd tx (profitability constraint)
                // 4. all 3 txs use the same amm
                // 5. 2nd tx's swapper is different from the 1st and 3rd
                // 6. a wrapper program is present in the 1st and 3rd txs and are the same

                // group swaps by amm
                let mut amm_swaps: HashMap<String, Vec<&Swap>> = HashMap::new();
                block_txs.iter().for_each(|tx| {
                    tx.swaps.iter().for_each(|swap| {
                        let swaps = amm_swaps.entry(swap.amm.clone()).or_insert(Vec::new());
                        swaps.push(swap);
                    });
                });

                // check #4
                amm_swaps.iter().for_each(|(_amm, swaps)| {
                    if swaps.len() < 3 {
                        return;
                    }
                    print!("{} {}", _amm, swaps.len());
                    // within the group, further group by direction (input token)
                    let mut input_swaps: HashMap<String, Vec<&Swap>> = HashMap::new();
                    swaps.iter().for_each(|swap| {
                        let input_swaps = input_swaps.entry(swap.input_mint.clone()).or_insert(Vec::new());
                        input_swaps.push(swap);
                    });
                    print!(".");
                    // bail out if there's not exactly 2 directions
                    if input_swaps.len() != 2 {
                        print!("\n");
                        return;
                    }
                    print!(".");
                    let mut iter = input_swaps.iter();
                    let dir0 = iter.next().unwrap();
                    let dir1 = iter.next().unwrap();
                    // look for 0-0-1 sandwiches (check #2)
                    for i in 0..dir0.1.len() {
                        for j in i+1..dir0.1.len() {
                            for k in 0..dir1.1.len() {
                                let s0 = dir0.1[i];
                                let s1 = dir0.1[j];
                                let s2 = dir1.1[k];
                                if is_valid_sandwich(s0, s1, s2) {
                                    // println!("found sandwich: {:?}", (s0, s1, s2));
                                    let sender = sender.clone();
                                    let s0 = s0.clone();
                                    let s1 = s1.clone();
                                    let s2 = s2.clone();
                                    let slot = block.slot;
                                    tokio::spawn(async move {
                                        sender.send(Sandwich::new(slot, s0, s1, s2, block.block_time.unwrap().timestamp)).await.unwrap();
                                        print!("!");
                                    });
                                }
                            }
                        }
                    }
                    print!(".");
                    // look for 1-1-0 sandwiches (check #2)
                    for i in 0..dir1.1.len() {
                        for j in i+1..dir1.1.len() {
                            for k in 0..dir0.1.len() {
                                let s0 = dir1.1[i];
                                let s1 = dir1.1[j];
                                let s2 = dir0.1[k];
                                if is_valid_sandwich(s0, s1, s2) {
                                    // println!("found sandwich: {:?}", (s0, s1, s2));
                                    let sender = sender.clone();
                                    let s0 = s0.clone();
                                    let s1 = s1.clone();
                                    let s2 = s2.clone();
                                    let slot = block.slot;
                                    tokio::spawn(async move {
                                        sender.send(Sandwich::new(slot, s0, s1, s2, block.block_time.unwrap().timestamp)).await.unwrap();
                                        print!("!");
                                    });
                                }
                            }
                        }
                    }
                    print!(".\n");
                });
                println!("block processed in {:?}", now.elapsed().unwrap());
            }
            Some(UpdateOneof::Account(account)) => {
                if let Some(account_info) = account.account {
                    let lut = AddressLookupTable::deserialize(&account_info.data).expect("unable to deserialize account");
                    let key = pubkey_from_slice(&account_info.pubkey[0..32]);
                    println!("lut updated: {:?}", key);
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
    let history = state.message_history.try_lock().unwrap();
    Json(history.iter().cloned().collect())
}

async fn start_web_server(sender: broadcast::Sender<Sandwich>, message_history: Arc<Mutex<VecDeque<Sandwich>>>) {
    let app = Router::new()
        .route("/", get(handle_websocket))
        .route("/history", get(handle_history))
        .with_state(AppState {
            message_history,
            sender,
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:11000")
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
    let (sender, mut receiver) = mpsc::channel::<Sandwich>(100);
    tokio::spawn(sandwich_finder(sender));
    let message_history = Arc::new(Mutex::new(VecDeque::<Sandwich>::with_capacity(100)));
    let (sender, _) = broadcast::channel::<Sandwich>(100);
    tokio::spawn(start_web_server(sender.clone(), message_history.clone()));
    while let Some(mut message) = receiver.recv().await {
        // println!("Received: {:?}", message);
        message.estimate_victim_loss();
        let mut hist = message_history.lock().unwrap();
        if hist.len() == 100 {
            hist.pop_front();
        }
        hist.push_back(message.clone());
        drop(hist);
        let _ = sender.send(message);
    }
}