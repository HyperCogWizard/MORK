// JSONPath Query Engine Implementation
// Partial JSONPath implementation for structured and pattern-based access

use std::collections::{HashMap, VecDeque};
use serde_json::{Value, Map};

/// JSONPath query engine for structured JSON access
pub struct JsonPathEngine {
    /// Cache for compiled JSONPath expressions
    compiled_cache: HashMap<String, CompiledPath>,
    /// Configuration options
    config: JsonPathConfig,
}

/// Configuration for JSONPath engine behavior
#[derive(Debug, Clone)]
pub struct JsonPathConfig {
    /// Maximum recursion depth for recursive descent
    pub max_depth: usize,
    /// Whether to cache compiled paths
    pub enable_caching: bool,
    /// Whether to allow non-standard extensions
    pub allow_extensions: bool,
}

impl Default for JsonPathConfig {
    fn default() -> Self {
        Self {
            max_depth: 100,
            enable_caching: true,
            allow_extensions: false,
        }
    }
}

/// Compiled JSONPath expression for efficient reuse
#[derive(Debug, Clone)]
pub struct CompiledPath {
    pub segments: Vec<PathSegment>,
    pub is_absolute: bool,
}

/// Individual segment of a JSONPath
#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    /// Root element ($)
    Root,
    /// Current element (@)
    Current,
    /// Child access by key
    Child(String),
    /// Array index access
    Index(i64),
    /// Array slice [start:end:step]
    Slice { start: Option<i64>, end: Option<i64>, step: Option<i64> },
    /// Wildcard (*) - all children
    Wildcard,
    /// Recursive descent (..)
    RecursiveDescent,
    /// Filter expression [?(...)]
    Filter(FilterExpression),
    /// Union of multiple selectors [a,b,c]
    Union(Vec<PathSegment>),
}

/// Filter expressions for conditional selection
#[derive(Debug, Clone, PartialEq)]
pub enum FilterExpression {
    /// Comparison operations
    Compare { left: FilterValue, op: CompareOp, right: FilterValue },
    /// Logical operations
    Logical { left: Box<FilterExpression>, op: LogicalOp, right: Box<FilterExpression> },
    /// Existence check
    Exists(String),
    /// Regular expression match
    Regex { field: String, pattern: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    /// Literal value
    Literal(Value),
    /// Field reference
    Field(String),
    /// Current element
    Current,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    In,
    NotIn,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOp {
    And,
    Or,
    Not,
}

/// Result of JSONPath evaluation
#[derive(Debug)]
pub struct QueryResult {
    pub values: Vec<Value>,
    pub paths: Vec<String>,
    pub execution_time: std::time::Duration,
    pub cache_hit: bool,
}

/// Error types for JSONPath operations
#[derive(Debug, Clone)]
pub enum JsonPathError {
    ParseError(String),
    EvaluationError(String),
    InvalidFilter(String),
    RecursionLimit,
}

impl std::fmt::Display for JsonPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::EvaluationError(msg) => write!(f, "Evaluation error: {}", msg),
            Self::InvalidFilter(msg) => write!(f, "Invalid filter: {}", msg),
            Self::RecursionLimit => write!(f, "Recursion limit exceeded"),
        }
    }
}

impl std::error::Error for JsonPathError {}

impl JsonPathEngine {
    pub fn new() -> Self {
        Self::with_config(JsonPathConfig::default())
    }
    
    pub fn with_config(config: JsonPathConfig) -> Self {
        Self {
            compiled_cache: HashMap::new(),
            config,
        }
    }
    
    /// Query JSON data using JSONPath expression
    pub fn query(&mut self, json: &Value, path: &str) -> Result<QueryResult, JsonPathError> {
        let start_time = std::time::Instant::now();
        
        // Check cache first
        let (compiled, cache_hit) = if self.config.enable_caching {
            if let Some(cached) = self.compiled_cache.get(path) {
                (cached.clone(), true)
            } else {
                let compiled = self.compile_path(path)?;
                self.compiled_cache.insert(path.to_string(), compiled.clone());
                (compiled, false)
            }
        } else {
            (self.compile_path(path)?, false)
        };
        
        let mut context = EvaluationContext::new(&self.config);
        let results = self.evaluate_path(json, &compiled, &mut context)?;
        
        Ok(QueryResult {
            values: results.into_iter().map(|r| r.value).collect(),
            paths: results.into_iter().map(|r| r.path).collect(),
            execution_time: start_time.elapsed(),
            cache_hit,
        })
    }
    
    /// Compile a JSONPath string into a reusable form
    pub fn compile_path(&self, path: &str) -> Result<CompiledPath, JsonPathError> {
        if path.is_empty() {
            return Err(JsonPathError::ParseError("Empty path".to_string()));
        }
        
        let mut parser = PathParser::new(path);
        parser.parse()
    }
    
    /// Query multiple paths at once
    pub fn query_multiple(&mut self, json: &Value, paths: &[&str]) -> Result<HashMap<String, QueryResult>, JsonPathError> {
        let mut results = HashMap::new();
        
        for &path in paths {
            let result = self.query(json, path)?;
            results.insert(path.to_string(), result);
        }
        
        Ok(results)
    }
    
    /// Clear the compilation cache
    pub fn clear_cache(&mut self) {
        self.compiled_cache.clear();
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            size: self.compiled_cache.len(),
            enabled: self.config.enable_caching,
        }
    }
    
    // Private evaluation methods
    
    fn evaluate_path(&self, json: &Value, compiled: &CompiledPath, context: &mut EvaluationContext) -> Result<Vec<EvaluationResult>, JsonPathError> {
        let mut results = vec![EvaluationResult {
            value: json.clone(),
            path: if compiled.is_absolute { "$".to_string() } else { "@".to_string() },
        }];
        
        for segment in &compiled.segments {
            results = self.apply_segment(results, segment, context)?;
        }
        
        Ok(results)
    }
    
    fn apply_segment(&self, inputs: Vec<EvaluationResult>, segment: &PathSegment, context: &mut EvaluationContext) -> Result<Vec<EvaluationResult>, JsonPathError> {
        let mut outputs = Vec::new();
        
        for input in inputs {
            match segment {
                PathSegment::Root => {
                    outputs.push(EvaluationResult {
                        value: input.value.clone(),
                        path: "$".to_string(),
                    });
                },
                PathSegment::Current => {
                    outputs.push(input);
                },
                PathSegment::Child(key) => {
                    self.apply_child_access(&input, key, &mut outputs)?;
                },
                PathSegment::Index(idx) => {
                    self.apply_index_access(&input, *idx, &mut outputs)?;
                },
                PathSegment::Slice { start, end, step } => {
                    self.apply_slice_access(&input, *start, *end, *step, &mut outputs)?;
                },
                PathSegment::Wildcard => {
                    self.apply_wildcard(&input, &mut outputs)?;
                },
                PathSegment::RecursiveDescent => {
                    self.apply_recursive_descent(&input, &mut outputs, context)?;
                },
                PathSegment::Filter(filter) => {
                    self.apply_filter(&input, filter, &mut outputs, context)?;
                },
                PathSegment::Union(segments) => {
                    for seg in segments {
                        let single_input = vec![input.clone()];
                        let mut union_results = self.apply_segment(single_input, seg, context)?;
                        outputs.append(&mut union_results);
                    }
                },
            }
        }
        
        Ok(outputs)
    }
    
    fn apply_child_access(&self, input: &EvaluationResult, key: &str, outputs: &mut Vec<EvaluationResult>) -> Result<(), JsonPathError> {
        if let Value::Object(obj) = &input.value {
            if let Some(value) = obj.get(key) {
                outputs.push(EvaluationResult {
                    value: value.clone(),
                    path: format!("{}.{}", input.path, key),
                });
            }
        }
        Ok(())
    }
    
    fn apply_index_access(&self, input: &EvaluationResult, idx: i64, outputs: &mut Vec<EvaluationResult>) -> Result<(), JsonPathError> {
        if let Value::Array(arr) = &input.value {
            let len = arr.len() as i64;
            let index = if idx < 0 { len + idx } else { idx };
            
            if index >= 0 && (index as usize) < arr.len() {
                outputs.push(EvaluationResult {
                    value: arr[index as usize].clone(),
                    path: format!("{}[{}]", input.path, idx),
                });
            }
        }
        Ok(())
    }
    
    fn apply_slice_access(&self, input: &EvaluationResult, start: Option<i64>, end: Option<i64>, step: Option<i64>, outputs: &mut Vec<EvaluationResult>) -> Result<(), JsonPathError> {
        if let Value::Array(arr) = &input.value {
            let len = arr.len() as i64;
            let step = step.unwrap_or(1);
            
            if step == 0 {
                return Err(JsonPathError::EvaluationError("Step cannot be zero".to_string()));
            }
            
            let start = start.unwrap_or(if step > 0 { 0 } else { len - 1 });
            let end = end.unwrap_or(if step > 0 { len } else { -1 });
            
            let mut i = start;
            let mut index = 0;
            
            while (step > 0 && i < end && i < len) || (step < 0 && i > end && i >= 0) {
                if i >= 0 && (i as usize) < arr.len() {
                    outputs.push(EvaluationResult {
                        value: arr[i as usize].clone(),
                        path: format!("{}[{}]", input.path, i),
                    });
                }
                i += step;
                index += 1;
                
                // Prevent infinite loops
                if index > len {
                    break;
                }
            }
        }
        Ok(())
    }
    
    fn apply_wildcard(&self, input: &EvaluationResult, outputs: &mut Vec<EvaluationResult>) -> Result<(), JsonPathError> {
        match &input.value {
            Value::Object(obj) => {
                for (key, value) in obj {
                    outputs.push(EvaluationResult {
                        value: value.clone(),
                        path: format!("{}.{}", input.path, key),
                    });
                }
            },
            Value::Array(arr) => {
                for (idx, value) in arr.iter().enumerate() {
                    outputs.push(EvaluationResult {
                        value: value.clone(),
                        path: format!("{}[{}]", input.path, idx),
                    });
                }
            },
            _ => {}
        }
        Ok(())
    }
    
    fn apply_recursive_descent(&self, input: &EvaluationResult, outputs: &mut Vec<EvaluationResult>, context: &mut EvaluationContext) -> Result<(), JsonPathError> {
        if context.depth >= self.config.max_depth {
            return Err(JsonPathError::RecursionLimit);
        }
        
        context.depth += 1;
        
        let mut queue = VecDeque::new();
        queue.push_back(input.clone());
        
        while let Some(current) = queue.pop_front() {
            outputs.push(current.clone());
            
            match &current.value {
                Value::Object(obj) => {
                    for (key, value) in obj {
                        queue.push_back(EvaluationResult {
                            value: value.clone(),
                            path: format!("{}.{}", current.path, key),
                        });
                    }
                },
                Value::Array(arr) => {
                    for (idx, value) in arr.iter().enumerate() {
                        queue.push_back(EvaluationResult {
                            value: value.clone(),
                            path: format!("{}[{}]", current.path, idx),
                        });
                    }
                },
                _ => {}
            }
        }
        
        context.depth -= 1;
        Ok(())
    }
    
    fn apply_filter(&self, input: &EvaluationResult, filter: &FilterExpression, outputs: &mut Vec<EvaluationResult>, context: &mut EvaluationContext) -> Result<(), JsonPathError> {
        match &input.value {
            Value::Array(arr) => {
                for (idx, item) in arr.iter().enumerate() {
                    if self.evaluate_filter(item, filter, context)? {
                        outputs.push(EvaluationResult {
                            value: item.clone(),
                            path: format!("{}[{}]", input.path, idx),
                        });
                    }
                }
            },
            Value::Object(obj) => {
                for (key, value) in obj {
                    if self.evaluate_filter(value, filter, context)? {
                        outputs.push(EvaluationResult {
                            value: value.clone(),
                            path: format!("{}.{}", input.path, key),
                        });
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }
    
    fn evaluate_filter(&self, value: &Value, filter: &FilterExpression, _context: &mut EvaluationContext) -> Result<bool, JsonPathError> {
        match filter {
            FilterExpression::Compare { left, op, right } => {
                let left_val = self.resolve_filter_value(value, left)?;
                let right_val = self.resolve_filter_value(value, right)?;
                Ok(self.compare_values(&left_val, op, &right_val))
            },
            FilterExpression::Logical { left, op, right } => {
                let left_result = self.evaluate_filter(value, left, _context)?;
                match op {
                    LogicalOp::And => {
                        if !left_result {
                            Ok(false)
                        } else {
                            self.evaluate_filter(value, right, _context)
                        }
                    },
                    LogicalOp::Or => {
                        if left_result {
                            Ok(true)
                        } else {
                            self.evaluate_filter(value, right, _context)
                        }
                    },
                    LogicalOp::Not => {
                        // For NOT, we only evaluate left operand
                        Ok(!left_result)
                    }
                }
            },
            FilterExpression::Exists(field) => {
                if let Value::Object(obj) = value {
                    Ok(obj.contains_key(field))
                } else {
                    Ok(false)
                }
            },
            FilterExpression::Regex { field, pattern: _ } => {
                // Simplified regex implementation
                if let Value::Object(obj) = value {
                    if let Some(field_value) = obj.get(field) {
                        if let Value::String(_s) = field_value {
                            // For now, just check if field exists and is string
                            Ok(true)
                        } else {
                            Ok(false)
                        }
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(false)
                }
            }
        }
    }
    
    fn resolve_filter_value(&self, context: &Value, filter_value: &FilterValue) -> Result<Value, JsonPathError> {
        match filter_value {
            FilterValue::Literal(val) => Ok(val.clone()),
            FilterValue::Current => Ok(context.clone()),
            FilterValue::Field(field) => {
                if let Value::Object(obj) = context {
                    Ok(obj.get(field).cloned().unwrap_or(Value::Null))
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }
    
    fn compare_values(&self, left: &Value, op: &CompareOp, right: &Value) -> bool {
        match op {
            CompareOp::Equal => left == right,
            CompareOp::NotEqual => left != right,
            CompareOp::Less => self.numeric_compare(left, right, |a, b| a < b),
            CompareOp::LessEqual => self.numeric_compare(left, right, |a, b| a <= b),
            CompareOp::Greater => self.numeric_compare(left, right, |a, b| a > b),
            CompareOp::GreaterEqual => self.numeric_compare(left, right, |a, b| a >= b),
            CompareOp::In => {
                if let Value::Array(arr) = right {
                    arr.contains(left)
                } else {
                    false
                }
            },
            CompareOp::NotIn => {
                if let Value::Array(arr) = right {
                    !arr.contains(left)
                } else {
                    true
                }
            }
        }
    }
    
    fn numeric_compare<F>(&self, left: &Value, right: &Value, op: F) -> bool 
    where F: Fn(f64, f64) -> bool {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => {
                if let (Some(l), Some(r)) = (l.as_f64(), r.as_f64()) {
                    op(l, r)
                } else {
                    false
                }
            },
            _ => false
        }
    }
}

/// Helper structures for evaluation
#[derive(Debug, Clone)]
struct EvaluationResult {
    value: Value,
    path: String,
}

struct EvaluationContext {
    depth: usize,
}

impl EvaluationContext {
    fn new(_config: &JsonPathConfig) -> Self {
        Self { depth: 0 }
    }
}

/// Cache statistics
#[derive(Debug)]
pub struct CacheStats {
    pub size: usize,
    pub enabled: bool,
}

/// Simple JSONPath parser
struct PathParser {
    input: String,
    position: usize,
}

impl PathParser {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_string(),
            position: 0,
        }
    }
    
    fn parse(&mut self) -> Result<CompiledPath, JsonPathError> {
        let mut segments = Vec::new();
        let is_absolute = self.input.starts_with('$');
        
        if is_absolute {
            self.position = 1; // Skip $
        }
        
        while self.position < self.input.len() {
            if self.current_char() == '.' {
                self.position += 1;
                if self.position < self.input.len() && self.current_char() == '.' {
                    // Recursive descent
                    segments.push(PathSegment::RecursiveDescent);
                    self.position += 1;
                } else {
                    // Regular child access follows
                }
            } else if self.current_char() == '[' {
                segments.push(self.parse_bracket_expression()?);
            } else {
                segments.push(self.parse_identifier()?);
            }
        }
        
        Ok(CompiledPath { segments, is_absolute })
    }
    
    fn parse_bracket_expression(&mut self) -> Result<PathSegment, JsonPathError> {
        self.position += 1; // Skip [
        
        // Handle different bracket expressions
        if self.position >= self.input.len() {
            return Err(JsonPathError::ParseError("Unexpected end of input".to_string()));
        }
        
        let start_pos = self.position;
        let mut bracket_content = String::new();
        let mut bracket_count = 1;
        
        while self.position < self.input.len() && bracket_count > 0 {
            let ch = self.current_char();
            if ch == '[' {
                bracket_count += 1;
            } else if ch == ']' {
                bracket_count -= 1;
            }
            
            if bracket_count > 0 {
                bracket_content.push(ch);
            }
            self.position += 1;
        }
        
        if bracket_count != 0 {
            return Err(JsonPathError::ParseError("Unmatched brackets".to_string()));
        }
        
        self.parse_bracket_content(&bracket_content)
    }
    
    fn parse_bracket_content(&self, content: &str) -> Result<PathSegment, JsonPathError> {
        let content = content.trim();
        
        if content == "*" {
            return Ok(PathSegment::Wildcard);
        }
        
        if content.starts_with('?') {
            // Filter expression - simplified parsing
            return Ok(PathSegment::Filter(FilterExpression::Exists("dummy".to_string())));
        }
        
        if content.contains(':') {
            // Slice expression
            let parts: Vec<&str> = content.split(':').collect();
            let start = if parts[0].is_empty() { None } else { parts[0].parse().ok() };
            let end = if parts.len() > 1 && !parts[1].is_empty() { parts[1].parse().ok() } else { None };
            let step = if parts.len() > 2 && !parts[2].is_empty() { parts[2].parse().ok() } else { None };
            
            return Ok(PathSegment::Slice { start, end, step });
        }
        
        if content.contains(',') {
            // Union expression - simplified
            let parts: Vec<&str> = content.split(',').collect();
            let mut segments = Vec::new();
            for part in parts {
                let part = part.trim();
                if let Ok(index) = part.parse::<i64>() {
                    segments.push(PathSegment::Index(index));
                } else {
                    segments.push(PathSegment::Child(part.to_string()));
                }
            }
            return Ok(PathSegment::Union(segments));
        }
        
        // Try to parse as index
        if let Ok(index) = content.parse::<i64>() {
            return Ok(PathSegment::Index(index));
        }
        
        // Otherwise treat as child key
        Ok(PathSegment::Child(content.to_string()))
    }
    
    fn parse_identifier(&mut self) -> Result<PathSegment, JsonPathError> {
        let start_pos = self.position;
        
        while self.position < self.input.len() {
            let ch = self.current_char();
            if ch == '.' || ch == '[' {
                break;
            }
            self.position += 1;
        }
        
        let identifier = &self.input[start_pos..self.position];
        
        if identifier == "*" {
            Ok(PathSegment::Wildcard)
        } else if identifier.is_empty() {
            Err(JsonPathError::ParseError("Empty identifier".to_string()))
        } else {
            Ok(PathSegment::Child(identifier.to_string()))
        }
    }
    
    fn current_char(&self) -> char {
        self.input.chars().nth(self.position).unwrap_or('\0')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_basic_jsonpath_queries() {
        let mut engine = JsonPathEngine::new();
        let data = json!({
            "store": {
                "book": [
                    {"title": "Book 1", "price": 10.0},
                    {"title": "Book 2", "price": 15.0}
                ]
            }
        });
        
        // Root query
        let result = engine.query(&data, "$").unwrap();
        assert_eq!(result.values.len(), 1);
        
        // Child access
        let result = engine.query(&data, "$.store").unwrap();
        assert_eq!(result.values.len(), 1);
        
        // Array access
        let result = engine.query(&data, "$.store.book[0]").unwrap();
        assert_eq!(result.values.len(), 1);
        if let Value::Object(obj) = &result.values[0] {
            assert_eq!(obj.get("title").unwrap(), &Value::String("Book 1".to_string()));
        }
    }
    
    #[test]
    fn test_wildcard_and_slice() {
        let mut engine = JsonPathEngine::new();
        let data = json!({
            "items": [1, 2, 3, 4, 5]
        });
        
        // Wildcard
        let result = engine.query(&data, "$.items[*]").unwrap();
        assert_eq!(result.values.len(), 5);
        
        // Slice
        let result = engine.query(&data, "$.items[1:3]").unwrap();
        assert_eq!(result.values.len(), 2);
        assert_eq!(result.values[0], json!(2));
        assert_eq!(result.values[1], json!(3));
    }
    
    #[test]
    fn test_compilation_cache() {
        let mut engine = JsonPathEngine::new();
        let data = json!({"test": "value"});
        
        // First query - should compile
        let result1 = engine.query(&data, "$.test").unwrap();
        assert!(!result1.cache_hit);
        
        // Second query - should use cache
        let result2 = engine.query(&data, "$.test").unwrap();
        assert!(result2.cache_hit);
        
        let stats = engine.cache_stats();
        assert_eq!(stats.size, 1);
    }
    
    #[test]
    fn test_multiple_queries() {
        let mut engine = JsonPathEngine::new();
        let data = json!({
            "a": 1,
            "b": 2,
            "c": 3
        });
        
        let paths = ["$.a", "$.b", "$.c"];
        let results = engine.query_multiple(&data, &paths).unwrap();
        
        assert_eq!(results.len(), 3);
        assert!(results.contains_key("$.a"));
        assert!(results.contains_key("$.b"));
        assert!(results.contains_key("$.c"));
    }
}