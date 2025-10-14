use std::{collections::{HashMap, HashSet}, env};

use mysql::{prelude::Queryable, Pool};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let rpc_client = RpcClient::new(env::var("RPC_URL").unwrap());
    let args = std::env::args().collect::<Vec<_>>();
    let epoch = if args.len() >= 2 {
        args[1].parse::<u64>().expect("Invalid epoch")
    } else {
        rpc_client.get_epoch_info().await.unwrap().epoch
    };
    let leader_schedule = rpc_client.get_leader_schedule(Some(epoch * 432000)).await.unwrap();
    let leader_schedule = leader_schedule.unwrap();
    let mysql_url = env::var("MYSQL").unwrap();
    let pool = Pool::new(mysql_url.as_str()).unwrap();
    let mut conn = pool.get_conn().unwrap();
    let leader_set: HashSet<_> = leader_schedule.keys().collect();
    let stmt = format!("insert ignore into leader_mapping (leader) values {}", leader_set.iter().map(|_| "(?)").collect::<Vec<_>>().join(","));
    conn.exec_drop(stmt, leader_set.iter().collect::<Vec<_>>()).unwrap();
    let stmt = format!("select id, leader from leader_mapping where leader in ({})", leader_set.iter().map(|_| "(?)").collect::<Vec<_>>().join(","));
    let leader_map: HashMap<String, u64> = HashMap::from_iter(conn.exec_map(stmt, leader_set.iter().collect::<Vec<_>>(), |(id, leader)| (leader, id)).unwrap());
    println!("{:#?}", leader_map);

    let rev_leader_schedule: HashMap<u64, u64> = leader_schedule.iter().fold(HashMap::new(), |mut acc, (k, v)| {
        v.iter().for_each(|v| {
            acc.insert(*v as u64 + 432000 * epoch, *leader_map.get(k).unwrap());
        });
        acc
    });
    // insert in batches of 1600 rows
    let stmt = "INSERT INTO leader_schedule (slot, leader_id) VALUES ";
    let mut query = String::from(stmt);
    let mut count = 0;
    let mut cum_count = 0;
    for (slot, leader) in rev_leader_schedule.iter() {
        query.push_str(&format!("({}, '{}'),", slot, leader));
        count += 1;
        cum_count += 1;
        if count == 1600 {
            query.pop();
            conn.exec_drop(query, ()).unwrap();
            query = String::from(stmt);
            count = 0;
            println!("inserted {}/{}", cum_count, rev_leader_schedule.len());
        }
    }
    if count > 0 {
        query.pop();
        conn.exec_drop(query, ()).unwrap();
    }
}