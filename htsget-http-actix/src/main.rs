use actix_web::{App, HttpServer};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};
use std::env::args;
mod get;
mod post;

const USAGE: &str =
  "Usage: [-p|--port {port number}] [-i|--ip {ip address}]\nThe default address is 127.0.0.1:8080";
const DEFAULT_IP: &str = "127.0.0.1";
const DEFAULT_PORT: &str = "8080";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let address = match parse_args() {
    Some(result) => result,
    None => {
      println!("{}", USAGE);
      return Ok(());
    }
  };
  HttpServer::new(|| {
    let htsget = HtsGetFromStorage::new(
      LocalStorage::new("data").expect("Couldn't create a Storage with the provided path"),
    );
    App::new()
      .data(htsget)
      .service(get::reads)
      .service(get::variants)
      .service(post::reads)
      .service(post::variants)
  })
  .bind(address)?
  .run()
  .await
}

fn parse_args() -> Option<String> {
  let args: Vec<String> = args().skip(1).collect();
  let mut ip = DEFAULT_IP.to_string();
  let mut port = DEFAULT_PORT.to_string();
  let mut counter = 0;
  while counter < args.len() {
    match args[counter].as_str() {
      "-p" | "--port" => {
        counter += 1;
        port = args.get(counter)?.clone();
      }
      "-i" | "--ip" => {
        counter += 1;
        ip = args.get(counter)?.clone();
      }
      _ => return None,
    }
    counter += 1;
  }
  Some(format!("{}:{}", ip, port))
}
