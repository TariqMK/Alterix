//! Runs the real rename engine on JSON from stdin ({ items, rules, options }) and prints the
//! preview as JSON. Used by assets/make_screenshots.mjs so screenshots show real results for
//! made-up files.

use alterix_lib::engine::{self, Env, InfoCache, Item, Options, Rule};
use std::collections::HashSet;
use std::io::Read;

#[derive(serde::Deserialize)]
struct Input {
    items: Vec<Item>,
    rules: Vec<Rule>,
    #[serde(default)]
    options: Options,
}

fn main() {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .expect("read stdin");
    let input: Input = serde_json::from_str(&text).expect("valid input JSON");
    let cache = InfoCache::default();
    let trace = HashSet::new();
    let env = Env {
        opts: &input.options,
        cache: &cache,
        script: None,
        trace: &trace,
    };
    let preview = engine::preview(&input.items, &input.rules, &env);
    println!(
        "{}",
        serde_json::to_string(&preview).expect("serialise preview")
    );
}
