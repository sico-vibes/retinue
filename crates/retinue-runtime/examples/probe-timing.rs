//! What a launch's CLI discovery costs, cold and warm (21 November 2026,
//! performance item 7).
//!
//! It runs the daemon's own discovery three times against the installed coding
//! agents and prints one JSON line per run:
//!
//! * `cold`: no cache file, so every version and protocol probe starts a CLI;
//! * `warm`: the cache the cold run just wrote, so those probes are answers from
//!   the file (the sign-in checks still ask the CLI, because a sign-in can change
//!   without the binary changing);
//! * `forced`: `runtime.refresh {force:true}`, which ignores the cache.
//!
//! `spawns` counts every CLI process the scan started, sign-in and account reads
//! included, so the difference between the runs is exactly what the cache saved.
//! It is a measurement, not a gate: `cargo run -p retinue-runtime --example
//! probe-timing -- [--cache <path>]`. It starts the owner's real CLIs, which is
//! read-only, and deletes only the cache file it names (a temp file by default).
//! A provider turned off in Retinue is not probed by the daemon; this example has
//! no settings to read, so give it `--off <provider>` for each one the owner has
//! switched off, and the runs match what a launch would do.

use retinue_runtime::{discover_except_cached, ProbeCache};
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut cache_path: Option<PathBuf> = None;
    let mut off: Vec<String> = Vec::new();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--cache" => cache_path = args.next().map(PathBuf::from),
            "--off" => {
                if let Some(provider) = args.next() {
                    off.push(provider);
                }
            }
            other => {
                eprintln!("usage: probe-timing [--cache <path>] [--off <provider>]... (unknown argument {other})");
                std::process::exit(2);
            }
        }
    }
    let cache_path = cache_path.unwrap_or_else(|| std::env::temp_dir().join("retinue-probe-timing.json"));
    // A cold run must be cold: the example owns the file it names and removes it
    // first so the first line is the uncached cost.
    let _ = std::fs::remove_file(&cache_path);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("the async runtime could not start");
    runtime.block_on(async move {
        for (run, force) in [("cold", false), ("warm", false), ("forced", true)] {
            let mut cache = ProbeCache::at(Some(cache_path.clone()));
            let started = Instant::now();
            let report = discover_except_cached(&off, &mut cache, force).await;
            let elapsed_ms = (started.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
            if let Err(error) = cache.save() {
                eprintln!("the cache could not be saved: {error}");
            }
            let rows = report.runtimes.iter().map(|runtime| json!({
                "provider": runtime.provider,
                "version": runtime.version,
                "status": runtime.status,
                "authState": runtime.auth_state,
                "probeCachedAgeMs": runtime.probe_cached_age_ms,
            })).collect::<Vec<_>>();
            println!("{}", json!({
                "run": run,
                "force": force,
                "ms": elapsed_ms,
                "cached": report.cached,
                "probed": report.probed,
                "spawns": report.spawns,
                "cacheEntries": cache.len(),
                "runtimes": rows,
            }));
        }
        let _ = std::fs::remove_file(&cache_path);
    });
}
