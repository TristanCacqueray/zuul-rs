use clap::{App, Arg};
use env_logger;
use zuul;

#[tokio::main]
async fn main() {
    env_logger::init();
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
    let client = zuul::create_client(api).unwrap();
    match matches.value_of("build") {
        Some(build) => {
            println!("Getting build: {}", build);
        }
        None => {
            let builds = client.builds().await.unwrap();
            println!("Builds: {:?}", builds);
        }
    }
}
