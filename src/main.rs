#[macro_use]
extern crate log;

use std::collections::HashMap;

use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{route, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use clap::Parser;

use crate::proxy::{process, ProxyOptions};

mod proxy;

#[route("/", method = "POST")]
async fn index(
    query: web::Query<HashMap<String, String>>,
    body: web::Bytes,
    req: HttpRequest,
    options: Data<ProxyOptions>,
) -> impl Responder {
    process(query, body, req, options)
        .await
        .unwrap_or_else(|e| HttpResponse::InternalServerError().body(e.to_string()))
}

#[derive(Parser)]
struct Args {
    /// Address this proxy listens from.
    #[clap(long, short, default_value = "0.0.0.0")]
    host: String,
    /// Port this proxy listens from.
    #[clap(long, short, default_value_t = 6632)]
    port: u16,
    /// Upstream printer (e.g. localhost:631/printers/Virtual_PDF_Printer)
    #[clap(long, short)]
    upstream: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    let args = Args::parse();
    info!(
        "Starting at ipp://{}:{}/ -> ipp://{}",
        args.host, args.port, args.upstream
    );

    let proxy_options = ProxyOptions {
        upstream: args.upstream,
    };

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(Data::new(proxy_options.clone()))
            .service(index)
    })
    .bind((args.host, args.port))?
    .run()
    .await
}
