use std::{fmt::Display, net::SocketAddr, path::PathBuf};
use clap::{command, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use t2_bus::prelude::*;


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
        #[arg(short, long)]
        tcp: Vec<SocketAddr>,
        #[arg(short, long)]
        unix: Vec<PathBuf>,
    },
    Cat{
        #[arg(short, long)]
        topic: Option<String>,
        #[arg(short, long)]
        tcp: Option<SocketAddr>,
        #[arg(short, long)]
        unix: Option<PathBuf>,
    },
    Pub{
        #[arg(short, long)]
        topic: String,
        #[arg(short, long)]
        value: f32,
        #[arg(short, long)]
        tcp: Option<SocketAddr>,
        #[arg(short, long)]
        unix: Option<PathBuf>,
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
    let command = Cli::parse();
    match command.command {
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
        Commands::Cat { tcp, unix, topic } => {
            let client = build_client(&tcp, &unix).await?;
            let topic = topic.unwrap_or("**".into());
            let mut sub = client.subscribe_bytes(&topic).await?;
            while let Some(msg) = sub.recv().await {
                let bytes: Vec<u8> = msg.payload.into();
                let number: Number = t2_bus::transport::cbor_codec::deser(&bytes[..])?;
                let number = number.0;
                let topic = msg.topic;
                println!("{topic}: {number}")
            }
        },
        Commands::Pub { tcp, unix, topic, value } => {
            let client = build_client(&tcp, &unix).await?;
            client.publish(&topic, &Number(value)).await?;
        },
    }

    Ok(())
}

async fn build_client(tcp: &Option<SocketAddr>, unix: &Option<PathBuf>) -> Result<Client, Error>{
    match tcp {
        Some(addr) => {
            Ok(t2_bus::transport::tcp::connect(addr).await?)
        },
        None => {
            match unix {
                Some(addr) => {
                    Ok(t2_bus::transport::unix::connect(&addr).await?)
                },
                None => { return Err(Error("You must specify either a unix or a tcp connection".into())); }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Number(f32);

impl PublishProtocol for Number{
    fn prefix() -> &'static str {
        "number"
    }
}