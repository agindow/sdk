//! Background HTTPS/curl transport with bounded stdout and private stdin config.
use super::{Error, Job};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};
pub struct HttpsReader {
    endpoint: String,
    tenant: String,
    token: String,
}
impl HttpsReader {
    pub fn new(endpoint: &str, tenant: &str, token: &str) -> Result<Self, Error> {
        if !endpoint.starts_with("https://")
            || endpoint.len() > 4096
            || endpoint
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || endpoint.contains(['"', '\\', '#', '?', '@'])
            || !endpoint.ends_with("/profile-snapshots")
            || tenant.is_empty()
            || tenant.chars().count() > 256
            || !(32..=512).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.~".contains(&b))
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            endpoint: endpoint.into(),
            tenant: tenant.into(),
            token: token.into(),
        })
    }
    pub fn fetch(&self, job: &Job) -> Result<Vec<u8>, Error> {
        self.fetch_program(job, std::path::Path::new("curl"))
    }
    fn fetch_program(&self, job: &Job, program: &std::path::Path) -> Result<Vec<u8>, Error> {
        if job.selected.scope.tenant != self.tenant
            || job.timeout_ms == 0
            || job.timeout_ms > 30_000
        {
            return Err(Error::Invalid);
        }
        job.selected.validate()?;
        let body = job.body()?;
        let config = format!(
            "url = {}\nheader = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\n",
            serde_json::to_string(&self.endpoint).map_err(|_| Error::Invalid)?,
            serde_json::to_string(&format!("Authorization: Bearer {}", self.token))
                .map_err(|_| Error::Invalid)?,
            serde_json::to_string(std::str::from_utf8(&body).map_err(|_| Error::Invalid)?)
                .map_err(|_| Error::Invalid)?
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
                &format!("{:.3}", job.timeout_ms as f64 / 1000.0),
                "--max-filesize",
                "131072",
                "--write-out",
                "\n%{http_code}",
                "--config",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error::Transport)?;
        let read = (|| {
            child
                .stdin
                .take()
                .ok_or(Error::Transport)?
                .write_all(config.as_bytes())
                .map_err(|_| Error::Transport)?;
            let mut output = Vec::new();
            child
                .stdout
                .take()
                .ok_or(Error::Transport)?
                .take(131077)
                .read_to_end(&mut output)
                .map_err(|_| Error::Transport)?;
            if output.len() > 131076 {
                return Err(Error::Capacity);
            }
            Ok(output)
        })();
        if read.is_err() {
            let _ = child.kill();
        }
        let status = child.wait().map_err(|_| Error::Transport)?;
        let mut output = read?;
        if !status.success() || !output.ends_with(b"\n200") {
            return Err(Error::Transport);
        }
        output.truncate(output.len() - 4);
        Ok(output)
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::refresh::{Cache, Limits, Selected};
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn curl_bounds_output_and_hides_credentials() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ti49-curl-{}", std::process::id()));
        fs::create_dir(&root)?;
        let program = root.join("curl-test");
        let input = root.join("input");
        let args = root.join("args");
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../contracts/examples/profile_snapshot.valid.json"
        ))?;
        let s: Selected =
            serde_json::from_value(serde_json::json!({"scope": value["profile"]["scope"],
            "generation":value["generation"],"grain":value["grain"],"bucket":value["bucket"],
            "start":value["profile"]["start"],"end":value["profile"]["end"]}))?;
        let mut cache = Cache::new(Limits {
            capacity: 1,
            bytes: 262144,
            ttl_ms: 100,
            refresh_ms: 40,
            timeout_ms: 10,
            backoff_ms: 5,
            recovery_ms: 50,
            attempts: 2,
        })?;
        cache.select(s, 0)?;
        let job = cache.begin(0).ok_or("job")?;
        let token = "test-reader-credential-with-32-bytes";
        let reader =
            HttpsReader::new("https://registry.test/profile-snapshots", "tenant-a", token)?;
        for (code, valid) in [("200", true), ("302", false), ("503", false)] {
            fs::write(
                &program,
                format!(
                    "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'body\\n{code}'\n",
                    input.display(),
                    args.display()
                ),
            )?;
            fs::set_permissions(&program, fs::Permissions::from_mode(0o700))?;
            assert_eq!(reader.fetch_program(&job, &program).is_ok(), valid);
            assert!(!fs::read_to_string(&args)?.contains(token));
            assert!(fs::read_to_string(&input)?.contains(token));
        }
        fs::write(
            &program,
            "#!/bin/sh\ncat >/dev/null\nhead -c 140000 /dev/zero\n",
        )?;
        assert_eq!(reader.fetch_program(&job, &program), Err(Error::Capacity));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
