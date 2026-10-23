// Личность клиента берётся из его сертификата, а права проверяются по таблице. Допиши все три части.
use rustls::ServerConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub fn identity_of(cert_der: &[u8]) -> Option<String> {
    use x509_parser::extensions::GeneralName;
    let (_, cert) = x509_parser::parse_x509_certificate(cert_der).ok()?;
    let san = cert.subject_alternative_name().ok().flatten()?;
    san.value.general_names.iter().find_map(|n| match n {
        GeneralName::DNSName(name) => Some(name.to_string()),
        _ => None,
    })
}

pub struct Rules {
    by_method: HashMap<String, Vec<String>>,
}

impl Rules {
    pub fn new(table: &[(&str, &[&str])]) -> Rules {
        Rules { by_method: table.iter().map(|(m, who)| (m.to_string(), who.iter().map(|w| w.to_string()).collect())).collect() }
    }

    pub fn allows(&self, identity: Option<&str>, method: &str) -> bool {
        match (identity, self.by_method.get(method)) {
            (Some(who), Some(allowed)) => allowed.iter().any(|a| a == who),
            _ => false,
        }
    }
}

pub async fn serve(listener: TcpListener, config: Arc<ServerConfig>, rules: Arc<Rules>) {
    let acceptor = TlsAcceptor::from(config);
    loop {
        let Ok((tcp, _)) = listener.accept().await else { continue };
        let acceptor = acceptor.clone();
        let rules = rules.clone();
        tokio::spawn(async move {
            let Ok(tls) = acceptor.accept(tcp).await else { return };
            let identity = tls.get_ref().1.peer_certificates().and_then(|chain| chain.first()).and_then(|cert| identity_of(cert.as_ref()));
            let mut reader = BufReader::new(tls);
            let mut line = String::new();
            if reader.read_line(&mut line).await.is_err() {
                return;
            }
            let method = line.split_whitespace().nth(1).unwrap_or("");
            let shown = identity.as_deref().unwrap_or("-");
            let verdict = if line.starts_with("CALL ") && rules.allows(identity.as_deref(), method) { "OK" } else { "DENIED" };
            let _ = reader.get_mut().write_all(format!("{verdict} {shown}\n").as_bytes()).await;
            let _ = reader.get_mut().shutdown().await;
        });
    }
}
