use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use service_runtime::Secret;
use tonic::service::Interceptor;
use tonic::transport::{
    Certificate, Channel, ClientTlsConfig, Endpoint, Identity, ServerTlsConfig,
};
use tonic::{Request, Status};
use x509_parser::extensions::GeneralName;

use crate::caller::caller;

pub const CERTS_ENV: &str = "CERTS_DIR";

pub const DEFAULT_CERTS: &str = "certs";

#[derive(Clone, PartialEq, Eq)]
pub struct Tls {
    pub name: String,
    pub ca: Vec<u8>,
    pub cert: Vec<u8>,
    pub key: Secret<Vec<u8>>,
}

impl fmt::Debug for Tls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tls")
            .field("name", &self.name)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

fn read(dir: &Path, file: &str) -> io::Result<Vec<u8>> {
    let path = dir.join(file);
    fs::read(&path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "{}: {error}; сертификаты выпускает `cargo xtask certs`",
                path.display()
            ),
        )
    })
}

impl Tls {
    pub fn load(dir: &Path, name: &str) -> io::Result<Tls> {
        Ok(Tls {
            name: name.to_string(),
            ca: read(dir, "ca.pem")?,
            cert: read(dir, &format!("{name}.pem"))?,
            key: Secret::new(read(dir, &format!("{name}.key"))?),
        })
    }

    pub fn server(&self) -> ServerTlsConfig {
        ServerTlsConfig::new()
            .identity(Identity::from_pem(&self.cert, self.key.expose()))
            .client_ca_root(Certificate::from_pem(&self.ca))
    }

    pub fn client(&self, peer: &str) -> ClientTlsConfig {
        ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(&self.ca))
            .identity(Identity::from_pem(&self.cert, self.key.expose()))
            .domain_name(peer)
    }
}

pub fn channel(
    addr: &str,
    peer: &str,
    tls: Option<&Tls>,
) -> Result<Channel, tonic::transport::Error> {
    let channel = match tls {
        Some(tls) => Endpoint::from_shared(format!("https://{addr}"))?
            .tls_config(tls.client(peer))?
            .connect_lazy(),
        None => Endpoint::from_shared(format!("http://{addr}"))?.connect_lazy(),
    };
    Ok(channel)
}

pub fn peer_names<T>(request: &Request<T>) -> Vec<String> {
    let Some(certs) = request.peer_certs() else {
        return Vec::new();
    };
    let Some(leaf) = certs.first() else {
        return Vec::new();
    };
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(leaf.as_ref()) else {
        return Vec::new();
    };
    let Ok(Some(san)) = cert.subject_alternative_name() else {
        return Vec::new();
    };
    san.value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::DNSName(dns) => Some(dns.to_string()),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Gate {
    allowed: Option<Vec<String>>,
}

impl Gate {
    pub fn open() -> Gate {
        Gate::default()
    }

    pub fn only(allowed: &[&str]) -> Gate {
        Gate {
            allowed: Some(allowed.iter().map(|name| name.to_string()).collect()),
        }
    }
}

impl Interceptor for Gate {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(allowed) = &self.allowed {
            let names = peer_names(&request);
            if !names.iter().any(|name| allowed.contains(name)) {
                return Err(Status::permission_denied(format!(
                    "сюда пускают только {}, а сертификат клиента на {}",
                    allowed.join(", "),
                    if names.is_empty() {
                        String::from("никого")
                    } else {
                        names.join(", ")
                    }
                )));
            }
        }
        caller(request)
    }
}
