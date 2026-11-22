use std::path::PathBuf;
use std::process::Command;

fn docker_binary() -> PathBuf {
    #[cfg(target_os = "macos")]
    for path in [
        "/usr/local/bin/docker",
        "/Applications/Docker.app/Contents/Resources/bin/docker",
    ] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("ProgramFiles") {
        let path = PathBuf::from(root).join("Docker/Docker/resources/bin/docker.exe");
        if path.is_file() {
            return path;
        }
    }
    "docker".into()
}

fn local_docker_host() -> String {
    #[cfg(unix)]
    {
        let mut sockets = Vec::new();
        #[cfg(target_os = "macos")]
        if let Some(home) = std::env::var_os("HOME") {
            sockets.push(PathBuf::from(home).join(".docker/run/docker.sock"));
        }
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            sockets.push(PathBuf::from(dir).join("docker.sock"));
        }
        sockets.push(PathBuf::from("/var/run/docker.sock"));
        if let Some(socket) = sockets
            .iter()
            .find(|socket| socket.is_absolute() && socket.exists())
        {
            return format!("unix://{}", socket.display());
        }
        "unix:///var/run/docker.sock".into()
    }
    #[cfg(windows)]
    {
        "npipe:////./pipe/docker_engine".into()
    }
}

pub(crate) fn docker_command() -> Command {
    let mut command = Command::new(docker_binary());
    command
        .env_remove("DOCKER_HOST")
        .env_remove("DOCKER_CONTEXT")
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .arg("--host")
        .arg(local_docker_host());
    crate::hide_console(&mut command);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_command_pins_an_explicit_local_endpoint() {
        let command = docker_command();
        let mut args = command.get_args();
        assert_eq!(args.next(), Some(std::ffi::OsStr::new("--host")));
        let endpoint = args.next().unwrap().to_string_lossy();
        assert!(endpoint.starts_with("unix://") || endpoint.starts_with("npipe://"));
    }
}
