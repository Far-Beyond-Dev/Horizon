/// Path-based event routing system for efficient hierarchical event lookups
use crate::events::EventHandler;
use std::collections::HashMap;
use std::sync::Arc;
use compact_str::CompactString;

/// A node in the event path tree
#[derive(Debug, Default)]
pub struct PathNode {
    /// Handlers registered at this exact path
    pub handlers: Vec<Arc<dyn EventHandler>>,
    /// Child nodes for deeper paths
    pub children: HashMap<CompactString, PathNode>,
}

/// Path-based event router that treats event keys as hierarchical paths
/// 
/// Instead of flat key lookup, this treats `core::player::connected` as:
/// - root -> "core" -> "player" -> "connected"
/// 
/// This enables:
/// 1. Faster lookups by following the path tree
/// 2. Efficient similarity searches for debugging
/// 3. Wildcard/pattern matching (future feature)
/// 4. Better namespace organization
#[derive(Debug, Default)]
pub struct PathRouter {
    root: PathNode,
}

impl PathRouter {
    /// Create a new path router
    pub fn new() -> Self {
        Self {
            root: PathNode::default(),
        }
    }

    /// Register a handler at the specified path
    pub fn register_handler(&mut self, path: &str, handler: Arc<dyn EventHandler>) {
        let parts: Vec<&str> = path.split(':').collect();
        let mut current = &mut self.root;
        // Navigate/create the path
        for part in parts {
            let key = CompactString::new(part);
            current = current.children.entry(key).or_default();
        }
        // Add handler at the final node
        current.handlers.push(handler);
    }

    /// Find handlers for the given path
    pub fn find_handlers(&self, path: &str) -> Option<&Vec<Arc<dyn EventHandler>>> {
        let parts: Vec<&str> = path.split(':').collect();
        let mut current = &self.root;
        // Navigate the path
        for part in parts {
            let key = CompactString::new(part);
            match current.children.get(&key) {
                Some(node) => current = node,
                None => return None,
            }
        }
        // Return handlers if any exist at this path
        if current.handlers.is_empty() {
            None
        } else {
            Some(&current.handlers)
        }
    }

    /// Find similar paths for debugging (when exact match fails)
    /// 
    /// This is much more efficient than scanning all keys since we can
    /// traverse only relevant branches of the tree.
    pub fn find_similar_paths(&self, target_path: &str, max_results: usize) -> Vec<String> {
        let target_parts: Vec<&str> = target_path.split(':').collect();
        let mut results = Vec::new();
        
        self.collect_similar_paths("", &self.root, &target_parts, 0, &mut results, max_results);
        results
    }

    /// Recursively collect paths that share common prefixes or components
    fn collect_similar_paths(
        &self,
        current_path: &str,
        node: &PathNode,
        target_parts: &[&str],
        depth: usize,
        results: &mut Vec<String>,
        max_results: usize,
    ) {
        if results.len() >= max_results {
            return;
        }

        // If this node has handlers, check if it's similar
        if !node.handlers.is_empty() && !current_path.is_empty() {
            let current_parts: Vec<&str> = current_path.split(':').collect();
            
            // Calculate similarity score based on shared components
            let similarity = self.calculate_similarity(&current_parts, target_parts);
            
            // Include if it shares at least one component or has similar length
            if similarity > 0.0 {
                results.push(current_path.to_string());
            }
        }

        // Continue traversing, prioritizing paths that match target components
        for (child_key, child_node) in &node.children {
            let child_path = if current_path.is_empty() {
                child_key.to_string()
            } else {
                format!("{}:{}", current_path, child_key)
            };

            // Priority traversal: paths that match current depth component first
            let has_matching_component = depth < target_parts.len() && 
                target_parts[depth] == child_key.as_str();
                
            if has_matching_component {
                self.collect_similar_paths(&child_path, child_node, target_parts, depth + 1, results, max_results);
            }
        }

        // Then traverse non-matching paths
        for (child_key, child_node) in &node.children {
            if results.len() >= max_results {
                break;
            }
            
            let has_matching_component = depth < target_parts.len() && 
                target_parts[depth] == child_key.as_str();
                
            if !has_matching_component {
                let child_path = if current_path.is_empty() {
                    child_key.to_string()
                } else {
                    format!("{}:{}", current_path, child_key)
                };
                self.collect_similar_paths(&child_path, child_node, target_parts, depth + 1, results, max_results);
            }
        }
    }

    /// Calculate similarity between two path component arrays
    fn calculate_similarity(&self, path1: &[&str], path2: &[&str]) -> f32 {
        let mut matches = 0;
        let max_len = path1.len().max(path2.len());
        
        if max_len == 0 {
            return 0.0;
        }
        
        // Count matching components at any position
        for component1 in path1 {
            for component2 in path2 {
                if component1 == component2 {
                    matches += 1;
                    break; // Only count each match once
                }
            }
        }
        
        matches as f32 / max_len as f32
    }

    /// Get total number of registered handlers across all paths
    pub fn total_handlers(&self) -> usize {
        self.count_handlers(&self.root)
    }

    /// Recursively count handlers in the tree
    fn count_handlers(&self, node: &PathNode) -> usize {
        let mut count = node.handlers.len();
        for child in node.children.values() {
            count += self.count_handlers(child);
        }
        count
    }

    /// Get all registered paths (for debugging/stats)
    pub fn get_all_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        self.collect_all_paths("", &self.root, &mut paths);
        paths
    }

    /// Recursively collect all paths with handlers
    fn collect_all_paths(&self, current_path: &str, node: &PathNode, paths: &mut Vec<String>) {
        if !node.handlers.is_empty() && !current_path.is_empty() {
            paths.push(current_path.to_string());
        }

        for (child_key, child_node) in &node.children {
            let child_path = if current_path.is_empty() {
                child_key.to_string()
            } else {
                format!("{}:{}", current_path, child_key)
            };
            self.collect_all_paths(&child_path, child_node, paths);
        }
    }

    /// Check if a path exists (has handlers)
    pub fn path_exists(&self, path: &str) -> bool {
        self.find_handlers(path).is_some()
    }

    /// Get handler count for a specific path
    pub fn handler_count_for_path(&self, path: &str) -> usize {
        self.find_handlers(path).map(|handlers| handlers.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventHandler;
    use crate::events::EventError;
    use std::sync::Arc;
    use std::any::TypeId;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockHandler {
        name: String,
    }

    #[async_trait]
    impl EventHandler for MockHandler {
        async fn handle(&self, _data: &[u8]) -> Result<(), EventError> {
            Ok(())
        }

        fn handler_name(&self) -> &str {
            &self.name
        }

        fn expected_type_id(&self) -> TypeId {
            TypeId::of::<()>()
        }
    }

    #[test]
    fn test_basic_registration_and_lookup() {
        let mut router = PathRouter::new();
        let handler = Arc::new(MockHandler { name: "test".to_string() });
        
        router.register_handler("core:player:connected", handler);
        
        assert!(router.path_exists("core:player:connected"));
        assert_eq!(router.handler_count_for_path("core:player:connected"), 1);
        assert!(!router.path_exists("core:player:disconnected"));
    }

    #[test]
    fn test_hierarchical_paths() {
        let mut router = PathRouter::new();
        
        router.register_handler("core:player:connected", Arc::new(MockHandler { name: "connect".to_string() }));
        router.register_handler("core:player:disconnected", Arc::new(MockHandler { name: "disconnect".to_string() }));
        router.register_handler("core:server:started", Arc::new(MockHandler { name: "start".to_string() }));
        
        assert!(router.path_exists("core:player:connected"));
        assert!(router.path_exists("core:player:disconnected"));
        assert!(router.path_exists("core:server:started"));
        assert!(!router.path_exists("core:player"));
        assert!(!router.path_exists("client:player:connected"));
    }

    #[test]
    fn test_similar_path_finding() {
        let mut router = PathRouter::new();
        
        router.register_handler("gorc_instance:GorcPlayer:0:move", Arc::new(MockHandler { name: "instance_move".to_string() }));
        router.register_handler("gorc_instance:GorcPlayer:2:chat", Arc::new(MockHandler { name: "instance_chat".to_string() }));
        
        let similar = router.find_similar_paths("gorc:GorcPlayer:0:move", 10);
        
        assert!(!similar.is_empty());
        assert!(similar.contains(&"gorc_instance:GorcPlayer:0:move".to_string()));
    }

    #[test]
    fn test_multiple_handlers_per_path() {
        let mut router = PathRouter::new();
        
        router.register_handler("core:tick", Arc::new(MockHandler { name: "handler1".to_string() }));
        router.register_handler("core:tick", Arc::new(MockHandler { name: "handler2".to_string() }));
        
        assert_eq!(router.handler_count_for_path("core:tick"), 2);
        assert_eq!(router.total_handlers(), 2);
    }
}