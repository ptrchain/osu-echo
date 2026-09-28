use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

const TLS_SUBDOMAINS: &[&str] = &[
    "localhost",
    "c",
    "c1",
    "c2",
    "c3",
    "c4",
    "c5",
    "c6",
    "ce",
    "cho",
    "osu",
    "osu!",
    "a",
    "b",
    "assets",
    "i",
    "s",
    "api",
];

/// Wildcard entry: covers every single-label subdomain, including ones a client may ask for that
/// are not listed above. A hosts file has no equivalent, so [`CLIENT_HOSTS`] is the list that
/// actually has to resolve.
const TLS_WILDCARD: &str = "*.localhost";

/// Every name the client may address, on the `localhost` devserver.
///
/// The bancho host has been spelled `cho`, `c`, `c1`…`c6` and `ce` over the client's lifetime and
/// which one it asks for depends on the build, so all of them are provided. Windows does not
/// resolve `*.localhost` on its own, so each of these needs a hosts entry — miss one and the
/// client simply never reaches the server ("connecting to server" forever, no request logged).
pub const CLIENT_HOSTS: &[&str] = &[
    "osu.localhost",
    "c.localhost",
    "c1.localhost",
    "c2.localhost",
    "c3.localhost",
    "c4.localhost",
    "c5.localhost",
    "c6.localhost",
    "ce.localhost",
    "cho.localhost",
    "a.localhost",
    "b.localhost",
    "assets.localhost",
    "i.localhost",
    "s.localhost",
    "api.localhost",
];

/// Bump when the certificate's contents change so existing installs regenerate it.
const CURRENT_CERT_VERSION: &str = "v3_wildcard_localhost";

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
    let cert_ver = tls_dir.join("cert_version.txt");

    let is_up_to_date = cert_ver.exists() && std::fs::read_to_string(&cert_ver).unwrap_or_default() == CURRENT_CERT_VERSION;

    if cert_pem.exists() && key_pem.exists() && cert_der.exists() && is_up_to_date {
        return Ok(TlsSetupPaths { cert_pem, key_pem, cert_der });
    }

    crate::logger::info("Generating self-signed TLS certificates for localhost (including b.localhost)...");

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Local osu localhost");
    params.distinguished_name = dn;

    let mut sans = Vec::new();
    for label in TLS_SUBDOMAINS {
        sans.push(SanType::DnsName(format!("{}.localhost", label).try_into()?));
    }
    sans.push(SanType::DnsName(TLS_WILDCARD.try_into()?));
    sans.push(SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))));
    params.subject_alt_names = sans;

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_pem_data = cert.pem();
    let cert_der_data = cert.der().to_vec();
    let key_pem_data = key_pair.serialize_pem();

    std::fs::write(&cert_pem, cert_pem_data)?;
    std::fs::write(&key_pem, key_pem_data)?;
    std::fs::write(&cert_der, &cert_der_data)?;
    let _ = std::fs::write(&cert_ver, CURRENT_CERT_VERSION);

    crate::logger::success(&format!("TLS certificates generated in {}", tls_dir.display()));
    let _ = install_windows_trust(&cert_der);

    Ok(TlsSetupPaths { cert_pem, key_pem, cert_der })
}

pub fn install_windows_trust(der_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        crate::logger::info("Registering certificate into Windows User Root Trust store...");
        // -f matters: without it certutil interactively asks for confirmation, so an unattended
        // launch blocks on a prompt and the certificate silently never gets trusted — the client
        // then fails the TLS handshake and just retries "connecting to server" forever.
        let output = std::process::Command::new("certutil")
            .args(["-user", "-f", "-addstore", "Root", &der_path.to_string_lossy()])
            .output()?;

        if output.status.success() {
            crate::logger::success("Local certificate trusted by Windows successfully.");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            crate::logger::warn(&format!(
                "Could not trust the local certificate automatically ({}). Trust {} manually, or run with --trust-cert.",
                format!("{}{}", stderr.trim(), stdout.trim()).trim(),
                der_path.display()
            ));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = der_path;
    }
    Ok(())
}

/// The block that has to be present in the Windows hosts file for `-devserver localhost`.
///
/// One name per line on purpose: Windows honours only the first nine names on a hosts line and
/// silently drops the rest, which looks exactly like a missing entry — the name is right there in
/// the file but does not resolve. Packing the list onto one line is therefore never correct.
pub fn hosts_file_block() -> String {
    let mut block = String::from("# osu-echo begin\r\n");
    for name in std::iter::once("localhost").chain(CLIENT_HOSTS.iter().copied()) {
        block.push_str(&format!("127.0.0.1 {}\r\n", name));
    }
    block.push_str("# osu-echo end\r\n");
    block
}

/// Whether every name the client may ask for already resolves to a loopback address.
///
/// This is the check that matters when nothing works: a name that does not resolve never reaches
/// the server, so the console stays silent while the client retries forever. It has to ask the
/// resolver rather than grep the hosts file, because a name can be present in the file and still
/// be ignored (see [`hosts_file_block`]).
pub fn unresolved_client_hosts() -> Vec<&'static str> {
    use std::net::ToSocketAddrs;

    CLIENT_HOSTS
        .iter()
        .copied()
        .filter(|host| match (*host, 443u16).to_socket_addrs() {
            Ok(mut addrs) => !addrs.any(|addr| addr.ip().is_loopback()),
            Err(_) => true,
        })
        .collect()
}

/// Prints a readiness report for `-devserver localhost`, naming exactly what is missing.
pub fn report_client_readiness() {
    let unresolved = unresolved_client_hosts();
    if unresolved.is_empty() {
        crate::logger::success("Client hostnames: all names required by -devserver localhost resolve to 127.0.0.1.");
    } else {
        crate::logger::error(&format!(
            "{} name(s) the osu! client needs do not resolve to 127.0.0.1: {}",
            unresolved.len(),
            unresolved.join(", ")
        ));
        crate::logger::error("Requests to those names never reach this server, which is what makes the client sit on \"connecting to server\" with an empty log.");
        crate::logger::error(&format!(
            "Fix: start the server once as Administrator so it can add these to the Windows hosts file, or paste them into %SystemRoot%\\System32\\drivers\\etc\\hosts yourself:\n{}",
            indent(&hosts_file_block())
        ));
    }
}

fn indent(block: &str) -> String {
    block.lines().map(|l| format!("  {}", l)).collect::<Vec<_>>().join("\n")
}

pub fn setup_windows_hosts() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        let hosts_path = std::path::Path::new(&system_root).join("System32\\drivers\\etc\\hosts");

        if !hosts_path.exists() {
            crate::logger::warn(&format!("Hosts file not found at {}", hosts_path.display()));
            return Ok(());
        }

        // Ask the resolver, not the file: a name can be listed in the hosts file and still be
        // ignored by Windows (more than nine names on one line), which is exactly the state that
        // leaves the client unable to connect.
        if unresolved_client_hosts().is_empty() {
            crate::logger::info("Windows hosts file already maps every name the client needs to 127.0.0.1.");
            return Ok(());
        }

        crate::logger::info("Configuring Windows hosts file for -devserver localhost subdomains...");
        let block = hosts_file_block();

        // Try direct write first (succeeds if already running with admin privileges). A previous
        // osu-echo line may be incomplete or malformed, so it is replaced rather than appended to.
        if rewrite_our_hosts_block(&hosts_path, &block) && unresolved_client_hosts().is_empty() {
            crate::logger::success("Added localhost subdomains to Windows hosts file.");
            let _ = std::process::Command::new("ipconfig").arg("/flushdns").output();
            return Ok(());
        }

        // Elevate: a script file avoids all the quoting problems of a multi-line hosts block.
        crate::logger::info("Requesting administrator privilege to update Windows hosts file...");
        if let Some(script_path) = write_hosts_elevated_script(&system_root, &block) {
            let _ = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','{}'",
                        script_path.to_string_lossy().replace('\'', "''")
                    ),
                ])
                .output();
            let _ = std::fs::remove_file(&script_path);
        }

        // Verify against the resolver: the file can contain the names and still not apply them.
        if unresolved_client_hosts().is_empty() {
            crate::logger::success("Added localhost subdomains to Windows hosts file successfully.");
            let _ = std::process::Command::new("ipconfig").arg("/flushdns").output();
        } else {
            crate::logger::warn("Could not automatically update Windows hosts file (elevation was cancelled or failed).");
            crate::logger::warn("Please add the following to C:\\Windows\\System32\\drivers\\etc\\hosts as Administrator (one name per line), then restart osu-echo:");
            crate::logger::warn(&indent(&block));
        }
    }

    Ok(())
}

/// Replaces this tool's block in the hosts file (or appends it), leaving every other line alone.
/// Fails when the file is not writable, which is the signal to ask for elevation.
#[cfg(target_os = "windows")]
fn rewrite_our_hosts_block(hosts_path: &Path, block: &str) -> bool {
    let existing = std::fs::read_to_string(hosts_path).unwrap_or_default();
    let kept: Vec<&str> = existing.lines().filter(|l| !l.contains("# osu-echo")).collect();

    let mut updated = kept.join("\r\n");
    if !updated.is_empty() {
        updated.push_str("\r\n");
    }
    updated.push_str(block);

    std::fs::write(hosts_path, updated).is_ok()
}

/// Writes a PowerShell script that performs the same edit with Administrator rights.
#[cfg(target_os = "windows")]
fn write_hosts_elevated_script(system_root: &str, block: &str) -> Option<std::path::PathBuf> {
    let script = format!(
        "$hosts = Join-Path '{}' 'System32\\drivers\\etc\\hosts'\n\
         $block = @'\n{}'@\n\
         $lines = @()\n\
         if (Test-Path $hosts) {{ $lines = Get-Content $hosts | Where-Object {{ $_ -notmatch '# osu-echo' }} }}\n\
         Set-Content -Path $hosts -Value (($lines + ($block -split \"`r?`n\" | Where-Object {{ $_ -ne '' }})) -join \"`r`n\") -Encoding ASCII\n",
        system_root.replace('\'', "''"),
        block
    );

    let path = std::env::temp_dir().join("osu-echo-setup-hosts.ps1");
    std::fs::write(&path, script).ok()?;
    Some(path)
}

/// Turns a TLS handshake failure into an actionable explanation.
///
/// The overwhelmingly common cause is a client too old to negotiate TLS 1.2: osu! builds from
/// 2022 and earlier only offer TLS 1.0/1.1, and rustls speaks TLS 1.2+ exclusively. Such a client
/// sends a hello without the `signature_algorithms` extension, which rustls refuses outright, so
/// the game just reports "connection failed" while the server looks healthy.
pub fn explain_handshake_failure(err: &str) -> Option<&'static str> {
    let lower = err.to_ascii_lowercase();
    if lower.contains("signaturealgorithms") || lower.contains("incompatible") || lower.contains("protocol version") || lower.contains("handshake") {
        Some(
            "This client cannot negotiate TLS 1.2 (osu! builds from 2022 and earlier only offer TLS 1.0/1.1), and this server is running in the default TLS 1.2+ mode. Restart the server with --legacy-tls to accept it, or update the osu! client.",
        )
    } else {
        None
    }
}

/// Opt-in TLS stack for clients that cannot negotiate TLS 1.2 (osu! builds from 2022 and earlier
/// only offer TLS 1.0/1.1).
///
/// Windows' own TLS stack cannot help here: Microsoft disables TLS 1.0/1.1 through a *machine-wide*
/// registry policy, and switching it back on to talk to one local server would weaken every other
/// program on the PC. The vendored OpenSSL used instead does not consult that policy, so this
/// listener lives entirely inside the server process — the OS setting stays untouched, nothing
/// off-machine can reach it (it binds loopback only), and dropping the flag restores a TLS 1.2+
/// listener.
#[cfg(all(target_os = "windows", feature = "legacy-tls"))]
pub struct LegacyTlsAcceptor {
    ctx: Arc<openssl::ssl::SslContext>,
}

#[cfg(all(target_os = "windows", feature = "legacy-tls"))]
impl LegacyTlsAcceptor {
    pub fn new(cert_path: &Path, key_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        use openssl::ssl::{SslContext, SslMethod, SslOptions, SslVersion};

        let mut builder = SslContext::builder(SslMethod::tls())?;
        builder.set_min_proto_version(Some(SslVersion::TLS1))?;
        // TLS 1.0/1.1 only offer CBC cipher suites with SHA-1 MACs, which OpenSSL 3 rejects at its
        // default security level. The listener is loopback-only, so relaxing the level here only
        // ever applies to a connection coming from this machine.
        builder.set_cipher_list("ALL:@SECLEVEL=0")?;
        builder.set_options(SslOptions::NO_TICKET);

        let ctx = builder.build();
        ctx.set_certificate_file(cert_path)?;
        ctx.set_private_key_file(key_path)?;
        ctx.check_private_key()?;

        Ok(Self { ctx: Arc::new(ctx) })
    }

    /// A fresh `Ssl` for one connection; the handshake itself is driven by `tokio-openssl`.
    pub fn ssl(&self) -> openssl::ssl::Ssl {
        openssl::ssl::Ssl::new(self.ctx.clone()).expect("Ssl::new with a validated context")
    }
}

/// The TLS front-end used by `--legacy-tls` when this build has no in-process legacy stack.
///
/// Embedded rather than shipped as a file so the server stays a single executable. It is a
/// byte-for-byte pipe: TLS is terminated in Python (whose OpenSSL ignores the machine-wide
/// SCHANNEL policy that blocks TLS 1.0/1.1) and the plaintext is forwarded to the server's own
/// HTTP listener, so the client still decides routing through its `Host` header.
const LEGACY_TLS_PROXY: &str = include_str!("legacy_tls_proxy.py");

/// Python interpreters to try, in order.
const PYTHON_CANDIDATES: &[&str] = &["python", "python3", "py"];

/// Locates a usable Python interpreter, preferring the newest one available.
pub fn find_python() -> Option<String> {
    for candidate in PYTHON_CANDIDATES {
        let mut args: Vec<&str> = vec!["--version"];
        if *candidate == "py" {
            args = vec!["-3", "--version"];
        }

        let ok = std::process::Command::new(candidate)
            .args(&args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if ok {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Starts the legacy TLS front-end on `https_port`, forwarding to the plain HTTP listener.
///
/// Returns the child so the caller can report its output; it is not supervised, so if Python is
/// missing or dies the log says so and the HTTPS port simply stays closed.
pub fn spawn_legacy_tls_proxy(https_port: u16, upstream_port: u16, cert_pem: &Path, key_pem: &Path) -> Result<std::process::Child, String> {
    let python = find_python().ok_or_else(|| {
        "no Python interpreter found (tried python, python3, py). Install Python 3, or rebuild with `--features legacy-tls` for a self-contained listener.".to_string()
    })?;

    let mut cmd = std::process::Command::new(&python);
    if python == "py" {
        cmd.arg("-3");
    }
    cmd.arg("-c")
        .arg(LEGACY_TLS_PROXY)
        .arg("--listen-port")
        .arg(https_port.to_string())
        .arg("--upstream-port")
        .arg(upstream_port.to_string())
        .arg("--cert")
        .arg(cert_pem)
        .arg("--key")
        .arg(key_pem)
        .stdin(std::process::Stdio::null());

    cmd.spawn().map_err(|e| format!("could not start {}: {}", python, e))
}

/// Present so callers can compile unconditionally; only the `legacy-tls` build can actually serve
/// TLS 1.0/1.1 in-process.
#[cfg(not(all(target_os = "windows", feature = "legacy-tls")))]
pub struct LegacyTlsAcceptor;

#[cfg(not(all(target_os = "windows", feature = "legacy-tls")))]
impl LegacyTlsAcceptor {
    pub fn new(_cert: &Path, _key: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Err("built without the legacy-tls feature (rebuild with `--features legacy-tls`)".into())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_certificate_covers_every_client_host() {
        // A name the client may ask for but that is not in the certificate is a silent TLS
        // failure, so the two lists are kept in step. The wildcard covers the rest.
        for host in CLIENT_HOSTS {
            let label = host.split('.').next().unwrap();
            assert!(
                TLS_SUBDOMAINS.contains(&label),
                "{} has no certificate entry (label `{}`)",
                host,
                label
            );
        }
        assert!(TLS_SUBDOMAINS.contains(&"cho"), "older clients ask for cho.localhost");
        assert!(TLS_SUBDOMAINS.contains(&"c1"), "c1.localhost is used by some builds");
        assert!(TLS_SUBDOMAINS.contains(&"c4"), "c4.localhost is used by current builds");
        assert_eq!(TLS_WILDCARD, "*.localhost");
    }

    #[test]
    fn test_hosts_file_block_puts_one_name_per_line() {
        let block = hosts_file_block();
        let mapping_lines: Vec<&str> = block.lines().filter(|l| l.starts_with("127.0.0.1")).collect();

        // Windows silently ignores every name after the ninth on a line, so each name needs its
        // own line or it will not resolve at all.
        for line in &mapping_lines {
            assert_eq!(line.split_whitespace().count(), 2, "not one name per line: {}", line);
        }
        assert_eq!(mapping_lines.len(), CLIENT_HOSTS.len() + 1, "localhost plus every client host");
        assert!(mapping_lines[0].ends_with(" localhost"), "got: {}", mapping_lines[0]);
        for host in CLIENT_HOSTS {
            assert!(block.contains(&format!("127.0.0.1 {}\r\n", host)), "block is missing {}", host);
        }
        assert!(block.contains("# osu-echo begin") && block.contains("# osu-echo end"));
    }

    #[test]
    fn test_legacy_tls_proxy_script_is_embedded_and_loopback_only() {
        // The front-end is what makes --legacy-tls work without touching Windows' TLS policy, so
        // it must actually allow the old protocol versions and must not be reachable off-machine.
        assert!(LEGACY_TLS_PROXY.contains("ssl.TLSVersion.TLSv1"), "TLS 1.0 must be allowed");
        assert!(LEGACY_TLS_PROXY.contains("ALL:@SECLEVEL=0"), "TLS 1.0 ciphers need security level 0");
        assert!(LEGACY_TLS_PROXY.contains("load_cert_chain"), "it has to present the local certificate");
        assert!(LEGACY_TLS_PROXY.contains("127.0.0.1"), "must default to loopback only");
    }

    #[test]
    fn test_handshake_failure_explains_old_clients() {
        // What rustls reports for a pre-TLS-1.2 hello (osu! 2022 and earlier).
        let old = "peer is incompatible: SignatureAlgorithmsExtensionRequired";
        let hint = explain_handshake_failure(old).unwrap();
        assert!(hint.contains("--legacy-tls"), "the hint must say how to accept old clients: {}", hint);
        assert!(hint.contains("TLS 1.2"), "the hint must name the problem: {}", hint);

        assert!(explain_handshake_failure("peer is incompatible: NoCompatibleCipherSuite").is_some());
        assert!(explain_handshake_failure("peer sent a fatal alert: BadCertificate").is_none());
    }

    #[test]
    fn test_rewrite_our_hosts_block_replaces_previous_entry() {
        let dir = std::env::temp_dir().join("osu-echo-hosts-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hosts");

        // A previous version wrote one long line, which Windows truncates after nine names.
        std::fs::write(&path, "127.0.0.1 localhost\r\n0.0.0.0 ads.example\r\n127.0.0.1 localhost c.localhost cho.localhost # osu-echo\r\n").unwrap();
        assert!(rewrite_our_hosts_block(&path, &hosts_file_block()));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("0.0.0.0 ads.example"), "unrelated lines must survive: {}", content);
        assert_eq!(content.matches("# osu-echo begin").count(), 1, "our block must not be duplicated: {}", content);
        for line in content.lines().filter(|l| l.starts_with("127.0.0.1")) {
            assert_eq!(line.split_whitespace().count(), 2, "not one name per line: {}", line);
        }
        for host in CLIENT_HOSTS {
            assert!(content.contains(&format!("127.0.0.1 {}", host)), "missing {}", host);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
