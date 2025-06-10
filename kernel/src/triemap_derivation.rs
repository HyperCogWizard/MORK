// Triemap Derivation Implementation
// Generalizes triemap structure over algebraic data types

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::hash::Hash;

/// Generic triemap trait that can be derived for algebraic data types
pub trait TrieMap<K, V> {
    fn new() -> Self;
    fn insert(&mut self, key: K, value: V) -> Option<V>;
    fn get(&self, key: &K) -> Option<&V>;
    fn remove(&mut self, key: &K) -> Option<V>;
    fn contains_key(&self, key: &K) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    
    // Relational algebra operations
    fn union(&self, other: &Self) -> Self where V: Clone;
    fn intersection(&self, other: &Self) -> Self where V: Clone;
    fn difference(&self, other: &Self) -> Self where V: Clone;
}

/// Bytes-based triemap implementation
#[derive(Debug, Clone)]
pub struct BytesTrieMap<V> {
    root: TrieNode<V>,
}

#[derive(Debug, Clone)]
struct TrieNode<V> {
    value: Option<V>,
    children: BTreeMap<u8, TrieNode<V>>,
}

impl<V> TrieNode<V> {
    fn new() -> Self {
        Self {
            value: None,
            children: BTreeMap::new(),
        }
    }
}

impl<V> TrieMap<&[u8], V> for BytesTrieMap<V> {
    fn new() -> Self {
        Self {
            root: TrieNode::new(),
        }
    }
    
    fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        let mut node = &mut self.root;
        for &byte in key {
            node = node.children.entry(byte).or_insert_with(TrieNode::new);
        }
        node.value.replace(value)
    }
    
    fn get(&self, key: &&[u8]) -> Option<&V> {
        let mut node = &self.root;
        for &byte in *key {
            node = node.children.get(&byte)?;
        }
        node.value.as_ref()
    }
    
    fn remove(&mut self, key: &&[u8]) -> Option<V> {
        self.remove_recursive(&mut self.root, key, 0)
    }
    
    fn contains_key(&self, key: &&[u8]) -> bool {
        self.get(key).is_some()
    }
    
    fn len(&self) -> usize {
        self.count_values(&self.root)
    }
    
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Union operation: combines two triemaps
    fn union(&self, other: &Self) -> Self where V: Clone {
        let mut result = self.clone();
        result.union_with(other);
        result
    }
    
    /// Intersection operation: keeps only common keys
    fn intersection(&self, other: &Self) -> Self where V: Clone {
        let mut result = Self::new();
        self.intersection_recursive(&self.root, &other.root, &mut result.root, &[]);
        result
    }
    
    /// Difference operation: removes keys present in other
    fn difference(&self, other: &Self) -> Self where V: Clone {
        let mut result = self.clone();
        result.difference_with(other);
        result
    }
}

impl<V> BytesTrieMap<V> {
    /// Insert with owned key
    pub fn insert_owned(&mut self, key: Vec<u8>, value: V) -> Option<V> {
        self.insert(&key, value)
    }
    
    /// Get with owned key
    pub fn get_owned(&self, key: &Vec<u8>) -> Option<&V> {
        self.get(&key.as_slice())
    }
    
    fn remove_recursive(&mut self, node: &mut TrieNode<V>, key: &&[u8], depth: usize) -> Option<V> {
        if depth == key.len() {
            return node.value.take();
        }
        
        let byte = key[depth];
        if let Some(child) = node.children.get_mut(&byte) {
            let result = self.remove_recursive(child, key, depth + 1);
            
            // Clean up empty nodes
            if child.value.is_none() && child.children.is_empty() {
                node.children.remove(&byte);
            }
            
            result
        } else {
            None
        }
    }
    
    fn count_values(&self, node: &TrieNode<V>) -> usize {
        let mut count = if node.value.is_some() { 1 } else { 0 };
        for child in node.children.values() {
            count += self.count_values(child);
        }
        count
    }
    
    fn union_with(&mut self, other: &Self) where V: Clone {
        self.union_recursive(&mut self.root, &other.root);
    }
    
    fn union_recursive(&mut self, node: &mut TrieNode<V>, other_node: &TrieNode<V>) where V: Clone {
        // If other node has a value and current doesn't, take it
        if node.value.is_none() && other_node.value.is_some() {
            node.value = other_node.value.clone();
        }
        
        // Recursively union children
        for (&byte, other_child) in &other_node.children {
            let child = node.children.entry(byte).or_insert_with(TrieNode::new);
            self.union_recursive(child, other_child);
        }
    }
    
    fn intersection_recursive(&self, node1: &TrieNode<V>, node2: &TrieNode<V>, result: &mut TrieNode<V>, _path: &[u8]) where V: Clone {
        // Include value only if both nodes have it
        if node1.value.is_some() && node2.value.is_some() {
            result.value = node1.value.clone();
        }
        
        // Recursively intersect children
        for (&byte, child1) in &node1.children {
            if let Some(child2) = node2.children.get(&byte) {
                let result_child = result.children.entry(byte).or_insert_with(TrieNode::new);
                self.intersection_recursive(child1, child2, result_child, _path);
            }
        }
    }
    
    fn difference_with(&mut self, other: &Self) where V: Clone {
        self.difference_recursive(&mut self.root, &other.root);
    }
    
    fn difference_recursive(&mut self, node: &mut TrieNode<V>, other_node: &TrieNode<V>) where V: Clone {
        // Remove value if it exists in other
        if other_node.value.is_some() {
            node.value = None;
        }
        
        // Recursively difference children
        for (&byte, child) in &mut node.children {
            if let Some(other_child) = other_node.children.get(&byte) {
                self.difference_recursive(child, other_child);
            }
        }
        
        // Clean up empty children
        node.children.retain(|_, child| child.value.is_some() || !child.children.is_empty());
    }
    
    /// Iterator over all key-value pairs
    pub fn iter(&self) -> TrieMapIterator<V> {
        TrieMapIterator::new(&self.root)
    }
}

/// Iterator for TrieMap
pub struct TrieMapIterator<'a, V> {
    stack: Vec<(Vec<u8>, &'a TrieNode<V>)>,
}

impl<'a, V> TrieMapIterator<'a, V> {
    fn new(root: &'a TrieNode<V>) -> Self {
        let mut iter = Self {
            stack: Vec::new(),
        };
        iter.stack.push((Vec::new(), root));
        iter
    }
}

impl<'a, V> Iterator for TrieMapIterator<'a, V> {
    type Item = (Vec<u8>, &'a V);
    
    fn next(&mut self) -> Option<Self::Item> {
        while let Some((path, node)) = self.stack.pop() {
            // Add children to stack in reverse order for lexicographic iteration
            for (&byte, child) in node.children.iter().rev() {
                let mut child_path = path.clone();
                child_path.push(byte);
                self.stack.push((child_path, child));
            }
            
            // Return value if present
            if let Some(value) = &node.value {
                return Some((path, value));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_operations() {
        let mut trie = BytesTrieMap::new();
        
        // Insert
        assert_eq!(trie.insert(b"hello", 1), None);
        assert_eq!(trie.insert(b"world", 2), None);
        assert_eq!(trie.insert(b"hello", 3), Some(1)); // Replace
        
        // Get
        assert_eq!(trie.get(&b"hello"[..]), Some(&3));
        assert_eq!(trie.get(&b"world"[..]), Some(&2));
        assert_eq!(trie.get(&b"missing"[..]), None);
        
        // Contains
        assert!(trie.contains_key(&b"hello"[..]));
        assert!(!trie.contains_key(&b"missing"[..]));
        
        // Length
        assert_eq!(trie.len(), 2);
        assert!(!trie.is_empty());
        
        // Remove
        assert_eq!(trie.remove(&b"hello"[..]), Some(3));
        assert_eq!(trie.remove(&b"hello"[..]), None); // Already removed
        assert_eq!(trie.len(), 1);
    }
    
    #[test]
    fn test_relational_operations() {
        let mut trie1 = BytesTrieMap::new();
        trie1.insert(b"a", 1);
        trie1.insert(b"b", 2);
        trie1.insert(b"c", 3);
        
        let mut trie2 = BytesTrieMap::new();
        trie2.insert(b"b", 20);
        trie2.insert(b"c", 30);
        trie2.insert(b"d", 40);
        
        // Union
        let union = trie1.union(&trie2);
        assert_eq!(union.len(), 4);
        assert_eq!(union.get(&b"a"[..]), Some(&1));
        assert_eq!(union.get(&b"b"[..]), Some(&2)); // From trie1
        assert_eq!(union.get(&b"c"[..]), Some(&3)); // From trie1
        assert_eq!(union.get(&b"d"[..]), Some(&40));
        
        // Intersection
        let intersection = trie1.intersection(&trie2);
        assert_eq!(intersection.len(), 2);
        assert_eq!(intersection.get(&b"b"[..]), Some(&2)); // From trie1
        assert_eq!(intersection.get(&b"c"[..]), Some(&3)); // From trie1
        assert_eq!(intersection.get(&b"a"[..]), None);
        assert_eq!(intersection.get(&b"d"[..]), None);
        
        // Difference
        let difference = trie1.difference(&trie2);
        assert_eq!(difference.len(), 1);
        assert_eq!(difference.get(&b"a"[..]), Some(&1));
        assert_eq!(difference.get(&b"b"[..]), None); // Removed
        assert_eq!(difference.get(&b"c"[..]), None); // Removed
    }
    
    #[test]
    fn test_iterator() {
        let mut trie = BytesTrieMap::new();
        trie.insert(b"apple", 1);
        trie.insert(b"app", 2);
        trie.insert(b"application", 3);
        
        let items: Vec<_> = trie.iter().collect();
        assert_eq!(items.len(), 3);
        
        // Check that we get all items (order may vary)
        let keys: Vec<Vec<u8>> = items.iter().map(|(k, _)| k.clone()).collect();
        assert!(keys.contains(&b"apple".to_vec()));
        assert!(keys.contains(&b"app".to_vec()));
        assert!(keys.contains(&b"application".to_vec()));
    }
}