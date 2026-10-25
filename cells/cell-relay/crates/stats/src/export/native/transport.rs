//! HTTPS-only curl transport for background native owners. No credential argv,
//! redirects, payload logs or unbounded response buffers. Requires installed curl.
use super::{invalid, validate};
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
            || !(endpoint.ends_with("/profiles") || endpoint.ends_with("/v1/investigations"))
            || tenant.is_empty()
            || tenant.chars().count() > 256
            || !(32..=512).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.~".contains(&b))
            || timeout_ms == 0
            || timeout_ms
                > if endpoint.ends_with("/v1/investigations") {
                    5000
                } else {
                    30_000
                }
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
    pub fn send(&self, bytes: &[u8]) -> std::io::Result<bool> {
        self.send_program(bytes, std::path::Path::new("curl"))
    }
    fn send_program(&self, bytes: &[u8], program: &std::path::Path) -> std::io::Result<bool> {
        let tenant = if self.endpoint.ends_with("/v1/investigations") {
            agw_audit::investigation_wire::validate(bytes)
                .map_err(|_| invalid("invalid investigation"))?
                .tenant
                .as_str()
                .to_owned()
        } else {
            validate(bytes)?.scope.tenant.as_str().to_owned()
        };
        if tenant != self.tenant {
            return Err(invalid("sender tenant mismatch"));
        }
        let body = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
        // JSON string escaping is also valid for curl's quoted config fields:
        // the profile's JSON escapes are escaped once more, never interpreted.
        let config = format!(
            "url = {}\nheader = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\n",
            serde_json::to_string(&self.endpoint)?,
            serde_json::to_string(&format!("Authorization: Bearer {}", self.token))?,
            serde_json::to_string(body)?
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
        Ok(output.status.success() && output.stdout == b"202")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn investigation_sender_validates_scope_before_launch() -> Result<(), Box<dyn std::error::Error>>
    {
        let sender = HttpsSender::new(
            "https://example.test/v1/investigations",
            "tenant-a",
            "test-only-credential-with-32-bytes",
            5000,
        )?;
        let body =
            include_bytes!("../../../../../../../contracts/examples/investigation_job.valid.json");
        assert!(
            sender
                .send_program(body, std::path::Path::new("/missing-agindow-curl"))
                .is_err()
        );
        assert!(
            HttpsSender::new(
                "https://example.test/v1/investigations",
                "tenant-a",
                "test-only-credential-with-32-bytes",
                5001
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn sender_keeps_secrets_in_stdin_and_accepts_only_queue_ack()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("ti48-curl-{}", std::process::id()));
        fs::create_dir(&dir)?;
        let program = dir.join("curl-test");
        let config = dir.join("input");
        let args = dir.join("args");
        let token = "test-only-credential-with-32-bytes";
        let body =
            include_bytes!("../../../../../../../contracts/examples/field_profile.valid.json");
        let sender = HttpsSender::new("https://example.test/profiles", "tenant-a", token, 1000)?;
        for (code, accepted) in [("202", true), ("302", false), ("503", false)] {
            fs::write(
                &program,
                format!(
                    "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' \"$@\" > '{}'\nprintf '{code}'\n",
                    config.display(),
                    args.display()
                ),
            )?;
            fs::set_permissions(&program, fs::Permissions::from_mode(0o700))?;
            assert_eq!(sender.send_program(body, &program)?, accepted);
            assert!(!fs::read_to_string(&args)?.contains(token));
            let input = fs::read_to_string(&config)?;
            assert!(input.contains(&format!("Authorization: Bearer {token}")));
            assert!(input.contains(&serde_json::to_string(std::str::from_utf8(body)?)?));
        }
        let wrong = HttpsSender::new("https://example.test/profiles", "victim", token, 1000)?;
        fs::remove_file(&program)?;
        assert!(wrong.send_program(body, &program).is_err());
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
