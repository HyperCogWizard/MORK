// Enhanced Bidirectional Pattern Matching and Unification
// Powers S-expression/space pattern engines

use std::collections::{HashMap, HashSet, BTreeMap};
use crate::expr_query::{ExprStructure, ExprId};

/// Enhanced unification engine for S-expressions
pub struct UnificationEngine {
    /// Cache for unification results
    unification_cache: HashMap<(PatternId, ExprId), UnificationResult>,
    /// Compiled patterns for reuse
    pattern_cache: HashMap<String, CompiledPattern>,
    /// Configuration
    config: UnificationConfig,
}

/// Configuration for unification behavior
#[derive(Debug, Clone)]
pub struct UnificationConfig {
    /// Maximum unification depth
    pub max_depth: usize,
    /// Enable occurs check
    pub occurs_check: bool,
    /// Enable caching
    pub enable_caching: bool,
    /// Maximum variable scope
    pub max_variables: usize,
}

impl Default for UnificationConfig {
    fn default() -> Self {
        Self {
            max_depth: 100,
            occurs_check: true,
            enable_caching: true,
            max_variables: 1000,
        }
    }
}

/// Unique identifier for patterns
pub type PatternId = u64;

/// Compiled pattern for efficient matching
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    pub id: PatternId,
    pub structure: PatternStructure,
    pub variables: Vec<Variable>,
    pub constraints: Vec<Constraint>,
}

/// Pattern structure for matching
#[derive(Debug, Clone, PartialEq)]
pub enum PatternStructure {
    /// Exact symbol match
    Symbol(Vec<u8>),
    /// Variable that can bind to any expression
    Variable(Variable),
    /// Compound pattern with sub-patterns
    Compound {
        arity: usize,
        patterns: Vec<PatternStructure>,
    },
    /// Wildcard that matches anything
    Wildcard,
    /// Conditional pattern with predicate
    Conditional {
        pattern: Box<PatternStructure>,
        condition: Condition,
    },
    /// Alternative patterns (OR)
    Alternative(Vec<PatternStructure>),
    /// Sequence pattern with optional repetition
    Sequence {
        patterns: Vec<PatternStructure>,
        min_matches: usize,
        max_matches: Option<usize>,
    },
}

/// Variable in patterns
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Variable {
    pub name: String,
    pub id: u32,
    pub var_type: VariableType,
}

/// Types of variables
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VariableType {
    /// Can bind to any single expression
    Expression,
    /// Can bind to a sequence of expressions
    Sequence,
    /// Can bind to symbols only
    Symbol,
    /// Can bind to compounds only
    Compound,
}

/// Constraints on variables
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Type constraint
    Type(Variable, VariableType),
    /// Equality constraint between variables
    Equal(Variable, Variable),
    /// Inequality constraint
    NotEqual(Variable, Variable),
    /// Structural constraint
    Structure(Variable, StructuralConstraint),
    /// Custom predicate constraint
    Predicate(Variable, fn(&ExprStructure) -> bool),
}

/// Structural constraints
#[derive(Debug, Clone)]
pub enum StructuralConstraint {
    /// Must have specific arity
    Arity(usize),
    /// Must contain specific symbol
    ContainsSymbol(Vec<u8>),
    /// Must match depth requirement
    Depth(usize),
    /// Must satisfy size constraint
    Size { min: Option<usize>, max: Option<usize> },
}

/// Conditions for conditional patterns
#[derive(Debug, Clone)]
pub enum Condition {
    /// Expression must satisfy predicate
    Predicate(fn(&ExprStructure) -> bool),
    /// Expression must unify with another pattern
    Unifiable(Box<PatternStructure>),
    /// Expression must have specific properties
    Property(PropertyCheck),
}

/// Property checks
#[derive(Debug, Clone)]
pub enum PropertyCheck {
    IsSymbol,
    IsVariable,
    IsCompound,
    HasArity(usize),
    ContainsSymbol(Vec<u8>),
    DepthEquals(usize),
    DepthGreaterThan(usize),
    DepthLessThan(usize),
}

/// Result of unification
#[derive(Debug, Clone)]
pub struct UnificationResult {
    pub success: bool,
    pub bindings: VariableBindings,
    pub constraints_satisfied: bool,
    pub execution_time: std::time::Duration,
}

/// Variable bindings from unification
#[derive(Debug, Clone)]
pub struct VariableBindings {
    pub bindings: HashMap<Variable, ExprStructure>,
    pub constraints: Vec<ConstraintBinding>,
}

/// Constraint bindings
#[derive(Debug, Clone)]
pub struct ConstraintBinding {
    pub constraint: Constraint,
    pub satisfied: bool,
    pub reason: Option<String>,
}

/// Bidirectional matching context
pub struct MatchingContext {
    depth: usize,
    variables: HashMap<Variable, ExprStructure>,
    constraints: Vec<Constraint>,
    max_depth: usize,
}

impl UnificationEngine {
    pub fn new() -> Self {
        Self::with_config(UnificationConfig::default())
    }
    
    pub fn with_config(config: UnificationConfig) -> Self {
        Self {
            unification_cache: HashMap::new(),
            pattern_cache: HashMap::new(),
            config,
        }
    }
    
    /// Compile a pattern from string representation
    pub fn compile_pattern(&mut self, pattern_str: &str) -> Result<CompiledPattern, UnificationError> {
        if let Some(cached) = self.pattern_cache.get(pattern_str) {
            return Ok(cached.clone());
        }
        
        let mut parser = PatternParser::new(pattern_str);
        let pattern = parser.parse()?;
        
        if self.config.enable_caching {
            self.pattern_cache.insert(pattern_str.to_string(), pattern.clone());
        }
        
        Ok(pattern)
    }
    
    /// Unify an expression with a pattern
    pub fn unify(&mut self, expr: &ExprStructure, pattern: &CompiledPattern) -> UnificationResult {
        let start_time = std::time::Instant::now();
        
        // Check cache if enabled
        if self.config.enable_caching {
            // For simplicity, we'll create a dummy ExprId - in real implementation this would be proper
            let expr_id = 0;
            if let Some(cached) = self.unification_cache.get(&(pattern.id, expr_id)) {
                return cached.clone();
            }
        }
        
        let mut context = MatchingContext::new(self.config.max_depth);
        let success = self.unify_recursive(expr, &pattern.structure, &mut context);
        
        let bindings = VariableBindings {
            bindings: context.variables.clone(),
            constraints: self.check_constraints(&pattern.constraints, &context.variables),
        };
        
        let constraints_satisfied = bindings.constraints.iter().all(|c| c.satisfied);
        
        let result = UnificationResult {
            success: success && constraints_satisfied,
            bindings,
            constraints_satisfied,
            execution_time: start_time.elapsed(),
        };
        
        result
    }
    
    /// Bidirectional matching - find expressions that match pattern
    pub fn find_matches(&mut self, pattern: &CompiledPattern, expressions: &[ExprStructure]) -> Vec<(usize, UnificationResult)> {
        let mut matches = Vec::new();
        
        for (idx, expr) in expressions.iter().enumerate() {
            let result = self.unify(expr, pattern);
            if result.success {
                matches.push((idx, result));
            }
        }
        
        matches
    }
    
    /// Pattern matching with multiple patterns
    pub fn multi_pattern_match(&mut self, patterns: &[CompiledPattern], expressions: &[ExprStructure]) -> MultiMatchResult {
        let mut results = HashMap::new();
        
        for (pattern_idx, pattern) in patterns.iter().enumerate() {
            let matches = self.find_matches(pattern, expressions);
            results.insert(pattern_idx, matches);
        }
        
        MultiMatchResult {
            pattern_matches: results,
            total_patterns: patterns.len(),
            total_expressions: expressions.len(),
        }
    }
    
    /// Enhanced unification with constraint propagation
    pub fn unify_with_constraints(&mut self, expr: &ExprStructure, pattern: &CompiledPattern, additional_constraints: &[Constraint]) -> UnificationResult {
        let mut enhanced_pattern = pattern.clone();
        enhanced_pattern.constraints.extend_from_slice(additional_constraints);
        
        self.unify(expr, &enhanced_pattern)
    }
    
    /// Generate all possible unifications
    pub fn generate_unifications(&mut self, expr: &ExprStructure, pattern: &CompiledPattern) -> Vec<UnificationResult> {
        // For patterns with alternatives, generate all possible unifications
        match &pattern.structure {
            PatternStructure::Alternative(alternatives) => {
                let mut results = Vec::new();
                for alt in alternatives {
                    let alt_pattern = CompiledPattern {
                        id: pattern.id,
                        structure: alt.clone(),
                        variables: pattern.variables.clone(),
                        constraints: pattern.constraints.clone(),
                    };
                    let result = self.unify(expr, &alt_pattern);
                    if result.success {
                        results.push(result);
                    }
                }
                results
            },
            _ => {
                vec![self.unify(expr, pattern)]
            }
        }
    }
    
    /// Clear caches
    pub fn clear_cache(&mut self) {
        self.unification_cache.clear();
        self.pattern_cache.clear();
    }
    
    /// Get engine statistics
    pub fn stats(&self) -> UnificationStats {
        UnificationStats {
            cached_patterns: self.pattern_cache.len(),
            cached_unifications: self.unification_cache.len(),
            cache_enabled: self.config.enable_caching,
        }
    }
    
    // Private implementation methods
    
    fn unify_recursive(&self, expr: &ExprStructure, pattern: &PatternStructure, context: &mut MatchingContext) -> bool {
        if context.depth >= context.max_depth {
            return false;
        }
        
        context.depth += 1;
        
        let result = match (expr, pattern) {
            (_, PatternStructure::Wildcard) => true,
            
            (expr, PatternStructure::Variable(var)) => {
                self.bind_variable(expr, var, context)
            },
            
            (ExprStructure::Symbol(s1), PatternStructure::Symbol(s2)) => s1 == s2,
            
            (ExprStructure::Compound { arity: a1, children: c1 }, 
             PatternStructure::Compound { arity: a2, patterns: p2 }) => {
                a1 == a2 && c1.len() == p2.len() &&
                c1.iter().zip(p2.iter()).all(|(child, pat)| self.unify_recursive(child, pat, context))
            },
            
            (expr, PatternStructure::Conditional { pattern, condition }) => {
                self.check_condition(expr, condition) && self.unify_recursive(expr, pattern, context)
            },
            
            (expr, PatternStructure::Alternative(alternatives)) => {
                alternatives.iter().any(|alt| self.unify_recursive(expr, alt, context))
            },
            
            (ExprStructure::Compound { children, .. }, PatternStructure::Sequence { patterns, min_matches, max_matches }) => {
                self.match_sequence(children, patterns, *min_matches, *max_matches, context)
            },
            
            _ => false,
        };
        
        context.depth -= 1;
        result
    }
    
    fn bind_variable(&self, expr: &ExprStructure, var: &Variable, context: &mut MatchingContext) -> bool {
        // Check if variable is already bound
        if let Some(existing) = context.variables.get(var) {
            // Must unify with existing binding
            self.expressions_equal(expr, existing)
        } else {
            // Check type compatibility
            if self.type_compatible(expr, &var.var_type) {
                context.variables.insert(var.clone(), expr.clone());
                true
            } else {
                false
            }
        }
    }
    
    fn expressions_equal(&self, expr1: &ExprStructure, expr2: &ExprStructure) -> bool {
        match (expr1, expr2) {
            (ExprStructure::Symbol(s1), ExprStructure::Symbol(s2)) => s1 == s2,
            (ExprStructure::Variable(v1), ExprStructure::Variable(v2)) => v1 == v2,
            (ExprStructure::Compound { arity: a1, children: c1 }, 
             ExprStructure::Compound { arity: a2, children: c2 }) => {
                a1 == a2 && c1.len() == c2.len() && 
                c1.iter().zip(c2.iter()).all(|(e1, e2)| self.expressions_equal(e1, e2))
            },
            _ => false,
        }
    }
    
    fn type_compatible(&self, expr: &ExprStructure, var_type: &VariableType) -> bool {
        match (expr, var_type) {
            (_, VariableType::Expression) => true,
            (ExprStructure::Symbol(_), VariableType::Symbol) => true,
            (ExprStructure::Compound { .. }, VariableType::Compound) => true,
            _ => false,
        }
    }
    
    fn check_condition(&self, expr: &ExprStructure, condition: &Condition) -> bool {
        match condition {
            Condition::Predicate(pred) => pred(expr),
            Condition::Property(prop) => self.check_property(expr, prop),
            Condition::Unifiable(pattern) => {
                // Simplified unifiability check
                match (expr, pattern.as_ref()) {
                    (_, PatternStructure::Wildcard) => true,
                    (ExprStructure::Symbol(s1), PatternStructure::Symbol(s2)) => s1 == s2,
                    _ => false,
                }
            }
        }
    }
    
    fn check_property(&self, expr: &ExprStructure, prop: &PropertyCheck) -> bool {
        match prop {
            PropertyCheck::IsSymbol => matches!(expr, ExprStructure::Symbol(_)),
            PropertyCheck::IsVariable => matches!(expr, ExprStructure::Variable(_)),
            PropertyCheck::IsCompound => matches!(expr, ExprStructure::Compound { .. }),
            PropertyCheck::HasArity(arity) => {
                if let ExprStructure::Compound { arity: expr_arity, .. } = expr {
                    expr_arity == arity
                } else {
                    false
                }
            },
            PropertyCheck::ContainsSymbol(symbol) => {
                self.contains_symbol(expr, symbol)
            },
            PropertyCheck::DepthEquals(depth) => {
                self.calculate_depth(expr) == *depth
            },
            PropertyCheck::DepthGreaterThan(depth) => {
                self.calculate_depth(expr) > *depth
            },
            PropertyCheck::DepthLessThan(depth) => {
                self.calculate_depth(expr) < *depth
            },
        }
    }
    
    fn contains_symbol(&self, expr: &ExprStructure, target: &[u8]) -> bool {
        match expr {
            ExprStructure::Symbol(s) => s == target,
            ExprStructure::Variable(_) => false,
            ExprStructure::Compound { children, .. } => {
                children.iter().any(|child| self.contains_symbol(child, target))
            },
        }
    }
    
    fn calculate_depth(&self, expr: &ExprStructure) -> usize {
        match expr {
            ExprStructure::Symbol(_) | ExprStructure::Variable(_) => 1,
            ExprStructure::Compound { children, .. } => {
                1 + children.iter().map(|c| self.calculate_depth(c)).max().unwrap_or(0)
            },
        }
    }
    
    fn match_sequence(&self, children: &[ExprStructure], patterns: &[PatternStructure], min_matches: usize, max_matches: Option<usize>, context: &mut MatchingContext) -> bool {
        if children.len() < min_matches {
            return false;
        }
        
        if let Some(max) = max_matches {
            if children.len() > max {
                return false;
            }
        }
        
        // Simple sequence matching - each pattern must match corresponding child
        if patterns.len() != children.len() {
            return false;
        }
        
        children.iter().zip(patterns.iter()).all(|(child, pattern)| {
            self.unify_recursive(child, pattern, context)
        })
    }
    
    fn check_constraints(&self, constraints: &[Constraint], bindings: &HashMap<Variable, ExprStructure>) -> Vec<ConstraintBinding> {
        let mut results = Vec::new();
        
        for constraint in constraints {
            let (satisfied, reason) = match constraint {
                Constraint::Type(var, var_type) => {
                    if let Some(expr) = bindings.get(var) {
                        (self.type_compatible(expr, var_type), None)
                    } else {
                        (false, Some("Variable not bound".to_string()))
                    }
                },
                Constraint::Equal(var1, var2) => {
                    match (bindings.get(var1), bindings.get(var2)) {
                        (Some(expr1), Some(expr2)) => {
                            (self.expressions_equal(expr1, expr2), None)
                        },
                        _ => (false, Some("Variables not bound".to_string()))
                    }
                },
                Constraint::NotEqual(var1, var2) => {
                    match (bindings.get(var1), bindings.get(var2)) {
                        (Some(expr1), Some(expr2)) => {
                            (!self.expressions_equal(expr1, expr2), None)
                        },
                        _ => (true, None) // If not both bound, constraint is satisfied
                    }
                },
                Constraint::Structure(var, struct_constraint) => {
                    if let Some(expr) = bindings.get(var) {
                        (self.check_structural_constraint(expr, struct_constraint), None)
                    } else {
                        (false, Some("Variable not bound".to_string()))
                    }
                },
                Constraint::Predicate(var, pred) => {
                    if let Some(expr) = bindings.get(var) {
                        (pred(expr), None)
                    } else {
                        (false, Some("Variable not bound".to_string()))
                    }
                },
            };
            
            results.push(ConstraintBinding {
                constraint: constraint.clone(),
                satisfied,
                reason,
            });
        }
        
        results
    }
    
    fn check_structural_constraint(&self, expr: &ExprStructure, constraint: &StructuralConstraint) -> bool {
        match constraint {
            StructuralConstraint::Arity(arity) => {
                if let ExprStructure::Compound { arity: expr_arity, .. } = expr {
                    expr_arity == arity
                } else {
                    false
                }
            },
            StructuralConstraint::ContainsSymbol(symbol) => {
                self.contains_symbol(expr, symbol)
            },
            StructuralConstraint::Depth(depth) => {
                self.calculate_depth(expr) == *depth
            },
            StructuralConstraint::Size { min, max } => {
                let size = self.calculate_size(expr);
                let min_ok = min.map_or(true, |m| size >= m);
                let max_ok = max.map_or(true, |m| size <= m);
                min_ok && max_ok
            },
        }
    }
    
    fn calculate_size(&self, expr: &ExprStructure) -> usize {
        match expr {
            ExprStructure::Symbol(_) | ExprStructure::Variable(_) => 1,
            ExprStructure::Compound { children, .. } => {
                1 + children.iter().map(|c| self.calculate_size(c)).sum::<usize>()
            },
        }
    }
}

impl MatchingContext {
    fn new(max_depth: usize) -> Self {
        Self {
            depth: 0,
            variables: HashMap::new(),
            constraints: Vec::new(),
            max_depth,
        }
    }
}

/// Result of multi-pattern matching
#[derive(Debug)]
pub struct MultiMatchResult {
    pub pattern_matches: HashMap<usize, Vec<(usize, UnificationResult)>>,
    pub total_patterns: usize,
    pub total_expressions: usize,
}

/// Statistics about the unification engine
#[derive(Debug)]
pub struct UnificationStats {
    pub cached_patterns: usize,
    pub cached_unifications: usize,
    pub cache_enabled: bool,
}

/// Errors in unification
#[derive(Debug, Clone)]
pub enum UnificationError {
    ParseError(String),
    TypeMismatch(String),
    ConstraintViolation(String),
    RecursionLimit,
    InvalidPattern(String),
}

impl std::fmt::Display for UnificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::TypeMismatch(msg) => write!(f, "Type mismatch: {}", msg),
            Self::ConstraintViolation(msg) => write!(f, "Constraint violation: {}", msg),
            Self::RecursionLimit => write!(f, "Recursion limit exceeded"),
            Self::InvalidPattern(msg) => write!(f, "Invalid pattern: {}", msg),
        }
    }
}

impl std::error::Error for UnificationError {}

/// Simple pattern parser
struct PatternParser {
    input: String,
    position: usize,
    next_var_id: u32,
}

impl PatternParser {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_string(),
            position: 0,
            next_var_id: 1,
        }
    }
    
    fn parse(&mut self) -> Result<CompiledPattern, UnificationError> {
        let structure = self.parse_pattern()?;
        
        Ok(CompiledPattern {
            id: 1, // Simplified - would use proper ID generation
            structure,
            variables: Vec::new(), // Would collect during parsing
            constraints: Vec::new(), // Would parse constraints
        })
    }
    
    fn parse_pattern(&mut self) -> Result<PatternStructure, UnificationError> {
        self.skip_whitespace();
        
        if self.position >= self.input.len() {
            return Err(UnificationError::ParseError("Unexpected end of input".to_string()));
        }
        
        let ch = self.current_char();
        
        match ch {
            '*' => {
                self.position += 1;
                Ok(PatternStructure::Wildcard)
            },
            '?' => {
                self.position += 1;
                let var = Variable {
                    name: format!("var_{}", self.next_var_id),
                    id: self.next_var_id,
                    var_type: VariableType::Expression,
                };
                self.next_var_id += 1;
                Ok(PatternStructure::Variable(var))
            },
            '(' => {
                self.parse_compound_pattern()
            },
            '"' => {
                self.parse_string_pattern()
            },
            _ => {
                // Parse identifier as symbol
                let identifier = self.parse_identifier()?;
                Ok(PatternStructure::Symbol(identifier.into_bytes()))
            }
        }
    }
    
    fn parse_compound_pattern(&mut self) -> Result<PatternStructure, UnificationError> {
        self.position += 1; // Skip (
        
        let mut patterns = Vec::new();
        
        while self.position < self.input.len() && self.current_char() != ')' {
            self.skip_whitespace();
            if self.current_char() == ')' {
                break;
            }
            patterns.push(self.parse_pattern()?);
            self.skip_whitespace();
        }
        
        if self.position >= self.input.len() {
            return Err(UnificationError::ParseError("Unmatched parenthesis".to_string()));
        }
        
        self.position += 1; // Skip )
        
        Ok(PatternStructure::Compound {
            arity: patterns.len(),
            patterns,
        })
    }
    
    fn parse_string_pattern(&mut self) -> Result<PatternStructure, UnificationError> {
        self.position += 1; // Skip opening quote
        
        let start = self.position;
        while self.position < self.input.len() && self.current_char() != '"' {
            self.position += 1;
        }
        
        if self.position >= self.input.len() {
            return Err(UnificationError::ParseError("Unterminated string".to_string()));
        }
        
        let content = self.input[start..self.position].to_string();
        self.position += 1; // Skip closing quote
        
        Ok(PatternStructure::Symbol(content.into_bytes()))
    }
    
    fn parse_identifier(&mut self) -> Result<String, UnificationError> {
        let start = self.position;
        
        while self.position < self.input.len() {
            let ch = self.current_char();
            if ch.is_whitespace() || ch == ')' || ch == '(' {
                break;
            }
            self.position += 1;
        }
        
        if start == self.position {
            return Err(UnificationError::ParseError("Expected identifier".to_string()));
        }
        
        Ok(self.input[start..self.position].to_string())
    }
    
    fn skip_whitespace(&mut self) {
        while self.position < self.input.len() && self.current_char().is_whitespace() {
            self.position += 1;
        }
    }
    
    fn current_char(&self) -> char {
        self.input.chars().nth(self.position).unwrap_or('\0')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_unification() {
        let mut engine = UnificationEngine::new();
        
        // Create a simple pattern
        let pattern = engine.compile_pattern("(add ? ?)").unwrap();
        
        // Create matching expression
        let expr = ExprStructure::Compound {
            arity: 3,
            children: vec![
                ExprStructure::Symbol(b"add".to_vec()),
                ExprStructure::Symbol(b"x".to_vec()),
                ExprStructure::Symbol(b"y".to_vec()),
            ],
        };
        
        let result = engine.unify(&expr, &pattern);
        assert!(result.success);
    }
    
    #[test]
    fn test_wildcard_matching() {
        let mut engine = UnificationEngine::new();
        
        let pattern = engine.compile_pattern("*").unwrap();
        
        let expr = ExprStructure::Symbol(b"anything".to_vec());
        let result = engine.unify(&expr, &pattern);
        assert!(result.success);
    }
    
    #[test]
    fn test_symbol_matching() {
        let mut engine = UnificationEngine::new();
        
        let pattern = engine.compile_pattern("\"hello\"").unwrap();
        
        // Matching case
        let expr1 = ExprStructure::Symbol(b"hello".to_vec());
        let result1 = engine.unify(&expr1, &pattern);
        assert!(result1.success);
        
        // Non-matching case
        let expr2 = ExprStructure::Symbol(b"world".to_vec());
        let result2 = engine.unify(&expr2, &pattern);
        assert!(!result2.success);
    }
    
    #[test]
    fn test_multi_pattern_matching() {
        let mut engine = UnificationEngine::new();
        
        let pattern1 = engine.compile_pattern("(add ? ?)").unwrap();
        let pattern2 = engine.compile_pattern("(sub ? ?)").unwrap();
        
        let expressions = vec![
            ExprStructure::Compound {
                arity: 3,
                children: vec![
                    ExprStructure::Symbol(b"add".to_vec()),
                    ExprStructure::Symbol(b"x".to_vec()),
                    ExprStructure::Symbol(b"y".to_vec()),
                ],
            },
            ExprStructure::Compound {
                arity: 3,
                children: vec![
                    ExprStructure::Symbol(b"sub".to_vec()),
                    ExprStructure::Symbol(b"a".to_vec()),
                    ExprStructure::Symbol(b"b".to_vec()),
                ],
            },
        ];
        
        let result = engine.multi_pattern_match(&[pattern1, pattern2], &expressions);
        
        // Both patterns should have one match each
        assert_eq!(result.pattern_matches.len(), 2);
        assert_eq!(result.pattern_matches[&0].len(), 1); // add pattern matches first expr
        assert_eq!(result.pattern_matches[&1].len(), 1); // sub pattern matches second expr
    }
    
    #[test]
    fn test_engine_caching() {
        let mut engine = UnificationEngine::new();
        
        // Compile same pattern twice
        let _pattern1 = engine.compile_pattern("(test ?)").unwrap();
        let _pattern2 = engine.compile_pattern("(test ?)").unwrap();
        
        let stats = engine.stats();
        assert_eq!(stats.cached_patterns, 1); // Should only cache once
    }
}