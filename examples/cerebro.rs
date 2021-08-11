use circ::front::cerebro::parser::*;

use circ::target::aby::output::write_aby_exec;
use circ::target::aby::trans::to_aby;

use circ::ir::term::extras::Letified;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "cerebro", about = "CirC: the circuit compiler")]
struct Options {
    /// Input file
    #[structopt(parse(from_os_str))]
    cerebro_path: PathBuf,

    #[structopt(short, long, name = "PARTIES")]
    parties: Option<u8>,
}


fn main() {

    env_logger::Builder::from_default_env()
        .format_level(false)
        .format_timestamp(None)
        .init();
        let options = Options::from_args();
        println!("{:?}", options);

    let path = options.cerebro_path;
    let parties = options.parties.unwrap();

    let module = read_from_file(path.clone()).unwrap();
    let computation = convert(module, parties);

    for output in computation.outputs {
        //let letified = Letified(output);
        println!("{}", output);
    }

    // println!("Converting Cerebro to aby");
    // let aby = to_aby(computation);
    // let path = PathBuf::from(path.clone());
    // write_aby_exec(aby, path);

}