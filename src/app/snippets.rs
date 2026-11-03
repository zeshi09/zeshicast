//! Snippets: reading them out of the store and writing changes back (P5.3).

use super::*;

impl Zeshicast {
    pub fn list_snippets(&self) -> Vec<SnippetSummary> {
        if storage::snippet_has_data(&self.config_dir) {
            storage::snippets_load(&self.config_dir)
                .unwrap_or_else(|error| {
                    log::error!("{error}");
                    Vec::new()
                })
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
        Ok(())
    }

    pub fn delete_snippet_by_id(&mut self, id: i64) -> io::Result<()> {
        storage::snippet_delete(&self.config_dir, id)
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.reload_snippets();
        Ok(())
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
        Ok(())
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
        Ok(())
    }

    fn reload_snippets(&mut self) {
        if storage::snippet_has_data(&self.config_dir) {
            match storage::snippets_load(&self.config_dir) {
                Ok(records) => {
                    self.snippets = records
                        .into_iter()
                        .map(|r| NamedValue {
                            name: r.title,
                            value: r.content,
                            tags: r.tags,
                        })
                        .collect();
                }
                Err(error) => log::error!("{error}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_are_not_mirrored_to_a_plaintext_file() {
        // Q6.3: the database is the only copy, and it is encrypted.
        let dir = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_app(dir.path().to_path_buf(), HashMap::new());
        app.save_snippet(None, "Bank", ":pin", "SECRET 4821", &[])
            .unwrap();
        app.add_snippet("Other", "value").unwrap();
        assert!(!dir.path().join("snippets.txt").exists());
        assert_eq!(app.list_snippets().len(), 2);
    }
}
