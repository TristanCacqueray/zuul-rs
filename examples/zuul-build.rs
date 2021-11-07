//! # zuul-build
//!
//! `zuul-build` is like 'tail -f' for builds result.
use clap::{App, Arg};
use env_logger;
use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use std::time::Duration;
use zuul;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Get CLI args
    let matches = App::new("A zuul client to stream build result")
        .arg(
            Arg::with_name("url")
                .long("url")
                .takes_value(true)
                .required(true)
                .help("The zuul api"),
        )
        .arg(
            Arg::with_name("since")
                .long("since")
                .takes_value(true)
                .help("Catchup until a certain build"),
        )
        .arg(Arg::with_name("json").long("json").help("Output json"))
        .get_matches();
    let client = zuul::create_client(matches.value_of("url").unwrap()).expect("Invalid url");
    let since = matches.value_of("since").map(|s| String::from(s));
    let json = matches.is_present("json");

    // Start the build stream
    let s = client.builds_tail(Duration::from_secs(10), since);
    pin_mut!(s);

    // Print new builds
    while let Some(build) = s.next().await {
        if json {
            match serde_json::to_string(&build) {
                Ok(v) => println!("{}", v),
                Err(v) => println!("{:?}", v),
            }
        } else {
            println!(
                "{} {} {} {}",
                build.log_url.unwrap_or("N/A".to_string()),
                build.uuid,
                build.project,
                build.job_name
            )
        }
    }
}
