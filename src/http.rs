use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use crate::{config::Config, error::Result};

pub struct HttpClient {
    client: Client,
}

pub struct RequestOptions<'a> {
    pub url: &'a str,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String,
}

impl HttpResponse {
    pub fn pretty_json(&self) -> Option<String> {
        serde_json::from_str::<Value>(&self.body)
            .ok()
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| self.body.clone()))
    }
}

impl HttpClient {
    pub fn new(config: &Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(concat!("sandbox-rs/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub fn get(&self, opts: RequestOptions, config: &Config) -> Result<HttpResponse> {
        let resp = self
            .client
            .get(opts.url)
            .headers(build_headers(&opts.headers, config)?)
            .query(&opts.query)
            .send()?;
        read_response(resp)
    }

    pub fn post_json(&self, opts: RequestOptions, body: Value, config: &Config) -> Result<HttpResponse> {
        let resp = self
            .client
            .post(opts.url)
            .headers(build_headers(&opts.headers, config)?)
            .query(&opts.query)
            .json(&body)
            .send()?;
        read_response(resp)
    }

    pub fn post_form(
        &self,
        opts: RequestOptions,
        form: Vec<(String, String)>,
        config: &Config,
    ) -> Result<HttpResponse> {
        let resp = self
            .client
            .post(opts.url)
            .headers(build_headers(&opts.headers, config)?)
            .query(&opts.query)
            .form(&form)
            .send()?;
        read_response(resp)
    }
}

fn build_headers(
    extra: &[(String, String)],
    config: &Config,
) -> Result<HeaderMap> {
    let mut map = HeaderMap::new();
    for (k, v) in config.default_headers.iter().chain(extra.iter()) {
        let name = HeaderName::from_str(k)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let value = HeaderValue::from_str(v)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        map.insert(name, value);
    }
    Ok(map)
}

fn read_response(resp: Response) -> Result<HttpResponse> {
    let status = resp.status().as_u16();
    let url = resp.url().to_string();
    let headers: HashMap<String, String> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = resp.text()?;
    Ok(HttpResponse { status, headers, body, url })
}
