use std::fs;
use std::path::PathBuf;

use zed_extension_api::serde_json::{Value, json};
use zed_extension_api::{self as zed, LanguageServerId, Result, settings::LspSettings};

struct TsrsExtension {
    cached_binary_path: Option<String>,
    cached_version: Option<String>,
}

#[derive(Debug, Default)]
struct TsrsSettings {
    package_version: Option<String>,
}

impl TsrsSettings {
    fn from_lsp_settings(settings: &Value) -> Self {
        let package_version = settings
            .get("package_version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Self { package_version }
    }
}

fn merge_json_value_into(source: Value, target: &mut Value) {
    match (source, target) {
        (Value::Object(source), Value::Object(target)) => {
            for (key, value) in source {
                if let Some(target) = target.get_mut(&key) {
                    merge_json_value_into(value, target);
                } else {
                    target.insert(key, value);
                }
            }
        }

        (Value::Array(source), Value::Array(target)) => {
            for value in source {
                target.push(value);
            }
        }

        (source, target) => *target = source,
    }
}

impl TsrsExtension {
    fn get_platform_package_name() -> Result<&'static str> {
        match zed::current_platform() {
            (zed::Os::Mac, zed::Architecture::Aarch64) => Ok("@tsrs/darwin-arm64"),
            (zed::Os::Linux, zed::Architecture::Aarch64) => Ok("@tsrs/linux-arm64-gnu"),
            (zed::Os::Linux, zed::Architecture::X8664) => Ok("@tsrs/linux-x64-gnu"),
            _ => Err("tsrs supports macOS ARM64 and Linux ARM64/x64 with glibc.".to_string()),
        }
    }

    fn get_native_binary_path() -> Result<PathBuf> {
        let platform_package = Self::get_platform_package_name()?;

        // Try to find the platform-specific package
        let package_path = PathBuf::from("node_modules").join(platform_package);

        if !package_path.exists() {
            return Err(format!(
                "Platform package {platform_package} not found at {package_path}. \
                Make sure the correct tsrs platform package is installed.",
                package_path = package_path.display()
            ));
        }

        let binary_path = package_path.join("tsrs");

        if !binary_path.exists() {
            return Err(format!(
                "Native binary not found at {}. The platform package may be corrupted.",
                binary_path.display()
            ));
        }

        Ok(binary_path)
    }

    fn binary_exists(&self) -> bool {
        Self::get_native_binary_path().is_ok()
    }

    fn get_installed_version(&self, package_name: &str) -> Option<String> {
        zed::npm_package_installed_version(package_name).ok().flatten()
    }

    fn should_install_or_update(&self, package_name: &str, target_version: &str) -> bool {
        if !self.binary_exists() {
            return true;
        }

        match self.get_installed_version(package_name) {
            Some(installed_version) => installed_version != target_version,
            None => true,
        }
    }

    fn install_package(&mut self, id: &LanguageServerId, custom_version: Option<&str>) -> Result<()> {
        let package_name = Self::get_platform_package_name()?;

        zed::set_language_server_installation_status(id, &zed::LanguageServerInstallationStatus::CheckingForUpdate);

        let target_version = match custom_version {
            Some(version) => version.to_string(),
            None => zed::npm_package_latest_version(package_name)?,
        };

        if self.should_install_or_update(package_name, &target_version) {
            zed::set_language_server_installation_status(id, &zed::LanguageServerInstallationStatus::Downloading);

            let result = zed::npm_install_package(package_name, &target_version);
            if let Err(error) = result
                && !self.binary_exists()
            {
                return Err(error);
            }
        }

        let binary_path = Self::get_native_binary_path()
            .map_err(|e| format!("Failed to locate native binary after installation: {}", e))?;

        // Cache the successful installation
        self.cached_binary_path = Some(binary_path.to_string_lossy().to_string());
        self.cached_version = Some(target_version);

        Ok(())
    }

    fn binary_path(&mut self, id: &LanguageServerId, package_version: Option<&str>) -> Result<String> {
        // Return cached path if we have it and binary still exists
        if let Some(cached_path) = self.cached_binary_path.as_ref()
            && fs::metadata(cached_path).is_ok_and(|stat| stat.is_file())
        {
            return Ok(cached_path.clone());
        }

        // Install or update package as needed
        self.install_package(id, package_version)?;

        let binary_path =
            Self::get_native_binary_path().map_err(|e| format!("Failed to locate native binary: {}", e))?;

        Ok(binary_path.to_string_lossy().to_string())
    }
}

impl zed::Extension for TsrsExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
            cached_version: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed_extension_api::LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> zed_extension_api::Result<zed_extension_api::Command> {
        let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree).ok();

        let settings = lsp_settings
            .as_ref()
            .and_then(|settings| settings.settings.as_ref().map(TsrsSettings::from_lsp_settings))
            .unwrap_or_default();

        let env = lsp_settings
            .and_then(|lsp_settings| lsp_settings.binary)
            .and_then(|binary| binary.env);

        let package_version = settings.package_version.as_deref();
        let executable_path = self.binary_path(language_server_id, package_version)?;

        Ok(zed::Command {
            command: std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(executable_path)
                .to_string_lossy()
                .into_owned(),
            args: vec!["--lsp".into(), "-stdio".into()],
            env: env.into_iter().flat_map(|env| env.into_iter()).collect(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &zed_extension_api::LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> zed_extension_api::Result<Option<zed_extension_api::serde_json::Value>> {
        let mut initialization_options = json!({
            "codeLensShowLocationsCommandName": "editor.action.showReferences"
        });

        if let Some(user_initialization_options) =
            LspSettings::for_worktree(server_id.as_ref(), worktree)?.initialization_options
        {
            merge_json_value_into(user_initialization_options, &mut initialization_options);
        }

        Ok(Some(initialization_options))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &zed_extension_api::LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> zed_extension_api::Result<Option<zed_extension_api::serde_json::Value>> {
        let config = json!({
            "inlayHints": {
                "parameterNames": {
                    "enabled": "all",
                    "suppressWhenArgumentMatchesName": false
                },
                "parameterTypes": {
                    "enabled": true
                },
                "variableTypes": {
                    "enabled": true,
                    "suppressWhenTypeMatchesName": false
                },
                "propertyDeclarationTypes": {
                    "enabled": true
                },
                "functionLikeReturnTypes": {
                    "enabled": true
                },
                "enumMemberValues": {
                    "enabled": true
                }
            },
            "implementationsCodeLens": {
                "enabled": true,
                "showOnAllClassMethods": true,
                "showOnInterfaceMethods": true
            },
            "referencesCodeLens": {
                "enabled": true,
                "showOnAllFunctions": true
            }
        });

        let mut settings = json!({
            "typescript": config.clone(),
            "javascript": config
        });

        if let Some(user_settings) = LspSettings::for_worktree(server_id.as_ref(), worktree)?.settings {
            merge_json_value_into(user_settings, &mut settings);
        }

        Ok(Some(settings))
    }
}

zed::register_extension!(TsrsExtension);
