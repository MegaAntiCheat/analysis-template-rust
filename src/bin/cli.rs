use analysis_template::{
    dev_print,
    lib::{
        algorithm::{analyse, get_algorithms, CheatAlgorithm},
        parameters::Parameters,
    },
    SILENT,
};
use std::{
    collections::HashMap,
    env,
    fs::{self},
};

use anyhow::Error;

pub use tf_demo_parser::{Demo, DemoParser, Parse, ParseError, ParserState, Stream};

use getopts::Options;

fn main() -> Result<(), Error> {
    let start = std::time::Instant::now();

    let mut opts = Options::new();
    opts.optopt("i", "input", "set input file path", "PATH");
    opts.optflag(
        "q",
        "quiet",
        "silence all output except for the final JSON string",
    );
    opts.optflag(
        "Q",
        "quiet-pretty",
        "same as -q, but with more human-readable json",
    );
    opts.optmulti("a", "algorithm", "specify the algorithm to run. Include multiple -a flags to run multiple algorithms. If not specified, the default algorithms are run.", "ALGORITHM [-a ALGORITHM]...");
    opts.optflag("c", "count", "only print the number of detections");
    opts.optflag("h", "help", "print this help menu");
    opts.optopt(
        "p",
        "params",
        "Parameter json file to use for the algorithms",
        "PATH",
    );

    fn print_help(opts: &getopts::Options) {
        println!("{}", opts.usage("Usage: analysis-template [options]"));
    }

    let matches = match opts.parse(env::args().skip(1)) {
        Ok(m) => m,
        Err(_) => {
            print_help(&opts);
            return Ok(());
        }
    };

    if matches.opt_present("h") {
        print_help(&opts);
        return Ok(());
    }

    let demo_path = matches.opt_str("i").expect("No input file path provided");
    let silent = matches.opt_present("q") || matches.opt_present("Q");
    let pretty = matches.opt_present("Q");
    SILENT.store(silent, std::sync::atomic::Ordering::Relaxed);

    // To add your algorithm, call new() on it and store inside a Box.
    // You will need to import it at the top of the file.
    let mut algorithms: Vec<Box<dyn CheatAlgorithm + Send>> = get_algorithms();
    let specified_algorithms = matches.opt_strs("a");
    if specified_algorithms.is_empty() && !matches.opt_present("a") {
        algorithms.retain(|a| a.default());
    } else {
        algorithms.retain(|a| specified_algorithms.contains(&a.algorithm_name().to_string()));
    }

    if let Some(param_file_path) = matches.opt_str("p") {
        dev_print!("Loading parameters from {}:", param_file_path);
        let c = fs::read(param_file_path).expect("Couldn't read parameter file");
        let config = serde_json::from_slice::<HashMap<String, Parameters>>(&c)
            .expect("Couldn't decode parameter file");
        for algo in algorithms.iter_mut() {
            let algorithm_name: String = algo.algorithm_name().to_owned();

            let algo_params = algo.params();
            if algo_params.is_none() {
                continue;
            }
            let algo_params = algo_params.unwrap();
            dev_print!("  {}", algorithm_name);
            algo_params.iter_mut().for_each(|(k, v)| {
                if let Some(config_params) = config.get(&algorithm_name) {
                    if let Some(param_value) = config_params.get(k) {
                        *v = param_value.clone();
                        dev_print!("    {} = {:?} (changed)", k, v);
                    } else {
                        dev_print!("    {} = {:?} (default)", k, v);
                    }
                } else {
                    dev_print!("    {} = {:?} (default)", k, v);
                }
            });
        }
    }

    let unknown_algorithms: Vec<String> = specified_algorithms
        .into_iter()
        .filter(|a| algorithms.iter().all(|b| b.algorithm_name() != *a))
        .collect();
    if !unknown_algorithms.is_empty() {
        panic!(
            "Unknown algorithms specified: {}",
            unknown_algorithms.join(", ")
        );
    } else if algorithms.is_empty() {
        panic!("No algorithms specified");
    }

    let file = fs::read(demo_path)?;
    let demo: Demo = Demo::new(&file);
    let analyser = analyse(&demo, algorithms)?;

    if start.elapsed().as_secs() >= 10 {
        analyser.print_metadata();
    }

    if SILENT.load(std::sync::atomic::Ordering::Relaxed) {
        analyser.print_detection_json(pretty);
    } else if matches.opt_present("c") {
        analyser.print_detection_summary();
    } else {
        analyser.print_detection_json(true);
    }

    let total_ticks = analyser.get_tick_count_u32();
    let total_time = start.elapsed().as_secs_f64();
    let total_tps = (total_ticks as f64) / total_time;
    dev_print!(
        "Done! (Processed {} ticks in {:.2} seconds averaging {:.2} tps)",
        total_ticks,
        total_time,
        total_tps
    );

    Ok(())
}
