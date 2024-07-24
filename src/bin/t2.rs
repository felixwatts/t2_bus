use std::{net::{Ipv4Addr, SocketAddr}, path::PathBuf};
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
        #[command(subcommand)]
        command: ConnectCommands,
    },
    Cat{
        #[arg(short, long)]
        topic: Option<String>,
        #[command(subcommand)]
        command: ConnectCommands, 
    },
    Pub{
        #[arg(short, long)]
        topic: String,
        #[arg(short, long)]
        value: f32,
        #[command(subcommand)]
        command: ConnectCommands,
    }
}

#[derive(Subcommand)]
enum ConnectCommands {
    Unix {
        // #[arg(short, long)]
        addr: Option<PathBuf>
    },
    Tcp{
        // #[arg(short, long)]
        addr: Option<SocketAddr>
    }
}

#[derive(Subcommand)]
enum ChildSubCommands {
    Grandchild {
        #[arg(short, long)]
        some_option: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        println!("{}", &e.to_string());
    }
}

async fn run() -> BusResult<()> {
    let command = Cli::parse();
    match command.command {
        Commands::Serve { command } => {
            match command {
                ConnectCommands::Unix{addr} => {
                    let addr = addr.unwrap_or(DEFAULT_BUS_ADDR.into());
                    let stopper = t2_bus::transport::unix::serve(&addr)?;
                    stopper.join().await?;
                },
                ConnectCommands::Tcp { addr } => {
                    let addr = addr.unwrap_or(SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_BUS_PORT));
                    let stopper = t2_bus::transport::tcp::serve(&addr).await?;
                    stopper.join().await?;
                },
            }
        },
        Commands::Cat { command, topic } => {
            let client = connect(&command).await?;
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
        Commands::Pub { command, topic, value } => {
            let client = connect(&command).await?;
            client.publish(&topic, &Number(value)).await?;
        },
    }

    Ok(())
}

async fn connect(command: &ConnectCommands) -> BusResult<Client> {
    match command {
        ConnectCommands::Unix{addr} => {
            let addr = addr.to_owned().unwrap_or(DEFAULT_BUS_ADDR.into());
            t2_bus::transport::unix::connect(&addr).await
        },
        ConnectCommands::Tcp { addr } => {
            let addr = addr.unwrap_or(SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_BUS_PORT));
            t2_bus::transport::tcp::connect(addr).await
        },
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Number(f32);

impl PublishProtocol for Number{
    fn prefix() -> &'static str {
        "number"
    }
}