// Expression Query Layer Implementation
// Support structured key queries on S-expressions

use std::collections::{BTreeMap, VecDeque};
use crate::triemap_derivation::{BytesTrieMap, TrieMap};

/// Represents different types of expression patterns for querying
#[derive(Debug, Clone, PartialEq)]
pub enum ExprPattern {
    /// Match any expression (wildcard)
    Any,
    /// Match a specific symbol
    Symbol(Vec<u8>),
    /// Match a variable by name
    Variable(String),
    /// Match an expression with specific arity and sub-patterns
    Compound {
        arity: usize,
        patterns: Vec<ExprPattern>,
    },
    /// Match expressions that satisfy a predicate
    Predicate(fn(&ExprPattern) -> bool),
}

/// Query engine for structured expression matching
pub struct ExprQueryEngine {
    /// Main storage for expressions indexed by structure
    structure_index: BytesTrieMap<Vec<ExprId>>,
    /// Symbol table for efficient symbol lookup
    symbol_index: BTreeMap<Vec<u8>, Vec<ExprId>>,
    /// Arity index for quick arity-based queries
    arity_index: BTreeMap<usize, Vec<ExprId>>,
    /// Expression storage
    expressions: BTreeMap<ExprId, StoredExpression>,
    /// Next available expression ID
    next_id: ExprId,
}

/// Unique identifier for expressions
pub type ExprId = u64;

/// Stored expression with metadata
#[derive(Debug, Clone)]
pub struct StoredExpression {
    pub id: ExprId,
    pub structure: ExprStructure,
    pub metadata: ExprMetadata,
}

/// Structure of an expression for indexing
#[derive(Debug, Clone, PartialEq)]
pub enum ExprStructure {
    Symbol(Vec<u8>),
    Variable(String),
    Compound {
        arity: usize,
        children: Vec<ExprStructure>,
    },
}

/// Metadata associated with expressions
#[derive(Debug, Clone)]
pub struct ExprMetadata {
    pub depth: usize,
    pub symbol_count: usize,
    pub variable_count: usize,
    pub created_at: std::time::SystemTime,
}

/// Query result containing matched expressions
#[derive(Debug)]
pub struct QueryResult {
    pub matched_ids: Vec<ExprId>,
    pub execution_time: std::time::Duration,
    pub stats: QueryStats,
}

/// Statistics about query execution
#[derive(Debug)]
pub struct QueryStats {
    pub expressions_scanned: usize,
    pub index_hits: usize,
    pub filters_applied: usize,
}

impl ExprQueryEngine {
    pub fn new() -> Self {
        Self {
            structure_index: BytesTrieMap::new(),
            symbol_index: BTreeMap::new(),
            arity_index: BTreeMap::new(),
            expressions: BTreeMap::new(),
            next_id: 1,
        }
    }
    
    /// Insert an expression into the query engine
    pub fn insert(&mut self, structure: ExprStructure) -> ExprId {
        let id = self.next_id;
        self.next_id += 1;
        
        let metadata = ExprMetadata {
            depth: self.calculate_depth(&structure),
            symbol_count: self.count_symbols(&structure),
            variable_count: self.count_variables(&structure),
            created_at: std::time::SystemTime::now(),
        };
        
        let expr = StoredExpression {
            id,
            structure: structure.clone(),
            metadata,
        };
        
        self.expressions.insert(id, expr);
        self.index_expression(id, &structure);
        
        id
    }
    
    /// Query expressions matching a pattern
    pub fn query(&self, pattern: &ExprPattern) -> QueryResult {
        let start_time = std::time::Instant::now();
        let mut stats = QueryStats {
            expressions_scanned: 0,
            index_hits: 0,
            filters_applied: 0,
        };
        
        let matched_ids = self.find_matches(pattern, &mut stats);
        
        QueryResult {
            matched_ids,
            execution_time: start_time.elapsed(),
            stats,
        }
    }
    
    /// Query with multiple patterns (AND operation)
    pub fn query_and(&self, patterns: &[ExprPattern]) -> QueryResult {
        let start_time = std::time::Instant::now();
        let mut stats = QueryStats {
            expressions_scanned: 0,
            index_hits: 0,
            filters_applied: 0,
        };
        
        if patterns.is_empty() {
            return QueryResult {
                matched_ids: Vec::new(),
                execution_time: start_time.elapsed(),
                stats,
            };
        }
        
        // Start with the first pattern
        let mut result_ids: std::collections::HashSet<ExprId> = 
            self.find_matches(&patterns[0], &mut stats).into_iter().collect();
        
        // Intersect with results from subsequent patterns
        for pattern in &patterns[1..] {
            let matches: std::collections::HashSet<ExprId> = 
                self.find_matches(pattern, &mut stats).into_iter().collect();
            result_ids = result_ids.intersection(&matches).cloned().collect();
        }
        
        QueryResult {
            matched_ids: result_ids.into_iter().collect(),
            execution_time: start_time.elapsed(),
            stats,
        }
    }
    
    /// Query with multiple patterns (OR operation)
    pub fn query_or(&self, patterns: &[ExprPattern]) -> QueryResult {
        let start_time = std::time::Instant::now();
        let mut stats = QueryStats {
            expressions_scanned: 0,
            index_hits: 0,
            filters_applied: 0,
        };
        
        let mut result_ids: std::collections::HashSet<ExprId> = std::collections::HashSet::new();
        
        for pattern in patterns {
            let matches: std::collections::HashSet<ExprId> = 
                self.find_matches(pattern, &mut stats).into_iter().collect();
            result_ids = result_ids.union(&matches).cloned().collect();
        }
        
        QueryResult {
            matched_ids: result_ids.into_iter().collect(),
            execution_time: start_time.elapsed(),
            stats,
        }
    }
    
    /// Get expression by ID
    pub fn get_expression(&self, id: ExprId) -> Option<&StoredExpression> {
        self.expressions.get(&id)
    }
    
    /// Get all expressions with a specific arity
    pub fn query_by_arity(&self, arity: usize) -> Vec<ExprId> {
        self.arity_index.get(&arity).cloned().unwrap_or_default()
    }
    
    /// Get all expressions containing a specific symbol
    pub fn query_by_symbol(&self, symbol: &[u8]) -> Vec<ExprId> {
        self.symbol_index.get(symbol).cloned().unwrap_or_default()
    }
    
    /// Remove an expression from the query engine
    pub fn remove(&mut self, id: ExprId) -> Option<StoredExpression> {
        if let Some(expr) = self.expressions.remove(&id) {
            self.unindex_expression(id, &expr.structure);
            Some(expr)
        } else {
            None
        }
    }
    
    /// Get statistics about the query engine
    pub fn stats(&self) -> EngineStats {
        EngineStats {
            total_expressions: self.expressions.len(),
            unique_symbols: self.symbol_index.len(),
            indexed_arities: self.arity_index.len(),
            structure_index_size: self.structure_index.len(),
        }
    }
    
    // Private helper methods
    
    fn find_matches(&self, pattern: &ExprPattern, stats: &mut QueryStats) -> Vec<ExprId> {
        match pattern {
            ExprPattern::Any => {
                stats.expressions_scanned += self.expressions.len();
                self.expressions.keys().cloned().collect()
            },
            ExprPattern::Symbol(symbol) => {
                stats.index_hits += 1;
                self.query_by_symbol(symbol)
            },
            ExprPattern::Variable(_) => {
                // For simplicity, scan all expressions
                stats.expressions_scanned += self.expressions.len();
                stats.filters_applied += 1;
                self.expressions.iter()
                    .filter_map(|(id, expr)| {
                        if self.matches_pattern(&expr.structure, pattern) {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .collect()
            },
            ExprPattern::Compound { arity, patterns: _ } => {
                stats.index_hits += 1;
                let mut candidates = self.query_by_arity(*arity);
                
                // Further filter by pattern matching
                stats.filters_applied += 1;
                candidates.retain(|&id| {
                    if let Some(expr) = self.expressions.get(&id) {
                        self.matches_pattern(&expr.structure, pattern)
                    } else {
                        false
                    }
                });
                
                candidates
            },
            ExprPattern::Predicate(_) => {
                // Scan all expressions and apply predicate
                stats.expressions_scanned += self.expressions.len();
                stats.filters_applied += 1;
                self.expressions.iter()
                    .filter_map(|(id, expr)| {
                        if self.matches_pattern(&expr.structure, pattern) {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        }
    }
    
    fn matches_pattern(&self, structure: &ExprStructure, pattern: &ExprPattern) -> bool {
        match (structure, pattern) {
            (_, ExprPattern::Any) => true,
            (ExprStructure::Symbol(s), ExprPattern::Symbol(p)) => s == p,
            (ExprStructure::Variable(v), ExprPattern::Variable(p)) => v == p,
            (ExprStructure::Compound { arity: sa, children: sc }, 
             ExprPattern::Compound { arity: pa, patterns: pp }) => {
                sa == pa && sc.len() == pp.len() && 
                sc.iter().zip(pp.iter()).all(|(child, pat)| self.matches_pattern(child, pat))
            },
            (_, ExprPattern::Predicate(pred)) => pred(pattern),
            _ => false,
        }
    }
    
    fn index_expression(&mut self, id: ExprId, structure: &ExprStructure) {
        match structure {
            ExprStructure::Symbol(symbol) => {
                self.symbol_index.entry(symbol.clone()).or_default().push(id);
            },
            ExprStructure::Variable(_) => {
                // Variables indexed separately if needed
            },
            ExprStructure::Compound { arity, children } => {
                self.arity_index.entry(*arity).or_default().push(id);
                
                // Index children recursively
                for child in children {
                    self.index_expression(id, child);
                }
                
                // Create structural key for trie indexing
                let structural_key = self.create_structural_key(structure);
                self.structure_index.insert_owned(structural_key, vec![id]);
            },
        }
    }
    
    fn unindex_expression(&mut self, id: ExprId, structure: &ExprStructure) {
        match structure {
            ExprStructure::Symbol(symbol) => {
                if let Some(ids) = self.symbol_index.get_mut(symbol) {
                    ids.retain(|&x| x != id);
                    if ids.is_empty() {
                        self.symbol_index.remove(symbol);
                    }
                }
            },
            ExprStructure::Variable(_) => {
                // Remove from variable index if implemented
            },
            ExprStructure::Compound { arity, children } => {
                if let Some(ids) = self.arity_index.get_mut(arity) {
                    ids.retain(|&x| x != id);
                    if ids.is_empty() {
                        self.arity_index.remove(arity);
                    }
                }
                
                // Unindex children recursively
                for child in children {
                    self.unindex_expression(id, child);
                }
                
                // Remove from structure index
                let structural_key = self.create_structural_key(structure);
                self.structure_index.remove(&structural_key.as_slice());
            },
        }
    }
    
    fn create_structural_key(&self, structure: &ExprStructure) -> Vec<u8> {
        let mut key = Vec::new();
        self.encode_structure(&mut key, structure);
        key
    }
    
    fn encode_structure(&self, buffer: &mut Vec<u8>, structure: &ExprStructure) {
        match structure {
            ExprStructure::Symbol(symbol) => {
                buffer.push(1); // Symbol marker
                buffer.extend_from_slice(symbol);
            },
            ExprStructure::Variable(var) => {
                buffer.push(2); // Variable marker
                buffer.extend_from_slice(var.as_bytes());
            },
            ExprStructure::Compound { arity, children } => {
                buffer.push(3); // Compound marker
                buffer.extend_from_slice(&arity.to_be_bytes());
                for child in children {
                    self.encode_structure(buffer, child);
                }
            },
        }
    }
    
    fn calculate_depth(&self, structure: &ExprStructure) -> usize {
        match structure {
            ExprStructure::Symbol(_) | ExprStructure::Variable(_) => 1,
            ExprStructure::Compound { children, .. } => {
                1 + children.iter().map(|c| self.calculate_depth(c)).max().unwrap_or(0)
            },
        }
    }
    
    fn count_symbols(&self, structure: &ExprStructure) -> usize {
        match structure {
            ExprStructure::Symbol(_) => 1,
            ExprStructure::Variable(_) => 0,
            ExprStructure::Compound { children, .. } => {
                children.iter().map(|c| self.count_symbols(c)).sum()
            },
        }
    }
    
    fn count_variables(&self, structure: &ExprStructure) -> usize {
        match structure {
            ExprStructure::Symbol(_) => 0,
            ExprStructure::Variable(_) => 1,
            ExprStructure::Compound { children, .. } => {
                children.iter().map(|c| self.count_variables(c)).sum()
            },
        }
    }
}

/// Statistics about the query engine
#[derive(Debug)]
pub struct EngineStats {
    pub total_expressions: usize,
    pub unique_symbols: usize,
    pub indexed_arities: usize,
    pub structure_index_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_query_operations() {
        let mut engine = ExprQueryEngine::new();
        
        // Insert some expressions
        let id1 = engine.insert(ExprStructure::Symbol(b"hello".to_vec()));
        let id2 = engine.insert(ExprStructure::Symbol(b"world".to_vec()));
        let id3 = engine.insert(ExprStructure::Variable("x".to_string()));
        
        // Query by symbol
        let result = engine.query(&ExprPattern::Symbol(b"hello".to_vec()));
        assert_eq!(result.matched_ids, vec![id1]);
        
        // Query any
        let result = engine.query(&ExprPattern::Any);
        assert_eq!(result.matched_ids.len(), 3);
        
        // Query by variable
        let result = engine.query(&ExprPattern::Variable("x".to_string()));
        assert_eq!(result.matched_ids, vec![id3]);
    }
    
    #[test]
    fn test_compound_expressions() {
        let mut engine = ExprQueryEngine::new();
        
        // Insert compound expression: (add x y)
        let add_expr = ExprStructure::Compound {
            arity: 3,
            children: vec![
                ExprStructure::Symbol(b"add".to_vec()),
                ExprStructure::Variable("x".to_string()),
                ExprStructure::Variable("y".to_string()),
            ],
        };
        let id1 = engine.insert(add_expr);
        
        // Insert another compound: (sub a b)
        let sub_expr = ExprStructure::Compound {
            arity: 3,
            children: vec![
                ExprStructure::Symbol(b"sub".to_vec()),
                ExprStructure::Variable("a".to_string()),
                ExprStructure::Variable("b".to_string()),
            ],
        };
        let id2 = engine.insert(sub_expr);
        
        // Query by arity
        let arity3_exprs = engine.query_by_arity(3);
        assert_eq!(arity3_exprs.len(), 2);
        assert!(arity3_exprs.contains(&id1));
        assert!(arity3_exprs.contains(&id2));
        
        // Query by symbol (should find expressions containing the symbol)
        let add_exprs = engine.query_by_symbol(b"add");
        assert_eq!(add_exprs, vec![id1]);
    }
    
    #[test]
    fn test_and_or_queries() {
        let mut engine = ExprQueryEngine::new();
        
        let id1 = engine.insert(ExprStructure::Symbol(b"foo".to_vec()));
        let id2 = engine.insert(ExprStructure::Symbol(b"bar".to_vec()));
        let id3 = engine.insert(ExprStructure::Variable("x".to_string()));
        
        // OR query
        let result = engine.query_or(&[
            ExprPattern::Symbol(b"foo".to_vec()),
            ExprPattern::Symbol(b"bar".to_vec()),
        ]);
        assert_eq!(result.matched_ids.len(), 2);
        
        // AND query (no matches since symbols are different)
        let result = engine.query_and(&[
            ExprPattern::Symbol(b"foo".to_vec()),
            ExprPattern::Symbol(b"bar".to_vec()),
        ]);
        assert_eq!(result.matched_ids.len(), 0);
        
        // AND query with Any (should match single expression)
        let result = engine.query_and(&[
            ExprPattern::Symbol(b"foo".to_vec()),
            ExprPattern::Any,
        ]);
        assert_eq!(result.matched_ids, vec![id1]);
    }
    
    #[test]
    fn test_engine_stats() {
        let mut engine = ExprQueryEngine::new();
        
        engine.insert(ExprStructure::Symbol(b"hello".to_vec()));
        engine.insert(ExprStructure::Symbol(b"world".to_vec()));
        engine.insert(ExprStructure::Variable("x".to_string()));
        
        let stats = engine.stats();
        assert_eq!(stats.total_expressions, 3);
        assert_eq!(stats.unique_symbols, 2);
    }
    
    #[test]
    fn test_remove_expression() {
        let mut engine = ExprQueryEngine::new();
        
        let id = engine.insert(ExprStructure::Symbol(b"test".to_vec()));
        assert_eq!(engine.stats().total_expressions, 1);
        
        let removed = engine.remove(id);
        assert!(removed.is_some());
        assert_eq!(engine.stats().total_expressions, 0);
        
        // Should not find the removed expression
        let result = engine.query(&ExprPattern::Symbol(b"test".to_vec()));
        assert_eq!(result.matched_ids.len(), 0);
    }
}