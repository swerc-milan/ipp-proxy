#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::db::{Database, Db};
use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{route, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use clap::Parser;
use sqlx::sqlite::SqlitePoolOptions;

use crate::proxy::{process, ProxyOptions};

mod db;
mod pdf;
mod proxy;

#[route("/{tail}*", method = "POST")]
async fn index(
    db: web::Data<Db>,
    path: web::Path<(String,)>,
    body: web::Bytes,
    req: HttpRequest,
    options: Data<ProxyOptions>,
) -> impl Responder {
    let params = parse_uri_path(&path.0);
    let database = Database::new(&db);
    let peer_addr = match req.peer_addr() {
        Some(peer_ip) => peer_ip,
        None => return HttpResponse::BadRequest().body("Cannot get client ip"),
    };
    let password = params.get("password").map(|s| s.as_ref());
    let team = match database.get_team(peer_addr.ip(), password).await {
        Ok(team) => team,
        Err(e) => {
            error!("Failed to get team: {:?}", e);
            return HttpResponse::Forbidden().body("You are not authorized");
        }
    };

    process(database, team, req, body, &options)
        .await
        .unwrap_or_else(|e| HttpResponse::InternalServerError().body(e.to_string()))
}

fn parse_uri_path(path: &str) -> HashMap<String, String> {
    path.split('/')
        .flat_map(|piece| {
            if let Some((key, value)) = piece.split_once('=') {
                Some((key.to_string(), value.to_string()))
            } else {
                None
            }
        })
        .collect()
}

#[derive(Parser)]
struct Args {
    /// Address this proxy listens from.
    #[clap(long, short, default_value = "0.0.0.0")]
    host: String,
    /// Port this proxy listens from.
    #[clap(long, short, default_value_t = 6632)]
    port: u16,
    /// Path to the database file.
    #[clap(long, short, default_value = "./db.sqlite3")]
    database: String,
    /// Path to store the jobs.
    #[clap(long, short, default_value = "./jobs")]
    jobs_dir: PathBuf,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    let args: Args = Args::parse();
    info!("Starting at ipp://{}:{}/", args.host, args.port);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&args.database)
        .await
        .expect("Failed to connect to db");

    let proxy_options = ProxyOptions {
        jobs_dir: args.jobs_dir,
    };
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(Data::new(pool.clone()))
            .app_data(Data::new(proxy_options.clone()))
            .service(index)
    })
    .bind((args.host, args.port))?
    .run()
    .await
}
