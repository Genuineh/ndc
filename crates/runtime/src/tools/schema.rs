//! Tool Schema - JSON Schema 定义用于 LLM 参数理解
//!
//! 设计参考 OpenCode/Zod 的 Schema 系统:
//! - 每个工具都有清晰的 JSON Schema
//! - Schema 用于 LLM 参数验证和理解
//! - 支持参数描述和类型约束

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// JSON Schema 类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonSchemaType {
    /// String 类型
    String,
    /// Number 类型
    Number,
    /// Integer 类型
    Integer,
    /// Boolean 类型
    Boolean,
    /// Object 类型
    Object,
    /// Array 类型
    Array,
    /// Null 类型
    Null,
}

/// JSON Schema 属性
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchemaProperty {
    /// 类型
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<JsonSchemaType>,

    /// 描述（LLM 理解的关键）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 是否必需
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// 默认值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// 枚举值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_: Option<Vec<serde_json::Value>>,

    /// 最小值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// 最大值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    /// 最小长度
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,

    /// 最大长度
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,

    /// 模式（正则表达式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// 数组 items
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    /// 对象 properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, JsonSchema>>,

    /// 额外属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,
}

impl JsonSchemaProperty {
    /// 创建字符串类型属性
    pub fn string(desc: impl Into<String>) -> Self {
        Self {
            type_: Some(JsonSchemaType::String),
            description: Some(desc.into()),
            required: None,
            default: None,
            enum_: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            items: None,
            properties: None,
            additional_properties: None,
        }
    }

    /// 创建数字类型属性
    pub fn number(desc: impl Into<String>) -> Self {
        Self {
            type_: Some(JsonSchemaType::Number),
            description: Some(desc.into()),
            required: None,
            default: None,
            enum_: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            items: None,
            properties: None,
            additional_properties: None,
        }
    }

    /// 创建整数类型属性
    pub fn integer(desc: impl Into<String>) -> Self {
        Self {
            type_: Some(JsonSchemaType::Integer),
            description: Some(desc.into()),
            required: None,
            default: None,
            enum_: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            items: None,
            properties: None,
            additional_properties: None,
        }
    }

    /// 创建布尔类型属性
    pub fn boolean(desc: impl Into<String>) -> Self {
        Self {
            type_: Some(JsonSchemaType::Boolean),
            description: Some(desc.into()),
            required: None,
            default: None,
            enum_: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            items: None,
            properties: None,
            additional_properties: None,
        }
    }

    /// 创建数组类型属性
    pub fn array(desc: impl Into<String>, items: JsonSchema) -> Self {
        Self {
            type_: Some(JsonSchemaType::Array),
            description: Some(desc.into()),
            required: None,
            default: None,
            enum_: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            items: Some(Box::new(items)),
            properties: None,
            additional_properties: None,
        }
    }

    /// 创建对象类型属性
    pub fn object(desc: impl Into<String>, properties: HashMap<String, JsonSchema>) -> Self {
        Self {
            type_: Some(JsonSchemaType::Object),
            description: Some(desc.into()),
            required: None,
            default: None,
            enum_: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            items: None,
            properties: Some(properties),
            additional_properties: None,
        }
    }

    /// 设置必需
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// 设置默认值
    pub fn default(mut self, default: serde_json::Value) -> Self {
        self.default = Some(default);
        self
    }

    /// 设置枚举值
    pub fn enum_values(mut self, values: Vec<serde_json::Value>) -> Self {
        self.enum_ = Some(values);
        self
    }

    /// 设置范围
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.minimum = Some(min);
        self.maximum = Some(max);
        self
    }

    /// 设置长度范围
    pub fn length_range(mut self, min: usize, max: usize) -> Self {
        self.min_length = Some(min);
        self.max_length = Some(max);
        self
    }

    /// 设置正则表达式模式
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }

    /// 转换为 JSON Schema Value
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({}))
    }
}

/// JSON Schema 完整定义
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonSchema {
    /// Schema 类型
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<JsonSchemaType>,

    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, JsonSchemaProperty>>,

    /// 必需字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    /// 数组 items
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    /// 额外属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,

    /// 描述转换为别名
    #[serde(rename = "$description", skip_serializing_if = "Option::is_none")]
    pub description_alias: Option<String>,
}

impl JsonSchema {
    /// 创建空对象 Schema
    pub fn object() -> Self {
        Self {
            type_: Some(JsonSchemaType::Object),
            description: None,
            properties: None,
            required: None,
            items: None,
            additional_properties: None,
            description_alias: None,
        }
    }

    /// 创建带描述的对象 Schema
    pub fn with_description(desc: impl Into<String>) -> Self {
        Self {
            type_: Some(JsonSchemaType::Object),
            description: Some(desc.into()),
            properties: None,
            required: None,
            items: None,
            additional_properties: None,
            description_alias: None,
        }
    }

    /// 添加属性
    pub fn property(mut self, name: impl Into<String>, prop: JsonSchemaProperty) -> Self {
        if self.properties.is_none() {
            self.properties = Some(HashMap::new());
        }
        self.properties.as_mut().unwrap().insert(name.into(), prop);
        self
    }

    /// 添加必需字段
    pub fn required_field(mut self, name: impl Into<String>) -> Self {
        if self.required.is_none() {
            self.required = Some(Vec::new());
        }
        self.required.as_mut().unwrap().push(name.into());
        self
    }

    /// 添加多个必需字段
    pub fn required_fields(mut self, names: Vec<impl Into<String>>) -> Self {
        if self.required.is_none() {
            self.required = Some(Vec::new());
        }
        for name in names {
            self.required.as_mut().unwrap().push(name.into());
        }
        self
    }

    /// 转换为 JSON Value
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({}))
    }
}

/// Tool Schema 构建器 - 简化 Schema 创建
#[derive(Debug, Default)]
pub struct ToolSchemaBuilder {
    schema: JsonSchema,
}

impl ToolSchemaBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置描述
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.schema.description = Some(desc.into());
        self
    }

    /// 添加字符串参数
    pub fn param_string(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        let name = name.into();
        let prop = JsonSchemaProperty::string(description);
        if self.schema.properties.is_none() {
            self.schema.properties = Some(HashMap::new());
        }
        self.schema
            .properties
            .as_mut()
            .unwrap()
            .insert(name.clone(), prop);
        self
    }

    /// 添加必需字符串参数
    pub fn required_string(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let prop = JsonSchemaProperty::string(description).required(true);
        if self.schema.properties.is_none() {
            self.schema.properties = Some(HashMap::new());
        }
        self.schema
            .properties
            .as_mut()
            .unwrap()
            .insert(name.clone(), prop);
        if self.schema.required.is_none() {
            self.schema.required = Some(Vec::new());
        }
        self.schema.required.as_mut().unwrap().push(name);
        self
    }

    /// 添加整数参数
    pub fn param_integer(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let prop = JsonSchemaProperty::integer(description);
        if self.schema.properties.is_none() {
            self.schema.properties = Some(HashMap::new());
        }
        self.schema
            .properties
            .as_mut()
            .unwrap()
            .insert(name.clone(), prop);
        self
    }

    /// 添加必需整数参数
    pub fn required_integer(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let prop = JsonSchemaProperty::integer(description).required(true);
        if self.schema.properties.is_none() {
            self.schema.properties = Some(HashMap::new());
        }
        self.schema
            .properties
            .as_mut()
            .unwrap()
            .insert(name.clone(), prop);
        if self.schema.required.is_none() {
            self.schema.required = Some(Vec::new());
        }
        self.schema.required.as_mut().unwrap().push(name);
        self
    }

    /// 添加布尔参数
    pub fn param_boolean(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let prop = JsonSchemaProperty::boolean(description);
        if self.schema.properties.is_none() {
            self.schema.properties = Some(HashMap::new());
        }
        self.schema
            .properties
            .as_mut()
            .unwrap()
            .insert(name.clone(), prop);
        self
    }

    /// 添加数组参数
    pub fn param_array(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        items: JsonSchema,
    ) -> Self {
        let name = name.into();
        let prop = JsonSchemaProperty::array(description, items);
        if self.schema.properties.is_none() {
            self.schema.properties = Some(HashMap::new());
        }
        self.schema
            .properties
            .as_mut()
            .unwrap()
            .insert(name.clone(), prop);
        self
    }

    /// 构建 Schema
    pub fn build(self) -> JsonSchema {
        self.schema
    }
}

/// 参数验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 是否有效
    pub valid: bool,
    /// 错误信息
    pub errors: Vec<String>,
}

impl ValidationResult {
    /// 创建有效结果
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    /// 创建无效结果
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
        }
    }

    /// 添加错误
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.valid = false;
    }
}

/// Schema 验证器
pub struct SchemaValidator;

impl SchemaValidator {
    /// 验证参数是否符合 Schema
    pub fn validate(params: &serde_json::Value, schema: &JsonSchema) -> ValidationResult {
        let mut result = ValidationResult::valid();

        // 必须是对象
        if !params.is_object() {
            result.add_error("Parameters must be an object");
            return result;
        }

        let params_obj = params.as_object().unwrap();

        // 检查必需字段
        if let Some(required) = &schema.required {
            for field in required {
                if !params_obj.contains_key(field) {
                    result.add_error(format!("Missing required field: {}", field));
                }
            }
        }

        // 检查每个属性
        if let Some(properties) = &schema.properties {
            for (name, prop) in properties {
                if let Some(value) = params_obj.get(name) {
                    // 验证类型
                    if let Some(type_) = &prop.type_
                        && !Self::check_type(value, type_)
                    {
                        result.add_error(format!(
                            "Field '{}' has wrong type, expected {:?}",
                            name, type_
                        ));
                    }

                    // 验证枚举
                    if let Some(enum_values) = &prop.enum_ {
                        let enum_values: Vec<_> = enum_values.iter().collect();
                        if !enum_values.contains(&value) {
                            result.add_error(format!(
                                "Field '{}' must be one of: {:?}",
                                name,
                                enum_values
                                    .iter()
                                    .map(|v| v.to_string())
                                    .collect::<Vec<_>>()
                            ));
                        }
                    }

                    // 验证范围
                    if let Some(min) = prop.minimum
                        && let Some(num) = value.as_f64()
                        && num < min
                    {
                        result.add_error(format!("Field '{}' must be >= {}", name, min));
                    }

                    if let Some(max) = prop.maximum
                        && let Some(num) = value.as_f64()
                        && num > max
                    {
                        result.add_error(format!("Field '{}' must be <= {}", name, max));
                    }

                    // 验证字符串长度
                    if let Some(min_len) = prop.min_length
                        && let Some(s) = value.as_str()
                        && s.len() < min_len
                    {
                        result
                            .add_error(format!("Field '{}' must have length >= {}", name, min_len));
                    }

                    if let Some(max_len) = prop.max_length
                        && let Some(s) = value.as_str()
                        && s.len() > max_len
                    {
                        result
                            .add_error(format!("Field '{}' must have length <= {}", name, max_len));
                    }

                    // 验证正则表达式
                    if let Some(pattern) = &prop.pattern
                        && let Some(s) = value.as_str()
                        && let Ok(regex) = regex::Regex::new(pattern)
                        && !regex.is_match(s)
                    {
                        result.add_error(format!(
                            "Field '{}' does not match pattern: {}",
                            name, pattern
                        ));
                    }
                }
            }
        }

        result
    }

    /// 检查值类型
    fn check_type(value: &serde_json::Value, expected: &JsonSchemaType) -> bool {
        match expected {
            JsonSchemaType::String => value.is_string(),
            JsonSchemaType::Number => value.is_number(),
            JsonSchemaType::Integer => value.is_i64() || value.is_u64(),
            JsonSchemaType::Boolean => value.is_boolean(),
            JsonSchemaType::Object => value.is_object(),
            JsonSchemaType::Array => value.is_array(),
            JsonSchemaType::Null => value.is_null(),
        }
    }
}

/// 生成 LLM 友好的工具描述
pub fn generate_tool_description(name: &str, description: &str, schema: &JsonSchema) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("Tool: {}", name));
    lines.push(String::new());
    lines.push(description.to_string());
    lines.push(String::new());
    lines.push("Parameters:".to_string());

    if let Some(properties) = &schema.properties {
        for (prop_name, prop) in properties {
            let required = schema
                .required
                .as_ref()
                .map(|r| r.contains(prop_name))
                .unwrap_or(false);

            let required_str = if required { " (required)" } else { "" };
            let type_str = prop
                .type_
                .as_ref()
                .map(|t| format!("{:?}", t).to_lowercase())
                .unwrap_or_else(|| "any".to_string());

            lines.push(format!("  - {}{}: {}", prop_name, required_str, type_str));

            if let Some(desc) = &prop.description {
                lines.push(format!("    {}", desc));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_object() {
        let schema = JsonSchema::object();
        let value = schema.to_value();
        // Check that type field exists (serialized as "type")
        let type_val = value.get("type").or(value.get("type_"));
        assert!(type_val.is_some(), "type field should exist");
    }

    #[test]
    fn test_schema_with_property() {
        let schema = ToolSchemaBuilder::new()
            .description("Test schema")
            .param_string("name", "The name")
            .required_integer("age", "The age")
            .build();

        let value = schema.to_value();
        assert_eq!(value["description"], "Test schema");
        assert!(value["properties"].is_object());
        assert!(value["properties"]["name"].is_object());
        assert!(value["properties"]["age"].is_object());
    }

    #[test]
    fn test_tool_schema_builder() {
        let schema = ToolSchemaBuilder::new()
            .description("Test tool")
            .required_string("filePath", "The file path to read")
            .param_integer("offset", "Line number to start from")
            .build();

        let value = schema.to_value();
        assert_eq!(value["description"], "Test tool");
        assert!(value["required"].is_array());
        assert!(
            value["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("filePath"))
        );
    }

    #[test]
    fn test_validation_valid() {
        let schema = ToolSchemaBuilder::new()
            .required_string("name", "The name")
            .required_integer("age", "The age")
            .build();

        let params = serde_json::json!({
            "name": "John",
            "age": 30
        });

        let result = SchemaValidator::validate(&params, &schema);
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validation_missing_required() {
        let schema = ToolSchemaBuilder::new()
            .required_string("name", "The name")
            .required_integer("age", "The age")
            .build();

        let params = serde_json::json!({
            "name": "John"
        });

        let result = SchemaValidator::validate(&params, &schema);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("age")));
    }

    #[test]
    fn test_validation_wrong_type() {
        let schema = ToolSchemaBuilder::new()
            .required_string("name", "The name")
            .required_integer("age", "The age")
            .build();

        let params = serde_json::json!({
            "name": "John",
            "age": "thirty"
        });

        let result = SchemaValidator::validate(&params, &schema);
        assert!(!result.valid);
    }

    #[test]
    fn test_validation_range() {
        // Test with JsonSchema directly for range validation
        let mut schema = JsonSchema::object();
        schema = schema.property(
            "age",
            JsonSchemaProperty::integer("The age").range(0.0, 150.0),
        );

        let params_valid = serde_json::json!({ "age": 30 });
        let result_valid = SchemaValidator::validate(&params_valid, &schema);
        assert!(result_valid.valid);

        let params_invalid = serde_json::json!({ "age": 200 });
        let result_invalid = SchemaValidator::validate(&params_invalid, &schema);
        assert!(!result_invalid.valid);
    }

    #[test]
    fn test_generate_tool_description() {
        let schema = ToolSchemaBuilder::new()
            .required_string("filePath", "The file path to read")
            .param_integer("offset", "Line number to start from")
            .build();

        let desc = generate_tool_description("read_file", "Read the contents of a file", &schema);

        assert!(desc.contains("read_file"));
        assert!(desc.contains("filePath (required)"));
        assert!(desc.contains("offset"));
        // offset is optional, so check it doesn't appear with (required)
        let lines_with_offset: Vec<&str> = desc.lines().filter(|l| l.contains("offset")).collect();
        assert!(
            !lines_with_offset.iter().any(|l| l.contains("(required)")),
            "offset should not be marked as required"
        );
    }

    #[test]
    fn test_property_string() {
        let prop = JsonSchemaProperty::string("A name")
            .required(true)
            .default(serde_json::json!("default"));
        let value = prop.to_value();
        // Check that type field exists
        let type_val = value.get("type").or(value.get("type_"));
        assert!(type_val.is_some(), "type field should exist");
        assert_eq!(value["description"], "A name");
        assert!(value["required"].as_bool().unwrap());
        assert_eq!(value["default"], "default");
    }

    #[test]
    fn test_property_enum() {
        let prop = JsonSchemaProperty::string("A status").enum_values(vec![
            serde_json::json!("active"),
            serde_json::json!("inactive"),
        ]);
        let value = prop.to_value();
        // Check enum field exists
        let enum_val = value.get("enum").or(value.get("enum_"));
        assert!(enum_val.is_some(), "enum field should exist");
        assert!(enum_val.unwrap().is_array());
    }
}
