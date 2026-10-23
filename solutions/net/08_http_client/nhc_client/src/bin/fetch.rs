use std::io::Write;
use std::process::ExitCode;

use http_client::{Client, Config, Request, parse_url};

fn usage() -> ExitCode {
    eprintln!("usage: fetch [-v] [-X METHOD] [-d BODY] [-H 'Name: value']... URL");
    ExitCode::from(64)
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut verbose = false;
    let mut method: Option<String> = None;
    let mut body: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut url: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-v" => verbose = true,
            "-X" => match args.next() {
                Some(m) => method = Some(m),
                None => return usage(),
            },
            "-d" => match args.next() {
                Some(b) => body = Some(b),
                None => return usage(),
            },
            "-H" => match args.next().as_deref().and_then(|h| h.split_once(':')) {
                Some((n, v)) => headers.push((n.trim().to_string(), v.trim().to_string())),
                None => return usage(),
            },
            _ if url.is_none() => url = Some(arg),
            _ => return usage(),
        }
    }
    let Some(url) = url else { return usage() };
    let method = method.unwrap_or_else(|| if body.is_some() { "POST" } else { "GET" }.to_string());
    let mut request = Request::new(&method, &url);
    request.headers = headers.clone();
    request.body = body.map(String::into_bytes);
    if verbose {
        if let Ok(parsed) = parse_url(&url) {
            eprintln!("> {method} {}", parsed.path);
            eprintln!("> Host: {}", parsed.authority());
        }
        for (n, v) in &headers {
            eprintln!("> {n}: {v}");
        }
    }
    match Client::new(Config::default()).send(request).await {
        Ok(response) => {
            if verbose {
                eprintln!("< {}", response.status);
                for (n, v) in &response.headers {
                    eprintln!("< {n}: {v}");
                }
            }
            let mut out = std::io::stdout();
            let _ = out.write_all(&response.body);
            let _ = out.flush();
            if response.status >= 400 { ExitCode::from(1) } else { ExitCode::SUCCESS }
        }
        Err(error) => {
            eprintln!("fetch: {error:?}");
            ExitCode::from(2)
        }
    }
}
