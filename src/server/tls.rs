use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

const TLS_SUBDOMAINS: &[&str] =
    &["localhost", "c.localhost", "c4.localhost", "c5.localhost", "c6.localhost", "ce.localhost", "osu.localhost", "a.localhost", "assets.localhost"];

pub struct TlsSetupPaths {
    pub cert_pem: PathBuf,
    pub key_pem: PathBuf,
    pub cert_der: PathBuf,
}

pub fn get_or_create_certificates(data_dir: &Path) -> Result<TlsSetupPaths, Box<dyn std::error::Error>> {
    let tls_dir = data_dir.join("local-tls");
    std::fs::create_dir_all(&tls_dir)?;

    let cert_pem = tls_dir.join("localhost.pem");
    let key_pem = tls_dir.join("localhost-key.pem");
    let cert_der = tls_dir.join("localhost.cer");

    if cert_pem.exists() && key_pem.exists() && cert_der.exists() {
        return Ok(TlsSetupPaths { cert_pem, key_pem, cert_der });
    }

    // Check if localhostosu astra already has them on this machine
    let astra_dir = PathBuf::from(r"E:\! Python\localhostosu astra\.data\local-tls");
    if astra_dir.exists() {
        let astra_cert = astra_dir.join("localhost.pem");
        let astra_key = astra_dir.join("localhost-key.pem");
        let astra_der = astra_dir.join("localhost.cer");
        if astra_cert.exists() && astra_key.exists() && astra_der.exists() {
            std::fs::copy(&astra_cert, &cert_pem)?;
            std::fs::copy(&astra_key, &key_pem)?;
            std::fs::copy(&astra_der, &cert_der)?;
            crate::logger::info("Reused existing trusted TLS certificates from localhostosu astra.");
            return Ok(TlsSetupPaths { cert_pem, key_pem, cert_der });
        }
    }

    crate::logger::info("Generating self-signed TLS certificates for localhost...");

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Local osu localhost");
    params.distinguished_name = dn;

    let mut sans = Vec::new();
    for sub in TLS_SUBDOMAINS {
        sans.push(SanType::DnsName((*sub).try_into()?));
    }
    sans.push(SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))));
    params.subject_alt_names = sans;

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_pem_data = cert.pem();
    let cert_der_data = cert.der().to_vec();
    let key_pem_data = key_pair.serialize_pem();

    std::fs::write(&cert_pem, cert_pem_data)?;
    std::fs::write(&key_pem, key_pem_data)?;
    std::fs::write(&cert_der, cert_der_data)?;

    crate::logger::success(&format!("TLS certificates generated in {}", tls_dir.display()));

    Ok(TlsSetupPaths { cert_pem, key_pem, cert_der })
}

pub fn install_windows_trust(der_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        crate::logger::info("Registering certificate into Windows User Root Trust store...");
        let output = std::process::Command::new("certutil").args(["-user", "-addstore", "Root", &der_path.to_string_lossy()]).output()?;

        if output.status.success() {
            crate::logger::success("Local certificate trusted by Windows successfully.");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            crate::logger::warn(&format!("certutil returned: {}", stderr.trim()));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = der_path;
    }
    Ok(())
}

pub fn create_tls_acceptor(cert_path: &Path, key_path: &Path) -> Result<TlsAcceptor, Box<dyn std::error::Error>> {
    let cert_file = std::fs::File::open(cert_path)?;
    let mut cert_reader = std::io::BufReader::new(cert_file);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

    let key_file = std::fs::File::open(key_path)?;
    let mut key_reader = std::io::BufReader::new(key_file);
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)?.ok_or("No private key found in key file")?;

    let config = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}
