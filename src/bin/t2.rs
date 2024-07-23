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
        #[command(subcommand)]
        command: ConnectCommands,
        #[arg(short, long)]
        topic: Option<String>  
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


// #[derive(Parser)]
// #[command(version, about, long_about = None)]
// struct Cli {
//     #[command(subcommand)]
//     command: MainCommands
// }

// #[derive(Subcommand)]
// enum MainCommands{
//     /// Start a bus server
//     Serve{
//         /// The 
//         #[arg(short, long)]
//         addr: TransportParams
//     },    
//     // /// Start a bus client
//     // Connect{
//     //     #[command(subcommand)]
//     //     command: ConnectCommands
//     // }
// }

// #[derive(Parser)]
// enum TransportParams{
//     Unix(UnixParams),
//     Tcp(TcpParams)
// }

// #[derive(Parser)]
// struct ServeParams{

// }

// #[derive(Subcommand)]
// enum ConnectCommands{
//     /// Connect to a server using the Unix socket transport
//     Unix(UnixParams)
// }

// #[derive(Parser)]
// struct UnixParams{
//     /// The Unix socket address to use for connecting or serving. Defaults to .t2
//     addr: Option<PathBuf>
// }

// #[derive(Parser)]
// struct TcpParams{
//     /// The TCP socket address to use for connecting or serving. Defaults to localhost:4242
//     addr: Option<SocketAddr>
// }

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
            let mut sub = client.subscribe_bytes("**").await?;
            while let Some(msg) = sub.recv().await {

            }
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