use clap::Parser;
use pgrules::{get_code_completion, Position, QueryType};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    global: bool,
    #[arg(long)]
    debug: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let mut rl = rustyline::DefaultEditor::new()?;
    let sql_context = if cli.global {
        QueryType::ScenarioGlobal
    } else {
        pgrules::QueryType::Search
    };
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

        let parsed = match pgrules::parse_to_sql(&expr, sql_context) {
            Ok(parsed) => parsed,
            Err(e) => {
                println!("Failed to parse rule: {}", e);

                if cli.debug {
                    match get_code_completion(&expr, Position::Byte(expr.len()), cli.global) {
                        Ok(completion) => {
                            let tokens = completion
                                .into_iter()
                                .map(|c| match c {
                                    pgrules::Token::Keyword(s) => s,
                                    pgrules::Token::Text(s) => s,
                                })
                                .collect::<Vec<_>>();
                            println!("Code completion: {tokens:?}");
                        }
                        Err(err) => {
                            println!("Error: {err}");
                        }
                    }
                }

                // println!("{e:#?}");
                // if let Some(parse_attempt) = e.parse_attempts() {
                //     let mut expected_tokens = parse_attempt
                //         .expected_tokens()
                //         .iter()
                //         .filter_map(|t| {
                //             let token = t.to_string().to_lowercase();
                //             let token = token.trim();
                //             let ignored = ["//", "/*", "+", "-", "0..9"];
                //             if token.is_empty() || ignored.contains(&token) {
                //                 return None;
                //             }
                //             Some(token.to_string())
                //         })
                //         .collect::<Vec<_>>();
                //     expected_tokens.sort();
                //     println!("{expected_tokens:?}");
                // }
                continue;
            }
        };
        let query = parsed.query;
        if let Ok(pq_conn_str) = std::env::var("PGCONNSTR") {
            info!("Executing query: {query}");
            let mut client = postgres::Client::connect(&pq_conn_str, postgres::tls::NoTls)
                .unwrap_or_else(|e| panic!("Failed to connect to db: {}", e));
            let with_clause = parsed.with_clause.unwrap_or_default();
            let wrapq = format!("SELECT json_agg(q) FROM ({with_clause} select * {query}) AS q");
            let row = client
                .query_one(&wrapq, &[])
                .unwrap_or_else(|e| panic!("Query failed: {}", e));
            let json: serde_json::Value = row.get(0);
            println!("{json}");
        } else {
            if let Some(with_clause) = parsed.with_clause {
                println!("With clause:\n{with_clause}");
            }
            println!("Query:\n{query}");
        }
    }
    Ok(())
}
