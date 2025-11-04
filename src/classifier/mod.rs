use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Terminal,
    NaturalLanguage,
}

pub fn classify(_input: &str) -> Result<Classification> {
    // TODO: integrate with Cerebras llama-3.3-70b classifier
    Ok(Classification::NaturalLanguage)
}
