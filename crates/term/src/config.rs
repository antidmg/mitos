use crate::keymap;
use crate::keymap::{merge_keys, KeyTrie};
use loader::merge_toml_values;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::fs;
use std::io::Error as IOError;
use toml::de::Error as TomlError;
use view::custom_commands::{CustomCommand, CustomCommands};
use view::{document::Mode, theme};

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub theme: Option<theme::Config>,
    pub keys: HashMap<Mode, KeyTrie>,
    pub editor: view::editor::Config,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigRaw {
    pub theme: Option<theme::Config>,
    pub keys: Option<HashMap<Mode, KeyTrie>>,
    pub editor: Option<toml::Value>,
    commands: Option<Commands>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Commands {
    #[serde(flatten)]
    commands: BTreeMap<String, CustomCommand>,
}

impl Commands {
    fn merge(&mut self, other: Self) {
        self.commands.extend(other.commands);
    }

    fn into_custom_commands(self) -> CustomCommands {
        CustomCommands::new(
            self.commands
                .into_iter()
                .map(|(name, command)| command.named(name))
                .collect(),
        )
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            theme: None,
            keys: keymap::default(),
            editor: view::editor::Config::default(),
        }
    }
}

#[derive(Debug)]
pub enum ConfigLoadError {
    BadConfig(TomlError),
    Error(IOError),
}

impl Default for ConfigLoadError {
    fn default() -> Self {
        ConfigLoadError::Error(IOError::new(std::io::ErrorKind::NotFound, "place holder"))
    }
}

impl Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigLoadError::BadConfig(err) => err.fmt(f),
            ConfigLoadError::Error(err) => err.fmt(f),
        }
    }
}

impl Config {
    pub fn load(
        global: Result<&String, ConfigLoadError>,
        local: Result<String, ConfigLoadError>,
    ) -> Result<Config, ConfigLoadError> {
        let global_config: Result<ConfigRaw, ConfigLoadError> =
            global.and_then(|file| toml::from_str(file).map_err(ConfigLoadError::BadConfig));
        let local_config: Result<ConfigRaw, ConfigLoadError> =
            local.and_then(|file| toml::from_str(&file).map_err(ConfigLoadError::BadConfig));
        let res = match (global_config, local_config) {
            (Ok(mut global), Ok(local)) => {
                let mut keys = keymap::default();
                if let Some(global_keys) = global.keys {
                    merge_keys(&mut keys, global_keys)
                }
                if let Some(local_keys) = local.keys {
                    merge_keys(&mut keys, local_keys)
                }

                let mut editor = match (global.editor, local.editor) {
                    (None, None) => view::editor::Config::default(),
                    (None, Some(val)) | (Some(val), None) => {
                        val.try_into().map_err(ConfigLoadError::BadConfig)?
                    }
                    (Some(global), Some(local)) => merge_toml_values(global, local, 3)
                        .try_into()
                        .map_err(ConfigLoadError::BadConfig)?,
                };

                if let Some(local_commands) = local.commands {
                    if let Some(global_commands) = &mut global.commands {
                        global_commands.merge(local_commands);
                    } else {
                        global.commands = Some(local_commands);
                    }
                }
                if let Some(commands) = global.commands {
                    editor.commands = commands.into_custom_commands();
                }

                Config {
                    theme: local.theme.or(global.theme),
                    keys,
                    editor,
                }
            }
            // if any configs are invalid return that first
            (_, Err(ConfigLoadError::BadConfig(err)))
            | (Err(ConfigLoadError::BadConfig(err)), _) => {
                return Err(ConfigLoadError::BadConfig(err))
            }
            (Ok(config), Err(_)) | (Err(_), Ok(config)) => {
                let mut keys = keymap::default();
                if let Some(keymap) = config.keys {
                    merge_keys(&mut keys, keymap);
                }
                let mut editor = config.editor.map_or_else(
                    || Ok(view::editor::Config::default()),
                    |val| val.try_into().map_err(ConfigLoadError::BadConfig),
                )?;
                if let Some(commands) = config.commands {
                    editor.commands = commands.into_custom_commands();
                }

                Config {
                    theme: config.theme,
                    keys,
                    editor,
                }
            }

            // these are just two io errors return the one for the global config
            (Err(err), Err(_)) => return Err(err),
        };

        Ok(res)
    }

    pub fn load_default() -> Result<Config, ConfigLoadError> {
        let global_config =
            fs::read_to_string(loader::config_file()).map_err(ConfigLoadError::Error)?;
        let local_config =
            fs::read_to_string(loader::workspace_config_file()).map_err(ConfigLoadError::Error);

        let phony_config = ConfigLoadError::Error(IOError::other("hacky placeholder"));
        let global_parsed = Config::load(Ok(&global_config), Err(phony_config))?;

        // We need to build a transient `WorkspaceTrust` just to ask whether the workspace is
        // trusted enough to load its `.mitos/config.toml`. The persisted-trust file on disk is the
        // source of truth either way; this transient instance has an empty cache and is dropped
        // after the check.
        let trust = loader::workspace_trust::WorkspaceTrust::new(
            (&global_parsed.editor.workspace_trust).into(),
        );
        if trust
            .query_current(loader::workspace_trust::TrustQuery::LocalConfig)
            .is_trusted()
        {
            let mut merged = Config::load(Ok(&global_config), local_config)?;
            // editor.workspace-trust is global/user-scope only. Without this override, a
            // workspace's `.mitos/config.toml` could set `level = "insecure"`; once the user trusted
            // *that* workspace, refresh_config would re-load with the override merged in and from
            // then on every subsequent workspace in the session would be implicitly trusted. Pin
            // the gate's own configuration to the global file.
            merged.editor.workspace_trust = global_parsed.editor.workspace_trust;
            Ok(merged)
        } else {
            Ok(global_parsed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Config {
        fn load_test(config: &str) -> Config {
            Config::load(Ok(&config.to_owned()), Err(ConfigLoadError::default())).unwrap()
        }

        fn load_test_result(config: &str) -> Result<Config, ConfigLoadError> {
            Config::load(Ok(&config.to_owned()), Err(ConfigLoadError::default()))
        }
    }

    #[test]
    fn parsing_keymaps_config_file() {
        use crate::keymap;
        use editor_core::hashmap;
        use view::document::Mode;

        let sample_keymaps = r#"
            [keys.insert]
            y = "move_line_down"
            S-C-a = "delete_selection"

            [keys.normal]
            A-F12 = "move_next_word_end"
        "#;

        let mut keys = keymap::default();
        merge_keys(
            &mut keys,
            hashmap! {
                Mode::Insert => keymap!({ "Insert mode"
                    "y" => move_line_down,
                    "S-C-a" => delete_selection,
                }),
                Mode::Normal => keymap!({ "Normal mode"
                    "A-F12" => move_next_word_end,
                }),
            },
        );

        assert_eq!(
            Config::load_test(sample_keymaps),
            Config {
                keys,
                ..Default::default()
            }
        );
    }

    #[test]
    fn keys_resolve_to_correct_defaults() {
        // From serde default
        let default_keys = Config::load_test("").keys;
        assert_eq!(default_keys, keymap::default());

        // From the Default trait
        let default_keys = Config::default().keys;
        assert_eq!(default_keys, keymap::default());
    }

    #[test]
    fn icons_are_controlled_by_one_editor_option() {
        assert!(!Config::load_test("").editor.icons);
        assert!(Config::load_test("[editor]\nicons = true").editor.icons);
    }

    #[test]
    fn breadcrumbs_are_disabled_by_default_and_configurable() {
        use view::editor::BreadcrumbPathOptions;

        let default = Config::load_test("").editor.breadcrumb;
        assert!(!default.enable);
        assert_eq!(default.path, BreadcrumbPathOptions::Full);

        let configured = Config::load_test("[editor.breadcrumb]\nenable = true\npath = \"file\"")
            .editor
            .breadcrumb;
        assert!(configured.enable);
        assert_eq!(configured.path, BreadcrumbPathOptions::File);
    }

    #[test]
    fn welcome_screen_is_enabled_by_default_and_can_be_disabled() {
        assert!(Config::load_test("").editor.welcome_screen);
        assert!(
            !Config::load_test("[editor]\nwelcome-screen = false")
                .editor
                .welcome_screen
        );
    }

    #[test]
    fn popup_border_is_not_configurable() {
        let config = "[editor]\npopup-border = \"none\"".to_owned();
        let error = Config::load(Ok(&config), Err(ConfigLoadError::default())).unwrap_err();
        assert!(error.to_string().contains("unknown field `popup-border`"));
    }

    #[test]
    fn deserializes_custom_commands() {
        let config = Config::load_test(
            r#"
[commands]
":wq" = [":write", ":quit"]
":w" = ":write!"
"0" = ":goto 1"

[commands.":wcd!"]
commands = [":write! %arg{0}", ":cd %sh{ %arg{0} | path dirname }"]
desc = "Force save, then change directory"
accepts = "<path>"
completer = ":write"
"#,
        );

        assert!(config.editor.commands.get("wq").is_some());
        assert!(config.editor.commands.get("0").unwrap().hidden);
        assert_eq!(
            config
                .editor
                .commands
                .get("wcd!")
                .unwrap()
                .completer
                .as_deref(),
            Some("write")
        );
    }

    #[test]
    fn local_custom_commands_override_global_commands() {
        let global = "[commands]\n':save' = ':write'".to_owned();
        let local = "[commands]\n':save' = ':write!'\n':quit' = ':quit'".to_owned();
        let config = Config::load(Ok(&global), Ok(local)).unwrap();

        assert_eq!(
            config.editor.commands.get("save").unwrap().commands,
            [":write!"]
        );
        assert!(config.editor.commands.get("quit").is_some());
    }

    #[test]
    fn rejects_macros_in_command_sequences() {
        let error =
            Config::load_test_result("[commands]\n':fail' = { commands = ['@100xd', ':write'] }")
                .unwrap_err();
        assert!(error
            .to_string()
            .contains("macro keybindings may not be used in command sequences"));
    }
}
