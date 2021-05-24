use std::path::PathBuf;
use bus::BusResult;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "bus", about = "T2 service bus")]
struct BusOpt {
    #[structopt(parse(from_os_str), default_value = bus::DEFAULT_BUS_ADDR)]
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
    let stopper = bus::serve_bus_unix_socket(&opt.addr)?;
    let (result_1, result_2) = stopper.join().await?;
    result_1?;
    result_2?;
    Ok(())
}
