use std::io::Read;
use std::path::Path;

use anyhow::{Context as _, Result};
use gpui::App;
use serde::Deserialize;
use terminal::{
    AbbreviationStoreEntity, ButtonBarStoreEntity, ButtonConfig, RuleStoreEntity,
    abbr_store::{Abbreviation, AbbreviationStore},
    button_bar_config::ButtonBarStore,
    rule_store::{AutomationRule, RuleStore},
};
use zip::ZipArchive;

#[derive(Deserialize)]
struct ConfigManifest {
    #[allow(dead_code)]
    version: u32,
    #[allow(dead_code)]
    description: Option<String>,
}

#[derive(Default)]
pub struct ImportResult {
    pub button_bar_imported: bool,
    pub abbreviations_imported: bool,
    pub rules_imported: bool,
    pub scripts_copied: usize,
}

impl ImportResult {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.button_bar_imported {
            parts.push("Button Bar");
        }
        if self.abbreviations_imported {
            parts.push("Abbreviations");
        }
        if self.rules_imported {
            parts.push("Terminal Rules");
        }
        if self.scripts_copied > 0 {
            parts.push("Scripts");
        }
        if parts.is_empty() {
            "No configurations imported".to_string()
        } else {
            format!("Imported: {}", parts.join(", "))
        }
    }
}

pub fn import_config_from_zip(path: &Path, cx: &mut App) -> Result<ImportResult> {
    let file = std::fs::File::open(path).context("Failed to open ZIP file")?;
    let mut archive = ZipArchive::new(file).context("Failed to read ZIP archive")?;

    let mut result = ImportResult::default();

    if let Ok(mut file) = archive.by_name("config.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let _manifest: ConfigManifest =
            serde_json::from_str(&content).context("Invalid config.json")?;
    }

    if let Ok(mut file) = archive.by_name("button_bar.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        if let Ok(config) = serde_json::from_str::<ButtonBarStore>(&content) {
            import_button_bar(&config, cx)?;
            result.button_bar_imported = true;
        }
    }

    if let Ok(mut file) = archive.by_name("abbreviations.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        if let Ok(config) = serde_json::from_str::<AbbreviationStore>(&content) {
            import_abbreviations(&config, cx)?;
            result.abbreviations_imported = true;
        }
    }

    if let Ok(mut file) = archive.by_name("terminal_rules.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        if let Ok(config) = serde_json::from_str::<RuleStore>(&content) {
            import_rules(&config, cx)?;
            result.rules_imported = true;
        }
    }

    let scripts_dir = paths::config_dir().join("scripts");
    std::fs::create_dir_all(&scripts_dir).context("Failed to create scripts directory")?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        if name.starts_with("scripts/") && name.ends_with(".py") {
            if let Some(script_name) = name.strip_prefix("scripts/") {
                if script_name.is_empty() {
                    continue;
                }
                let dest = scripts_dir.join(script_name);
                let mut content = Vec::new();
                file.read_to_end(&mut content)?;
                std::fs::write(&dest, &content)?;
                result.scripts_copied += 1;
            }
        }
    }

    Ok(result)
}

fn import_button_bar(config: &ButtonBarStore, cx: &mut App) -> Result<()> {
    let Some(store_entity) = ButtonBarStoreEntity::try_global(cx) else {
        return Ok(());
    };

    store_entity.update(cx, |store_entity, cx| {
        let existing_scripts: std::collections::HashSet<_> = store_entity
            .buttons()
            .iter()
            .filter_map(|b| b.script_path.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect();

        let scripts_dir = paths::config_dir().join("scripts");

        for button in &config.buttons {
            let script_filename = button
                .script_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if existing_scripts.contains(&script_filename) {
                continue;
            }

            let resolved_path = if button.script_path.is_absolute() {
                button.script_path.clone()
            } else {
                scripts_dir.join(&button.script_path)
            };

            let new_button = ButtonConfig {
                id: uuid::Uuid::new_v4(),
                label: button.label.clone(),
                script_path: resolved_path,
                tooltip: button.tooltip.clone(),
                icon: button.icon.clone(),
                enabled: button.enabled,
            };

            store_entity.add_button(new_button, cx);
        }
    });

    Ok(())
}

fn import_abbreviations(config: &AbbreviationStore, cx: &mut App) -> Result<()> {
    let Some(store_entity) = AbbreviationStoreEntity::try_global(cx) else {
        return Ok(());
    };

    store_entity.update(cx, |store_entity, cx| {
        let existing_triggers: std::collections::HashSet<_> = store_entity
            .abbreviations()
            .iter()
            .map(|a| a.trigger.clone())
            .collect();

        for abbr in &config.abbreviations {
            if existing_triggers.contains(&abbr.trigger) {
                continue;
            }

            let new_abbr = Abbreviation {
                id: uuid::Uuid::new_v4(),
                trigger: abbr.trigger.clone(),
                expansion: abbr.expansion.clone(),
                enabled: abbr.enabled,
                protocol: abbr.protocol.clone(),
            };

            store_entity.add_abbreviation(new_abbr, cx);
        }
    });

    Ok(())
}

fn import_rules(config: &RuleStore, cx: &mut App) -> Result<()> {
    let Some(store_entity) = RuleStoreEntity::try_global(cx) else {
        return Ok(());
    };

    store_entity.update(cx, |store_entity, cx| {
        let existing_names: std::collections::HashSet<_> =
            store_entity.rules().iter().map(|r| r.name.clone()).collect();

        for rule in &config.rules {
            if existing_names.contains(&rule.name) {
                continue;
            }

            let new_rule = AutomationRule {
                id: uuid::Uuid::new_v4(),
                name: rule.name.clone(),
                enabled: rule.enabled,
                trigger: rule.trigger.clone(),
                max_triggers: rule.max_triggers,
                condition: rule.condition.clone(),
                action: rule.action.clone(),
            };

            store_entity.add_rule(new_rule, cx);
        }
    });

    Ok(())
}
