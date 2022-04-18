#[macro_use]
extern crate log;

use std::collections::HashMap;

use crate::db::{Database, Db};
use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{route, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use clap::Parser;
use sqlx::sqlite::SqlitePoolOptions;

use crate::proxy::{process, ProxyOptions};

mod db;
mod proxy;

#[route("/", method = "POST")]
async fn index(
    db: web::Data<Db>,
    query: web::Query<HashMap<String, String>>,
    body: web::Bytes,
    req: HttpRequest,
    options: Data<ProxyOptions>,
) -> impl Responder {
    let database = Database::new(&db);
    let peer_addr = match req.peer_addr() {
        Some(peer_ip) => peer_ip,
        None => return HttpResponse::BadRequest().body("Cannot get client ip"),
    };
    let password = query.0.get("password").map(|s| s.as_ref());
    let team = match database.get_team(peer_addr.ip(), password).await {
        Ok(team) => team,
        Err(e) => {
            error!("Failed to get team: {:?}", e);
            return HttpResponse::Forbidden().body("You are not authorized");
        }
    };

    process(database, team, body, req, options)
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
    /// Upstream printer (e.g. localhost:631/printers/Virtual_PDF_Printer)
    #[clap(long, short, default_value = "./db.sqlite3")]
    database: String,
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

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&args.database)
        .await
        .expect("Failed to connect to db");

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(Data::new(proxy_options.clone()))
            .app_data(Data::new(pool.clone()))
            .service(index)
    })
    .bind((args.host, args.port))?
    .run()
    .await
}
