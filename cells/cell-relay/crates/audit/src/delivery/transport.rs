//! HTTPS-only curl transport for background native owners. No credential argv,
//! redirects, payload logs or unbounded response buffers. Requires installed curl.
use super::{invalid, validate};
use crate::{
    contracts::audit_artifact::AuditArtifact,
    sink::{Sink, SinkError},
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub struct HttpsSender {
    endpoint: String,
    tenant: String,
    token: String,
    timeout_ms: u32,
}
impl HttpsSender {
    pub fn new(
        endpoint: &str,
        tenant: &str,
        token: &str,
        timeout_ms: u32,
    ) -> std::io::Result<Self> {
        if !endpoint.starts_with("https://")
            || endpoint.len() > 4096
            || endpoint
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || endpoint.contains(['"', '\\', '#', '?', '@'])
            || !endpoint.ends_with("/v1/audit")
            || !crate::valid_identifier(tenant)
            || !(32..=512).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.~".contains(&b))
            || timeout_ms == 0
            || timeout_ms > 30_000
        {
            return Err(invalid("invalid native HTTPS configuration"));
        }
        Ok(Self {
            endpoint: endpoint.into(),
            tenant: tenant.into(),
            token: token.into(),
            timeout_ms,
        })
    }
    fn send_program(
        &self,
        artifact: &AuditArtifact,
        program: &std::path::Path,
    ) -> std::io::Result<u16> {
        validate(artifact, &self.tenant)?;
        let body = serde_json::to_string(artifact)?;
        // JSON string escaping is also valid for curl's quoted config fields:
        // the profile's JSON escapes are escaped once more, never interpreted.
        let config = format!(
            "url = {}\nheader = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\n",
            serde_json::to_string(&self.endpoint)?,
            serde_json::to_string(&format!("Authorization: Bearer {}", self.token))?,
            serde_json::to_string(&body)?
        );
        let config = format!(
            "{config}header = {}\n",
            serde_json::to_string(&format!("X-Agindow-Tenant: {}", self.tenant))?
        );
        let mut child = Command::new(program)
            .args([
                "--disable",
                "--globoff",
                "--silent",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-redirs",
                "0",
                "--connect-timeout",
                "5",
                "--max-time",
                &format!("{:.3}", f64::from(self.timeout_ms) / 1000.0),
                "--output",
                "/dev/null",
                "--write-out",
                "%{http_code}",
                "--config",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let written = match child.stdin.take() {
            Some(mut input) => input.write_all(config.as_bytes()),
            None => Err(invalid("missing curl stdin")),
        };
        if let Err(e) = written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(invalid("audit HTTPS transport failed"));
        }
        let status = std::str::from_utf8(&output.stdout).map_err(std::io::Error::other)?;
        status.parse::<u16>().map_err(std::io::Error::other)
    }
}

impl Sink for HttpsSender {
    fn tenant(&self) -> Option<&str> {
        Some(&self.tenant)
    }
    fn emit(&mut self, artifact: &AuditArtifact) -> Result<(), SinkError> {
        acknowledge(self.send_program(artifact, std::path::Path::new("curl")))
    }
}
fn acknowledge(result: std::io::Result<u16>) -> Result<(), SinkError> {
    match result {
        Ok(200 | 201) => Ok(()),
        Ok(409) => Err(SinkError::conflict()),
        _ => Err(SinkError::new("audit HTTPS delivery not acknowledged")),
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn credentials_stay_in_stdin_and_only_durable_statuses_acknowledge()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("ti57-curl-{}", std::process::id()));
        fs::create_dir(&dir)?;
        let program = dir.join("curl-test");
        let config = dir.join("input");
        let args = dir.join("args");
        let token = "test-only-credential-with-32-bytes";
        let artifact: AuditArtifact = serde_json::from_str(include_str!(
            "../../../../../../contracts/examples/audit_artifact.valid.json"
        ))?;
        let sender = HttpsSender::new("https://example.test/v1/audit", "acme", token, 1000)?;
        for code in [200, 201, 202, 302, 409, 503] {
            fs::write(
                &program,
                format!(
                    "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' \"$@\" > '{}'\nprintf '{code}'\n",
                    config.display(),
                    args.display()
                ),
            )?;
            fs::set_permissions(&program, fs::Permissions::from_mode(0o700))?;
            let result = sender.send_program(&artifact, &program)?;
            assert_eq!(result, code);
            assert_eq!(acknowledge(Ok(result)).is_ok(), matches!(code, 200 | 201));
            assert_eq!(
                acknowledge(Ok(result))
                    .err()
                    .is_some_and(|e| e.is_conflict()),
                code == 409
            );
            let argv = fs::read_to_string(&args)?;
            assert!(!argv.contains(token));
            assert!(argv.contains("--disable"));
            assert!(argv.contains("--max-redirs\n0"));
            let input = fs::read_to_string(&config)?;
            assert!(input.contains(&format!("Authorization: Bearer {token}")));
            assert!(input.contains("X-Agindow-Tenant: acme"));
            assert!(input.contains(&serde_json::to_string(&serde_json::to_string(&artifact)?)?));
        }
        let wrong = HttpsSender::new("https://example.test/v1/audit", "victim", token, 1000)?;
        fs::remove_file(&program)?;
        assert!(wrong.send_program(&artifact, &program).is_err());
        for endpoint in [
            "http://example.test/v1/audit",
            "https://user@example.test/v1/audit",
            "https://example.test/v1/audit?secret",
        ] {
            assert!(HttpsSender::new(endpoint, "acme", token, 1000).is_err());
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
