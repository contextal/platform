//! Shared configuration items

use serde::Deserialize;

#[derive(Deserialize)]
/// Message broker configuration
pub struct BrokerConfig {
    /// The broker hostname
    pub host: Option<String>,
    /// The broker port
    pub port: Option<u16>,
    /// The username to use with the broker
    pub user: Option<String>,
    /// The password to use with the broker
    pub pass: Option<String>,
}

#[derive(Deserialize)]
/// Clamd service configuration
pub struct ClamdServiceConfig {
    /// The clamd hostname
    pub host: String,
    /// The clamd port
    pub port: u16,
    /// The path to the objects store in the clamd container
    pub objects_path: String,
}

#[derive(Deserialize)]
/// GraphDB configuration
pub struct DBConfig {
    /// The database name
    pub dbname: String,
    /// The host
    pub host: String,
    /// The port
    pub port: u16,
    /// The authentication username
    pub user: String,
    /// The authentication password
    pub pass: String,
}
