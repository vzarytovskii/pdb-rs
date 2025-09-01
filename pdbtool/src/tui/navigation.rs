use ratatui::widgets::{ListItem, ListState};
use std::path::Path;
use ms_pdb::Pdb;

/// Tree node identifiers for different PDB content types
#[derive(Clone, Debug, PartialEq)]
pub enum TreeNodeId {
    DebugInfo,
    Globals,
    Symbols,
    SymbolsGlobal,
    SymbolsPublic,
    SymbolsModules,
    SymbolsModule { index: usize },
    Types,
    Modules,
    Module { index: usize },
    SourceFiles,
    SourceFile { index: usize },
    Names,
    Streams,
    Stream { index: u32 },
}

/// Tree node structure for hierarchical navigation
#[derive(Clone)]
pub struct TreeNode {
    pub label: String,
    pub id: TreeNodeId,
    pub children: Vec<TreeNode>,
    pub is_expanded: bool,
    pub depth: usize,
}

impl TreeNode {
    pub fn new(label: &str, id: TreeNodeId) -> Self {
        Self {
            label: label.to_string(),
            id,
            children: Vec::new(),
            is_expanded: false,
            depth: 0,
        }
    }

    pub fn with_children(label: &str, id: TreeNodeId, children: Vec<TreeNode>) -> Self {
        let mut node = Self::new(label, id);
        node.children = children;
        node
    }

    pub fn toggle_expand(&mut self) {
        self.is_expanded = !self.is_expanded;
    }

    pub fn expand(&mut self) {
        self.is_expanded = true;
    }

    pub fn collapse(&mut self) {
        self.is_expanded = false;
    }

    pub fn collapse_all(&mut self) {
        self.is_expanded = false;
        for child in &mut self.children {
            child.collapse_all();
        }
    }

    pub fn expand_all(&mut self) {
        self.is_expanded = true;
        for child in &mut self.children {
            child.expand_all();
        }
    }

    /// Flatten the tree into a linear list for display
    pub fn flatten(&self, depth: usize) -> Vec<FlatTreeItem> {
        let mut result = Vec::new();
        let prefix = if self.children.is_empty() {
            "  "
        } else if self.is_expanded {
            "▼ "
        } else {
            "▶ "
        };

        let indent = "  ".repeat(depth);
        result.push(FlatTreeItem {
            display: format!("{}{}{}", indent, prefix, self.label),
            depth,
            has_children: !self.children.is_empty(),
            id: self.id.clone(),
            original_label: self.label.clone(),
        });

        if self.is_expanded {
            for child in &self.children {
                result.extend(child.flatten(depth + 1));
            }
        }

        result
    }
}

/// Flattened tree item for display purposes
#[derive(Clone, Debug)]
pub struct FlatTreeItem {
    pub display: String,
    pub depth: usize,
    pub has_children: bool,
    pub id: TreeNodeId,
    pub original_label: String,
}

/// Tree navigation state and operations
pub struct TreeNavigator {
    pub tree_nodes: Vec<TreeNode>,
    pub flat_items: Vec<FlatTreeItem>,
    pub list_state: ListState,
    pub search_query: String,
    pub search_mode: bool,
    pub filtered_indices: Vec<usize>,
}

impl TreeNavigator {
    pub fn new(pdb_file: &Path) -> Self {
        let tree_nodes = Self::build_pdb_tree(pdb_file);
        let flat_items = Self::flatten_tree(&tree_nodes);

        Self {
            tree_nodes,
            flat_items,
            list_state: ListState::default(),
            search_query: String::new(),
            search_mode: false,
            filtered_indices: Vec::new(),
        }
    }

    /// Build the complete PDB tree structure
    fn build_pdb_tree(pdb_file: &Path) -> Vec<TreeNode> {
        // Create stream children - for now create a reasonable number
        // In a real implementation, this would read the PDB to get actual stream count
        let mut stream_children = Vec::new();
        for i in 0..50 { // Assume max 50 streams for now
            stream_children.push(TreeNode::new(
                &format!("Stream #{}", i),
                TreeNodeId::Stream { index: i },
            ));
        }

        // Create symbol hierarchy structure
        let symbols_children = vec![
            TreeNode::new("Global Symbols", TreeNodeId::SymbolsGlobal),
            TreeNode::new("Public Symbols", TreeNodeId::SymbolsPublic),
            TreeNode::new("Module Symbols", TreeNodeId::SymbolsModules),
        ];

        let debug_info_children = vec![
            TreeNode::new("Modules", TreeNodeId::Modules),
            TreeNode::new("Names", TreeNodeId::Names),
            TreeNode::new("Globals", TreeNodeId::Globals),
            TreeNode::new("Types", TreeNodeId::Types),
            TreeNode::with_children("Symbols", TreeNodeId::Symbols, symbols_children),
            TreeNode::with_children("Streams", TreeNodeId::Streams, stream_children),
        ];

        let file_name = pdb_file.file_name().unwrap().to_str().unwrap();

        let mut nodes = vec![
            TreeNode::with_children(file_name, TreeNodeId::DebugInfo, debug_info_children),
        ];

        Self::set_depths(&mut nodes, 0);
        nodes
    }

    fn set_depths(nodes: &mut [TreeNode], depth: usize) {
        for node in nodes {
            node.depth = depth;
            Self::set_depths(&mut node.children, depth + 1);
        }
    }

    fn flatten_tree(nodes: &[TreeNode]) -> Vec<FlatTreeItem> {
        let mut result = Vec::new();
        for node in nodes {
            result.extend(node.flatten(0));
        }
        result
    }

    pub fn update_flat_items(&mut self) {
        self.flat_items = Self::flatten_tree(&self.tree_nodes);
        if self.search_mode && !self.search_query.is_empty() {
            self.update_search_filter();
        }
    }

    pub fn get_visible_items(&self) -> Vec<ListItem<'static>> {
        if self.search_mode && !self.filtered_indices.is_empty() {
            self.filtered_indices
                .iter()
                .filter_map(|&i| self.flat_items.get(i))
                .map(|item| ListItem::new(item.display.clone()))
                .collect()
        } else {
            self.flat_items
                .iter()
                .map(|item| ListItem::new(item.display.clone()))
                .collect()
        }
    }

    pub fn get_selected_node_id(&self) -> Option<TreeNodeId> {
        if let Some(selected) = self.list_state.selected() {
            if self.search_mode && !self.filtered_indices.is_empty() {
                self.filtered_indices
                    .get(selected)
                    .and_then(|&i| self.flat_items.get(i))
                    .map(|item| item.id.clone())
            } else {
                self.flat_items.get(selected).map(|item| item.id.clone())
            }
        } else {
            None
        }
    }

    pub fn move_up(&mut self) {
        let max_items = if self.search_mode && !self.filtered_indices.is_empty() {
            self.filtered_indices.len()
        } else {
            self.flat_items.len()
        };

        if max_items == 0 {
            return;
        }

        if let Some(selected) = self.list_state.selected() {
            if selected > 0 {
                self.list_state.select(Some(selected - 1));
            }
        } else {
            self.list_state.select(Some(0));
        }
    }

    pub fn move_down(&mut self) {
        let max_items = if self.search_mode && !self.filtered_indices.is_empty() {
            self.filtered_indices.len()
        } else {
            self.flat_items.len()
        };

        if max_items == 0 {
            return;
        }

        if let Some(selected) = self.list_state.selected() {
            if selected < max_items - 1 {
                self.list_state.select(Some(selected + 1));
            }
        } else {
            self.list_state.select(Some(0));
        }
    }

    pub fn toggle_selected_node(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let actual_index = if self.search_mode && !self.filtered_indices.is_empty() {
                if let Some(&actual_idx) = self.filtered_indices.get(selected) {
                    actual_idx
                } else {
                    return;
                }
            } else {
                selected
            };

            if actual_index >= self.flat_items.len() {
                return;
            }

            let has_children = self.flat_items[actual_index].has_children;
            if !has_children {
                return;
            }

            if let Some(path) = self.find_node_path(actual_index) {
                self.toggle_node_by_path(&path);
                self.update_flat_items();
            }
        }
    }

    pub fn expand_selected_node(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let actual_index = if self.search_mode && !self.filtered_indices.is_empty() {
                if let Some(&actual_idx) = self.filtered_indices.get(selected) {
                    actual_idx
                } else {
                    return;
                }
            } else {
                selected
            };

            if actual_index >= self.flat_items.len() {
                return;
            }

            let has_children = self.flat_items[actual_index].has_children;
            if !has_children {
                return;
            }

            if let Some(path) = self.find_node_path(actual_index) {
                self.expand_node_by_path(&path);
                self.update_flat_items();
            }
        }
    }

    pub fn collapse_or_navigate_to_parent(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let actual_index = if self.search_mode && !self.filtered_indices.is_empty() {
                if let Some(&actual_idx) = self.filtered_indices.get(selected) {
                    actual_idx
                } else {
                    return;
                }
            } else {
                selected
            };

            if actual_index >= self.flat_items.len() {
                return;
            }

            let current_item = &self.flat_items[actual_index];
            
            // If the current node has children and is expanded, collapse it
            if current_item.has_children {
                if let Some(path) = self.find_node_path(actual_index) {
                    if self.is_node_expanded_by_path(&path) {
                        self.collapse_node_by_path(&path);
                        self.update_flat_items();
                        return;
                    }
                }
            }
            
            // Otherwise, navigate to parent
            if current_item.depth > 0 {
                if let Some(parent_index) = self.find_parent_index(actual_index) {
                    // Update selection to reflect the new flat items structure
                    if self.search_mode && !self.filtered_indices.is_empty() {
                        // In search mode, find the parent in filtered results
                        if let Some(filtered_pos) = self.filtered_indices.iter().position(|&i| i == parent_index) {
                            self.list_state.select(Some(filtered_pos));
                        }
                    } else {
                        self.list_state.select(Some(parent_index));
                    }
                }
            }
        }
    }

    pub fn expand_all(&mut self) {
        for node in &mut self.tree_nodes {
            node.expand_all();
        }
        self.update_flat_items();
    }

    pub fn collapse_all(&mut self) {
        for node in &mut self.tree_nodes {
            node.collapse_all();
        }
        self.update_flat_items();
    }

    // Search functionality
    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.filtered_indices.clear();
    }

    pub fn exit_search_mode(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.filtered_indices.clear();
        self.list_state.select(None);
    }

    pub fn update_search_query(&mut self, query: String) {
        self.search_query = query;
        self.update_search_filter();
        // Reset selection when search changes
        self.list_state.select(if self.filtered_indices.is_empty() {
            None
        } else {
            Some(0)
        });
    }

    fn update_search_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_indices.clear();
            return;
        }

        let query_lower = self.search_query.to_lowercase();
        self.filtered_indices = self
            .flat_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.original_label.to_lowercase().contains(&query_lower)
                    || item.display.to_lowercase().contains(&query_lower)
            })
            .map(|(i, _)| i)
            .collect();
    }

    pub fn get_search_status(&self) -> String {
        if !self.search_mode {
            return String::new();
        }

        if self.search_query.is_empty() {
            "Search: (type to search)".to_string()
        } else {
            format!(
                "Search: {} ({} matches)",
                self.search_query,
                self.filtered_indices.len()
            )
        }
    }

    // Path finding and node manipulation
    fn find_node_path(&self, target_index: usize) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        Self::find_path_recursive(&self.tree_nodes, target_index, &mut 0, &mut path)
    }

    fn find_path_recursive(
        nodes: &[TreeNode],
        target_index: usize,
        current_index: &mut usize,
        path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        for (i, node) in nodes.iter().enumerate() {
            if *current_index == target_index {
                path.push(i);
                return Some(path.clone());
            }
            *current_index += 1;

            if node.is_expanded {
                path.push(i);
                if let Some(result) =
                    Self::find_path_recursive(&node.children, target_index, current_index, path)
                {
                    return Some(result);
                }
                path.pop();
            }
        }
        None
    }

    fn toggle_node_by_path(&mut self, path: &[usize]) {
        if path.is_empty() {
            return;
        }

        let mut current_nodes = &mut self.tree_nodes;
        for (i, &index) in path.iter().enumerate() {
            if index >= current_nodes.len() {
                return;
            }

            if i == path.len() - 1 {
                current_nodes[index].toggle_expand();
            } else {
                current_nodes = &mut current_nodes[index].children;
            }
        }
    }

    fn expand_node_by_path(&mut self, path: &[usize]) {
        if path.is_empty() {
            return;
        }

        let mut current_nodes = &mut self.tree_nodes;
        for (i, &index) in path.iter().enumerate() {
            if index >= current_nodes.len() {
                return;
            }

            if i == path.len() - 1 {
                current_nodes[index].expand();
            } else {
                current_nodes = &mut current_nodes[index].children;
            }
        }
    }

    fn collapse_node_by_path(&mut self, path: &[usize]) {
        if path.is_empty() {
            return;
        }

        let mut current_nodes = &mut self.tree_nodes;
        for (i, &index) in path.iter().enumerate() {
            if index >= current_nodes.len() {
                return;
            }

            if i == path.len() - 1 {
                current_nodes[index].collapse();
            } else {
                current_nodes = &mut current_nodes[index].children;
            }
        }
    }

    fn is_node_expanded_by_path(&self, path: &[usize]) -> bool {
        if path.is_empty() {
            return false;
        }

        let mut current_nodes = &self.tree_nodes;
        for (i, &index) in path.iter().enumerate() {
            if index >= current_nodes.len() {
                return false;
            }

            if i == path.len() - 1 {
                return current_nodes[index].is_expanded;
            } else {
                current_nodes = &current_nodes[index].children;
            }
        }
        false
    }

    fn find_parent_index(&self, child_index: usize) -> Option<usize> {
        if child_index >= self.flat_items.len() {
            return None;
        }

        let child_depth = self.flat_items[child_index].depth;
        if child_depth == 0 {
            return None; // Root level, no parent
        }

        // Look backwards for an item with depth one less than the child
        for i in (0..child_index).rev() {
            if self.flat_items[i].depth == child_depth - 1 {
                return Some(i);
            }
        }

        None
    }

    /// Update symbol modules with actual PDB module information
    pub fn update_symbols_from_pdb(&mut self, pdb: &Pdb) {
        // Find the "Symbols" node under the main PDB entry and update module symbols
        for root_node in &mut self.tree_nodes {
            if matches!(root_node.id, TreeNodeId::DebugInfo) {
                // Look for the Symbols node within the debug info children
                for symbols_node in &mut root_node.children {
                    if matches!(symbols_node.id, TreeNodeId::Symbols) {
                        // Look for the SymbolsModules node within the Symbols children
                        for symbols_child in &mut symbols_node.children {
                            if matches!(symbols_child.id, TreeNodeId::SymbolsModules) {
                                // Clear existing module children and rebuild with actual modules
                                symbols_child.children.clear();
                                
                                // Read DBI stream to get module information
                                if let Ok(dbi_stream) = pdb.read_dbi_stream() {
                                    for (module_index, module) in dbi_stream.iter_modules().enumerate() {
                                        let module_name = format!(
                                            "#{} - {}",
                                            module_index,
                                            module.module_name()
                                        );
                                        
                                        symbols_child.children.push(TreeNode::new(
                                            &module_name,
                                            TreeNodeId::SymbolsModule { index: module_index },
                                        ));
                                    }
                                }
                                
                                // Update depths for the new children
                                Self::set_depths(&mut symbols_child.children, symbols_child.depth + 1);
                                break;
                            }
                        }
                        break;
                    }
                }
                break;
            }
        }
        
        // Refresh the flat items after updating
        self.update_flat_items();
    }

    /// Update stream nodes with actual PDB stream information
    pub fn update_streams_from_pdb(&mut self, pdb: &Pdb) {
        let num_streams = pdb.num_streams();
        
        // Find the "Streams" node under the main PDB entry and update its children
        for root_node in &mut self.tree_nodes {
            if matches!(root_node.id, TreeNodeId::DebugInfo) {
                // Look for the Streams node within the debug info children
                for child_node in &mut root_node.children {
                    if matches!(child_node.id, TreeNodeId::Streams) {
                        // Clear existing stream children and rebuild with actual stream count
                        child_node.children.clear();
                        
                        // Create stream children based on actual PDB streams
                        for i in 0..num_streams {
                            let stream_name = if pdb.is_stream_valid(i) {
                                let size = pdb.stream_len(i);
                                if size > 1024 {
                                    format!("Stream #{} ({:.1} KB)", i, size as f64 / 1024.0)
                                } else {
                                    format!("Stream #{} ({} bytes)", i, size)
                                }
                            } else {
                                format!("Stream #{} (nil)", i)
                            };
                            
                            child_node.children.push(TreeNode::new(
                                &stream_name,
                                TreeNodeId::Stream { index: i },
                            ));
                        }
                        
                        // Update depths for the new children
                        Self::set_depths(&mut child_node.children, child_node.depth + 1);
                        break;
                    }
                }
                break;
            }
        }
        
        // Refresh the flat items after updating
        self.update_flat_items();
    }
}
