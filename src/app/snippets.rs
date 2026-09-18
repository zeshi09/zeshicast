//! Snippets: reading them out of the store and writing changes back (P5.3).

use super::*;

impl Zeshicast {
    pub fn list_snippets(&self) -> Vec<SnippetSummary> {
        if storage::snippet_has_data(&self.config_dir) {
            storage::snippets_load(&self.config_dir)
                .into_iter()
                .map(|r| SnippetSummary {
                    id: r.id,
                    name: r.title,
                    prefix: r.prefix,
                    preview: crate::clipboard_preview(&r.content),
                    value: r.content,
                    tags: r.tags,
                })
                .collect()
        } else {
            self.snippets
                .iter()
                .enumerate()
                .map(|(i, snippet)| SnippetSummary {
                    id: (i + 1) as i64,
                    name: snippet.name.clone(),
                    prefix: String::new(),
                    preview: crate::clipboard_preview(&snippet.value),
                    value: snippet.value.clone(),
                    tags: snippet.tags.clone(),
                })
                .collect()
        }
    }

    pub fn delete_snippet(&mut self, name: &str, value: &str) -> io::Result<()> {
        storage::snippet_delete_by_title_and_content(&self.config_dir, name, value).ok();
        self.snippets
            .retain(|snippet| snippet.name != name || snippet.value != value);
        self.write_snippets()
    }

    pub fn delete_snippet_by_id(&mut self, id: i64) -> io::Result<()> {
        storage::snippet_delete(&self.config_dir, id).ok();
        self.reload_snippets();
        self.write_snippets()
    }

    pub fn add_snippet(&mut self, name: &str, value: &str) -> io::Result<()> {
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snippet name and value are required",
            ));
        }
        storage::snippet_insert(&self.config_dir, name, "", value, &["ai".to_string()]).ok();
        self.snippets.push(NamedValue {
            name: name.to_string(),
            value: value.to_string(),
            tags: vec!["ai".to_string()],
        });
        self.write_snippets()
    }

    pub fn save_snippet(
        &mut self,
        id: Option<i64>,
        title: &str,
        prefix: &str,
        content: &str,
        tags: &[String],
    ) -> io::Result<()> {
        let title = title.trim();
        let content = content.trim();
        if title.is_empty() || content.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snippet title and content are required",
            ));
        }

        if let Some(id) = id {
            storage::snippet_update(&self.config_dir, id, title, prefix, content, tags).ok();
        } else {
            storage::snippet_insert(&self.config_dir, title, prefix, content, tags).ok();
        }

        self.reload_snippets();
        self.write_snippets()
    }

    fn reload_snippets(&mut self) {
        if storage::snippet_has_data(&self.config_dir) {
            self.snippets = storage::snippets_load(&self.config_dir)
                .into_iter()
                .map(|r| NamedValue {
                    name: r.title,
                    value: r.content,
                    tags: r.tags,
                })
                .collect();
        }
    }

    fn write_snippets(&self) -> io::Result<()> {
        let lines = self
            .snippets
            .iter()
            .map(|snippet| {
                if snippet.tags.is_empty() {
                    format!("{} = {}", snippet.name, snippet.value)
                } else {
                    format!(
                        "{} | {} = {}",
                        snippet.name,
                        snippet.tags.join(", "),
                        snippet.value
                    )
                }
            })
            .collect::<Vec<_>>();
        write_lines(&self.config_dir.join("snippets.txt"), &lines)
    }
}
