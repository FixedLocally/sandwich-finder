use std::{collections::HashMap, env};

use mysql::{prelude::Queryable, Pool};

/// Sandwicher-colluder report
/// The main metrics we're looking for here are sandwiches per slot (Sc) and proportion of slots with sandwiches (Sc_p),
/// and our hypothesis is that colluders will have a higher value in both values.
/// Solana validators typically only receive transactions when it's close to their leader slot,
/// and colluders relays these transactions to the sandwichers, who will sandwich the transactions where feasible and submit ASAP,
/// or the tx may land on its own (without its slippage being artifically inflated!).
/// Therefore, colluders are expected to have higher Sc and Sc_p values compared to non-colluders.
/// Since txs may take a couple slots to land (sent to a colluder but landed after the colluder's leader slots), leaders
/// of prior slots (`offset_range`) will also be credited for any given sandwich. Ideally, slots farther away should receive
/// less credits, but that's unimplemented for now.
fn main() {
    dotenv::dotenv().ok();
    let mysql_url = env::var("MYSQL").unwrap();
    let pool = Pool::new(mysql_url.as_str()).unwrap();
    let mut conn = pool.get_conn().unwrap();
    let slot_range = (318555120, 318614107);
    let offset_range = (0, 5);
    // fetch leaders within the concerned slot range to serve as the basis of normalisation
    let leader_count = conn.exec_fold("select leader from leader_schedule where slot between ? and ?", slot_range, HashMap::new(), |mut acc, row: (String,)| {
        let count = acc.entry(row.0).or_insert(0);
        *count += 1;
        acc
    }).unwrap();
    // raw score calculations (sandwiches in leader slot with offset to account for tx delay)
    let offset_stmt = conn.prep("select leader, count(*) from (select b.slot, l.leader from swap s, `transaction` t, block b, leader_schedule l where b.slot between ? and ? and s.tx_id=t.id and t.slot=b.slot and b.slot=(l.slot+?) group by s.sandwich_id) t0 group by leader order by count(*) desc;").unwrap();
    let presence_offset_stmt = conn.prep("select leader, count(*) from (select distinct b.slot, l.leader from swap s, `transaction` t, block b, leader_schedule l where b.slot between ? and ? and s.tx_id=t.id and t.slot=b.slot and b.slot=(l.slot+?) group by s.sandwich_id) t0 group by leader order by count(*) desc;").unwrap();
    let mut scores: HashMap<String, i32> = HashMap::new();
    let mut presence_scores: HashMap<String, i32> = HashMap::new();
    for i in offset_range.0..offset_range.1 {
        conn.exec_iter(&offset_stmt, (slot_range.0, slot_range.1, i)).unwrap().for_each(|row| {
            let (leader, count): (String, i32) = mysql::from_row(row.unwrap());
            let score = scores.entry(leader).or_insert(0);
            *score += count;
        });
        conn.exec_iter(&presence_offset_stmt, (slot_range.0, slot_range.1, i)).unwrap().for_each(|row| {
            let (leader, count): (String, i32) = mysql::from_row(row.unwrap());
            let score = presence_scores.entry(leader).or_insert(0);
            *score += count;
        });
    }
    // normalise scores into an approximate measure of sandwiches per slot
    let norm_factor = (offset_range.1 - offset_range.0) as f64;
    let mut normalised_scores = scores.iter().map(|(k, v)| {
        let count = leader_count.get(k).unwrap_or(&0);
        (k.clone(), *v as f64 / *count as f64 / norm_factor)
    }).collect::<Vec<(String, f64)>>();
    let presence_normalised_scores = presence_scores.iter().map(|(k, v)| {
        let count = leader_count.get(k).unwrap_or(&0);
        (k.clone(), *v as f64 / *count as f64 / norm_factor)
    }).collect::<HashMap<String, f64>>();
    // and sort by normalised score
    normalised_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    // print report
    println!("{:45}: {:7} {:7} {:7} {:7} {:7}", "Leader", "Sc", "Sc_p", "R-Sc", "R-Sc_p", "Slots");
    for (leader, score) in normalised_scores.iter() {
        println!("{:45}: {:7.3} {:7.3} {:7.3} {:7.3} {:7}", leader, score, presence_normalised_scores[leader], scores[leader] as f64 / norm_factor, presence_scores[leader] as f64 / norm_factor, leader_count[leader]);
    }
}