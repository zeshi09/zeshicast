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
                    preview: crate::search::clipboard::clipboard_preview(&r.content),
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
                    preview: crate::search::clipboard::clipboard_preview(&snippet.value),
                    value: snippet.value.clone(),
                    tags: snippet.tags.clone(),
                })
                .collect()
        }
    }

    pub fn delete_snippet(&mut self, name: &str, value: &str) -> io::Result<()> {
        storage::snippet_delete_by_title_and_content(&self.config_dir, name, value)
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.snippets
            .retain(|snippet| snippet.name != name || snippet.value != value);
        self.write_snippets()
    }

    pub fn delete_snippet_by_id(&mut self, id: i64) -> io::Result<()> {
        storage::snippet_delete(&self.config_dir, id)
            .map_err(|error| io::Error::other(error.to_string()))?;
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
        storage::snippet_insert(&self.config_dir, name, "", value, &["ai".to_string()])
            .map_err(|error| io::Error::other(error.to_string()))?;
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
            storage::snippet_update(&self.config_dir, id, title, prefix, content, tags)
                .map_err(|error| io::Error::other(error.to_string()))?;
        } else {
            storage::snippet_insert(&self.config_dir, title, prefix, content, tags)
                .map_err(|error| io::Error::other(error.to_string()))?;
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
                // The file format is line- and delimiter-based: strip newlines
                // so a pasted value cannot break it (M-13).
                let name = sanitize_snippet_field(&snippet.name);
                let value = sanitize_snippet_field(&snippet.value);
                if snippet.tags.is_empty() {
                    format!("{name} = {value}")
                } else {
                    let tags = sanitize_snippet_field(&snippet.tags.join(", "));
                    format!("{name} | {tags} = {value}")
                }
            })
            .collect::<Vec<_>>();
        write_lines(&self.config_dir.join("snippets.txt"), &lines)
    }
}

/// Keep a snippet field on one line: `snippets.txt` is parsed line by line.
fn sanitize_snippet_field(field: &str) -> String {
    field.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_fields_never_break_the_line_format() {
        assert_eq!(sanitize_snippet_field("one\ntwo\rthree"), "one two three");
        assert_eq!(sanitize_snippet_field("plain"), "plain");
    }
}
