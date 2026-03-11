use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Events emitted by the file watcher
#[derive(Debug, Clone)]
pub enum FileEvent {
    /// A markdown file was created or modified
    Changed(PathBuf),
    /// A markdown file was deleted
    Deleted(PathBuf),
}

/// File watcher for monitoring vault changes
pub struct VaultWatcher {
    _debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
    receiver: mpsc::UnboundedReceiver<FileEvent>,
}

impl VaultWatcher {
    /// Create a new vault watcher
    pub fn new<P: AsRef<Path>>(vault_path: P) -> Result<Self> {
        let vault_path = vault_path.as_ref().to_path_buf();
        let (tx, rx) = mpsc::unbounded_channel();

        // Create debounced watcher with 2 second delay
        let tx_clone = tx.clone();
        let mut debouncer = new_debouncer(
            Duration::from_secs(2),
            None,
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    for event in events {
                        if let Err(e) = Self::handle_event(&tx_clone, event.event) {
                            error!("Error handling file event: {}", e);
                        }
                    }
                }
                Err(errors) => {
                    for error in errors {
                        error!("File watcher error: {}", error);
                    }
                }
            },
        )
        .context("Failed to create file watcher")?;

        // Start watching the vault directory
        debouncer
            .watcher()
            .watch(&vault_path, RecursiveMode::Recursive)
            .context("Failed to watch vault directory")?;

        info!("File watcher started for: {}", vault_path.display());

        Ok(Self {
            _debouncer: debouncer,
            receiver: rx,
        })
    }

    /// Handle a file system event
    fn handle_event(tx: &mpsc::UnboundedSender<FileEvent>, event: Event) -> Result<()> {
        // Only process markdown files
        let paths: Vec<PathBuf> = event
            .paths
            .iter()
            .filter(|p| Self::is_markdown_file(p))
            .cloned()
            .collect();

        if paths.is_empty() {
            return Ok(());
        }

        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in paths {
                    debug!("File changed: {}", path.display());
                    tx.send(FileEvent::Changed(path))
                        .context("Failed to send file event")?;
                }
            }
            EventKind::Remove(_) => {
                for path in paths {
                    debug!("File deleted: {}", path.display());
                    tx.send(FileEvent::Deleted(path))
                        .context("Failed to send file event")?;
                }
            }
            _ => {
                // Ignore other event types (access, etc.)
            }
        }

        Ok(())
    }

    /// Check if a path is a markdown file
    fn is_markdown_file(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        if let Some(ext) = path.extension() {
            ext == "md" || ext == "markdown"
        } else {
            false
        }
    }

    /// Get the next file event (async)
    pub async fn next_event(&mut self) -> Option<FileEvent> {
        self.receiver.recv().await
    }

    /// Try to get the next event without blocking
    pub fn try_next_event(&mut self) -> Option<FileEvent> {
        self.receiver.try_recv().ok()
    }
}

/// Scan a directory for all markdown files
pub fn scan_vault<P: AsRef<Path>>(vault_path: P) -> Result<Vec<PathBuf>> {
    let vault_path = vault_path.as_ref();
    let mut markdown_files = Vec::new();

    for entry in walkdir::WalkDir::new(vault_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Prune hidden directories (e.g. .trash, .obsidian) from traversal entirely
            !e.file_name()
                .to_str()
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Check if it's a markdown file
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" || ext == "markdown" {
                    markdown_files.push(path.to_path_buf());
                }
            }
        }
    }

    info!("Found {} markdown files in vault", markdown_files.len());
    Ok(markdown_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_vault() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path();

        // Create some markdown files
        fs::write(vault_path.join("note1.md"), "# Note 1").unwrap();
        fs::write(vault_path.join("note2.md"), "# Note 2").unwrap();

        // Create a subdirectory with a markdown file
        let subdir = vault_path.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("note3.md"), "# Note 3").unwrap();

        // Create a hidden file (should be ignored)
        fs::write(vault_path.join(".hidden.md"), "# Hidden").unwrap();

        // Create a non-markdown file (should be ignored)
        fs::write(vault_path.join("readme.txt"), "Readme").unwrap();

        let files = scan_vault(vault_path).unwrap();

        // Should find 3 markdown files (excluding hidden)
        assert_eq!(files.len(), 3);

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert!(file_names.contains(&"note1.md".to_string()));
        assert!(file_names.contains(&"note2.md".to_string()));
        assert!(file_names.contains(&"note3.md".to_string()));
        assert!(!file_names.contains(&".hidden.md".to_string()));
    }

    #[tokio::test]
    async fn test_watcher_creation() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path();

        let watcher = VaultWatcher::new(vault_path);
        assert!(watcher.is_ok());
    }
}
