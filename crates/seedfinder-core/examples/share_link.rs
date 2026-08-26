//! Prints the deep link for the JSON query document on stdin.
use std::io::Read;

fn main() {
    let mut json = String::new();
    std::io::stdin().read_to_string(&mut json).unwrap();
    let query = shpd_seedfinder_core::json_query::decode(&json).unwrap();
    println!(
        "{}",
        shpd_seedfinder_core::deep_link::encode(&query).unwrap()
    );
}
