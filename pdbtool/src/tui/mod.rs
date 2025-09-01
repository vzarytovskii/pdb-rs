//! Interactive TUI mode for exploring PDB files
//!
//! This module provides a terminal-based user interface built with ratatui
//! for interactively exploring PDB file contents. The TUI features:
//! - Left pane: Tree view of PDB structure (streams, modules, symbols, etc.)
//! - Right pane: Details panel showing selected item information
//! - Lazy loading of data as user navigates
//! - Search and filtering capabilities
//! - Enhanced navigation with keyboard shortcuts

use anyhow::Result;
use std::path::Path;

mod app;
mod content;
mod navigation;
mod widgets;

pub use app::TuiApp;

pub fn run_tui(pdb_file_name: String) -> Result<()> {
    let pdb_file = Path::new(&pdb_file_name);

    let pdb = Box::new(ms_pdb::Pdb::open(pdb_file)?);

    let mut app = TuiApp::new(*pdb, pdb_file)?;
    app.run()
}
