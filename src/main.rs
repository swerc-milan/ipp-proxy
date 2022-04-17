#[macro_use]
extern crate log;

use actix_web::middleware::Logger;
use actix_web::{route, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use anyhow::{anyhow, Error};
use ipp::parser::IppParser;
use ipp::prelude::*;
use ipp::reader::IppReader;
use reqwest::header::HeaderMap;
use reqwest::RequestBuilder;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Read;

#[route("/", method = "POST")]
async fn index(
    query: web::Query<HashMap<String, String>>,
    body: web::Bytes,
    req: HttpRequest,
) -> impl Responder {
    process(query, body, req)
        .await
        .unwrap_or_else(|e| HttpResponse::InternalServerError().body(e.to_string()))
}

async fn process(
    query: web::Query<HashMap<String, String>>,
    body: web::Bytes,
    req: HttpRequest,
) -> Result<HttpResponse, Error> {
    let reader = body.as_ref().to_vec();
    let reader = IppReader::new(Cursor::new(reader));
    let mut parsed = IppParser::new(reader).parse()?;

    let operation = Operation::try_from(parsed.header().operation_or_status)
        .map_err(|_| anyhow!("Invalid op"))?;
    debug!("{:?} received from {:?}", operation, req.peer_addr());

    let upstream = "localhost:631/printers/Virtual_PDF_Printer";
    patch_ipp_message(&mut parsed, upstream);
    let (ipp_response, headers) = forward_to_upstream_printer(parsed, req, upstream).await;

    let mut http_response = HttpResponse::Ok();
    for (name, value) in headers {
        http_response.insert_header((name.unwrap(), value));
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
) -> RequestBuilder {
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
    message.into_read().read_to_end(&mut body).unwrap();
    builder = builder.body(body);
    builder
}

async fn forward_to_upstream_printer(
    message: IppRequestResponse,
    http_request: HttpRequest,
    upstream_printer: &str,
) -> (IppRequestResponse, HeaderMap) {
    // Send the request to the upstream printer.
    let request = build_upstream_http_request(message, http_request, upstream_printer);
    let response = request.send().await.unwrap();
    let headers = response.headers().clone();
    let body = response.bytes().await.unwrap();
    let parser = IppParser::new(Cursor::new(body));
    let response = parser.parse().unwrap();

    (response, headers)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    HttpServer::new(|| App::new().wrap(Logger::default()).service(index))
        .bind("0.0.0.0:6632")?
        .run()
        .await
}
