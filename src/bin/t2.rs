use std::{fmt::Display, net::SocketAddr, path::PathBuf};
use clap::{command, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use t2_bus::prelude::*;
use tokio::net::ToSocketAddrs;


pub const DEFAULT_BUS_ADDR: &str = ".t2";
pub const DEFAULT_BUS_PORT: u16 = 4242;

#[derive(Parser)]
#[command(version = "1.0", author = "Felix Watts", about = "Utilities related to the t2 bus.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve {
        #[arg(long)]
        tcp: Vec<SocketAddr>,
        #[arg(long)]
        unix: Vec<PathBuf>,
    },
    Sub{
        #[arg(long)]
        topic: String,
        #[arg(long)]
        tcp: Option<String>,
        #[arg(long)]
        unix: Option<PathBuf>,
    },
    Pub{
        #[arg(long)]
        topic: String,
        #[arg(long)]
        value: String,
        #[arg(long)]
        tcp: Option<String>,
        #[arg(long)]
        unix: Option<PathBuf>,
    }
}

impl Commands{
    fn validate(&self) -> Result<(), Error> {
        match self{
            Commands::Serve { .. } => Ok(()),
            Commands::Sub { tcp, unix, .. } => {

                if tcp.is_none() == unix.is_none() {
                    return Err(Error("You must specify either a tcp or a unix connection.".into()))
                }
                Ok(())
            },
            Commands::Pub { topic, value, tcp, unix } => {
                if tcp.is_none() == unix.is_none() {
                    return Err(Error("You must specify either a tcp or a unix connection.".into()))
                }

                if !(topic.starts_with("f32/") || topic.starts_with("string/")) {
                    return Err(Error("Unknown protocol".into()))
                }

                if topic.starts_with("f32/") && value.parse::<f32>().is_err() {
                    return Err(Error("When the topic starts with f32/ then the value must be a valid f32".into()))
                }

                Ok(())
            },
        }
    }
}

struct Error(String);

impl From<BusError> for Error{
    fn from(value: BusError) -> Self {
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
        Commands::Serve { tcp, unix } => {
            let mut builder = t2_bus::prelude::ServerBuilder::new();

            for addr in tcp.into_iter() {
                builder = builder.serve_tcp(addr);
            }

            for addr in unix.into_iter() {
                builder = builder.serve_unix_socket(addr);
            }

            let (stopper, _) = builder.build().await?;

            stopper.join().await?;
        },
        Commands::Sub { tcp, unix, topic } => {
            let client = build_client(&tcp, &unix).await?;

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
        Commands::Pub { tcp, unix, topic, value } => {
            let client = build_client(&tcp, &unix).await?;
            let payload = if topic.starts_with("f32/") {
                t2_bus::transport::cbor_codec::ser(&F32Protocol(value.parse().unwrap()))?

            } else {
                t2_bus::transport::cbor_codec::ser(&StringProtocol(value.parse().unwrap()))?
            };

            client.publish_bytes(&topic, payload).await?;
        },
    }

    Ok(())
}

async fn build_client(tcp: &Option<impl ToSocketAddrs>, unix: &Option<PathBuf>) -> Result<Client, Error>{
    match tcp {
        Some(addr) => {
            Ok(t2_bus::transport::tcp::connect(addr).await?)
        },
        None => {
            match unix {
                Some(addr) => {
                    Ok(t2_bus::transport::unix::connect(addr).await?)
                },
                None => { Err(Error("You must specify either a unix or a tcp connection".into()))}
            }
        }
    }
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