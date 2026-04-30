use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionReport {
    pub operation: String,
    pub executed: usize,
    pub blocked: usize,
    pub guarded: usize,
    pub skipped: usize,
    pub errors: usize,
    pub asset_generated: usize,
    pub details: Vec<String>,
}

impl ExecutionReport {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            executed: 0,
            blocked: 0,
            guarded: 0,
            skipped: 0,
            errors: 0,
            asset_generated: 0,
            details: Vec::new(),
        }
    }

    pub fn summary_line(&self) -> String {
        let mut chars = self.operation.chars();
        let display = match chars.next() {
            Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
            None => self.operation.clone(),
        };
        format!(
            "{} executed: {} actions, {} blocked, {} guarded, {} skipped, {} errors, {} assets",
            display,
            self.executed,
            self.blocked,
            self.guarded,
            self.skipped,
            self.errors,
            self.asset_generated
        )
    }
}
