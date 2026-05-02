use anyhow::{Result, anyhow};
use dialoguer::{Select, theme::ColorfulTheme};

pub trait Prompt: Clone {
    fn select(&self, message: &str, options: &[String]) -> Result<usize>;
}

#[derive(Default, Clone)]
pub struct DialoguerPrompt;

impl Prompt for DialoguerPrompt {
    fn select(&self, message: &str, options: &[String]) -> Result<usize> {
        if options.is_empty() {
            return Err(anyhow!("no options provided"));
        }

        let labels: Vec<&str> = options.iter().map(|option| option.as_str()).collect();
        let theme = ColorfulTheme::default();
        let selection = Select::with_theme(&theme)
            .with_prompt(message)
            .items(&labels)
            .default(0)
            .interact_opt()?;

        selection.ok_or_else(|| anyhow!("no selection was made"))
    }
}

#[derive(Default, Clone)]
pub struct DryRunPrompt;

impl Prompt for DryRunPrompt {
    fn select(&self, message: &str, _options: &[String]) -> Result<usize> {
        println!("[DRY RUN] Would prompt: {}", message);
        // Always select "Do nothing" which is typically the last option
        // This is a fallback - ideally GitCleaner won't call this in dry mode
        Err(anyhow!("dry run mode - no interactive selections"))
    }
}
