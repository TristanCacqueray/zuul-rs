use clap::{App, Arg};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Build {
    url: String,
    job_name: String,
}

fn main() {
    let matches = App::new("A zuul client")
        .arg(
            Arg::with_name("url")
                .long("url")
                .takes_value(true)
                .required(true)
                .help("The zuul api"),
        )
        .arg(
            Arg::with_name("build")
                .long("build")
                .takes_value(true)
                .help("A build uuid"),
        )
        .get_matches();
    let api = matches.value_of("url").unwrap();
    println!("The api: {}", api);
    match matches.value_of("build") {
        Some(build) => println!("Getting build: {}", build),
        None => println!("Getting any build"),
    }
}
