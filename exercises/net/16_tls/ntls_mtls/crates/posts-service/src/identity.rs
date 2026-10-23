use x509_parser::extensions::GeneralName;

pub fn dns_names(cert_der: &[u8]) -> Vec<String> {
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(cert_der) else { return Vec::new() };
    let Ok(Some(san)) = cert.subject_alternative_name() else { return Vec::new() };
    san.value
        .general_names
        .iter()
        .filter_map(|n| match n {
            GeneralName::DNSName(name) => Some(name.to_string()),
            _ => None,
        })
        .collect()
}
