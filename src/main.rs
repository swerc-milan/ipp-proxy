#[macro_use]
extern crate log;

use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{route, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use anyhow::{anyhow, Error};
use clap::Parser;
use ipp::parser::IppParser;
use ipp::prelude::*;
use ipp::reader::IppReader;
use reqwest::header::HeaderMap;
use reqwest::RequestBuilder;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Read;

#[derive(Debug, Clone)]
struct ProxyOptions {
    upstream: String,
}

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

async fn process(
    query: web::Query<HashMap<String, String>>,
    body: web::Bytes,
    req: HttpRequest,
    options: Data<ProxyOptions>,
) -> Result<HttpResponse, Error> {
    let reader = body.as_ref().to_vec();
    let reader = IppReader::new(Cursor::new(reader));
    let mut parsed = IppParser::new(reader).parse()?;

    let operation = Operation::try_from(parsed.header().operation_or_status)
        .map_err(|_| anyhow!("Invalid op"))?;
    debug!("{:?} received from {:?}", operation, req.peer_addr());

    patch_ipp_message(&mut parsed, &options.upstream);
    let (ipp_response, headers) =
        forward_to_upstream_printer(parsed, req, &options.upstream).await?;

    let mut http_response = HttpResponse::Ok();
    for (name, value) in headers {
        if let Some(name) = name {
            http_response.insert_header((name, value));
        }
    }
    Ok(http_response.body(ipp_response.to_bytes()))
}

/// Patch the printer-uri attribute setting the correct upstream URI.
fn patch_ipp_message(request: &mut IppRequestResponse, upstream_printer: &str) {
    let attrs = request.attributes_mut();
    for group in attrs.groups_mut() {
        for attr in group.attributes_mut().values_mut() {
            if attr.name() == IppAttribute::PRINTER_URI {
                *attr = IppAttribute::new(
                    IppAttribute::PRINTER_URI,
                    IppValue::Uri(format!("ipp://{}", upstream_printer)),
                );
            }
        }
    }
}

fn build_upstream_http_request(
    message: IppRequestResponse,
    http_request: HttpRequest,
    upstream_printer: &str,
) -> Result<RequestBuilder, Error> {
    let client = reqwest::Client::new();
    let headers = http_request.headers();
    let mut builder = client.post(format!("http://{}", upstream_printer));
    for (name, value) in headers {
        let name = name.to_string().to_lowercase();
        // Ignore these headers from the original request.
        if name == "host" || name == "content-length" {
            continue;
        }
        builder = builder.header(name, value);
    }
    let mut body = vec![];
    message.into_read().read_to_end(&mut body)?;
    builder = builder.body(body);
    Ok(builder)
}

async fn forward_to_upstream_printer(
    message: IppRequestResponse,
    http_request: HttpRequest,
    upstream_printer: &str,
) -> Result<(IppRequestResponse, HeaderMap), Error> {
    // Send the request to the upstream printer.
    let request = build_upstream_http_request(message, http_request, upstream_printer)?;
    let response = request.send().await?;
    let headers = response.headers().clone();
    let body = response.bytes().await?;
    let parser = IppParser::new(Cursor::new(body));
    let response = parser.parse()?;

    Ok((response, headers))
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
