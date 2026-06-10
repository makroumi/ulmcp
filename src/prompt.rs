//! Prompt templates.

#[derive(Debug, Clone)]
pub struct PromptDef {
    pub name: String,
    pub description: String,
    pub template: String,
    pub params: Vec<PromptParam>,
}

#[derive(Debug, Clone)]
pub struct PromptParam {
    pub name: String,
    pub description: String,
    pub required: bool,
}

impl PromptDef {
    pub fn new(name: impl Into<String>, template: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            template: template.into(),
            params: Vec::new(),
        }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }

    pub fn param(
        mut self,
        name: impl Into<String>,
        desc: impl Into<String>,
        required: bool,
    ) -> Self {
        self.params.push(PromptParam {
            name: name.into(),
            description: desc.into(),
            required,
        });
        self
    }

    /// Render the template with the given arguments.
    pub fn render(
        &self,
        args: &std::collections::HashMap<String, String>,
    ) -> Result<String, String> {
        let mut output = self.template.clone();
        for param in &self.params {
            let placeholder = format!("{{{{{}}}}}", param.name);
            if let Some(value) = args.get(&param.name) {
                output = output.replace(&placeholder, value);
            } else if param.required {
                return Err(format!("missing required parameter: {:?}", param.name));
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn render_basic() {
        let p = PromptDef::new("greet", "Hello {{name}}, welcome to {{project}}!")
            .param("name", "User name", true)
            .param("project", "Project name", true);

        let mut args = HashMap::new();
        args.insert("name".into(), "Alice".into());
        args.insert("project".into(), "uldb".into());

        let rendered = p.render(&args).unwrap();
        assert_eq!(rendered, "Hello Alice, welcome to uldb!");
    }

    #[test]
    fn render_missing_required() {
        let p = PromptDef::new("greet", "Hello {{name}}").param("name", "User name", true);

        let args = HashMap::new();
        assert!(p.render(&args).is_err());
    }

    #[test]
    fn render_optional_missing() {
        let p = PromptDef::new("greet", "Hello {{name}} from {{city}}")
            .param("name", "User name", true)
            .param("city", "City", false);

        let mut args = HashMap::new();
        args.insert("name".into(), "Bob".into());

        // Optional param not provided: placeholder remains
        let rendered = p.render(&args).unwrap();
        assert!(rendered.contains("Bob"));
    }
}
