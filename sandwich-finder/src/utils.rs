use std::{collections::HashMap, env, fmt::Debug, str::FromStr};

use dashmap::DashMap;
use derive_getters::Getters;
use mysql::{Pool, Value};
use serde::{ser::SerializeStruct, Serialize};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{account::ReadableAccount, address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount}, bs58, instruction::{AccountMeta, Instruction}, pubkey::Pubkey};
use yellowstone_grpc_proto::{geyser::{SubscribeUpdateBlock, SubscribeUpdateTransactionInfo}, prelude::{InnerInstruction, InnerInstructions, RewardType, TransactionStatusMeta}};

const DONT_FRONT_START: [u8; 32] = [10,241,195,67,33,136,202,58,99,81,53,161,58,24,149,26,206,189,41,230,172,45,174,103,255,219,6,215,64,0,0,0];
const DONT_FRONT_END: [u8; 32]   = [10,241,195,67,33,136,202,58,99,82,11,83,236,186,243,27,60,23,98,46,152,130,58,175,28,197,174,53,128,0,0,0];

const RAYDIUM_V4_PUBKEY: Pubkey = Pubkey::from_str_const("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
const RAYDIUM_V5_PUBKEY: Pubkey = Pubkey::from_str_const("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
const RAYDIUM_LP_PUBKEY: Pubkey = Pubkey::from_str_const("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");
const PDF_PUBKEY: Pubkey = Pubkey::from_str_const("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const PDF2_PUBKEY: Pubkey = Pubkey::from_str_const("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
const WHIRLPOOL_PUBKEY: Pubkey = Pubkey::from_str_const("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const DLMM_PUBKEY: Pubkey = Pubkey::from_str_const("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
const METEORA_PUBKEY: Pubkey = Pubkey::from_str_const("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB");

const WSOL_PUBKEY: Pubkey = Pubkey::from_str_const("So11111111111111111111111111111111111111112");

#[derive(Clone, Serialize, Getters)]
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

impl Swap {
    pub fn new(
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
    ) -> Self {
        Self {
            outer_program,
            program,
            amm,
            signer,
            subject,
            input_mint,
            output_mint,
            input_amount,
            output_amount,
            order,
            sig,
            dont_front,
        }
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

#[derive(Clone)]
pub enum SwapType {
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

#[derive(Debug, Clone, Getters)]
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

#[derive(Getters)]
pub struct DecompiledTransaction {
    sig: String,
    instructions: Vec<Instruction>,
    swaps: Vec<Swap>,
    payer: Pubkey,
    order: u64,
    account_keys: Vec<Pubkey>,
}

impl DecompiledTransaction {
    pub fn new(
        sig: String,
        instructions: Vec<Instruction>,
        swaps: Vec<Swap>,
        payer: Pubkey,
        order: u64,
        account_keys: Vec<Pubkey>,
    ) -> Self {
        Self {
            sig,
            instructions,
            swaps,
            payer,
            order,
            account_keys,
        }
    }
}

#[derive(Clone, Getters)]
pub struct DbBlock {
    slot: u64,
    ts: i64,
    tx_count: usize,
    vote_count: usize,
    reward_lamports: Option<i64>,
    successful_cu: u64,
    total_cu: u64,
}

#[derive(Clone)]
pub enum DbMessage {
    Block(DbBlock),
    Sandwich(Sandwich),
}

pub fn create_db_pool() -> Pool {
    let url = env::var("MYSQL").unwrap();
    let pool = Pool::new(url.as_str()).unwrap();
    pool
}

pub fn block_stats(block: &SubscribeUpdateBlock) -> DbMessage {
    let ts = block.block_time.unwrap().timestamp;
    let slot = block.slot;
    let reward_lamports= if let Some(rewards) = &block.rewards {
        rewards.rewards.iter()
            .filter(|f| f.reward_type == RewardType::Fee as i32)
            .map(|f| f.lamports)
            .reduce(|a, b| a + b)
    } else {
        None
    };
    // vote count/successful/total units
    let stats = block.transactions.iter().fold((0, 0, 0), |a, tx| {
        let vote_count = if tx.is_vote {
            a.0 + 1
        } else {
            a.0
        };
        if let Some(meta) = &tx.meta {
            if let Some(units) = meta.compute_units_consumed {
                return if meta.err.is_none() {
                    (vote_count, a.1 + units, a.2 + units)
                } else {
                    (vote_count, a.1, a.2 + units)
                };
            }
        }
        (vote_count, a.1, a.2)
    });
    DbMessage::Block(DbBlock {
        slot,
        ts,
        tx_count: block.transactions.len(),
        vote_count: stats.0,
        reward_lamports,
        successful_cu: stats.1,
        total_cu: stats.2,
    })
}

pub async fn decompile(raw_tx: &SubscribeUpdateTransactionInfo, rpc_client: &RpcClient, lut_cache: &DashMap<Pubkey, AddressLookupTableAccount>) -> Option<DecompiledTransaction> {
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
                    return Some(DecompiledTransaction::new(
                        sig,
                        ixs,
                        swaps,
                        account_keys[0],
                        raw_tx.index,
                        account_keys,
                    ));
                }
            }
        }
    }
    None    
}

pub async fn decompile_tx<'a>(raw_tx: &'a SubscribeUpdateTransactionInfo, rpc_client: &RpcClient, lut_cache: &DashMap<Pubkey, AddressLookupTableAccount>) -> Option<(&'a SubscribeUpdateTransactionInfo, Vec<Instruction>, Vec<Pubkey>)> {
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

pub fn find_sandwiches(in_trades: &Vec<&Swap>, out_trades: &Vec<&Swap>, slot: u64, ts: i64) -> Vec<Sandwich> {
    // for each in_trade, we look for an out_trade that satisfies the sandwich criteria
    // since we've already went this far, we just need to pass checks 1, 3, 6
    // and we can consider all trades between the in/out trades to be sandwiched
    let mut sandwiches = Vec::new();
    let mut last_found_index: u64 = 0;
    // todo: should match closing trades with the same signer, if possible
    for i in 0..in_trades.len() {
        let in_trade = in_trades[i];
        let mut matching_out_trade: Option<&Swap> = None;
        let mut nonmatching_out_trade: Option<&Swap> = None;
        if *in_trade.order() <= last_found_index {
            // we already found another sandwich that includes this trade
            continue;
        }
        for j in 0..out_trades.len() {
            let out_trade = out_trades[j];
            // check #1
            if out_trade.order() <= in_trade.order() {
                continue;
            }
            // check #3
            if out_trade.output_amount() < in_trade.input_amount() {
                continue;
            }
            if out_trade.input_amount() > in_trade.output_amount() {
                continue;
            }
            // check #6
            if in_trade.outer_program() != out_trade.outer_program() || in_trade.outer_program().is_none() || out_trade.outer_program().is_none() {
                continue;
            }
            if in_trade.outer_program() == &Some("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string()) {
                continue;
            }
            if nonmatching_out_trade.is_none() {
                nonmatching_out_trade = Some(out_trade);
            }
            if out_trade.signer() == in_trade.signer() && matching_out_trade.is_none() {
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
            if victim.order() >= out_trade.order() {
                // subsequent in_trade's will have even higher order
                break;
            }
            // check #5
            if victim.signer() == in_trade.signer() || victim.signer() == out_trade.signer() {
                continue;
            }
            victims.push(victim.clone());
        }
        if !victims.is_empty() {
            sandwiches.push(Sandwich::new(slot, in_trade.clone(), victims, out_trade.clone(), ts));
            last_found_index = *out_trade.order();
            break; // already found the sandwich for this in_trade
        }
    }
    sandwiches
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
                swaps.push(Swap::new(
                    None,
                    ix.program_id.to_string(),
                    ix.accounts[amm_index].pubkey.to_string(),
                    account_keys[0].to_string(),
                    account_keys[input.1 as usize].to_string(),
                    input.0.to_string(),
                    output.0.to_string(),
                    input.2,
                    output.2,
                    tx_index,
                    sig.clone(),
                    dont_front,
                ));
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
            if inner_ix.instructions.len() < j + send_ix_index || inner_ix.instructions.len() < j + recv_ix_index {
                println!("we encountered a problem with tx {} - not enough inner instructions", sig);
                return; // not enough inner instructions
            }
            let send_inner_ix = &inner_ix.instructions[j + send_ix_index];
            let recv_inner_ix = &inner_ix.instructions[j + recv_ix_index];
            let input = find_transferred_token(send_inner_ix, meta);
            let output = find_transferred_token(recv_inner_ix, meta);
            if let Some(input) = input {
                if let Some(output) = output {
                    swaps.push(Swap::new(
                        Some(ix.program_id.to_string()),
                        program_id.to_string(),
                        account_keys[inner.accounts[amm_index] as usize].to_string(),
                        account_keys[0].to_string(),
                        account_keys[input.1 as usize].to_string(),
                        input.0.to_string(),
                        output.0.to_string(),
                        input.2,
                        output.2,
                        tx_index,
                        sig.clone(),
                        dont_front,
                    ));
                }
            }
        }
    });
    swaps
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

pub fn pubkey_from_slice(slice: &[u8]) -> Pubkey {
    Pubkey::new_from_array(slice.try_into().expect("slice with incorrect length"))
}