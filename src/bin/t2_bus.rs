use std::path::PathBuf;
use t2_bus::prelude::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "bus", about = "T2 service bus")]
struct BusOpt {
    #[structopt(parse(from_os_str), default_value = t2_bus::DEFAULT_BUS_ADDR)]
    addr: PathBuf,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        println!("{}", &e.to_string());
    }
}

async fn run() -> BusResult<()> {
    let opt = BusOpt::from_args();
    let stopper = listen_and_serve_unix(&opt.addr)?;
    stopper.join().await?;
    Ok(())
}
