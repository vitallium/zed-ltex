use std::{fs, path::Path};

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
        self.download_language_server_with(
            platform,
            arch,
            Path::new("."),
            || {
                zed::latest_github_release(
                    "ltex-plus/ltex-ls-plus",
                    zed::GithubReleaseOptions {
                        require_assets: true,
                        pre_release: false,
                    },
                )
            },
            |url, version_dir, file_kind| {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Downloading,
                );
                zed::download_file(url, version_dir, file_kind)
                    .map_err(|e| format!("failed to download file: {e}"))
            },
        )
    }

    fn download_language_server_with(
        &mut self,
        platform: zed::Os,
        arch: zed::Architecture,
        root: &Path,
        fetch_release: impl FnOnce() -> Result<zed::GithubRelease>,
        download: impl FnOnce(&str, &str, zed::DownloadedFileType) -> Result<()>,
    ) -> Result<LtexBinary> {
        let fallback = self.find_downloaded_binary_in(platform, root);
        let update = (|| {
            let release = fetch_release()?;
            let version = release.version;
            let asset_stem = format!(
                "ltex-ls-plus-{version}-{os}-{arch}",
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
                            "The requested architecture x86 is not supported by `ltex-ls-plus`."
                                .into(),
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
            let download_url = release
                .assets
                .iter()
                .find(|asset| asset.name == asset_name)
                .map(|asset| &asset.download_url)
                .ok_or_else(|| format!("no asset found matching {asset_name}"))?;

            let version_dir = format!("ltex-ls-plus-{version}");
            let binary_name = match platform {
                zed::Os::Windows => "ltex-ls-plus.bat",
                _ => "ltex-ls-plus",
            };
            let install_path = root.join(&version_dir);
            let binary_path = install_path
                .join(&version_dir)
                .join("bin")
                .join(binary_name);

            if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
                let file_kind = match platform {
                    zed::Os::Windows => zed::DownloadedFileType::Zip,
                    _ => zed::DownloadedFileType::GzipTar,
                };
                if let Err(error) = download(download_url, &version_dir, file_kind) {
                    remove_failed_install(&install_path, &binary_path);
                    return Err(error);
                }

                if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
                    remove_failed_install(&install_path, &binary_path);
                    return Err(format!(
                        "downloaded archive did not contain expected executable {}",
                        binary_path.display()
                    ));
                }

                let entries = fs::read_dir(root)
                    .map_err(|e| format!("failed to list working directory {e}"))?;
                for entry in entries {
                    let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                    if entry.file_name().to_str() != Some(&version_dir) {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }

            Ok(LtexBinary {
                path: binary_path.to_string_lossy().into_owned(),
                args: Some(vec![]),
                use_bundled_java: true,
            })
        })();

        match update {
            Ok(binary) => {
                self.cached_binary = Some(binary.clone());
                Ok(binary)
            }
            Err(error) => {
                if let Some(binary) = fallback {
                    self.cached_binary = Some(binary.clone());
                    Ok(binary)
                } else {
                    Err(error)
                }
            }
        }
    }

    fn find_downloaded_binary_in(&self, platform: zed::Os, root: &Path) -> Option<LtexBinary> {
        let binary_name = match platform {
            zed::Os::Windows => "ltex-ls-plus.bat",
            _ => "ltex-ls-plus",
        };
        let prefix = "ltex-ls-plus-";
        let mut version_dirs = fs::read_dir(root)
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
            let path = root
                .join(&version_dir)
                .join(&version_dir)
                .join("bin")
                .join(binary_name);
            fs::metadata(&path)
                .is_ok_and(|stat| stat.is_file())
                .then_some(LtexBinary {
                    path: path.to_string_lossy().into_owned(),
                    args: Some(vec![]),
                    use_bundled_java: true,
                })
        })
    }
}

fn remove_failed_install(install_path: &Path, binary_path: &Path) {
    fs::remove_file(binary_path).ok();
    fs::remove_dir_all(install_path).ok();
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
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "zed-ltex-test-{}-{}",
                std::process::id(),
                NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl AsRef<Path> for TempDir {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn installed_binary(root: &Path, version: &str) -> PathBuf {
        let version_dir = format!("ltex-ls-plus-{version}");
        let path = root
            .join(&version_dir)
            .join(&version_dir)
            .join("bin/ltex-ls-plus");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "executable").unwrap();
        path
    }

    fn linux_release(version: &str) -> zed::GithubRelease {
        zed::GithubRelease {
            version: version.into(),
            assets: vec![zed::GithubReleaseAsset {
                name: format!("ltex-ls-plus-{version}-linux-x64.tar.gz"),
                download_url: "https://example.com/ltex.tar.gz".into(),
            }],
        }
    }

    fn extension() -> LtexExtension {
        LtexExtension {
            cached_binary: None,
        }
    }

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
    fn metadata_failure_uses_complete_downloaded_binary() {
        let root = TempDir::new();
        let installed = installed_binary(root.as_ref(), "1.0.0");
        let mut extension = extension();

        let binary = extension
            .download_language_server_with(
                zed::Os::Linux,
                zed::Architecture::X8664,
                root.as_ref(),
                || Err("metadata unavailable".into()),
                |_, _, _| panic!("download must not run"),
            )
            .unwrap();

        assert_eq!(binary.path, installed.to_string_lossy());
        assert!(binary.use_bundled_java);
    }

    #[test]
    fn missing_asset_uses_complete_downloaded_binary() {
        let root = TempDir::new();
        let installed = installed_binary(root.as_ref(), "1.0.0");
        let mut extension = extension();

        let binary = extension
            .download_language_server_with(
                zed::Os::Linux,
                zed::Architecture::X8664,
                root.as_ref(),
                || {
                    Ok(zed::GithubRelease {
                        version: "2.0.0".into(),
                        assets: vec![],
                    })
                },
                |_, _, _| panic!("download must not run"),
            )
            .unwrap();

        assert_eq!(binary.path, installed.to_string_lossy());
    }

    #[test]
    fn download_failure_uses_complete_downloaded_binary() {
        let root = TempDir::new();
        let installed = installed_binary(root.as_ref(), "1.0.0");
        let mut extension = extension();

        let binary = extension
            .download_language_server_with(
                zed::Os::Linux,
                zed::Architecture::X8664,
                root.as_ref(),
                || Ok(linux_release("2.0.0")),
                |_, _, _| Err("archive unavailable".into()),
            )
            .unwrap();

        assert_eq!(binary.path, installed.to_string_lossy());
    }

    #[test]
    fn download_error_after_launcher_creation_uses_preexisting_fallback() {
        let root = TempDir::new();
        let installed = installed_binary(root.as_ref(), "1.0.0");
        let failed = root.as_ref().join("ltex-ls-plus-2.0.0");
        let mut extension = extension();

        let binary = extension
            .download_language_server_with(
                zed::Os::Linux,
                zed::Architecture::X8664,
                root.as_ref(),
                || Ok(linux_release("2.0.0")),
                |_, _, _| {
                    installed_binary(root.as_ref(), "2.0.0");
                    Err("extraction failed".into())
                },
            )
            .unwrap();

        assert_eq!(binary.path, installed.to_string_lossy());
        assert!(!failed.exists());
    }

    #[test]
    fn failed_partial_install_is_not_eligible_on_next_invocation() {
        let root = TempDir::new();
        let failed = root.as_ref().join("ltex-ls-plus-2.0.0");
        let mut extension = extension();

        let first = extension.download_language_server_with(
            zed::Os::Linux,
            zed::Architecture::X8664,
            root.as_ref(),
            || Ok(linux_release("2.0.0")),
            |_, _, _| {
                installed_binary(root.as_ref(), "2.0.0");
                Err("original extraction error".into())
            },
        );

        assert_eq!(first.err().as_deref(), Some("original extraction error"));
        assert!(!failed.exists());

        let second = extension.download_language_server_with(
            zed::Os::Linux,
            zed::Architecture::X8664,
            root.as_ref(),
            || Err("next metadata error".into()),
            |_, _, _| panic!("download must not run"),
        );

        assert_eq!(second.err().as_deref(), Some("next metadata error"));
    }

    #[test]
    fn successful_update_wins_and_removes_old_install() {
        let root = TempDir::new();
        let old = installed_binary(root.as_ref(), "1.0.0");
        let expected = root
            .as_ref()
            .join("ltex-ls-plus-2.0.0/ltex-ls-plus-2.0.0/bin/ltex-ls-plus");
        let mut extension = extension();

        let binary = extension
            .download_language_server_with(
                zed::Os::Linux,
                zed::Architecture::X8664,
                root.as_ref(),
                || Ok(linux_release("2.0.0")),
                |_, _, _| {
                    installed_binary(root.as_ref(), "2.0.0");
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(binary.path, expected.to_string_lossy());
        assert!(expected.is_file());
        assert!(!old.exists());
    }

    #[test]
    fn update_error_without_complete_install_returns_original_error() {
        let root = TempDir::new();
        let mut extension = extension();

        let result = extension.download_language_server_with(
            zed::Os::Linux,
            zed::Architecture::X8664,
            root.as_ref(),
            || Ok(linux_release("2.0.0")),
            |_, _, _| Err("original download error".into()),
        );

        assert_eq!(result.err().as_deref(), Some("original download error"));
    }

    #[test]
    fn incomplete_update_keeps_complete_older_install_for_fallback() {
        let root = TempDir::new();
        let installed = installed_binary(root.as_ref(), "1.0.0");
        let mut extension = extension();

        let binary = extension
            .download_language_server_with(
                zed::Os::Linux,
                zed::Architecture::X8664,
                root.as_ref(),
                || Ok(linux_release("2.0.0")),
                |_, _, _| {
                    fs::create_dir(root.as_ref().join("ltex-ls-plus-2.0.0"))
                        .map_err(|error| error.to_string())?;
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(binary.path, installed.to_string_lossy());
        assert!(installed.is_file());
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
