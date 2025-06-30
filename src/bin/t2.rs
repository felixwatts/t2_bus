use std::collections::HashSet;
use std::{fmt::Display, net::SocketAddr, path::PathBuf};
use clap::{command, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use t2_bus::prelude::*;
use regex::Regex;
use std::net::AddrParseError;

pub const DEFAULT_BUS_ADDR: &str = ".t2";
pub const DEFAULT_BUS_PORT: u16 = 4242;
const BUS_ADDR_NAME_RGX: &str = r"^[a-z_]+$";
const BUS_ADDR_RGX: &str = r"^(tcp|unix|name):(.+)$";
const BUS_ADDR_CONFIG_RGX: &str = r"(.+?) (tcp|unix):(.*?)\n";

#[derive(Debug)]
enum ResolvedBusAddr{
    Tcp(String),
    Unix(PathBuf)
}

impl Display for ResolvedBusAddr{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self{
            ResolvedBusAddr::Tcp(addr) => write!(f, "tcp:{}", addr),
            ResolvedBusAddr::Unix(addr) => write!(f, "unix:{}", addr.display()),
        }
    }
}

fn validate_bus_addr_name(s: &str) -> Result<String, String> {
    let valid = Regex::new(BUS_ADDR_NAME_RGX).unwrap().is_match(s);
    match valid {
        true => Ok(s.to_string()),
        false => Err(format!("Invalid bus address name, must contain only lowercase and underscore.")),
    }
}

fn validate_bus_addr(s: &str) -> Result<String, String> {
    let valid = Regex::new(BUS_ADDR_RGX).unwrap().is_match(s);
    match valid {
        true => Ok(s.to_string()),
        false => Err(format!("Invalid bus address. Must be in the format of (tcp|unix|name):<address or name>")),
    }
}

#[derive(Parser)]
#[command(version = "1.0", author = "Felix Watts", about = "Utilities related to the t2 bus.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve {
        #[arg(value_parser = validate_bus_addr)]
        addr: Vec<String>
    },
    Sub{
        topic: String,
        #[arg(value_parser = validate_bus_addr)]
        addr: Option<String>
    },
    Pub{
        topic: String,
        value: String,
        #[arg(value_parser = validate_bus_addr)]
        addr: Option<String>
    },
    Lst{
        topic: String,
        #[arg(value_parser = validate_bus_addr)]
        addr: Option<String>
    },
    Register{
        #[arg(value_parser = validate_bus_addr_name)]
        name: String,
        #[arg(value_parser = validate_bus_addr)]
        addr: String,
        #[arg(long)]
        default: bool
    },
    Unregister{
        #[arg(value_parser = validate_bus_addr_name)]
        name: String
    }
}

impl Commands{
    fn validate(&self) -> Result<(), Error> {
        match self{
            Commands::Serve { .. } => Ok(()),
            Commands::Sub { ..} => Ok(()),
            Commands::Lst { ..} => Ok(()),
            Commands::Pub { topic, value, .. } => {
                if !(topic.starts_with("f32/") || topic.starts_with("string/")) {
                    return Err(Error("Unknown protocol".into()))
                }

                if topic.starts_with("f32/") && value.parse::<f32>().is_err() {
                    return Err(Error("When the topic starts with f32/ then the value must be a valid f32".into()))
                }

                Ok(())
            },
            Commands::Register { .. } => Ok(()),
            Commands::Unregister { .. } => Ok(()),
        }
    }
}

struct Error(String);

impl From<std::io::Error> for Error{
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<BusError> for Error{
    fn from(value: BusError) -> Self {
        Self(value.to_string())
    }
}

impl From<AddrParseError> for Error{
    fn from(value: AddrParseError) -> Self {
        Self(value.to_string())
    }
}

impl Display for Error{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        println!("{}", &e.to_string());
    }
}

async fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    cli.command.validate()?;
    match cli.command {
        Commands::Serve { addr } => {
            let mut builder = t2_bus::prelude::ServerBuilder::new();

            for addr in addr.into_iter() {
                let resolved_addr = resolve_addr(&Some(addr))?;
                match resolved_addr {
                    ResolvedBusAddr::Tcp(addr) => {
                        builder = builder.serve_tcp(addr.parse::<SocketAddr>()?);
                    },
                    ResolvedBusAddr::Unix(addr) => {
                        builder = builder.serve_unix_socket(addr);
                    },
                }
            }

            let (stopper, _) = builder.build().await?;

            stopper.join().await?;
        },
        Commands::Sub { addr, topic } => {
            let client = build_client(&addr).await?;

            let mut sub = client.subscribe_bytes(&topic).await?;
            while let Some(msg) = sub.recv().await {
                let val_str = if msg.topic.starts_with("f32/") {
                    let bytes: Vec<u8> = msg.payload.into();
                    let payload: F32Protocol = t2_bus::transport::cbor_codec::deser(&bytes[..])?;
                    payload.0.to_string()
                } else if msg.topic.starts_with("string/") {
                    let bytes: Vec<u8> = msg.payload.into();
                    let payload: StringProtocol = t2_bus::transport::cbor_codec::deser(&bytes[..])?;
                    payload.0
                } else {
                    let bytes: Vec<u8> = msg.payload.into();
                    format!("0x{}", &hex::encode(bytes))
                };

                println!("{}: {val_str}", msg.topic)
            }
        },
        Commands::Lst { addr, topic } => {
            let client = build_client(&addr).await?;
            let mut encountered_topics = HashSet::new();

            let mut sub = client.subscribe_bytes(&topic).await?;
            while let Some(PubMsg{ topic, .. }) = sub.recv().await {
                if !encountered_topics.contains(&topic) {
                    println!("{topic}");
                    encountered_topics.insert(topic);
                }
            }
        },
        Commands::Pub { addr, topic, value } => {
            let client = build_client(&addr).await?;
            let payload = if topic.starts_with("f32/") {
                t2_bus::transport::cbor_codec::ser(&F32Protocol(value.parse().unwrap()))?

            } else {
                t2_bus::transport::cbor_codec::ser(&StringProtocol(value.parse().unwrap()))?
            };

            client.publish_bytes(&topic, payload).await?;
        },
        Commands::Register { name, addr, default } => {
            let home = match std::env::var("HOME") {
                Ok(home) => home,
                Err(_) => return Err(Error("HOME environment variable not set".into()))
            };
            let resolved_addr = resolve_addr(&Some(addr))?;
            let mut config = parse_config()?;
            config.retain(|(n, _)| n != &name);

            if default {
                config.insert(0, (name, resolved_addr));
            } else {
                config.push((name, resolved_addr));
            }

            let config_str = config
                .iter()
                .map(|(name, addr)| format!("{} {}\n", name, addr.to_string()))
                .collect::<Vec<_>>()
                .join("");
            let path = PathBuf::from(home).join(".t2");
            std::fs::write(path, config_str)?;
        },
        Commands::Unregister { name } => {
            let home = match std::env::var("HOME") {
                Ok(home) => home,
                Err(_) => return Err(Error("HOME environment variable not set".into()))
            };
            let mut config = parse_config()?;
            config.retain(|(n, _)| n != &name);
            let config_str = config
                .iter()
                .map(|(name, addr)| format!("{} {}\n", name, addr.to_string()))
                .collect::<Vec<_>>()
                .join("");
            let path = PathBuf::from(home).join(".t2");
            std::fs::write(path, config_str)?;
        }
    }

    Ok(())
}

async fn build_client(addr: &Option<String>) -> Result<Client, Error>{
    let resolved_addr = resolve_addr(addr)?;

    match resolved_addr {
        ResolvedBusAddr::Tcp(addr) => {
            Ok(t2_bus::transport::tcp::connect(addr.parse::<SocketAddr>()?).await?)
        },
        ResolvedBusAddr::Unix(addr) => {
            Ok(t2_bus::transport::unix::connect(&addr).await?)
        }
    }
}

fn resolve_addr(addr: &Option<String>) -> Result<ResolvedBusAddr, Error>{
    match addr{
        Some(addr) => {
            let matches = regex::Regex::new(BUS_ADDR_RGX).unwrap().captures(addr).unwrap();
            let typ = matches.get(1).unwrap().as_str();
            let addr = matches.get(2).unwrap().as_str();
            match typ{
                "tcp" => Ok(ResolvedBusAddr::Tcp(addr.to_string())),
                "unix" => Ok(ResolvedBusAddr::Unix(PathBuf::from(addr))),
                "name" => {
                    let config = parse_config()?;
                    let addr = config.into_iter().find(|(name, _)| name == addr).ok_or(Error("Address not found in config".into()))?;
                    Ok(addr.1)
                },
                _ => Err(Error("Invalid address type".into()))
            }
        },
        None => {
            let mut config = parse_config()?;
            if config.is_empty() {
                return Err(Error("No default address found in config. Use the register command to add one or specify an address".into()));
            }
            let addr = config.remove(0).1;
            Ok(addr)
        }
    }
}

fn parse_config() -> Result<Vec<(String, ResolvedBusAddr)>, Error>{
    let home = match std::env::var("HOME") {
        Ok(home) => home,
        Err(_) => return Ok(Vec::new()),
    };

    let path = PathBuf::from(home).join(".t2");
    if !path.exists() {
        return Ok(Vec::new());
    }
    
    let config = std::fs::read_to_string(path).unwrap();
    
    let addrs= regex::Regex::new(BUS_ADDR_CONFIG_RGX)
        .unwrap()
        .captures_iter(&config)
        .map(|m| {
            let name = m.get(1).unwrap().as_str();
            let addr = m.get(3).unwrap().as_str();
            let addr_type = m.get(2).unwrap().as_str();
            let addr = match addr_type{
                "tcp" => ResolvedBusAddr::Tcp(addr.to_string()),
                "unix" => ResolvedBusAddr::Unix(PathBuf::from(addr)),
                _ => panic!()
            };
            (name.to_string(), addr)
        })
        .collect::<Vec<_>>();

    Ok(addrs)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct F32Protocol(f32);

impl PublishProtocol for F32Protocol{
    fn prefix() -> &'static str {
        "f32"
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct StringProtocol(String);

impl PublishProtocol for StringProtocol{
    fn prefix() -> &'static str {
        "string"
    }
}