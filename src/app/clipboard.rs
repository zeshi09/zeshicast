//! Clipboard history: recording text and images, retention, and pruning the
//! image cache (P5.3).

use super::*;

impl Zeshicast {
    pub fn add_clipboard_text(&mut self, text: &str) -> io::Result<bool> {
        if !self.clipboard_history_enabled() || self.clipboard_private_mode() {
            return Ok(false);
        }
        // Incoming text is sanitized here: pasted text must never be able to
        // claim to be an image entry (M-15).
        let text = crate::normalize_clipboard_text(text);
        if text.is_empty() {
            return Ok(false);
        }
        self.store_clipboard_entry(text)
    }

    /// Retention/insertion tail shared by text and image entries. The caller is
    /// responsible for sanitizing untrusted input; the image sentinel is added
    /// by [`Self::add_clipboard_image`] after sanitization, not parsed from it.
    fn store_clipboard_entry(&mut self, text: String) -> io::Result<bool> {
        let retention = self.clipboard_retention();
        storage::clipboard_insert_with_limit(&self.config_dir, &text, retention)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_timestamps
            .entry(text.clone())
            .or_insert_with(crate::unix_now);
        self.clipboard_history.retain(|e| e != &text);
        self.clipboard_history.insert(0, text);
        self.clipboard_history.truncate(retention);
        prune_clipboard_image_cache(&self.clipboard_history)?;
        Ok(true)
    }

    pub fn delete_clipboard_item(&mut self, action: &Action) -> io::Result<()> {
        self.delete_clipboard_value(&action.value())
    }

    pub fn delete_clipboard_value(&mut self, value: &str) -> io::Result<()> {
        storage::clipboard_delete(&self.config_dir, value)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_history.retain(|e| e != value);
        self.clipboard_timestamps.remove(value);
        prune_clipboard_image_cache(&self.clipboard_history)?;
        Ok(())
    }

    pub fn clear_clipboard_history(&mut self) -> io::Result<()> {
        storage::clipboard_clear(&self.config_dir).map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_history.clear();
        self.clipboard_timestamps.clear();
        clear_clipboard_image_cache()?;
        Ok(())
    }

    pub fn list_clipboard_history(&self) -> Vec<ClipboardSummary> {
        self.clipboard_history
            .iter()
            .map(|entry| {
                let kind = classify_clipboard_text(entry);
                let preview = if kind == ClipboardKind::Image {
                    "Image".to_string()
                } else {
                    crate::clipboard_preview(entry)
                };
                ClipboardSummary {
                    preview,
                    value: entry.clone(),
                    kind,
                    size_bytes: entry.len(),
                    timestamp: self.clipboard_timestamps.get(entry).copied(),
                }
            })
            .collect()
    }

    /// Record a captured image (already cached as a PNG at `path`).
    pub fn add_clipboard_image(&mut self, path: &str) -> io::Result<bool> {
        if !self.clipboard_capture_images() {
            return Ok(false);
        }
        // The entry is built here rather than sanitized from input, so the
        // sentinel survives and the cached PNG is recognised (and kept by the
        // pruner) instead of degrading into a text entry.
        self.store_clipboard_entry(format!("{CLIPBOARD_IMAGE_PREFIX}{path}"))
    }

    pub(crate) fn clipboard_retention(&self) -> usize {
        clipboard_retention_value(&self.preferences)
    }

    pub(crate) fn enforce_clipboard_retention(&mut self) -> io::Result<()> {
        let retention = self.clipboard_retention();
        storage::clipboard_prune(&self.config_dir, retention)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_history.truncate(retention);
        prune_clipboard_image_cache(&self.clipboard_history)
    }
}
