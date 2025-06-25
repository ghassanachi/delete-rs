use clap::{Parser, Subcommand};
use rand::Rng;
use redis::{Commands, Connection};
use ulid::Ulid;
use url::Url;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Redis Url to connect to
    #[arg(short, long, env)]
    redis_url: Url,

    #[command(subcommand)]
    command: CliCommands,
}

#[derive(Subcommand)]
enum CliCommands {
    /// Clean up the redis instance
    Cleanup {
        /// Key pattern to filter and clean up (default is all keys "*")
        #[arg(default_value_t = String::from("*"))]
        keys: String,

        /// Wether the `delete` operation should be performed on stale keys
        #[arg(long, default_value_t = false)]
        commit: bool,

        /// Max ttl for the keys that get removed (default -1 -- no ttl)
        #[arg(short, long, default_value_t = -1)]
        max_ttl: i64,
    },
    /// Seed the redis instance with some dummy values
    Seed {
        /// Prefix for the keys to create
        #[arg(short, long, default_value_t = String::new())]
        prefix: String,

        /// Number of keys to create
        #[arg(short, long, default_value_t = 40_000)]
        num_keys: usize,

        /// Threshold of keys that get a ttl
        #[arg(short, long, default_value_t = 0.1)]
        threshold: f64,

        /// TTL for the keys that get one (in secs)
        #[arg(long, default_value_t = 10)]
        ttl: u64,
    },
}

fn cleanup(conn: &mut Connection, keys: String, max_ttl: i64, commit: bool) -> eyre::Result<()> {
    eprintln!("=>Running Cleanup (keys: {keys}, commit: {commit})");
    let mut keys: Vec<String> = conn.keys(keys)?;
    let num_keys = keys.len();
    eprintln!("==>Retrieved {} keys", keys.len());

    keys.sort();
    eprintln!("==>Sorted {} keys", keys.len());

    for (idx, key) in keys.iter().enumerate() {
        let i = idx + 1;
        let ttl: i64 = conn.ttl(key)?;

        let mut parts_iter = key.split(":");
        match (parts_iter.next(), parts_iter.last()) {
            (Some(prefix), Some(num)) if prefix == "bull" && num.parse::<usize>().is_err() => {
                eprintln!(
                    "===>[🛠️ MANAGED SKIPPING] Key({}, ttl: {ttl}) keys | ({i}/{num_keys})",
                    key,
                );
                continue;
            }
            // In all other cases we continue;
            _ => {}
        }

        let should_delete = ttl <= max_ttl;
        eprintln!(
            "===>[{}] Key({}, ttl: {ttl}) keys | ({i}/{num_keys})",
            if should_delete {
                "🗑 DELETE"
            } else {
                "🚫 SKIPPING"
            },
            key,
        );

        if commit && should_delete {
            let _: () = conn.del(key)?;
            eprintln!("===>[♲ DELETED]",);
        }
    }
    Ok(())
}

fn seed(
    conn: &mut Connection,
    prefix: String,
    num_keys: usize,
    threshold: f64,
    ttl: u64,
) -> eyre::Result<()> {
    eprintln!(
        "=>Running Seed (prefix: {prefix}, num_keys: {num_keys}, threshold: {threshold}, ttl: {ttl})"
    );

    let mut rng = rand::rng();

    for i in 1..=num_keys {
        let key = &format!("{prefix}:{}", Ulid::new());
        let level = rng.random_range(0.0..1.0);
        if level < threshold {
            let _: () = conn.set_ex(key, true, ttl)?;
            eprintln!("=> Created Key({key}, ttl: {ttl}) | ({i}/{num_keys})");
        } else {
            let _: () = conn.set(key, true)?;
            eprintln!("=> Created Key({key}, ttl: None) | ({i}/{num_keys})");
        }
    }
    Ok(())
}

fn main() -> eyre::Result<()> {
    eprintln!("Starting Redis Cleanup");
    let cli = Cli::parse();

    let client = redis::Client::open(cli.redis_url)?;
    let mut conn = client.get_connection()?;
    eprintln!("=>Acquired Connection");

    match cli.command {
        CliCommands::Cleanup {
            commit,
            max_ttl,
            keys,
        } => cleanup(&mut conn, keys, max_ttl, commit)?,
        CliCommands::Seed {
            prefix,
            num_keys,
            threshold,
            ttl,
        } => seed(&mut conn, prefix, num_keys, threshold, ttl)?,
    }
    Ok(())
}
