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

pub fn setup_windows_hosts() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        use std::io::Write;

        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        let hosts_path = std::path::Path::new(&system_root).join("System32\\drivers\\etc\\hosts");

        if !hosts_path.exists() {
            crate::logger::warn(&format!("Hosts file not found at {}", hosts_path.display()));
            return Ok(());
        }

        let content = std::fs::read_to_string(&hosts_path).unwrap_or_default();
        let required = ["osu.localhost", "c.localhost", "a.localhost", "assets.localhost"];
        let all_present = required.iter().all(|domain| content.contains(domain));

        if all_present {
            crate::logger::info("Windows hosts file already contains localhost subdomains.");
            return Ok(());
        }

        crate::logger::info("Configuring Windows hosts file for -devserver localhost subdomains...");
        let hosts_line = "\r\n127.0.0.1 osu.localhost c.localhost c4.localhost c5.localhost c6.localhost ce.localhost a.localhost assets.localhost # osu-echo\r\n";

        // Try direct append first (succeeds if already running with admin privileges)
        let direct_success = std::fs::OpenOptions::new()
            .append(true)
            .open(&hosts_path)
            .and_then(|mut f| f.write_all(hosts_line.as_bytes()))
            .is_ok();

        if direct_success {
            crate::logger::success("Added localhost subdomains to Windows hosts file.");
            let _ = std::process::Command::new("ipconfig").arg("/flushdns").output();
            return Ok(());
        }

        // Elevate via PowerShell RunAs if permission denied
        crate::logger::info("Requesting administrator privilege to update Windows hosts file...");
        let ps_script = format!(
            "Add-Content -Path '{}\\System32\\drivers\\etc\\hosts' -Value '`r`n127.0.0.1 osu.localhost c.localhost c4.localhost c5.localhost c6.localhost ce.localhost a.localhost assets.localhost # osu-echo'",
            system_root.replace('\'', "''")
        );

        let _ = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile', '-Command', \"{}\"", ps_script.replace('"', "\\\"")),
            ])
            .output();

        // Verify if it succeeded
        let updated_content = std::fs::read_to_string(&hosts_path).unwrap_or_default();
        if required.iter().all(|domain| updated_content.contains(domain)) {
            crate::logger::success("Added localhost subdomains to Windows hosts file successfully.");
            let _ = std::process::Command::new("ipconfig").arg("/flushdns").output();
        } else {
            crate::logger::warn("Could not automatically update Windows hosts file (elevation was cancelled or failed).");
            crate::logger::warn("Please manually add the following line to C:\\Windows\\System32\\drivers\\etc\\hosts as Administrator:");
            crate::logger::warn("  127.0.0.1 osu.localhost c.localhost c4.localhost c5.localhost c6.localhost ce.localhost a.localhost assets.localhost");
        }
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
