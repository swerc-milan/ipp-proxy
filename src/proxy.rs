use std::io::Cursor;
use std::io::Read;

use crate::db::{Database, Team};
use actix_web::{web, HttpRequest, HttpResponse};
use anyhow::{anyhow, Error};
use ipp::parser::IppParser;
use ipp::prelude::*;
use ipp::reader::IppReader;
use reqwest::header::HeaderMap;
use reqwest::RequestBuilder;

#[derive(Debug, Clone)]
pub struct ProxyOptions {}

pub async fn process(
    db: Database<'_>,
    team: Team,
    req: HttpRequest,
    body: web::Bytes,
    options: &ProxyOptions,
) -> Result<HttpResponse, Error> {
    let reader = body.as_ref().to_vec();
    let reader = IppReader::new(Cursor::new(reader));
    let mut parsed = IppParser::new(reader).parse()?;

    let operation = Operation::try_from(parsed.header().operation_or_status)
        .map_err(|_| anyhow!("Invalid op"))?;
    debug!(
        "{:?} received from team {} ({})",
        operation, team.team_id, team.team_name
    );

    let ipp_upstream = &team.ipp_upstream;
    patch_ipp_printer_uri(&mut parsed, ipp_upstream);
    let (ipp_response, headers) = forward_to_upstream_printer(parsed, req, ipp_upstream).await?;

    let mut http_response = HttpResponse::Ok();
    for (name, value) in headers {
        if let Some(name) = name {
            http_response.insert_header((name, value));
        }
    }
    Ok(http_response.body(ipp_response.to_bytes()))
}

/// Patch the printer-uri attribute setting the correct upstream URI.
fn patch_ipp_printer_uri(request: &mut IppRequestResponse, upstream_printer: &str) {
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
    // TODO: patch the response setting the printer-uri of the proxy (http_request.uri())
    let parser = IppParser::new(Cursor::new(body));
    let response = parser.parse()?;

    Ok((response, headers))
}
