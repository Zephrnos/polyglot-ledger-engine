use clap::Parser;

mod core;
mod models;

#[derive(Parser, Debug)]
#[command(about = "A worker process that requires a traceable ID.")]
struct Args {
    #[arg(short = 'w', long = "worker_id")]
    worker_id: String,
}

fn main() {

    let args = Args::parse();

    let _id = args.worker_id;

}
