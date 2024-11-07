#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let mut rl = rustyline::DefaultEditor::new()?;
    loop {
        let readline = rl.readline(">> ");
        if let Err(e) = readline {
            match e {
                rustyline::error::ReadlineError::Eof
                | rustyline::error::ReadlineError::Interrupted => break,
                _ => return Err(e.into()),
            }
        }
        let expr = readline.unwrap();
        rl.add_history_entry(&expr)?;

        let parsed = pgrules::parse_to_sql(&expr);
        if let Err(e) = parsed {
            println!("Failed to parse rule: {}", e);
            continue;
        }
        let q = parsed.unwrap();
        if let Ok(pq_conn_str) = std::env::var("PGCONNSTR") {
            info!("Executing query: {q}");
            let mut client = postgres::Client::connect(&pq_conn_str, postgres::tls::NoTls)
                .unwrap_or_else(|e| panic!("Failed to connect to db: {}", e));
            let wrapq = format!("SELECT json_agg(q) FROM (select * {q}) AS q");
            let row = client
                .query_one(&wrapq, &[])
                .unwrap_or_else(|e| panic!("Query failed: {}", e));
            let json: serde_json::Value = row.get(0);
            println!("{json}");
        } else {
            println!("Query:\n{q}");
        }
    }
    Ok(())
}
