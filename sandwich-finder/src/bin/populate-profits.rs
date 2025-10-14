use std::env;

use mysql::{prelude::Queryable, Pool};

const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
// const DEBUG_SANDWICH_ID: u64 = 0;

fn est_val(amt: u128, n: u128, d: u128) -> u64 {
    if d == 0 {
        return 0;
    }
    // (amt as u128 * n as u128 / d as u128) as u64
    0
}

fn calc_est_profit(fr_in: u64, fr_out: u64, br_in: u64, br_out: u64, t1_total: u64, t2_total: u64, min_order: u64, max_order: u64, size: u64, t1_mint: &Option<String>, t2_mint: &Option<String>, debug: bool) -> u64 {
    // sol_profit + token_profit * sol_per_token
    let t1_diff = br_out - fr_in;
    let t2_diff = fr_out - br_in;
    if debug {
        println!("frontrun {fr_in} -> {fr_out}");
        println!("backrun {br_in} -> {br_out}");
        println!("diff {t1_diff} / {t2_diff}");
        println!("total {t1_total} / {t2_total}");
        println!("direction: {t1_mint:?} -> {t2_mint:?}");
        println!("order in block: {min_order} - {max_order} #{size}");
    }
    // if max_order - min_order > 2 * size { // the ingredients are spread throughout the block, maybe false +ve
    //     return 0;
    // }
    if let Some(t1_mint) = t1_mint {
        if t1_mint == WSOL_MINT {
            let est_profit = t1_diff + est_val(t2_diff as u128, t1_total as u128, t2_total as u128);
            if debug {println!("t1 est profit {}", est_profit);}
            return est_profit;
        }
    }
    if let Some(t2_mint) = t2_mint {
        if t2_mint == WSOL_MINT {
            let est_profit = t2_diff + est_val(t1_diff as u128, t2_total as u128, t1_total as u128);
            if debug {println!("t2 est profit {}", est_profit);}
            return est_profit;
        }
    }
    return 0;
}

fn main() {
    dotenv::dotenv().ok();
    let args: Vec<String> = env::args().collect();
    let debug_sandwich_id: u64 = if args.len() > 1 {
        args[1].parse().unwrap_or(0)
    } else {
        0
    };
    let mysql_url = env::var("MYSQL").unwrap();
    let pool = Pool::new(mysql_url.as_str()).unwrap();
    let mut conn: mysql::PooledConn = pool.get_conn().unwrap();
    let stmt = conn.prep("SELECT ifnull(max(id), 0) FROM `sandwich` where est_profit_lamports>0").unwrap();
    let max_id: u64 = conn.exec_first(&stmt, ()).unwrap().unwrap_or(0);
    let op = if debug_sandwich_id > 0 { "=" } else { ">=" };
    let stmt = conn.prep(format!("SELECT sandwich_id, order_in_block, input_mint, input_amount, output_mint, output_amount, swap_type from sandwich_view where sandwich_id {} ? order by sandwich_id asc", op)).unwrap();

    let mut update_conn = pool.get_conn().unwrap();
    let update_stmt = update_conn.prep("UPDATE sandwich SET est_profit_lamports=? WHERE id=?").unwrap();

    let mut t1_total: u64 = 0;
    let mut t2_total: u64 = 0;
    let mut t1_mint: Option<String> = None;
    let mut t2_mint: Option<String> = None;

    let mut fr_in: u64 = 0;
    let mut fr_out: u64 = 0;
    let mut br_in: u64 = 0;
    let mut br_out: u64 = 0;

    let mut max_order: u64 = 0;
    let mut min_order: u64 = 99999999;
    let mut size: u64 = 0;

    let mut cur_id = if debug_sandwich_id > 0 { debug_sandwich_id } else { max_id + 1 };
    conn.exec_map(&stmt, (cur_id,), |(sandwich_id, order_in_block, input_mint, input_amount, output_mint, output_amount, swap_type): (u64, u64, String, u64, String, u64, String)| {
        if sandwich_id != cur_id {
            let est_profit = calc_est_profit(fr_in, fr_out, br_in, br_out, t1_total, t2_total, min_order, max_order, size, &t1_mint, &t2_mint, debug_sandwich_id > 0);
            println!("sandwich_id: {cur_id} est_profit: {est_profit}");
            if est_profit > 0 && est_profit < 1000_000_000_000 && debug_sandwich_id == 0 {
                update_conn.exec_drop(&update_stmt, (est_profit, cur_id)).unwrap();
            }
            // reset vars
            t1_total = 0;
            t2_total = 0;
            t1_mint = None;
            t2_mint = None;
            fr_in = 0;
            fr_out = 0;
            br_in = 0;
            br_out = 0;
            max_order = 0;
            min_order = 99999999;
            size = 0;
            cur_id = sandwich_id;
        }
        if t1_mint.is_none() {
            if swap_type == "BACKRUN" {
                t2_mint = Some(input_mint.clone());
                t1_mint = Some(output_mint.clone());
            } else {
                t1_mint = Some(input_mint.clone());
                t2_mint = Some(output_mint.clone());
            }
        }
        match swap_type.as_str() {
            "FRONTRUN" => {
                fr_in += input_amount;
                fr_out += output_amount;
                t1_total += input_amount;
                t2_total += output_amount;
            }
            "BACKRUN" => {
                br_in += input_amount;
                br_out += output_amount;
                // t1_total -= output_amount;
                // t2_total -= input_amount;
            }
            "VICTIM" => {
                // t1_total += input_amount;
                // t2_total += output_amount;
            }
            _ => {
                panic!("Unknown swap type: {}", swap_type);
            }
        }
        max_order = max_order.max(order_in_block);
        min_order = min_order.min(order_in_block);
        size += 1;
    }).unwrap();
    let est_profit = calc_est_profit(fr_in, fr_out, br_in, br_out, t1_total, t2_total, min_order, max_order, size, &t1_mint, &t2_mint, debug_sandwich_id > 0);
    println!("sandwich_id: {cur_id} est_profit: {est_profit}");
    if est_profit > 0 && est_profit < 1000_000_000_000 && debug_sandwich_id == 0 {
        update_conn.exec_drop(&update_stmt, (est_profit, cur_id)).unwrap();
    }
}