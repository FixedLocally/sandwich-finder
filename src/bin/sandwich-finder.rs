use std::{collections::{HashMap, VecDeque}, env, fmt::Debug, net::SocketAddr, str::FromStr, sync::{Arc, RwLock}, vec};
use axum::{extract::{ws::{Message, WebSocket}, Path, State, WebSocketUpgrade}, response::IntoResponse, routing::get, Json, Router};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use mysql::{prelude::Queryable, Pool, TxOpts, Value};
use serde::{ser::SerializeStruct, Serialize};

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{account::ReadableAccount, address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount}, bs58, commitment_config::CommitmentConfig, instruction::{AccountMeta, Instruction}, pubkey::Pubkey};
use tokio::sync::{broadcast, mpsc};
use yellowstone_grpc_client::GeyserGrpcBuilder;
use yellowstone_grpc_proto::{geyser::{subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequestFilterAccounts, SubscribeRequestPing, SubscribeUpdateTransactionInfo}, prelude::{InnerInstruction, InnerInstructions, SubscribeRequest, SubscribeRequestFilterBlocks, TransactionStatusMeta}, tonic::transport::Endpoint};

const RAYDIUM_V4_PUBKEY: Pubkey = Pubkey::from_str_const("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
const RAYDIUM_V5_PUBKEY: Pubkey = Pubkey::from_str_const("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
const RAYDIUM_LP_PUBKEY: Pubkey = Pubkey::from_str_const("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");
const PDF_PUBKEY: Pubkey = Pubkey::from_str_const("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const PDF2_PUBKEY: Pubkey = Pubkey::from_str_const("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
const WHIRLPOOL_PUBKEY: Pubkey = Pubkey::from_str_const("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const DLMM_PUBKEY: Pubkey = Pubkey::from_str_const("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
const METEORA_PUBKEY: Pubkey = Pubkey::from_str_const("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB");

const WSOL_PUBKEY: Pubkey = Pubkey::from_str_const("So11111111111111111111111111111111111111112");

const DONT_FRONT_START: [u8; 32] = [10,241,195,67,33,136,202,58,99,81,53,161,58,24,149,26,206,189,41,230,172,45,174,103,255,219,6,215,64,0,0,0];
const DONT_FRONT_END: [u8; 32]   = [10,241,195,67,33,136,202,58,99,82,11,83,236,186,243,27,60,23,98,46,152,130,58,175,28,197,174,53,128,0,0,0];

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
    order: u64,
    sig: String,
    dont_front: bool,
}

#[derive(Clone)]
struct DbBlock {
    slot: u64,
    ts: i64,
    tx_count: usize,
}

#[derive(Clone)]
enum DbMessage {
    Block(DbBlock),
    Sandwich(Sandwich),
}

#[derive(Clone)]
enum SwapType {
    Frontrun,
    Victim,
    Backrun,
}

impl From<String> for SwapType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "FRONTRUN" => SwapType::Frontrun,
            "VICTIM" => SwapType::Victim,
            "BACKRUN" => SwapType::Backrun,
            _ => panic!("unknown swap type"),
        }
    }
}

impl Into<Value> for SwapType {
    fn into(self) -> Value {
        match self {
            SwapType::Frontrun => Value::from("FRONTRUN"),
            SwapType::Victim => Value::from("VICTIM"),
            SwapType::Backrun => Value::from("BACKRUN"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sandwich {
    slot: u64,
    frontrun: Swap,
    victim: Vec<Swap>,
    backrun: Swap,
    ts: i64,
}

impl Sandwich {
    pub fn new(slot: u64, frontrun: Swap, victim: Vec<Swap>, backrun: Swap, ts: i64) -> Self {
        Self {
            slot,
            frontrun,
            victim,
            backrun,
            ts,
        }
    }

    pub fn estimate_victim_loss(&self) -> (u64, u64) {
        let (a1, a2) = (self.frontrun.input_amount as i128, self.victim[0].input_amount as i128);
        let (b1, b2) = (self.frontrun.output_amount as i128, self.victim[0].output_amount as i128);
        let (a3, b3) = (a1 + a2, b1 + b2);
        let (c1, c2) = (-a1 * b1, -a3 * b3);
        // | b1   -a1 | | a | = | c1 |
        // | b3   -a3 | | b |   | c2 |
        let det = a1 * b3 - b1 * a3;
        let det_a = a1 * c2 - c1 * a3;
        let det_b = b1 * c2 - b3 * c1;
        let a = det_a / det;
        let b = det_b / det;
        let k = a * b;
        let b2_ = b - k / (a + a2);
        let a2_ = a - k / (b - b2);
        ((a2 - a2_) as u64, (b2_ - b2) as u64)
    }
}

impl Serialize for Sandwich {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let mut state = serializer.serialize_struct("Sandwich", 6)?;
        state.serialize_field("slot", &self.slot)?;
        state.serialize_field("frontrun", &self.frontrun)?;
        state.serialize_field("victim", &self.victim)?;
        state.serialize_field("backrun", &self.backrun)?;
        state.serialize_field("ts", &self.ts)?;
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
    message_history: Arc<RwLock<VecDeque<Sandwich>>>,
    sender: broadcast::Sender<Sandwich>,
    pool: Pool,
}

fn create_db_pool() -> Pool {
    let url = env::var("MYSQL").unwrap();
    let pool = Pool::new(url.as_str()).unwrap();
    pool
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

fn find_transferred_token(ix: &InnerInstruction, meta: &TransactionStatusMeta) -> Option<(Pubkey, u8, u64)> {
    // transfer: 1/0; transferChecked: 2/0
    let (i1, i0, subject_idx, range) = match ix.data[0] {
        2 => (99, 99, ix.accounts[0], 4..12), // system program transfer
        3 => (ix.accounts[1], ix.accounts[0], ix.accounts[2], 1..9), // transfer
        12 => (ix.accounts[2], ix.accounts[0], ix.accounts[3], 1..9), // transferChecked
        228 => (99, 99, ix.accounts[0], 48..56), // anchor self cpi log for pdf (no subject)
        _ => return None,
    };
    let amount = u64::from_le_bytes(ix.data[range].try_into().expect("slice with incorrect length"));
    if (i1, i0) == (99, 99) {
        return Some((WSOL_PUBKEY, subject_idx, amount));
    }
    return meta.post_token_balances.iter().filter(|x| x.account_index == i1 as u32 || x.account_index == i0 as u32).map(|x| {
        (Pubkey::from_str(&x.mint).expect("invalid pubkey"), subject_idx, amount)
    }).next();
}

fn find_swaps(ix: &Instruction, inner_ix: &InnerInstructions, swap_program: &Pubkey, discriminant: &[u8], amm_index: usize, send_ix_index: usize, recv_ix_index: usize, data_len: usize, meta: &TransactionStatusMeta, account_keys: &Vec<Pubkey>, sig: String, tx_index: u64, dont_front: bool) -> Vec<Swap> {
    let mut swaps: Vec<Swap> = Vec::new();
    // case 1
    if ix.program_id == *swap_program && ix.data.len() == data_len && ix.data[0..discriminant.len()] == *discriminant {
        let send_inner_ix = &inner_ix.instructions[send_ix_index - 1];
        let recv_inner_ix = &inner_ix.instructions[recv_ix_index - 1];
        let input = find_transferred_token(send_inner_ix, meta);
        let output = find_transferred_token(recv_inner_ix, meta);
        if let Some(input) = input {
            if let Some(output) = output {
                swaps.push(Swap {
                    outer_program: None,
                    program: ix.program_id.to_string(),
                    amm: ix.accounts[amm_index].pubkey.to_string(),
                    signer: account_keys[0].to_string(),
                    subject: account_keys[input.1 as usize].to_string(),
                    input_mint: input.0.to_string(),
                    output_mint: output.0.to_string(),
                    input_amount: input.2,
                    output_amount: output.2,
                    sig: sig.clone(),
                    order: tx_index,
                    dont_front,
                });
            }
        }
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
            let input = find_transferred_token(send_inner_ix, meta);
            let output = find_transferred_token(recv_inner_ix, meta);
            if let Some(input) = input {
                if let Some(output) = output {
                    swaps.push(Swap {
                        outer_program: Some(ix.program_id.to_string()),
                        program: program_id.to_string(),
                        amm: account_keys[inner.accounts[amm_index] as usize].to_string(),
                        signer: account_keys[0].to_string(),
                        subject: account_keys[input.1 as usize].to_string(),
                        input_mint: input.0.to_string(),
                        output_mint: output.0.to_string(),
                        input_amount: input.2,
                        output_amount: output.2,
                        sig: sig.clone(),
                        order: tx_index,
                        dont_front,
                    });
                }
            }
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

                    // don't front flag - if the tx contains a pubkey that starts with jitodontfront, which is pubkeys within [DONT_FRONT_START, DONT_FRONT_END)
                    let dont_front = account_keys.iter().any(|k| k.to_bytes() >= DONT_FRONT_START && k.to_bytes() < DONT_FRONT_END);
                    
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
                    // discriminant/amm_index/send_ix_index/recv_ix_index/data_len
                    // ray v4 swap
                    // 09/1/+1/+2/17
                    // ray v5 swap_exact_in/swap_exact_out
                    // 8fbe5adac41e33de/3/+1/+2/24
                    // 37d96256a34ab4ad/3/+1/+2/24
                    // pdf buy/sell
                    // 66063d1201daebea/3/+2/+1/24
                    // 33e685a4017f83ad/3/+1/+2/24
                    ixs.iter().enumerate().for_each(|(i, ix)| {
                        let inner_ix = inner_ix_map.get(&i);
                        if let Some(inner_ix) = inner_ix {
                            // ray v4 swap
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_V4_PUBKEY, &[0x09], 1, 1, 2, 17, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // ray v5 swap_base_input/swap_base_output
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_V5_PUBKEY, &[0x8f, 0xbe, 0x5a, 0xda, 0xc4, 0x1e, 0x33, 0xde], 3, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_V5_PUBKEY, &[0x37, 0xd9, 0x62, 0x56, 0xa3, 0x4a, 0xb4, 0xad], 3, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // ray launchpad buy_exact_in/sell_exact_in
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_LP_PUBKEY, &[0xfa, 0xea, 0x0d, 0x7b, 0xd5, 0x9c, 0x13, 0xec], 4, 2, 3, 32, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            swaps.extend(find_swaps(ix, inner_ix, &RAYDIUM_LP_PUBKEY, &[0x95, 0x27, 0xde, 0x9b, 0xd3, 0x7c, 0x98, 0x1a], 4, 2, 3, 32, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // pdf buy/sell
                            swaps.extend(find_swaps(ix, inner_ix, &PDF_PUBKEY, &[0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea], 3, 2, 1, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            swaps.extend(find_swaps(ix, inner_ix, &PDF_PUBKEY, &[0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad], 3, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // pdf2 buy/sell
                            swaps.extend(find_swaps(ix, inner_ix, &PDF2_PUBKEY, &[0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea], 0, 2, 1, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            swaps.extend(find_swaps(ix, inner_ix, &PDF2_PUBKEY, &[0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad], 0, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // whirlpool swap
                            swaps.extend(find_swaps(ix, inner_ix, &WHIRLPOOL_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 2, 1, 2, 42, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // dlmm swap
                            swaps.extend(find_swaps(ix, inner_ix, &DLMM_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 1, 2, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            // meteora swap (swap, (charge_fee),  deposit, send, mint_lp, withdraw, recv, burn_lp)
                            swaps.extend(find_swaps(ix, inner_ix, &METEORA_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 2, 5, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
                            swaps.extend(find_swaps(ix, inner_ix, &METEORA_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 3, 6, 24, meta, &account_keys, sig.clone(), raw_tx.index, dont_front));
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

fn find_sandwiches(in_trades: &Vec<&Swap>, out_trades: &Vec<&Swap>, slot: u64, ts: i64) -> Vec<Sandwich> {
    // for each in_trade, we look for an out_trade that satisfies the sandwich criteria
    // since we've already went this far, we just need to pass checks 1, 3, 6
    // and we can consider all trades between the in/out trades to be sandwiched
    let mut sandwiches = Vec::new();
    let mut last_found_index = 0;
    // todo: should match closing trades with the same signer, if possible
    for i in 0..in_trades.len() {
        let in_trade = in_trades[i];
        let mut matching_out_trade: Option<&Swap> = None;
        let mut nonmatching_out_trade: Option<&Swap> = None;
        if in_trade.order <= last_found_index {
            // we already found another sandwich that includes this trade
            continue;
        }
        for j in 0..out_trades.len() {
            let out_trade = out_trades[j];
            // check #1
            if out_trade.order <= in_trade.order {
                continue;
            }
            // check #3
            if out_trade.output_amount < in_trade.input_amount {
                continue;
            }
            if out_trade.input_amount > in_trade.output_amount {
                continue;
            }
            // check #6
            if in_trade.outer_program != out_trade.outer_program || in_trade.outer_program.is_none() || out_trade.outer_program.is_none() {
                continue;
            }
            if in_trade.outer_program == Some("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string()) {
                continue;
            }
            if nonmatching_out_trade.is_none() {
                nonmatching_out_trade = Some(out_trade);
            }
            if out_trade.signer == in_trade.signer && matching_out_trade.is_none() {
                matching_out_trade = Some(out_trade);
                break; // already found the sandwich for this in_trade
            }
            if nonmatching_out_trade.is_some() && matching_out_trade.is_some() {
                break; // found both candidates, go to evaluation
            }
        }
        // these two trades form the sandwich, now we just need to find the victims (in_trades between in_trade and out_trade)
        let mut victims: Vec<Swap> = Vec::new();
        if nonmatching_out_trade.is_none() {
            // no sandwich found, go to next in_trade
            continue;
        }
        let out_trade = if let Some(matching_out_trade) = matching_out_trade {
            // we have a matching out_trade, use it
            matching_out_trade
        } else {
            nonmatching_out_trade.unwrap()
        };
        for k in i+1..in_trades.len() {
            let victim = in_trades[k];
            // check #1
            if victim.order >= out_trade.order {
                // subsequent in_trade's will have even higher order
                break;
            }
            // check #5
            if victim.signer == in_trade.signer || victim.signer == out_trade.signer {
                continue;
            }
            victims.push(victim.clone());
        }
        if !victims.is_empty() {
            sandwiches.push(Sandwich::new(slot, in_trade.clone(), victims, out_trade.clone(), ts));
            last_found_index = out_trade.order;
            break; // already found the sandwich for this in_trade
        }
    }
    sandwiches
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
                db_sender.send(DbMessage::Block(DbBlock {
                    slot,
                    ts,
                    tx_count: block.transactions.len(),
                })).await.unwrap();
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
                let swap_count = block_txs.iter().map(|tx| tx.swaps.len()).sum::<usize>();
                block_txs.sort_by_key(|x| x.order);
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
                    tx.swaps.iter().for_each(|swap| {
                        let swaps = amm_swaps.entry(&swap.amm).or_insert(Vec::new());
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
                        let input_swaps = input_swaps.entry(&swap.input_mint).or_insert(Vec::new());
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
    let insert_block_stmt = conn.prep("insert into block (slot, timestamp, tx_count) values (?, ?, ?)").unwrap();
    let insert_tx_stmt = conn.prep("insert into transaction (tx_hash, signer, slot, order_in_block, dont_front) values (?, ?, ?, ?, ?)").unwrap();
    let insert_swap_stmt = conn.prep("insert into swap (sandwich_id, outer_program, inner_program, amm, subject, input_mint, output_mint, input_amount, output_amount, tx_id, swap_type) values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)").unwrap();

    let mut tx_db_id_cache: HashMap<String, u64> = HashMap::new();
    while let Some(msg) = receiver.recv().await {
        match msg {
            DbMessage::Block(block) => {
                conn.exec_drop(&insert_block_stmt, (block.slot, block.ts, block.tx_count)).unwrap();
            }
            DbMessage::Sandwich(sandwich) => {
                let mut dbtx = conn.start_transaction(TxOpts::default()).unwrap();
                // obtain an id for this sandwich
                dbtx.query_drop("insert into sandwich values ()").unwrap();
                let sandwich_id = dbtx.last_insert_id();
                let mut swaps = Vec::new();
                swaps.push((&sandwich.frontrun, SwapType::Frontrun));
                swaps.extend(sandwich.victim.iter().map(|x| (x, SwapType::Victim)));
                swaps.push((&sandwich.backrun, SwapType::Backrun));
                // figure out which txs are new to the db
                let args: Vec<_> = swaps.iter().filter_map(|swap| {
                    if tx_db_id_cache.contains_key(&swap.0.sig) {
                        None
                    } else {
                        Some((&swap.0.sig, &swap.0.signer, sandwich.slot, swap.0.order, swap.0.dont_front))
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
                    let tx_id = tx_db_id_cache.get(&swap.0.sig).unwrap();
                    (sandwich_id, swap.0.outer_program.as_deref(), swap.0.program.as_str(), swap.0.amm.as_str(), swap.0.subject.as_str(), swap.0.input_mint.as_str(), swap.0.output_mint.as_str(), swap.0.input_amount, swap.0.output_amount, tx_id, swap.1.clone())
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
        let swap = Swap {
            outer_program,
            program: inner_program,
            amm,
            signer,
            subject,
            input_mint,
            output_mint,
            input_amount,
            output_amount,
            sig: tx_hash.clone(),
            order: order_in_block,
            dont_front,
        };
        slot = slot_;
        ts = ts_;
        match swap_type.into() {
            SwapType::Frontrun => frontrun = Some(swap),
            SwapType::Victim => victims.push(swap),
            SwapType::Backrun => backrun = Some(swap),
        };
    }
    if frontrun.is_some() && backrun.is_some() && victims.len() > 0 {
        let sandwich = Sandwich {
            slot: slot as u64,
            frontrun: frontrun.unwrap(),
            victim: victims,
            backrun: backrun.unwrap(),
            ts,
        };
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