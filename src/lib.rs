use std::fs;

use zed_extension_api::{
    self as zed,
    serde_json::{json, Value},
    settings::LspSettings,
    Command, LanguageServerId, Result, Worktree,
};

#[derive(Clone)]
struct LtexBinary {
    path: String,
    args: Option<Vec<String>>,
    use_bundled_java: bool,
}

struct LtexExtension {
    cached_binary: Option<LtexBinary>,
}

impl LtexExtension {
    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<LtexBinary> {
        if let Some(cached_binary) = &self.cached_binary {
            if fs::metadata(&cached_binary.path).is_ok_and(|stat| stat.is_file()) {
                return Ok(cached_binary.clone());
            }
        }

        let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;

        if let Some(binary_settings) = lsp_settings.binary.as_ref() {
            if let Some(path) = &binary_settings.path {
                let binary = LtexBinary {
                    path: path.clone(),
                    args: binary_settings.arguments.clone(),
                    use_bundled_java: false,
                };
                self.cached_binary = Some(binary.clone());
                return Ok(binary);
            }
        }

        if let Some(path) = worktree.which("ltex-ls-plus") {
            let binary = LtexBinary {
                path,
                args: None,
                use_bundled_java: false,
            };
            self.cached_binary = Some(binary.clone());
            return Ok(binary);
        }

        self.download_language_server(language_server_id)
    }

    fn download_language_server(
        &mut self,
        language_server_id: &LanguageServerId,
    ) -> Result<LtexBinary> {
        let (platform, arch) = zed::current_platform();
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = match zed::latest_github_release(
            "ltex-plus/ltex-ls-plus",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        ) {
            Ok(release) => release,
            Err(error) => {
                if let Some(binary) = self.find_downloaded_binary(platform) {
                    self.cached_binary = Some(binary.clone());
                    return Ok(binary);
                }
                return Err(error);
            }
        };
        let version = release.version;

        let asset_stem = format!(
            "ltex-ls-plus-{version}-{os}-{arch}",
            version = version,
            os = match platform {
                zed::Os::Mac => "mac",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X8664 => "x64",
                zed::Architecture::X86 =>
                    return Err(
                        "The requested architecture x86 is not supported by `ltex-ls-plus`.".into()
                    ),
            }
        );
        let asset_name = format!(
            "{asset_stem}.{suffix}",
            suffix = match platform {
                zed::Os::Windows => "zip",
                _ => "tar.gz",
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {asset_name}"))?;

        let version_dir = format!("ltex-ls-plus-{version}");
        let binary_name = match platform {
            zed::Os::Windows => "ltex-ls-plus.bat",
            _ => "ltex-ls-plus",
        };
        let binary_path = format!("{version_dir}/{version_dir}/bin/{binary_name}");

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            let file_kind = match platform {
                zed::Os::Windows => zed::DownloadedFileType::Zip,
                _ => zed::DownloadedFileType::GzipTar,
            };
            zed::download_file(&asset.download_url, &version_dir, file_kind)
                .map_err(|e| format!("failed to download file: {e}"))?;

            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        let binary = LtexBinary {
            path: binary_path,
            args: Some(vec![]),
            use_bundled_java: true,
        };
        self.cached_binary = Some(binary.clone());
        Ok(binary)
    }

    fn find_downloaded_binary(&self, platform: zed::Os) -> Option<LtexBinary> {
        let binary_name = match platform {
            zed::Os::Windows => "ltex-ls-plus.bat",
            _ => "ltex-ls-plus",
        };
        let prefix = "ltex-ls-plus-";
        let mut version_dirs = fs::read_dir(".")
            .ok()?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().into_string().ok()?;
                let version = name
                    .strip_prefix(prefix)?
                    .split('.')
                    .map(str::parse::<u32>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                entry.file_type().ok()?.is_dir().then_some((version, name))
            })
            .collect::<Vec<_>>();
        version_dirs.sort_unstable_by(|a, b| b.cmp(a));

        version_dirs.into_iter().find_map(|(_, version_dir)| {
            let path = format!("{version_dir}/{version_dir}/bin/{binary_name}");
            fs::metadata(&path)
                .is_ok_and(|stat| stat.is_file())
                .then_some(LtexBinary {
                    path,
                    args: Some(vec![]),
                    use_bundled_java: true,
                })
        })
    }
}

fn language_server_environment(
    mut shell_env: Vec<(String, String)>,
    use_bundled_java: bool,
) -> Vec<(String, String)> {
    if use_bundled_java {
        shell_env.retain(|(key, _)| key != "JAVA_HOME");
        shell_env.push(("JAVA_HOME".into(), String::new()));
    }
    shell_env
}

impl zed::Extension for LtexExtension {
    fn new() -> Self {
        Self {
            cached_binary: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let ltex_binary = self.language_server_binary(language_server_id, worktree)?;

        let env = language_server_environment(worktree.shell_env(), ltex_binary.use_bundled_java);

        Ok(zed::Command {
            command: ltex_binary.path,
            args: ltex_binary.args.unwrap_or_default(),
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> Result<Option<zed_extension_api::serde_json::Value>> {
        let mut options = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.initialization_options.clone())
            .unwrap_or_default();
        default_to_server_managed_external_files(&mut options);
        Ok(Some(options))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> Result<Option<zed_extension_api::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone())
            .unwrap_or_default();
        Ok(Some(settings))
    }
}

/// Sets `ltex.externalFiles.managedByEditor` to `false` unless the user set it.
///
/// "Add to dictionary", "Disable rule" and "Hide false positive" write to files.
/// Zed cannot run extension code when the user picks a code action, so the
/// extension cannot write these files. With this option, LTeX+ LS (19.0.0 or
/// later) reads and writes them itself. Older servers ignore the option.
fn default_to_server_managed_external_files(options: &mut Value) {
    if options.is_null() {
        *options = json!({});
    }
    let Some(external_files) = options
        .as_object_mut()
        .and_then(|root| {
            root.entry("ltex")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        })
        .and_then(|ltex| {
            ltex.entry("externalFiles")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        })
    else {
        return;
    };
    external_files
        .entry("managedByEditor")
        .or_insert(Value::Bool(false));
}

zed::register_extension!(LtexExtension);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn host_environment(
        shell_env: &[(&str, &str)],
        command_env: Vec<(String, String)>,
        explicit_env: &[(&str, &str)],
    ) -> HashMap<String, String> {
        let mut env = shell_env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>();
        env.extend(command_env);
        env.extend(
            explicit_env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string())),
        );
        env
    }

    #[test]
    fn bundled_binary_forces_its_java_and_preserves_other_environment() {
        let shell_env = [("JAVA_HOME", "/system/java"), ("PATH", "/usr/bin")];
        let command_env = language_server_environment(
            shell_env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            true,
        );

        let env = host_environment(&shell_env, command_env, &[]);

        assert_eq!(env.get("JAVA_HOME").map(String::as_str), Some(""));
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
    }

    #[test]
    fn local_or_custom_binary_preserves_java_home() {
        let shell_env = [("JAVA_HOME", "/system/java"), ("PATH", "/usr/bin")];
        let command_env = language_server_environment(
            shell_env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            false,
        );

        let env = host_environment(&shell_env, command_env, &[]);

        assert_eq!(
            env.get("JAVA_HOME").map(String::as_str),
            Some("/system/java")
        );
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
    }

    #[test]
    fn explicit_host_override_wins_for_bundled_binary() {
        let shell_env = [("JAVA_HOME", "/system/java")];
        let command_env = language_server_environment(
            shell_env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            true,
        );

        let env = host_environment(&shell_env, command_env, &[("JAVA_HOME", "/chosen/java")]);

        assert_eq!(
            env.get("JAVA_HOME").map(String::as_str),
            Some("/chosen/java")
        );
    }

    #[test]
    fn adds_the_default_when_nothing_is_set() {
        let mut options = Value::Null;
        default_to_server_managed_external_files(&mut options);
        assert_eq!(
            options,
            json!({"ltex": {"externalFiles": {"managedByEditor": false}}})
        );
    }

    #[test]
    fn keeps_the_users_value_and_other_options() {
        let mut options =
            json!({"locale": "de", "ltex": {"externalFiles": {"managedByEditor": true}}});
        default_to_server_managed_external_files(&mut options);
        assert_eq!(
            options,
            json!({"locale": "de", "ltex": {"externalFiles": {"managedByEditor": true}}})
        );
    }
}
